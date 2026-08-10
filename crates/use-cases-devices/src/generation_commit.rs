use application_ports::device_generation_commit::{
    DeviceGenerationCommitErrorClass, DeviceGenerationCommitOutcome, DeviceGenerationCommitPort,
    DeviceGenerationCommitRequest,
};
use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceExecutionBlocker, DeviceExecutionPreconditionPort,
    DeviceExecutionReadiness, DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobPortError,
    DeviceJobPortErrorClass, DeviceJobRepositoryPort,
};
use application_ports::generation_objects::GenerationObjectDescriptorVerifyPort;
use application_ports::generations::{GenerationPortError, GenerationPortErrorClass};
use core::fmt;
use device_domain::{DeviceJobStatus, DeviceJobTarget};
use profile_platform_primitives::ActorContext;

pub struct DeviceGenerationCommitServices<'a, D, A, P, R, V, C> {
    device_identity: &'a D,
    authorization: &'a A,
    preconditions: &'a P,
    repository: &'a R,
    object_verifier: &'a V,
    commit: &'a C,
}

impl<'a, D, A, P, R, V, C> DeviceGenerationCommitServices<'a, D, A, P, R, V, C> {
    #[must_use]
    pub const fn new(
        device_identity: &'a D,
        authorization: &'a A,
        preconditions: &'a P,
        repository: &'a R,
        object_verifier: &'a V,
        commit: &'a C,
    ) -> Self {
        Self {
            device_identity,
            authorization,
            preconditions,
            repository,
            object_verifier,
            commit,
        }
    }
}

pub async fn execute_commit_dirty_generation<D, A, P, R, V, C>(
    actor: &ActorContext,
    services: &DeviceGenerationCommitServices<'_, D, A, P, R, V, C>,
    request: &DeviceGenerationCommitRequest,
) -> Result<DeviceGenerationCommitOutcome, DeviceGenerationCommitOperationError>
where
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
    R: DeviceJobRepositoryPort,
    V: GenerationObjectDescriptorVerifyPort,
    C: DeviceGenerationCommitPort,
{
    let target = DeviceJobTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        request.device_id().clone(),
        request.profile_id().clone(),
        request.base_generation_id().clone(),
    );

    let authenticated = services
        .device_identity
        .authenticated_device_id(actor)
        .await
        .map_err(map_device_port_error)?;
    if authenticated != *request.device_id() {
        return Err(DeviceGenerationCommitOperationError::Forbidden);
    }

    if !services
        .authorization
        .is_device_job_authorized(actor, &target, DeviceJobCapability::Complete)
        .await
        .map_err(map_device_port_error)?
    {
        return Err(DeviceGenerationCommitOperationError::Forbidden);
    }

    match services
        .preconditions
        .evaluate_device_execution(actor, &target)
        .await
        .map_err(map_device_port_error)?
    {
        DeviceExecutionReadiness::Ready => {}
        DeviceExecutionReadiness::Blocked(blocker) => {
            return Err(DeviceGenerationCommitOperationError::PreconditionFailed(
                blocker,
            ));
        }
    }

    let job = services
        .repository
        .load_device_job(actor.tenant_scope().tenant_id(), request.job_id())
        .await
        .map_err(map_device_port_error)?
        .ok_or(DeviceGenerationCommitOperationError::NotFound)?;
    if job.target() != &target {
        return Err(DeviceGenerationCommitOperationError::NotFound);
    }
    if job.version() != request.expected_job_version() {
        return Err(DeviceGenerationCommitOperationError::VersionConflict);
    }
    if job.status() != DeviceJobStatus::Running {
        return Err(DeviceGenerationCommitOperationError::StaleClaim);
    }
    let claim = job
        .active_claim()
        .ok_or(DeviceGenerationCommitOperationError::StaleClaim)?;
    if claim.claim_id() != request.claim_id()
        || claim.fence() != request.claim_fence()
        || job.last_fence() != request.claim_fence()
        || claim.target() != &target
        || claim.is_expired(request.observed_at())
    {
        return Err(DeviceGenerationCommitOperationError::StaleClaim);
    }

    validate_request(actor, request)?;
    let verified = services
        .object_verifier
        .verify_generation_object_descriptor_exact(actor.tenant_scope(), request.object())
        .await
        .map_err(map_generation_port_error)?;
    if !verified {
        return Err(DeviceGenerationCommitOperationError::ObjectVerificationFailed);
    }

    services
        .commit
        .commit_device_generation(actor, request)
        .await
        .map_err(|error| DeviceGenerationCommitOperationError::Commit(error.class()))
}

