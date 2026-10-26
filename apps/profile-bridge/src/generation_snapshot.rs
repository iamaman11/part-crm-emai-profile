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
/// Streaming-only BPGW0001 size; legacy Vec API remains capped at 64 MiB.
#[allow(dead_code)] // K2 production streaming adapter is not yet active.
pub(crate) const MAX_STREAM_WORKSPACE_SNAPSHOT_BYTES: u64 = 536_870_912;
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
    if expected_inventory.entries().len() > MAX_SNAPSHOT_FILES {
        return Err(GenerationSnapshotError::SnapshotTooLarge);
    }
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
    Ok(StreamedWorkspaceSnapshot {
        plaintext_bytes: length,
        plaintext_digest: digest.finalize().into(),
    })
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
        GenerationSnapshotError, WORKSPACE_SNAPSHOT_MAGIC, encode_workspace_snapshot,
        encode_workspace_snapshot_streaming, materialize_workspace_snapshot,
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
