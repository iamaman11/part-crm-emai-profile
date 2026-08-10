use crate::local_profile::{
    GenerationInventory, GenerationWorkspace, LocalGenerationRecord, LocalGenerationState,
    LocalProfileError, MaterializationRoot, RecoveryClone,
};
use encrypted_generation_domain::{
    GenerationDek, GenerationIdentity, GenerationMetadata, NoncePrefix, SealedGeneration,
    seal_generation,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use std::fmt;
use std::fs::File;
use std::io::Read;

const SNAPSHOT_MAGIC: &[u8; 8] = b"BPGW0001";
const MAX_SNAPSHOT_BYTES: usize = 67_108_864;
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyGenerationError {
    InvalidState,
    IdentityMismatch,
    CandidateMatchesBase,
    SnapshotTooLarge,
    SourceChanged,
    KeyUnavailable,
    EncryptionFailed,
    Local(LocalProfileError),
}

impl fmt::Display for DirtyGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidState => "local generation is not dirty under retained writer ownership",
            Self::IdentityMismatch => {
                "local generation identity does not match dirty candidate request"
            }
            Self::CandidateMatchesBase => "dirty candidate generation must be immutable and new",
            Self::SnapshotTooLarge => {
                "dirty generation snapshot exceeds encrypted-container policy"
            }
            Self::SourceChanged => "dirty generation changed while candidate snapshot was prepared",
            Self::KeyUnavailable => "generation sealing material is unavailable",
            Self::EncryptionFailed => "dirty generation encryption failed",
            Self::Local(error) => {
                return write!(formatter, "dirty generation local failure: {error}");
            }
        })
    }
}

impl std::error::Error for DirtyGenerationError {}

impl From<LocalProfileError> for DirtyGenerationError {
    fn from(error: LocalProfileError) -> Self {
        Self::Local(error)
    }
}

pub struct GenerationSealingMaterial {
    dek: GenerationDek,
    nonce_prefix: NoncePrefix,
    chunk_size: u32,
}

impl GenerationSealingMaterial {
    #[must_use]
    pub const fn new(dek: GenerationDek, nonce_prefix: NoncePrefix, chunk_size: u32) -> Self {
        Self {
            dek,
            nonce_prefix,
            chunk_size,
        }
    }
}

pub trait GenerationSealingMaterialPort {
    type Error;

    fn material_for(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<GenerationSealingMaterial, Self::Error>;
}

pub struct PreparedDirtyGeneration {
    candidate_workspace: GenerationWorkspace,
    candidate_inventory: GenerationInventory,
    sealed: SealedGeneration,
    metadata_digest: String,
    container_digest: String,
}

impl PreparedDirtyGeneration {
    #[must_use]
    pub const fn candidate_workspace(&self) -> &GenerationWorkspace {
        &self.candidate_workspace
    }

    #[must_use]
    pub const fn candidate_inventory(&self) -> &GenerationInventory {
        &self.candidate_inventory
    }

    #[must_use]
    pub const fn sealed(&self) -> &SealedGeneration {
        &self.sealed
    }

