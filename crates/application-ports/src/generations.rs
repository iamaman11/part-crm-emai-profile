use profile_domain::ProfileGeneration;
use profile_platform_primitives::{ActorContext, GenerationId, TenantScope};

pub trait ProfileGenerationRepository {
    type Error;

    fn get_generation(
        &self,
        scope: &TenantScope,
        generation_id: &GenerationId,
    ) -> Result<Option<ProfileGeneration>, Self::Error>;

    fn save_generation(
        &mut self,
        actor: &ActorContext,
        generation: &ProfileGeneration,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationObjectReference {
    generation_id: GenerationId,
    ciphertext_digest: String,
}

impl GenerationObjectReference {
    #[must_use]
    pub fn new(generation_id: GenerationId, ciphertext_digest: impl Into<String>) -> Self {
        Self {
            generation_id,
            ciphertext_digest: ciphertext_digest.into(),
        }
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn ciphertext_digest(&self) -> &str {
        &self.ciphertext_digest
    }
}

pub trait GenerationObjectStorePort {
    type Error;

    fn verify_generation_object(
        &self,
        scope: &TenantScope,
        reference: &GenerationObjectReference,
    ) -> Result<bool, Self::Error>;
}
