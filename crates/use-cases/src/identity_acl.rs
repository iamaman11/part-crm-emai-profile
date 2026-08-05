use crate::ApplicationError;
use client_domain::{ClientKind, ClientRecord, ProfileClientAssignment};
use contracts::ProblemCode;
use identity_access_domain::lifecycle::require_active_owner;
use identity_access_domain::{
    AuthorizationDecision, ClientCapability, ClientGrant, ClientGrantRole, Membership,
    MembershipRole, MembershipStatus, ProfileCapability, ProfileGrant, ProfileGrantRole,
    authorize_client, authorize_profile,
};
use profile_domain::BrowserProfile;
use profile_platform_primitives::{
    ActorContext, ActorId, ClientId, ProfileId, TenantId, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientView {
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
}

impl ClientView {
    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn kind(&self) -> ClientKind {
        self.kind
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileView {
    profile_id: ProfileId,
    linked_client_id: Option<ClientId>,
}

impl ProfileView {
    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn linked_client_id(&self) -> Option<&ClientId> {
        self.linked_client_id.as_ref()
    }
}

pub fn query_client(
    actor: &ActorContext,
    membership: Option<&Membership>,
    grant: Option<&ClientGrant>,
    requested_client_id: &ClientId,
    client: Option<&ClientRecord>,
) -> Result<ClientView, ApplicationError> {
    let Some(membership) = membership else {
        return Err(neutral_not_found());
    };
    let Some(client) = client else {
        return Err(neutral_not_found());
    };
    if client.tenant_id() != actor.tenant_scope().tenant_id()
        || client.client_id() != requested_client_id
        || authorize_client(
            actor,
            membership,
            requested_client_id,
            grant,
            ClientCapability::View,
        ) != AuthorizationDecision::Allowed
    {
        return Err(neutral_not_found());
    }

    Ok(ClientView {
        client_id: client.client_id().clone(),
        kind: client.kind(),
        display_name: client.display_name().to_owned(),
    })
}

pub fn query_profile(
    actor: &ActorContext,
    membership: Option<&Membership>,
    grant: Option<&ProfileGrant>,
    requested_profile_id: &ProfileId,
    profile: Option<&BrowserProfile>,
    active_assignment: Option<&ProfileClientAssignment>,
) -> Result<ProfileView, ApplicationError> {
    let Some(membership) = membership else {
        return Err(neutral_not_found());
    };
    let Some(profile) = profile else {
        return Err(neutral_not_found());
    };
    if profile.tenant_id() != actor.tenant_scope().tenant_id()
        || profile.profile_id() != requested_profile_id
        || authorize_profile(
            actor,
            membership,
            requested_profile_id,
            grant,
            ProfileCapability::View,
        ) != AuthorizationDecision::Allowed
    {
        return Err(neutral_not_found());
    }

    let linked_client_id = active_assignment
        .filter(|assignment| {
            assignment.tenant_id() == actor.tenant_scope().tenant_id()
                && assignment.profile_id() == requested_profile_id
        })
        .map(|assignment| assignment.client_id().clone());

    Ok(ProfileView {
        profile_id: profile.profile_id().clone(),
        linked_client_id,
    })
}

pub fn decide_create_profile(
    actor: &ActorContext,
    owner_membership: &Membership,
    tenant_id: TenantId,
    profile_id: ProfileId,
) -> Result<BrowserProfile, ApplicationError> {
    require_active_owner(actor, owner_membership)
        .map_err(|_| ApplicationError::new(ProblemCode::Forbidden))?;
    if actor.tenant_scope().tenant_id() != &tenant_id {
        return Err(neutral_not_found());
    }
    Ok(BrowserProfile::create(tenant_id, profile_id))
}

pub fn decide_assign_profile_to_client(
    actor: &ActorContext,
    owner_membership: &Membership,
    profile: &BrowserProfile,
    client: &ClientRecord,
    now: UnixMillis,
    reason: impl Into<String>,
) -> Result<ProfileClientAssignment, ApplicationError> {
    require_active_owner(actor, owner_membership)
        .map_err(|_| ApplicationError::new(ProblemCode::Forbidden))?;
    if profile.tenant_id() != actor.tenant_scope().tenant_id()
        || client.tenant_id() != actor.tenant_scope().tenant_id()
    {
        return Err(neutral_not_found());
    }

    ProfileClientAssignment::assign(
        profile.tenant_id(),
        profile.profile_id().clone(),
        client,
        actor.actor_id().clone(),
        now,
        reason,
    )
    .map_err(|_| ApplicationError::new(ProblemCode::InvalidState))
}

pub fn decide_profile_grant(
    actor: &ActorContext,
    owner_membership: &Membership,
    target_membership: &Membership,
    profile: &BrowserProfile,
    role: ProfileGrantRole,
) -> Result<ProfileGrant, ApplicationError> {
    require_active_owner(actor, owner_membership)
        .map_err(|_| ApplicationError::new(ProblemCode::Forbidden))?;
    require_grant_target(actor, target_membership)?;
    if profile.tenant_id() != actor.tenant_scope().tenant_id() {
        return Err(neutral_not_found());
    }

    Ok(ProfileGrant::new(
        actor.tenant_scope().tenant_id().clone(),
        target_membership.actor_id().clone(),
        profile.profile_id().clone(),
        role,
    ))
}

pub fn decide_client_grant(
    actor: &ActorContext,
    owner_membership: &Membership,
    target_membership: &Membership,
    client: &ClientRecord,
    role: ClientGrantRole,
) -> Result<ClientGrant, ApplicationError> {
    require_active_owner(actor, owner_membership)
        .map_err(|_| ApplicationError::new(ProblemCode::Forbidden))?;
    require_grant_target(actor, target_membership)?;
    if client.tenant_id() != actor.tenant_scope().tenant_id() {
        return Err(neutral_not_found());
    }

    Ok(ClientGrant::new(
        actor.tenant_scope().tenant_id().clone(),
        target_membership.actor_id().clone(),
        client.client_id().clone(),
        role,
    ))
}

pub fn decide_revoke_profile_grant(
    actor: &ActorContext,
    owner_membership: &Membership,
    existing_grant: Option<&ProfileGrant>,
    target_actor_id: &ActorId,
    profile_id: &ProfileId,
) -> Result<(), ApplicationError> {
    require_active_owner(actor, owner_membership)
        .map_err(|_| ApplicationError::new(ProblemCode::Forbidden))?;
    let Some(grant) = existing_grant else {
        return Err(neutral_not_found());
    };
    if grant.tenant_id() != actor.tenant_scope().tenant_id()
        || grant.actor_id() != target_actor_id
        || grant.profile_id() != profile_id
    {
        return Err(neutral_not_found());
    }
    Ok(())
}

pub fn decide_revoke_client_grant(
    actor: &ActorContext,
    owner_membership: &Membership,
    existing_grant: Option<&ClientGrant>,
    target_actor_id: &ActorId,
    client_id: &ClientId,
) -> Result<(), ApplicationError> {
    require_active_owner(actor, owner_membership)
        .map_err(|_| ApplicationError::new(ProblemCode::Forbidden))?;
    let Some(grant) = existing_grant else {
        return Err(neutral_not_found());
    };
    if grant.tenant_id() != actor.tenant_scope().tenant_id()
        || grant.actor_id() != target_actor_id
        || grant.client_id() != client_id
    {
        return Err(neutral_not_found());
    }
    Ok(())
}

fn require_grant_target(
    actor: &ActorContext,
    target_membership: &Membership,
) -> Result<(), ApplicationError> {
    if target_membership.tenant_id() != actor.tenant_scope().tenant_id()
        || target_membership.role() != MembershipRole::Member
        || target_membership.status() != MembershipStatus::Active
    {
        return Err(neutral_not_found());
    }
    Ok(())
}

const fn neutral_not_found() -> ApplicationError {
    ApplicationError::new(ProblemCode::NotFound)
}

#[cfg(test)]
mod tests {
    use super::{
        decide_assign_profile_to_client, decide_client_grant, decide_profile_grant, query_client,
        query_profile,
    };
    use crate::ApplicationError;
    use client_domain::{ClientKind, ClientRecord};
    use contracts::ProblemCode;
    use identity_access_domain::{
        ClientGrantRole, Membership, MembershipRole, MembershipStatus, ProfileGrantRole,
    };
    use profile_domain::BrowserProfile;
    use profile_platform_primitives::{
        ActorContext, ActorId, ClientId, CorrelationId, ProfileId, TenantId, TenantScope,
        UnixMillis,
    };

    struct Fixture {
        owner_actor: ActorContext,
        member_actor: ActorContext,
        owner_membership: Membership,
        member_membership: Membership,
        client: ClientRecord,
        profile: BrowserProfile,
    }

    fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JACLTEST")?;
        let owner_id = ActorId::parse("actor_01JACLOWNER")?;
        let member_id = ActorId::parse("actor_01JACLMEMBER")?;
        let owner_actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            owner_id.clone(),
            CorrelationId::parse("corr_01JACLOWNER")?,
        );
        let member_actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            member_id.clone(),
            CorrelationId::parse("corr_01JACLMEMBER")?,
        );
        let owner_membership = Membership::new(
            tenant_id.clone(),
            owner_id,
            MembershipRole::TenantOwner,
            MembershipStatus::Active,
        );
        let member_membership = Membership::new(
            tenant_id.clone(),
            member_id,
            MembershipRole::Member,
            MembershipStatus::Active,
        );
        let client = ClientRecord::create(
            tenant_id.clone(),
            ClientId::parse("client_01JACLTEST")?,
            ClientKind::Person,
            "Synthetic ACL Client",
        )?;
        let profile = BrowserProfile::create(
            tenant_id,
            ProfileId::parse("profile_01JACLTEST")?,
        );
        Ok(Fixture {
            owner_actor,
            member_actor,
            owner_membership,
            member_membership,
            client,
            profile,
        })
    }

    #[test]
    fn owner_and_explicitly_granted_member_complete_client_and_profile_queries()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let client_grant = decide_client_grant(
            &fixture.owner_actor,
            &fixture.owner_membership,
            &fixture.member_membership,
            &fixture.client,
            ClientGrantRole::Viewer,
        )?;
        let profile_grant = decide_profile_grant(
            &fixture.owner_actor,
            &fixture.owner_membership,
            &fixture.member_membership,
            &fixture.profile,
            ProfileGrantRole::Viewer,
        )?;

        assert_eq!(
            query_client(
                &fixture.member_actor,
                Some(&fixture.member_membership),
                Some(&client_grant),
                fixture.client.client_id(),
                Some(&fixture.client),
            )?
            .client_id(),
            fixture.client.client_id()
        );
        assert_eq!(
            query_profile(
                &fixture.member_actor,
                Some(&fixture.member_membership),
                Some(&profile_grant),
                fixture.profile.profile_id(),
                Some(&fixture.profile),
                None,
            )?
            .profile_id(),
            fixture.profile.profile_id()
        );
        Ok(())
    }

    #[test]
    fn assignment_never_grants_client_or_profile_access()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let assignment = decide_assign_profile_to_client(
            &fixture.owner_actor,
            &fixture.owner_membership,
            &fixture.profile,
            &fixture.client,
            UnixMillis::new(100),
            "historical association only",
        )?;

        assert_eq!(
            query_profile(
                &fixture.member_actor,
                Some(&fixture.member_membership),
                None,
                fixture.profile.profile_id(),
                Some(&fixture.profile),
                Some(&assignment),
            ),
            Err(ApplicationError::new(ProblemCode::NotFound))
        );
        assert_eq!(
            query_client(
                &fixture.member_actor,
                Some(&fixture.member_membership),
                None,
                fixture.client.client_id(),
                Some(&fixture.client),
            ),
            Err(ApplicationError::new(ProblemCode::NotFound))
        );
        Ok(())
    }

    #[test]
    fn missing_membership_suspended_revoked_and_foreign_resource_are_neutral()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let expected = Err(ApplicationError::new(ProblemCode::NotFound));
        assert_eq!(
            query_client(
                &fixture.member_actor,
                None,
                None,
                fixture.client.client_id(),
                Some(&fixture.client),
            ),
            expected
        );

        for status in [MembershipStatus::Suspended, MembershipStatus::Revoked] {
            let inactive = Membership::new(
                fixture.member_actor.tenant_scope().tenant_id().clone(),
                fixture.member_actor.actor_id().clone(),
                MembershipRole::Member,
                status,
            );
            assert_eq!(
                query_profile(
                    &fixture.member_actor,
                    Some(&inactive),
                    None,
                    fixture.profile.profile_id(),
                    Some(&fixture.profile),
                    None,
                ),
                Err(ApplicationError::new(ProblemCode::NotFound))
            );
        }

        let foreign_client = ClientRecord::create(
            TenantId::parse("tenant_02JACLTEST")?,
            fixture.client.client_id().clone(),
            ClientKind::Person,
            "Foreign Synthetic Client",
        )?;
        assert_eq!(
            query_client(
                &fixture.member_actor,
                Some(&fixture.member_membership),
                None,
                fixture.client.client_id(),
                Some(&foreign_client),
            ),
            query_client(
                &fixture.member_actor,
                Some(&fixture.member_membership),
                None,
                fixture.client.client_id(),
                None,
            )
        );
        Ok(())
    }
}