fn validate_request(
    actor: &ActorContext,
    request: &DeviceGenerationCommitRequest,
) -> Result<(), DeviceGenerationCommitOperationError> {
    let object = request.object();
    let expected_coordinator_version = request
        .coordinator()
        .coordinator_sequence()
        .checked_add(1)
        .ok_or(DeviceGenerationCommitOperationError::InvalidRequest)?;
    if object.profile_id() != request.profile_id()
        || object.generation_id() == request.base_generation_id()
        || object.container_bytes() == 0
        || request.claim_fence() == 0
        || request.coordinator().epoch() == 0
        || request.coordinator().coordinator_version() == 0
        || request.coordinator().coordinator_sequence() == 0
        || request.coordinator().coordinator_version() != expected_coordinator_version
        || !is_sha256_hex(object.metadata_digest())
        || !is_sha256_hex(object.container_digest())
    {
        return Err(DeviceGenerationCommitOperationError::InvalidRequest);
    }
    let canonical = format!(
        "tenants/{}/profiles/{}/generations/{}.bpgc",
        actor.tenant_scope().tenant_id().as_str(),
        request.profile_id().as_str(),
        object.generation_id().as_str()
    );
    if object.object_key() != canonical {
        return Err(DeviceGenerationCommitOperationError::InvalidRequest);
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn map_device_port_error(error: DeviceJobPortError) -> DeviceGenerationCommitOperationError {
    match error.class() {
        DeviceJobPortErrorClass::AuthenticationFailed => {
            DeviceGenerationCommitOperationError::Forbidden
        }
        DeviceJobPortErrorClass::IntegrityFailure => {
            DeviceGenerationCommitOperationError::IntegrityFailure
        }
        DeviceJobPortErrorClass::DependencyUnavailable => {
            DeviceGenerationCommitOperationError::DependencyUnavailable
        }
    }
}

fn map_generation_port_error(error: GenerationPortError) -> DeviceGenerationCommitOperationError {
    match error.class() {
        GenerationPortErrorClass::DependencyUnavailable
        | GenerationPortErrorClass::InternalFailure => {
            DeviceGenerationCommitOperationError::DependencyUnavailable
        }
        GenerationPortErrorClass::IntegrityFailure
        | GenerationPortErrorClass::NotFound
        | GenerationPortErrorClass::VersionConflict
        | GenerationPortErrorClass::InvalidState
        | GenerationPortErrorClass::Conflict => {
            DeviceGenerationCommitOperationError::IntegrityFailure
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceGenerationCommitOperationError {
    InvalidRequest,
    Forbidden,
    NotFound,
    VersionConflict,
    StaleClaim,
    PreconditionFailed(DeviceExecutionBlocker),
    ObjectVerificationFailed,
    IntegrityFailure,
    DependencyUnavailable,
    Commit(DeviceGenerationCommitErrorClass),
}

impl fmt::Display for DeviceGenerationCommitOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "device generation commit request is invalid",
            Self::Forbidden => "device generation commit is forbidden",
            Self::NotFound => "device generation commit target was not found",
            Self::VersionConflict => "device generation job version is stale",
            Self::StaleClaim => "device generation claim or fence is stale",
            Self::PreconditionFailed(_) => "device generation execution precondition failed",
            Self::ObjectVerificationFailed => {
                "immutable generation object failed exact verification"
            }
            Self::IntegrityFailure => "device generation commit integrity validation failed",
            Self::DependencyUnavailable => "device generation commit dependency is unavailable",
            Self::Commit(_) => "device generation catalog commit failed",
        })
    }
}

impl std::error::Error for DeviceGenerationCommitOperationError {}
