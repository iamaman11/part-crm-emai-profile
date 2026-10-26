//! Read-only identity-bound source for an already fsynced BPGC0001 spool.
//!
//! This is not another generation or authority. The existing save owner must
//! persist the exact checkpoint and receive a fresh signed PUT capability
//! before a network effect. Unknown/torn spool files are NEVER removed here.
use encrypted_generation_domain::MAX_STREAM_CONTAINER_BYTES;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const VERIFY_BUFFER_BYTES: usize = 65_536;

/// One open file descriptor; never materializes the ciphertext in a Vec.
/// The digest is supplied by the caller's authenticated sealed metadata and
/// must be rechecked after curl reads: mutation during the PUT blocks commit.
#[derive(Debug)]
pub struct VerifiedGenerationObjectFile {
    file: File,
    expected_bytes: u64,
    expected_sha256: [u8; 32],
}

impl VerifiedGenerationObjectFile {
    pub fn open(path: &Path, expected_bytes: u64, expected_sha256: [u8; 32]) -> io::Result<Self> {
        if !path.is_absolute() || expected_bytes == 0 || expected_bytes > MAX_STREAM_CONTAINER_BYTES
        {
            return Err(invalid_source());
        }
        let path_meta = fs::symlink_metadata(path)?;
        if path_meta.file_type().is_symlink()
            || !path_meta.is_file()
            || path_meta.len() != expected_bytes
        {
            return Err(invalid_source());
        }
        let file = File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(invalid_source());
        }
        let mut source = Self {
            file,
            expected_bytes,
            expected_sha256,
        };
        source.recheck_and_rewind()?;
        Ok(source)
    }

    #[must_use]
    pub const fn expected_bytes(&self) -> u64 {
        self.expected_bytes
    }

    /// The open handle, not a reopened pathname, is the byte source for PUT.
    pub fn reader(&mut self) -> &mut File {
        &mut self.file
    }

    /// May be called after every network effect. Exact digest/size must
    /// still match; no cloud commit is allowed on failure.
    pub fn recheck_and_rewind(&mut self) -> io::Result<()> {
        if self.file.metadata()?.len() != self.expected_bytes {
            return Err(invalid_source());
        }
        self.file.seek(SeekFrom::Start(0))?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; VERIFY_BUFFER_BYTES];
        let mut bytes = 0_u64;
        loop {
            let count = self.file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes = bytes.checked_add(count as u64).ok_or_else(invalid_source)?;
            if bytes > self.expected_bytes {
                return Err(invalid_source());
            }
            digest.update(&buffer[..count]);
        }
        if bytes != self.expected_bytes
            || <[u8; 32]>::from(digest.finalize()) != self.expected_sha256
        {
            return Err(invalid_source());
        }
        self.file.seek(SeekFrom::Start(0))?;
        Ok(())
    }
}

