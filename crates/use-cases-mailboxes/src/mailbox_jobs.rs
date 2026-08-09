use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_jobs::{
    MailboxJobApplicationPort, MailboxJobCreateWrite, MailboxJobPortError,
    MailboxJobPortErrorClass, MailboxJobPreparedRun, MailboxJobReadModel, MailboxJobRunWrite,
    MailboxJobStatus,
};
use application_ports::mailboxes::{
    MailboxProviderPort, MailboxProviderPortError, MailboxReplayDecision, MailboxReplayReceipt,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use mailbox_domain::{
    MailboxError, MailboxFailureDisposition, MailboxObservation, MailboxProviderFailure,
};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, MailboxJobId, UnixMillis,
};

const MAILBOX_JOB_CREATE_COMMAND: &str = "mailbox.job_create";
const MAILBOX_JOB_RUN_COMMAND: &str = "mailbox.job_run";
const MAILBOX_JOB_EVENT_PAYLOAD: &str = "{}";
const MAX_JOB_DELAY_MS: u64 = 604_800_000;
const MAX_CURSOR_LENGTH: usize = 512;
const MAX_JOB_ATTEMPTS: u32 = 10;
const RETRY_BASE_DELAY_MS: u64 = 30_000;
const RETRY_MAX_DELAY_MS: u64 = 900_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteCreateMailboxJobCommand {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    cursor: Option<String>,
    delay_ms: u64,
    max_attempts: u32,
    evidence: CommandExecutionEvidence,
}

impl ExecuteCreateMailboxJobCommand {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        cursor: Option<String>,
        delay_ms: u64,
        max_attempts: u32,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            cursor,
            delay_ms,
            max_attempts,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteRunMailboxJobCommand {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
}

impl ExecuteRunMailboxJobCommand {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            expected_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobMutationOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl MailboxJobMutationOutcome {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobDetails {
    job_id: MailboxJobId,
    status: MailboxJobStatus,
    attempt: u32,
    max_attempts: u32,
    next_run_at: UnixMillis,
    provider_status: Option<String>,
    bounded_item_count: u32,
    version: AggregateVersion,
}

impl MailboxJobDetails {
    #[must_use]
    pub const fn job_id(&self) -> &MailboxJobId {
        &self.job_id
    }

