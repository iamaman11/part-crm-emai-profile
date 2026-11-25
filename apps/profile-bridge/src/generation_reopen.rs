use crate::generation_object_spool::VerifiedGenerationObjectFile;
use encrypted_generation_domain::MAX_STREAM_CONTAINER_BYTES;
use encrypted_generation_domain::GenerationDek;
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};
use std::fs;
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
    /// The read-only generation request received HTTP 408, 429 or 5xx.
    /// This is never a fresh claim, retry grant or write authorization.
    ServiceTemporarilyUnavailable,
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

    /// Optional historical read-only candidate. Never a WORKING certificate.
    /// Existing mocks/implementations fail closed until they deliberately
    /// implement this feature; normal CURRENT reopen stays unchanged.
    fn verified_predecessor_capability(
        &mut self,
        _tenant_id: &TenantId,
        _profile_id: &ProfileId,
    ) -> Result<Option<GenerationDownloadCapability>, Self::Error> {
        Ok(None)
    }

    fn verified_predecessor_opening_material(
        &mut self,
        _tenant_id: &TenantId,
        _profile_id: &ProfileId,
        _metadata_prelude: &[u8],
    ) -> Result<Option<GenerationDek>, Self::Error> {
        Ok(None)
    }

    /// The server may persist a receipt ONLY for a completed real-browser
    /// historical probe. Default mocks reject it; a hash is never authority.
    #[allow(clippy::too_many_arguments)]
    fn certify_verified_predecessor_working(
        &mut self,
        _tenant_id: &TenantId,
        _profile_id: &ProfileId,
        _current: &GenerationId,
        _historical: &GenerationId,
        _opening_prelude: &[u8],
        _runtime_inventory_sha256: &str,
        _native_snapshot_sha256: &str,
        _native_browser_witness_sha256: &str,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }
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

/// Effect-only immutable object downloader. This port receives only an already server-issued,
/// descriptor-bound GET capability and materializes a verified OS-owned temporary file. It owns
/// no R2 credentials, cannot select another object, and cannot buffer a generation-sized Vec.
pub trait GenerationObjectDownloadPort {
    type Error;