fn invalid_source() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "generation object spool changed",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(1);

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new() -> io::Result<Self> {
            let dir = std::env::temp_dir().join(format!(
                "bridge-k2-object-spool-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir)?;
            Ok(Self(dir))
        }
        fn path(&self) -> std::path::PathBuf {
            self.0.join("sealed.bpgc")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fingerprint(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    #[test]
    fn spool_is_read_only_bounded_and_detects_a_changed_file_after_network_read() -> io::Result<()>
    {
        let fixture = Fixture::new()?;
        let path = fixture.path();
        let original = b"BPGC0001exact-ciphertext";
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(original)?;
        file.sync_all()?;
        drop(file);
        let mut spool = VerifiedGenerationObjectFile::open(
            &path,
            original.len() as u64,
            fingerprint(original),
        )?;
        assert_eq!(spool.expected_bytes(), original.len() as u64);
        let mut transmitted = Vec::new();
        spool.reader().read_to_end(&mut transmitted)?;
        assert_eq!(transmitted, original);
        spool.recheck_and_rewind()?;
        let mut changed = fs::OpenOptions::new().write(true).open(&path)?;
        changed.write_all(b"BPGC0001mutated-ciphertext")?;
        changed.sync_all()?;
        assert_eq!(
            spool.recheck_and_rewind().unwrap_err().kind(),
            io::ErrorKind::InvalidData,
        );
        assert!(path.exists(), "uncertain ciphertext is never deleted");
        assert!(VerifiedGenerationObjectFile::open(
            &path, original.len() as u64, fingerprint(original),
        ).is_err());
        Ok(())
    }

    #[test]
    fn encrypted_stream_seals_directly_to_verified_spool_without_ciphertext_vec()
    -> Result<(), Box<dyn std::error::Error>> {
        use encrypted_generation_domain::{
            GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
            open_generation_streaming, seal_generation_streaming,
        };
        use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
        let fixture = Fixture::new()?;
        let source_len = 2_400_017_u64;
        let identity = GenerationIdentity::new(
            TenantId::parse("tenant_01JSTREAMOBJECT")?,
            ProfileId::parse("profile_01JSTREAMOBJECT")?,
            GenerationId::parse("generation_01JSTREAMOBJECT")?,
            None,
        );
        let key = GenerationDek::new(KeyId::parse("k2_stream_verified_object")?, [0x39; 32]);
        let metadata = GenerationMetadata::for_reader(
            identity,
            key.key_id().clone(),
            NoncePrefix::new([0x41; 16]),
            65_536,
            &mut std::io::repeat(0xa7).take(source_len),
        )?;
        let path = fixture.path();
        let mut sealed_file = fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(&path)?;
        let report = seal_generation_streaming(
            &metadata,
            &key,
            &mut std::io::repeat(0xa7).take(source_len),
            &mut sealed_file,
        )?;
        sealed_file.sync_all()?;
        drop(sealed_file);
        let mut sealed = VerifiedGenerationObjectFile::open(
            &path,
            report.container_bytes(),
            report.container_digest().bytes(),
        )?;
        assert_eq!(sealed.expected_bytes(), report.container_bytes());
        let mut sink = std::io::sink();
        let opened = open_generation_streaming(sealed.reader(), &key, &mut sink)?;
        assert_eq!(opened, report);
        sealed.recheck_and_rewind()?;
        assert!(
            VerifiedGenerationObjectFile::open(&path, report.container_bytes(), [0x55; 32],)
                .is_err(),
            "foreign advertised digest cannot be replayed"
        );
        Ok(())
    }

    #[test]
    fn rejects_missing_wrong_size_foreign_digest_directory_and_relative_path() -> io::Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.path();
        assert!(VerifiedGenerationObjectFile::open(&path, 1, fingerprint(b"x")).is_err());
        fs::write(&path, b"immutable")?;
        for (size, sha) in [
            (0, fingerprint(b"immutable")),
            (9, fingerprint(b"wrongone")),
            (10, fingerprint(b"immutable")),
            (MAX_STREAM_CONTAINER_BYTES + 1, fingerprint(b"immutable")),
        ] {
            assert!(VerifiedGenerationObjectFile::open(&path, size, sha).is_err());
        }
        assert!(VerifiedGenerationObjectFile::open(
            &path, 9, fingerprint(b"immutable")
        ).is_ok(), "the exact original file must be accepted");
        assert!(
            VerifiedGenerationObjectFile::open(
                Path::new("relative.bpgc"),
                8,
                fingerprint(b"immutable")
            )
            .is_err()
        );
        assert!(
            VerifiedGenerationObjectFile::open(&fixture.0, 8, fingerprint(b"immutable")).is_err()
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_to_a_trusted_spool() -> io::Result<()> {
        let fixture = Fixture::new()?;
        fs::write(fixture.path(), b"immutable")?;
        let link = fixture.0.join("link.bpgc");
        std::os::unix::fs::symlink(fixture.path(), &link)?;
        assert!(VerifiedGenerationObjectFile::open(&link, 9, fingerprint(b"immutable")).is_err());
        Ok(())
    }
}
