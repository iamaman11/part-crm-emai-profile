//! Read-only identity-bound source for an already fsynced BPGC0001 spool.
//!
//! This is not another generation or authority. The existing save owner must
//! persist the exact checkpoint and receive a fresh signed PUT capability
//! before a network effect. Unknown/torn spool files are NEVER removed here.
use encrypted_generation_domain::{
    MAX_GENERATION_METADATA_PRELUDE_BYTES, MAX_STREAM_CONTAINER_BYTES,
    inspect_generation_metadata_prelude,
};
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
    payload_offset: u64,
}

impl VerifiedGenerationObjectFile {
    pub fn open(path: &Path, expected_bytes: u64, expected_sha256: [u8; 32]) -> io::Result<Self> {
        Self::open_payload(path, 0, expected_bytes, expected_sha256)
    }

    /// Exact encrypted bytes inside a BPAT0001 checkpoint. The bounded
    /// header must be verified separately by the checkpoint's current owner.
    pub fn open_checkpoint_payload(
        path: &Path,
        offset: u64,
        expected_bytes: u64,
        expected_sha256: [u8; 32],
    ) -> io::Result<Self> {
        if !(12..=12 + 4_096).contains(&offset) {
            return Err(invalid_source());
        }
        Self::open_payload(path, offset, expected_bytes, expected_sha256)
    }

    fn open_payload(
        path: &Path,
        payload_offset: u64,
        expected_bytes: u64,
        expected_sha256: [u8; 32],
    ) -> io::Result<Self> {
        if !path.is_absolute() || expected_bytes == 0 || expected_bytes > MAX_STREAM_CONTAINER_BYTES
        {
            return Err(invalid_source());
        }
        let path_meta = fs::symlink_metadata(path)?;
        if path_meta.file_type().is_symlink()
            || !path_meta.is_file()
            || path_meta.len()
                != payload_offset
                    .checked_add(expected_bytes)
                    .ok_or_else(invalid_source)?
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
            payload_offset,
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

    /// Read only the bounded BPGC metadata prelude after checking the entire
    /// file against its original descriptor. The returned bytes can be sent to
    /// the existing authenticated opening-material owner; this does NOT
    /// establish AEAD validity, browser WORKING or launch authority.
    ///
    /// Never allocate a generation-sized Vec for a 256/512 MiB R2 object.
    /// Recheck the same open file handle after parsing and rewind it for
    /// the later authenticated streaming decrypt.
    pub fn read_descriptor_verified_metadata_prelude(&mut self) -> io::Result<Vec<u8>> {
        self.recheck_and_rewind()?;
        let mut header = [0_u8; 12];
        self.file.read_exact(&mut header)?;
        if &header[..8] != b"BPGC0001" {
            return Err(invalid_source());
        }
        let metadata_len =
            u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
        if metadata_len == 0
            || metadata_len > MAX_GENERATION_METADATA_PRELUDE_BYTES - header.len()
            || (header.len() + metadata_len) as u64 > self.expected_bytes
        {
            return Err(invalid_source());
        }
        let mut prelude = Vec::with_capacity(header.len() + metadata_len);
        prelude.extend_from_slice(&header);
        prelude.resize(header.len() + metadata_len, 0);
        self.file.read_exact(&mut prelude[header.len()..])?;
        let inspected =
            inspect_generation_metadata_prelude(&prelude).map_err(|_| invalid_source())?;
        if inspected.prelude_bytes() != prelude.len() {
            return Err(invalid_source());
        }
        self.recheck_and_rewind()?;
        Ok(prelude)
    }

    /// May be called after every network effect. Exact digest/size must
    /// still match; no cloud commit is allowed on failure.
    pub fn recheck_and_rewind(&mut self) -> io::Result<()> {
        if self.file.metadata()?.len()
            != self
                .payload_offset
                .checked_add(self.expected_bytes)
                .ok_or_else(invalid_source)?
        {
            return Err(invalid_source());
        }
        self.file.seek(SeekFrom::Start(self.payload_offset))?;
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
        self.file.seek(SeekFrom::Start(self.payload_offset))?;
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
        assert!(matches!(
            spool.recheck_and_rewind(),
            Err(ref error) if error.kind() == io::ErrorKind::InvalidData,
        ));
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
        let prelude = sealed.read_descriptor_verified_metadata_prelude()?;
        let inspected = encrypted_generation_domain::inspect_generation_metadata_prelude(&prelude)?;
        assert_eq!(inspected.metadata_digest(), report.metadata_digest());
        assert_eq!(inspected.metadata().generation_id(), metadata.generation_id());
        assert!(prelude.len() <= encrypted_generation_domain::MAX_GENERATION_METADATA_PRELUDE_BYTES);
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
    fn descriptor_verified_prelude_rejects_untrusted_header_without_opening_a_key()
    -> io::Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.path();
        for (case, header) in [
            (0, [b'X', b'P', b'G', b'C', b'0', b'0', b'0', b'1', 0, 0, 0, 1]),
            (1, [b'B', b'P', b'G', b'C', b'0', b'0', b'0', b'1', 0, 0, 16, 1]),
        ] {
            if case != 0 {
                fs::remove_file(&path)?;
            }
            fs::write(&path, header)?;
            let mut object = VerifiedGenerationObjectFile::open(
                &path,
                header.len() as u64,
                fingerprint(&header),
            )?;
            assert!(object.read_descriptor_verified_metadata_prelude().is_err());
            assert_eq!(fs::read(&path)?, header, "invalid evidence is retained");
        }
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
        assert!(
            VerifiedGenerationObjectFile::open(&path, 9, fingerprint(b"immutable")).is_ok(),
            "the exact original file must be accepted"
        );
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

    #[test]
    fn embedded_checkpoint_streams_only_ciphertext_and_refuses_mutation() -> io::Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.path();
        let header = b"BPAT0001----serialized-original-metadata";
        let ciphertext = b"BPGC0001encrypted-specific-object";
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        file.write_all(header)?;
        file.write_all(ciphertext)?;
        file.sync_all()?;
        drop(file);
        let offset = header.len() as u64;
        let size = ciphertext.len() as u64;
        let mut exact = VerifiedGenerationObjectFile::open_checkpoint_payload(
            &path,
            offset,
            size,
            fingerprint(ciphertext),
        )?;
        let mut copied = Vec::new();
        exact.reader().read_to_end(&mut copied)?;
        assert_eq!(copied, ciphertext);
        exact.recheck_and_rewind()?;
        assert!(
            VerifiedGenerationObjectFile::open_checkpoint_payload(
                &path,
                11,
                size,
                fingerprint(ciphertext),
            )
            .is_err()
        );
        assert!(
            VerifiedGenerationObjectFile::open_checkpoint_payload(
                &path,
                offset + 1,
                size,
                fingerprint(ciphertext),
            )
            .is_err()
        );
        let mut changed = fs::OpenOptions::new().write(true).open(&path)?;
        changed.seek(SeekFrom::Start(offset))?;
        changed.write_all(b"XXXXXXXX")?;
        changed.sync_all()?;
        assert!(exact.recheck_and_rewind().is_err());
        assert!(path.exists());
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
