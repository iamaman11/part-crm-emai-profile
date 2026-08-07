use identity_access_domain::{ClientGrant, Membership, ProfileGrant};
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
