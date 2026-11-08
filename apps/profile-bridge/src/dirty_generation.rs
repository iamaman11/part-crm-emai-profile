use crate::generation_snapshot::{GenerationSnapshotError, encode_workspace_snapshot};
use crate::local_profile::{
    GenerationInventory, GenerationWorkspace, LocalGenerationRecord, LocalGenerationState,
    LocalProfileError, MaterializationRoot, RecoveryClone,
};
use encrypted_generation_domain::{
    GenerationDek, GenerationIdentity, GenerationMetadata, NoncePrefix, PlaintextDigest,
    SealedGeneration, seal_generation,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use std::fmt;

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
    pub(crate) fn into_parts(self) -> (GenerationDek, NoncePrefix, u32) {
        (self.dek, self.nonce_prefix, self.chunk_size)
    }
    #[must_use]
    pub const fn new(dek: GenerationDek, nonce_prefix: NoncePrefix, chunk_size: u32) -> Self {
        Self {
            dek,
            nonce_prefix,
            chunk_size,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SealingMaterialFailure {
    Unavailable,
    Transport,
    ServerUncertain,
    Rejected,
}

pub trait GenerationSealingMaterialPort {
    type Error;

    /// Default for local/test sealers; the remote adapter refines transport
    /// failures without ever granting permission to replay a sealing request.
    fn classify_error(_error: &Self::Error) -> SealingMaterialFailure {
        SealingMaterialFailure::Unavailable
    }

    fn material_for(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        generation_id: &GenerationId,
        plaintext_digest: [u8; 32],
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

/// Reuse the existing encrypted successor format for a CLOSED local-first
/// CURRENT. Unlike the former generation-folder model, this source folder
/// remains stable: there is no secondary native browser workspace or second
/// generation directory to create. Caller snapshots under the same writer lock.
#[cfg(any(windows, test))]
#[allow(clippy::too_many_arguments)]
pub fn prepare_closed_current_successor<K: GenerationSealingMaterialPort>(
    source: &GenerationWorkspace,
    original_inventory: &GenerationInventory,
    snapshot: &[u8],
    tenant: &TenantId,
    profile: &ProfileId,
    base: &GenerationId,
    keys: &mut K,
) -> Result<PreparedDirtyGeneration, DirtyGenerationError> {
    use sha2::{Digest, Sha256};
    if source.tenant_id() != tenant
        || source.profile_id() != profile
        || source.base_generation_id() != Some(base)
        || source.inventory()? != *original_inventory
    {
        return Err(DirtyGenerationError::IdentityMismatch);
    }
    let digest = PlaintextDigest::calculate(snapshot).bytes();
    let mut hash = Sha256::new();
    hash.update(b"profile-bridge/closed-current-successor/v2\0");
    for part in [
        tenant.as_str().as_bytes(),
        profile.as_str().as_bytes(),
        base.as_str().as_bytes(),
        &digest,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    let candidate_id = GenerationId::parse(format!(
        "generation_{}", digest_hex(hash.finalize().into()),
    ))
    .map_err(|_| DirtyGenerationError::IdentityMismatch)?;
    if &candidate_id == base {
        return Err(DirtyGenerationError::CandidateMatchesBase);
    }
    let material = keys
        .material_for(tenant, profile, base, &candidate_id, digest)
        .map_err(|_| DirtyGenerationError::KeyUnavailable)?;
    let metadata = GenerationMetadata::for_plaintext(
        GenerationIdentity::new(
            tenant.clone(), profile.clone(), candidate_id, Some(base.clone()),
        ),
        material.dek.key_id().clone(),
        material.nonce_prefix,
        material.chunk_size,
        snapshot,
    )
    .map_err(|_| DirtyGenerationError::EncryptionFailed)?;
    let sealed = seal_generation(&metadata, &material.dek, snapshot)
        .map_err(|_| DirtyGenerationError::EncryptionFailed)?;
    if source.inventory()? != *original_inventory {
        return Err(DirtyGenerationError::SourceChanged);
    }
    Ok(PreparedDirtyGeneration {
        candidate_workspace: source.clone(),
        candidate_inventory: original_inventory.clone(),
        metadata_digest: digest_hex(sealed.metadata_digest().bytes()),
        container_digest: digest_hex(sealed.container_digest().bytes()),
        sealed,
    })
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
    prepare_dirty_generation_candidate_inner(
        local_record,
        source_workspace,
        root,
        tenant_id,
        profile_id,
        candidate_generation_id,
        keys,
        false,
    )
}

#[cfg(any(windows, test))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn recover_local_unsynced_native_clone(
    record: &LocalGenerationRecord,
    source: &GenerationWorkspace,
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    candidate: &GenerationId,
) -> Result<GenerationWorkspace, DirtyGenerationError> {
    let (workspace, _) =
        create_verified_native_candidate(record, source, root, tenant, profile, candidate, true)?;
    Ok(workspace)
}

/// Shared original native clone owner for small Save and large recovered Save.
/// A crashed N+1 clone is reused only if its whole inventory is unchanged.
#[allow(clippy::too_many_arguments)]
fn create_verified_native_candidate(
    local_record: &LocalGenerationRecord,
    source_workspace: &GenerationWorkspace,
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    candidate_generation_id: &GenerationId,
    allow_preexisting_verified_clone: bool,
) -> Result<(GenerationWorkspace, GenerationInventory), DirtyGenerationError> {
    if local_record.state() != LocalGenerationState::DirtyLocal || !local_record.is_locked() {
        return Err(DirtyGenerationError::InvalidState);
    }
    let base_generation_id = local_record.generation_id();
    if base_generation_id == candidate_generation_id {
        return Err(DirtyGenerationError::CandidateMatchesBase);
    }
    if source_workspace.generation_id() != base_generation_id
        || source_workspace.tenant_id() != tenant_id
        || source_workspace.profile_id() != profile_id
    {
        return Err(DirtyGenerationError::IdentityMismatch);
    }

    #[cfg(windows)]
    let native_clone_started = std::time::Instant::now();
    let (candidate_workspace, candidate_inventory) = match RecoveryClone::create(
        source_workspace,
        root,
        tenant_id,
        profile_id,
        candidate_generation_id,
    ) {
        Ok(clone) => (clone.workspace().clone(), clone.verify_clone_only()?),
        Err(LocalProfileError::TargetAlreadyExists) if allow_preexisting_verified_clone => {
            // A complete clone may survive a crash BEFORE sealed checkpoint fsync.
            // No replacement, repair, unlink or second candidate is permitted.
            // Any torn/different Firefox file leaves the old LOCAL_UNSYNCED
            // evidence untouched and blocks this attempt.
            let existing = root.open_generation(tenant_id, profile_id, candidate_generation_id)?;
            let original_inventory = source_workspace.inventory()?;
            if existing.inventory()? != original_inventory {
                return Err(DirtyGenerationError::SourceChanged);
            }
            (existing, original_inventory)
        }
        Err(error) => return Err(DirtyGenerationError::Local(error)),
    };
    if source_workspace.inventory()? != candidate_inventory
        || candidate_workspace.inventory()? != candidate_inventory
    {
        return Err(DirtyGenerationError::SourceChanged);
    }
    #[cfg(windows)]
    eprintln!(
        "bridge_save_subphase name=native_clone_and_integrity elapsed_ms={}",
        native_clone_started.elapsed().as_millis()
    );
    Ok((candidate_workspace, candidate_inventory))
}

#[allow(clippy::too_many_arguments)]
fn prepare_dirty_generation_candidate_inner<K: GenerationSealingMaterialPort>(
    local_record: &LocalGenerationRecord,
    source_workspace: &GenerationWorkspace,
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    candidate_generation_id: &GenerationId,
    keys: &mut K,
    allow_preexisting_verified_clone: bool,
) -> Result<PreparedDirtyGeneration, DirtyGenerationError> {
    let base_generation_id = local_record.generation_id();
    #[cfg(windows)]
    let snapshot_started = std::time::Instant::now();
    let (candidate_workspace, candidate_inventory) = create_verified_native_candidate(
        local_record,
        source_workspace,
        root,
        tenant_id,
        profile_id,
        candidate_generation_id,
        allow_preexisting_verified_clone,
    )?;
    let snapshot = encode_workspace_snapshot(&candidate_workspace, &candidate_inventory)
        .map_err(map_snapshot_encode_error)?;
    if candidate_workspace.inventory()? != candidate_inventory
        || source_workspace.inventory()? != candidate_inventory
    {
        return Err(DirtyGenerationError::SourceChanged);
    }
    let plaintext_digest = PlaintextDigest::calculate(&snapshot).bytes();
    #[cfg(windows)]
    eprintln!(
        "bridge_save_subphase name=snapshot_and_verify elapsed_ms={}",
        snapshot_started.elapsed().as_millis()
    );

    #[cfg(windows)]
    let key_material_started = std::time::Instant::now();
    let material = keys
        .material_for(
            tenant_id,
            profile_id,
            base_generation_id,
            candidate_generation_id,
            plaintext_digest,
        )
        .map_err(|_| DirtyGenerationError::KeyUnavailable)?;
    #[cfg(windows)]
    eprintln!(
        "bridge_save_subphase name=sealing_material_request elapsed_ms={}",
        key_material_started.elapsed().as_millis()
    );
    #[cfg(windows)]
    let sealing_started = std::time::Instant::now();
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
    #[cfg(windows)]
    eprintln!(
        "bridge_save_subphase name=seal_and_digest elapsed_ms={}",
        sealing_started.elapsed().as_millis()
    );

    Ok(PreparedDirtyGeneration {
        candidate_workspace,
        candidate_inventory,
        sealed,
        metadata_digest,
        container_digest,
    })
}

fn map_snapshot_encode_error(error: GenerationSnapshotError) -> DirtyGenerationError {
    match error {
        GenerationSnapshotError::SnapshotTooLarge => DirtyGenerationError::SnapshotTooLarge,
        GenerationSnapshotError::Local(error) => DirtyGenerationError::Local(error),
        GenerationSnapshotError::SourceChanged
        | GenerationSnapshotError::InvalidFormat
        | GenerationSnapshotError::UnsafePath
        | GenerationSnapshotError::TargetAlreadyExists
        | GenerationSnapshotError::PrePublish => DirtyGenerationError::SourceChanged,
    }
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
        prepare_closed_current_successor, prepare_dirty_generation_candidate,
    };
    use crate::generation_snapshot::encode_workspace_snapshot;
    use crate::local_profile::{LocalGenerationRecord, MaterializationRoot};
    use encrypted_generation_domain::{
        GenerationDek, KeyId, NoncePrefix, open_generation_expected,
    };
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId, UnixMillis};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[derive(Default)]
    struct FakeKeys {
        observed_base_generation_id: Option<GenerationId>,
        observed_plaintext_digest: Option<[u8; 32]>,
    }

    impl GenerationSealingMaterialPort for FakeKeys {
        type Error = ();

        fn material_for(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            base_generation_id: &GenerationId,
            _generation_id: &GenerationId,
            plaintext_digest: [u8; 32],
        ) -> Result<GenerationSealingMaterial, Self::Error> {
            self.observed_base_generation_id = Some(base_generation_id.clone());
            self.observed_plaintext_digest = Some(plaintext_digest);
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
    fn closed_current_uses_existing_successor_cipher_and_rejects_mutated_source()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let raw = fixture.root.create_profile_current(
            &fixture.tenant_id, &fixture.profile_id, &fixture.base_generation_id, 1,
        )?;
        fs::write(raw.path().join("cookies.sqlite"), b"cookie data")?;
        let source = fixture.root.update_profile_current_sync_base(
            &raw, &fixture.base_generation_id, 2,
        )?;
        let before = source.inventory()?;
        let plaintext = encode_workspace_snapshot(&source, &before)?;
        let mut keys = FakeKeys::default();
        let first = prepare_closed_current_successor(
            &source, &before, &plaintext, &fixture.tenant_id,
            &fixture.profile_id, &fixture.base_generation_id, &mut keys,
        )?;
        let second = prepare_closed_current_successor(
            &source, &before, &plaintext, &fixture.tenant_id,
            &fixture.profile_id, &fixture.base_generation_id, &mut keys,
        )?;
        assert_eq!(
            first.sealed().metadata().generation_id(),
            second.sealed().metadata().generation_id(),
        );
        assert_eq!(
            first.sealed().metadata().base_generation_id(),
            Some(&fixture.base_generation_id),
        );
        assert_eq!(
            keys.observed_base_generation_id.as_ref(),
            Some(&fixture.base_generation_id),
        );
        assert_eq!(source.path(), first.candidate_workspace().path());
        fs::write(source.path().join("cookies.sqlite"), b"new local change")?;
        assert!(matches!(
            prepare_closed_current_successor(
                &source, &before, &plaintext, &fixture.tenant_id,
                &fixture.profile_id, &fixture.base_generation_id, &mut keys,
            ),
            Err(DirtyGenerationError::IdentityMismatch),
        ));
        assert_eq!(fs::read(source.path().join("cookies.sqlite"))?, b"new local change");
        fixture.cleanup();
        Ok(())
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
        let mut keys = FakeKeys::default();

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
        assert_eq!(
            keys.observed_base_generation_id.as_ref(),
            Some(&fixture.base_generation_id)
        );
        assert_eq!(
            keys.observed_plaintext_digest,
            Some(prepared.sealed().metadata().plaintext_digest().bytes())
        );
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
        let mut keys = FakeKeys::default();
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
