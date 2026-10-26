//! Read-only, crash-surviving evidence for an exact P3 successor attempt.
//!
//! This is NOT an authentication token, a coordinator lease, or permission to
//! call PUT/commit after restart. Only the live retained owner can publish.
//! Recovery must obtain a fresh authoritative, ACL/fencing-safe decision before
//! using these exact bytes. Never remove an unknown record during cleanup.
use crate::dirty_generation::{GenerationSealingMaterialPort, PreparedDirtyGeneration};
use crate::generation_object_spool::VerifiedGenerationObjectFile;
use crate::generation_snapshot::WorkspaceSnapshotReader;
use crate::local_profile::{GenerationWorkspace, MaterializationRoot};
use crate::shipping_generation_save::CommittedGenerationSuccessor;
use encrypted_generation_domain::{
    GenerationIdentity, GenerationMetadata, MAX_GENERATION_CONTAINER_BYTES,
    MAX_GENERATION_METADATA_PRELUDE_BYTES, MAX_STREAM_CONTAINER_BYTES, StreamingGenerationReport,
    inspect_generation_metadata_prelude, seal_generation_streaming,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use serde::{Deserialize, Serialize};
use session_domain::ProfileLease;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: &[u8; 8] = b"BPAT0001";
const MAX_HEADER_BYTES: usize = 4_096;
const MAX_RECORD_BYTES: usize = MAX_GENERATION_CONTAINER_BYTES + MAX_HEADER_BYTES + 12;
const MAX_STREAM_RECORD_BYTES: u64 = MAX_STREAM_CONTAINER_BYTES + MAX_HEADER_BYTES as u64 + 12;
const STAGE_TRIES: usize = 16;
static STAGE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SaveAttemptEvidenceError {
    Io(std::io::ErrorKind),
    Invalid,
    Changed,
    Unavailable,
}

impl From<std::io::Error> for SaveAttemptEvidenceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.kind())
    }
}

impl std::fmt::Display for SaveAttemptEvidenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(kind) => write!(formatter, "save-attempt local I/O failure: {kind:?}"),
            Self::Invalid => formatter.write_str("save-attempt evidence is invalid"),
            Self::Changed => formatter.write_str("save-attempt workspace or bytes changed"),
            Self::Unavailable => formatter.write_str("save-attempt atomic staging unavailable"),
        }
    }
}

impl std::error::Error for SaveAttemptEvidenceError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptHeader {
    schema: String,
    tenant: String,
    profile: String,
    base_generation: String,
    successor: String,
    session_fingerprint: String,
    fencing_fingerprint: String,
    /// Original-device fingerprint; legacy records may be inspected but
    /// cannot authorize an authenticated cross-session cloud retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    device_fingerprint: Option<String>,
    epoch: u64,
    source_inventory: u64,
    candidate_inventory: u64,
    candidate_bytes: u64,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
    /// A first sealed checkpoint made from an older lone LOCAL_UNSYNCED
    /// under a NEW writer lease. Not a reusable original fence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovered_lone_origin: Option<RecoveredLoneOrigin>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveredLoneOrigin {
    epoch: u64,
    session_fingerprint: String,
    fencing_fingerprint: String,
}

/// A verified *local copy* of encrypted bytes, not proof of a server commit or
/// browser usability. The original lease is descriptive evidence only.
pub(crate) struct ReadOnlySaveAttempt {
    header: AttemptHeader,
    ciphertext: Vec<u8>,
}

impl ReadOnlySaveAttempt {
    #[cfg(any(windows, test))]
    #[must_use]
    pub(crate) fn prior_epoch(&self) -> u64 {
        self.header.epoch
    }

    #[cfg(any(windows, test))]
    #[must_use]
    pub(crate) fn matches_fresh_device(&self, lease: &ProfileLease) -> bool {
        let digest = credential_fingerprint(b"device", lease.device_id().as_str());
        self.header.device_fingerprint.as_deref() == Some(digest.as_str())
    }

    /// Only a first checkpoint sealed FROM an older local intent may use
    /// the same freshly claimed writer during its initial upload. Ordinary
    /// crash replay still requires a STRICTLY newer lease.
    #[cfg(any(windows, test))]
    pub(crate) fn first_seal_under_current_lease(&self, lease: &ProfileLease) -> bool {
        self.header.recovered_lone_origin.is_some() && self.matches_original_lease(lease)
    }

    #[must_use]
    pub(crate) fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    #[must_use]
    pub(crate) fn container_digest(&self) -> &str {
        &self.header.container_digest
    }

    #[must_use]
    pub(crate) fn base_generation_id(&self) -> &str {
        &self.header.base_generation
    }

    #[must_use]
    pub(crate) fn successor_id(&self) -> &str {
        &self.header.successor
    }

    #[must_use]
    pub(crate) fn object_key(&self) -> &str {
        &self.header.object_key
    }

    #[must_use]
    pub(crate) fn metadata_digest(&self) -> &str {
        &self.header.metadata_digest
    }

    #[must_use]
    pub(crate) fn container_bytes(&self) -> u64 {
        self.header.container_bytes
    }

    /// The original fencing token and session ID must never be saved verbatim
    /// as reusable credentials or exposed through diagnostic records.
    #[must_use]
    pub(crate) fn matches_original_lease(&self, lease: &ProfileLease) -> bool {
        self.header.session_fingerprint
            == credential_fingerprint(b"session", lease.session_id().as_str())
            && self.header.fencing_fingerprint
                == credential_fingerprint(b"fencing", lease.fencing_token().as_str())
            && self.header.epoch == lease.epoch()
    }
}

/// Minimal LOCAL_UNSYNCED *evidence*, not another save authority or a
/// separately snapshotted generation. This small same-root record is durable
/// BEFORE any network-dependent sealed candidate work. The original Firefox
/// working bytes remain in their original generation directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalUnsyncedIntent {
    schema: String,
    tenant: String,
    profile: String,
    base_generation: String,
    epoch: u64,
    session_fingerprint: String,
    fencing_fingerprint: String,
    /// Original-device fingerprint; legacy records may be inspected but
    /// cannot authorize an authenticated cross-session cloud retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    device_fingerprint: Option<String>,
    source_inventory: u64,
    source_bytes: u64,
}

const MAX_LOCAL_UNSYNCED_BYTES: usize = 2_048;

