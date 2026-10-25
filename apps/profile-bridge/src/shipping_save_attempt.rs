//! Read-only, crash-surviving evidence for an exact P3 successor attempt.
//!
//! This is NOT an authentication token, a coordinator lease, or permission to
//! call PUT/commit after restart. Only the live retained owner can publish.
//! Recovery must obtain a fresh authoritative, ACL/fencing-safe decision before
//! using these exact bytes. Never remove an unknown record during cleanup.
use crate::dirty_generation::PreparedDirtyGeneration;
use crate::local_profile::{GenerationWorkspace, MaterializationRoot};
use crate::shipping_generation_save::CommittedGenerationSuccessor;
use encrypted_generation_domain::{
    MAX_GENERATION_CONTAINER_BYTES, inspect_generation_metadata_prelude,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use serde::{Deserialize, Serialize};
use session_domain::ProfileLease;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: &[u8; 8] = b"BPAT0001";
const MAX_HEADER_BYTES: usize = 4_096;
const MAX_RECORD_BYTES: usize = MAX_GENERATION_CONTAINER_BYTES + MAX_HEADER_BYTES + 12;
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
    epoch: u64,
    source_inventory: u64,
    candidate_inventory: u64,
    candidate_bytes: u64,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
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
    if intent.schema != "bridge-p3-local-unsynced-v1"
        || intent.tenant != tenant.as_str()
        || intent.profile != profile.as_str()
        || exact.header.schema != "bridge-p3-attempt-v1"
        || exact.header.tenant != intent.tenant
        || exact.header.profile != intent.profile
        || exact.header.base_generation != intent.base_generation
        || exact.header.epoch != intent.epoch
        || exact.header.session_fingerprint != intent.session_fingerprint
        || exact.header.fencing_fingerprint != intent.fencing_fingerprint
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
        epoch: lease.epoch(),
        source_inventory: source_inventory.inventory_digest(),
        candidate_inventory: candidate_inventory.inventory_digest(),
        candidate_bytes: candidate_inventory.total_bytes(),
        object_key: prepared.object_key(),
        metadata_digest: prepared.metadata_digest().to_owned(),
        container_digest: prepared.container_digest().to_owned(),
        container_bytes: u64::try_from(ciphertext.len())
            .map_err(|_| SaveAttemptEvidenceError::Invalid)?,
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
fn read_bounded_attempt_header(path: &PathBuf) -> Result<AttemptHeader, SaveAttemptEvidenceError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() <= 12
        || metadata.len() > MAX_RECORD_BYTES as u64
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
