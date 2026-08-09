use application_ports::mailboxes::{
    MailboxObservation, MailboxProviderPort, MailboxProviderPortError,
};
use mailbox_domain::{
    MailboxBinding, MailboxJob, MailboxJobStatus, MailboxProvider, MailboxProviderFailure,
    validate_bounded_item_count, validate_cursor, validate_provider_status,
};
use std::future::{Ready, ready};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataMailboxProviderAdapter {
    provider: MailboxProvider,
    provider_status: String,
    bounded_item_count: u32,
    next_cursor: Option<String>,
}

impl MetadataMailboxProviderAdapter {
    pub fn new(
        provider: MailboxProvider,
        provider_status: impl Into<String>,
        bounded_item_count: u32,
        next_cursor: Option<String>,
    ) -> Result<Self, mailbox_domain::MailboxError> {
        let provider_status = provider_status.into();
        validate_provider_status(&provider_status)?;
        validate_bounded_item_count(bounded_item_count)?;
        validate_cursor(next_cursor.as_deref())?;
        Ok(Self {
            provider,
            provider_status,
            bounded_item_count,
            next_cursor,
        })
    }

    fn check_now(
        &self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Result<MailboxObservation, MailboxProviderPortError> {
        validate_binding_job(binding, job)?;
        if binding.provider() != self.provider {
            return Err(MailboxProviderPortError::IntegrityFailure);
        }
        MailboxObservation::new(
            binding.binding_id().clone(),
            self.provider_status.clone(),
            self.bounded_item_count,
            self.next_cursor.clone(),
        )
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)
    }
}

impl MailboxProviderPort for MetadataMailboxProviderAdapter {
    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Ready<Result<MailboxObservation, MailboxProviderPortError>> {
        ready(self.check_now(binding, job))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeterministicMailboxOutcome {
    Success {
        provider_status: String,
        bounded_item_count: u32,
        next_cursor: Option<String>,
    },
    Failure(MailboxProviderFailure),
}

#[derive(Clone, Debug)]
pub struct DeterministicFakeMailboxProvider {
    outcome: DeterministicMailboxOutcome,
    calls: u32,
}

impl DeterministicFakeMailboxProvider {
    #[must_use]
    pub const fn new(outcome: DeterministicMailboxOutcome) -> Self {
        Self { outcome, calls: 0 }
    }

    #[must_use]
    pub const fn calls(&self) -> u32 {
        self.calls
    }

    fn check_now(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Result<MailboxObservation, MailboxProviderPortError> {
        validate_binding_job(binding, job)?;
        self.calls = self
            .calls
            .checked_add(1)
            .ok_or(MailboxProviderPortError::IntegrityFailure)?;
        match &self.outcome {
            DeterministicMailboxOutcome::Success {
                provider_status,
                bounded_item_count,
                next_cursor,
            } => MailboxObservation::new(
                binding.binding_id().clone(),
                provider_status.clone(),
                *bounded_item_count,
                next_cursor.clone(),
            )
            .map_err(|_| MailboxProviderPortError::IntegrityFailure),
            DeterministicMailboxOutcome::Failure(failure) => {
                Err(MailboxProviderPortError::Failure(*failure))
            }
        }
    }
}

impl MailboxProviderPort for DeterministicFakeMailboxProvider {
    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Ready<Result<MailboxObservation, MailboxProviderPortError>> {
        ready(self.check_now(binding, job))
    }
}

fn validate_binding_job(
    binding: &MailboxBinding,
    job: &MailboxJob,
) -> Result<(), MailboxProviderPortError> {
    if !binding.is_executable()
        || job.tenant_id() != binding.tenant_id()
        || job.binding_id() != binding.binding_id()
        || job.status() != MailboxJobStatus::Running
    {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DeterministicFakeMailboxProvider, DeterministicMailboxOutcome,
        MetadataMailboxProviderAdapter,
    };
    use application_ports::mailboxes::MailboxProviderPortError;
    use mailbox_domain::{
        MailboxBinding, MailboxJob, MailboxProvider, MailboxProviderFailure,
        MailboxProviderFailureClass,
    };
    use profile_platform_primitives::{
        MailboxBindingId, MailboxJobId, SecretHandle, TenantId, UnixMillis,
    };

    fn binding() -> Result<MailboxBinding, Box<dyn std::error::Error>> {
        Ok(MailboxBinding::create(
            TenantId::parse("tenant_01JMAILADAPTER")?,
            MailboxBindingId::parse("mailbox_01JMAILADAPTER")?,
            MailboxProvider::Imap,
            SecretHandle::parse("secret_01JMAILADAPTER")?,
        ))
    }

    fn running_job(binding: &MailboxBinding) -> Result<MailboxJob, Box<dyn std::error::Error>> {
        let mut job = MailboxJob::create(
            binding,
            MailboxJobId::parse("mailjob_01JMAILADAPTER")?,
            None,
            UnixMillis::new(1),
            3,
        )?;
        job.queue(UnixMillis::new(1))?;
        job.start(binding)?;
        Ok(job)
    }

    #[test]
    fn metadata_adapter_returns_only_bounded_observation() -> Result<(), Box<dyn std::error::Error>>
    {
        let binding = binding()?;
        let job = running_job(&binding)?;
        let adapter = MetadataMailboxProviderAdapter::new(
            MailboxProvider::Imap,
            "SYNTHETIC_OK",
            4,
            Some("cursor-next".to_owned()),
        )?;
        let observation = adapter.check_now(&binding, &job)?;
        assert_eq!(observation.provider_status(), "SYNTHETIC_OK");
        assert_eq!(observation.bounded_item_count(), 4);
        assert_eq!(observation.next_cursor(), Some("cursor-next"));
        Ok(())
    }

    #[test]
    fn fake_provider_translates_failure_without_deciding_job_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = running_job(&binding)?;
        let failure = MailboxProviderFailure::new(MailboxProviderFailureClass::RateLimited, None)?;
        let mut adapter = DeterministicFakeMailboxProvider::new(
            DeterministicMailboxOutcome::Failure(failure),
        );
        assert_eq!(
            adapter.check_now(&binding, &job),
            Err(MailboxProviderPortError::Failure(failure))
        );
        assert_eq!(adapter.calls(), 1);
        Ok(())
    }

    #[test]
    fn wrong_provider_selection_fails_as_integrity_not_business_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = running_job(&binding)?;
        let adapter =
            MetadataMailboxProviderAdapter::new(MailboxProvider::GmailApi, "OK", 0, None)?;
        assert_eq!(
            adapter.check_now(&binding, &job),
            Err(MailboxProviderPortError::IntegrityFailure)
        );
        Ok(())
    }

    #[test]
    fn counter_overflow_fails_closed_as_integrity() -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = running_job(&binding)?;
        let failure = MailboxProviderFailure::new(MailboxProviderFailureClass::Permanent, None)?;
        let mut adapter = DeterministicFakeMailboxProvider {
            outcome: DeterministicMailboxOutcome::Failure(failure),
            calls: u32::MAX,
        };
        assert_eq!(
            adapter.check_now(&binding, &job),
            Err(MailboxProviderPortError::IntegrityFailure)
        );
        assert_eq!(adapter.calls(), u32::MAX);
        Ok(())
    }
}
