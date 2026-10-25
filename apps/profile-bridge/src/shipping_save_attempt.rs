//! Read-only, crash-surviving evidence for an exact P3 successor attempt.
//!
//! This is NOT an authentication token, a coordinator lease, or permission to
//! call PUT/commit after restart. Only the live retained owner can publish.
//! Recovery must obtain a fresh authoritative, ACL/fencing-safe decision before
//! using these exact bytes. Never remove an unknown record during cleanup.
use crate::dirty_generation::PreparedDirtyGeneration;
use crate::local_profile::{GenerationWorkspace, MaterializationRoot};
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
    #[must_use]
    pub(crate) fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    #[must_use]
    pub(crate) fn container_digest(&self) -> &str {
        &self.header.container_digest
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
        let path = root.path().join(format!(
            ".profile-save-attempt-v1.pending-{}-{sequence}",
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
        let _ = fs::remove_file(&path);
        return Err(error.into());
    }

    // Hardlink publishes complete synced bytes without replacing another
    // actor's existing record, on Windows and POSIX same-volume filesystems.
    let publication = fs::hard_link(&path, &canonical);
    // Failed stage cleanup is an explicit save failure. Unknown files stay
    // untouched; no broad orphan sweeper is allowed here.
    fs::remove_file(&path)?;
    match publication {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_existing_exact(&canonical, &header_bytes, ciphertext)
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

fn attempt_path(
    root: &MaterializationRoot,
    tenant: &TenantId,
    profile: &ProfileId,
    successor: &GenerationId,
) -> PathBuf {
    let mut sha = Sha256::new();
    sha.update(b"bridge-p3-save-attempt-path-v1\0");
    for part in [tenant.as_str(), profile.as_str(), successor.as_str()] {
        sha.update((part.len() as u64).to_be_bytes());
        sha.update(part.as_bytes());
    }
    root.path().join(format!(
        ".profile-save-attempt-v1-{}.bin",
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
