use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceExecutionBlocker, DeviceExecutionPreconditionPort,
    DeviceExecutionReadiness, DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobInsertOutcome,
    DeviceJobPortError, DeviceJobPortErrorClass, DeviceJobRepositoryPort, DeviceJobWriteOutcome,
};
use device_domain::{
    DeviceClaimId, DeviceJob, DeviceJobError, DeviceJobId, DeviceJobStatus, DeviceJobTarget,
};
use profile_platform_primitives::{ActorContext, AggregateVersion, UnixMillis};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueDeviceJobCommand {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    max_attempts: u32,
    issued_at: UnixMillis,
}

impl IssueDeviceJobCommand {
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        max_attempts: u32,
        issued_at: UnixMillis,
    ) -> Self {
        Self {
            job_id,
            target,
            max_attempts,
            issued_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimDeviceJobCommand {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    expected_version: AggregateVersion,
    claim_id: DeviceClaimId,
    observed_at: UnixMillis,
    lease_expires_at: UnixMillis,
}

impl ClaimDeviceJobCommand {
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        expected_version: AggregateVersion,
        claim_id: DeviceClaimId,
        observed_at: UnixMillis,
        lease_expires_at: UnixMillis,
    ) -> Self {
        Self {
            job_id,
            target,
            expected_version,
            claim_id,
            observed_at,
            lease_expires_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartbeatDeviceJobCommand {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    expected_version: AggregateVersion,
    claim_id: DeviceClaimId,
    fence: u64,
    observed_at: UnixMillis,
    lease_expires_at: UnixMillis,
}

impl HeartbeatDeviceJobCommand {
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        expected_version: AggregateVersion,
        claim_id: DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
        lease_expires_at: UnixMillis,
    ) -> Self {
        Self {
            job_id,
            target,
            expected_version,
            claim_id,
            fence,
            observed_at,
            lease_expires_at,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobOutcome {
    Succeeded,
    RetryScheduled { retry_at: UnixMillis },
    ProfileBusy { retry_at: UnixMillis },
    AuthRequired,
    RecoveryRequired,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyDeviceJobOutcomeCommand {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    expected_version: AggregateVersion,
    claim_id: DeviceClaimId,
    fence: u64,
    observed_at: UnixMillis,
    outcome: DeviceJobOutcome,
}

impl ApplyDeviceJobOutcomeCommand {
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        expected_version: AggregateVersion,
        claim_id: DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
        outcome: DeviceJobOutcome,
    ) -> Self {
        Self {
            job_id,
            target,
            expected_version,
            claim_id,
            fence,
            observed_at,
            outcome,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpireDeviceClaimCommand {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    expected_version: AggregateVersion,
    observed_at: UnixMillis,
    retry_at: UnixMillis,
}

impl ExpireDeviceClaimCommand {
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        expected_version: AggregateVersion,
        observed_at: UnixMillis,
        retry_at: UnixMillis,
    ) -> Self {
        Self {
            job_id,
            target,
            expected_version,
            observed_at,
            retry_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeDeviceJobCommand {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    expected_version: AggregateVersion,
    observed_at: UnixMillis,
}

impl ResumeDeviceJobCommand {
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        expected_version: AggregateVersion,
        observed_at: UnixMillis,
    ) -> Self {
        Self {
            job_id,
            target,
            expected_version,
            observed_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelDeviceJobCommand {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    expected_version: AggregateVersion,
    observed_at: UnixMillis,
}

impl CancelDeviceJobCommand {
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        expected_version: AggregateVersion,
        observed_at: UnixMillis,
    ) -> Self {
        Self {
            job_id,
            target,
            expected_version,
            observed_at,
        }
    }
}

pub async fn execute_issue_device_job<A, R>(
    actor: &ActorContext,
    authorization: &A,
    repository: &R,
    command: IssueDeviceJobCommand,
) -> Result<DeviceJob, DeviceJobOperationError>
where
    A: DeviceJobAuthorizationPort,
    R: DeviceJobRepositoryPort,
{
    ensure_tenant(actor, &command.target)?;
    authorize(
        authorization,
        actor,
        &command.target,
        DeviceJobCapability::Issue,
    )
    .await?;
    let job = DeviceJob::issue(
        command.job_id,
        command.target,
        command.max_attempts,
        command.issued_at,
    )
    .map_err(DeviceJobOperationError::Domain)?;
    match repository
        .insert_device_job(actor.tenant_scope().tenant_id(), &job)
        .await
        .map_err(map_port_error)?
    {
        DeviceJobInsertOutcome::Inserted => Ok(job),
        DeviceJobInsertOutcome::Conflict => Err(DeviceJobOperationError::Conflict),
    }
}

pub async fn execute_claim_device_job<D, A, P, R>(
    actor: &ActorContext,
    device_identity: &D,
    authorization: &A,
    preconditions: &P,
    repository: &R,
    command: ClaimDeviceJobCommand,
) -> Result<DeviceJob, DeviceJobOperationError>
where
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
    R: DeviceJobRepositoryPort,
{
    ensure_tenant(actor, &command.target)?;
    ensure_authenticated_device(device_identity, actor, &command.target).await?;
    authorize(
        authorization,
        actor,
        &command.target,
        DeviceJobCapability::Claim,
    )
    .await?;
    ensure_execution_ready(preconditions, actor, &command.target).await?;
    let mut job = load_exact_job(
        repository,
        actor,
        &command.job_id,
        &command.target,
        command.expected_version,
    )
    .await?;
    job.claim(
        command.claim_id,
        command.observed_at,
        command.lease_expires_at,
    )
    .map_err(DeviceJobOperationError::Domain)?;
    persist(repository, actor, command.expected_version, &job).await?;
    Ok(job)
}

pub async fn execute_heartbeat_device_job<D, A, R>(
    actor: &ActorContext,
    device_identity: &D,
    authorization: &A,
    repository: &R,
    command: HeartbeatDeviceJobCommand,
) -> Result<DeviceJob, DeviceJobOperationError>
where
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    R: DeviceJobRepositoryPort,
{
    ensure_tenant(actor, &command.target)?;
    ensure_authenticated_device(device_identity, actor, &command.target).await?;
    authorize(
        authorization,
        actor,
        &command.target,
        DeviceJobCapability::Heartbeat,
    )
    .await?;
    let mut job = load_exact_job(
        repository,
        actor,
        &command.job_id,
        &command.target,
        command.expected_version,
    )
    .await?;
    job.heartbeat(
        &command.claim_id,
        command.fence,
        command.observed_at,
        command.lease_expires_at,
    )
    .map_err(DeviceJobOperationError::Domain)?;
    persist(repository, actor, command.expected_version, &job).await?;
    Ok(job)
}

pub async fn execute_apply_device_job_outcome<D, A, P, R>(
    actor: &ActorContext,
    device_identity: &D,
    authorization: &A,
    preconditions: &P,
    repository: &R,
    command: ApplyDeviceJobOutcomeCommand,
) -> Result<DeviceJob, DeviceJobOperationError>
where
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
    R: DeviceJobRepositoryPort,
{
    ensure_tenant(actor, &command.target)?;
    ensure_authenticated_device(device_identity, actor, &command.target).await?;
    authorize(
        authorization,
        actor,
        &command.target,
        DeviceJobCapability::Complete,
    )
    .await?;
    ensure_execution_ready(preconditions, actor, &command.target).await?;
    let mut job = load_exact_job(
        repository,
        actor,
        &command.job_id,
        &command.target,
        command.expected_version,
    )
    .await?;
    match command.outcome {
        DeviceJobOutcome::Succeeded => {
            job.succeed(&command.claim_id, command.fence, command.observed_at)
        }
        DeviceJobOutcome::RetryScheduled { retry_at } => job.schedule_retry(
            &command.claim_id,
            command.fence,
            command.observed_at,
            retry_at,
        ),
        DeviceJobOutcome::ProfileBusy { retry_at } => job.mark_profile_busy(
            &command.claim_id,
            command.fence,
            command.observed_at,
            retry_at,
        ),
        DeviceJobOutcome::AuthRequired => {
            job.require_auth(&command.claim_id, command.fence, command.observed_at)
        }
        DeviceJobOutcome::RecoveryRequired => {
            job.require_recovery(&command.claim_id, command.fence, command.observed_at)
        }
        DeviceJobOutcome::Failed => job.fail(&command.claim_id, command.fence, command.observed_at),
    }
    .map_err(DeviceJobOperationError::Domain)?;
    persist(repository, actor, command.expected_version, &job).await?;
    Ok(job)
}

pub async fn execute_expire_device_claim<A, R>(
    actor: &ActorContext,
    authorization: &A,
    repository: &R,
    command: ExpireDeviceClaimCommand,
) -> Result<DeviceJob, DeviceJobOperationError>
where
    A: DeviceJobAuthorizationPort,
    R: DeviceJobRepositoryPort,
{
    ensure_tenant(actor, &command.target)?;
    authorize(
        authorization,
        actor,
        &command.target,
        DeviceJobCapability::Recover,
    )
    .await?;
    let mut job = load_exact_job(
        repository,
        actor,
        &command.job_id,
        &command.target,
        command.expected_version,
    )
    .await?;
    job.expire_claim(command.observed_at, command.retry_at)
        .map_err(DeviceJobOperationError::Domain)?;
    persist(repository, actor, command.expected_version, &job).await?;
    Ok(job)
}

pub async fn execute_resume_device_job<A, P, R>(
    actor: &ActorContext,
    authorization: &A,
    preconditions: &P,
    repository: &R,
    command: ResumeDeviceJobCommand,
) -> Result<DeviceJob, DeviceJobOperationError>
where
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
    R: DeviceJobRepositoryPort,
{
    ensure_tenant(actor, &command.target)?;
    authorize(
        authorization,
        actor,
        &command.target,
        DeviceJobCapability::Recover,
    )
    .await?;
    ensure_execution_ready(preconditions, actor, &command.target).await?;
    let mut job = load_exact_job(
        repository,
        actor,
        &command.job_id,
        &command.target,
        command.expected_version,
    )
    .await?;
    match job.status() {
        DeviceJobStatus::AuthRequired => job.resume_after_auth(command.observed_at),
        DeviceJobStatus::RecoveryRequired => job.resume_after_recovery(command.observed_at),
        _ => Err(DeviceJobError::InvalidState),
    }
    .map_err(DeviceJobOperationError::Domain)?;
    persist(repository, actor, command.expected_version, &job).await?;
    Ok(job)
}

pub async fn execute_cancel_device_job<A, R>(
    actor: &ActorContext,
    authorization: &A,
    repository: &R,
    command: CancelDeviceJobCommand,
) -> Result<DeviceJob, DeviceJobOperationError>
where
    A: DeviceJobAuthorizationPort,
    R: DeviceJobRepositoryPort,
{
    ensure_tenant(actor, &command.target)?;
    authorize(
        authorization,
        actor,
        &command.target,
        DeviceJobCapability::Cancel,
    )
    .await?;
    let mut job = load_exact_job(
        repository,
        actor,
        &command.job_id,
        &command.target,
        command.expected_version,
    )
    .await?;
    job.cancel(command.observed_at)
        .map_err(DeviceJobOperationError::Domain)?;
    persist(repository, actor, command.expected_version, &job).await?;
    Ok(job)
}

fn ensure_tenant(
    actor: &ActorContext,
    target: &DeviceJobTarget,
) -> Result<(), DeviceJobOperationError> {
    if actor.tenant_scope().tenant_id() == target.tenant_id() {
        Ok(())
    } else {
        Err(DeviceJobOperationError::InvalidRequest)
    }
}

async fn ensure_authenticated_device<D: AuthenticatedDevicePort>(
    device_identity: &D,
    actor: &ActorContext,
    target: &DeviceJobTarget,
) -> Result<(), DeviceJobOperationError> {
    let authenticated = device_identity
        .authenticated_device_id(actor)
        .await
        .map_err(map_port_error)?;
    if &authenticated == target.device_id() {
        Ok(())
    } else {
        Err(DeviceJobOperationError::Forbidden)
    }
}

async fn authorize<A: DeviceJobAuthorizationPort>(
    authorization: &A,
    actor: &ActorContext,
    target: &DeviceJobTarget,
    capability: DeviceJobCapability,
) -> Result<(), DeviceJobOperationError> {
    if authorization
        .is_device_job_authorized(actor, target, capability)
        .await
        .map_err(map_port_error)?
    {
        Ok(())
    } else {
        Err(DeviceJobOperationError::Forbidden)
    }
}

async fn ensure_execution_ready<P: DeviceExecutionPreconditionPort>(
    preconditions: &P,
    actor: &ActorContext,
    target: &DeviceJobTarget,
) -> Result<(), DeviceJobOperationError> {
    match preconditions
        .evaluate_device_execution(actor, target)
        .await
        .map_err(map_port_error)?
    {
        DeviceExecutionReadiness::Ready => Ok(()),
        DeviceExecutionReadiness::Blocked(blocker) => {
            Err(DeviceJobOperationError::PreconditionFailed(blocker))
        }
    }
}

async fn load_exact_job<R: DeviceJobRepositoryPort>(
    repository: &R,
    actor: &ActorContext,
    job_id: &DeviceJobId,
    target: &DeviceJobTarget,
    expected_version: AggregateVersion,
) -> Result<DeviceJob, DeviceJobOperationError> {
    let job = repository
        .load_device_job(actor.tenant_scope().tenant_id(), job_id)
        .await
        .map_err(map_port_error)?
        .ok_or(DeviceJobOperationError::NotFound)?;
    if job.target() != target {
        return Err(DeviceJobOperationError::NotFound);
    }
    if job.version() != expected_version {
        return Err(DeviceJobOperationError::VersionConflict);
    }
    Ok(job)
}

async fn persist<R: DeviceJobRepositoryPort>(
    repository: &R,
    actor: &ActorContext,
    expected_version: AggregateVersion,
    job: &DeviceJob,
) -> Result<(), DeviceJobOperationError> {
    match repository
        .compare_and_swap_device_job(actor.tenant_scope().tenant_id(), expected_version, job)
        .await
        .map_err(map_port_error)?
    {
        DeviceJobWriteOutcome::Applied => Ok(()),
        DeviceJobWriteOutcome::VersionConflict => Err(DeviceJobOperationError::VersionConflict),
    }
}

fn map_port_error(error: DeviceJobPortError) -> DeviceJobOperationError {
    match error.class() {
        DeviceJobPortErrorClass::IntegrityFailure => DeviceJobOperationError::IntegrityFailure,
        DeviceJobPortErrorClass::DependencyUnavailable => {
            DeviceJobOperationError::DependencyUnavailable
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceJobOperationError {
    InvalidRequest,
    Forbidden,
    NotFound,
    Conflict,
    VersionConflict,
    PreconditionFailed(DeviceExecutionBlocker),
    Domain(DeviceJobError),
    IntegrityFailure,
    DependencyUnavailable,
}

impl core::fmt::Display for DeviceJobOperationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidRequest => formatter.write_str("device job request is invalid"),
            Self::Forbidden => formatter.write_str("device job operation is forbidden"),
            Self::NotFound => formatter.write_str("device job is not visible or does not exist"),
            Self::Conflict => formatter.write_str("device job already exists"),
            Self::VersionConflict => formatter.write_str("device job version conflict"),
            Self::PreconditionFailed(blocker) => {
                write!(
                    formatter,
                    "device execution precondition failed: {blocker:?}"
                )
            }
            Self::Domain(error) => error.fmt(formatter),
            Self::IntegrityFailure => formatter.write_str("device job integrity failure"),
            Self::DependencyUnavailable => formatter.write_str("device job dependency unavailable"),
        }
    }
}

impl std::error::Error for DeviceJobOperationError {}
