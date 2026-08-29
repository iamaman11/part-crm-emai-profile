use encrypted_generation_domain::GenerationDek;
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

/// Server-selected immutable generation download capability.
///
/// The Bridge cannot construct this value from a generation/object selector. The shipping
/// control-plane adapter validates the authoritative descriptor and the descriptor-bound signed
/// R2 GET before constructing it. The signed URL is zeroed when the capability is dropped.
pub struct GenerationDownloadCapability {
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: [u8; 32],
    container_digest: [u8; 32],
    container_bytes: u64,
    signed_url: Vec<u8>,
    expires_seconds: u32,
}

impl GenerationDownloadCapability {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        generation_id: GenerationId,
        object_key: String,
        metadata_digest: [u8; 32],
        container_digest: [u8; 32],
        container_bytes: u64,
        signed_url: String,
        expires_seconds: u32,
    ) -> Self {
        Self {
            generation_id,
            object_key,
            metadata_digest,
            container_digest,
            container_bytes,
            signed_url: signed_url.into_bytes(),
            expires_seconds,
        }
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
    pub const fn metadata_digest(&self) -> [u8; 32] {
        self.metadata_digest
    }

    #[must_use]
    pub const fn container_digest(&self) -> [u8; 32] {
        self.container_digest
    }

    #[must_use]
    pub const fn container_bytes(&self) -> u64 {
        self.container_bytes
    }

    #[must_use]
    pub fn signed_url(&self) -> Option<&str> {
        core::str::from_utf8(&self.signed_url).ok()
    }

    #[must_use]
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }
}

impl core::fmt::Debug for GenerationDownloadCapability {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("GenerationDownloadCapability")
            .field("generation_id", &self.generation_id)
            .field("object_key", &self.object_key)
            .field("metadata_digest", &"[SHA256]")
            .field("container_digest", &"[SHA256]")
            .field("container_bytes", &self.container_bytes)
            .field("signed_url", &"[REDACTED]")
            .field("expires_seconds", &self.expires_seconds)
            .finish()
    }
}

impl Drop for GenerationDownloadCapability {
    fn drop(&mut self) {
        self.signed_url.fill(0);
    }
}

/// Control-plane half of authoritative reopen. Implementations must use the current live
/// coordinator witness and must not accept caller-selected generation/key/digest authority.
pub trait GenerationReopenControlPort {
    type Error;

    fn download_capability(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
    ) -> Result<GenerationDownloadCapability, Self::Error>;

    fn opening_material(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        metadata_prelude: &[u8],
    ) -> Result<GenerationDek, Self::Error>;
}

/// Effect-only immutable object downloader. This port receives only the already server-issued,
/// descriptor-bound GET capability; it owns no R2 credentials and cannot choose an object key.
pub trait GenerationObjectDownloadPort {
    type Error;

    fn download_generation_object(
        &mut self,
        capability: &GenerationDownloadCapability,
    ) -> Result<Vec<u8>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::GenerationDownloadCapability;
    use profile_platform_primitives::GenerationId;

    #[test]
    fn signed_download_capability_redacts_the_bearer_url() -> Result<(), Box<dyn std::error::Error>>
    {
        let capability = GenerationDownloadCapability::new(
            GenerationId::parse("generation_reopen_capability_01")?,
            "tenants/tenant_reopen_capability_01/profiles/profile_reopen_capability_01/generations/generation_reopen_capability_01.bpgc".to_owned(),
            [0xaa; 32],
            [0xbb; 32],
            4096,
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/object?X-Amz-Signature=SECRET_SENTINEL".to_owned(),
            300,
        );
        let debug = format!("{capability:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("SECRET_SENTINEL"));
        assert!(capability.signed_url().is_some());
        assert_eq!(capability.expires_seconds(), 300);
        Ok(())
    }
}