    #[must_use]
    pub const fn status(&self) -> MailboxJobStatus {
        self.status
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    #[must_use]
    pub const fn next_run_at(&self) -> UnixMillis {
        self.next_run_at
    }

    #[must_use]
    pub fn provider_status(&self) -> Option<&str> {
        self.provider_status.as_deref()
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

impl From<MailboxJobReadModel> for MailboxJobDetails {
    fn from(value: MailboxJobReadModel) -> Self {
        Self {
            job_id: value.job().job_id().clone(),
            status: value.job().status(),
            attempt: value.job().attempt(),
            max_attempts: value.job().max_attempts(),
            next_run_at: value.job().next_run_at(),
            provider_status: value.provider_status().map(str::to_owned),
            bounded_item_count: value.bounded_item_count(),
            version: value.job().version(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxJobOperationError {
    InvalidRequest,
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for MailboxJobOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "mailbox job request is invalid",
            Self::NotFound => "mailbox job not found",
            Self::VersionConflict => "mailbox job version conflict",
            Self::InvalidState => "mailbox job invalid state",
            Self::Conflict => "mailbox job command conflict",
            Self::IntegrityFailure => "mailbox job integrity failure",
            Self::InternalFailure => "mailbox job internal failure",
            Self::DependencyUnavailable => "mailbox job dependency unavailable",
        })
    }
}

impl std::error::Error for MailboxJobOperationError {}

pub fn authorize_mailbox_job(role: MembershipRole) -> Result<(), MailboxJobOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(MailboxJobOperationError::NotFound)
    }
}

pub fn validate_create_mailbox_job_request(
    delay_ms: u64,
    max_attempts: u32,
    cursor: Option<&str>,
) -> Result<(), MailboxJobOperationError> {
    if delay_ms > MAX_JOB_DELAY_MS
        || max_attempts == 0
        || max_attempts > MAX_JOB_ATTEMPTS
        || cursor.is_some_and(|value| value.len() > MAX_CURSOR_LENGTH)
    {
        return Err(MailboxJobOperationError::InvalidRequest);
    }
    Ok(())
}

pub fn validate_mailbox_job_run_version(
    expected_version: AggregateVersion,
) -> Result<AggregateVersion, MailboxJobOperationError> {
    expected_version
        .next()
        .and_then(AggregateVersion::next)
        .and_then(AggregateVersion::next)
        .map_err(|_| MailboxJobOperationError::InternalFailure)
}

pub async fn execute_create_mailbox_job<P: MailboxJobApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteCreateMailboxJobCommand,
) -> Result<MailboxJobMutationOutcome, MailboxJobOperationError> {
    authorize_mailbox_job(role)?;
    validate_create_mailbox_job_request(
        command.delay_ms,
        command.max_attempts,
        command.cursor.as_deref(),
    )?;
    let scheduled_at = UnixMillis::new(
        command
            .evidence
            .now()
            .value()
            .checked_add(command.delay_ms)
            .ok_or(MailboxJobOperationError::InternalFailure)?,
    );

    match port
        .decide_replay(actor, MAILBOX_JOB_CREATE_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        MailboxReplayDecision::Miss => {}
        MailboxReplayDecision::Replay(receipt) => {
            return Ok(create_replay_outcome(&command.job_id, &receipt));
        }
        MailboxReplayDecision::Conflict => return Err(MailboxJobOperationError::Conflict),
    }

    let write = MailboxJobCreateWrite::new(
        command.binding_id,
        command.job_id,
        command.cursor,
        scheduled_at,
        command.max_attempts,
        command.evidence,
        MAILBOX_JOB_EVENT_PAYLOAD,
    );
    match port.create_job(actor, &write).await {
        Ok(()) => Ok(MailboxJobMutationOutcome {
            result_code: "created".to_owned(),
            resource_id: write.job_id().as_str().to_owned(),
            aggregate_version: AggregateVersion::INITIAL,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxJobPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, MAILBOX_JOB_CREATE_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxReplayDecision::Replay(receipt) => {
                    Ok(create_replay_outcome(write.job_id(), &receipt))
                }
                MailboxReplayDecision::Miss | MailboxReplayDecision::Conflict => {
                    Err(MailboxJobOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn get_mailbox_job<P: MailboxJobApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    binding_id: &MailboxBindingId,
    job_id: &MailboxJobId,
) -> Result<MailboxJobDetails, MailboxJobOperationError> {
    authorize_mailbox_job(role)?;
    port.find_job(actor.tenant_scope(), binding_id, job_id)
        .await
        .map_err(map_port_error)?
        .map(MailboxJobDetails::from)
        .ok_or(MailboxJobOperationError::NotFound)
}

pub async fn execute_run_mailbox_job<R, P>(
    actor: &ActorContext,
    role: MembershipRole,
    repository: &R,
    provider: &mut P,
    command: ExecuteRunMailboxJobCommand,
) -> Result<MailboxJobMutationOutcome, MailboxJobOperationError>
where
    R: MailboxJobApplicationPort,
    P: MailboxProviderPort,
{
    authorize_mailbox_job(role)?;
    let response_version = validate_mailbox_job_run_version(command.expected_version)?;

    match repository
        .decide_replay(actor, MAILBOX_JOB_RUN_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        MailboxReplayDecision::Miss => {}
        MailboxReplayDecision::Replay(receipt) => {
            return Ok(run_replay_outcome(
                &command.job_id,
                response_version,
                &receipt,
            ));
        }
        MailboxReplayDecision::Conflict => return Err(MailboxJobOperationError::Conflict),
    }

    let binding = repository
        .find_binding(actor.tenant_scope(), &command.binding_id)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxJobOperationError::NotFound)?;
    let job = repository
        .find_job(actor.tenant_scope(), &command.binding_id, &command.job_id)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxJobOperationError::NotFound)?;
    if job.job().version() != command.expected_version {
        return Err(MailboxJobOperationError::VersionConflict);
    }

    let prepared =
        prepare_mailbox_run(&binding, job.job(), command.evidence.now(), provider).await?;
    if prepared.version() != response_version {
        return Err(MailboxJobOperationError::IntegrityFailure);
    }
    let result_code = result_code(&prepared)?;
    let write = MailboxJobRunWrite::new(
        command.binding_id,
        command.job_id,
        command.expected_version,
        prepared,
        command.evidence,
        MAILBOX_JOB_EVENT_PAYLOAD,
    );
    match repository.run_job(actor, &write).await {
        Ok(()) => Ok(MailboxJobMutationOutcome {
            result_code: result_code.to_owned(),
            resource_id: write.job_id().as_str().to_owned(),
            aggregate_version: response_version,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxJobPortErrorClass::Conflict => {
            match repository
                .decide_replay(actor, MAILBOX_JOB_RUN_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxReplayDecision::Replay(receipt) => Ok(run_replay_outcome(
                    write.job_id(),
                    response_version,
                    &receipt,
                )),
                MailboxReplayDecision::Miss | MailboxReplayDecision::Conflict => {
                    Err(MailboxJobOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

async fn prepare_mailbox_run<P: MailboxProviderPort>(
    binding: &mailbox_domain::MailboxBinding,
    job: &mailbox_domain::MailboxJob,
    now: UnixMillis,
    provider: &mut P,
) -> Result<MailboxJobPreparedRun, MailboxJobOperationError> {
    let mut next = job.clone();
    next.queue(now).map_err(map_domain_error)?;
    next.start(binding).map_err(map_domain_error)?;
    let provider_result = provider.check_mailbox(binding, &next).await;
    apply_provider_result(binding, &mut next, now, provider_result)
}

fn apply_provider_result(
    binding: &mailbox_domain::MailboxBinding,
    job: &mut mailbox_domain::MailboxJob,
    now: UnixMillis,
    provider_result: Result<MailboxObservation, MailboxProviderPortError>,
) -> Result<MailboxJobPreparedRun, MailboxJobOperationError> {
    match provider_result {
        Ok(observation) => {
            if observation.binding_id() != binding.binding_id() {
                return Err(MailboxJobOperationError::IntegrityFailure);
            }
            job.succeed(observation.next_cursor().map(str::to_owned))
                .map_err(map_domain_error)?;
            Ok(MailboxJobPreparedRun::from_job(
                job,
                observation.provider_status(),
                observation.bounded_item_count(),
            ))
        }
        Err(MailboxProviderPortError::Failure(failure)) => {
            apply_provider_failure(job, now, failure)
        }
        Err(MailboxProviderPortError::IntegrityFailure) => {
            Err(MailboxJobOperationError::IntegrityFailure)
        }
    }
}

fn apply_provider_failure(
    job: &mut mailbox_domain::MailboxJob,
    now: UnixMillis,
    failure: MailboxProviderFailure,
) -> Result<MailboxJobPreparedRun, MailboxJobOperationError> {
    let provider_status = failure.class().storage_value();
    match failure.disposition() {
        MailboxFailureDisposition::Retryable => {
            if job.attempt() >= job.max_attempts() {
                job.fail().map_err(map_domain_error)?;
                return Ok(MailboxJobPreparedRun::from_job(job, "RETRY_EXHAUSTED", 0));
            }
            let retry_at = bounded_retry_at(now, job.attempt(), failure.retry_at())?;
            job.retry(now, retry_at).map_err(map_domain_error)?;
        }
        MailboxFailureDisposition::AuthRequired => {
            job.require_auth().map_err(map_domain_error)?;
        }
        MailboxFailureDisposition::Suspended => {
            job.suspend().map_err(map_domain_error)?;
        }
        MailboxFailureDisposition::Terminal => {
            job.fail().map_err(map_domain_error)?;
        }
    }
    Ok(MailboxJobPreparedRun::from_job(job, provider_status, 0))
}

fn bounded_retry_at(
    now: UnixMillis,
    attempt: u32,
    provider_hint: Option<UnixMillis>,
) -> Result<UnixMillis, MailboxJobOperationError> {
    let exponent = attempt.saturating_sub(1).min(5);
    let factor = 1_u64 << exponent;
    let policy_delay = RETRY_BASE_DELAY_MS
        .checked_mul(factor)
        .unwrap_or(RETRY_MAX_DELAY_MS)
        .min(RETRY_MAX_DELAY_MS);
    let policy_at = UnixMillis::new(
        now.value()
            .checked_add(policy_delay)
            .ok_or(MailboxJobOperationError::InternalFailure)?,
    );
    let max_at = UnixMillis::new(
        now.value()
            .checked_add(RETRY_MAX_DELAY_MS)
            .ok_or(MailboxJobOperationError::InternalFailure)?,
    );
    Ok(provider_hint
        .filter(|hint| *hint > now)
        .map_or(policy_at, |hint| hint.max(policy_at).min(max_at)))
}

fn result_code(prepared: &MailboxJobPreparedRun) -> Result<&'static str, MailboxJobOperationError> {
    match prepared.status() {
        MailboxJobStatus::Succeeded => Ok("succeeded"),
        MailboxJobStatus::RetryPending => Ok("retry_pending"),
        MailboxJobStatus::AuthRequired => Ok("auth_required"),
        MailboxJobStatus::Suspended => Ok("suspended"),
        MailboxJobStatus::Failed => Ok("failed"),
        MailboxJobStatus::Scheduled | MailboxJobStatus::Queued | MailboxJobStatus::Running => {
            Err(MailboxJobOperationError::IntegrityFailure)
        }
    }
}

fn create_replay_outcome(
    job_id: &MailboxJobId,
    receipt: &MailboxReplayReceipt,
) -> MailboxJobMutationOutcome {
    MailboxJobMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(job_id.as_str())
            .to_owned(),
        aggregate_version: AggregateVersion::INITIAL,
        replayed: true,
    }
}

fn run_replay_outcome(
    job_id: &MailboxJobId,
    response_version: AggregateVersion,
    receipt: &MailboxReplayReceipt,
) -> MailboxJobMutationOutcome {
    MailboxJobMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(job_id.as_str())
            .to_owned(),
        aggregate_version: response_version,
        replayed: true,
    }
}

fn map_domain_error(error: MailboxError) -> MailboxJobOperationError {
    if error == MailboxError::VersionOverflow {
        MailboxJobOperationError::InternalFailure
    } else {
        MailboxJobOperationError::InvalidState
    }
}

fn map_port_error(error: MailboxJobPortError) -> MailboxJobOperationError {
    match error.class() {
        MailboxJobPortErrorClass::NotFound => MailboxJobOperationError::NotFound,
        MailboxJobPortErrorClass::VersionConflict => MailboxJobOperationError::VersionConflict,
        MailboxJobPortErrorClass::InvalidState => MailboxJobOperationError::InvalidState,
        MailboxJobPortErrorClass::Conflict => MailboxJobOperationError::Conflict,
        MailboxJobPortErrorClass::IntegrityFailure => MailboxJobOperationError::IntegrityFailure,
        MailboxJobPortErrorClass::InternalFailure => MailboxJobOperationError::InternalFailure,
        MailboxJobPortErrorClass::DependencyUnavailable => {
            MailboxJobOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MailboxJobOperationError, apply_provider_failure, authorize_mailbox_job, bounded_retry_at,
        validate_create_mailbox_job_request, validate_mailbox_job_run_version,
    };
    use identity_access_domain::MembershipRole;
    use mailbox_domain::{
        MailboxBinding, MailboxJob, MailboxJobStatus, MailboxProvider, MailboxProviderFailure,
        MailboxProviderFailureClass,
    };
    use profile_platform_primitives::{
        AggregateVersion, MailboxBindingId, MailboxJobId, SecretHandle, TenantId, UnixMillis,
    };

    fn binding() -> Result<MailboxBinding, Box<dyn std::error::Error>> {
        Ok(MailboxBinding::create(
            TenantId::parse("tenant_01JMAILAPP")?,
            MailboxBindingId::parse("mailbox_01JMAILAPP")?,
            MailboxProvider::Imap,
            SecretHandle::parse("secret_01JMAILAPP")?,
        ))
    }

    fn running_job(
        binding: &MailboxBinding,
        max_attempts: u32,
    ) -> Result<MailboxJob, Box<dyn std::error::Error>> {
        let mut job = MailboxJob::create(
            binding,
            MailboxJobId::parse("mailjob_01JMAILAPP")?,
            None,
            UnixMillis::new(1),
            max_attempts,
        )?;
        job.queue(UnixMillis::new(1))?;
        job.start(binding)?;
        Ok(job)
    }

    #[test]
    fn owner_only_authorization_is_disclosure_neutral() {
        assert_eq!(authorize_mailbox_job(MembershipRole::TenantOwner), Ok(()));
        assert_eq!(
            authorize_mailbox_job(MembershipRole::Member),
            Err(MailboxJobOperationError::NotFound)
        );
    }

    #[test]
    fn request_validation_and_run_version_are_fail_closed() -> Result<(), Box<dyn std::error::Error>>
    {
        assert_eq!(validate_create_mailbox_job_request(0, 3, None), Ok(()));
        assert_eq!(
            validate_create_mailbox_job_request(0, 3, Some(&"x".repeat(513))),
            Err(MailboxJobOperationError::InvalidRequest)
        );
        assert_eq!(
            validate_mailbox_job_run_version(AggregateVersion::INITIAL)?.value(),
            4
        );
        Ok(())
    }

    #[test]
    fn retry_policy_is_bounded_and_provider_hint_cannot_escape_bound()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            bounded_retry_at(UnixMillis::new(10), 1, None)?.value(),
            30_010
        );
        assert_eq!(
            bounded_retry_at(UnixMillis::new(10), 1, Some(UnixMillis::new(99_999_999)),)?.value(),
            900_010
        );
        Ok(())
    }

    #[test]
    fn provider_failures_drive_canonical_job_states_in_application()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;

        let mut retry = running_job(&binding, 3)?;
        let retry_failure = MailboxProviderFailure::new(
            MailboxProviderFailureClass::RateLimited,
            Some(UnixMillis::new(40_000)),
        )?;
        let retry_prepared =
            apply_provider_failure(&mut retry, UnixMillis::new(10), retry_failure)?;
        assert_eq!(retry_prepared.status(), MailboxJobStatus::RetryPending);
        assert_eq!(retry_prepared.version().value(), 4);
        assert_eq!(retry_prepared.retry_at(), Some(UnixMillis::new(40_000)));

        let mut auth = running_job(&binding, 3)?;
        let auth_prepared = apply_provider_failure(
            &mut auth,
            UnixMillis::new(10),
            MailboxProviderFailure::new(MailboxProviderFailureClass::Authentication, None)?,
        )?;
        assert_eq!(auth_prepared.status(), MailboxJobStatus::AuthRequired);

        let mut suspended = running_job(&binding, 3)?;
        let suspended_prepared = apply_provider_failure(
            &mut suspended,
            UnixMillis::new(10),
            MailboxProviderFailure::new(MailboxProviderFailureClass::ProviderPolicy, None)?,
        )?;
        assert_eq!(suspended_prepared.status(), MailboxJobStatus::Suspended);

        let mut terminal = running_job(&binding, 3)?;
        let terminal_prepared = apply_provider_failure(
            &mut terminal,
            UnixMillis::new(10),
            MailboxProviderFailure::new(MailboxProviderFailureClass::Permanent, None)?,
        )?;
        assert_eq!(terminal_prepared.status(), MailboxJobStatus::Failed);
        Ok(())
    }
}
