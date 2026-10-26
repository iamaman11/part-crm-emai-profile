use crate::generation_object_spool::VerifiedGenerationObjectFile;
use encrypted_generation_domain::MAX_STREAM_CONTAINER_BYTES;
use encrypted_generation_domain::{GenerationDek, MAX_GENERATION_CONTAINER_BYTES};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

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

/// Lossless, safe classification from the existing authenticated control-plane owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationReopenControlError {
    Transport,
    HttpStatus,
    InvalidResponse,
    Clock,
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

/// A single already-signed HTTPS R2 GET that writes its response incrementally
/// to an UNPUBLISHED sink. A 200 response alone is never cloud/open proof.
/// The effect must not follow redirects, retry or select an object URL.
pub trait SignedGenerationObjectStreamGetPort {
    type Error;

    fn get_exact_stream(
        &mut self,
        signed_url: &str,
        max_bytes: u64,
        destination: &mut dyn Write,
    ) -> Result<u16, Self::Error>;
}

/// A bounded authenticated ciphertext-spool downloader. No DEK, browser
/// profile or server authority is created by this network effect.
pub struct VerifiedGenerationObjectStreamDownloader<T> {
    transport: T,
}

impl<T> VerifiedGenerationObjectStreamDownloader<T> {
    #[must_use]
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
}

/// Enforce the authoritative descriptor BEFORE an untrusted response can
/// fill disk. Hash exactly the bytes written, never an untrusted Content-Length.
struct BoundedGenerationGetWriter<'a> {
    file: &'a mut std::fs::File,
    expected_bytes: u64,
    written: u64,
    hasher: Sha256,
}

