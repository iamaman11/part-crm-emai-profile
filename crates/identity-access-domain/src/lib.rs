#![forbid(unsafe_code)]

use profile_platform_primitives::{ActorContext, ActorId, ClientId, ProfileId, TenantId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipRole {
    TenantOwner,
    Member,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipStatus {
    Active,
    Suspended,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Membership {
    tenant_id: TenantId,
    actor_id: ActorId,
    role: MembershipRole,
    status: MembershipStatus,
}

impl Membership {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        actor_id: ActorId,
        role: MembershipRole,
        status: MembershipStatus,
    ) -> Self {
        Self {
            tenant_id,
            actor_id,
            role,
            status,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn role(&self) -> MembershipRole {
        self.role
    }

    #[must_use]
    pub const fn status(&self) -> MembershipStatus {
        self.status
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileGrantRole {
    Viewer,
    Operator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileGrant {
    tenant_id: TenantId,
    actor_id: ActorId,
    profile_id: ProfileId,
    role: ProfileGrantRole,
}

impl ProfileGrant {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        actor_id: ActorId,
        profile_id: ProfileId,
        role: ProfileGrantRole,
    ) -> Self {
        Self {
            tenant_id,
            actor_id,
            profile_id,
            role,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientGrantRole {
    Viewer,
    Editor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientGrant {
    tenant_id: TenantId,
    actor_id: ActorId,
    client_id: ClientId,
    role: ClientGrantRole,
}

impl ClientGrant {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        actor_id: ActorId,
        client_id: ClientId,
        role: ClientGrantRole,
    ) -> Self {
        Self {
            tenant_id,
            actor_id,
            client_id,
            role,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileCapability {
    View,
    Operate,
    Administer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientCapability {
    View,
    Edit,
    Administer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DenyReason {
    TenantMismatch,
    ActorMismatch,
    MembershipInactive,
    GrantMissing,
    CapabilityMissing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationDecision {
    Allowed,
    Denied(DenyReason),
}

#[must_use]
pub fn authorize_profile(
    actor: &ActorContext,
    membership: &Membership,
    profile_id: &ProfileId,
    grant: Option<&ProfileGrant>,
    capability: ProfileCapability,
) -> AuthorizationDecision {
    if actor.tenant_scope().tenant_id() != membership.tenant_id()
        || actor.tenant_scope().tenant_id() != &membership.tenant_id
    {
        return AuthorizationDecision::Denied(DenyReason::TenantMismatch);
    }

    if actor.actor_id() != membership.actor_id() {
        return AuthorizationDecision::Denied(DenyReason::ActorMismatch);
    }

    if membership.status() != MembershipStatus::Active {
        return AuthorizationDecision::Denied(DenyReason::MembershipInactive);
    }

    if membership.role() == MembershipRole::TenantOwner {
        return AuthorizationDecision::Allowed;
    }

    let Some(grant) = grant else {
        return AuthorizationDecision::Denied(DenyReason::GrantMissing);
    };

    if &grant.tenant_id != actor.tenant_scope().tenant_id() {
        return AuthorizationDecision::Denied(DenyReason::TenantMismatch);
    }
    if &grant.actor_id != actor.actor_id() {
        return AuthorizationDecision::Denied(DenyReason::ActorMismatch);
    }
    if &grant.profile_id != profile_id {
        return AuthorizationDecision::Denied(DenyReason::GrantMissing);
    }

    match (grant.role, capability) {
        (ProfileGrantRole::Viewer, ProfileCapability::View)
        | (ProfileGrantRole::Operator, ProfileCapability::View | ProfileCapability::Operate) => {
            AuthorizationDecision::Allowed
        }
        _ => AuthorizationDecision::Denied(DenyReason::CapabilityMissing),
    }
}

#[must_use]
pub fn authorize_client(
    actor: &ActorContext,
    membership: &Membership,
    client_id: &ClientId,
    grant: Option<&ClientGrant>,
    capability: ClientCapability,
) -> AuthorizationDecision {
    if actor.tenant_scope().tenant_id() != membership.tenant_id() {
        return AuthorizationDecision::Denied(DenyReason::TenantMismatch);
    }
    if actor.actor_id() != membership.actor_id() {
        return AuthorizationDecision::Denied(DenyReason::ActorMismatch);
    }
    if membership.status() != MembershipStatus::Active {
        return AuthorizationDecision::Denied(DenyReason::MembershipInactive);
    }
    if membership.role() == MembershipRole::TenantOwner {
        return AuthorizationDecision::Allowed;
    }

    let Some(grant) = grant else {
        return AuthorizationDecision::Denied(DenyReason::GrantMissing);
    };

    if &grant.tenant_id != actor.tenant_scope().tenant_id() {
        return AuthorizationDecision::Denied(DenyReason::TenantMismatch);
    }
    if &grant.actor_id != actor.actor_id() {
        return AuthorizationDecision::Denied(DenyReason::ActorMismatch);
    }
    if &grant.client_id != client_id {
        return AuthorizationDecision::Denied(DenyReason::GrantMissing);
    }

    match (grant.role, capability) {
        (ClientGrantRole::Viewer, ClientCapability::View)
        | (ClientGrantRole::Editor, ClientCapability::View | ClientCapability::Edit) => {
            AuthorizationDecision::Allowed
        }
        _ => AuthorizationDecision::Denied(DenyReason::CapabilityMissing),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthorizationDecision, DenyReason, Membership, MembershipRole, MembershipStatus,
        ProfileCapability, ProfileGrant, ProfileGrantRole, authorize_profile,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, ProfileId, TenantId, TenantScope,
    };

    fn fixture() -> Result<
        (ActorContext, Membership, ProfileId),
        Box<dyn std::error::Error>,
    > {
        let tenant_id = TenantId::parse("tenant_01JDOMAIN")?;
        let actor_id = ActorId::parse("actor_01JDOMAIN")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            actor_id.clone(),
            CorrelationId::parse("corr_01JDOMAIN")?,
        );
        let membership = Membership::new(
            tenant_id,
            actor_id,
            MembershipRole::Member,
            MembershipStatus::Active,
        );
        let profile_id = ProfileId::parse("profile_01JDOMAIN")?;
        Ok((actor, membership, profile_id))
    }

    #[test]
    fn default_deny_requires_explicit_profile_grant() -> Result<(), Box<dyn std::error::Error>> {
        let (actor, membership, profile_id) = fixture()?;
        assert_eq!(
            authorize_profile(
                &actor,
                &membership,
                &profile_id,
                None,
                ProfileCapability::View,
            ),
            AuthorizationDecision::Denied(DenyReason::GrantMissing)
        );
        Ok(())
    }

    #[test]
    fn viewer_cannot_operate_profile() -> Result<(), Box<dyn std::error::Error>> {
        let (actor, membership, profile_id) = fixture()?;
        let grant = ProfileGrant::new(
            actor.tenant_scope().tenant_id().clone(),
            actor.actor_id().clone(),
            profile_id.clone(),
            ProfileGrantRole::Viewer,
        );
        assert_eq!(
            authorize_profile(
                &actor,
                &membership,
                &profile_id,
                Some(&grant),
                ProfileCapability::Operate,
            ),
            AuthorizationDecision::Denied(DenyReason::CapabilityMissing)
        );
        Ok(())
    }

    #[test]
    fn cross_tenant_grant_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let (actor, membership, profile_id) = fixture()?;
        let grant = ProfileGrant::new(
            TenantId::parse("tenant_02JDOMAIN")?,
            actor.actor_id().clone(),
            profile_id.clone(),
            ProfileGrantRole::Operator,
        );
        assert_eq!(
            authorize_profile(
                &actor,
                &membership,
                &profile_id,
                Some(&grant),
                ProfileCapability::Operate,
            ),
            AuthorizationDecision::Denied(DenyReason::TenantMismatch)
        );
        Ok(())
    }
}
