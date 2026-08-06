use profile_platform_primitives::{
    DeviceId, GenerationId, ProfileId, TenantId, UnixMillis,
};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const ROOT_MARKER: &str = ".profile-platform-root";
const ROOT_MARKER_CONTENT: &str = "profile-platform-local-root-v1\n";
const GENERATION_MARKER: &str = ".profile-generation";
const GENERATION_MARKER_CONTENT: &str = "profile-platform-generation-v1\n";
const BRIDGE_LOCK_FILE: &str = ".profile-platform.lock";
const MAX_INVENTORY_FILES: usize = 100_000;
const MAX_RELATIVE_PATH_BYTES: usize = 512;
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalProfileError {
    RootMustBeAbsolute,
    RootIsSymlink,
    RootIsNotDirectory,
    RootUnmarkedAndNotEmpty,
    RootMarkerMismatch,
    UnsafeRelativePath,
    TargetAlreadyExists,
    GenerationMarkerMissing,
    GenerationMarkerMismatch,
    SymbolicLinkRejected,
    SpecialFileRejected,
    InventoryLimitExceeded,
    InventorySizeOverflow,
    LockBusy,
    LockOwnershipMismatch,
    InvalidPolicy,
    InvalidTransition,
    ClockRegression,
    TimeOverflow,
    SourceChanged,
    CloneChanged,
    Io(std::io::ErrorKind),
}

impl fmt::Display for LocalProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RootMustBeAbsolute => "local profile root must be absolute",
            Self::RootIsSymlink => "local profile root cannot be a symbolic link",
            Self::RootIsNotDirectory => "local profile root must be a directory",
            Self::RootUnmarkedAndNotEmpty => {
                "an existing non-empty local profile root must already be marked"
            }
            Self::RootMarkerMismatch => "local profile root marker does not match",
            Self::UnsafeRelativePath => "inventory contains an unsafe relative path",
            Self::TargetAlreadyExists => "local generation target already exists",
            Self::GenerationMarkerMissing => "local generation marker is missing",
            Self::GenerationMarkerMismatch => "local generation marker does not match",
            Self::SymbolicLinkRejected => "symbolic links are rejected in local generations",
            Self::SpecialFileRejected => "special filesystem entries are rejected",
            Self::InventoryLimitExceeded => "local generation inventory file limit exceeded",
            Self::InventorySizeOverflow => "local generation inventory size overflow",
            Self::LockBusy => "local generation already has a Bridge writer lock",
            Self::LockOwnershipMismatch => "Bridge lock ownership does not match",
            Self::InvalidPolicy => "local lifecycle policy is invalid",
            Self::InvalidTransition => "local generation state transition is invalid",
            Self::ClockRegression => "observed local lifecycle time moved backwards",
            Self::TimeOverflow => "local lifecycle time overflow",
            Self::SourceChanged => "source generation changed during clone creation",
            Self::CloneChanged => "recovery clone no longer matches its accepted inventory",
            Self::Io(_) => "local filesystem operation failed",
        })
    }
}

impl std::error::Error for LocalProfileError {}

