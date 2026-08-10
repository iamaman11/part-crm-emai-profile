use crate::generation_objects::GenerationObjectDescriptor;
use core::{fmt, future::Future};
use device_domain::{DeviceClaimId, DeviceJobId};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, FencingToken, GenerationId, ProfileId, SessionId,
    UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorGenerationCommitWitness {
    session_id: SessionId,
    fencing_token: FencingToken,
    epoch: u64,
    coordinator_version: u64,
    coordinator_sequence: u64,
}

impl CoordinatorGenerationCommitWitness {
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        fencing_token: FencingToken,
        epoch: u64,
        coordinator_version: u64,
        coordinator_sequence: u64,
    ) -> Self {
        Self {
            session_id,
            fencing_token,
            epoch,
            coordinator_version,
            coordinator_sequence,
        }
    }

    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    #[must_use]
    pub const fn fencing_token(&self) -> &FencingToken {
        &self.fencing_token
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    #[must_use]
    pub const fn coordinator_version(&self) -> u64 {
        self.coordinator_version
    }

    #[must_use]
    pub const fn coordinator_sequence(&self) -> u64 {
        self.coordinator_sequence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceGenerationCommitRequest {
    job_id: DeviceJobId,
    claim_id: DeviceClaimId,
    expected_job_version: AggregateVersion,
    claim_fence: u64,
    device_id: DeviceId,
    profile_id: ProfileId,
    base_generation_id: GenerationId,
    object: GenerationObjectDescriptor,
    expected_profile_version: AggregateVersion,
    coordinator: CoordinatorGenerationCommitWitness,
    observed_at: UnixMillis,
}

impl DeviceGenerationCommitRequest {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        job_id: DeviceJobId,
        claim_id: DeviceClaimId,
        expected_job_version: AggregateVersion,
        claim_fence: u64,
        device_id: DeviceId,
        profile_id: ProfileId,
        base_generation_id: GenerationId,
        object: GenerationObjectDescriptor,
        expected_profile_version: AggregateVersion,
        coordinator: CoordinatorGenerationCommitWitness,
        observed_at: UnixMillis,
    ) -> Self {
        Self {
            job_id,
            claim_id,
            expected_job_version,
            claim_fence,
            device_id,
            profile_id,
            base_generation_id,
            object,
            expected_profile_version,
            coordinator,
            observed_at,
        }
    }

    #[must_use]
    pub const fn job_id(&self) -> &DeviceJobId {
        &self.job_id
    }

    #[must_use]
    pub const fn claim_id(&self) -> &DeviceClaimId {
        &self.claim_id
    }

    #[must_use]
    pub const fn expected_job_version(&self) -> AggregateVersion {
        self.expected_job_version
    }

    #[must_use]
    pub const fn claim_fence(&self) -> u64 {
        self.claim_fence
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn base_generation_id(&self) -> &GenerationId {
        &self.base_generation_id
    }

    #[must_use]
    pub const fn object(&self) -> &GenerationObjectDescriptor {
        &self.object
    }

    #[must_use]
    pub const fn expected_profile_version(&self) -> AggregateVersion {
        self.expected_profile_version
    }

    #[must_use]
    pub const fn coordinator(&self) -> &CoordinatorGenerationCommitWitness {
        &self.coordinator
    }

    #[must_use]
    pub const fn observed_at(&self) -> UnixMillis {
        self.observed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceGenerationCommitOutcome {
    Activated,
    AlreadyActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceGenerationCommitErrorClass {
    StaleAuthority,
    VersionConflict,
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceGenerationCommitError {
    class: DeviceGenerationCommitErrorClass,
}

impl DeviceGenerationCommitError {
    #[must_use]
    pub const fn new(class: DeviceGenerationCommitErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> DeviceGenerationCommitErrorClass {
        self.class
    }
}

impl fmt::Display for DeviceGenerationCommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            DeviceGenerationCommitErrorClass::StaleAuthority => {
                "device generation commit authority is stale"
            }
            DeviceGenerationCommitErrorClass::VersionConflict => {
                "device generation commit lost an optimistic concurrency race"
            }
            DeviceGenerationCommitErrorClass::IntegrityFailure => {
                "device generation commit integrity validation failed"
            }
            DeviceGenerationCommitErrorClass::DependencyUnavailable => {
                "device generation commit dependency is unavailable"
            }
        })
    }
}

impl std::error::Error for DeviceGenerationCommitError {}

/// Read-only preparation boundary used by the authenticated Worker transport to derive the
/// optimistic profile version from the live base-generation pointer. The device never supplies
/// this version; the final D1 commit still rechecks it atomically.
pub trait DeviceGenerationProfileVersionPort {
    fn load_active_profile_version(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
    ) -> impl Future<Output = Result<Option<AggregateVersion>, DeviceGenerationCommitError>>;
}

/// Final metadata-only commit boundary for a verified immutable generation object.
///
/// Production implementations must revalidate the authenticated actor/device binding, exact
/// running job claim/fence/base generation, coordinator authority and profile version while the
/// catalog register/verify/activate mutation is serialized. Application-layer checks are only an
/// early fail-closed filter and never replace those commit-time authority checks.
pub trait DeviceGenerationCommitPort {
    fn commit_device_generation(
        &self,
        actor: &ActorContext,
        request: &DeviceGenerationCommitRequest,
    ) -> impl Future<Output = Result<DeviceGenerationCommitOutcome, DeviceGenerationCommitError>>;
}