    #[must_use]
    pub fn object_key(&self) -> String {
        self.sealed.metadata().object_key()
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_dirty_generation_candidate<K: GenerationSealingMaterialPort>(
    local_record: &LocalGenerationRecord,
    source_workspace: &GenerationWorkspace,
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    candidate_generation_id: &GenerationId,
    keys: &mut K,
) -> Result<PreparedDirtyGeneration, DirtyGenerationError> {
    if local_record.state() != LocalGenerationState::DirtyLocal || !local_record.is_locked() {
        return Err(DirtyGenerationError::InvalidState);
    }
    let base_generation_id = local_record.generation_id();
    if base_generation_id == candidate_generation_id {
        return Err(DirtyGenerationError::CandidateMatchesBase);
    }
    let source_name = source_workspace
        .path()
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(DirtyGenerationError::IdentityMismatch)?;
    if source_name != base_generation_id.as_str() {
        return Err(DirtyGenerationError::IdentityMismatch);
    }

    let clone = RecoveryClone::create(
        source_workspace,
        root,
        tenant_id,
        profile_id,
        candidate_generation_id,
    )?;
    let candidate_inventory = clone.verify_clone_only()?;
    let snapshot = encode_workspace_snapshot(clone.workspace(), &candidate_inventory)?;
    if clone.verify_clone_only()? != candidate_inventory {
        return Err(DirtyGenerationError::SourceChanged);
    }

    let material = keys
        .material_for(tenant_id, profile_id, candidate_generation_id)
        .map_err(|_| DirtyGenerationError::KeyUnavailable)?;
    let metadata = GenerationMetadata::for_plaintext(
        GenerationIdentity::new(
            tenant_id.clone(),
            profile_id.clone(),
            candidate_generation_id.clone(),
            Some(base_generation_id.clone()),
        ),
        material.dek.key_id().clone(),
        material.nonce_prefix,
        material.chunk_size,
        &snapshot,
    )
    .map_err(|_| DirtyGenerationError::EncryptionFailed)?;
    let sealed = seal_generation(&metadata, &material.dek, &snapshot)
        .map_err(|_| DirtyGenerationError::EncryptionFailed)?;
    let metadata_digest = digest_hex(sealed.metadata_digest().bytes());
    let container_digest = digest_hex(sealed.container_digest().bytes());

    Ok(PreparedDirtyGeneration {
        candidate_workspace: clone.workspace().clone(),
        candidate_inventory,
        sealed,
        metadata_digest,
        container_digest,
    })
}

fn encode_workspace_snapshot(
    workspace: &GenerationWorkspace,
    expected_inventory: &GenerationInventory,
) -> Result<Vec<u8>, DirtyGenerationError> {
    let entry_count = u32::try_from(expected_inventory.entries().len())
        .map_err(|_| DirtyGenerationError::SnapshotTooLarge)?;
    let mut output = Vec::new();
    output.extend_from_slice(SNAPSHOT_MAGIC);
    output.extend_from_slice(&entry_count.to_be_bytes());

    for entry in expected_inventory.entries() {
        let path = entry.relative_path().as_bytes();
        let path_length =
            u16::try_from(path.len()).map_err(|_| DirtyGenerationError::SnapshotTooLarge)?;
        checked_extend(&mut output, &path_length.to_be_bytes())?;
        checked_extend(&mut output, path)?;
        checked_extend(&mut output, &entry.bytes().to_be_bytes())?;

        let full_path = workspace.path().join(entry.relative_path());
        let metadata = std::fs::symlink_metadata(&full_path).map_err(LocalProfileError::from)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != entry.bytes()
        {
            return Err(DirtyGenerationError::SourceChanged);
        }
        let expected_bytes =
            usize::try_from(entry.bytes()).map_err(|_| DirtyGenerationError::SnapshotTooLarge)?;
        let remaining = MAX_SNAPSHOT_BYTES
            .checked_sub(output.len())
            .ok_or(DirtyGenerationError::SnapshotTooLarge)?;
        if expected_bytes > remaining {
            return Err(DirtyGenerationError::SnapshotTooLarge);
        }
        let mut file = File::open(&full_path).map_err(LocalProfileError::from)?;
        let start = output.len();
        output.resize(
            start
                .checked_add(expected_bytes)
                .ok_or(DirtyGenerationError::SnapshotTooLarge)?,
            0,
        );
        file.read_exact(&mut output[start..])
            .map_err(LocalProfileError::from)?;
        if fnv_digest(&output[start..]) != entry.content_digest() {
            return Err(DirtyGenerationError::SourceChanged);
        }
    }

    if workspace.inventory().map_err(DirtyGenerationError::Local)? != *expected_inventory {
        return Err(DirtyGenerationError::SourceChanged);
    }
    Ok(output)
}

fn checked_extend(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DirtyGenerationError> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(DirtyGenerationError::SnapshotTooLarge)?;
    if new_len > MAX_SNAPSHOT_BYTES {
        return Err(DirtyGenerationError::SnapshotTooLarge);
    }
    output.extend_from_slice(bytes);
    Ok(())
}

fn fnv_digest(bytes: &[u8]) -> u64 {
    let mut digest = FNV_OFFSET_BASIS;
    for byte in bytes {
        digest ^= u64::from(*byte);
        digest = digest.wrapping_mul(FNV_PRIME);
    }
    digest
}

fn digest_hex(bytes: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        DirtyGenerationError, GenerationSealingMaterial, GenerationSealingMaterialPort, digest_hex,
        prepare_dirty_generation_candidate,
    };
    use crate::local_profile::{LocalGenerationRecord, MaterializationRoot};
    use encrypted_generation_domain::{
        GenerationDek, KeyId, NoncePrefix, open_generation_expected,
    };
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId, UnixMillis};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct FakeKeys;

