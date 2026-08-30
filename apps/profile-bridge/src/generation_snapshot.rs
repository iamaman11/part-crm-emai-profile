use crate::local_profile::{
    GenerationInventory, GenerationWorkspace, LocalProfileError, MaterializationRoot,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const WORKSPACE_SNAPSHOT_MAGIC: &[u8; 8] = b"BPGW0001";
pub(crate) const MAX_WORKSPACE_SNAPSHOT_BYTES: usize = 67_108_864;
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

pub(crate) fn materialize_workspace_snapshot(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    snapshot: &[u8],
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
    );
    if result.is_err() {
        remove_owned_staging(&staging_path);
    }
    result
}

fn materialize_into_staging(
    root: &MaterializationRoot,
    staging: GenerationWorkspace,
    staging_path: &Path,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    entries: &[SnapshotEntry<'_>],
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

    let final_path = authoritative_generation_path(root, tenant_id, profile_id, generation_id);
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

fn authoritative_generation_path(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> PathBuf {
    root.path()
        .join(tenant_id.as_str())
        .join(profile_id.as_str())
        .join(generation_id.as_str())
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
        materialize_workspace_snapshot, parse_workspace_snapshot,
    };
    use crate::local_profile::MaterializationRoot;
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
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
        fs::write(source.path().join("prefs.js"), b"prefs")?;
        fs::write(source.path().join("storage/default/state.bin"), b"state")?;
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
