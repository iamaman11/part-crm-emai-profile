use core::fmt;
use identity_access_domain::{ClientGrant, Membership, MembershipRole, ProfileGrant};
use profile_platform_primitives::{ActorId, ClientId, ProfileId, TenantScope};

pub trait MembershipRepository {
    type Error;

    fn find_membership(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
    ) -> Result<Option<Membership>, Self::Error>;

    fn find_profile_grant(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileGrant>, Self::Error>;

    fn find_client_grant(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        client_id: &ClientId,
    ) -> Result<Option<ClientGrant>, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveMembershipPortErrorClass {
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveMembershipPortError {
    class: ActiveMembershipPortErrorClass,
}

impl ActiveMembershipPortError {
    #[must_use]
    pub const fn new(class: ActiveMembershipPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ActiveMembershipPortErrorClass {
        self.class
    }
}

impl fmt::Display for ActiveMembershipPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ActiveMembershipPortErrorClass::IntegrityFailure => {
                "active membership integrity failure"
            }
            ActiveMembershipPortErrorClass::DependencyUnavailable => {
                "active membership dependency unavailable"
            }
        })
    }
}

impl std::error::Error for ActiveMembershipPortError {}

/// Current membership evidence used by security-sensitive execution-time revalidation.
///
/// The identity bounded context remains the semantic owner of membership lifecycle and role
/// interpretation. Callers receive only the currently active role and must fail closed on `None`.
#[allow(async_fn_in_trait)]
pub trait ActiveMembershipPort {
    async fn active_membership_role(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
    ) -> Result<Option<MembershipRole>, ActiveMembershipPortError>;
}
