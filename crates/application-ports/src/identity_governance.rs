use crate::CommandExecutionEvidence;
use core::fmt;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, InvitationId, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityReplayReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl IdentityReplayReceipt {
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
pub enum IdentityReplayDecision {
    Miss,
    Replay(IdentityReplayReceipt),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityGovernancePortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentityGovernancePortError {
    class: IdentityGovernancePortErrorClass,
}

impl IdentityGovernancePortError {
    #[must_use]
    pub const fn new(class: IdentityGovernancePortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> IdentityGovernancePortErrorClass {
        self.class
    }
}

impl fmt::Display for IdentityGovernancePortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            IdentityGovernancePortErrorClass::NotFound => "identity governance resource not found",
            IdentityGovernancePortErrorClass::VersionConflict => {
                "identity governance version conflict"
            }
            IdentityGovernancePortErrorClass::InvalidState => "identity governance invalid state",
            IdentityGovernancePortErrorClass::Conflict => "identity governance conflict",
            IdentityGovernancePortErrorClass::IntegrityFailure => {
                "identity governance integrity failure"
            }
            IdentityGovernancePortErrorClass::InternalFailure => {
                "identity governance internal failure"
            }
            IdentityGovernancePortErrorClass::DependencyUnavailable => {
                "identity governance dependency unavailable"
            }
        })
    }
}

impl std::error::Error for IdentityGovernancePortError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerTransferWrite {
    next_owner_actor_id: ActorId,
    current_owner_version: AggregateVersion,
    next_owner_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl OwnerTransferWrite {
    #[must_use]
    pub fn new(
        next_owner_actor_id: ActorId,
        current_owner_version: AggregateVersion,
        next_owner_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            next_owner_actor_id,
            current_owner_version,
            next_owner_version,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn next_owner_actor_id(&self) -> &ActorId {
        &self.next_owner_actor_id
    }

    #[must_use]
    pub const fn current_owner_version(&self) -> AggregateVersion {
        self.current_owner_version
    }

    #[must_use]
    pub const fn next_owner_version(&self) -> AggregateVersion {
        self.next_owner_version
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
pub struct InvitationCreateWrite {
    invitation_id: InvitationId,
    invited_contact_hmac: String,
    expires_at: UnixMillis,
    tenant_expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl InvitationCreateWrite {
    #[must_use]
    pub fn new(
        invitation_id: InvitationId,
        invited_contact_hmac: impl Into<String>,
        expires_at: UnixMillis,
        tenant_expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            invitation_id,
            invited_contact_hmac: invited_contact_hmac.into(),
            expires_at,
            tenant_expected_version,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn invitation_id(&self) -> &InvitationId {
        &self.invitation_id
    }

    #[must_use]
    pub fn invited_contact_hmac(&self) -> &str {
        &self.invited_contact_hmac
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn tenant_expected_version(&self) -> AggregateVersion {
        self.tenant_expected_version
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipStatusTarget {
    Active,
    Suspended,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MembershipStatusWrite {
    target_actor_id: ActorId,
    expected_version: AggregateVersion,
    next_status: MembershipStatusTarget,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl MembershipStatusWrite {
    #[must_use]
    pub fn new(
        target_actor_id: ActorId,
        expected_version: AggregateVersion,
        next_status: MembershipStatusTarget,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            target_actor_id,
            expected_version,
            next_status,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn target_actor_id(&self) -> &ActorId {
        &self.target_actor_id
    }

    #[must_use]
    pub const fn expected_version(&self) -> AggregateVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn next_status(&self) -> MembershipStatusTarget {
        self.next_status
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

#[allow(async_fn_in_trait)]
pub trait ActiveOwnerGovernanceApplicationPort {
    async fn decide_identity_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IdentityReplayDecision, IdentityGovernancePortError>;

    async fn transfer_owner(
        &self,
        actor: &ActorContext,
        write: &OwnerTransferWrite,
    ) -> Result<(), IdentityGovernancePortError>;

    async fn create_invitation(
        &self,
        actor: &ActorContext,
        write: &InvitationCreateWrite,
    ) -> Result<(), IdentityGovernancePortError>;

    async fn update_membership_status(
        &self,
        actor: &ActorContext,
        write: &MembershipStatusWrite,
    ) -> Result<(), IdentityGovernancePortError>;
}
