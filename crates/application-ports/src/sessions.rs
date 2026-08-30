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

    /// Release coordinator ownership after an exactly verified successor has already become the
    /// authoritative backend generation. Generic abort/error cleanup remains `close_lease`; a
    /// shipping coordinator may strengthen this operation to a clean release because confirmed
    /// save has removed the dirty-writer ambiguity.
    fn close_confirmed_save(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        self.close_lease(lease)
    }
}

pub trait ProfileCoordinatorRuntimePort: ProfileCoordinatorPort {
    fn heartbeat_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error>;
}
