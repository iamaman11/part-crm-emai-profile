use application_ports::{MailboxObservation, MailboxProviderPort};
use mailbox_domain::{
    MailboxBinding, MailboxBindingStatus, MailboxJob, MailboxJobStatus, MailboxProvider,
    validate_cursor, validate_provider_status,
};
use profile_platform_primitives::{AggregateVersion, UnixMillis};
use std::fmt;

const MAX_BOUNDED_ITEM_COUNT: u32 = 10_000;
const RETRY_DELAY_MS: u64 = 60_000;

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
    SchedulingOverflow,
    InvalidJobState,
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
            | Self::InvalidObservation
            | Self::SchedulingOverflow
            | Self::InvalidJobState => None,
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
            Self::SchedulingOverflow => "mailbox retry schedule overflow",
            Self::InvalidJobState => "mailbox job state is invalid for provider execution",
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxRunDecision {
    status: MailboxJobStatus,
    attempt: u32,
    version: AggregateVersion,
    cursor: Option<String>,
    provider_status: String,
    bounded_item_count: u32,
    retry_at: Option<UnixMillis>,
}

impl MailboxRunDecision {
    #[must_use]
    pub const fn status(&self) -> MailboxJobStatus {
        self.status
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    #[must_use]
    pub fn provider_status(&self) -> &str {
        &self.provider_status
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }

    #[must_use]
    pub const fn retry_at(&self) -> Option<UnixMillis> {
        self.retry_at
    }
}

pub fn decide_mailbox_run<P>(
    binding: &MailboxBinding,
    job: &MailboxJob,
    now: UnixMillis,
    provider: &mut P,
) -> Result<MailboxRunDecision, MailboxProviderAdapterError>
where
    P: MailboxProviderPort<Error = MailboxProviderAdapterError>,
{
    validate_binding_job(binding, job)?;
    let mut next = job.clone();
    next.start(now)
        .map_err(|_| MailboxProviderAdapterError::InvalidJobState)?;

    match provider.check_mailbox(binding, &next) {
        Ok(observation) => {
            validate_observation(
                observation.provider_status(),
                observation.bounded_item_count(),
                observation.next_cursor(),
            )?;
            next.succeed(observation.next_cursor().map(str::to_owned))
                .map_err(|_| MailboxProviderAdapterError::InvalidJobState)?;
            Ok(decision_from_job(
                &next,
                observation.provider_status().to_owned(),
                observation.bounded_item_count(),
                None,
            ))
        }
        Err(MailboxProviderAdapterError::RetryableFailure) => {
            if next.attempt() >= next.max_attempts() {
                next.fail()
                    .map_err(|_| MailboxProviderAdapterError::InvalidJobState)?;
                return Ok(decision_from_job(
                    &next,
                    "RETRY_EXHAUSTED".to_owned(),
                    0,
                    None,
                ));
            }
            let retry_at = UnixMillis::new(
                now.value()
                    .checked_add(RETRY_DELAY_MS)
                    .ok_or(MailboxProviderAdapterError::SchedulingOverflow)?,
            );
            next.retry(now, retry_at)
                .map_err(|_| MailboxProviderAdapterError::InvalidJobState)?;
            Ok(decision_from_job(
                &next,
                "RETRYABLE_FAILURE".to_owned(),
                0,
                Some(retry_at),
            ))
        }
        Err(MailboxProviderAdapterError::TerminalFailure) => {
            next.fail()
                .map_err(|_| MailboxProviderAdapterError::InvalidJobState)?;
            Ok(decision_from_job(
                &next,
                "TERMINAL_FAILURE".to_owned(),
                0,
                None,
            ))
        }
        Err(error) => Err(error),
    }
}

fn decision_from_job(
    job: &MailboxJob,
    provider_status: String,
    bounded_item_count: u32,
    retry_at: Option<UnixMillis>,
) -> MailboxRunDecision {
    MailboxRunDecision {
        status: job.status(),
        attempt: job.attempt(),
        version: job.version(),
        cursor: job.cursor().map(str::to_owned),
        provider_status,
        bounded_item_count,
        retry_at,
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
        decide_mailbox_run, DeterministicFakeMailboxProvider, DeterministicMailboxOutcome,
        MailboxProviderAdapterError, MetadataMailboxProviderAdapter,
    };
    use application_ports::MailboxProviderPort;
    use mailbox_domain::{MailboxBinding, MailboxJob, MailboxJobStatus, MailboxProvider};
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

    fn job(
        binding: &MailboxBinding,
        max_attempts: u32,
    ) -> Result<MailboxJob, Box<dyn std::error::Error>> {
        Ok(MailboxJob::create(
            binding,
            MailboxJobId::parse("mailjob_01JMAILADAPTER")?,
            None,
            UnixMillis::new(1),
            max_attempts,
        )?)
    }

    #[test]
    fn metadata_adapter_returns_only_bounded_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding, 3)?;
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
    fn retryable_failure_schedules_retry_without_payload_data()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding, 3)?;
        let mut adapter = DeterministicFakeMailboxProvider::new(
            DeterministicMailboxOutcome::RetryableFailure,
        );
        let decision = decide_mailbox_run(&binding, &job, UnixMillis::new(10), &mut adapter)?;
        assert_eq!(decision.status(), MailboxJobStatus::RetryPending);
        assert_eq!(decision.attempt(), 1);
        assert_eq!(decision.version().value(), 3);
        assert_eq!(decision.retry_at(), Some(UnixMillis::new(60_010)));
        assert_eq!(decision.provider_status(), "RETRYABLE_FAILURE");
        assert_eq!(decision.bounded_item_count(), 0);
        assert_eq!(adapter.calls(), 1);
        Ok(())
    }

    #[test]
    fn terminal_failure_is_terminal() -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding, 3)?;
        let mut adapter = DeterministicFakeMailboxProvider::new(
            DeterministicMailboxOutcome::TerminalFailure,
        );
        let decision = decide_mailbox_run(&binding, &job, UnixMillis::new(10), &mut adapter)?;
        assert_eq!(decision.status(), MailboxJobStatus::Failed);
        assert_eq!(decision.attempt(), 1);
        assert_eq!(decision.version().value(), 3);
        Ok(())
    }

    #[test]
    fn single_attempt_retryable_failure_becomes_failed()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding, 1)?;
        let mut adapter = DeterministicFakeMailboxProvider::new(
            DeterministicMailboxOutcome::RetryableFailure,
        );
        let decision = decide_mailbox_run(&binding, &job, UnixMillis::new(10), &mut adapter)?;
        assert_eq!(decision.status(), MailboxJobStatus::Failed);
        assert_eq!(decision.provider_status(), "RETRY_EXHAUSTED");
        Ok(())
    }

    #[test]
    fn metadata_success_advances_cursor_and_finishes()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding, 3)?;
        let mut adapter = MetadataMailboxProviderAdapter::new(
            MailboxProvider::Imap,
            "SYNTHETIC_OK",
            2,
            Some("cursor-2".to_owned()),
        )?;
        let decision = decide_mailbox_run(&binding, &job, UnixMillis::new(10), &mut adapter)?;
        assert_eq!(decision.status(), MailboxJobStatus::Succeeded);
        assert_eq!(decision.cursor(), Some("cursor-2"));
        assert_eq!(decision.bounded_item_count(), 2);
        assert_eq!(decision.version().value(), 3);
        Ok(())
    }
}
