use crate::CommandExecutionEvidence;
use crate::identity_governance::{IdentityGovernancePortError, IdentityReplayDecision};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorId, CorrelationId, IdentityId, InvitationId, TenantScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedIdentitySnapshot {
    subject: String,
    contact_hint: Option<String>,
}

impl VerifiedIdentitySnapshot {
    #[must_use]
    pub fn new(subject: impl Into<String>, contact_hint: Option<String>) -> Self {
        Self {
            subject: subject.into(),
            contact_hint,
        }
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn contact_hint(&self) -> Option<&str> {
        self.contact_hint.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveIdentityBinding {
    actor_id: ActorId,
    role: MembershipRole,
}

impl ActiveIdentityBinding {
    #[must_use]
    pub const fn new(actor_id: ActorId, role: MembershipRole) -> Self {
        Self { actor_id, role }
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn role(&self) -> MembershipRole {
        self.role
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TenantIdentityBoundary {
    membership_count: u64,
    active_owner_count: u64,
}

impl TenantIdentityBoundary {
    #[must_use]
    pub const fn new(membership_count: u64, active_owner_count: u64) -> Self {
        Self {
            membership_count,
            active_owner_count,
        }
    }

    #[must_use]
    pub const fn membership_count(self) -> u64 {
        self.membership_count
    }

    #[must_use]
    pub const fn active_owner_count(self) -> u64 {
        self.active_owner_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedIdentityCeremonyContext {
    scope: TenantScope,
    actor_id: ActorId,
    correlation_id: CorrelationId,
    identity: VerifiedIdentitySnapshot,
}

impl VerifiedIdentityCeremonyContext {
    #[must_use]
    pub const fn new(
        scope: TenantScope,
        actor_id: ActorId,
        correlation_id: CorrelationId,
        identity: VerifiedIdentitySnapshot,
    ) -> Self {
        Self {
            scope,
            actor_id,
            correlation_id,
            identity,
        }
    }

    #[must_use]
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }

    #[must_use]
    pub const fn identity(&self) -> &VerifiedIdentitySnapshot {
        &self.identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapOwnerWrite {
    identity_id: IdentityId,
    tenant_display_name: String,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl BootstrapOwnerWrite {
    #[must_use]
    pub fn new(
        identity_id: IdentityId,
        tenant_display_name: impl Into<String>,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            identity_id,
            tenant_display_name: tenant_display_name.into(),
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn identity_id(&self) -> &IdentityId {
        &self.identity_id
    }

    #[must_use]
    pub fn tenant_display_name(&self) -> &str {
        &self.tenant_display_name
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
pub struct InvitationAcceptWrite {
    invitation_id: InvitationId,
    identity_id: IdentityId,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl InvitationAcceptWrite {
    #[must_use]
    pub fn new(
        invitation_id: InvitationId,
        identity_id: IdentityId,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            invitation_id,
            identity_id,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn invitation_id(&self) -> &InvitationId {
        &self.invitation_id
    }

    #[must_use]
    pub const fn identity_id(&self) -> &IdentityId {
        &self.identity_id
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
pub trait IdentityCeremonyApplicationPort {
    async fn find_active_identity_binding(
        &self,
        scope: &TenantScope,
        identity: &VerifiedIdentitySnapshot,
        correlation_id: &CorrelationId,
    ) -> Result<Option<ActiveIdentityBinding>, IdentityGovernancePortError>;

    async fn tenant_identity_boundary(
        &self,
        scope: &TenantScope,
    ) -> Result<TenantIdentityBoundary, IdentityGovernancePortError>;

    async fn decide_ceremony_replay(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IdentityReplayDecision, IdentityGovernancePortError>;

    async fn bootstrap_owner(
        &self,
        context: &VerifiedIdentityCeremonyContext,
        write: &BootstrapOwnerWrite,
    ) -> Result<(), IdentityGovernancePortError>;

    async fn accept_invitation(
        &self,
        context: &VerifiedIdentityCeremonyContext,
        write: &InvitationAcceptWrite,
    ) -> Result<(), IdentityGovernancePortError>;
}