    impl GenerationSealingMaterialPort for FakeKeys {
        type Error = ();

        fn material_for(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            _generation_id: &GenerationId,
        ) -> Result<GenerationSealingMaterial, Self::Error> {
            Ok(GenerationSealingMaterial::new(
                GenerationDek::new(
                    KeyId::parse("key_dirty_generation_01").map_err(|_| ())?,
                    [9_u8; 32],
                ),
                NoncePrefix::new([7_u8; 16]),
                4096,
            ))
        }
    }

    struct Fixture {
        root_path: PathBuf,
        root: MaterializationRoot,
        tenant_id: TenantId,
        profile_id: ProfileId,
        base_generation_id: GenerationId,
        candidate_generation_id: GenerationId,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-dirty-generation-{}-{counter}",
                std::process::id()
            ));
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            Ok(Self {
                root_path,
                root,
                tenant_id: TenantId::parse(format!("tenant_01JDIRTY{counter}"))?,
                profile_id: ProfileId::parse(format!("profile_01JDIRTY{counter}"))?,
                base_generation_id: GenerationId::parse(format!("generation_01JBASE{counter}"))?,
                candidate_generation_id: GenerationId::parse(format!(
                    "generation_01JCANDIDATE{counter}"
                ))?,
            })
        }

        fn cleanup(&self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    fn dirty_record(
        generation_id: GenerationId,
    ) -> Result<LocalGenerationRecord, Box<dyn std::error::Error>> {
        let mut record = LocalGenerationRecord::new(generation_id, 0, UnixMillis::new(10));
        record.set_locked(true)?;
        record.begin_use(UnixMillis::new(11))?;
        record.graceful_close(UnixMillis::new(12))?;
        Ok(record)
    }

    #[test]
    fn dirty_workspace_becomes_new_encrypted_candidate_without_mutating_source()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let source = fixture.root.create_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.base_generation_id,
        )?;
        fs::create_dir_all(source.path().join("storage/default"))?;
        fs::write(source.path().join("prefs.js"), b"user_pref('mail', true);")?;
        fs::write(source.path().join("storage/default/state.bin"), b"state-v2")?;
        let source_before = source.inventory()?;
        let record = dirty_record(fixture.base_generation_id.clone())?;
        let mut keys = FakeKeys;

        let prepared = prepare_dirty_generation_candidate(
            &record,
            &source,
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.candidate_generation_id,
            &mut keys,
        )?;

        assert_eq!(source.inventory()?, source_before);
        assert_eq!(prepared.candidate_inventory(), &source_before);
        assert_eq!(prepared.metadata_digest().len(), 64);
        assert_eq!(
            prepared.metadata_digest(),
            digest_hex(prepared.sealed().metadata_digest().bytes())
        );
        assert_eq!(
            prepared.sealed().metadata_digest(),
            prepared.sealed().metadata().canonical_digest()?
        );
        assert_eq!(prepared.container_digest().len(), 64);
        assert!(
            prepared
                .object_key()
                .contains(fixture.candidate_generation_id.as_str())
        );
        let key = GenerationDek::new(KeyId::parse("key_dirty_generation_01")?, [9_u8; 32]);
        let opened = open_generation_expected(
            prepared.sealed().container(),
            &key,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.candidate_generation_id,
        )?;
        assert_eq!(
            opened.metadata().base_generation_id(),
            Some(&fixture.base_generation_id)
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn non_dirty_or_same_generation_candidate_fails_before_key_access()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let source = fixture.root.create_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.base_generation_id,
        )?;
        let clean =
            LocalGenerationRecord::new(fixture.base_generation_id.clone(), 0, UnixMillis::new(10));
        let mut keys = FakeKeys;
        assert_eq!(
            prepare_dirty_generation_candidate(
                &clean,
                &source,
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.candidate_generation_id,
                &mut keys,
            )
            .map(|_| ()),
            Err(DirtyGenerationError::InvalidState)
        );
        let dirty = dirty_record(fixture.base_generation_id.clone())?;
        assert_eq!(
            prepare_dirty_generation_candidate(
                &dirty,
                &source,
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.base_generation_id,
                &mut keys,
            )
            .map(|_| ()),
            Err(DirtyGenerationError::CandidateMatchesBase)
        );
        fixture.cleanup();
        Ok(())
    }
}
