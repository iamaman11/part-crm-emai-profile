use super::LocalProfileError;
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const ROOT_MARKER: &str = ".profile-platform-root";
const ROOT_MARKER_CONTENT: &str = "profile-platform-local-root-v1\n";
const GENERATION_MARKER: &str = ".profile-generation";
const GENERATION_MARKER_CONTENT: &str = "profile-platform-generation-v1\n";
const BRIDGE_LOCK_FILE: &str = ".profile-platform.lock";
const BROWSER_STATE_DIRECTORY: &str = "user_data";
const FIREFOX_PARENT_LOCK_PATH: &str = "user_data/.parentlock";
const FIREFOX_WINDOWS_PARENT_LOCK_PATH: &str = "user_data/parent.lock";
const FIREFOX_LOCK_PATH: &str = "user_data/lock";
const MAX_INVENTORY_FILES: usize = 100_000;
const MAX_RELATIVE_PATH_BYTES: usize = 512;
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationRoot {
    canonical_path: PathBuf,
}

impl MaterializationRoot {
    pub fn open_or_create(path: impl Into<PathBuf>) -> Result<Self, LocalProfileError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(LocalProfileError::RootMustBeAbsolute);
        }

        if path.exists() {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(LocalProfileError::RootIsSymlink);
            }
            if !metadata.is_dir() {
                return Err(LocalProfileError::RootIsNotDirectory);
            }
        } else {
            fs::create_dir_all(&path)?;
        }

        let marker = path.join(ROOT_MARKER);
        if marker.exists() {
            if read_control_text(&marker)? != ROOT_MARKER_CONTENT {
                return Err(LocalProfileError::RootMarkerMismatch);
            }
        } else {
            if fs::read_dir(&path)?.next().transpose()?.is_some() {
                return Err(LocalProfileError::RootUnmarkedAndNotEmpty);
            }
            write_new_text(&marker, ROOT_MARKER_CONTENT)?;
        }

        let canonical_path = fs::canonicalize(&path)?;
        Ok(Self { canonical_path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn create_generation(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<GenerationWorkspace, LocalProfileError> {
        let tenant_path = ensure_directory(&self.canonical_path, tenant_id.as_str())?;
        let profile_path = ensure_directory(&tenant_path, profile_id.as_str())?;
        let generation_path = profile_path.join(generation_id.as_str());
        if generation_path.exists() {
            return Err(LocalProfileError::TargetAlreadyExists);
        }
        fs::create_dir(&generation_path)?;
        write_new_text(
            &generation_path.join(GENERATION_MARKER),
            GENERATION_MARKER_CONTENT,
        )?;
        GenerationWorkspace::open(generation_path)
    }

    pub fn open_generation(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<GenerationWorkspace, LocalProfileError> {
        let tenant_path = open_directory(&self.canonical_path, tenant_id.as_str())?;
        let profile_path = open_directory(&tenant_path, profile_id.as_str())?;
        GenerationWorkspace::open(profile_path.join(generation_id.as_str()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationWorkspace {
    canonical_path: PathBuf,
}

impl GenerationWorkspace {
    fn open(path: PathBuf) -> Result<Self, LocalProfileError> {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(LocalProfileError::SymbolicLinkRejected);
        }
        if !metadata.is_dir() {
            return Err(LocalProfileError::RootIsNotDirectory);
        }
        let marker = path.join(GENERATION_MARKER);
        if !marker.exists() {
            return Err(LocalProfileError::GenerationMarkerMissing);
        }
        if read_control_text(&marker)? != GENERATION_MARKER_CONTENT {
            return Err(LocalProfileError::GenerationMarkerMismatch);
        }
        Ok(Self {
            canonical_path: fs::canonicalize(path)?,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn inventory(&self) -> Result<GenerationInventory, LocalProfileError> {
        build_inventory(&self.canonical_path)
    }

    /// Digest only immutable generation materialization inputs.
    ///
    /// `user_data/**` is mutable browser state (cookies, localStorage, caches and Firefox
    /// runtime lock artifacts) and must not invalidate a generation's fingerprint/runtime
    /// identity after a clean browser session. The state directory itself must still be a
    /// real directory rather than a symlink.
    pub fn materialization_inventory_digest(&self) -> Result<u64, LocalProfileError> {
        build_materialization_inventory_digest(&self.canonical_path)
    }
}

#[derive(Debug)]
pub struct BridgeWorkspaceLock {
    lock_path: PathBuf,
    ownership: String,
}

impl BridgeWorkspaceLock {
    pub fn acquire(
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        epoch: u64,
    ) -> Result<Self, LocalProfileError> {
        if epoch == 0 {
            return Err(LocalProfileError::InvalidPolicy);
        }
        let lock_path = workspace.path().join(BRIDGE_LOCK_FILE);
        let ownership = format!(
            "profile-platform-bridge-lock-v1\n{}\n{epoch}\n",
            device_id.as_str()
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    LocalProfileError::LockBusy
                } else {
                    LocalProfileError::from(error)
                }
            })?;
        file.write_all(ownership.as_bytes())?;
        file.sync_all()?;
        Ok(Self {
            lock_path,
            ownership,
        })
    }

    pub fn release(self) -> Result<(), LocalProfileError> {
        if read_control_text(&self.lock_path)? != self.ownership {
            return Err(LocalProfileError::LockOwnershipMismatch);
        }
        fs::remove_file(self.lock_path)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryEntry {
    relative_path: String,
    bytes: u64,
    content_digest: u64,
}

impl InventoryEntry {
    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn content_digest(&self) -> u64 {
        self.content_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationInventory {
    entries: Vec<InventoryEntry>,
    total_bytes: u64,
    inventory_digest: u64,
}

impl GenerationInventory {
    #[must_use]
    pub fn entries(&self) -> &[InventoryEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    #[must_use]
    pub const fn inventory_digest(&self) -> u64 {
        self.inventory_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryClone {
    source: GenerationWorkspace,
    clone: GenerationWorkspace,
    source_inventory: GenerationInventory,
    clone_inventory: GenerationInventory,
}

impl RecoveryClone {
    pub fn create(
        source: &GenerationWorkspace,
        root: &MaterializationRoot,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        recovery_generation_id: &GenerationId,
    ) -> Result<Self, LocalProfileError> {
        let source_inventory = source.inventory()?;
        let clone = root.create_generation(tenant_id, profile_id, recovery_generation_id)?;
        copy_inventory(source.path(), clone.path(), &source_inventory)?;
        let clone_inventory = clone.inventory()?;
        if clone_inventory != source_inventory {
            return Err(LocalProfileError::CloneChanged);
        }
        if source.inventory()? != source_inventory {
            return Err(LocalProfileError::SourceChanged);
        }
        Ok(Self {
            source: source.clone(),
            clone,
            source_inventory,
            clone_inventory,
        })
    }

    #[must_use]
    pub const fn workspace(&self) -> &GenerationWorkspace {
        &self.clone
    }

    pub fn verify_clone_only(&self) -> Result<GenerationInventory, LocalProfileError> {
        if self.source.inventory()? != self.source_inventory {
            return Err(LocalProfileError::SourceChanged);
        }
        let current_clone = self.clone.inventory()?;
        if current_clone != self.clone_inventory {
            return Err(LocalProfileError::CloneChanged);
        }
        Ok(current_clone)
    }
}

fn ensure_directory(parent: &Path, safe_segment: &str) -> Result<PathBuf, LocalProfileError> {
    if safe_segment.is_empty()
        || safe_segment == "."
        || safe_segment == ".."
        || safe_segment.contains(['/', '\\'])
    {
        return Err(LocalProfileError::UnsafeRelativePath);
    }
    let path = parent.join(safe_segment);
    if path.exists() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(LocalProfileError::SymbolicLinkRejected);
        }
        if !metadata.is_dir() {
            return Err(LocalProfileError::RootIsNotDirectory);
        }
    } else {
        fs::create_dir(&path)?;
    }
    Ok(path)
}

fn open_directory(parent: &Path, safe_segment: &str) -> Result<PathBuf, LocalProfileError> {
    if safe_segment.is_empty()
        || safe_segment == "."
        || safe_segment == ".."
        || safe_segment.contains(['/', '\\'])
    {
        return Err(LocalProfileError::UnsafeRelativePath);
    }
    let path = parent.join(safe_segment);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() {
        return Err(LocalProfileError::SymbolicLinkRejected);
    }
    if !metadata.is_dir() {
        return Err(LocalProfileError::RootIsNotDirectory);
    }
    Ok(path)
}

fn write_new_text(path: &Path, content: &str) -> Result<(), LocalProfileError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn read_control_text(path: &Path) -> Result<String, LocalProfileError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(LocalProfileError::SymbolicLinkRejected);
    }
    if !metadata.is_file() {
        return Err(LocalProfileError::SpecialFileRejected);
    }
    let mut content = String::new();
    File::open(path)?.read_to_string(&mut content)?;
    Ok(content)
}

fn build_inventory(root: &Path) -> Result<GenerationInventory, LocalProfileError> {
    let mut entries = Vec::new();
    collect_inventory(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if entries.len() > MAX_INVENTORY_FILES {
        return Err(LocalProfileError::InventoryLimitExceeded);
    }
    let mut total_bytes = 0_u64;
    let mut digest = FNV_OFFSET_BASIS;
    for entry in &entries {
        total_bytes = total_bytes
            .checked_add(entry.bytes)
            .ok_or(LocalProfileError::InventorySizeOverflow)?;
        digest = inventory_entry_digest(digest, entry);
    }
    Ok(GenerationInventory {
        entries,
        total_bytes,
        inventory_digest: digest,
    })
}

fn build_materialization_inventory_digest(root: &Path) -> Result<u64, LocalProfileError> {
    let mut entries = Vec::new();
    collect_materialization_inventory(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if entries.len() > MAX_INVENTORY_FILES {
        return Err(LocalProfileError::InventoryLimitExceeded);
    }
    Ok(entries
        .iter()
        .fold(FNV_OFFSET_BASIS, inventory_entry_digest))
}

fn inventory_entry_digest(digest: u64, entry: &InventoryEntry) -> u64 {
    let digest = fnv_update(digest, entry.relative_path.as_bytes());
    let digest = fnv_update(digest, &entry.bytes.to_le_bytes());
    fnv_update(digest, &entry.content_digest.to_le_bytes())
}

fn collect_inventory(
    root: &Path,
    current: &Path,
    entries: &mut Vec<InventoryEntry>,
) -> Result<(), LocalProfileError> {
    let mut children = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| LocalProfileError::UnsafeRelativePath)?;
        let relative_path = normalized_relative_path(relative)?;
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        if is_ephemeral_browser_lock(&relative_path) {
            if file_type.is_symlink() || file_type.is_file() {
                continue;
            }
            return Err(LocalProfileError::SpecialFileRejected);
        }
        if file_type.is_symlink() {
            return Err(LocalProfileError::SymbolicLinkRejected);
        }
        if is_bridge_control_file(&relative_path) {
            if !file_type.is_file() {
                return Err(LocalProfileError::SpecialFileRejected);
            }
            continue;
        }
        if file_type.is_dir() {
            collect_inventory(root, &path, entries)?;
        } else if file_type.is_file() {
            push_inventory_entry(entries, relative_path, &path, metadata.len())?;
        } else {
            return Err(LocalProfileError::SpecialFileRejected);
        }
    }
    Ok(())
}

fn collect_materialization_inventory(
    root: &Path,
    current: &Path,
    entries: &mut Vec<InventoryEntry>,
) -> Result<(), LocalProfileError> {
    let mut children = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| LocalProfileError::UnsafeRelativePath)?;
        let relative_path = normalized_relative_path(relative)?;
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(LocalProfileError::SymbolicLinkRejected);
        }
        if is_bridge_control_file(&relative_path) {
            if !file_type.is_file() {
                return Err(LocalProfileError::SpecialFileRejected);
            }
            continue;
        }
        if relative_path == BROWSER_STATE_DIRECTORY {
            if !file_type.is_dir() {
                return Err(LocalProfileError::SpecialFileRejected);
            }
            continue;
        }
        if file_type.is_dir() {
            collect_materialization_inventory(root, &path, entries)?;
        } else if file_type.is_file() {
            push_inventory_entry(entries, relative_path, &path, metadata.len())?;
        } else {
            return Err(LocalProfileError::SpecialFileRejected);
        }
    }
    Ok(())
}

fn push_inventory_entry(
    entries: &mut Vec<InventoryEntry>,
    relative_path: String,
    path: &Path,
    bytes: u64,
) -> Result<(), LocalProfileError> {
    if entries.len() >= MAX_INVENTORY_FILES {
        return Err(LocalProfileError::InventoryLimitExceeded);
    }
    entries.push(InventoryEntry {
        relative_path,
        bytes,
        content_digest: hash_file(path)?,
    });
    Ok(())
}

fn normalized_relative_path(relative: &Path) -> Result<String, LocalProfileError> {
    let mut parts = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(value) = component else {
            return Err(LocalProfileError::UnsafeRelativePath);
        };
        let part = value
            .to_str()
            .ok_or(LocalProfileError::UnsafeRelativePath)?;
        if part.is_empty() || part == "." || part == ".." {
            return Err(LocalProfileError::UnsafeRelativePath);
        }
        parts.push(part);
    }
    let path = parts.join("/");
    if path.is_empty() || path.len() > MAX_RELATIVE_PATH_BYTES {
        return Err(LocalProfileError::UnsafeRelativePath);
    }
    Ok(path)
}

fn is_bridge_control_file(relative_path: &str) -> bool {
    matches!(relative_path, GENERATION_MARKER | BRIDGE_LOCK_FILE)
}

fn is_ephemeral_browser_lock(relative_path: &str) -> bool {
    matches!(
        relative_path,
        FIREFOX_PARENT_LOCK_PATH | FIREFOX_WINDOWS_PARENT_LOCK_PATH | FIREFOX_LOCK_PATH
    )
}

fn hash_file(path: &Path) -> Result<u64, LocalProfileError> {
    let mut file = File::open(path)?;
    let mut buffer = [0_u8; 8192];
    let mut digest = FNV_OFFSET_BASIS;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(digest);
        }
        digest = fnv_update(digest, &buffer[..read]);
    }
}

fn fnv_update(mut digest: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        digest ^= u64::from(*byte);
        digest = digest.wrapping_mul(FNV_PRIME);
    }
    digest
}

fn copy_inventory(
    source: &Path,
    destination: &Path,
    inventory: &GenerationInventory,
) -> Result<(), LocalProfileError> {
    for entry in inventory.entries() {
        let source_path = source.join(Path::new(entry.relative_path()));
        let destination_path = destination.join(Path::new(entry.relative_path()));
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let copied = fs::copy(source_path, destination_path)?;
        if copied != entry.bytes() {
            return Err(LocalProfileError::CloneChanged);
        }
    }
    Ok(())
}