impl Write for BoundedGenerationGetWriter<'_> {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        let next = self
            .written
            .checked_add(value.len() as u64)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "generation GET length overflow")
            })?;
        if next > self.expected_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "generation GET exceeds original signed descriptor",
            ));
        }
        self.file.write_all(value)?;
        self.hasher.update(value);
        self.written = next;
        Ok(value.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl<T: SignedGenerationObjectStreamGetPort> VerifiedGenerationObjectStreamDownloader<T> {
    /// Staging path must be absolute, on an existing trusted regular directory
    /// and have a reserved .pending-* name. Create-new means even a failed
    /// previous GET cannot silently be replaced. On every error staged bytes
    /// remain unpublished, available for conservative crash diagnostics.
    pub fn download_to_new_pending_file(
        &mut self,
        capability: &GenerationDownloadCapability,
        destination: &Path,
    ) -> Result<VerifiedGenerationObjectFile, GenerationObjectDownloadError> {
        if !destination.is_absolute()
            || !destination
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".pending-") && name.len() <= 128)
            || capability.container_bytes() == 0
            || capability.container_bytes() > MAX_STREAM_CONTAINER_BYTES
            || !(1..=300).contains(&capability.expires_seconds())
        {
            return Err(GenerationObjectDownloadError::InvalidCapability);
        }
        let parent = destination
            .parent()
            .ok_or(GenerationObjectDownloadError::InvalidCapability)?;
        let metadata =
            fs::symlink_metadata(parent).map_err(|_| GenerationObjectDownloadError::Transport)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(GenerationObjectDownloadError::InvalidCapability);
        }
        let url = capability
            .signed_url()
            .ok_or(GenerationObjectDownloadError::InvalidCapability)?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)
            .map_err(|_| GenerationObjectDownloadError::Transport)?;
        let mut writer = BoundedGenerationGetWriter {
            file: &mut output,
            expected_bytes: capability.container_bytes(),
            written: 0,
            hasher: Sha256::new(),
        };
        let status = self
            .transport
            .get_exact_stream(url, capability.container_bytes(), &mut writer)
            .map_err(|_| GenerationObjectDownloadError::Transport)?;
        if status != 200 {
            return Err(GenerationObjectDownloadError::HttpStatus);
        }
        if writer.written != capability.container_bytes() {
            return Err(GenerationObjectDownloadError::SizeMismatch);
        }
        let digest: [u8; 32] = writer.hasher.finalize().into();
        if digest != capability.container_digest() {
            return Err(GenerationObjectDownloadError::DigestMismatch);
        }
        output
            .sync_all()
            .map_err(|_| GenerationObjectDownloadError::Transport)?;
        drop(output);
        VerifiedGenerationObjectFile::open(
            destination,
            capability.container_bytes(),
            capability.container_digest(),
        )
        .map_err(|_| GenerationObjectDownloadError::DigestMismatch)
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

    /// Data is generated into the caller's bounded sink, not into a giant
    /// fake HTTP Vec: exercise exactly the large-payload GET architecture.
    #[derive(Clone, Copy)]
    struct StreamGet {
        status: u16,
        bytes: u64,
        byte: u8,
        fail_after: Option<u64>,
    }

    impl super::SignedGenerationObjectStreamGetPort for StreamGet {
        type Error = std::io::Error;

        fn get_exact_stream(
            &mut self,
            _signed_url: &str,
            _max_bytes: u64,
            target: &mut dyn std::io::Write,
        ) -> Result<u16, Self::Error> {
            let block = [self.byte; 65_536];
            let mut sent = 0_u64;
            while sent < self.bytes {
                let count = (self.bytes - sent).min(block.len() as u64) as usize;
                target.write_all(&block[..count])?;
                sent += count as u64;
                if self.fail_after.is_some_and(|threshold| sent >= threshold) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "synthetic signed GET interruption",
                    ));
                }
            }
            Ok(self.status)
        }
    }

    fn stream_capability(
        bytes: u64,
        byte: u8,
    ) -> Result<GenerationDownloadCapability, Box<dyn std::error::Error>> {
        let mut sha = Sha256::new();
        let block = [byte; 65_536];
        let mut remaining = bytes;
        while remaining != 0 {
            let take = (remaining.min(block.len() as u64)) as usize;
            sha.update(&block[..take]);
            remaining -= take as u64;
        }
        Ok(GenerationDownloadCapability::new(
            GenerationId::parse("generation_stream_get_descriptor")?,
            "tenants/t/profiles/p/generations/g.bpgc".to_owned(),
            [0x10; 32],
            sha.finalize().into(),
            bytes,
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/item?X-Amz-Signature=SECRET".to_owned(),
            120,
        ))
    }

    #[test]
    fn stream_get_writes_verified_fsynced_bpgc_without_generation_sized_vec()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::VerifiedGenerationObjectStreamDownloader;
        use std::io::Read;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let dir = std::env::temp_dir().join(format!(
            "k2-stream-get-valid-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&dir)?;
        let len = 2_500_031_u64;
        let capability = stream_capability(len, 0x72)?;
        let path = dir.join(".pending-signed-get-valid");
        let mut download = VerifiedGenerationObjectStreamDownloader::new(StreamGet {
            status: 200,
            bytes: len,
            byte: 0x72,
            fail_after: None,
        });
        let mut spool = download.download_to_new_pending_file(&capability, &path)?;
        assert_eq!(spool.expected_bytes(), len);
        let mut buffer = [0_u8; 65_536];
        let mut total = 0_u64;
        loop {
            let count = spool.reader().read(&mut buffer)?;
            if count == 0 {
                break;
            }
            assert!(buffer[..count].iter().all(|byte| *byte == 0x72));
            total += count as u64;
        }
        assert_eq!(total, len);
        spool.recheck_and_rewind()?;
        // A crash/retry can never silently replace the first pending file.
        assert!(
            download
                .download_to_new_pending_file(&capability, &path)
                .is_err()
        );
        drop(spool);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn stream_get_redirect_mismatch_overflow_and_disconnect_never_publish()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{
            GenerationObjectDownloadError as Error, VerifiedGenerationObjectStreamDownloader,
        };
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let dir = std::env::temp_dir().join(format!(
            "k2-stream-get-invalid-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&dir)?;
        let len = 131_077_u64;
        let capability = stream_capability(len, 0x42)?;
        for (index, simulated, expected) in [
            (
                0,
                StreamGet {
                    status: 302,
                    bytes: len,
                    byte: 0x42,
                    fail_after: None,
                },
                Error::HttpStatus,
            ),
            (
                1,
                StreamGet {
                    status: 200,
                    bytes: len - 1,
                    byte: 0x42,
                    fail_after: None,
                },
                Error::SizeMismatch,
            ),
            (
                2,
                StreamGet {
                    status: 200,
                    bytes: len,
                    byte: 0x43,
                    fail_after: None,
                },
                Error::DigestMismatch,
            ),
            (
                3,
                StreamGet {
                    status: 200,
                    bytes: len + 1,
                    byte: 0x42,
                    fail_after: None,
                },
                Error::Transport,
            ),
            (
                4,
                StreamGet {
                    status: 200,
                    bytes: len,
                    byte: 0x42,
                    fail_after: Some(65_536),
                },
                Error::Transport,
            ),
        ] {
            let path = dir.join(format!(".pending-untrusted-{index}"));
            let mut downloader = VerifiedGenerationObjectStreamDownloader::new(simulated);
            let outcome = downloader.download_to_new_pending_file(&capability, &path);
            assert!(matches!(outcome, Err(reason) if reason == expected));
            assert!(
                path.exists(),
                "partial network data must not be quietly published"
            );
        }
        assert!(!dir.join("generation_stream_get_descriptor").exists());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    struct StreamFileGet(std::path::PathBuf);

    impl super::SignedGenerationObjectStreamGetPort for StreamFileGet {
        type Error = std::io::Error;

        fn get_exact_stream(
            &mut self,
            _signed_url: &str,
            max_bytes: u64,
            target: &mut dyn std::io::Write,
        ) -> Result<u16, Self::Error> {
            let mut source = std::fs::File::open(&self.0)?;
            let copied = std::io::copy(&mut source, target)?;
            if copied > max_bytes {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "over limit",
                ));
            }
            Ok(200)
        }
    }

    #[test]
    fn verified_r2_file_stream_then_authenticated_bpgc_open_reuses_existing_format()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::VerifiedGenerationObjectStreamDownloader;
        use encrypted_generation_domain::{
            GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
            open_generation_streaming, seal_generation_streaming,
        };
        use profile_platform_primitives::{ProfileId, TenantId};
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let dir = std::env::temp_dir().join(format!(
            "k2-stream-r2-bpgc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&dir)?;
        let original = dir.join("already-sealed.bpgc");
        let plaintext_bytes = 1_350_017_u64;
        let key = GenerationDek::new(KeyId::parse("k2_verified_r2_stream_test_key")?, [0x35; 32]);
        let generation = GenerationId::parse("generation_signed_verified_stream")?;
        let metadata = GenerationMetadata::for_reader(
            GenerationIdentity::new(
                TenantId::parse("tenant_verified_stream")?,
                ProfileId::parse("profile_verified_stream")?,
                generation.clone(),
                None,
            ),
            key.key_id().clone(),
            NoncePrefix::new([0x42; 16]),
            65_536,
            &mut std::io::repeat(0x73).take(plaintext_bytes),
        )?;
        let mut source = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&original)?;
        let expected = seal_generation_streaming(
            &metadata,
            &key,
            &mut std::io::repeat(0x73).take(plaintext_bytes),
            &mut source,
        )?;
        source.flush()?;
        source.sync_all()?;
        drop(source);

        let capability = GenerationDownloadCapability::new(
            generation,
            metadata.object_key(),
            expected.metadata_digest().bytes(),
            expected.container_digest().bytes(),
            expected.container_bytes(),
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/object?X-Amz-Signature=SECRET".to_owned(),
            120,
        );
        let pending = dir.join(".pending-network-ciphertext");
        let mut fetcher = VerifiedGenerationObjectStreamDownloader::new(StreamFileGet(original));
        let mut object = fetcher.download_to_new_pending_file(&capability, &pending)?;
        let prelude = object.read_descriptor_verified_metadata_prelude()?;
        let inspected = encrypted_generation_domain::inspect_generation_metadata_prelude(&prelude)?;
        assert_eq!(inspected.metadata_digest(), expected.metadata_digest());
        assert_eq!(inspected.metadata().generation_id(), &generation);
        let mut opened_count = 0_u64;
        struct CountSink<'a>(&'a mut u64);
        impl Write for CountSink<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                *self.0 += bytes.len() as u64;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let reopened =
            open_generation_streaming(object.reader(), &key, &mut CountSink(&mut opened_count))?;
        assert_eq!(reopened, expected);
        assert_eq!(opened_count, plaintext_bytes);
        object.recheck_and_rewind()?;
        assert_eq!(object.expected_bytes(), expected.container_bytes());
        drop(object);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

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