/// Called only by the existing operator after the native browser has closed
/// and while its original Bridge workspace lock is retained. Never stores
/// an expired lease token, cookies, credentials, or a full workspace clone.
/// A canonical different/torn/foreign file remains a hard blocker.
pub(crate) fn persist_local_unsynced_intent(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    lease: &ProfileLease,
) -> Result<(), SaveAttemptEvidenceError> {
    if source.tenant_id() != lease.tenant_id()
        || source.profile_id() != lease.profile_id()
        || lease.epoch() == 0
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let intent = LocalUnsyncedIntent {
        schema: "bridge-p3-local-unsynced-v1".to_owned(),
        tenant: lease.tenant_id().as_str().to_owned(),
        profile: lease.profile_id().as_str().to_owned(),
        base_generation: source.generation_id().as_str().to_owned(),
        epoch: lease.epoch(),
        session_fingerprint: credential_fingerprint(b"session", lease.session_id().as_str()),
        fencing_fingerprint: credential_fingerprint(b"fencing", lease.fencing_token().as_str()),
        device_fingerprint: Some(credential_fingerprint(
            b"device",
            lease.device_id().as_str(),
        )),
        source_inventory: inventory.inventory_digest(),
        source_bytes: inventory.total_bytes(),
    };
    let bytes = serde_json::to_vec(&intent).map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    if bytes.is_empty() || bytes.len() > MAX_LOCAL_UNSYNCED_BYTES {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let canonical = local_unsynced_path(root, lease.tenant_id(), lease.profile_id());
    match fs::symlink_metadata(&canonical) {
        Ok(_) => {
            if read_local_unsynced(&canonical)? == intent {
                return Ok(());
            }
            return Err(SaveAttemptEvidenceError::Changed);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let mut stage = None;
    for _ in 0..STAGE_TRIES {
        let seq = STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = root.path().join(format!(
            "{}.pending-local-unsynced-{}-{seq}",
            scope_file_prefix(lease.tenant_id(), lease.profile_id()),
            std::process::id()
        ));
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => {
                stage = Some((path, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let (stage_path, mut file) = stage.ok_or(SaveAttemptEvidenceError::Unavailable)?;
    let written = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = written {
        // A torn staged record remains scoped blocking evidence. No browser
        // bytes, native WAL, identity, or unrelated Profile are touched.
        return Err(error.into());
    }
    finish_local_unsynced_publication(
        &stage_path,
        &canonical,
        &intent,
        fs::hard_link(&stage_path, &canonical),
    )
}

/// Never unlink the last fsynced local-save evidence if the no-replace
/// publication fails. A .pending-local-unsynced record is a hard blocker on
/// restart, not an authority to open N or retry a cloud effect.
fn finish_local_unsynced_publication(
    stage: &PathBuf,
    canonical: &PathBuf,
    intent: &LocalUnsyncedIntent,
    publication: std::io::Result<()>,
) -> Result<(), SaveAttemptEvidenceError> {
    match publication {
        Ok(()) => {
            fs::remove_file(stage)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_local_unsynced(canonical)? != *intent {
                return Err(SaveAttemptEvidenceError::Changed);
            }
            // A byte-identical canonical marker already provides the
            // durable intent. Failure to unlink stage remains fail-closed.
            fs::remove_file(stage)?;
            Ok(())
        }
        // The staged marker is the ONLY durable local intent here. Preserve
        // it even on permissions, volume, antivirus or power-loss errors.
        Err(error) => Err(error.into()),
    }
}

/// Retire the small local-close intent only AFTER the entire exact encrypted
/// candidate and its immutable checkpoint are independently verified on disk.
/// From then on that canonical checkpoint is the sole unresolved-save evidence.
/// This never retires the ciphertext checkpoint or grants any cloud write.
pub(crate) fn retire_local_unsynced_after_exact_checkpoint(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    lease: &ProfileLease,
    checkpoint: &ReadOnlySaveAttempt,
) -> Result<(), SaveAttemptEvidenceError> {
    let path = local_unsynced_path(root, lease.tenant_id(), lease.profile_id());
    let intent = match read_local_unsynced(&path) {
        Ok(value) => value,
        Err(SaveAttemptEvidenceError::Io(std::io::ErrorKind::NotFound)) => return Ok(()),
        Err(error) => return Err(error),
    };
    let inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if intent.schema != "bridge-p3-local-unsynced-v1"
        || intent.tenant != lease.tenant_id().as_str()
        || intent.profile != lease.profile_id().as_str()
        || intent.base_generation != source.generation_id().as_str()
        || intent.epoch != lease.epoch()
        || intent.session_fingerprint
            != credential_fingerprint(b"session", lease.session_id().as_str())
        || intent.fencing_fingerprint
            != credential_fingerprint(b"fencing", lease.fencing_token().as_str())
        || intent.device_fingerprint.as_deref()
            != Some(credential_fingerprint(b"device", lease.device_id().as_str()).as_str())
        || checkpoint.header.device_fingerprint != intent.device_fingerprint
        || intent.source_inventory != inventory.inventory_digest()
        || intent.source_bytes != inventory.total_bytes()
        || checkpoint.header.tenant != intent.tenant
        || checkpoint.header.profile != intent.profile
        || checkpoint.header.base_generation != intent.base_generation
        || checkpoint.header.source_inventory != intent.source_inventory
        || checkpoint.header.candidate_bytes != intent.source_bytes
        || !checkpoint.matches_original_lease(lease)
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    // No sweep. If deletion fails, fail closed and keep both records.
    fs::remove_file(path)?;
    Ok(())
}

/// A crash between the complete exact checkpoint hardlink and small intent
/// retirement legitimately leaves TWO records for the SAME attempt. This
/// proves an exact pairing without a surviving old lease or replayable token.
/// It NEVER implies D1 ACTIVE, network retry permission, or browser readiness.
fn matching_local_unsynced_for_exact(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    exact: &ReadOnlySaveAttempt,
) -> Result<bool, SaveAttemptEvidenceError> {
    let marker = local_unsynced_path(root, tenant, profile);
    let intent = match read_local_unsynced(&marker) {
        Ok(intent) => intent,
        Err(SaveAttemptEvidenceError::Io(std::io::ErrorKind::NotFound)) => return Ok(false),
        Err(error) => return Err(error),
    };
    let originally_sealed = exact.header.recovered_lone_origin.is_none()
        && exact.header.epoch == intent.epoch
        && exact.header.session_fingerprint == intent.session_fingerprint
        && exact.header.fencing_fingerprint == intent.fencing_fingerprint;
    let recovered_seal = exact
        .header
        .recovered_lone_origin
        .as_ref()
        .is_some_and(|origin| {
            origin.epoch == intent.epoch
                && origin.session_fingerprint == intent.session_fingerprint
                && origin.fencing_fingerprint == intent.fencing_fingerprint
                && exact.header.epoch > origin.epoch
                && deterministic_lone_successor(&intent)
                    .is_ok_and(|candidate| candidate.as_str() == exact.successor_id())
        });
    if intent.schema != "bridge-p3-local-unsynced-v1"
        || intent.tenant != tenant.as_str()
        || intent.profile != profile.as_str()
        || exact.header.schema != "bridge-p3-attempt-v1"
        || exact.header.tenant != intent.tenant
        || exact.header.profile != intent.profile
        || exact.header.base_generation != intent.base_generation
        || !(originally_sealed || recovered_seal)
        || exact.header.device_fingerprint != intent.device_fingerprint
        || exact.header.source_inventory != intent.source_inventory
        || exact.header.candidate_bytes != intent.source_bytes
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let base_id = GenerationId::parse(intent.base_generation)
        .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    let source = root
        .open_generation(tenant, profile, &base_id)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if inventory.inventory_digest() != intent.source_inventory
        || inventory.total_bytes() != intent.source_bytes
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    Ok(true)
}

/// The existing exact checkpoint has already been checked under the fresh
/// native writer/Bridge root admission after affirmative D1 ACTIVE_EXACT.
/// Retire only its matching predecessor intent; never touch Firefox files
/// or the complete ciphertext checkpoint needed after a further crash.
#[cfg(any(windows, test))]
pub(crate) fn retire_matching_local_unsynced_after_restart(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    exact: &ReadOnlySaveAttempt,
) -> Result<(), SaveAttemptEvidenceError> {
    if !matching_local_unsynced_for_exact(root, tenant, profile, exact)? {
        return Ok(());
    }
    fs::remove_file(local_unsynced_path(root, tenant, profile))?;
    Ok(())
}

/// Read-only original-device provenance proof for an unsealed local close.
/// The previous session/fence is never recoverable or re-authorized.
#[cfg(any(windows, test))]
pub(crate) fn local_unsynced_matches_fresh_lease(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    base: &GenerationId,
    fresh_lease: &ProfileLease,
) -> Result<bool, SaveAttemptEvidenceError> {
    let intent = read_local_unsynced(&local_unsynced_path(root, tenant, profile))?;
    if intent.schema != "bridge-p3-local-unsynced-v1"
        || intent.tenant != tenant.as_str()
        || intent.profile != profile.as_str()
        || intent.base_generation != base.as_str()
        || intent.epoch == 0
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let device_digest = credential_fingerprint(b"device", fresh_lease.device_id().as_str());
    Ok(fresh_lease.tenant_id() == tenant
        && fresh_lease.profile_id() == profile
        && fresh_lease.epoch() > intent.epoch
        && intent.device_fingerprint.as_deref() == Some(device_digest.as_str()))
}

/// At the FIRST controlled close derive the same immutable N+1 as all later
/// renewed-lease recovery attempts. A crash after clone creation but before
/// checkpoint publication must NEVER change the candidate ID.
pub(crate) fn original_local_unsynced_successor(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    lease: &ProfileLease,
) -> Result<Option<GenerationId>, SaveAttemptEvidenceError> {
    let tenant = lease.tenant_id();
    let profile = lease.profile_id();
    let intent = match read_local_unsynced(&local_unsynced_path(root, tenant, profile)) {
        Ok(value) => value,
        Err(SaveAttemptEvidenceError::Io(std::io::ErrorKind::NotFound)) => {
            return if has_unresolved_save_attempt_for_profile(root, tenant, profile)? {
                Err(SaveAttemptEvidenceError::Changed)
            } else {
                Ok(None)
            };
        }
        Err(error) => return Err(error),
    };
    // A canonical intent alongside a torn stage or competing exact attempt
    // is NOT a clean new-save permission. Refuse a new candidate early.
    if !matches!(
        classify_scoped_pending_save_evidence(root, tenant, profile),
        Ok(ScopedPendingSaveEvidence::LocalUnsynced {
            base_generation,
            prior_epoch,
        }) if base_generation == *source.generation_id() && prior_epoch == lease.epoch()
    ) {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if intent.schema != "bridge-p3-local-unsynced-v1"
        || intent.tenant != tenant.as_str()
        || intent.profile != profile.as_str()
        || intent.base_generation != source.generation_id().as_str()
        || intent.epoch != lease.epoch()
        || intent.epoch == 0
        || intent.session_fingerprint
            != credential_fingerprint(b"session", lease.session_id().as_str())
        || intent.fencing_fingerprint
            != credential_fingerprint(b"fencing", lease.fencing_token().as_str())
        || intent.device_fingerprint.as_deref()
            != Some(credential_fingerprint(b"device", lease.device_id().as_str()).as_str())
        || intent.source_inventory != inventory.inventory_digest()
        || intent.source_bytes != inventory.total_bytes()
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    Ok(Some(deterministic_lone_successor(&intent)?))
}

/// The generation is fixed by original immutable local-close evidence,
/// NOT by a subsequently renewed session/fence. Repeated authorized startups
/// can never silently choose N+2 after a partly created N+1.
#[cfg(any(windows, test))]
pub(crate) fn deterministic_lone_unsynced_successor(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    base: &GenerationId,
    fresh_lease: &ProfileLease,
) -> Result<Option<GenerationId>, SaveAttemptEvidenceError> {
    if !local_unsynced_matches_fresh_lease(root, tenant, profile, base, fresh_lease)? {
        return Ok(None);
    }
    let intent = read_local_unsynced(&local_unsynced_path(root, tenant, profile))?;
    Ok(Some(deterministic_lone_successor(&intent)?))
}

fn deterministic_lone_successor(
    intent: &LocalUnsyncedIntent,
) -> Result<GenerationId, SaveAttemptEvidenceError> {
    let mut hash = Sha256::new();
    hash.update(b"bridge-p3-original-lone-intent-successor-v1");
    for value in [
        intent.tenant.as_bytes(),
        intent.profile.as_bytes(),
        intent.base_generation.as_bytes(),
        intent.session_fingerprint.as_bytes(),
        intent.fencing_fingerprint.as_bytes(),
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
    hash.update(intent.epoch.to_be_bytes());
    hash.update(intent.source_inventory.to_be_bytes());
    hash.update(intent.source_bytes.to_be_bytes());
    GenerationId::parse(format!("generation_{}", hex(&hash.finalize())))
        .map_err(|_| SaveAttemptEvidenceError::Invalid)
}

fn local_unsynced_path(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
) -> PathBuf {
    root.path().join(format!(
        "{}-local-unsynced-v1.intent",
        scope_file_prefix(tenant, profile)
    ))
}

fn read_local_unsynced(path: &PathBuf) -> Result<LocalUnsyncedIntent, SaveAttemptEvidenceError> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink()
        || !meta.is_file()
        || meta.len() == 0
        || meta.len() > MAX_LOCAL_UNSYNCED_BYTES as u64
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take((MAX_LOCAL_UNSYNCED_BYTES as u64) + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > MAX_LOCAL_UNSYNCED_BYTES {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    serde_json::from_slice(&bytes).map_err(|_| SaveAttemptEvidenceError::Invalid)
}

/// No-replace, fsynced publication outside the generation workspace/snapshot.
/// A pre-existing different record (including corrupted/truncated/symlink)
/// permanently blocks this ordinary save attempt, without deleting any bytes.
pub(crate) fn persist_exact_save_attempt(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    prepared: &PreparedDirtyGeneration,
    lease: &ProfileLease,
) -> Result<(), SaveAttemptEvidenceError> {
    persist_exact_save_attempt_with_origin(root, source, prepared, lease, None)
}

/// First checkpoint from a preexisting local-close intent after a fresh writer
/// has revalidated the native inventory and the deterministic N+1. The old
/// intent stays until authoritative ACTIVE_EXACT local completion; a crash
/// after fsync therefore leaves BOTH correctly paired records.
#[cfg(any(windows, test))]
pub(crate) fn persist_exact_save_attempt_from_lone_intent(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    prepared: &PreparedDirtyGeneration,
    fresh_lease: &ProfileLease,
) -> Result<(), SaveAttemptEvidenceError> {
    let tenant = fresh_lease.tenant_id();
    let profile = fresh_lease.profile_id();
    let candidate = deterministic_lone_unsynced_successor(
        root,
        tenant,
        profile,
        source.generation_id(),
        fresh_lease,
    )?
    .ok_or(SaveAttemptEvidenceError::Changed)?;
    if &candidate != prepared.candidate_workspace().generation_id() {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let intent = read_local_unsynced(&local_unsynced_path(root, tenant, profile))?;
    let inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if inventory.inventory_digest() != intent.source_inventory
        || inventory.total_bytes() != intent.source_bytes
        || intent.device_fingerprint.as_deref()
            != Some(credential_fingerprint(b"device", fresh_lease.device_id().as_str()).as_str())
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let origin = RecoveredLoneOrigin {
        epoch: intent.epoch,
        session_fingerprint: intent.session_fingerprint,
        fencing_fingerprint: intent.fencing_fingerprint,
    };
    persist_exact_save_attempt_with_origin(root, source, prepared, fresh_lease, Some(origin))
}

fn persist_exact_save_attempt_with_origin(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    prepared: &PreparedDirtyGeneration,
    lease: &ProfileLease,
    recovered_lone_origin: Option<RecoveredLoneOrigin>,
) -> Result<(), SaveAttemptEvidenceError> {
    let candidate = prepared.candidate_workspace();
    let metadata = prepared.sealed().metadata();
    if lease.tenant_id() != candidate.tenant_id()
        || lease.profile_id() != candidate.profile_id()
        || metadata.tenant_id() != candidate.tenant_id()
        || metadata.profile_id() != candidate.profile_id()
        || metadata.generation_id() != candidate.generation_id()
        || metadata.base_generation_id() != Some(source.generation_id())
        || source.tenant_id() != candidate.tenant_id()
        || source.profile_id() != candidate.profile_id()
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let source_inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let candidate_inventory = candidate
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if source_inventory != *prepared.candidate_inventory()
        || candidate_inventory != *prepared.candidate_inventory()
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let ciphertext = prepared.sealed().container();
    let header = AttemptHeader {
        schema: "bridge-p3-attempt-v1".to_owned(),
        tenant: lease.tenant_id().as_str().to_owned(),
        profile: lease.profile_id().as_str().to_owned(),
        base_generation: source.generation_id().as_str().to_owned(),
        successor: candidate.generation_id().as_str().to_owned(),
        session_fingerprint: credential_fingerprint(b"session", lease.session_id().as_str()),
        fencing_fingerprint: credential_fingerprint(b"fencing", lease.fencing_token().as_str()),
        device_fingerprint: Some(credential_fingerprint(
            b"device",
            lease.device_id().as_str(),
        )),
        epoch: lease.epoch(),
        source_inventory: source_inventory.inventory_digest(),
        candidate_inventory: candidate_inventory.inventory_digest(),
        candidate_bytes: candidate_inventory.total_bytes(),
        object_key: prepared.object_key(),
        metadata_digest: prepared.metadata_digest().to_owned(),
        container_digest: prepared.container_digest().to_owned(),
        container_bytes: u64::try_from(ciphertext.len())
            .map_err(|_| SaveAttemptEvidenceError::Invalid)?,
        recovered_lone_origin,
    };
    let header_bytes =
        serde_json::to_vec(&header).map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    let size = 12_usize
        .checked_add(header_bytes.len())
        .and_then(|n| n.checked_add(ciphertext.len()))
        .ok_or(SaveAttemptEvidenceError::Invalid)?;
    if header_bytes.is_empty()
        || header_bytes.len() > MAX_HEADER_BYTES
        || ciphertext.is_empty()
        || ciphertext.len() > MAX_GENERATION_CONTAINER_BYTES
        || size > MAX_RECORD_BYTES
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }

    let canonical = attempt_path(
        root,
        candidate.tenant_id(),
        candidate.profile_id(),
        candidate.generation_id(),
    );
    // Idempotence is only byte-for-byte: a different header, key, nonce or
    // ciphertext under the same deterministic attempt never gets overwritten.
    match fs::symlink_metadata(&canonical) {
        Ok(_) => return verify_existing_exact(&canonical, &header_bytes, ciphertext),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let mut staged = None;
    for _ in 0..STAGE_TRIES {
        let sequence = STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        // Scope the pending staging name as well: an interrupted file write
        // must block only its own Profile on a subsequent Bridge launch.
        let path = root.path().join(format!(
            "{}.pending-{}-{sequence}",
            scope_file_prefix(lease.tenant_id(), lease.profile_id()),
            std::process::id()
        ));
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => {
                staged = Some((path, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let (path, mut file) = staged.ok_or(SaveAttemptEvidenceError::Unavailable)?;
    let write_result = file
        .write_all(MAGIC)
        .and_then(|_| file.write_all(&(header_bytes.len() as u32).to_be_bytes()))
        .and_then(|_| file.write_all(&header_bytes))
        .and_then(|_| file.write_all(ciphertext))
        .and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        // Do not delete the only scoped indication of an interrupted
        // checkpoint write. This may be torn evidence, NEVER an authority
        // to retry; fresh startup must fence the original Firefox bytes.
        return Err(error.into());
    }

    // Publish synced bytes without replacing an existing checkpoint.
    // On failure the staged ciphertext is the only durable local evidence;
    // never unlink it before the canonical checkpoint has been verified.
    finish_exact_checkpoint_publication(
        &path,
        &canonical,
        &header_bytes,
        ciphertext,
        fs::hard_link(&path, &canonical),
    )
}

/// Fail-closed no-replace checkpoint publication, in the existing Bridge
/// save-attempt owner (not another state machine or a cleanup service).
/// Neither a rejected link nor a mismatched canonical file may erase the
/// only fsynced exact candidate. A remaining pending sibling blocks Open.
fn finish_exact_checkpoint_publication(
    stage: &PathBuf,
    canonical: &PathBuf,
    header_bytes: &[u8],
    ciphertext: &[u8],
    publication: std::io::Result<()>,
) -> Result<(), SaveAttemptEvidenceError> {
    match publication {
        Ok(()) => {
            fs::remove_file(stage)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_existing_exact(canonical, header_bytes, ciphertext)?;
            // Only a byte-identical full checkpoint can substitute the
            // staged candidate, otherwise retain both as conflict evidence.
            fs::remove_file(stage)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

/// Reads bounded ciphertext after process restart and classifies local evidence.
/// The result never authorizes a cloud effect, local binding or workspace cleanup.
pub(crate) fn inspect_exact_save_attempt(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    successor: &GenerationId,
) -> Result<Option<ReadOnlySaveAttempt>, SaveAttemptEvidenceError> {
    let canonical = attempt_path(root, tenant, profile, successor);
    let record = match read_regular_bounded(&canonical) {
        Ok(bytes) => bytes,
        Err(SaveAttemptEvidenceError::Io(std::io::ErrorKind::NotFound)) => return Ok(None),
        Err(error) => return Err(error),
    };
    let (header, payload) = parse_record(&record)?;
    if header.tenant != tenant.as_str()
        || header.profile != profile.as_str()
        || header.successor != successor.as_str()
        || header.schema != "bridge-p3-attempt-v1"
        || header.base_generation == header.successor
        || header.session_fingerprint.len() != 64
        || header.fencing_fingerprint.len() != 64
        || header.epoch == 0
        || header.recovered_lone_origin.as_ref().is_some_and(|origin| {
            origin.epoch == 0
                || origin.epoch >= header.epoch
                || origin.session_fingerprint.len() != 64
                || origin.fencing_fingerprint.len() != 64
        })
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let prelude = inspect_generation_metadata_prelude(payload)
        .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    if prelude.metadata().tenant_id() != tenant
        || prelude.metadata().profile_id() != profile
        || prelude.metadata().generation_id() != successor
        || prelude
            .metadata()
            .base_generation_id()
            .map(GenerationId::as_str)
            != Some(header.base_generation.as_str())
        || prelude.metadata().object_key() != header.object_key
        || hex(&prelude.metadata_digest().bytes()) != header.metadata_digest
        || hex(&Sha256::digest(payload)) != header.container_digest
        || u64::try_from(payload.len()).map_err(|_| SaveAttemptEvidenceError::Invalid)?
            != header.container_bytes
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let candidate = root
        .open_generation(tenant, profile, successor)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let inventory = candidate
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if inventory.inventory_digest() != header.candidate_inventory
        || inventory.total_bytes() != header.candidate_bytes
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let base = GenerationId::parse(header.base_generation.clone())
        .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    let source = root
        .open_generation(tenant, profile, &base)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?
        .inventory_digest()
        != header.source_inventory
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    Ok(Some(ReadOnlySaveAttempt {
        header,
        ciphertext: payload.to_vec(),
    }))
}

/// Retire a *specific*, fully committed checkpoint after the caller has also
/// proven accepted local launch readiness and both ownership releases.
///
/// Never sweep arbitrary attempt files. The caller must gate this method on
/// DirtyCloseCompletion::is_fully_saved_locally(). A failed retirement retains
/// evidence; it is never permission to discard/rebuild a candidate.
pub(crate) fn retire_fully_saved_attempt(
    root: &MaterializationRoot,
    prepared: &PreparedDirtyGeneration,
    committed: &CommittedGenerationSuccessor,
    lease: &ProfileLease,
) -> Result<(), SaveAttemptEvidenceError> {
    let workspace = prepared.candidate_workspace();
    if committed.generation_id() != workspace.generation_id()
        || committed.object_key() != prepared.object_key()
        || committed.metadata_digest() != prepared.metadata_digest()
        || committed.container_digest() != prepared.container_digest()
        || committed.container_bytes()
            != u64::try_from(prepared.sealed().container().len())
                .map_err(|_| SaveAttemptEvidenceError::Invalid)?
        || workspace.tenant_id() != lease.tenant_id()
        || workspace.profile_id() != lease.profile_id()
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let canonical = attempt_path(
        root,
        lease.tenant_id(),
        lease.profile_id(),
        committed.generation_id(),
    );
    // Completion has published an immutable local binding sidecar, so the
    // current candidate inventory intentionally differs from the pre-commit
    // inventory in the original checkpoint. Compare the original encrypted
    // payload and exact descriptors, not the post-commit workspace inventory.
    let record = read_regular_bounded(&canonical)?;
    let (header, container) = parse_record(&record)?;
    if header.tenant != lease.tenant_id().as_str()
        || header.profile != lease.profile_id().as_str()
        || header.base_generation
            != prepared
                .sealed()
                .metadata()
                .base_generation_id()
                .ok_or(SaveAttemptEvidenceError::Invalid)?
                .as_str()
        || header.successor != committed.generation_id().as_str()
        || header.epoch != lease.epoch()
        || header.session_fingerprint
            != credential_fingerprint(b"session", lease.session_id().as_str())
        || header.fencing_fingerprint
            != credential_fingerprint(b"fencing", lease.fencing_token().as_str())
        || header.device_fingerprint.as_deref()
            != Some(credential_fingerprint(b"device", lease.device_id().as_str()).as_str())
        || header.object_key != committed.object_key()
        || header.metadata_digest != committed.metadata_digest()
        || header.container_digest != committed.container_digest()
        || header.container_bytes != committed.container_bytes()
        || header.candidate_inventory != prepared.candidate_inventory().inventory_digest()
        || header.candidate_bytes != prepared.candidate_inventory().total_bytes()
        || container != prepared.sealed().container()
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    // No fallback deletion if a concurrent operation removed/replaced it.
    // In particular, never remove the candidate or the base profile data.
    fs::remove_file(canonical)?;
    Ok(())
}

/// Retire only the same completed exact save after the existing D1 owner
/// confirmed ACTIVE_EXACT, the Bridge identity/binding owner accepted N+1,
/// and its fresh OS workspace lock was released. This method grants nothing.
#[cfg(any(windows, test))]
pub(crate) fn retire_active_exact_attempt_after_restart(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    observed: &ReadOnlySaveAttempt,
) -> Result<(), SaveAttemptEvidenceError> {
    if observed.header.tenant != tenant.as_str() || observed.header.profile != profile.as_str() {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let successor = GenerationId::parse(observed.successor_id())
        .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    // Revalidate original browser bytes and original immutable ciphertext
    // AFTER the local binding write; do not trust just its existence.
    let rechecked = inspect_exact_save_attempt(root, tenant, profile, &successor)?
        .ok_or(SaveAttemptEvidenceError::Changed)?;
    if rechecked.header != observed.header || rechecked.ciphertext != observed.ciphertext {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let candidate = root
        .open_generation(tenant, profile, &successor)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let binding = crate::browser_execution::load_materialization_binding(
        &candidate, tenant, profile, &successor,
    )
    .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if binding.source_container_sha256() != observed.container_digest()
        || candidate
            .materialization_inventory_digest()
            .map_err(|_| SaveAttemptEvidenceError::Changed)?
            != binding.materialized_inventory_digest()
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    // A failed removal retains the checkpoint and blocks launch as before.
    fs::remove_file(attempt_path(root, tenant, profile, &successor))?;
    Ok(())
}

/// Conservative startup guard. A crash can happen before, during or after
/// publication of an exact attempt. Both canonical and pending stage files
/// block a fresh browser launch for that *same Profile* until the original
/// attempt is reconciled by the authoritative recovery owner.
///
/// The filename is a one-way tenant/Profile scope fingerprint, so a damaged
/// file does not become invisible simply because its JSON header is torn.
/// This is a read-only check; it neither trusts the file nor grants a lease.
pub(crate) fn has_unresolved_save_attempt_for_profile(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
) -> Result<bool, SaveAttemptEvidenceError> {
    let prefix = scope_file_prefix(tenant, profile);
    let mut inspected = 0_usize;
    for entry in fs::read_dir(root.path())? {
        let entry = entry?;
        inspected = inspected.saturating_add(1);
        if inspected > 100_000 {
            return Err(SaveAttemptEvidenceError::Unavailable);
        }
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|value| value.starts_with(&prefix))
        {
            // Do not require an intact filename extension. A corrupted,
            // truncated, symbolic or staged name must not evade the guard.
            return Ok(true);
        }
    }
    Ok(false)
}

/// Read-only, restart-safe discovery of the *one* canonical exact save attempt for
/// this Profile. A checkpoint filename is not a capability: verify its bounded
/// header, deterministic scoped name, ciphertext and BOTH local inventories
/// through the original exact inspector before returning the evidence.
/// Unknown staged/torn or competing records remain hard blockers; this
/// function never gives permission to PUT/commit or to delete anything.
/// Transient, read-only classification by the existing Bridge save owner.
/// Neither a verified lone intent nor an exact checkpoint authorizes cloud
/// PUT/commit, release of a writer, or deletion of original Firefox data.
pub(crate) enum ScopedPendingSaveEvidence {
    None,
    LocalUnsynced {
        base_generation: GenerationId,
        prior_epoch: u64,
    },
    Exact(Box<ReadOnlySaveAttempt>),
}

/// Keep the original exact-only API fail-closed: callers that cannot
/// handle a lone LOCAL_UNSYNCED never silently mistake it for no evidence.
pub(crate) fn discover_unresolved_exact_save_attempt(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
) -> Result<Option<ReadOnlySaveAttempt>, SaveAttemptEvidenceError> {
    match classify_scoped_pending_save_evidence(root, tenant, profile)? {
        ScopedPendingSaveEvidence::None => Ok(None),
        ScopedPendingSaveEvidence::Exact(attempt) => Ok(Some(*attempt)),
        ScopedPendingSaveEvidence::LocalUnsynced {
            base_generation,
            prior_epoch,
        } => {
            // Read both fields even in non-Windows shipping builds; this
            // exact-only API intentionally refuses any lone local intent.
            let _ = (base_generation, prior_epoch);
            Err(SaveAttemptEvidenceError::Changed)
        }
    }
}

/// Scope-aware post-crash diagnosis; reads original native inventory without
/// recovering an expired lease, inventing N+1, or writing anything to disk.
pub(crate) fn classify_scoped_pending_save_evidence(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
) -> Result<ScopedPendingSaveEvidence, SaveAttemptEvidenceError> {
    let prefix = scope_file_prefix(tenant, profile);
    let marker = local_unsynced_path(root, tenant, profile);
    let mut candidate = None;
    let mut has_local_intent = false;
    let mut inspected = 0_usize;
    for entry in fs::read_dir(root.path())? {
        let entry = entry?;
        inspected = inspected.saturating_add(1);
        if inspected > 100_000 {
            return Err(SaveAttemptEvidenceError::Unavailable);
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            // Unknown non-UTF8 names cannot be treated as a trusted attempt.
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        if path == marker {
            // A single small LOCAL_UNSYNCED marker may accompany only the
            // single canonical exact checkpoint of the original attempt.
            // Check the contents AFTER inspecting the full exact checkpoint.
            if has_local_intent {
                return Err(SaveAttemptEvidenceError::Invalid);
            }
            has_local_intent = true;
            continue;
        }
        if candidate.is_some() {
            return Err(SaveAttemptEvidenceError::Invalid);
        }

        // Every unknown/torn .pending stage, foreign sibling or extra
        // extension is a blocker, NOT a second candidate or authority.
        let header = read_bounded_attempt_header(&path)?;
        if header.schema != "bridge-p3-attempt-v1"
            || header.tenant != tenant.as_str()
            || header.profile != profile.as_str()
        {
            return Err(SaveAttemptEvidenceError::Invalid);
        }
        let generation =
            GenerationId::parse(header.successor).map_err(|_| SaveAttemptEvidenceError::Invalid)?;
        if path != attempt_path(root, tenant, profile, &generation) {
            return Err(SaveAttemptEvidenceError::Invalid);
        }
        candidate = Some(generation);
    }
    let exact = candidate
        .map(|generation| {
            inspect_exact_save_attempt(root, tenant, profile, &generation)?
                .ok_or(SaveAttemptEvidenceError::Changed)
        })
        .transpose()?;
    // A valid lone intent is diagnostic evidence, NOT a saved checkpoint.
    // Source bytes/WAL and original browser identity must still match the
    // precise closed inventory; a forged or stale file remains UNKNOWN.
    if has_local_intent {
        if let Some(attempt) = exact.as_ref() {
            if !matching_local_unsynced_for_exact(root, tenant, profile, attempt)? {
                return Err(SaveAttemptEvidenceError::Changed);
            }
        } else {
            let intent = read_local_unsynced(&marker)?;
            if intent.schema != "bridge-p3-local-unsynced-v1"
                || intent.tenant != tenant.as_str()
                || intent.profile != profile.as_str()
                || intent.epoch == 0
                || intent.session_fingerprint.len() != 64
                || intent.fencing_fingerprint.len() != 64
                || !intent
                    .session_fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || !intent
                    .fencing_fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(SaveAttemptEvidenceError::Invalid);
            }
            let base_generation = GenerationId::parse(intent.base_generation)
                .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
            let source = root
                .open_generation(tenant, profile, &base_generation)
                .map_err(|_| SaveAttemptEvidenceError::Changed)?;
            let inventory = source
                .inventory()
                .map_err(|_| SaveAttemptEvidenceError::Changed)?;
            if inventory.inventory_digest() != intent.source_inventory
                || inventory.total_bytes() != intent.source_bytes
            {
                return Err(SaveAttemptEvidenceError::Changed);
            }
            return Ok(ScopedPendingSaveEvidence::LocalUnsynced {
                base_generation,
                prior_epoch: intent.epoch,
            });
        }
    }
    Ok(match exact {
        Some(attempt) => ScopedPendingSaveEvidence::Exact(Box::new(attempt)),
        None => ScopedPendingSaveEvidence::None,
    })
}

/// Header-only bounded read: discovery never loads the large sealed body
/// merely to obtain the successor selector. Exact ciphertext verification
/// remains the responsibility of inspect_exact_save_attempt.
/// Read-only evidence for a fsynced BPAT0001 checkpoint with a verified
/// ciphertext RANGE in the very same file. This is not a new format, cloud
/// permission, or second persistent generation.
#[allow(dead_code)] // Activated only when the K2 shipping owner is switched.
pub(crate) struct ReadOnlyStreamedSaveAttempt {
    header: AttemptHeader,
    ciphertext: VerifiedGenerationObjectFile,
}

#[allow(dead_code)]
impl ReadOnlyStreamedSaveAttempt {
    pub(crate) fn ciphertext(&mut self) -> &mut VerifiedGenerationObjectFile {
        &mut self.ciphertext
    }

    pub(crate) fn original_epoch(&self) -> u64 {
        self.header.epoch
    }

    pub(crate) fn original_device(&self, lease: &ProfileLease) -> bool {
        self.header.device_fingerprint.as_deref()
            == Some(credential_fingerprint(b"device", lease.device_id().as_str()).as_str())
    }

    pub(crate) fn original_lease(&self, lease: &ProfileLease) -> bool {
        self.header.epoch == lease.epoch()
            && self.header.session_fingerprint
                == credential_fingerprint(b"session", lease.session_id().as_str())
            && self.header.fencing_fingerprint
                == credential_fingerprint(b"fencing", lease.fencing_token().as_str())
    }

    pub(crate) fn object_key(&self) -> &str {
        &self.header.object_key
    }

    pub(crate) fn metadata_digest(&self) -> &str {
        &self.header.metadata_digest
    }

    pub(crate) fn container_digest(&self) -> &str {
        &self.header.container_digest
    }

    pub(crate) fn container_bytes(&self) -> u64 {
        self.header.container_bytes
    }
}

/// Existing BPAT0001 header+payload contract, now streaming from a sealed
/// on-disk BPGC0001 source. A completed temp write is fsynced, re-verified
/// by SHA-256 and published no-replace with the existing scoped attempt path.
/// Errors KEEP uncertain stages and the original Firefox N/N+1 intact.
/// This alone never uploads, commits, authorizes a lease or marks Saved.
#[allow(dead_code)] // K2 shipping conversion is a separate activation gate.
/// An existing sealed BPGC file may be copied into the original BPAT
/// format, but this adapter does not create plaintext or secondary authority.
#[allow(dead_code)] // K2 shipping rollout still gated.
pub(crate) fn persist_streaming_exact_save_attempt(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    candidate: &GenerationWorkspace,
    report: &StreamingGenerationReport,
    sealed_file: &Path,
    lease: &ProfileLease,
) -> Result<(), SaveAttemptEvidenceError> {
    let mut input = VerifiedGenerationObjectFile::open(
        sealed_file,
        report.container_bytes(),
        report.container_digest().bytes(),
    )
    .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    persist_streaming_checkpoint_with_writer(root, source, candidate, report, lease, |output| {
        let copied = std::io::copy(&mut input.reader().take(report.container_bytes()), output)?;
        if copied != report.container_bytes() {
            return Err(SaveAttemptEvidenceError::Changed);
        }
        input
            .recheck_and_rewind()
            .map_err(|_| SaveAttemptEvidenceError::Changed)
    })
}

/// Direct native Firefox -> EXACT encrypted BPAT checkpoint. No plaintext
/// snapshot spool and no second ciphertext spool. Existing current writer
/// authority/OS admission MUST be held by the caller; this only establishes
/// local immutable evidence, never permission to upload or commit.
///
/// Pass 1 hashes the exact canonical BPGW reader before calling the key owner.
/// Pass 2 seals to a nonallocating sink, computing the deterministic BPGC
/// descriptor. Pass 3 writes *the same ciphertext* directly into an fsynced
/// BPAT.pending stage. Digest/inventory verification and no-replace hardlink
/// remain in the ONE existing checkpoint owner. A torn stage remains a hard
/// blocker; no N+2 and no source-file modification occurs.
#[allow(dead_code)] // Activated by the K2 shipping owner integration gate.
pub(crate) fn persist_streaming_exact_save_attempt_from_native<K: GenerationSealingMaterialPort>(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    candidate: &GenerationWorkspace,
    lease: &ProfileLease,
    sealer: &mut K,
) -> Result<(), SaveAttemptEvidenceError> {
    if source.tenant_id() != lease.tenant_id()
        || source.profile_id() != lease.profile_id()
        || candidate.tenant_id() != lease.tenant_id()
        || candidate.profile_id() != lease.profile_id()
        || source.generation_id() == candidate.generation_id()
        || lease.epoch() == 0
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    // Existing checkpoint never triggers new key/nonce material or overwrites.
    let target = attempt_path(
        root,
        lease.tenant_id(),
        lease.profile_id(),
        candidate.generation_id(),
    );
    match fs::symlink_metadata(&target) {
        Ok(_) => return Err(SaveAttemptEvidenceError::Changed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if candidate
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?
        != inventory
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let mut reader = WorkspaceSnapshotReader::new(candidate, &inventory)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let expected_bytes = reader.expected_bytes();
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    let mut counted = 0_u64;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| SaveAttemptEvidenceError::Changed)?;
        if count == 0 {
            break;
        }
        counted = counted
            .checked_add(count as u64)
            .ok_or(SaveAttemptEvidenceError::Invalid)?;
        digest.update(&buffer[..count]);
    }
    if counted != expected_bytes
        || source
            .inventory()
            .map_err(|_| SaveAttemptEvidenceError::Changed)?
            != inventory
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let measured_digest: [u8; 32] = digest.finalize().into();
    let material = sealer
        .material_for(
            lease.tenant_id(),
            lease.profile_id(),
            source.generation_id(),
            candidate.generation_id(),
            measured_digest,
        )
        .map_err(|_| SaveAttemptEvidenceError::Unavailable)?;
    let (key, nonce, chunk_size) = material.into_parts();
    // The first pass is bound to the original DEK request. Neither a
    // changed key-owner source nor a subsequent same-size edit can change
    // the ciphertext silently: both sealing passes authenticate this digest.
    let mut comparison = WorkspaceSnapshotReader::new(candidate, &inventory)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let metadata = GenerationMetadata::for_prehashed_stream(
        GenerationIdentity::new(
            lease.tenant_id().clone(),
            lease.profile_id().clone(),
            candidate.generation_id().clone(),
            Some(source.generation_id().clone()),
        ),
        key.key_id().clone(),
        nonce,
        chunk_size,
        expected_bytes,
        measured_digest,
    )
    .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    let expected =
        seal_generation_streaming(&metadata, &key, &mut comparison, &mut std::io::sink())
            .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?
        != inventory
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    persist_streaming_checkpoint_with_writer(root, source, candidate, &expected, lease, |output| {
        let mut actual_reader = WorkspaceSnapshotReader::new(candidate, &inventory)
            .map_err(|_| SaveAttemptEvidenceError::Changed)?;
        let actual = seal_generation_streaming(&metadata, &key, &mut actual_reader, output)
            .map_err(|_| SaveAttemptEvidenceError::Changed)?;
        if actual != expected
            || source
                .inventory()
                .map_err(|_| SaveAttemptEvidenceError::Changed)?
                != inventory
        {
            return Err(SaveAttemptEvidenceError::Changed);
        }
        Ok(())
    })
}

fn streaming_checkpoint_header(
    source: &GenerationWorkspace,
    candidate: &GenerationWorkspace,
    report: &StreamingGenerationReport,
    lease: &ProfileLease,
) -> Result<AttemptHeader, SaveAttemptEvidenceError> {
    let metadata = report.metadata();
    if lease.tenant_id() != source.tenant_id()
        || lease.profile_id() != source.profile_id()
        || source.tenant_id() != candidate.tenant_id()
        || source.profile_id() != candidate.profile_id()
        || metadata.tenant_id() != source.tenant_id()
        || metadata.profile_id() != source.profile_id()
        || metadata.generation_id() != candidate.generation_id()
        || metadata.base_generation_id() != Some(source.generation_id())
        || source.generation_id() == candidate.generation_id()
        || lease.epoch() == 0
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let base_inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let candidate_inventory = candidate
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if base_inventory != candidate_inventory {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    let header = AttemptHeader {
        schema: "bridge-p3-attempt-v1".to_owned(),
        tenant: lease.tenant_id().as_str().to_owned(),
        profile: lease.profile_id().as_str().to_owned(),
        base_generation: source.generation_id().as_str().to_owned(),
        successor: candidate.generation_id().as_str().to_owned(),
        session_fingerprint: credential_fingerprint(b"session", lease.session_id().as_str()),
        fencing_fingerprint: credential_fingerprint(b"fencing", lease.fencing_token().as_str()),
        device_fingerprint: Some(credential_fingerprint(
            b"device",
            lease.device_id().as_str(),
        )),
        epoch: lease.epoch(),
        source_inventory: base_inventory.inventory_digest(),
        candidate_inventory: candidate_inventory.inventory_digest(),
        candidate_bytes: candidate_inventory.total_bytes(),
        object_key: metadata.object_key(),
        metadata_digest: hex(&report.metadata_digest().bytes()),
        container_digest: hex(&report.container_digest().bytes()),
        container_bytes: report.container_bytes(),
        recovered_lone_origin: None,
    };

    Ok(header)
}

fn persist_streaming_checkpoint_with_writer(
    root: &MaterializationRoot,
    source: &GenerationWorkspace,
    candidate: &GenerationWorkspace,
    report: &StreamingGenerationReport,
    lease: &ProfileLease,
    write_ciphertext: impl FnOnce(&mut File) -> Result<(), SaveAttemptEvidenceError>,
) -> Result<(), SaveAttemptEvidenceError> {
    let header = streaming_checkpoint_header(source, candidate, report, lease)?;
    let header_bytes =
        serde_json::to_vec(&header).map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    if header_bytes.is_empty()
        || header_bytes.len() > MAX_HEADER_BYTES
        || report.container_bytes() == 0
        || report.container_bytes() > MAX_STREAM_CONTAINER_BYTES
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let canonical = attempt_path(
        root,
        lease.tenant_id(),
        lease.profile_id(),
        candidate.generation_id(),
    );
    match fs::symlink_metadata(&canonical) {
        Ok(_) => {
            let existing = inspect_streaming_exact_save_attempt(
                root,
                lease.tenant_id(),
                lease.profile_id(),
                candidate.generation_id(),
            )?
            .ok_or(SaveAttemptEvidenceError::Changed)?;
            if existing.header == header {
                return Ok(());
            }
            return Err(SaveAttemptEvidenceError::Changed);
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    let mut staged = None;
    for _ in 0..STAGE_TRIES {
        let sequence = STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = root.path().join(format!(
            "{}.pending-{}-{sequence}",
            scope_file_prefix(lease.tenant_id(), lease.profile_id()),
            std::process::id(),
        ));
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => {
                staged = Some((path, file));
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err.into()),
        }
    }
    let (stage, mut out) = staged.ok_or(SaveAttemptEvidenceError::Unavailable)?;
    let write = (|| -> Result<(), SaveAttemptEvidenceError> {
        out.write_all(MAGIC)?;
        out.write_all(&(header_bytes.len() as u32).to_be_bytes())?;
        out.write_all(&header_bytes)?;
        write_ciphertext(&mut out)?;
        out.sync_all()?;
        Ok(())
    })();
    drop(out);
    // A failed fsync/copy means stage is an authoritative BLOCKER, never a
    // disposable tmp. Do not publish or remove it.
    write?;
    let mut verification = VerifiedGenerationObjectFile::open_checkpoint_payload(
        &stage,
        12 + header_bytes.len() as u64,
        report.container_bytes(),
        report.container_digest().bytes(),
    )
    .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    verification
        .recheck_and_rewind()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    match fs::hard_link(&stage, &canonical) {
        Ok(()) => {
            fs::remove_file(&stage)?;
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = inspect_streaming_exact_save_attempt(
                root,
                lease.tenant_id(),
                lease.profile_id(),
                candidate.generation_id(),
            )?
            .ok_or(SaveAttemptEvidenceError::Changed)?;
            if existing.header != header {
                return Err(SaveAttemptEvidenceError::Changed);
            }
            fs::remove_file(&stage)?;
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

/// Read-only streaming inspector: same scoped BPAT0001 name, bounded JSON and
/// BPGC0001 prelude, SHA-256 over the single embedded ciphertext payload,
/// exact generation metadata and BOTH original local inventories. It loads
/// no generation-sized Vec and grants absolutely no cloud authority.
#[allow(dead_code)] // Enabled by the future K2 operator save/replay integration.
pub(crate) fn inspect_streaming_exact_save_attempt(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    successor: &GenerationId,
) -> Result<Option<ReadOnlyStreamedSaveAttempt>, SaveAttemptEvidenceError> {
    let canonical = attempt_path(root, tenant, profile, successor);
    let stat = match fs::symlink_metadata(&canonical) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if !stat.is_file()
        || stat.file_type().is_symlink()
        || stat.len() <= 12
        || stat.len() > MAX_STREAM_RECORD_BYTES
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let header = read_bounded_attempt_header(&canonical)?;
    if header.schema != "bridge-p3-attempt-v1"
        || header.tenant != tenant.as_str()
        || header.profile != profile.as_str()
        || header.successor != successor.as_str()
        || header.epoch == 0
        || header.base_generation == header.successor
        || header.session_fingerprint.len() != 64
        || header.fencing_fingerprint.len() != 64
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let mut file = File::open(&canonical)?;
    let mut prefix = [0_u8; 12];
    file.read_exact(&mut prefix)?;
    let header_len = u32::from_be_bytes(
        prefix[8..12]
            .try_into()
            .map_err(|_| SaveAttemptEvidenceError::Invalid)?,
    ) as u64;
    let payload_offset = 12 + header_len;
    if header_len == 0
        || header_len > MAX_HEADER_BYTES as u64
        || stat.len()
            != payload_offset
                .checked_add(header.container_bytes)
                .ok_or(SaveAttemptEvidenceError::Invalid)?
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let hash_bytes = header.container_digest.as_bytes();
    if hash_bytes.len() != 64 || !hash_bytes.iter().all(|b| b.is_ascii_hexdigit()) {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let mut expected_sha = [0_u8; 32];
    for (i, byte) in expected_sha.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&header.container_digest[i * 2..i * 2 + 2], 16)
            .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    }
    let mut payload = VerifiedGenerationObjectFile::open_checkpoint_payload(
        &canonical,
        payload_offset,
        header.container_bytes,
        expected_sha,
    )
    .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let mut bpgc_prefix = [0_u8; 12];
    payload.reader().read_exact(&mut bpgc_prefix)?;
    if &bpgc_prefix[..8] != b"BPGC0001" {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let metadata_len = u32::from_be_bytes(
        bpgc_prefix[8..12]
            .try_into()
            .map_err(|_| SaveAttemptEvidenceError::Invalid)?,
    ) as usize;
    if metadata_len == 0 || metadata_len > MAX_GENERATION_METADATA_PRELUDE_BYTES - 12 {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let mut metadata_prefix = vec![0_u8; 12 + metadata_len];
    metadata_prefix[..12].copy_from_slice(&bpgc_prefix);
    payload.reader().read_exact(&mut metadata_prefix[12..])?;
    let prelude = inspect_generation_metadata_prelude(&metadata_prefix)
        .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    if prelude.metadata().tenant_id() != tenant
        || prelude.metadata().profile_id() != profile
        || prelude.metadata().generation_id() != successor
        || prelude
            .metadata()
            .base_generation_id()
            .map(GenerationId::as_str)
            != Some(header.base_generation.as_str())
        || prelude.metadata().object_key() != header.object_key
        || hex(&prelude.metadata_digest().bytes()) != header.metadata_digest
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    payload
        .recheck_and_rewind()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let base = GenerationId::parse(header.base_generation.clone())
        .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    let source = root
        .open_generation(tenant, profile, &base)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let candidate = root
        .open_generation(tenant, profile, successor)
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let source_inventory = source
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    let candidate_inventory = candidate
        .inventory()
        .map_err(|_| SaveAttemptEvidenceError::Changed)?;
    if source_inventory.inventory_digest() != header.source_inventory
        || candidate_inventory.inventory_digest() != header.candidate_inventory
        || candidate_inventory.total_bytes() != header.candidate_bytes
    {
        return Err(SaveAttemptEvidenceError::Changed);
    }
    Ok(Some(ReadOnlyStreamedSaveAttempt {
        header,
        ciphertext: payload,
    }))
}

fn read_bounded_attempt_header(path: &PathBuf) -> Result<AttemptHeader, SaveAttemptEvidenceError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() <= 12
        || metadata.len() > MAX_STREAM_RECORD_BYTES
    {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let mut file = File::open(path)?;
    let mut prefix = [0_u8; 12];
    file.read_exact(&mut prefix)?;
    if &prefix[..8] != MAGIC {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let length = u32::from_be_bytes(
        prefix[8..12]
            .try_into()
            .map_err(|_| SaveAttemptEvidenceError::Invalid)?,
    ) as usize;
    if length == 0 || length > MAX_HEADER_BYTES || metadata.len() <= 12 + length as u64 {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let mut header_bytes = vec![0_u8; length];
    file.read_exact(&mut header_bytes)?;
    serde_json::from_slice(&header_bytes).map_err(|_| SaveAttemptEvidenceError::Invalid)
}

fn scope_file_prefix(tenant: &TenantId, profile: &ProfileId) -> String {
    let mut sha = Sha256::new();
    sha.update(b"bridge-p3-save-attempt-profile-scope-v1\0");
    for part in [tenant.as_str(), profile.as_str()] {
        sha.update((part.len() as u64).to_be_bytes());
        sha.update(part.as_bytes());
    }
    format!(".profile-save-attempt-v1-{}", hex(&sha.finalize()))
}

fn attempt_path(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    successor: &GenerationId,
) -> PathBuf {
    let mut sha = Sha256::new();
    sha.update(b"bridge-p3-save-attempt-successor-v1\0");
    sha.update((successor.as_str().len() as u64).to_be_bytes());
    sha.update(successor.as_str().as_bytes());
    root.path().join(format!(
        "{}-{}.bin",
        scope_file_prefix(tenant, profile),
        hex(&sha.finalize())
    ))
}

fn verify_existing_exact(
    canonical: &PathBuf,
    header_bytes: &[u8],
    ciphertext: &[u8],
) -> Result<(), SaveAttemptEvidenceError> {
    let existing = read_regular_bounded(canonical)?;
    let (header, payload) = parse_record(&existing)?;
    let expected: AttemptHeader =
        serde_json::from_slice(header_bytes).map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    if header == expected && payload == ciphertext {
        Ok(())
    } else {
        Err(SaveAttemptEvidenceError::Changed)
    }
}

fn read_regular_bounded(path: &PathBuf) -> Result<Vec<u8>, SaveAttemptEvidenceError> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_RECORD_BYTES as u64 {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let file = File::open(path)?;
    let mut result = Vec::new();
    file.take((MAX_RECORD_BYTES as u64) + 1)
        .read_to_end(&mut result)?;
    if result.len() > MAX_RECORD_BYTES {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    Ok(result)
}

fn parse_record(bytes: &[u8]) -> Result<(AttemptHeader, &[u8]), SaveAttemptEvidenceError> {
    if bytes.len() < 12 || &bytes[..8] != MAGIC {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let len = u32::from_be_bytes(
        bytes[8..12]
            .try_into()
            .map_err(|_| SaveAttemptEvidenceError::Invalid)?,
    ) as usize;
    if len == 0 || len > MAX_HEADER_BYTES || len > bytes.len() - 12 {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    let header = serde_json::from_slice(&bytes[12..12 + len])
        .map_err(|_| SaveAttemptEvidenceError::Invalid)?;
    let ciphertext = &bytes[12 + len..];
    if ciphertext.is_empty() || ciphertext.len() > MAX_GENERATION_CONTAINER_BYTES {
        return Err(SaveAttemptEvidenceError::Invalid);
    }
    Ok((header, ciphertext))
}

fn credential_fingerprint(kind: &[u8], value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"bridge-p3-ephemeral-owner-fingerprint-v1\0");
    hash.update((kind.len() as u64).to_be_bytes());
    hash.update(kind);
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
    hex(&hash.finalize())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod direct_native_checkpoint_tests {
    use super::{
        SaveAttemptEvidenceError, attempt_path, inspect_streaming_exact_save_attempt,
        persist_streaming_exact_save_attempt_from_native,
    };
    use crate::dirty_generation::{GenerationSealingMaterial, GenerationSealingMaterialPort};
    use crate::local_profile::{GenerationWorkspace, MaterializationRoot};
    use encrypted_generation_domain::{
        GenerationDek, KeyId, NoncePrefix, open_generation_streaming,
    };
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, ProfileId, SessionId, TenantId,
    };
    use session_domain::ProfileLease;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    struct Keys {
        calls: usize,
        mutate: Option<PathBuf>,
    }

    impl GenerationSealingMaterialPort for Keys {
        type Error = ();

        fn material_for(
            &mut self,
            _tenant: &TenantId,
            _profile: &ProfileId,
            _base: &GenerationId,
            _successor: &GenerationId,
            _original_digest: [u8; 32],
        ) -> Result<GenerationSealingMaterial, Self::Error> {
            self.calls += 1;
            if let Some(path) = self.mutate.as_ref() {
                fs::write(path, b"mutated same-length Firefox WAL").map_err(|_| ())?;
            }
            Ok(GenerationSealingMaterial::new(
                GenerationDek::new(
                    KeyId::parse("native_direct_checkpoint_key_01").map_err(|_| ())?,
                    [0x39; 32],
                ),
                NoncePrefix::new([0x19; 16]),
                65_536,
            ))
        }
    }

    struct Fixture {
        path: PathBuf,
        root: MaterializationRoot,
        tenant: TenantId,
        profile: ProfileId,
        n: GenerationId,
        next: GenerationId,
        lease: ProfileLease,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let seq = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bridge-k2-direct-checkpoint-{}-{seq}",
                std::process::id(),
            ));
            let tenant = TenantId::parse(format!("tenant_k2_direct_{seq}"))?;
            let profile = ProfileId::parse(format!("profile_k2_direct_{seq}"))?;
            Ok(Self {
                root: MaterializationRoot::open_or_create(path.clone())?,
                path,
                lease: ProfileLease::issue(
                    tenant.clone(),
                    profile.clone(),
                    SessionId::parse(format!("session_k2_direct_{seq}"))?,
                    DeviceId::parse(format!("device_k2_direct_{seq}"))?,
                    7,
                    FencingToken::parse(format!("fence_k2_direct_{seq}"))?,
                )?,
                tenant,
                profile,
                n: GenerationId::parse(format!("generation_k2_base_{seq}"))?,
                next: GenerationId::parse(format!("generation_k2_next_{seq}"))?,
            })
        }

        fn native(
            &self,
        ) -> Result<(GenerationWorkspace, GenerationWorkspace), Box<dyn std::error::Error>>
        {
            let source = self
                .root
                .create_generation(&self.tenant, &self.profile, &self.n)?;
            let candidate = self
                .root
                .create_generation(&self.tenant, &self.profile, &self.next)?;
            for workspace in [&source, &candidate] {
                let dir = workspace.path().join("user_data");
                fs::create_dir_all(&dir)?;
                fs::write(dir.join("cookies.sqlite-wal"), vec![0x46; 2_400_109])?;
                fs::write(workspace.path().join("prefs.js"), b"native preferences")?;
            }
            assert_eq!(source.inventory()?, candidate.inventory()?);
            Ok((source, candidate))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = crate::test_support::remove_test_root(&self.path);
        }
    }

    #[test]
    fn direct_native_to_fsynced_bpat_is_authenticated_retryable_and_spool_free()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let (source, candidate) = fixture.native()?;
        let mut keys = Keys {
            calls: 0,
            mutate: None,
        };
        persist_streaming_exact_save_attempt_from_native(
            &fixture.root,
            &source,
            &candidate,
            &fixture.lease,
            &mut keys,
        )?;
        assert_eq!(keys.calls, 1);
        let mut exact = inspect_streaming_exact_save_attempt(
            &fixture.root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.next,
        )?
        .ok_or("missing direct checkpoint")?;
        assert!(exact.original_lease(&fixture.lease));
        assert!(exact.original_device(&fixture.lease));
        assert!(exact.container_bytes() > 2_400_109);
        let key = GenerationDek::new(KeyId::parse("native_direct_checkpoint_key_01")?, [0x39; 32]);
        let report =
            open_generation_streaming(exact.ciphertext().reader(), &key, &mut std::io::sink())?;
        assert_eq!(report.metadata().generation_id(), &fixture.next);
        assert_eq!(report.metadata().base_generation_id(), Some(&fixture.n));
        exact.ciphertext().recheck_and_rewind()?;

        let canonical = attempt_path(
            &fixture.root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.next,
        );
        assert!(canonical.exists());
        for entry in fs::read_dir(fixture.root.path())? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            assert!(
                !name.contains(".pending-"),
                "successful attempt left a torn stage"
            );
            assert!(!name.ends_with(".bpgc"), "no secondary ciphertext spool");
        }
        assert_eq!(
            persist_streaming_exact_save_attempt_from_native(
                &fixture.root,
                &source,
                &candidate,
                &fixture.lease,
                &mut keys,
            ),
            Err(SaveAttemptEvidenceError::Changed),
        );
        assert_eq!(
            keys.calls, 1,
            "no fresh nonce/key on duplicate exact attempt"
        );
        assert_eq!(
            fs::read(source.path().join("prefs.js"))?,
            b"native preferences"
        );
        assert_eq!(source.inventory()?, candidate.inventory()?);
        Ok(())
    }

    #[test]
    fn mutation_while_selecting_key_blocks_direct_checkpoint_and_preserves_native()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let (source, candidate) = fixture.native()?;
        let changed = candidate.path().join("prefs.js");
        let mut keys = Keys {
            calls: 0,
            mutate: Some(changed.clone()),
        };
        assert_eq!(
            persist_streaming_exact_save_attempt_from_native(
                &fixture.root,
                &source,
                &candidate,
                &fixture.lease,
                &mut keys,
            ),
            Err(SaveAttemptEvidenceError::Changed),
        );
        assert_eq!(keys.calls, 1);
        assert!(
            !attempt_path(
                &fixture.root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.next,
            )
            .exists()
        );
        assert_eq!(
            fs::read(source.path().join("prefs.js"))?,
            b"native preferences"
        );
        assert_eq!(fs::read(changed)?, b"mutated same-length Firefox WAL");
        Ok(())
    }
}

#[cfg(test)]
mod streaming_checkpoint_tests {
    use super::{
        SaveAttemptEvidenceError, attempt_path, inspect_streaming_exact_save_attempt,
        persist_streaming_exact_save_attempt,
    };
    use crate::local_profile::MaterializationRoot;
    use encrypted_generation_domain::{
        GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
        seal_generation_streaming,
    };
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, ProfileId, SessionId, TenantId,
    };
    use session_domain::ProfileLease;
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn streamed_checkpoint_survives_restart_without_loading_ciphertext()
    -> Result<(), Box<dyn std::error::Error>> {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("bridge-stream-attempt-{}-{id}", std::process::id(),));
        let root = MaterializationRoot::open_or_create(path.clone())?;
        let tenant = TenantId::parse(format!("tenant_streamed_attempt_{id}"))?;
        let profile = ProfileId::parse(format!("profile_streamed_attempt_{id}"))?;
        let n = GenerationId::parse(format!("generation_streamed_base_{id}"))?;
        let next = GenerationId::parse(format!("generation_streamed_next_{id}"))?;
        let source = root.create_generation(&tenant, &profile, &n)?;
        let candidate = root.create_generation(&tenant, &profile, &next)?;
        fs::write(
            source.path().join("prefs.js"),
            b"native Firefox cookie/WAL witness",
        )?;
        fs::write(
            candidate.path().join("prefs.js"),
            b"native Firefox cookie/WAL witness",
        )?;
        let lease = ProfileLease::issue(
            tenant.clone(),
            profile.clone(),
            SessionId::parse(format!("session_streamed_{id}"))?,
            DeviceId::parse(format!("device_streamed_{id}"))?,
            3,
            FencingToken::parse(format!("fence_streamed_{id}"))?,
        )?;
        let sealed_path = path.join("local-sealed.bpgc");
        let content_size = 1_500_031_u64;
        let key = GenerationDek::new(
            KeyId::parse("bridge_streaming_checkpoint_key_01")?,
            [0x34; 32],
        );
        let metadata = GenerationMetadata::for_reader(
            GenerationIdentity::new(
                tenant.clone(),
                profile.clone(),
                next.clone(),
                Some(n.clone()),
            ),
            key.key_id().clone(),
            NoncePrefix::new([0x27; 16]),
            65_536,
            &mut std::io::repeat(0x5a).take(content_size),
        )?;
        let mut sealed = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&sealed_path)?;
        let descriptor = seal_generation_streaming(
            &metadata,
            &key,
            &mut std::io::repeat(0x5a).take(content_size),
            &mut sealed,
        )?;
        sealed.sync_all()?;
        drop(sealed);
        persist_streaming_exact_save_attempt(
            &root,
            &source,
            &candidate,
            &descriptor,
            &sealed_path,
            &lease,
        )?;
        // Retry of the exact immutable checkpoint is idempotent, never N+2.
        persist_streaming_exact_save_attempt(
            &root,
            &source,
            &candidate,
            &descriptor,
            &sealed_path,
            &lease,
        )?;
        let mut evidence = inspect_streaming_exact_save_attempt(&root, &tenant, &profile, &next)?
            .ok_or("missing streaming checkpoint")?;
        assert_eq!(evidence.original_epoch(), 3);
        assert!(evidence.original_device(&lease));
        assert!(evidence.original_lease(&lease));
        assert_eq!(evidence.container_bytes(), descriptor.container_bytes());
        assert_eq!(
            evidence.container_digest(),
            super::hex(&descriptor.container_digest().bytes()),
        );
        assert_eq!(evidence.object_key(), metadata.object_key());
        let mut actual = std::io::sink();
        let mut reader = evidence.ciphertext().reader();
        assert_eq!(
            std::io::copy(&mut reader, &mut actual)?,
            descriptor.container_bytes(),
        );
        evidence.ciphertext().recheck_and_rewind()?;
        let checkpoint = attempt_path(&root, &tenant, &profile, &next);
        let previous_length = fs::metadata(&checkpoint)?.len();
        let mut mutation = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&checkpoint)?;
        mutation.seek(SeekFrom::Start(previous_length - 1))?;
        let mut last = [0_u8; 1];
        mutation.read_exact(&mut last)?;
        mutation.seek(SeekFrom::Start(previous_length - 1))?;
        mutation.write_all(&[last[0] ^ 1])?;
        mutation.sync_all()?;
        assert!(inspect_streaming_exact_save_attempt(&root, &tenant, &profile, &next,).is_err());
        assert_eq!(
            fs::read(source.path().join("prefs.js"))?,
            b"native Firefox cookie/WAL witness",
        );
        assert!(
            checkpoint.exists(),
            "damaged evidence must never be deleted"
        );
        drop(evidence);
        drop(mutation);
        drop(root);
        let _ = crate::test_support::remove_test_root(&path);
        Ok(())
    }

    #[test]
    fn streaming_checkpoint_rejects_foreign_identity_without_discarding_original()
    -> Result<(), Box<dyn std::error::Error>> {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("bridge-stream-foreign-{}-{id}", std::process::id(),));
        let root = MaterializationRoot::open_or_create(path.clone())?;
        let tenant = TenantId::parse(format!("tenant_foreign_{id}"))?;
        let profile = ProfileId::parse(format!("profile_foreign_{id}"))?;
        let n = GenerationId::parse(format!("generation_foreign_n_{id}"))?;
        let next = GenerationId::parse(format!("generation_foreign_next_{id}"))?;
        let source = root.create_generation(&tenant, &profile, &n)?;
        let candidate = root.create_generation(&tenant, &profile, &next)?;
        fs::write(source.path().join("prefs.js"), b"first")?;
        fs::write(candidate.path().join("prefs.js"), b"second")?;
        let lease = ProfileLease::issue(
            tenant.clone(),
            profile.clone(),
            SessionId::parse(format!("session_foreign_{id}"))?,
            DeviceId::parse(format!("device_foreign_{id}"))?,
            3,
            FencingToken::parse(format!("fence_foreign_{id}"))?,
        )?;
        let key = GenerationDek::new(
            KeyId::parse("bridge_streaming_checkpoint_key_02")?,
            [0x20; 32],
        );
        let metadata = GenerationMetadata::for_reader(
            GenerationIdentity::new(
                tenant.clone(),
                profile.clone(),
                next.clone(),
                Some(n.clone()),
            ),
            key.key_id().clone(),
            NoncePrefix::new([0x21; 16]),
            65_536,
            &mut b"ciphertext".as_slice(),
        )?;
        let sealed_path = path.join("foreign.bpgc");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&sealed_path)?;
        let descriptor =
            seal_generation_streaming(&metadata, &key, &mut b"ciphertext".as_slice(), &mut file)?;
        file.sync_all()?;
        drop(file);
        assert_eq!(
            persist_streaming_exact_save_attempt(
                &root,
                &source,
                &candidate,
                &descriptor,
                &sealed_path,
                &lease,
            ),
            Err(SaveAttemptEvidenceError::Changed),
        );
        assert!(!attempt_path(&root, &tenant, &profile, &next).exists());
        assert_eq!(fs::read(source.path().join("prefs.js"))?, b"first");
        assert_eq!(fs::read(candidate.path().join("prefs.js"))?, b"second");
        let _ = crate::test_support::remove_test_root(&path);
        Ok(())
    }
}

#[cfg(test)]
mod local_unsynced_publication_tests {
    use super::{
        LocalUnsyncedIntent, STAGE_SEQUENCE, SaveAttemptEvidenceError,
        finish_exact_checkpoint_publication, finish_local_unsynced_publication,
    };
    use std::fs::{self, File};
    use std::io::{Error, ErrorKind, Write};
    use std::sync::atomic::Ordering;

    #[test]
    fn refused_exact_checkpoint_hardlink_preserves_fsynced_ciphertext()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!(
            "bridge-exact-checkpoint-publish-{}-{}",
            std::process::id(),
            STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&dir)?;
        let stage = dir.join(".profile-save-attempt-v1-abc.pending-checkpoint");
        let canonical = dir.join(".profile-save-attempt-v1-abc-checkpoint.bin");
        let serialized = b"original-fsynced-encrypted-attempt";
        let mut file = File::create(&stage)?;
        file.write_all(serialized)?;
        file.sync_all()?;
        drop(file);

        assert_eq!(
            finish_exact_checkpoint_publication(
                &stage,
                &canonical,
                b"original-header",
                b"original-ciphertext",
                Err(Error::from(ErrorKind::PermissionDenied)),
            ),
            Err(SaveAttemptEvidenceError::Io(ErrorKind::PermissionDenied)),
        );
        assert_eq!(fs::read(&stage)?, serialized);
        assert!(!canonical.exists());

        // An incompatible canonical checkpoint is not an excuse to unlink
        // the only complete staged ciphertext. Neither file is trusted.
        fs::write(&canonical, b"foreign-or-torn")?;
        assert_eq!(
            finish_exact_checkpoint_publication(
                &stage,
                &canonical,
                b"original-header",
                b"original-ciphertext",
                Err(Error::from(ErrorKind::AlreadyExists)),
            ),
            Err(SaveAttemptEvidenceError::Invalid),
        );
        assert_eq!(fs::read(&stage)?, serialized);
        assert_eq!(fs::read(&canonical)?, b"foreign-or-torn");
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn refused_hardlink_keeps_the_only_fsynced_local_intent()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!(
            "bridge-local-unsynced-publish-{}-{}",
            std::process::id(),
            STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&dir)?;
        let stage = dir.join(".profile-save-attempt-v1-abc.pending-local-unsynced");
        let canonical = dir.join(".profile-save-attempt-v1-abc-local-unsynced-v1.intent");
        let intent = LocalUnsyncedIntent {
            schema: "bridge-p3-local-unsynced-v1".to_owned(),
            tenant: "test_tenant".to_owned(),
            profile: "test_profile".to_owned(),
            base_generation: "test_generation".to_owned(),
            epoch: 4,
            session_fingerprint: "a".repeat(64),
            fencing_fingerprint: "b".repeat(64),
            device_fingerprint: Some("c".repeat(64)),
            source_inventory: 42,
            source_bytes: 11,
        };
        let serialized = serde_json::to_vec(&intent)?;
        let mut f = File::create(&stage)?;
        f.write_all(&serialized)?;
        f.sync_all()?;
        drop(f);
        assert_eq!(
            finish_local_unsynced_publication(
                &stage,
                &canonical,
                &intent,
                Err(Error::from(ErrorKind::PermissionDenied)),
            ),
            Err(SaveAttemptEvidenceError::Io(ErrorKind::PermissionDenied)),
        );
        assert_eq!(fs::read(&stage)?, serialized);
        assert!(!canonical.exists());
        // A corrupt preexisting canonical cannot silently take the place of
        // the only valid staged intent. Both must remain blocking evidence.
        fs::write(&canonical, b"foreign-or-torn")?;
        assert_eq!(
            finish_local_unsynced_publication(
                &stage,
                &canonical,
                &intent,
                Err(Error::from(ErrorKind::AlreadyExists)),
            ),
            Err(SaveAttemptEvidenceError::Invalid),
        );
        assert_eq!(fs::read(&stage)?, serialized);
        assert_eq!(fs::read(&canonical)?, b"foreign-or-torn");
        fs::remove_dir_all(dir)?;
        Ok(())
    }
}
