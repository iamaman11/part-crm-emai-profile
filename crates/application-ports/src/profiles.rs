use profile_domain::{BrowserProfile, ProfileGeneration};
use profile_platform_primitives::{ActorContext, GenerationId, ProfileId, TenantScope};

pub trait ProfileRepository {
    type Error;

    fn get_profile(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<BrowserProfile>, Self::Error>;

    fn get_generation(
        &self,
        scope: &TenantScope,
        generation_id: &GenerationId,
    ) -> Result<Option<ProfileGeneration>, Self::Error>;

    fn save_profile(
        &mut self,
        actor: &ActorContext,
        profile: &BrowserProfile,
    ) -> Result<(), Self::Error>;
}
