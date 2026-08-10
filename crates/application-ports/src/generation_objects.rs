use crate::generations::GenerationPortError;
use core::future::Future;
use profile_platform_primitives::{GenerationId, ProfileId, TenantScope};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationObjectUploadOutcome {
    Created,
    Idempotent,
    ImmutableConflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImmutableGenerationObject<'a> {
    profile_id: &'a ProfileId,
    generation_id: &'a GenerationId,
    object_key: &'a str,
    metadata_digest: &'a str,
    container_digest: &'a str,
    container: &'a [u8],
}

impl<'a> ImmutableGenerationObject<'a> {
    #[must_use]
    pub const fn new(
        profile_id: &'a ProfileId,
        generation_id: &'a GenerationId,
        object_key: &'a str,
        metadata_digest: &'a str,
        container_digest: &'a str,
        container: &'a [u8],
    ) -> Self {
        Self {
            profile_id,
            generation_id,
            object_key,
            metadata_digest,
            container_digest,
            container,
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        self.profile_id
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationObjectDescriptor {
    profile_id: ProfileId,
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
}

impl GenerationObjectDescriptor {
    #[must_use]
    pub fn new(
        profile_id: ProfileId,
        generation_id: GenerationId,
        object_key: impl Into<String>,
        metadata_digest: impl Into<String>,
        container_digest: impl Into<String>,
        container_bytes: u64,
    ) -> Self {
        Self {
            profile_id,
            generation_id,
            object_key: object_key.into(),
            metadata_digest: metadata_digest.into(),
            container_digest: container_digest.into(),
            container_bytes,
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }

    #[must_use]
    pub const fn container_bytes(&self) -> u64 {
        self.container_bytes
    }
}

pub trait GenerationObjectUploadPort {
    fn put_generation_object_if_absent(
        &self,
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> impl Future<Output = Result<GenerationObjectUploadOutcome, GenerationPortError>>;
}

pub trait GenerationObjectExactVerifyPort {
    fn verify_generation_object_exact(
        &self,
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> impl Future<Output = Result<bool, GenerationPortError>>;
}

pub trait GenerationObjectDescriptorVerifyPort {
    fn verify_generation_object_descriptor_exact(
        &self,
        scope: &TenantScope,
        descriptor: &GenerationObjectDescriptor,
    ) -> impl Future<Output = Result<bool, GenerationPortError>>;
}
