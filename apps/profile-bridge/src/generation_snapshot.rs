use crate::local_profile::{
    GenerationInventory, GenerationWorkspace, LocalProfileError, MaterializationRoot,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const WORKSPACE_SNAPSHOT_MAGIC: &[u8; 8] = b"BPGW0001";
pub(crate) const MAX_WORKSPACE_SNAPSHOT_BYTES: usize = 67_108_864;
/// Maximum native Firefox bytes, separate from snapshot's framing overhead.
#[allow(dead_code)] // K2 shipping integration is still gated.
pub(crate) const MAX_STREAM_NATIVE_PROFILE_BYTES: u64 = 536_870_912; // 512 MiB
/// The same encrypted-generation-domain bound applies to BPGW0001 encoded
/// bytes, so no successful snapshot can later fail encryption from a smaller
/// policy limit. Legacy Vec API remains capped at 64 MiB.
#[allow(dead_code)] // K2 shipping integration is still gated.
pub(crate) const MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES: u64 =
    encrypted_generation_domain::MAX_STREAM_PLAINTEXT_BYTES;
const MAX_SNAPSHOT_FILES: usize = 100_000;
const MAX_RELATIVE_PATH_BYTES: usize = 512;
const STAGING_ATTEMPTS: usize = 32;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GenerationSnapshotError {
    InvalidFormat,
    SnapshotTooLarge,
    UnsafePath,
    SourceChanged,
    TargetAlreadyExists,
    PrePublish,
    Local(LocalProfileError),
}

impl fmt::Display for GenerationSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFormat => "workspace snapshot is not canonical BPGW0001",
            Self::SnapshotTooLarge => "workspace snapshot exceeds canonical size policy",
            Self::UnsafePath => "workspace snapshot contains an unsafe Windows path",
            Self::SourceChanged => "workspace changed while canonical snapshot was encoded",
            Self::TargetAlreadyExists => "authoritative generation already exists locally",
            Self::PrePublish => "authoritative generation pre-publish evidence failed",
            Self::Local(error) => {
                return write!(formatter, "workspace snapshot local failure: {error}");
            }
        })
    }
}

impl std::error::Error for GenerationSnapshotError {}

impl From<LocalProfileError> for GenerationSnapshotError {
    fn from(error: LocalProfileError) -> Self {
        Self::Local(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // K2 production streaming adapter is not yet active.
pub(crate) struct StreamedWorkspaceSnapshot {
    pub(crate) plaintext_bytes: u64,
    pub(crate) plaintext_digest: [u8; 32],
}

/// Canonical BPGW0001, but no complete Firefox file or profile is held in
/// memory. Writes to an UNPUBLISHED output: caller must fsync/no-replace
/// and publish only after success and exact source inventory revalidation.
#[allow(dead_code)] // K2 production streaming adapter is not yet active.
pub(crate) fn encode_workspace_snapshot_streaming(
    workspace: &GenerationWorkspace,
    expected_inventory: &GenerationInventory,
    output: &mut impl Write,
) -> Result<StreamedWorkspaceSnapshot, GenerationSnapshotError> {
    // Check *native* size and the exact BPGW0001 wire size before writing
    // a single byte to the unpublished spool. Metadata overhead from many
    // tiny Firefox files must not disqualify an otherwise valid 512 MiB
    // native profile, nor can a huge inventory fill disk before rejection.
    let expected_length = encoded_stream_snapshot_size(expected_inventory)?;
    let count = u32::try_from(expected_inventory.entries().len())
        .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
    let mut length = 0_u64;
    let mut digest = Sha256::new();
    write_stream_piece(output, &mut digest, &mut length, WORKSPACE_SNAPSHOT_MAGIC)?;
    write_stream_piece(output, &mut digest, &mut length, &count.to_be_bytes())?;
    let mut previous: Option<&str> = None;
    let mut seen = HashSet::new();
    let mut buffer = [0_u8; 65_536];
    for entry in expected_inventory.entries() {
        let relative = entry.relative_path();
        validate_windows_relative_path(relative)?;
        if previous.is_some_and(|p| p >= relative) || !seen.insert(relative.to_lowercase()) {
            return Err(GenerationSnapshotError::UnsafePath);
        }
        previous = Some(relative);
        let name = relative.as_bytes();
        let name_length =
            u16::try_from(name.len()).map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
        write_stream_piece(output, &mut digest, &mut length, &name_length.to_be_bytes())?;
        write_stream_piece(output, &mut digest, &mut length, name)?;
        write_stream_piece(
            output,
            &mut digest,
            &mut length,
            &entry.bytes().to_be_bytes(),
        )?;
        let full = workspace.path().join(relative);
        let metadata = fs::symlink_metadata(&full).map_err(LocalProfileError::from)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != entry.bytes()
        {
            return Err(GenerationSnapshotError::SourceChanged);
        }
        let mut file = File::open(&full).map_err(LocalProfileError::from)?;
        let mut remaining = entry.bytes();
        while remaining > 0 {
            let take = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
            file.read_exact(&mut buffer[..take])
                .map_err(LocalProfileError::from)?;
            write_stream_piece(output, &mut digest, &mut length, &buffer[..take])?;
            remaining -= take as u64;
        }
    }
    if workspace
        .inventory()
        .map_err(GenerationSnapshotError::Local)?
        != *expected_inventory
    {
        return Err(GenerationSnapshotError::SourceChanged);
    }
    if length != expected_length {
        return Err(GenerationSnapshotError::SourceChanged);
    }
    Ok(StreamedWorkspaceSnapshot {
        plaintext_bytes: length,
        plaintext_digest: digest.finalize().into(),
    })
}

/// Preflight against the same canonical BPGW0001 layout used by the actual
/// stream. This avoids writing any staging bytes for oversized input while
/// retaining 64 KiB bounded file reads for accepted profiles.
#[allow(dead_code)] // K2 production streaming adapter is still gated.
fn encoded_stream_snapshot_size(
    inventory: &GenerationInventory,
) -> Result<u64, GenerationSnapshotError> {
    if inventory.entries().len() > MAX_SNAPSHOT_FILES
        || inventory.total_bytes() > MAX_STREAM_NATIVE_PROFILE_BYTES
    {
        return Err(GenerationSnapshotError::SnapshotTooLarge);
    }
    let mut size = 12_u64; // BPGW0001 + u32 file count
    for entry in inventory.entries() {
        let path_len = entry.relative_path().len();
        if path_len == 0 || path_len > MAX_RELATIVE_PATH_BYTES {
            return Err(GenerationSnapshotError::UnsafePath);
        }
        // 2 bytes path length + path + 8 bytes file length + exact content.
        size = size
            .checked_add(2 + path_len as u64 + 8)
            .and_then(|value| value.checked_add(entry.bytes()))
            .ok_or(GenerationSnapshotError::SnapshotTooLarge)?;
        if size > MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES {
            return Err(GenerationSnapshotError::SnapshotTooLarge);
        }
    }
    Ok(size)
}

/// On-demand canonical BPGW0001 reader over the ORIGINAL closed Firefox
/// files. No plaintext disk spool and no generation-sized in-memory buffer.
/// Construct a fresh reader for the metadata digest pass and the exact seal
/// pass; the sealing owner must hold the original device writer lock.
#[allow(dead_code)] // Activated only with the K2 shipping cutover.
pub(crate) struct WorkspaceSnapshotReader<'a> {
    workspace: &'a GenerationWorkspace,
    inventory: &'a GenerationInventory,
    expected_bytes: u64,
    emitted_bytes: u64,
    pending: Vec<u8>,
    pending_offset: usize,
    next_entry: usize,
    current_file: Option<File>,
    remaining_file_bytes: u64,
    verified_eof: bool,
}

#[allow(dead_code)] // The legacy 64 MiB shipping path has not switched yet.
impl<'a> WorkspaceSnapshotReader<'a> {
    pub(crate) fn new(
        workspace: &'a GenerationWorkspace,
        inventory: &'a GenerationInventory,
    ) -> Result<Self, GenerationSnapshotError> {
        let expected_bytes = encoded_stream_snapshot_size(inventory)?;
        let mut previous: Option<&str> = None;
        let mut seen = HashSet::new();
        for entry in inventory.entries() {
            let relative = entry.relative_path();
            validate_windows_relative_path(relative)?;
            if previous.is_some_and(|p| p >= relative) || !seen.insert(relative.to_lowercase()) {
                return Err(GenerationSnapshotError::UnsafePath);
            }
            previous = Some(relative);
        }
        let mut pending = Vec::with_capacity(12);
        pending.extend_from_slice(WORKSPACE_SNAPSHOT_MAGIC);
        let entries = u32::try_from(inventory.entries().len())
            .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
        pending.extend_from_slice(&entries.to_be_bytes());
        Ok(Self {
            workspace,
            inventory,
            expected_bytes,
            emitted_bytes: 0,
            pending,
            pending_offset: 0,
            next_entry: 0,
            current_file: None,
            remaining_file_bytes: 0,
            verified_eof: false,
        })
    }

    #[must_use]
    pub(crate) const fn expected_bytes(&self) -> u64 {
        self.expected_bytes
    }

    fn advance_written(&mut self, count: usize) -> std::io::Result<()> {
        self.emitted_bytes = self
            .emitted_bytes
            .checked_add(count as u64)
            .ok_or_else(snapshot_reader_changed)?;
        if self.emitted_bytes > self.expected_bytes {
            return Err(snapshot_reader_changed());
        }
        Ok(())
    }
}

impl Read for WorkspaceSnapshotReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.pending_offset < self.pending.len() {
                let available = self.pending.len() - self.pending_offset;
                let count = available.min(output.len());
                output[..count].copy_from_slice(
                    &self.pending[self.pending_offset..self.pending_offset + count],
                );
                self.pending_offset += count;
                self.advance_written(count)?;
                return Ok(count);
            }
            self.pending.clear();
            self.pending_offset = 0;

            if let Some(file) = self.current_file.as_mut() {
                if self.remaining_file_bytes != 0 {
                    let allowed =
                        usize::try_from(self.remaining_file_bytes.min(output.len() as u64))
                            .map_err(|_| snapshot_reader_changed())?;
                    let count = file.read(&mut output[..allowed])?;
                    if count == 0 {
                        return Err(snapshot_reader_changed());
                    }
                    self.remaining_file_bytes -= count as u64;
                    self.advance_written(count)?;
                    return Ok(count);
                }
                self.current_file = None;
            }

            if let Some(entry) = self.inventory.entries().get(self.next_entry) {
                self.next_entry += 1;
                let relative = entry.relative_path();
                // The relative path has been checked before any bytes are
                // exposed. Refuse a changed/symlink target on both passes.
                let path = self.workspace.path().join(relative);
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.len() != entry.bytes()
                {
                    return Err(snapshot_reader_changed());
                }
                let file = File::open(&path)?;
                if !file.metadata()?.is_file() || file.metadata()?.len() != entry.bytes() {
                    return Err(snapshot_reader_changed());
                }
                self.current_file = Some(file);
                self.remaining_file_bytes = entry.bytes();
                let name = relative.as_bytes();
                let name_len = u16::try_from(name.len()).map_err(|_| snapshot_reader_changed())?;
                self.pending.extend_from_slice(&name_len.to_be_bytes());
                self.pending.extend_from_slice(name);
                self.pending.extend_from_slice(&entry.bytes().to_be_bytes());
                continue;
            }

            if !self.verified_eof {
                // Checking only the first inventory would miss mutation
                // between the metadata pass and ciphertext sealing pass.
                let current = self
                    .workspace
                    .inventory()
                    .map_err(|_| snapshot_reader_changed())?;
                if current != *self.inventory || self.emitted_bytes != self.expected_bytes {
                    return Err(snapshot_reader_changed());
                }
                self.verified_eof = true;
            }
            return Ok(0);
        }
    }
}

