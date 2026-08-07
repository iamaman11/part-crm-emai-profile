use application_ports::{MailboxObservation, MailboxProviderPort};
use mailbox_domain::{
    MailboxBinding, MailboxBindingStatus, MailboxJob, MailboxProvider, validate_cursor,
    validate_provider_status,
};
use std::fmt;

const MAX_BOUNDED_ITEM_COUNT: u32 = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxProviderFailureKind {
    Retryable,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxProviderAdapterError {
    BindingRevoked,
    BindingMismatch,
    ProviderMismatch,
    InvalidObservation,
    RetryableFailure,
    TerminalFailure,
}

impl MailboxProviderAdapterError {
    #[must_use]
    pub const fn failure_kind(self) -> Option<MailboxProviderFailureKind> {
        match self {
            Self::RetryableFailure => Some(MailboxProviderFailureKind::Retryable),
            Self::TerminalFailure => Some(MailboxProviderFailureKind::Terminal),
            Self::BindingRevoked
            | Self::BindingMismatch
            | Self::ProviderMismatch
            | Self::InvalidObservation => None,
        }
    }
}

impl fmt::Display for MailboxProviderAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BindingRevoked => "mailbox binding is revoked",
            Self::BindingMismatch => "mailbox job does not belong to binding",
            Self::ProviderMismatch => "mailbox provider does not match binding",
            Self::InvalidObservation => "mailbox provider observation is invalid",
            Self::RetryableFailure => "mailbox provider retryable failure",
            Self::TerminalFailure => "mailbox provider terminal failure",
        })
    }
}

impl std::error::Error for MailboxProviderAdapterError {}

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
    ) -> Result<Self, MailboxProviderAdapterError> {
        let provider_status = provider_status.into();
        validate_observation(&provider_status, bounded_item_count, next_cursor.as_deref())?;
        Ok(Self {
            provider,
            provider_status,
            bounded_item_count,
            next_cursor,
        })
    }
}

impl MailboxProviderPort for MetadataMailboxProviderAdapter {
    type Error = MailboxProviderAdapterError;

    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Result<MailboxObservation, Self::Error> {
        validate_binding_job(binding, job)?;
        if binding.provider() != self.provider {
            return Err(MailboxProviderAdapterError::ProviderMismatch);
        }
        Ok(MailboxObservation::new(
            binding.binding_id().clone(),
            self.provider_status.clone(),
            self.bounded_item_count,
            self.next_cursor.clone(),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeterministicMailboxOutcome {
    Success {
        provider_status: String,
        bounded_item_count: u32,
        next_cursor: Option<String>,
    },
    RetryableFailure,
    TerminalFailure,
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
}

impl MailboxProviderPort for DeterministicFakeMailboxProvider {
    type Error = MailboxProviderAdapterError;

    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Result<MailboxObservation, Self::Error> {
        validate_binding_job(binding, job)?;
        self.calls = self.calls.saturating_add(1);
        match &self.outcome {
            DeterministicMailboxOutcome::Success {
                provider_status,
                bounded_item_count,
                next_cursor,
            } => {
                validate_observation(provider_status, *bounded_item_count, next_cursor.as_deref())?;
                Ok(MailboxObservation::new(
                    binding.binding_id().clone(),
                    provider_status.clone(),
                    *bounded_item_count,
                    next_cursor.clone(),
                ))
            }
            DeterministicMailboxOutcome::RetryableFailure => {
                Err(MailboxProviderAdapterError::RetryableFailure)
            }
            DeterministicMailboxOutcome::TerminalFailure => {
                Err(MailboxProviderAdapterError::TerminalFailure)
            }
        }
    }
}

fn validate_binding_job(
    binding: &MailboxBinding,
    job: &MailboxJob,
) -> Result<(), MailboxProviderAdapterError> {
    if binding.status() != MailboxBindingStatus::Active {
        return Err(MailboxProviderAdapterError::BindingRevoked);
    }
    if job.tenant_id() != binding.tenant_id() || job.binding_id() != binding.binding_id() {
        return Err(MailboxProviderAdapterError::BindingMismatch);
    }
    Ok(())
}

fn validate_observation(
    provider_status: &str,
    bounded_item_count: u32,
    next_cursor: Option<&str>,
) -> Result<(), MailboxProviderAdapterError> {
    if bounded_item_count > MAX_BOUNDED_ITEM_COUNT
        || validate_provider_status(provider_status).is_err()
        || validate_cursor(next_cursor).is_err()
    {
        return Err(MailboxProviderAdapterError::InvalidObservation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DeterministicFakeMailboxProvider, DeterministicMailboxOutcome,
        MailboxProviderAdapterError, MetadataMailboxProviderAdapter,
    };
    use application_ports::MailboxProviderPort;
    use mailbox_domain::{MailboxBinding, MailboxJob, MailboxProvider};
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

    fn job(binding: &MailboxBinding) -> Result<MailboxJob, Box<dyn std::error::Error>> {
        Ok(MailboxJob::create(
            binding,
            MailboxJobId::parse("mailjob_01JMAILADAPTER")?,
            None,
            UnixMillis::new(1),
            3,
        )?)
    }

    #[test]
    fn metadata_adapter_returns_only_bounded_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding)?;
        let mut adapter = MetadataMailboxProviderAdapter::new(
            MailboxProvider::Imap,
            "SYNTHETIC_OK",
            4,
            Some("cursor-next".to_owned()),
        )?;
        let observation = adapter.check_mailbox(&binding, &job)?;
        assert_eq!(observation.provider_status(), "SYNTHETIC_OK");
        assert_eq!(observation.bounded_item_count(), 4);
        assert_eq!(observation.next_cursor(), Some("cursor-next"));
        Ok(())
    }

    #[test]
    fn deterministic_fake_classifies_retryable_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding)?;
        let mut adapter = DeterministicFakeMailboxProvider::new(
            DeterministicMailboxOutcome::RetryableFailure,
        );
        assert_eq!(
            adapter.check_mailbox(&binding, &job),
            Err(MailboxProviderAdapterError::RetryableFailure)
        );
        assert_eq!(adapter.calls(), 1);
        Ok(())
    }
}