    /// Only the authoritative materialization owner supplies the trusted root.
    fn download_generation_object_to_temporary_file(
        &mut self,
        capability: &GenerationDownloadCapability,
        directory: &Path,
    ) -> Result<VerifiedGenerationObjectFile, Self::Error>;
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

impl<T: SignedGenerationObjectStreamGetPort> GenerationObjectDownloadPort
    for VerifiedGenerationObjectStreamDownloader<T>
{
    type Error = GenerationObjectDownloadError;

    fn download_generation_object_to_temporary_file(
        &mut self,
        capability: &GenerationDownloadCapability,
        directory: &Path,
    ) -> Result<VerifiedGenerationObjectFile, Self::Error> {
        self.download_to_temporary_file(capability, directory)
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
    /// Restore scratch must remain inside an existing trusted directory.
    /// The OS owns deletion of its unnamed file when the last handle closes,
    /// including abnormal termination; no global pending-file cleaner exists.
    pub fn download_to_temporary_file(
        &mut self,
        capability: &GenerationDownloadCapability,
        directory: &Path,
    ) -> Result<VerifiedGenerationObjectFile, GenerationObjectDownloadError> {
        if !directory.is_absolute()
            || capability.container_bytes() == 0
            || capability.container_bytes() > MAX_STREAM_CONTAINER_BYTES
            || !(1..=300).contains(&capability.expires_seconds())
        {
            return Err(GenerationObjectDownloadError::InvalidCapability);
        }
        let metadata = fs::symlink_metadata(directory)
            .map_err(|_| GenerationObjectDownloadError::Transport)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(GenerationObjectDownloadError::InvalidCapability);
        }
        let url = capability
            .signed_url()
            .ok_or(GenerationObjectDownloadError::InvalidCapability)?;
        let mut output = tempfile::tempfile_in(directory)
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
        VerifiedGenerationObjectFile::from_temporary_file(
            output,
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
        VerifiedGenerationObjectStreamDownloader,
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
        let mut download = VerifiedGenerationObjectStreamDownloader::new(StreamGet {
            status: 200,
            bytes: len,
            byte: 0x72,
            fail_after: None,
        });
        let mut spool = download.download_to_temporary_file(&capability, &dir)?;
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
        // Every request owns an independent OS-cleaned handle, not a global
        // digest-named file that could be left behind by a dead process.
        let second = download.download_to_temporary_file(&capability, &dir)?;
        drop(second);
        drop(spool);
        assert!(std::fs::read_dir(&dir)?.next().is_none());
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
        for (_index, simulated, expected) in [
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
            let mut downloader = VerifiedGenerationObjectStreamDownloader::new(simulated);
            let outcome = downloader.download_to_temporary_file(&capability, &dir);
            assert!(matches!(outcome, Err(reason) if reason == expected));
            assert!(
                std::fs::read_dir(&dir)?.next().is_none(),
                "a failed exact GET must leave no named scratch bytes"
            );
            // The same descriptor can be retried without manual deletion.
            let mut retry = VerifiedGenerationObjectStreamDownloader::new(StreamGet {
                status: 200,
                bytes: len,
                byte: 0x42,
                fail_after: None,
            });
            let verified = retry.download_to_temporary_file(&capability, &dir)?;
            drop(verified);
        }
        assert!(!dir.join("generation_stream_get_descriptor").exists());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn temporary_download_does_not_touch_foreign_named_pending_file()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::VerifiedGenerationObjectStreamDownloader;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(".pending-foreign-preserved");
        std::fs::write(&path, b"unknown owner evidence")?;
        let len = 65_537;
        let capability = stream_capability(len, 0x42)?;
        let mut downloader = VerifiedGenerationObjectStreamDownloader::new(StreamGet {
            status: 200,
            bytes: len,
            byte: 0x42,
            fail_after: None,
        });
        let verified = downloader.download_to_temporary_file(&capability, dir.path())?;
        drop(verified);
        assert_eq!(std::fs::read(&path)?, b"unknown owner evidence");
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
        let mut fetcher = VerifiedGenerationObjectStreamDownloader::new(StreamFileGet(original));
        let mut object = fetcher.download_to_temporary_file(&capability, &dir)?;
        let prelude = object.read_descriptor_verified_metadata_prelude()?;
        let inspected = encrypted_generation_domain::inspect_generation_metadata_prelude(&prelude)?;
        assert_eq!(inspected.metadata_digest(), expected.metadata_digest());
        assert_eq!(
            inspected.metadata().generation_id(),
            capability.generation_id()
        );
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

    #[test]
    fn streamed_transport_rejects_oversized_descriptor_before_creating_scratch()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let oversized = GenerationDownloadCapability::new(
            GenerationId::parse("generation_stream_too_large")?,
            "tenants/t/profiles/p/generations/g.bpgc".to_owned(),
            [0x77; 32],
            [0x99; 32],
            encrypted_generation_domain::MAX_STREAM_CONTAINER_BYTES + 1,
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/item?X-Amz-Signature=SECRET".to_owned(),
            120,
        );
        let mut downloader = VerifiedGenerationObjectStreamDownloader::new(StreamGet {
            status: 200,
            bytes: 0,
            byte: 0,
            fail_after: None,
        });
        assert!(matches!(
            downloader.download_generation_object_to_temporary_file(&oversized, dir.path()),
            Err(GenerationObjectDownloadError::InvalidCapability)
        ));
        assert_eq!(std::fs::read_dir(dir.path())?.count(), 0);
        Ok(())
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

    #[cfg(windows)]
    #[test]
    fn windows_os_tempfile_child_fixture() -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        let Some(root) = std::env::var_os("PROFILE_BRIDGE_TEST_CHILD_TEMP_ROOT") else {
            return Ok(());
        };
        // The parent deliberately kills this process before either File drops.
        let mut ciphertext = tempfile::tempfile_in(&root)?;
        let mut plaintext = tempfile::tempfile_in(&root)?;
        ciphertext.write_all(&[0x82; 4096])?;
        plaintext.write_all(&[0x53; 4096])?;
        ciphertext.sync_all()?;
        plaintext.sync_all()?;
        std::fs::write(std::path::Path::new(&root).join("child-ready"), b"ready")?;
        loop {
            std::thread::park();
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_death_retires_both_os_owned_restore_files()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::process::{Command, Stdio};
        use std::time::Duration;
        let root = tempfile::tempdir()?;
        let ready = root.path().join("child-ready");
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("generation_reopen::tests::windows_os_tempfile_child_fixture")
            .env("PROFILE_BRIDGE_TEST_CHILD_TEMP_ROOT", root.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let started = (0..100).any(|_| {
            if ready.exists() {
                return true;
            }
            if child.try_wait().ok().flatten().is_some() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
            false
        });
        if !started {
            let _ = child.kill();
            let _ = child.wait();
            return Err("OS-owned scratch child did not hold both handles".into());
        }
        child.kill()?;
        child.wait()?;
        let remaining = std::fs::read_dir(root.path())?
            .map(|entry| entry.map(|file| file.file_name()))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(remaining, vec![std::ffi::OsString::from("child-ready")]);
        Ok(())
    }
}