fn snapshot_reader_changed() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Firefox snapshot source changed during streaming",
    )
}

#[allow(dead_code)]
fn write_stream_piece(
    output: &mut impl Write,
    digest: &mut Sha256,
    length: &mut u64,
    piece: &[u8],
) -> Result<(), GenerationSnapshotError> {
    let next = length
        .checked_add(piece.len() as u64)
        .ok_or(GenerationSnapshotError::SnapshotTooLarge)?;
    if next > MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES {
        return Err(GenerationSnapshotError::SnapshotTooLarge);
    }
    output.write_all(piece).map_err(LocalProfileError::from)?;
    digest.update(piece);
    *length = next;
    Ok(())
}

pub(crate) fn encode_workspace_snapshot(
    workspace: &GenerationWorkspace,
    expected_inventory: &GenerationInventory,
) -> Result<Vec<u8>, GenerationSnapshotError> {
    let entry_count = u32::try_from(expected_inventory.entries().len())
        .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
    let mut output = Vec::new();
    output.extend_from_slice(WORKSPACE_SNAPSHOT_MAGIC);
    output.extend_from_slice(&entry_count.to_be_bytes());

    for entry in expected_inventory.entries() {
        let path = entry.relative_path().as_bytes();
        let path_length =
            u16::try_from(path.len()).map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
        checked_extend(&mut output, &path_length.to_be_bytes())?;
        checked_extend(&mut output, path)?;
        checked_extend(&mut output, &entry.bytes().to_be_bytes())?;

        let full_path = workspace.path().join(entry.relative_path());
        let metadata = fs::symlink_metadata(&full_path).map_err(LocalProfileError::from)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != entry.bytes()
        {
            return Err(GenerationSnapshotError::SourceChanged);
        }
        let expected_bytes = usize::try_from(entry.bytes())
            .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
        let remaining = MAX_WORKSPACE_SNAPSHOT_BYTES
            .checked_sub(output.len())
            .ok_or(GenerationSnapshotError::SnapshotTooLarge)?;
        if expected_bytes > remaining {
            return Err(GenerationSnapshotError::SnapshotTooLarge);
        }
        let mut file = File::open(&full_path).map_err(LocalProfileError::from)?;
        let start = output.len();
        output.resize(
            start
                .checked_add(expected_bytes)
                .ok_or(GenerationSnapshotError::SnapshotTooLarge)?,
            0,
        );
        file.read_exact(&mut output[start..])
            .map_err(LocalProfileError::from)?;
    }

    if workspace
        .inventory()
        .map_err(GenerationSnapshotError::Local)?
        != *expected_inventory
    {
        return Err(GenerationSnapshotError::SourceChanged);
    }
    Ok(output)
}

