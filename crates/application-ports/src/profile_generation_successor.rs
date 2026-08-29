use crate::generation_objects::GenerationObjectDescriptor;
use core::{fmt, future::Future};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, FencingToken, GenerationId, ProfileId, SessionId,
    UnixMillis,
};

/// Exact coordinator witness carried across the final successor commit boundary.
///
/// The Profile coordinator remains authoritative. Persistence may keep only a digest of the
/// fencing token; callers must revalidate the raw token against the live coordinator before
/// invoking the catalog commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileGenerationCommitWitness {
    session_id: SessionId,
    fencing_token: FencingToken,
    epoch: u64,
    coordinator_version: u64,
    coordinator_sequence: u64,
}

impl ProfileGenerationCommitWitness {
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

/// Read-only proof request for the exact interactive writer currently owning a Profile session.
/// The device comes from the authenticated Bridge machine perimeter; the actor comes from that
/// machine's D1 binding. No coordinator version or client clock is accepted from the machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileGenerationWriterAuthorityRequest {
    device_id: DeviceId,
    profile_id: ProfileId,
    session_id: SessionId,
    fencing_token: FencingToken,
    epoch: u64,
}

impl ProfileGenerationWriterAuthorityRequest {
    #[must_use]
    pub const fn new(
        device_id: DeviceId,
        profile_id: ProfileId,
        session_id: SessionId,
        fencing_token: FencingToken,
        epoch: u64,
    ) -> Self {
        Self {
            device_id,
            profile_id,
            session_id,
            fencing_token,
            epoch,
        }
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
}

/// Server-owned coordinator version/sequence proven against the raw interactive writer witness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileGenerationWriterAuthority {
    coordinator_version: u64,
    coordinator_sequence: u64,
}

impl ProfileGenerationWriterAuthority {
    #[must_use]
    pub const fn new(coordinator_version: u64, coordinator_sequence: u64) -> Self {
        Self {
            coordinator_version,
            coordinator_sequence,
        }
    }

    #[must_use]
    pub const fn coordinator_version(self) -> u64 {
        self.coordinator_version
    }

    #[must_use]
    pub const fn coordinator_sequence(self) -> u64 {
        self.coordinator_sequence
    }
}

/// Metadata-only request for the one atomic `N -> verified active N+1` transition.
///
/// The encrypted object must already exist and have passed exact object verification. This request
/// intentionally contains no device-job identity: job-driven execution is an authority adapter to
/// this lifecycle, not the owner of Profile generation semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileGenerationSuccessorCommitRequest {
    device_id: DeviceId,
    profile_id: ProfileId,
    base_generation_id: GenerationId,
    object: GenerationObjectDescriptor,
    expected_profile_version: AggregateVersion,
    coordinator: ProfileGenerationCommitWitness,
    observed_at: UnixMillis,
}

impl ProfileGenerationSuccessorCommitRequest {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        device_id: DeviceId,
        profile_id: ProfileId,
        base_generation_id: GenerationId,
        object: GenerationObjectDescriptor,
        expected_profile_version: AggregateVersion,
        coordinator: ProfileGenerationCommitWitness,
        observed_at: UnixMillis,
    ) -> Self {
        Self {
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
    pub const fn coordinator(&self) -> &ProfileGenerationCommitWitness {
        &self.coordinator
    }

    #[must_use]
    pub const fn observed_at(&self) -> UnixMillis {
        self.observed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileGenerationSuccessorCommitOutcome {
    Activated,
    AlreadyActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileGenerationSuccessorCommitErrorClass {
    StaleAuthority,
    VersionConflict,
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileGenerationSuccessorCommitError {
    class: ProfileGenerationSuccessorCommitErrorClass,
}

impl ProfileGenerationSuccessorCommitError {
    #[must_use]
    pub const fn new(class: ProfileGenerationSuccessorCommitErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ProfileGenerationSuccessorCommitErrorClass {
        self.class
    }
}

impl fmt::Display for ProfileGenerationSuccessorCommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ProfileGenerationSuccessorCommitErrorClass::StaleAuthority => {
                "profile generation successor authority is stale"
            }
            ProfileGenerationSuccessorCommitErrorClass::VersionConflict => {
                "profile generation successor lost an optimistic concurrency race"
            }
            ProfileGenerationSuccessorCommitErrorClass::IntegrityFailure => {
                "profile generation successor integrity validation failed"
            }
            ProfileGenerationSuccessorCommitErrorClass::DependencyUnavailable => {
                "profile generation successor dependency is unavailable"
            }
        })
    }
}

impl std::error::Error for ProfileGenerationSuccessorCommitError {}

/// Read-only preparation boundary for interactive save. It derives the predecessor Profile version
/// from server state and accepts either the exact live base or the exact candidate already active
/// after a lost-response replay. Callers never provide an optimistic Profile version.
pub trait ProfileGenerationSuccessorVersionPort {
    fn load_successor_expected_profile_version(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        candidate_generation_id: &GenerationId,
    ) -> impl Future<Output = Result<Option<AggregateVersion>, ProfileGenerationSuccessorCommitError>>;
}

/// Read-only Profile Coordinator proof for an interactive save writer. Implementations must prove
/// the exact Claim actor/device/session provenance plus raw fencing token, epoch and live lease using
/// a server-owned timestamp. A positive proof returns only the authoritative version/sequence that
/// the final commit must bind.
pub trait ProfileGenerationWriterAuthorityPort {
    fn prove_profile_generation_writer_authority(
        &self,
        actor: &ActorContext,
        request: &ProfileGenerationWriterAuthorityRequest,
    ) -> impl Future<
        Output = Result<ProfileGenerationWriterAuthority, ProfileGenerationSuccessorCommitError>,
    >;
}

/// Single catalog lifecycle owner for an already-uploaded and exactly verified successor.
///
/// Implementations must atomically register the immutable generation, mark it verified and move the
/// Profile active pointer from the exact expected base to that successor. They must recheck all
/// durable authority witnesses at commit time. A job-driven caller may couple additional job state
/// to this operation, but must not duplicate the generation transition itself.
pub trait ProfileGenerationSuccessorCommitPort {
    fn commit_profile_generation_successor(
        &self,
        actor: &ActorContext,
        request: &ProfileGenerationSuccessorCommitRequest,
    ) -> impl Future<
        Output = Result<
            ProfileGenerationSuccessorCommitOutcome,
            ProfileGenerationSuccessorCommitError,
        >,
    >;
}
