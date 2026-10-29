use super::filesystem;
use super::{GenerationWorkspace, LocalProfileError, MaterializationRoot};
use profile_platform_primitives::DeviceId;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;

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

    /// Same-device recovery after an interrupted Bridge. The caller must have already
    /// redeemed fresh device/ACL and exact coordinator generation/lease authority.
    /// Root-wide exclusive OS admission excludes *all* live cooperative Bridge writers
    /// and Windows delivery activation until the new per-generation marker is created.
    pub(crate) fn recover_interrupted_launch(
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        fresh_epoch: u64,
    ) -> Result<Self, LocalProfileError> {
        let root = workspace_root(workspace)?;
        let exclusive = open_root_marker(root)?;
        exclusive.try_lock().map_err(map_try_lock_error)?;

        // A stale Windows parent.lock file is permitted only when its actual OS handle
        // is free. On other platforms, uncertain Firefox markers remain fail-closed.
        // This checks the original working directory, never a replacement clone.
        if !matches!(
            crate::browser_execution::browser_writer_locks_absent(workspace),
            Ok(true)
        ) {
            return Err(LocalProfileError::LockBusy);
        }

        let inner = filesystem::BridgeWorkspaceLock::replace_interrupted(
            workspace,
            device_id,
            fresh_epoch,
        )?;
        drop(exclusive);

        // The newly established marker blocks delivery activation while we switch
        // from exclusive crash reconciliation to ordinary shared writer admission.
        let admission = open_root_marker(root)?;
        if let Err(error) = admission.try_lock_shared() {
            // A concurrent updater won admission first. Release our new marker;
            // never start the browser while that updater owns the root.
            inner.release()?;
            return Err(map_try_lock_error(error));
        }
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
/// this handle is closed, including process termination. On Windows, a retained Bridge
/// marker is *not* a live writer: native Firefox handles are checked separately
/// without deleting or recovering any generation. Unknown native locks fail closed.
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
        #[cfg(windows)]
        {
            // This marker is durable *recovery evidence*, not a kernel lock. A
            // crashed Bridge may leave it alongside the only unsynced N+1.
            // Root exclusivity already excludes live cooperative Bridge writers.
            // Keep the marker, its epoch, and every pending browser byte intact.
            match fs::symlink_metadata(root.join(BRIDGE_LOCK_FILE)) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(LocalProfileError::SymbolicLinkRejected);
                }
                Ok(metadata) if !metadata.is_file() => {
                    return Err(LocalProfileError::SpecialFileRejected);
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }

            let user_data = root.join("user_data");
            match fs::symlink_metadata(&user_data) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(LocalProfileError::SymbolicLinkRejected);
                }
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(LocalProfileError::SpecialFileRejected);
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }

            // An orphaned live Firefox can outlast Bridge. Reuse the native
            // handle-based probe; unknown/unreadable locks block activation.
            return Ok(!matches!(
                crate::browser_execution::browser_writer_locks_absent_at(root),
                Ok(true)
            ));
        }

        #[cfg(not(windows))]
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

    #[cfg(not(windows))]
    #[test]
    fn unresolved_per_generation_writer_evidence_blocks_activation()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, root, workspace, _device) = fixture("stale-writer")?;
        fs::write(
            workspace.path().join(BRIDGE_LOCK_FILE),
            b"unresolved-writer\n",
        )?;

        assert!(matches!(
            DeliveryActivationGuard::acquire(&root),
            Err(LocalProfileError::LockBusy)
        ));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn retained_marker_and_pending_save_allow_upgrade_without_modifying_local_data()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, root, workspace, device) = fixture("pending-save-install")?;
        let marker = workspace.path().join(BRIDGE_LOCK_FILE);
        let content = format!("profile-platform-bridge-lock-v1\n{}\n5\n", device.as_str());
        fs::write(&marker, content.as_bytes())?;
        let browser_data = workspace.path().join("user_data");
        fs::create_dir(&browser_data)?;
        let parent_lock = browser_data.join("parent.lock");
        let wal = browser_data.join("cookies.sqlite-wal");
        let pending = workspace.path().join("unsynced-successor.bin");
        fs::write(&parent_lock, b"stale-native-parent-lock")?;
        fs::write(&wal, b"original-uncheckpointed-wal")?;
        fs::write(&pending, b"encrypted-original-n-plus-one")?;

        // Neither a valid old Bridge marker nor an inert Firefox parent.lock
        // is a live writer; admission must not repair/delete/rename the data.
        let guard = DeliveryActivationGuard::acquire(&root)?;
        assert!(guard.confirms_quiescence());
        assert!(matches!(
            BridgeWorkspaceLock::acquire(&workspace, &device, 6),
            Err(LocalProfileError::LockBusy)
        ));
        assert_eq!(fs::read(&marker)?, content.as_bytes());
        assert_eq!(fs::read(&parent_lock)?, b"stale-native-parent-lock");
        assert_eq!(fs::read(&wal)?, b"original-uncheckpointed-wal");
        assert_eq!(fs::read(&pending)?, b"encrypted-original-n-plus-one");
        drop(guard);
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn live_native_firefox_handle_blocks_upgrade_even_without_a_bridge_writer()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::windows::fs::OpenOptionsExt;
        let (_test_root, root, workspace, _device) = fixture("orphan-firefox")?;
        let browser_data = workspace.path().join("user_data");
        fs::create_dir(&browser_data)?;
        let lock = browser_data.join("parent.lock");
        fs::write(&lock, b"native-writer")?;
        let browser_handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&lock)?;
        assert!(matches!(
            DeliveryActivationGuard::acquire(&root),
            Err(LocalProfileError::LockBusy)
        ));
        drop(browser_handle);
        assert!(DeliveryActivationGuard::acquire(&root)?.confirms_quiescence());
        assert_eq!(fs::read(&lock)?, b"native-writer");
        Ok(())
    }

    #[test]
    fn interrupted_same_device_reclaims_only_old_marker_and_preserves_browser_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, root, workspace, device) = fixture("same-device-recovery")?;
        let user_data = workspace.path().join("user_data");
        fs::create_dir(&user_data)?;
        let cookies = user_data.join("cookies.sqlite");
        let wal = user_data.join("cookies.sqlite-wal");
        let fingerprint = workspace.path().join("camoufox-config.json");
        let identity = workspace.path().join(".profile-browser-identity-v1");
        fs::write(&cookies, b"original-cookie-db")?;
        fs::write(&wal, b"uncheckpointed-wal")?;
        // Firefox on Windows can leave parent.lock on disk after power loss.
        // A *free* kernel file handle is not an active native browser writer.
        #[cfg(windows)]
        fs::write(user_data.join("parent.lock"), b"stale-native-parent-lock")?;
        fs::write(&fingerprint, b"pinned-config")?;
        fs::write(&identity, b"pinned-identity")?;

        // Simulate the predecessor dying without explicit release. No root OS lock
        // survives, but its persistent marker remains.
        fs::write(
            workspace.path().join(BRIDGE_LOCK_FILE),
            format!("profile-platform-bridge-lock-v1\n{}\n5\n", device.as_str()),
        )?;
        assert!(matches!(
            BridgeWorkspaceLock::acquire(&workspace, &device, 6),
            Err(LocalProfileError::LockBusy)
        ));
        let recovered = BridgeWorkspaceLock::recover_interrupted_launch(&workspace, &device, 6)?;
        assert_eq!(fs::read(cookies)?, b"original-cookie-db");
        assert_eq!(fs::read(wal)?, b"uncheckpointed-wal");
        assert_eq!(fs::read(fingerprint)?, b"pinned-config");
        assert_eq!(fs::read(identity)?, b"pinned-identity");
        assert!(matches!(
            DeliveryActivationGuard::acquire(&root),
            Err(LocalProfileError::LockBusy)
        ));
        assert!(matches!(
            BridgeWorkspaceLock::acquire(&workspace, &device, 7),
            Err(LocalProfileError::LockBusy)
        ));
        recovered.release()?;
        assert!(DeliveryActivationGuard::acquire(&root)?.confirms_quiescence());
        Ok(())
    }

    #[test]
    fn interrupted_reclaim_rejects_foreign_malformed_and_unadvanced_epoch()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, _root, workspace, device) = fixture("reclaim-fail-closed")?;
        let marker = workspace.path().join(BRIDGE_LOCK_FILE);
        let foreign = DeviceId::parse("device_foreign_reclaim_test")?;
        fs::write(
            &marker,
            format!("profile-platform-bridge-lock-v1\n{}\n5\n", foreign.as_str()),
        )?;
        assert!(matches!(
            BridgeWorkspaceLock::recover_interrupted_launch(&workspace, &device, 6),
            Err(LocalProfileError::LockBusy)
        ));

        let prior = format!("profile-platform-bridge-lock-v1\n{}\n5\n", device.as_str());
        fs::write(&marker, &prior)?;
        for epoch in [1, 4, 5] {
            assert!(matches!(
                BridgeWorkspaceLock::recover_interrupted_launch(&workspace, &device, epoch),
                Err(LocalProfileError::LockBusy)
            ));
            assert_eq!(fs::read_to_string(&marker)?, prior);
        }
        fs::write(&marker, b"untrusted-marker\n")?;
        assert!(matches!(
            BridgeWorkspaceLock::recover_interrupted_launch(&workspace, &device, 6),
            Err(LocalProfileError::LockBusy)
        ));
        assert_eq!(fs::read(&marker)?, b"untrusted-marker\n");
        Ok(())
    }

    #[test]
    fn interrupted_reclaim_refuses_active_bridge_admission()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_test_root, root, workspace, device) = fixture("bridge-still-live")?;
        fs::write(
            workspace.path().join(BRIDGE_LOCK_FILE),
            format!("profile-platform-bridge-lock-v1\n{}\n5\n", device.as_str()),
        )?;
        let active = super::open_root_marker(root.path())?;
        active.try_lock_shared()?;
        assert!(matches!(
            BridgeWorkspaceLock::recover_interrupted_launch(&workspace, &device, 6),
            Err(LocalProfileError::LockBusy)
        ));
        assert!(workspace.path().join(BRIDGE_LOCK_FILE).exists());
        Ok(())
    }

    #[test]
    fn interrupted_reclaim_refuses_native_firefox_writer() -> Result<(), Box<dyn std::error::Error>>
    {
        let (_test_root, _root, workspace, device) = fixture("firefox-still-live")?;
        let user_data = workspace.path().join("user_data");
        fs::create_dir(&user_data)?;
        fs::write(
            workspace.path().join(BRIDGE_LOCK_FILE),
            format!("profile-platform-bridge-lock-v1\n{}\n5\n", device.as_str()),
        )?;
        #[cfg(windows)]
        let writer = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .share_mode(0)
                .open(user_data.join("parent.lock"))?
        };
        #[cfg(not(windows))]
        fs::write(user_data.join(".parentlock"), b"unresolved-native-writer")?;
        assert!(matches!(
            BridgeWorkspaceLock::recover_interrupted_launch(&workspace, &device, 6),
            Err(LocalProfileError::LockBusy)
        ));
        assert!(workspace.path().join(BRIDGE_LOCK_FILE).exists());
        #[cfg(windows)]
        drop(writer);
        Ok(())
    }
}