#[derive(Clone)]
struct StreamedSnapshotFile {
    relative_path: String,
    bytes: u64,
    digest: [u8; 32],
}

/// Reopen BPGW0001 from a VERIFIED DECRYPTED spool (never directly from an
/// unauthenticated R2 stream). The caller passes the digest/length established
/// by the authenticated BPGC0001 metadata. No generation path is made visible
/// until all filenames, all content, strict EOF, SHA-256, and staged disk
/// rereads agree. Never overwrite a current N or any previous LKG.
#[allow(dead_code)] // K2 shipping download interface is still Vec-based.
pub(crate) fn materialize_workspace_snapshot_streaming_with_pre_publish(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    source: &mut impl Read,
    expected: StreamedWorkspaceSnapshot,
    mut before_publish: impl FnMut(&GenerationWorkspace) -> Result<(), GenerationSnapshotError>,
) -> Result<GenerationWorkspace, GenerationSnapshotError> {
    if expected.plaintext_bytes > MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES {
        return Err(GenerationSnapshotError::SnapshotTooLarge);
    }
    match root.open_generation(tenant_id, profile_id, generation_id) {
        Ok(_) => return Err(GenerationSnapshotError::TargetAlreadyExists),
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => {}
        Err(error) => return Err(GenerationSnapshotError::Local(error)),
    }
    let staged = create_staging_workspace(root, tenant_id, profile_id)?;
    let staging_path = staged.path().to_path_buf();
    let result = materialize_stream_into_staging(
        root,
        staged,
        &staging_path,
        tenant_id,
        profile_id,
        generation_id,
        source,
        expected,
        &mut before_publish,
    );
    if result.is_err() {
        remove_owned_staging(&staging_path);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn materialize_stream_into_staging(
    root: &MaterializationRoot,
    staging: GenerationWorkspace,
    staging_path: &Path,
    tenant: &TenantId,
    profile: &ProfileId,
    generation: &GenerationId,
    source: &mut impl Read,
    expected: StreamedWorkspaceSnapshot,
    before_publish: &mut impl FnMut(&GenerationWorkspace) -> Result<(), GenerationSnapshotError>,
) -> Result<GenerationWorkspace, GenerationSnapshotError> {
    let mut total = 0_u64;
    let mut hash = Sha256::new();
    let mut magic = [0_u8; 8];
    read_stream_piece(source, &mut hash, &mut total, &mut magic)?;
    if &magic != WORKSPACE_SNAPSHOT_MAGIC {
        return Err(GenerationSnapshotError::InvalidFormat);
    }
    let mut count_bytes = [0_u8; 4];
    read_stream_piece(source, &mut hash, &mut total, &mut count_bytes)?;
    let count = u32::from_be_bytes(count_bytes) as usize;
    if count > MAX_SNAPSHOT_FILES {
        return Err(GenerationSnapshotError::SnapshotTooLarge);
    }
    let mut entries = Vec::with_capacity(count);
    let mut native_total = 0_u64;
    let mut seen = HashSet::new();
    let mut buffer = [0_u8; 65_536];
    let mut previous = String::new();
    for _ in 0..count {
        let mut length = [0_u8; 2];
        read_stream_piece(source, &mut hash, &mut total, &mut length)?;
        let path_len = u16::from_be_bytes(length) as usize;
        if path_len == 0 || path_len > MAX_RELATIVE_PATH_BYTES {
            return Err(GenerationSnapshotError::UnsafePath);
        }
        let mut path = vec![0_u8; path_len];
        read_stream_piece(source, &mut hash, &mut total, &mut path)?;
        let relative = String::from_utf8(path).map_err(|_| GenerationSnapshotError::UnsafePath)?;
        validate_windows_relative_path(&relative)?;
        if (!previous.is_empty() && previous.as_str() >= relative.as_str())
            || !seen.insert(relative.to_lowercase())
        {
            return Err(GenerationSnapshotError::UnsafePath);
        }
        previous.clone_from(&relative);
        let mut content_len = [0_u8; 8];
        read_stream_piece(source, &mut hash, &mut total, &mut content_len)?;
        let bytes = u64::from_be_bytes(content_len);
        native_total = native_total
            .checked_add(bytes)
            .ok_or(GenerationSnapshotError::SnapshotTooLarge)?;
        if native_total > MAX_STREAM_NATIVE_PROFILE_BYTES {
            return Err(GenerationSnapshotError::SnapshotTooLarge);
        }
        if total
            .checked_add(bytes)
            .is_none_or(|sum| sum > expected.plaintext_bytes)
        {
            return Err(GenerationSnapshotError::SnapshotTooLarge);
        }
        let target = staging_path.join(&relative);
        let parent = target.parent().ok_or(GenerationSnapshotError::UnsafePath)?;
        ensure_descendant_directories(staging_path, parent)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(LocalProfileError::from)?;
        let mut remaining = bytes;
        let mut file_hash = Sha256::new();
        while remaining > 0 {
            let count = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
            read_stream_piece(source, &mut hash, &mut total, &mut buffer[..count])?;
            file.write_all(&buffer[..count])
                .map_err(LocalProfileError::from)?;
            file_hash.update(&buffer[..count]);
            remaining -= count as u64;
        }
        file.sync_all().map_err(LocalProfileError::from)?;
        entries.push(StreamedSnapshotFile {
            relative_path: relative,
            bytes,
            digest: file_hash.finalize().into(),
        });
    }
    let mut eof = [0_u8; 1];
    if source.read(&mut eof).map_err(LocalProfileError::from)? != 0
        || total != expected.plaintext_bytes
        || <[u8; 32]>::from(hash.finalize()) != expected.plaintext_digest
    {
        return Err(GenerationSnapshotError::InvalidFormat);
    }
    verify_streamed_workspace(&staging, &entries)?;
    before_publish(&staging)?;

    // Identical no-replace atomic publish boundary to the in-memory reader.
    let final_path = root
        .prepare_generation_path(tenant, profile, generation)
        .map_err(|error| match error {
            LocalProfileError::TargetAlreadyExists => GenerationSnapshotError::TargetAlreadyExists,
            error => GenerationSnapshotError::Local(error),
        })?;
    if fs::symlink_metadata(&final_path).is_ok() {
        return Err(GenerationSnapshotError::TargetAlreadyExists);
    }
    fs::rename(staging_path, &final_path).map_err(LocalProfileError::from)?;
    drop(staging);
    let workspace = root
        .open_generation(tenant, profile, generation)
        .map_err(GenerationSnapshotError::Local)?;
    verify_streamed_workspace(&workspace, &entries)?;
    Ok(workspace)
}

fn verify_streamed_workspace(
    workspace: &GenerationWorkspace,
    entries: &[StreamedSnapshotFile],
) -> Result<(), GenerationSnapshotError> {
    let inventory = workspace
        .inventory()
        .map_err(GenerationSnapshotError::Local)?;
    if inventory.entries().len() != entries.len() {
        return Err(GenerationSnapshotError::SourceChanged);
    }
    let mut buffer = [0_u8; 65_536];
    for (actual, original) in inventory.entries().iter().zip(entries) {
        if actual.relative_path() != original.relative_path || actual.bytes() != original.bytes {
            return Err(GenerationSnapshotError::SourceChanged);
        }
        let path = workspace.path().join(&original.relative_path);
        let metadata = fs::symlink_metadata(&path).map_err(LocalProfileError::from)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != original.bytes
        {
            return Err(GenerationSnapshotError::SourceChanged);
        }
        let mut file = File::open(&path).map_err(LocalProfileError::from)?;
        let mut digest = Sha256::new();
        loop {
            let count = file.read(&mut buffer).map_err(LocalProfileError::from)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        if <[u8; 32]>::from(digest.finalize()) != original.digest {
            return Err(GenerationSnapshotError::SourceChanged);
        }
    }
    Ok(())
}

fn read_stream_piece(
    source: &mut impl Read,
    hash: &mut Sha256,
    total: &mut u64,
    buffer: &mut [u8],
) -> Result<(), GenerationSnapshotError> {
    let next = total
        .checked_add(buffer.len() as u64)
        .ok_or(GenerationSnapshotError::SnapshotTooLarge)?;
    if next > MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES {
        return Err(GenerationSnapshotError::SnapshotTooLarge);
    }
    source.read_exact(buffer).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            GenerationSnapshotError::InvalidFormat
        } else {
            GenerationSnapshotError::Local(error.into())
        }
    })?;
    hash.update(buffer);
    *total = next;
    Ok(())
}

