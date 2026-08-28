use profile_platform_primitives::{ActorContext, DeviceId, LaunchIntentId, ProfileId};
use session_domain::ProfileLease;

pub trait ProfileCoordinatorPort {
    type Error;

    fn claim_launch_intent(
        &mut self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        device_id: &DeviceId,
        launch_intent_id: &LaunchIntentId,
    ) -> Result<ProfileLease, Self::Error>;

    fn close_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error>;
}
