use profile_platform_primitives::{ActorContext, DeviceId, ProfileId};
use session_domain::ProfileLease;

pub trait ProfileCoordinatorPort {
    type Error;

    fn acquire_lease(
        &mut self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        device_id: &DeviceId,
    ) -> Result<ProfileLease, Self::Error>;

    fn close_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error>;
}