#[cfg(test)]
pub(crate) fn materialize_workspace_snapshot(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    snapshot: &[u8],
) -> Result<GenerationWorkspace, GenerationSnapshotError> {
    materialize_workspace_snapshot_with_pre_publish(
        root,
        tenant_id,
        profile_id,
        generation_id,
        snapshot,
        |_| Ok(()),
    )
}

pub(crate) fn materialize_workspace_snapshot_with_pre_publish(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    snapshot: &[u8],
    mut before_publish: impl FnMut(&GenerationWorkspace) -> Result<(), GenerationSnapshotError>,
) -> Result<GenerationWorkspace, GenerationSnapshotError> {
    let entries = parse_workspace_snapshot(snapshot)?;
    match root.open_generation(tenant_id, profile_id, generation_id) {
        Ok(_) => return Err(GenerationSnapshotError::TargetAlreadyExists),
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => {}
        Err(error) => return Err(GenerationSnapshotError::Local(error)),
    }

    let staging = create_staging_workspace(root, tenant_id, profile_id)?;
    let staging_path = staging.path().to_path_buf();
    let result = materialize_into_staging(
        root,
        staging,
        &staging_path,
        tenant_id,
        profile_id,
        generation_id,
        &entries,
        &mut before_publish,
    );
    if result.is_err() {
        remove_owned_staging(&staging_path);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn materialize_into_staging(
    root: &MaterializationRoot,
    staging: GenerationWorkspace,
    staging_path: &Path,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    entries: &[SnapshotEntry<'_>],
    before_publish: &mut impl FnMut(&GenerationWorkspace) -> Result<(), GenerationSnapshotError>,
) -> Result<GenerationWorkspace, GenerationSnapshotError> {
    for entry in entries {
        let target = staging_path.join(entry.relative_path);
        let parent = target.parent().ok_or(GenerationSnapshotError::UnsafePath)?;
        ensure_descendant_directories(staging_path, parent)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(LocalProfileError::from)?;
        file.write_all(entry.content)
            .map_err(LocalProfileError::from)?;
        file.sync_all().map_err(LocalProfileError::from)?;
        let metadata = file.metadata().map_err(LocalProfileError::from)?;
        if !metadata.is_file()
            || metadata.len()
                != u64::try_from(entry.content.len())
                    .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?
        {
            return Err(GenerationSnapshotError::SourceChanged);
        }
    }
    verify_materialized_snapshot(&staging, entries)?;
    before_publish(&staging)?;

    let final_path = root
        .prepare_generation_path(tenant_id, profile_id, generation_id)
        .map_err(|error| match error {
            LocalProfileError::TargetAlreadyExists => GenerationSnapshotError::TargetAlreadyExists,
            error => GenerationSnapshotError::Local(error),
        })?;
    if fs::symlink_metadata(&final_path).is_ok() {
        return Err(GenerationSnapshotError::TargetAlreadyExists);
    }
    fs::rename(staging_path, &final_path).map_err(LocalProfileError::from)?;
    drop(staging);
    let workspace = root
        .open_generation(tenant_id, profile_id, generation_id)
        .map_err(GenerationSnapshotError::Local)?;
    verify_materialized_snapshot(&workspace, entries)?;
    Ok(workspace)
}

fn create_staging_workspace(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
) -> Result<GenerationWorkspace, GenerationSnapshotError> {
    for _ in 0..STAGING_ATTEMPTS {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staging_id =
            GenerationId::parse(format!("reopen_stage_{}_{}", std::process::id(), sequence))
                .map_err(|_| GenerationSnapshotError::InvalidFormat)?;
        match root.create_generation(tenant_id, profile_id, &staging_id) {
            Ok(workspace) => return Ok(workspace),
            Err(LocalProfileError::TargetAlreadyExists) => continue,
            Err(error) => return Err(GenerationSnapshotError::Local(error)),
        }
    }
    Err(GenerationSnapshotError::TargetAlreadyExists)
}

/// Exact full-byte comparison with server-authenticated workspace, including user_data.
/// Immutable inventory alone cannot prove absence of unsaved browser state.
pub(crate) fn verify_workspace_matches_snapshot(
    workspace: &GenerationWorkspace,
    snapshot: &[u8],
) -> Result<(), GenerationSnapshotError> {
    let entries = parse_workspace_snapshot(snapshot)?;
    verify_materialized_snapshot(workspace, &entries)
}

fn verify_materialized_snapshot(
    workspace: &GenerationWorkspace,
    expected: &[SnapshotEntry<'_>],
) -> Result<(), GenerationSnapshotError> {
    let inventory = workspace
        .inventory()
        .map_err(GenerationSnapshotError::Local)?;
    if inventory.entries().len() != expected.len() {
        return Err(GenerationSnapshotError::SourceChanged);
    }
    for (actual, expected) in inventory.entries().iter().zip(expected) {
        if actual.relative_path() != expected.relative_path
            || actual.bytes()
                != u64::try_from(expected.content.len())
                    .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?
        {
            return Err(GenerationSnapshotError::SourceChanged);
        }
        let full_path = workspace.path().join(expected.relative_path);
        let metadata = fs::symlink_metadata(&full_path).map_err(LocalProfileError::from)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(GenerationSnapshotError::SourceChanged);
        }
        let bytes = fs::read(&full_path).map_err(LocalProfileError::from)?;
        if bytes.as_slice() != expected.content {
            return Err(GenerationSnapshotError::SourceChanged);
        }
    }
    Ok(())
}

fn ensure_descendant_directories(
    staging_root: &Path,
    parent: &Path,
) -> Result<(), GenerationSnapshotError> {
    let relative = parent
        .strip_prefix(staging_root)
        .map_err(|_| GenerationSnapshotError::UnsafePath)?;
    let mut current = staging_root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(value) = component else {
            return Err(GenerationSnapshotError::UnsafePath);
        };
        current.push(value);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(GenerationSnapshotError::UnsafePath);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(LocalProfileError::from)?;
                let metadata = fs::symlink_metadata(&current).map_err(LocalProfileError::from)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(GenerationSnapshotError::UnsafePath);
                }
            }
            Err(error) => return Err(GenerationSnapshotError::Local(error.into())),
        }
    }
    Ok(())
}