impl From<std::io::Error> for LocalProfileError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

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
            if read_exact_text(&marker)? != ROOT_MARKER_CONTENT {
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
        if read_exact_text(&marker)? != GENERATION_MARKER_CONTENT {
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
        if read_exact_text(&self.lock_path)? != self.ownership {
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LocalGenerationState {
    MaterializedClean,
    InUse,
    DirtyLocal,
    RecoveryRequired,
    Quarantined,
    SyncedEvictable,
    Evicted,
}

impl LocalGenerationState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MaterializedClean => "materialized_clean",
            Self::InUse => "in_use",
            Self::DirtyLocal => "dirty_local",
            Self::RecoveryRequired => "recovery_required",
            Self::Quarantined => "quarantined",
            Self::SyncedEvictable => "synced_evictable",
            Self::Evicted => "evicted",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalGenerationRecord {
    generation_id: GenerationId,
    state: LocalGenerationState,
    bytes: u64,
    last_activity_at: UnixMillis,
    session_started_at: Option<UnixMillis>,
    locked: bool,
}

impl LocalGenerationRecord {
    #[must_use]
    pub const fn new(
        generation_id: GenerationId,
        bytes: u64,
        observed_at: UnixMillis,
    ) -> Self {
        Self {
            generation_id,
            state: LocalGenerationState::MaterializedClean,
            bytes,
            last_activity_at: observed_at,
            session_started_at: None,
            locked: false,
        }
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn state(&self) -> LocalGenerationState {
        self.state
    }

    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn last_activity_at(&self) -> UnixMillis {
        self.last_activity_at
    }

    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn set_locked(&mut self, locked: bool) -> Result<(), LocalProfileError> {
        if self.state == LocalGenerationState::Evicted && locked {
            return Err(LocalProfileError::InvalidTransition);
        }
        self.locked = locked;
        Ok(())
    }

    pub fn begin_use(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if !self.locked
            || !matches!(
                self.state,
                LocalGenerationState::MaterializedClean
                    | LocalGenerationState::SyncedEvictable
            )
        {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::InUse;
        self.last_activity_at = now;
        self.session_started_at = Some(now);
        Ok(())
    }

    pub fn observe_activity(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::InUse {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn graceful_close(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::InUse {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::DirtyLocal;
        self.last_activity_at = now;
        self.session_started_at = None;
        Ok(())
    }

    pub fn observe_crash(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::InUse {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::RecoveryRequired;
        self.last_activity_at = now;
        self.session_started_at = None;
        Ok(())
    }

    pub fn complete_recovery(
        &mut self,
        clone_integrity_passed: bool,
        now: UnixMillis,
    ) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::RecoveryRequired {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = if clone_integrity_passed {
            LocalGenerationState::MaterializedClean
        } else {
            LocalGenerationState::Quarantined
        };
        self.last_activity_at = now;
        self.locked = false;
        Ok(())
    }

    pub fn mark_synced(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::DirtyLocal {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::SyncedEvictable;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn evict(&mut self) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::SyncedEvictable || self.locked {
            return Err(LocalProfileError::InvalidTransition);
        }
        self.state = LocalGenerationState::Evicted;
        self.bytes = 0;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForgottenWindowAction {
    None,
    Warn,
    Drain,
    ForceClose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForgottenWindowPolicy {
    warn_after_ms: u64,
    drain_after_ms: u64,
    hard_ttl_ms: u64,
}

impl ForgottenWindowPolicy {
    pub const fn new(
        warn_after_ms: u64,
        drain_after_ms: u64,
        hard_ttl_ms: u64,
    ) -> Result<Self, LocalProfileError> {
        if warn_after_ms == 0
            || warn_after_ms >= drain_after_ms
            || drain_after_ms >= hard_ttl_ms
        {
            return Err(LocalProfileError::InvalidPolicy);
        }
        Ok(Self {
            warn_after_ms,
            drain_after_ms,
            hard_ttl_ms,
        })
    }

    pub fn evaluate(
        self,
        generation: &LocalGenerationRecord,
        now: UnixMillis,
    ) -> Result<ForgottenWindowAction, LocalProfileError> {
        if generation.state != LocalGenerationState::InUse {
            return Ok(ForgottenWindowAction::None);
        }
        let started_at = generation
            .session_started_at
            .ok_or(LocalProfileError::InvalidTransition)?;
        let age = elapsed(started_at, now)?;
        let idle = elapsed(generation.last_activity_at, now)?;
        if age >= self.hard_ttl_ms {
            Ok(ForgottenWindowAction::ForceClose)
        } else if idle >= self.drain_after_ms {
            Ok(ForgottenWindowAction::Drain)
        } else if idle >= self.warn_after_ms {
            Ok(ForgottenWindowAction::Warn)
        } else {
            Ok(ForgottenWindowAction::None)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuotaPlan {
    total_bytes: u64,
    bytes_to_reclaim: u64,
    reclaimable_bytes: u64,
    candidates: Vec<GenerationId>,
}

impl QuotaPlan {
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    #[must_use]
    pub const fn bytes_to_reclaim(&self) -> u64 {
        self.bytes_to_reclaim
    }

    #[must_use]
    pub const fn reclaimable_bytes(&self) -> u64 {
        self.reclaimable_bytes
    }

    #[must_use]
    pub fn candidates(&self) -> &[GenerationId] {
        &self.candidates
    }

    #[must_use]
    pub const fn is_satisfied(&self) -> bool {
        self.reclaimable_bytes >= self.bytes_to_reclaim
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotaPolicy {
    maximum_bytes: u64,
}

impl QuotaPolicy {
    pub const fn new(maximum_bytes: u64) -> Result<Self, LocalProfileError> {
        if maximum_bytes == 0 {
            return Err(LocalProfileError::InvalidPolicy);
        }
        Ok(Self { maximum_bytes })
    }

    pub fn plan(
        self,
        generations: &[LocalGenerationRecord],
    ) -> Result<QuotaPlan, LocalProfileError> {
        let mut total_bytes = 0_u64;
        for generation in generations {
            total_bytes = total_bytes
                .checked_add(generation.bytes)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
        }
        let bytes_to_reclaim = total_bytes.saturating_sub(self.maximum_bytes);
        let mut eligible = generations
            .iter()
            .filter(|generation| {
                generation.state == LocalGenerationState::SyncedEvictable
                    && !generation.locked
            })
            .collect::<Vec<_>>();
        eligible.sort_by(|left, right| {
            left.last_activity_at
                .cmp(&right.last_activity_at)
                .then_with(|| left.generation_id.cmp(&right.generation_id))
        });

        let mut reclaimable_bytes = 0_u64;
        let mut candidates = Vec::new();
        for generation in eligible {
            if reclaimable_bytes >= bytes_to_reclaim {
                break;
            }
            reclaimable_bytes = reclaimable_bytes
                .checked_add(generation.bytes)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
            candidates.push(generation.generation_id.clone());
        }
        Ok(QuotaPlan {
            total_bytes,
            bytes_to_reclaim,
            reclaimable_bytes,
            candidates,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportBundleSummary {
    total_generations: u64,
    total_bytes: u64,
    state_counts: BTreeMap<LocalGenerationState, u64>,
    inventory_failures: u64,
}

impl SupportBundleSummary {
    pub fn from_records(
        records: &[LocalGenerationRecord],
        inventory_failures: u64,
    ) -> Result<Self, LocalProfileError> {
        let mut total_bytes = 0_u64;
        let mut state_counts = BTreeMap::new();
        for record in records {
            total_bytes = total_bytes
                .checked_add(record.bytes)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
            let count = state_counts.entry(record.state).or_insert(0_u64);
            *count = count
                .checked_add(1)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
        }
        let total_generations = u64::try_from(records.len())
            .map_err(|_| LocalProfileError::InventorySizeOverflow)?;
        Ok(Self {
            total_generations,
            total_bytes,
            state_counts,
            inventory_failures,
        })
    }

    #[must_use]
    pub fn render_metadata_only(&self) -> String {
        let mut output = format!(
            "schema=local-profile-support-v1\ntotal_generations={}\ntotal_bytes={}\ninventory_failures={}\n",
            self.total_generations, self.total_bytes, self.inventory_failures
        );
        for (state, count) in &self.state_counts {
            output.push_str(&format!("state.{}={}\n", state.as_str(), count));
        }
        output
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
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn read_exact_text(path: &Path) -> Result<String, LocalProfileError> {
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
        digest = fnv_update(digest, entry.relative_path.as_bytes());
        digest = fnv_update(digest, &entry.bytes.to_le_bytes());
        digest = fnv_update(digest, &entry.content_digest.to_le_bytes());
    }
    Ok(GenerationInventory {
        entries,
        total_bytes,
        inventory_digest: digest,
    })
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
        if is_bridge_control_file(&relative_path) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(LocalProfileError::SymbolicLinkRejected);
        }
        if file_type.is_dir() {
            collect_inventory(root, &path, entries)?;
        } else if file_type.is_file() {
            if entries.len() >= MAX_INVENTORY_FILES {
                return Err(LocalProfileError::InventoryLimitExceeded);
            }
            entries.push(InventoryEntry {
                relative_path,
                bytes: metadata.len(),
                content_digest: hash_file(&path)?,
            });
        } else {
            return Err(LocalProfileError::SpecialFileRejected);
        }
    }
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

fn ensure_monotonic(previous: UnixMillis, now: UnixMillis) -> Result<(), LocalProfileError> {
    if now < previous {
        Err(LocalProfileError::ClockRegression)
    } else {
        Ok(())
    }
}

fn elapsed(previous: UnixMillis, now: UnixMillis) -> Result<u64, LocalProfileError> {
    now.value()
        .checked_sub(previous.value())
        .ok_or(LocalProfileError::ClockRegression)
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeWorkspaceLock, ForgottenWindowAction, ForgottenWindowPolicy,
        LocalGenerationRecord, LocalGenerationState, LocalProfileError, MaterializationRoot,
        QuotaPolicy, RecoveryClone, SupportBundleSummary,
    };
    use profile_platform_primitives::{
        DeviceId, GenerationId, ProfileId, TenantId, UnixMillis,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> Result<PathBuf, LocalProfileError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| LocalProfileError::ClockRegression)?
            .as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "part-crm-step8-{label}-{}-{nonce}",
            std::process::id()
        )))
    }

    fn ids() -> Result<(TenantId, ProfileId), Box<dyn std::error::Error>> {
        Ok((
            TenantId::parse("tenant_01JSTEP8")?,
            ProfileId::parse("profile_01JSTEP8")?,
        ))
    }

    #[test]
    fn materialization_root_builds_opaque_generation_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = test_root("paths")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let (tenant_id, profile_id) = ids()?;
        let generation_id = GenerationId::parse("generation_01JSTEP8A")?;
        let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
        assert!(workspace.path().starts_with(root.path()));
        assert!(workspace.path().ends_with(generation_id.as_str()));
        assert!(!workspace.path().to_string_lossy().contains('@'));
        fs::remove_dir_all(root_path)?;
        Ok(())
    }

    #[test]
    fn bridge_lock_is_exclusive_and_preserves_browser_lock_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = test_root("locks")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let (tenant_id, profile_id) = ids()?;
        let generation_id = GenerationId::parse("generation_01JSTEP8B")?;
        let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
        fs::write(workspace.path().join(".parentlock"), b"browser-owned")?;
        fs::write(workspace.path().join("lock"), b"browser-owned")?;
        let device_id = DeviceId::parse("device_01JSTEP8")?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device_id, 1)?;
        assert!(matches!(
            BridgeWorkspaceLock::acquire(&workspace, &device_id, 2),
            Err(LocalProfileError::LockBusy)
        ));
        lock.release()?;
        assert!(workspace.path().join(".parentlock").exists());
        assert!(workspace.path().join("lock").exists());
        let second = BridgeWorkspaceLock::acquire(&workspace, &device_id, 2)?;
        second.release()?;
        fs::remove_dir_all(root_path)?;
        Ok(())
    }

    #[test]
    fn inventory_is_deterministic_and_includes_browser_owned_locks()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = test_root("inventory")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let (tenant_id, profile_id) = ids()?;
        let generation_id = GenerationId::parse("generation_01JSTEP8C")?;
        let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
        fs::create_dir(workspace.path().join("storage"))?;
        fs::write(workspace.path().join("z.txt"), b"z")?;
        fs::write(workspace.path().join("storage/a.bin"), b"alpha")?;
        fs::write(workspace.path().join(".parentlock"), b"present")?;
        let first = workspace.inventory()?;
        let second = workspace.inventory()?;
        assert_eq!(first, second);
        assert_eq!(first.entries().len(), 3);
        assert!(
            first
                .entries()
                .iter()
                .any(|entry| entry.relative_path() == ".parentlock")
        );
        fs::remove_dir_all(root_path)?;
        Ok(())
    }

    #[test]
    fn recovery_is_clone_only_and_detects_clone_mutation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = test_root("recovery")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let (tenant_id, profile_id) = ids()?;
        let source_id = GenerationId::parse("generation_01JSTEP8D")?;
        let recovery_id = GenerationId::parse("generation_01JSTEP8E")?;
        let source = root.create_generation(&tenant_id, &profile_id, &source_id)?;
        fs::create_dir(source.path().join("storage"))?;
        fs::write(source.path().join("storage/state.bin"), b"accepted-state")?;
        let source_before = source.inventory()?;
        let recovery = RecoveryClone::create(
            &source,
            &root,
            &tenant_id,
            &profile_id,
            &recovery_id,
        )?;
        assert_eq!(recovery.verify_clone_only()?, source_before);
        fs::write(
            recovery.workspace().path().join("storage/state.bin"),
            b"mutated-clone",
        )?;
        assert_eq!(
            recovery.verify_clone_only(),
            Err(LocalProfileError::CloneChanged)
        );
        assert_eq!(source.inventory()?, source_before);
        fs::remove_dir_all(root_path)?;
        Ok(())
    }

    #[test]
    fn forgotten_window_policy_progresses_warn_drain_force_close()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = ForgottenWindowPolicy::new(100, 200, 500)?;
        let mut generation = LocalGenerationRecord::new(
            GenerationId::parse("generation_01JSTEP8F")?,
            10,
            UnixMillis::new(1_000),
        );
        generation.set_locked(true)?;
        generation.begin_use(UnixMillis::new(1_000))?;
        assert_eq!(
            policy.evaluate(&generation, UnixMillis::new(1_099))?,
            ForgottenWindowAction::None
        );
        assert_eq!(
            policy.evaluate(&generation, UnixMillis::new(1_100))?,
            ForgottenWindowAction::Warn
        );
        assert_eq!(
            policy.evaluate(&generation, UnixMillis::new(1_200))?,
            ForgottenWindowAction::Drain
        );
        assert_eq!(
            policy.evaluate(&generation, UnixMillis::new(1_500))?,
            ForgottenWindowAction::ForceClose
        );
        Ok(())
    }

    #[test]
    fn quota_never_selects_dirty_recovery_in_use_or_locked_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut dirty = LocalGenerationRecord::new(
            GenerationId::parse("generation_01JSTEP8G")?,
            80,
            UnixMillis::new(10),
        );
        dirty.set_locked(true)?;
        dirty.begin_use(UnixMillis::new(11))?;
        dirty.graceful_close(UnixMillis::new(12))?;
        dirty.set_locked(false)?;

        let mut recovery = LocalGenerationRecord::new(
            GenerationId::parse("generation_01JSTEP8H")?,
            70,
            UnixMillis::new(20),
        );
        recovery.set_locked(true)?;
        recovery.begin_use(UnixMillis::new(21))?;
        recovery.observe_crash(UnixMillis::new(22))?;

        let mut eligible = LocalGenerationRecord::new(
            GenerationId::parse("generation_01JSTEP8I")?,
            60,
            UnixMillis::new(1),
        );
        eligible.set_locked(true)?;
        eligible.begin_use(UnixMillis::new(2))?;
        eligible.graceful_close(UnixMillis::new(3))?;
        eligible.set_locked(false)?;
        eligible.mark_synced(UnixMillis::new(4))?;

        let mut locked_synced = LocalGenerationRecord::new(
            GenerationId::parse("generation_01JSTEP8J")?,
            50,
            UnixMillis::new(5),
        );
        locked_synced.set_locked(true)?;
        locked_synced.begin_use(UnixMillis::new(6))?;
        locked_synced.graceful_close(UnixMillis::new(7))?;
        locked_synced.mark_synced(UnixMillis::new(8))?;

        let records = [dirty, recovery, eligible.clone(), locked_synced];
        let plan = QuotaPolicy::new(200)?.plan(&records)?;
        assert_eq!(plan.total_bytes(), 260);
        assert_eq!(plan.bytes_to_reclaim(), 60);
        assert_eq!(plan.reclaimable_bytes(), 60);
        assert!(plan.is_satisfied());
        assert_eq!(plan.candidates(), [eligible.generation_id().clone()]);
        assert_eq!(records[0].state(), LocalGenerationState::DirtyLocal);
        assert_eq!(records[1].state(), LocalGenerationState::RecoveryRequired);
        Ok(())
    }

    #[test]
    fn support_summary_contains_metadata_only()
    -> Result<(), Box<dyn std::error::Error>> {
        let record = LocalGenerationRecord::new(
            GenerationId::parse("generation_01JSTEP8K")?,
            42,
            UnixMillis::new(1),
        );
        let rendered =
            SupportBundleSummary::from_records(&[record], 2)?.render_metadata_only();
        assert!(rendered.contains("total_generations=1"));
        assert!(rendered.contains("total_bytes=42"));
        assert!(rendered.contains("inventory_failures=2"));
        assert!(!rendered.contains("generation_01JSTEP8K"));
        assert!(!rendered.contains("user@example.com"));
        assert!(!rendered.contains("secret"));
        assert!(!rendered.contains('\\'));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn inventory_rejects_symbolic_links() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let root_path = test_root("symlink")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let (tenant_id, profile_id) = ids()?;
        let generation_id = GenerationId::parse("generation_01JSTEP8L")?;
        let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
        fs::write(workspace.path().join("target.txt"), b"target")?;
        symlink("target.txt", workspace.path().join("link.txt"))?;
        assert_eq!(
            workspace.inventory(),
            Err(LocalProfileError::SymbolicLinkRejected)
        );
        fs::remove_dir_all(root_path)?;
        Ok(())
    }
}
