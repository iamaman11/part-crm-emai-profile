use profile_domain::BrowserProfile;
use profile_platform_primitives::{ActorContext, ProfileId, TenantScope};

pub trait ProfileRepository {
    type Error;

    fn get_profile(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<BrowserProfile>, Self::Error>;

    fn save_profile(
        &mut self,
        actor: &ActorContext,
        profile: &BrowserProfile,
    ) -> Result<(), Self::Error>;
}