fn remove_owned_staging(path: &Path) {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            let _ = fs::remove_file(path);
        } else if metadata.is_dir() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[derive(Clone, Copy)]
struct SnapshotEntry<'a> {
    relative_path: &'a str,
    content: &'a [u8],
}

fn parse_workspace_snapshot(
    snapshot: &[u8],
) -> Result<Vec<SnapshotEntry<'_>>, GenerationSnapshotError> {
    if snapshot.len() > MAX_WORKSPACE_SNAPSHOT_BYTES
        || snapshot.len() < WORKSPACE_SNAPSHOT_MAGIC.len() + 4
        || &snapshot[..WORKSPACE_SNAPSHOT_MAGIC.len()] != WORKSPACE_SNAPSHOT_MAGIC
    {
        return Err(GenerationSnapshotError::InvalidFormat);
    }
    let mut cursor = WORKSPACE_SNAPSHOT_MAGIC.len();
    let count = usize::try_from(read_u32(snapshot, &mut cursor)?)
        .map_err(|_| GenerationSnapshotError::InvalidFormat)?;
    if count > MAX_SNAPSHOT_FILES {
        return Err(GenerationSnapshotError::InvalidFormat);
    }

    let mut entries = Vec::with_capacity(count);
    let mut previous_path: Option<&str> = None;
    let mut casefolded_paths = HashSet::with_capacity(count);
    for _ in 0..count {
        let path_len = usize::from(read_u16(snapshot, &mut cursor)?);
        if path_len == 0 || path_len > MAX_RELATIVE_PATH_BYTES {
            return Err(GenerationSnapshotError::UnsafePath);
        }
        let path_bytes = read_bytes(snapshot, &mut cursor, path_len)?;
        let relative_path =
            std::str::from_utf8(path_bytes).map_err(|_| GenerationSnapshotError::UnsafePath)?;
        validate_windows_relative_path(relative_path)?;
        if previous_path.is_some_and(|previous| previous >= relative_path) {
            return Err(GenerationSnapshotError::InvalidFormat);
        }
        let casefolded = relative_path.to_lowercase();
        if !casefolded_paths.insert(casefolded) {
            return Err(GenerationSnapshotError::UnsafePath);
        }
        let content_len = usize::try_from(read_u64(snapshot, &mut cursor)?)
            .map_err(|_| GenerationSnapshotError::SnapshotTooLarge)?;
        let content = read_bytes(snapshot, &mut cursor, content_len)?;
        entries.push(SnapshotEntry {
            relative_path,
            content,
        });
        previous_path = Some(relative_path);
    }
    if cursor != snapshot.len() {
        return Err(GenerationSnapshotError::InvalidFormat);
    }
    Ok(entries)
}

fn validate_windows_relative_path(path: &str) -> Result<(), GenerationSnapshotError> {
    if path.is_empty()
        || path.len() > MAX_RELATIVE_PATH_BYTES
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains("//")
        || matches!(
            path,
            ".profile-generation"
                | ".profile-platform.lock"
                | "user_data/.parentlock"
                | "user_data/parent.lock"
                | "user_data/lock"
        )
    {
        return Err(GenerationSnapshotError::UnsafePath);
    }
    for component in path.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with(['.', ' '])
            || component.bytes().any(|byte| {
                byte.is_ascii_control()
                    || matches!(byte, b'<' | b'>' | b':' | b'"' | b'|' | b'?' | b'*')
            })
            || windows_reserved_component(component)
        {
            return Err(GenerationSnapshotError::UnsafePath);
        }
    }
    Ok(())
}

