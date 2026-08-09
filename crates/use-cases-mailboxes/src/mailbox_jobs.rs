use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_jobs::{
    MailboxJobApplicationPort, MailboxJobCreateWrite, MailboxJobPortError,
    MailboxJobPortErrorClass, MailboxJobPreparedRun, MailboxJobReadModel, MailboxJobRunWrite,
    MailboxJobStatus,
};
use application_ports::mailboxes::{MailboxReplayDecision, MailboxReplayReceipt};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, MailboxJobId, UnixMillis,
};

const MAILBOX_JOB_CREATE_COMMAND: &str = "mailbox.job_create";
const MAILBOX_JOB_RUN_COMMAND: &str = "mailbox.job_run";
const MAILBOX_JOB_EVENT_PAYLOAD: &str = "{}";
const MAX_JOB_DELAY_MS: u64 = 604_800_000;
const MAX_CURSOR_LENGTH: usize = 512;
const MAX_JOB_ATTEMPTS: u32 = 10;

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

pub async fn execute_run_mailbox_job<P: MailboxJobApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &mut P,
    command: ExecuteRunMailboxJobCommand,
) -> Result<MailboxJobMutationOutcome, MailboxJobOperationError> {
    authorize_mailbox_job(role)?;
    let response_version = validate_mailbox_job_run_version(command.expected_version)?;

    match port
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

    let binding = port
        .find_binding(actor.tenant_scope(), &command.binding_id)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxJobOperationError::NotFound)?;
    let job = port
        .find_job(actor.tenant_scope(), &command.binding_id, &command.job_id)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxJobOperationError::NotFound)?;
    if job.job().version() != command.expected_version {
        return Err(MailboxJobOperationError::VersionConflict);
    }

    let prepared = port
        .prepare_run(&binding, job.job(), command.evidence.now())
        .map_err(map_port_error)?;
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
    match port.run_job(actor, &write).await {
        Ok(()) => Ok(MailboxJobMutationOutcome {
            result_code: result_code.to_owned(),
            resource_id: write.job_id().as_str().to_owned(),
            aggregate_version: response_version,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxJobPortErrorClass::Conflict => {
            match port
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

fn result_code<D>(
    prepared: &MailboxJobPreparedRun<D>,
) -> Result<&'static str, MailboxJobOperationError> {
    match prepared.status() {
        MailboxJobStatus::Succeeded => Ok("succeeded"),
        MailboxJobStatus::RetryPending => Ok("retry_pending"),
        MailboxJobStatus::Failed => Ok("failed"),
        MailboxJobStatus::Scheduled
        | MailboxJobStatus::Queued
        | MailboxJobStatus::Running
        | MailboxJobStatus::AuthRequired
        | MailboxJobStatus::Suspended => Err(MailboxJobOperationError::IntegrityFailure),
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
        MailboxJobOperationError, authorize_mailbox_job, validate_create_mailbox_job_request,
        validate_mailbox_job_run_version,
    };
    use identity_access_domain::MembershipRole;
    use profile_platform_primitives::AggregateVersion;

    #[test]
    fn owner_only_authorization_is_disclosure_neutral() {
        assert_eq!(authorize_mailbox_job(MembershipRole::TenantOwner), Ok(()));
        assert_eq!(
            authorize_mailbox_job(MembershipRole::Member),
            Err(MailboxJobOperationError::NotFound)
        );
    }

    #[test]
    fn request_validation_is_bounded_and_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(validate_create_mailbox_job_request(0, 3, None), Ok(()));
        assert_eq!(
            validate_create_mailbox_job_request(0, 3, Some(&"x".repeat(513))),
            Err(MailboxJobOperationError::InvalidRequest)
        );
        assert_eq!(
            validate_mailbox_job_run_version(AggregateVersion::INITIAL)?.value(),
            3
        );
        Ok(())
    }
}
