use encrypted_generation_domain::{GenerationDek, MAX_GENERATION_CONTAINER_BYTES};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};

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

/// Response from the narrow exact-URL GET effect. Redirects are not represented as success: the
/// effect returns the direct HTTP status and the verified wrapper requires an exact 200.
pub struct SignedGenerationObjectGetResponse {
    status: u16,
    body: Vec<u8>,
}

impl SignedGenerationObjectGetResponse {
    #[must_use]
    pub const fn new(status: u16, body: Vec<u8>) -> Self {
        Self { status, body }
    }

    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    fn into_body(mut self) -> Vec<u8> {
        core::mem::take(&mut self.body)
    }
}

impl Drop for SignedGenerationObjectGetResponse {
    fn drop(&mut self) {
        self.body.fill(0);
    }
}

/// Raw network effect for one already-issued signed generation URL. Implementations must perform
/// exactly one HTTPS GET, must not follow redirects and must enforce `max_bytes` while reading.
pub trait SignedGenerationObjectGetPort {
    type Error;

    fn get_exact(
        &mut self,
        signed_url: &str,
        max_bytes: usize,
    ) -> Result<SignedGenerationObjectGetResponse, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationObjectDownloadError {
    InvalidCapability,
    Transport,
    HttpStatus,
    SizeMismatch,
    DigestMismatch,
}

impl core::fmt::Display for GenerationObjectDownloadError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCapability => "generation download capability is invalid",
            Self::Transport => "generation object download transport failed",
            Self::HttpStatus => "generation object download did not return direct HTTP 200",
            Self::SizeMismatch => "generation object size does not match authoritative descriptor",
            Self::DigestMismatch => {
                "generation object digest does not match authoritative descriptor"
            }
        })
    }
}

impl std::error::Error for GenerationObjectDownloadError {}

/// Effect-only immutable object downloader. This port receives only the already server-issued,
/// descriptor-bound GET capability; it owns no R2 credentials and cannot choose an object key.
pub trait GenerationObjectDownloadPort {
    type Error;

    fn download_generation_object(
        &mut self,
        capability: &GenerationDownloadCapability,
    ) -> Result<Vec<u8>, Self::Error>;
}

pub struct VerifiedGenerationObjectDownloader<T> {
    transport: T,
}

impl<T> VerifiedGenerationObjectDownloader<T> {
    #[must_use]
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<T> GenerationObjectDownloadPort for VerifiedGenerationObjectDownloader<T>
where
    T: SignedGenerationObjectGetPort,
{
    type Error = GenerationObjectDownloadError;

    fn download_generation_object(
        &mut self,
        capability: &GenerationDownloadCapability,
    ) -> Result<Vec<u8>, Self::Error> {
        let expected_bytes = usize::try_from(capability.container_bytes())
            .map_err(|_| Self::Error::InvalidCapability)?;
        if expected_bytes == 0 || expected_bytes > MAX_GENERATION_CONTAINER_BYTES {
            return Err(Self::Error::InvalidCapability);
        }
        let signed_url = capability
            .signed_url()
            .ok_or(Self::Error::InvalidCapability)?;
        let response = self
            .transport
            .get_exact(signed_url, expected_bytes)
            .map_err(|_| Self::Error::Transport)?;
        if response.status() != 200 {
            return Err(Self::Error::HttpStatus);
        }
        let mut body = response.into_body();
        if body.len() != expected_bytes {
            body.fill(0);
            return Err(Self::Error::SizeMismatch);
        }
        let digest: [u8; 32] = Sha256::digest(&body).into();
        if digest != capability.container_digest() {
            body.fill(0);
            return Err(Self::Error::DigestMismatch);
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GenerationDownloadCapability, GenerationObjectDownloadError, GenerationObjectDownloadPort,
        SignedGenerationObjectGetPort, SignedGenerationObjectGetResponse,
        VerifiedGenerationObjectDownloader,
    };
    use profile_platform_primitives::GenerationId;
    use sha2::{Digest, Sha256};

    struct FakeGet {
        status: u16,
        body: Vec<u8>,
        observed_max: Option<usize>,
    }

    impl SignedGenerationObjectGetPort for FakeGet {
        type Error = ();

        fn get_exact(
            &mut self,
            _signed_url: &str,
            max_bytes: usize,
        ) -> Result<SignedGenerationObjectGetResponse, Self::Error> {
            self.observed_max = Some(max_bytes);
            Ok(SignedGenerationObjectGetResponse::new(
                self.status,
                self.body.clone(),
            ))
        }
    }

    fn capability_for(
        body: &[u8],
    ) -> Result<GenerationDownloadCapability, Box<dyn std::error::Error>> {
        let digest: [u8; 32] = Sha256::digest(body).into();
        Ok(GenerationDownloadCapability::new(
            GenerationId::parse("generation_reopen_capability_01")?,
            "tenants/tenant_reopen_capability_01/profiles/profile_reopen_capability_01/generations/generation_reopen_capability_01.bpgc".to_owned(),
            [0xaa; 32],
            digest,
            u64::try_from(body.len())?,
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/object?X-Amz-Signature=SECRET_SENTINEL".to_owned(),
            300,
        ))
    }

    #[test]
    fn signed_download_capability_redacts_the_bearer_url() -> Result<(), Box<dyn std::error::Error>>
    {
        let capability = capability_for(b"generation-object")?;
        let debug = format!("{capability:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("SECRET_SENTINEL"));
        assert!(capability.signed_url().is_some());
        assert_eq!(capability.expires_seconds(), 300);
        Ok(())
    }

    #[test]
    fn verified_downloader_accepts_only_exact_size_and_digest()
    -> Result<(), Box<dyn std::error::Error>> {
        let body = b"exact-generation-object".to_vec();
        let capability = capability_for(&body)?;
        let mut downloader = VerifiedGenerationObjectDownloader::new(FakeGet {
            status: 200,
            body: body.clone(),
            observed_max: None,
        });
        assert_eq!(downloader.download_generation_object(&capability)?, body);
        assert_eq!(downloader.transport.observed_max, Some(body.len()));
        Ok(())
    }

    #[test]
    fn verified_downloader_rejects_redirect_size_and_digest_mismatch()
    -> Result<(), Box<dyn std::error::Error>> {
        let body = b"exact-generation-object".to_vec();
        let capability = capability_for(&body)?;
        for (status, response_body, expected) in [
            (302, body.clone(), GenerationObjectDownloadError::HttpStatus),
            (
                200,
                b"short".to_vec(),
                GenerationObjectDownloadError::SizeMismatch,
            ),
            (
                200,
                b"wrong-generation-object".to_vec(),
                GenerationObjectDownloadError::DigestMismatch,
            ),
        ] {
            let mut downloader = VerifiedGenerationObjectDownloader::new(FakeGet {
                status,
                body: response_body,
                observed_max: None,
            });
            assert_eq!(
                downloader.download_generation_object(&capability),
                Err(expected)
            );
        }
        Ok(())
    }
}