fn windows_reserved_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn read_u16(snapshot: &[u8], cursor: &mut usize) -> Result<u16, GenerationSnapshotError> {
    let bytes = read_bytes(snapshot, cursor, 2)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(snapshot: &[u8], cursor: &mut usize) -> Result<u32, GenerationSnapshotError> {
    let bytes = read_bytes(snapshot, cursor, 4)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(snapshot: &[u8], cursor: &mut usize) -> Result<u64, GenerationSnapshotError> {
    let bytes = read_bytes(snapshot, cursor, 8)?;
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn read_bytes<'a>(
    snapshot: &'a [u8],
    cursor: &mut usize,
    len: usize,
) -> Result<&'a [u8], GenerationSnapshotError> {
    let end = cursor
        .checked_add(len)
        .ok_or(GenerationSnapshotError::InvalidFormat)?;
    let bytes = snapshot
        .get(*cursor..end)
        .ok_or(GenerationSnapshotError::InvalidFormat)?;
    *cursor = end;
    Ok(bytes)
}

fn checked_extend(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), GenerationSnapshotError> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(GenerationSnapshotError::SnapshotTooLarge)?;
    if new_len > MAX_WORKSPACE_SNAPSHOT_BYTES {
        return Err(GenerationSnapshotError::SnapshotTooLarge);
    }
    output.extend_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        GenerationSnapshotError, StreamedWorkspaceSnapshot, WORKSPACE_SNAPSHOT_MAGIC,
        WorkspaceSnapshotReader, encode_workspace_snapshot, encode_workspace_snapshot_streaming,
        encoded_stream_snapshot_size, materialize_workspace_snapshot,
        materialize_workspace_snapshot_streaming_with_pre_publish,
        materialize_workspace_snapshot_with_pre_publish, parse_workspace_snapshot,
    };
    use crate::local_profile::MaterializationRoot;
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
    use sha2::Digest;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct Fixture {
        root_path: PathBuf,
        root: MaterializationRoot,
        tenant_id: TenantId,
        profile_id: ProfileId,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-generation-snapshot-{}-{counter}",
                std::process::id()
            ));
            Ok(Self {
                root: MaterializationRoot::open_or_create(root_path.clone())?,
                root_path,
                tenant_id: TenantId::parse(format!("tenant_snap_{counter}"))?,
                profile_id: ProfileId::parse(format!("profile_snap_{counter}"))?,
            })
        }

        fn cleanup(&self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    fn raw_snapshot(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(WORKSPACE_SNAPSHOT_MAGIC);
        output.extend_from_slice(&u32::try_from(entries.len()).unwrap_or(0).to_be_bytes());
        for (path, content) in entries {
            output.extend_from_slice(&u16::try_from(path.len()).unwrap_or(0).to_be_bytes());
            output.extend_from_slice(path.as_bytes());
            output.extend_from_slice(&u64::try_from(content.len()).unwrap_or(0).to_be_bytes());
            output.extend_from_slice(content);
        }
        output
    }

    #[test]
    fn full_512_mib_firefox_native_profile_fits_maximum_canonical_framing() {
        let raw = 536_870_912_u64;
        let max_file_count = 100_000_u64;
        let max_path_bytes = 512_u64;
        let encoded = 12 + raw + max_file_count * (2 + max_path_bytes + 8);
        assert_eq!(super::MAX_STREAM_NATIVE_PROFILE_BYTES, raw);
        assert_eq!(
            super::MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES,
            encrypted_generation_domain::MAX_STREAM_PLAINTEXT_BYTES,
        );
        assert!(
            encoded <= super::MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES,
            "512 MiB native profiles must not be rejected solely due to BPGW metadata",
        );
    }

    #[test]
    fn direct_two_pass_snapshot_reader_seals_original_firefox_without_plaintext_spool()
    -> Result<(), Box<dyn std::error::Error>> {
        use encrypted_generation_domain::{
            GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
            open_generation_streaming, seal_generation_streaming,
        };
        use std::io::Read;
        let fixture = Fixture::new()?;
        let result = (|| {
            let base = GenerationId::parse("generation_direct_streaming_reader")?;
            let next = GenerationId::parse("generation_direct_streaming_successor")?;
            let source =
                fixture
                    .root
                    .create_generation(&fixture.tenant_id, &fixture.profile_id, &base)?;
            fs::write(source.path().join("prefs.js"), b"user_pref('local', true);")?;
            let directory = source.path().join("user_data");
            fs::create_dir_all(&directory)?;
            fs::write(directory.join("cookies.sqlite-wal"), vec![0x37; 177_013])?;
            fs::write(directory.join("empty.sqlite"), b"")?;
            let inventory = source.inventory()?;
            let canonical = encode_workspace_snapshot(&source, &inventory)?;
            let mut digest_reader = WorkspaceSnapshotReader::new(&source, &inventory)?;
            assert_eq!(digest_reader.expected_bytes(), canonical.len() as u64);
            let key = GenerationDek::new(KeyId::parse("direct_streaming_reader_key")?, [0x7e; 32]);
            let metadata = GenerationMetadata::for_reader(
                GenerationIdentity::new(
                    fixture.tenant_id.clone(),
                    fixture.profile_id.clone(),
                    next,
                    Some(base),
                ),
                key.key_id().clone(),
                NoncePrefix::new([0x54; 16]),
                65_536,
                &mut digest_reader,
            )?;
            let mut seal_reader = WorkspaceSnapshotReader::new(&source, &inventory)?;
            let mut encrypted = Vec::new();
            let report =
                seal_generation_streaming(&metadata, &key, &mut seal_reader, &mut encrypted)?;
            assert_eq!(report.container_bytes(), encrypted.len() as u64);
            let mut decrypted = Vec::new();
            let opened =
                open_generation_streaming(&mut encrypted.as_slice(), &key, &mut decrypted)?;
            assert_eq!(report, opened);
            assert_eq!(decrypted, canonical);
            let mut eof = [0_u8; 4];
            assert_eq!(seal_reader.read(&mut eof)?, 0);
            assert_eq!(source.inventory()?, inventory);
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        fixture.cleanup();
        result
    }

    #[test]
    fn native_mutation_between_streaming_passes_fails_closed_without_new_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        use encrypted_generation_domain::{
            GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
            seal_generation_streaming,
        };
        let fixture = Fixture::new()?;
        let result = (|| {
            let base = GenerationId::parse("generation_direct_changed_base")?;
            let next = GenerationId::parse("generation_direct_changed_next")?;
            let source =
                fixture
                    .root
                    .create_generation(&fixture.tenant_id, &fixture.profile_id, &base)?;
            let native = source.path().join("prefs.js");
            fs::write(&native, b"original Firefox native bytes")?;
            let before = source.inventory()?;
            let key = GenerationDek::new(KeyId::parse("direct_streaming_changed_key")?, [0x5c; 32]);
            let mut digest_reader = WorkspaceSnapshotReader::new(&source, &before)?;
            let metadata = GenerationMetadata::for_reader(
                GenerationIdentity::new(
                    fixture.tenant_id.clone(),
                    fixture.profile_id.clone(),
                    next.clone(),
                    Some(base),
                ),
                key.key_id().clone(),
                NoncePrefix::new([0x32; 16]),
                65_536,
                &mut digest_reader,
            )?;
            fs::write(&native, b"modified Firefox native bytes")?;
            let mut seal_reader = WorkspaceSnapshotReader::new(&source, &before)?;
            let mut unpublishable_ciphertext = Vec::new();
            assert!(
                seal_generation_streaming(
                    &metadata,
                    &key,
                    &mut seal_reader,
                    &mut unpublishable_ciphertext,
                )
                .is_err()
            );
            assert_eq!(fs::read(&native)?, b"modified Firefox native bytes");
            assert!(
                fixture
                    .root
                    .open_generation(&fixture.tenant_id, &fixture.profile_id, &next,)
                    .is_err()
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        fixture.cleanup();
        result
    }

    #[test]
    fn streamed_snapshot_is_byte_identical_and_digest_identical_to_legacy()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let result = (|| {
            let generation = GenerationId::parse("generation_streaming_snapshot")?;
            let source = fixture.root.create_generation(
                &fixture.tenant_id,
                &fixture.profile_id,
                &generation,
            )?;
            fs::write(source.path().join("prefs.js"), b"user_pref('a', true);")?;
            let directory = source.path().join("user_data");
            fs::create_dir_all(&directory)?;
            fs::write(directory.join("cookies.sqlite-wal"), vec![0x8a; 180_093])?;
            let inventory = source.inventory()?;
            let legacy = encode_workspace_snapshot(&source, &inventory)?;
            let mut output = Vec::new();
            let report = encode_workspace_snapshot_streaming(&source, &inventory, &mut output)?;
            assert_eq!(
                encoded_stream_snapshot_size(&inventory)?,
                output.len() as u64
            );
            assert_eq!(legacy, output);
            assert_eq!(report.plaintext_bytes, output.len() as u64);
            let expected: [u8; 32] = sha2::Sha256::digest(&legacy).into();
            assert_eq!(report.plaintext_digest, expected);
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        fixture.cleanup();
        result
    }

    #[test]
    fn workspace_snapshot_and_bpgc_container_round_trip_using_only_file_spools()
    -> Result<(), Box<dyn std::error::Error>> {
        use encrypted_generation_domain::{
            GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
            open_generation_streaming, seal_generation_streaming,
        };
        use std::io::{Read, Seek, SeekFrom};
        let fixture = Fixture::new()?;
        let result = (|| {
            let base = GenerationId::parse("generation_streamed_snapshot_base")?;
            let next = GenerationId::parse("generation_streamed_snapshot_next")?;
            let source =
                fixture
                    .root
                    .create_generation(&fixture.tenant_id, &fixture.profile_id, &base)?;
            fs::write(source.path().join("prefs.js"), b"immutable Firefox prefs")?;
            let directory = source.path().join("user_data");
            fs::create_dir_all(&directory)?;
            fs::write(directory.join("cookies.sqlite-wal"), vec![0x4a; 3_700_016])?;
            let inventory = source.inventory()?;
            let snapshot_path = fixture.root.path().join("k2-snapshot-test.bin");
            let container_path = fixture.root.path().join("k2-container-test.bin");
            let reopened_path = fixture.root.path().join("k2-reopened-test.bin");
            let mut snapshot = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .read(true)
                .open(&snapshot_path)?;
            let report = encode_workspace_snapshot_streaming(&source, &inventory, &mut snapshot)?;
            snapshot.sync_all()?;
            snapshot.seek(SeekFrom::Start(0))?;
            let identity = GenerationIdentity::new(
                fixture.tenant_id.clone(),
                fixture.profile_id.clone(),
                next,
                Some(base),
            );
            let key = GenerationDek::new(KeyId::parse("streaming-snapshot-key")?, [0x21; 32]);
            let nonce = NoncePrefix::new([0x2a; 16]);
            let metadata = GenerationMetadata::for_reader(
                identity,
                key.key_id().clone(),
                nonce,
                65_536,
                &mut snapshot,
            )?;
            assert_eq!(report.plaintext_bytes, metadata.plaintext_bytes());
            assert_eq!(report.plaintext_digest, metadata.plaintext_digest().bytes(),);
            snapshot.seek(SeekFrom::Start(0))?;
            let mut container = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .read(true)
                .open(&container_path)?;
            let sealed = seal_generation_streaming(&metadata, &key, &mut snapshot, &mut container)?;
            container.sync_all()?;
            container.seek(SeekFrom::Start(0))?;
            let mut reopened = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .read(true)
                .open(&reopened_path)?;
            let opened = open_generation_streaming(&mut container, &key, &mut reopened)?;
            reopened.sync_all()?;
            assert_eq!(opened, sealed);
            assert_eq!(reopened.metadata()?.len(), snapshot.metadata()?.len());
            reopened.seek(SeekFrom::Start(0))?;
            snapshot.seek(SeekFrom::Start(0))?;
            let mut expected = [0_u8; 65_536];
            let mut actual = [0_u8; 65_536];
            loop {
                let count = snapshot.read(&mut expected)?;
                if count == 0 {
                    assert_eq!(reopened.read(&mut actual)?, 0);
                    break;
                }
                reopened.read_exact(&mut actual[..count])?;
                assert_eq!(&expected[..count], &actual[..count]);
            }
            assert_eq!(source.inventory()?, inventory);
            reopened.seek(SeekFrom::Start(0))?;
            let restored = materialize_workspace_snapshot_streaming_with_pre_publish(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                opened.metadata().generation_id(),
                &mut reopened,
                StreamedWorkspaceSnapshot {
                    plaintext_bytes: opened.metadata().plaintext_bytes(),
                    plaintext_digest: opened.metadata().plaintext_digest().bytes(),
                },
                |_| Ok(()),
            )?;
            assert_eq!(restored.inventory()?, inventory);
            assert_eq!(
                fs::read(restored.path().join("user_data/cookies.sqlite-wal"))?,
                vec![0x4a; 3_700_016],
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        fixture.cleanup();
        result
    }

    #[test]
    fn streamed_materializer_refuses_trailing_corrupt_and_unsafe_native_restore()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let result = (|| {
            let next = GenerationId::parse("generation_streamed_unpublished_unsafe")?;
            let valid = raw_snapshot(&[
                ("prefs.js", b"good"),
                ("user_data/cookies.sqlite-wal", b"wal"),
            ]);
            let proof = StreamedWorkspaceSnapshot {
                plaintext_bytes: valid.len() as u64,
                plaintext_digest: sha2::Sha256::digest(&valid).into(),
            };
            let materialized = materialize_workspace_snapshot_streaming_with_pre_publish(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &next,
                &mut valid.as_slice(),
                proof,
                |_| Ok(()),
            )?;
            assert_eq!(fs::read(materialized.path().join("prefs.js"))?, b"good");
            assert_eq!(
                fs::read(materialized.path().join("user_data/cookies.sqlite-wal"))?,
                b"wal",
            );
            assert_eq!(
                materialize_workspace_snapshot_streaming_with_pre_publish(
                    &fixture.root,
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &next,
                    &mut valid.as_slice(),
                    proof,
                    |_| Ok(()),
                )
                .err(),
                Some(GenerationSnapshotError::TargetAlreadyExists),
            );
            let next = GenerationId::parse("generation_streamed_restore_trailing")?;
            let mut invalid = valid.clone();
            invalid.push(0x91);
            assert!(
                materialize_workspace_snapshot_streaming_with_pre_publish(
                    &fixture.root,
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &next,
                    &mut invalid.as_slice(),
                    proof,
                    |_| Ok(()),
                )
                .is_err()
            );
            assert!(
                fixture
                    .root
                    .open_generation(&fixture.tenant_id, &fixture.profile_id, &next,)
                    .is_err()
            );
            let next = GenerationId::parse("generation_streamed_restore_escape")?;
            let unsafe_source = raw_snapshot(&[("../prefs.js", b"escape")]);
            let unsafe_proof = StreamedWorkspaceSnapshot {
                plaintext_bytes: unsafe_source.len() as u64,
                plaintext_digest: sha2::Sha256::digest(&unsafe_source).into(),
            };
            assert!(
                materialize_workspace_snapshot_streaming_with_pre_publish(
                    &fixture.root,
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &next,
                    &mut unsafe_source.as_slice(),
                    unsafe_proof,
                    |_| Ok(()),
                )
                .is_err()
            );
            assert!(
                fixture
                    .root
                    .open_generation(&fixture.tenant_id, &fixture.profile_id, &next,)
                    .is_err()
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        fixture.cleanup();
        result
    }
    #[test]
    fn stream_restore_pre_publish_failure_never_exposes_generation_and_is_retryable()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let result = (|| {
            let generation = GenerationId::parse("generation_unpublished_stream_crash")?;
            let snapshot = raw_snapshot(&[("prefs.js", b"retained-firefox")]);
            let expected = StreamedWorkspaceSnapshot {
                plaintext_bytes: snapshot.len() as u64,
                plaintext_digest: sha2::Sha256::digest(&snapshot).into(),
            };
            assert_eq!(
                materialize_workspace_snapshot_streaming_with_pre_publish(
                    &fixture.root,
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &generation,
                    &mut snapshot.as_slice(),
                    expected,
                    |_| Err(GenerationSnapshotError::PrePublish),
                )
                .err(),
                Some(GenerationSnapshotError::PrePublish),
            );
            assert!(
                fixture
                    .root
                    .open_generation(&fixture.tenant_id, &fixture.profile_id, &generation,)
                    .is_err()
            );
            let recovered = materialize_workspace_snapshot_streaming_with_pre_publish(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &generation,
                &mut snapshot.as_slice(),
                expected,
                |_| Ok(()),
            )?;
            assert_eq!(
                fs::read(recovered.path().join("prefs.js"))?,
                b"retained-firefox",
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        fixture.cleanup();
        result
    }

    #[test]
    fn streamed_snapshot_preserves_native_bytes_on_changed_file_and_failed_sink()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let result = (|| {
            let generation = GenerationId::parse("generation_streaming_bounded_failure")?;
            let source = fixture.root.create_generation(
                &fixture.tenant_id,
                &fixture.profile_id,
                &generation,
            )?;
            let native = source.path().join("prefs.js");
            fs::write(&native, b"first-native-close")?;
            let previous = source.inventory()?;
            fs::write(&native, b"changed-after-close")?;
            assert!(
                encode_workspace_snapshot_streaming(&source, &previous, &mut Vec::new(),).is_err()
            );
            let current = source.inventory()?;
            let mut tiny = [0_u8; 4];
            let mut no_space = std::io::Cursor::new(&mut tiny[..]);
            assert!(
                encode_workspace_snapshot_streaming(&source, &current, &mut no_space,).is_err()
            );
            assert_eq!(fs::read(&native)?, b"changed-after-close");
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        fixture.cleanup();
        result
    }

    #[test]
    fn canonical_snapshot_round_trips_into_exact_authoritative_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let source_id = GenerationId::parse("generation_snapshot_source")?;
        let target_id = GenerationId::parse("generation_snapshot_target")?;
        let source =
            fixture
                .root
                .create_generation(&fixture.tenant_id, &fixture.profile_id, &source_id)?;
        fs::create_dir_all(source.path().join("storage/default"))?;
        fs::create_dir_all(source.path().join("user_data/storage/default"))?;
        fs::write(source.path().join("prefs.js"), b"prefs")?;
        fs::write(source.path().join("storage/default/state.bin"), b"state")?;
        // Full N and LKG snapshots must carry both the pinned Camoufox identity
        // and the actual Firefox database/WAL state. A lock artifact is not data.
        fs::write(
            source.path().join("camoufox-config.json"),
            b"stable-fingerprint",
        )?;
        fs::write(
            source.path().join(".profile-browser-identity-v1"),
            b"stable-identity-manifest",
        )?;
        fs::write(
            source.path().join("user_data/cookies.sqlite"),
            b"cookies-db",
        )?;
        fs::write(
            source.path().join("user_data/cookies.sqlite-wal"),
            b"cookies-wal",
        )?;
        fs::write(
            source.path().join("user_data/storage/default/indexeddb"),
            b"indexed-db",
        )?;
        fs::write(
            source.path().join("user_data/parent.lock"),
            b"stale-os-lock",
        )?;
        let inventory = source.inventory()?;
        let snapshot = encode_workspace_snapshot(&source, &inventory)?;
        let target = materialize_workspace_snapshot(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &target_id,
            &snapshot,
        )?;
        assert_eq!(target.inventory()?, inventory);
        assert_eq!(fs::read(target.path().join("prefs.js"))?, b"prefs");
        assert_eq!(
            fs::read(target.path().join("storage/default/state.bin"))?,
            b"state"
        );
        for (relative, expected) in [
            ("camoufox-config.json", b"stable-fingerprint".as_slice()),
            (
                ".profile-browser-identity-v1",
                b"stable-identity-manifest".as_slice(),
            ),
            ("user_data/cookies.sqlite", b"cookies-db".as_slice()),
            ("user_data/cookies.sqlite-wal", b"cookies-wal".as_slice()),
            (
                "user_data/storage/default/indexeddb",
                b"indexed-db".as_slice(),
            ),
        ] {
            assert_eq!(fs::read(target.path().join(relative))?, expected);
        }
        assert!(!target.path().join("user_data/parent.lock").exists());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn pre_publish_failure_never_exposes_canonical_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let target_id = GenerationId::parse("generation_snapshot_pre_publish_failure")?;
        assert_eq!(
            materialize_workspace_snapshot_with_pre_publish(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &target_id,
                &raw_snapshot(&[("prefs.js", b"new")]),
                |_workspace| Err(GenerationSnapshotError::PrePublish),
            )
            .map(|_| ()),
            Err(GenerationSnapshotError::PrePublish)
        );
        assert!(
            fixture
                .root
                .open_generation(&fixture.tenant_id, &fixture.profile_id, &target_id)
                .is_err()
        );
        // A failed pre-publish callback did not expose a canonical generation or
        // poison a later exact retry: only this owned staging directory is discarded.
        let reopened = materialize_workspace_snapshot(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &target_id,
            &raw_snapshot(&[("prefs.js", b"new")]),
        )?;
        assert_eq!(fs::read(reopened.path().join("prefs.js"))?, b"new");
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn parser_rejects_truncation_trailing_bytes_unsorted_and_case_aliases() {
        let canonical = raw_snapshot(&[("a", b"1"), ("b", b"2")]);
        assert!(parse_workspace_snapshot(&canonical).is_ok());
        assert_eq!(
            parse_workspace_snapshot(&canonical[..canonical.len() - 1]).map(|_| ()),
            Err(GenerationSnapshotError::InvalidFormat)
        );
        let mut trailing = canonical.clone();
        trailing.push(0);
        assert_eq!(
            parse_workspace_snapshot(&trailing).map(|_| ()),
            Err(GenerationSnapshotError::InvalidFormat)
        );
        assert_eq!(
            parse_workspace_snapshot(&raw_snapshot(&[("b", b"1"), ("a", b"2")])).map(|_| ()),
            Err(GenerationSnapshotError::InvalidFormat)
        );
        assert_eq!(
            parse_workspace_snapshot(&raw_snapshot(&[("A", b"1"), ("a", b"2")])).map(|_| ()),
            Err(GenerationSnapshotError::UnsafePath)
        );
    }

    #[test]
    fn parser_rejects_windows_path_escape_and_device_aliases() {
        for path in [
            "../escape",
            "/absolute",
            "C:/absolute",
            "dir\\escape",
            "dir//double",
            "dir/./dot",
            "dir/../parent",
            "dir/trailing.",
            "dir/trailing ",
            "dir/file:ads",
            "dir/CON.txt",
            "dir/com1.log",
            "user_data/.parentlock",
            ".profile-generation",
            ".profile-platform.lock",
        ] {
            assert_eq!(
                parse_workspace_snapshot(&raw_snapshot(&[(path, b"x")])).map(|_| ()),
                Err(GenerationSnapshotError::UnsafePath),
                "path unexpectedly accepted: {path}"
            );
        }
    }

    #[test]
    fn materializer_never_overwrites_existing_authoritative_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let target_id = GenerationId::parse("generation_snapshot_existing")?;
        let existing =
            fixture
                .root
                .create_generation(&fixture.tenant_id, &fixture.profile_id, &target_id)?;
        fs::write(existing.path().join("prefs.js"), b"old")?;
        assert_eq!(
            materialize_workspace_snapshot(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &target_id,
                &raw_snapshot(&[("prefs.js", b"new")]),
            )
            .map(|_| ()),
            Err(GenerationSnapshotError::TargetAlreadyExists)
        );
        assert_eq!(fs::read(existing.path().join("prefs.js"))?, b"old");
        fixture.cleanup();
        Ok(())
    }
}
