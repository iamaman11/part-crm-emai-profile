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

    /// Clean cancellation is allowed only before this session could have started a browser,
    /// with exact own lease/fence and proven local lock cleanup. Implementations without a
    /// distinct evidenced release remain fail-closed through generic uncertain release.
    fn close_without_browser_use(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        self.close_lease(lease)
    }

    /// Release coordinator ownership after an exactly verified successor has already become the
    /// authoritative backend generation. Generic abort/error cleanup remains `close_lease`; a
    /// shipping coordinator may strengthen this operation to a clean release because confirmed
    /// save has removed the dirty-writer ambiguity.
    fn close_confirmed_save(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        self.close_lease(lease)
    }

    /// A separate *read-only historical browser probe* may cleanly release
    /// CURRENT ownership only after the original native current remained
    /// quiescent, an isolated browser passed independent live observation,
    /// the historical R2/current authority was revalidated after stopping,
    /// and BOTH local locks were released. No ordinary failure/revocation
    /// may invoke this path. Other coordinators default to Uncertain.
    fn close_verified_historical_probe(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        self.close_lease(lease)
    }
}

pub trait ProfileCoordinatorRuntimePort: ProfileCoordinatorPort {
    fn heartbeat_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error>;

    /// Narrow transport uncertainty, NOT a new lease, heartbeat success or
    /// evidence that the coordinator has not revoked this session. Only the
    /// concrete coordinator can classify its transport result. Default stays
    /// fail-closed for every other implementation.
    fn heartbeat_transport_uncertain(&self, _error: &Self::Error) -> bool {
        false
    }
}
