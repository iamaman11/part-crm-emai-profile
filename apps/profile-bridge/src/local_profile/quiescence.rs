use super::filesystem;
use super::{GenerationWorkspace, LocalProfileError, MaterializationRoot};
use profile_platform_primitives::DeviceId;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

const ROOT_MARKER: &str = ".profile-platform-root";
const BRIDGE_LOCK_FILE: &str = ".profile-platform.lock";
const GENERATION_DEPTH_FROM_ROOT: usize = 3;

/// Existing per-generation Bridge writer ownership plus one root-scoped shared OS admission lock.
///
/// The shared lock does not replace the canonical per-generation control file. It only makes writer
/// admission mutually exclusive with Windows delivery activation without introducing durable state.
#[derive(Debug)]
pub struct BridgeWorkspaceLock {
    inner: filesystem::BridgeWorkspaceLock,
    _admission: File,
}

impl BridgeWorkspaceLock {
    pub fn acquire(
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        epoch: u64,
    ) -> Result<Self, LocalProfileError> {
        let root = workspace_root(workspace)?;
        let admission = open_root_marker(root)?;
        admission.try_lock_shared().map_err(map_try_lock_error)?;
        let inner = filesystem::BridgeWorkspaceLock::acquire(workspace, device_id, epoch)?;
        Ok(Self {
            inner,
            _admission: admission,
        })
    }

    pub fn release(self) -> Result<(), LocalProfileError> {
        let Self {
            inner,
            _admission: admission,
        } = self;
        let result = inner.release();
        drop(admission);
        result
    }
}

/// Race-free witness that no current Profile Bridge writer owns any generation under one canonical
/// materialization root and that no new writer can be admitted while activation is decided.
///
/// `File::try_lock` is OS-backed (`LockFileEx` on Windows) and the exclusive lock is released when
/// this handle is closed, including process termination. Existing per-generation lockfiles are also
/// checked after exclusivity is acquired so unresolved predecessor/cleanup evidence fails closed.
#[derive(Debug)]
pub struct DeliveryActivationGuard {
    _exclusive: File,
}

impl DeliveryActivationGuard {
    pub fn acquire(root: &MaterializationRoot) -> Result<Self, LocalProfileError> {
        let exclusive = open_root_marker(root.path())?;
        exclusive.try_lock().map_err(map_try_lock_error)?;
        if bridge_writer_lock_present(root.path(), 0)? {
            return Err(LocalProfileError::LockBusy);
        }
        Ok(Self {
            _exclusive: exclusive,
        })
    }

    #[must_use]
    pub const fn confirms_quiescence(&self) -> bool {
        true
    }
}

fn workspace_root(workspace: &GenerationWorkspace) -> Result<&Path, LocalProfileError> {
    workspace
        .path()
        .ancestors()
        .nth(GENERATION_DEPTH_FROM_ROOT)
        .ok_or(LocalProfileError::UnsafeRelativePath)
}

fn open_root_marker(root: &Path) -> Result<File, LocalProfileError> {
    let marker = root.join(ROOT_MARKER);
    let metadata = fs::symlink_metadata(&marker)?;
    if metadata.file_type().is_symlink() {
        return Err(LocalProfileError::SymbolicLinkRejected);
    }
    if !metadata.is_file() {
        return Err(LocalProfileError::SpecialFileRejected);
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(marker)
        .map_err(LocalProfileError::from)
}

fn bridge_writer_lock_present(root: &Path, depth: usize) -> Result<bool, LocalProfileError> {
    if depth == GENERATION_DEPTH_FROM_ROOT {
        return match fs::symlink_metadata(root.join(BRIDGE_LOCK_FILE)) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        };
    }

    let mut children = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(LocalProfileError::SymbolicLinkRejected);
        }
        if file_type.is_dir() {
            if bridge_writer_lock_present(&path, depth + 1)? {
                return Ok(true);
            }
        } else if !file_type.is_file() {
            return Err(LocalProfileError::SpecialFileRejected);
        }
    }
    Ok(false)
}

fn map_try_lock_error(error: TryLockError) -> LocalProfileError {
    match error {
        TryLockError::WouldBlock => LocalProfileError::LockBusy,
        TryLockError::Error(error) => LocalProfileError::from(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{BRIDGE_LOCK_FILE, BridgeWorkspaceLock, DeliveryActivationGuard};
    use crate::local_profile::{LocalProfileError, MaterializationRoot};
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn create(label: &str) -> Result<Self, std::io::Error> {
            let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-quiescence-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            fs::remove_dir(&path)?;
            Ok(Self(path))
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    type Fixture = (
        TestRoot,
        MaterializationRoot,
        crate::local_profile::GenerationWorkspace,
        DeviceId,
    );

    fn fixture(label: &str) -> Result<Fixture, Box<dyn std::error::Error>> {
        let test_root = TestRoot::create(label)?;
        let root = MaterializationRoot::open_or_create(&test_root.0)?;
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let tenant = TenantId::parse(format!("tenant_quiescence_{sequence}"))?;
        let profile = ProfileId::parse(format!("profile_quiescence_{sequence}"))?;
        let generation = GenerationId::parse(format!("generation_quiescence_{sequence}"))?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let device = DeviceId::parse(format!("device_quiescence_{sequence}"))?;
        Ok((test_root, root, workspace, device))
    }

    #[test]
    fn active_writer_blocks_activation_until_writer_releases()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, root, workspace, device) = fixture("writer-blocks")?;
        let writer = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;

        assert!(matches!(
            DeliveryActivationGuard::acquire(&root),
            Err(LocalProfileError::LockBusy)
        ));

        writer.release()?;
        let guard = DeliveryActivationGuard::acquire(&root)?;
        assert!(guard.confirms_quiescence());
        Ok(())
    }

    #[test]
    fn activation_guard_blocks_new_writer_without_toctou_window()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, root, workspace, device) = fixture("activation-blocks")?;
        let guard = DeliveryActivationGuard::acquire(&root)?;

        assert!(matches!(
            BridgeWorkspaceLock::acquire(&workspace, &device, 1),
            Err(LocalProfileError::LockBusy)
        ));

        drop(guard);
        BridgeWorkspaceLock::acquire(&workspace, &device, 1)?.release()?;
        Ok(())
    }

    #[test]
    fn unresolved_per_generation_writer_evidence_blocks_activation()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, root, workspace, _device) = fixture("stale-writer")?;
        fs::write(workspace.path().join(BRIDGE_LOCK_FILE), b"unresolved-writer\n")?;

        assert!(matches!(
            DeliveryActivationGuard::acquire(&root),
            Err(LocalProfileError::LockBusy)
        ));
        Ok(())
    }
}
