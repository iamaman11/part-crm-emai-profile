#![forbid(unsafe_code)]

pub mod identity_acl;

use client_domain::{ClientError, ClientKind, ClientRecord};
use contracts::ProblemCode;
use core::fmt;
use identity_access_domain::{
    AuthorizationDecision, Membership, ProfileCapability, ProfileGrant, authorize_profile,
};
use profile_domain::{BrowserProfile, ProfileStatus};
use profile_platform_primitives::{ActorContext, ClientId, DeviceId, ProfileId, TenantId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenProfileCommand {
    profile_id: ProfileId,
    device_id: DeviceId,
}

impl OpenProfileCommand {
    #[must_use]
    pub const fn new(profile_id: ProfileId, device_id: DeviceId) -> Self {
        Self {
            profile_id,
            device_id,
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenProfileDecision {
    profile_id: ProfileId,
    device_id: DeviceId,
}

impl OpenProfileDecision {
    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

pub fn decide_open_profile(
    actor: &ActorContext,
    membership: &Membership,
    grant: Option<&ProfileGrant>,
    profile: &BrowserProfile,
    command: OpenProfileCommand,
) -> Result<OpenProfileDecision, ApplicationError> {
    if actor.tenant_scope().tenant_id() != profile.tenant_id()
        || command.profile_id() != profile.profile_id()
    {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    if authorize_profile(
        actor,
        membership,
        profile.profile_id(),
        grant,
        ProfileCapability::Operate,
    ) != AuthorizationDecision::Allowed
    {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    if profile.status() != ProfileStatus::Ready || profile.active_generation_id().is_none() {
        return Err(ApplicationError::new(ProblemCode::InvalidState));
    }

    Ok(OpenProfileDecision {
        profile_id: command.profile_id,
        device_id: command.device_id,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateClientCommand {
    tenant_id: TenantId,
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
}

impl CreateClientCommand {
    #[must_use]
    pub fn new(
        tenant_id: TenantId,
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            client_id,
            kind,
            display_name: display_name.into(),
        }
    }
}

pub fn decide_create_client(
    actor: &ActorContext,
    command: CreateClientCommand,
) -> Result<ClientRecord, ApplicationError> {
    if actor.tenant_scope().tenant_id() != &command.tenant_id {
        return Err(ApplicationError::new(ProblemCode::Forbidden));
    }

    ClientRecord::create(
        command.tenant_id,
        command.client_id,
        command.kind,
        command.display_name,
    )
    .map_err(map_client_error)
}

fn map_client_error(error: ClientError) -> ApplicationError {
    match error {
        ClientError::InvalidDisplayName => ApplicationError::new(ProblemCode::InvalidRequest),
        ClientError::InvalidStatusTransition => ApplicationError::new(ProblemCode::InvalidState),
        ClientError::VersionOverflow => ApplicationError::new(ProblemCode::InternalFailure),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationError {
    code: ProblemCode,
}

impl ApplicationError {
    #[must_use]
    pub const fn new(code: ProblemCode) -> Self {
        Self { code }
    }

    #[must_use]
    pub const fn code(self) -> ProblemCode {
        self.code
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.stable_code())
    }
}

impl std::error::Error for ApplicationError {}

#[cfg(test)]
mod tests {
    use super::{ApplicationError, OpenProfileCommand, decide_open_profile};
    use contracts::ProblemCode;
    use identity_access_domain::{
        Membership, MembershipRole, MembershipStatus, ProfileGrant, ProfileGrantRole,
    };
    use profile_domain::{
        BrowserProfile, GenerationVerification, ProfileGeneration, ProfileStatus,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, DeviceId, GenerationId, ProfileId, TenantId,
        TenantScope,
    };

    struct Fixture {
        actor: ActorContext,
        membership: Membership,
        grant: ProfileGrant,
        profile: BrowserProfile,
    }

    fn fixture(role: ProfileGrantRole) -> Result<Fixture, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JUSECASE")?;
        let actor_id = ActorId::parse("actor_01JUSECASE")?;
        let profile_id = ProfileId::parse("profile_01JUSECASE")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            actor_id.clone(),
            CorrelationId::parse("corr_01JUSECASE")?,
        );
        let membership = Membership::new(
            tenant_id.clone(),
            actor_id.clone(),
            MembershipRole::Member,
            MembershipStatus::Active,
        );
        let grant = ProfileGrant::new(tenant_id.clone(), actor_id, profile_id.clone(), role);
        let mut profile = BrowserProfile::create(tenant_id.clone(), profile_id.clone());
        let generation = ProfileGeneration::new(
            tenant_id,
            profile_id,
            GenerationId::parse("generation_01JUSECASE")?,
            GenerationVerification::Verified,
        );
        profile.activate_generation(&generation)?;
        Ok(Fixture {
            actor,
            membership,
            grant,
            profile,
        })
    }

    #[test]
    fn operator_can_open_profile_with_verified_active_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture(ProfileGrantRole::Operator)?;
        let decision = decide_open_profile(
            &fixture.actor,
            &fixture.membership,
            Some(&fixture.grant),
            &fixture.profile,
            OpenProfileCommand::new(
                fixture.profile.profile_id().clone(),
                DeviceId::parse("device_01JUSECASE")?,
            ),
        )?;
        assert_eq!(decision.profile_id(), fixture.profile.profile_id());
        assert!(fixture.profile.active_generation_id().is_some());
        Ok(())
    }

    #[test]
    fn viewer_cannot_open_profile() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture(ProfileGrantRole::Viewer)?;
        assert_eq!(
            decide_open_profile(
                &fixture.actor,
                &fixture.membership,
                Some(&fixture.grant),
                &fixture.profile,
                OpenProfileCommand::new(
                    fixture.profile.profile_id().clone(),
                    DeviceId::parse("device_01JUSECASE")?,
                ),
            ),
            Err(ApplicationError::new(ProblemCode::NotFound))
        );
        Ok(())
    }

    #[test]
    fn profile_without_active_generation_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture(ProfileGrantRole::Operator)?;
        let draft = BrowserProfile::create(
            fixture.profile.tenant_id().clone(),
            fixture.profile.profile_id().clone(),
        );
        assert_eq!(
            decide_open_profile(
                &fixture.actor,
                &fixture.membership,
                Some(&fixture.grant),
                &draft,
                OpenProfileCommand::new(
                    draft.profile_id().clone(),
                    DeviceId::parse("device_01JUSECASE")?,
                ),
            ),
            Err(ApplicationError::new(ProblemCode::InvalidState))
        );
        Ok(())
    }

    #[test]
    fn non_ready_profile_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = fixture(ProfileGrantRole::Operator)?;
        fixture.profile.transition(ProfileStatus::InUse)?;
        assert_eq!(
            decide_open_profile(
                &fixture.actor,
                &fixture.membership,
                Some(&fixture.grant),
                &fixture.profile,
                OpenProfileCommand::new(
                    fixture.profile.profile_id().clone(),
                    DeviceId::parse("device_01JUSECASE")?,
                ),
            ),
            Err(ApplicationError::new(ProblemCode::InvalidState))
        );
        Ok(())
    }
}
