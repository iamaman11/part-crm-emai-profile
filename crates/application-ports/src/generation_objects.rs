use crate::generations::GenerationPortError;
use core::future::Future;
use profile_platform_primitives::{GenerationId, TenantScope};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationObjectUploadOutcome {
    Created,
    Idempotent,
    ImmutableConflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImmutableGenerationObject<'a> {
    generation_id: &'a GenerationId,
    object_key: &'a str,
    metadata_digest: &'a str,
    container_digest: &'a str,
    container: &'a [u8],
}

impl<'a> ImmutableGenerationObject<'a> {
    #[must_use]
    pub const fn new(
        generation_id: &'a GenerationId,
        object_key: &'a str,
        metadata_digest: &'a str,
        container_digest: &'a str,
        container: &'a [u8],
    ) -> Self {
        Self {
            generation_id,
            object_key,
            metadata_digest,
            container_digest,
            container,
        }
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        self.generation_id
    }

    #[must_use]
    pub const fn object_key(&self) -> &str {
        self.object_key
    }

    #[must_use]
    pub const fn metadata_digest(&self) -> &str {
        self.metadata_digest
    }

    #[must_use]
    pub const fn container_digest(&self) -> &str {
        self.container_digest
    }

    #[must_use]
    pub const fn container(&self) -> &[u8] {
        self.container
    }
}

pub trait GenerationObjectUploadPort {
    fn put_generation_object_if_absent(
        &self,
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> impl Future<Output = Result<GenerationObjectUploadOutcome, GenerationPortError>>;
}
