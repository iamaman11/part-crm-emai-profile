use crate::commands::CommandExecutionEvidence;
use core::fmt;
use identity_access_domain::MembershipRole;
pub use profile_domain::ProfileStatus;
use profile_domain::{BrowserProfile, ProfileGeneration};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, AssignmentId, ClientId, GenerationId, ProfileId,
    TenantScope,
};

pub trait ProfileRepository {
    type Error;

    fn get_profile(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<BrowserProfile>, Self::Error>;

    fn get_generation(
        &self,
        scope: &TenantScope,
        generation_id: &GenerationId,
    ) -> Result<Option<ProfileGeneration>, Self::Error>;

    fn save_profile(
        &mut self,
        actor: &ActorContext,
        profile: &BrowserProfile,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileCreateWrite {
    profile: BrowserProfile,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl ProfileCreateWrite {
    #[must_use]
    pub fn new(
        profile: BrowserProfile,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            profile,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn profile(&self) -> &BrowserProfile {
        &self.profile
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileAssignmentWrite {
    assignment_id: AssignmentId,
    profile_id: ProfileId,
    client_id: ClientId,
    expected_profile_version: AggregateVersion,
    reason: String,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl ProfileAssignmentWrite {
    #[must_use]
    pub fn new(
        assignment_id: AssignmentId,
        profile_id: ProfileId,
        client_id: ClientId,
        expected_profile_version: AggregateVersion,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            assignment_id,
            profile_id,
            client_id,
            expected_profile_version,
            reason: reason.into(),
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn assignment_id(&self) -> &AssignmentId {
        &self.assignment_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn expected_profile_version(&self) -> AggregateVersion {
        self.expected_profile_version
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileReplayReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl ProfileReplayReceipt {
    #[must_use]
    pub fn new(result_code: impl Into<String>, result_reference: Option<String>) -> Self {
        Self {
            result_code: result_code.into(),
            result_reference,
        }
    }

    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn result_reference(&self) -> Option<&str> {
        self.result_reference.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileReplayDecision {
    Miss,
    Replay(ProfileReplayReceipt),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfilePortErrorClass {
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfilePortError {
    class: ProfilePortErrorClass,
}

impl ProfilePortError {
    #[must_use]
    pub const fn new(class: ProfilePortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ProfilePortErrorClass {
        self.class
    }
}

impl fmt::Display for ProfilePortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ProfilePortErrorClass::Conflict => "profile port conflict",
            ProfilePortErrorClass::IntegrityFailure => "profile port integrity failure",
            ProfilePortErrorClass::InternalFailure => "profile port internal failure",
            ProfilePortErrorClass::DependencyUnavailable => "profile port dependency unavailable",
        })
    }
}

impl std::error::Error for ProfilePortError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileAssignmentPortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileAssignmentPortError {
    class: ProfileAssignmentPortErrorClass,
}

impl ProfileAssignmentPortError {
    #[must_use]
    pub const fn new(class: ProfileAssignmentPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ProfileAssignmentPortErrorClass {
        self.class
    }
}

impl fmt::Display for ProfileAssignmentPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ProfileAssignmentPortErrorClass::NotFound => "profile assignment not found",
            ProfileAssignmentPortErrorClass::VersionConflict => {
                "profile assignment version conflict"
            }
            ProfileAssignmentPortErrorClass::InvalidState => "profile assignment invalid state",
            ProfileAssignmentPortErrorClass::Conflict => "profile assignment conflict",
            ProfileAssignmentPortErrorClass::IntegrityFailure => {
                "profile assignment integrity failure"
            }
            ProfileAssignmentPortErrorClass::InternalFailure => {
                "profile assignment internal failure"
            }
            ProfileAssignmentPortErrorClass::DependencyUnavailable => {
                "profile assignment dependency unavailable"
            }
        })
    }
}

impl std::error::Error for ProfileAssignmentPortError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileReadModel {
    profile_id: ProfileId,
    status: ProfileStatus,
    version: AggregateVersion,
    linked_client_id: Option<ClientId>,
}

impl ProfileReadModel {
    #[must_use]
    pub const fn new(
        profile_id: ProfileId,
        status: ProfileStatus,
        version: AggregateVersion,
        linked_client_id: Option<ClientId>,
    ) -> Self {
        Self {
            profile_id,
            status,
            version,
            linked_client_id,
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn status(&self) -> ProfileStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn linked_client_id(&self) -> Option<&ClientId> {
        self.linked_client_id.as_ref()
    }
}

#[allow(async_fn_in_trait)]
pub trait ProfileApplicationPort {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ProfileReplayDecision, ProfilePortError>;

    async fn create_profile(
        &self,
        actor: &ActorContext,
        write: &ProfileCreateWrite,
    ) -> Result<(), ProfilePortError>;

    async fn find_visible_profile(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileReadModel>, ProfilePortError>;
}

#[allow(async_fn_in_trait)]
pub trait ProfileAssignmentApplicationPort {
    async fn decide_assignment_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ProfileReplayDecision, ProfileAssignmentPortError>;

    async fn assign_profile(
        &self,
        actor: &ActorContext,
        write: &ProfileAssignmentWrite,
    ) -> Result<(), ProfileAssignmentPortError>;
}
