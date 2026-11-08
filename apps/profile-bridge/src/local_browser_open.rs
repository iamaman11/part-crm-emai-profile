//! The sole normal Windows browser Open owner: local folder + native Camoufox.
//! Cloud authorization/CURRENT/R2 belong to synchronization, not this path.
#![forbid(unsafe_code)]

use crate::generation_initialization::{InitialProfileOpen, InitialProfileStatus};
use crate::local_profile::{BridgeWorkspaceLock, LocalProfileError, MaterializationRoot};
use profile_platform_primitives::{DeviceId, GenerationId};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const PYTHON: &str = "python/python.exe";
const ENTRYPOINT: &str = "camouhost/real.py";
const LOCK: &str = "camouhost/runtime-lock.json";
const CONFIG: &str = "camoufox-config.json";
const LOCAL_UNSYNCED: &str = ".local-unsynced";
const LOCAL_UNSYNCED_CONTENT: &[u8] = b"local-unsynced-v2";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalOpenError {
    InvalidLocalWorkspace,
    RestoreRequired,
    RuntimeUnavailable,
    ProfileBusy,
    IdentityPreparation,
    BrowserNotClosedCleanly,
}
impl fmt::Display for LocalOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLocalWorkspace => "Local browser folder is unavailable",
            Self::RestoreRequired => "This READY profile has no local copy. Restore the confirmed current version before opening",
            Self::RuntimeUnavailable => "Installed Camoufox runtime is unavailable",
            Self::ProfileBusy => "This local browser profile is already in use",
            Self::IdentityPreparation => "Local Camoufox identity could not be prepared",
            Self::BrowserNotClosedCleanly => {
                "Browser close was not verified; local files were kept"
            }
        })
    }
}
impl std::error::Error for LocalOpenError {}

/// Exactly one deterministic local working folder per tenant/profile. The
/// cloud object version cannot change its local pathname, and historical
/// generations are not an input to ordinary Open.
fn local_generation(intent: &InitialProfileOpen) -> Result<GenerationId, LocalOpenError> {
    let mut h = Sha256::new();
    h.update(b"profile-bridge/local-current/v2\0");
    h.update(intent.tenant_id.as_str().as_bytes());
    h.update([0]);
    h.update(intent.profile_id.as_str().as_bytes());
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    GenerationId::parse(format!("generation_{hex}"))
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)
}

fn open_or_create_workspace(
    root: &MaterializationRoot,
    intent: &InitialProfileOpen,
    generation: &GenerationId,
) -> Result<crate::local_profile::GenerationWorkspace, LocalOpenError> {
    match root.open_profile_current(&intent.tenant_id, &intent.profile_id) {
        Ok(current) => Ok(current.into_workspace()),
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound))
            if intent.status == InitialProfileStatus::Draft =>
        {
            root.create_profile_current(
                &intent.tenant_id,
                &intent.profile_id,
                generation,
                intent.expected_version,
            )
            .map_err(|_| LocalOpenError::InvalidLocalWorkspace)
        }
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound))
            if intent.status == InitialProfileStatus::Ready =>
        {
            Err(LocalOpenError::RestoreRequired)
        }
        Err(_) => Err(LocalOpenError::InvalidLocalWorkspace),
    }
}

fn run_python(runtime_root: &Path, folder: &Path, action: &str) -> Result<bool, LocalOpenError> {
    // Never invoke a user-selected executable, shell or remote path.
    for relative in [PYTHON, ENTRYPOINT, LOCK] {
        let file = runtime_root.join(relative);
        let meta = fs::symlink_metadata(file).map_err(|_| LocalOpenError::RuntimeUnavailable)?;
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() == 0 {
            return Err(LocalOpenError::RuntimeUnavailable);
        }
    }
    let completed = Command::new(runtime_root.join(PYTHON))
        .arg("-B")
        .arg(runtime_root.join(ENTRYPOINT))
        .arg(action)
        .arg(folder)
        .current_dir(runtime_root)
        .env("CAMOUHOST_RUNTIME_LOCK", runtime_root.join(LOCK))
        .env("CAMOUHOST_EXECUTION_MODE", "user-native")
        .env("CAMOUHOST_HEADLESS_MODE", "false")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| LocalOpenError::RuntimeUnavailable)?;
    Ok(completed.success())
}

fn mark_local_unsynced(folder: &Path) -> Result<(), LocalOpenError> {
    // This is NOT a cloud Save receipt. Persist the pending-sync evidence
    // before any user bytes can change, including on interrupted/dirty close.
    // No background service, online/offline mode or retry state machine.
    let file = folder.join(LOCAL_UNSYNCED);
    match OpenOptions::new().create_new(true).write(true).open(&file) {
        Ok(mut stream) => {
            stream
                .write_all(LOCAL_UNSYNCED_CONTENT)
                .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
            stream
                .sync_all()
                .map_err(|_| LocalOpenError::InvalidLocalWorkspace)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata =
                fs::symlink_metadata(&file).map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() != LOCAL_UNSYNCED_CONTENT.len() as u64
                || fs::read(&file).map_err(|_| LocalOpenError::InvalidLocalWorkspace)?
                    != LOCAL_UNSYNCED_CONTENT
            {
                return Err(LocalOpenError::InvalidLocalWorkspace);
            }
            Ok(())
        }
        Err(_) => Err(LocalOpenError::InvalidLocalWorkspace),
    }
}

pub fn open(
    root: &MaterializationRoot,
    intent: &InitialProfileOpen,
    device: &DeviceId,
    runtime_root: &Path,
) -> Result<(), LocalOpenError> {
    let generation = local_generation(intent)?;
    let workspace = open_or_create_workspace(root, intent, &generation)?;
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?
        .as_millis();
    let epoch = u64::try_from(epoch).map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
    let writer = match BridgeWorkspaceLock::acquire(&workspace, device, epoch) {
        Ok(writer) => writer,
        Err(LocalProfileError::LockBusy) => {
            BridgeWorkspaceLock::recover_interrupted_launch(&workspace, device, epoch)
                .map_err(|_| LocalOpenError::ProfileBusy)?
        }
        Err(_) => return Err(LocalOpenError::InvalidLocalWorkspace),
    };
    let config = workspace.path().join(CONFIG);
    if !config.exists() {
        // No R2 snapshot, cloud claim or generation initialization. Official
        // Camoufox fpgen creates the only local v2 identity once.
        if !run_python(runtime_root, workspace.path(), "--materialize-identity")? {
            return Err(LocalOpenError::IdentityPreparation);
        }
    }
    mark_local_unsynced(workspace.path())?;
    if !run_python(runtime_root, workspace.path(), "--local-open")? {
        // An abnormal exit leaves the durable lock marker as crash evidence.
        // Never erase the user's cookies/WAL/session to make a retry pass.
        return Err(LocalOpenError::BrowserNotClosedCleanly);
    }
    writer
        .release()
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)
}

#[cfg(test)]
mod tests {
    use super::{LocalOpenError, local_generation, mark_local_unsynced, open_or_create_workspace};
    use crate::generation_initialization::{InitialProfileOpen, InitialProfileStatus};
    use profile_platform_primitives::{ActorId, ProfileId, TenantId};

    fn intent() -> InitialProfileOpen {
        InitialProfileOpen {
            tenant_id: TenantId::parse("tenant_01JLOCAL").unwrap(),
            actor_id: ActorId::parse("actor_01JLOCAL").unwrap(),
            profile_id: ProfileId::parse("profile_01JLOCAL").unwrap(),
            expected_version: 1,
            status: InitialProfileStatus::Draft,
        }
    }
    #[test]
    fn pending_local_data_is_not_a_cloud_save_receipt() {
        let folder = tempfile::tempdir().unwrap();
        mark_local_unsynced(folder.path()).unwrap();
        assert_eq!(
            std::fs::read(folder.path().join(".local-unsynced")).unwrap(),
            b"local-unsynced-v2"
        );
        mark_local_unsynced(folder.path()).unwrap();
        std::fs::write(folder.path().join(".local-unsynced"), b"corrupt").unwrap();
        assert_eq!(
            mark_local_unsynced(folder.path()),
            Err(LocalOpenError::InvalidLocalWorkspace)
        );
    }

    #[test]
    fn ready_without_local_copy_fails_closed_instead_of_creating_a_blank_folder() {
        let directory = tempfile::tempdir().unwrap();
        let root =
            crate::local_profile::MaterializationRoot::open_or_create(directory.path()).unwrap();
        let mut intent = intent();
        intent.status = InitialProfileStatus::Ready;
        let generation = local_generation(&intent).unwrap();
        assert!(matches!(
            open_or_create_workspace(&root, &intent, &generation),
            Err(LocalOpenError::RestoreRequired)
        ));
        assert!(
            root.open_profile_current(&intent.tenant_id, &intent.profile_id)
                .is_err()
        );
    }

    #[test]
    fn draft_without_local_copy_creates_one_local_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let root =
            crate::local_profile::MaterializationRoot::open_or_create(directory.path()).unwrap();
        let intent = intent();
        let generation = local_generation(&intent).unwrap();
        let created = open_or_create_workspace(&root, &intent, &generation).unwrap();
        let reopened = root
            .open_profile_current(&intent.tenant_id, &intent.profile_id)
            .unwrap();
        assert_eq!(created.path(), reopened.path());
    }

    #[test]
    fn same_profile_uses_one_local_folder_without_cloud_versions_or_transport() {
        let mut selected = intent();
        let first = local_generation(&selected).unwrap();
        selected.expected_version = 999;
        selected.status = InitialProfileStatus::Ready;
        assert_eq!(local_generation(&selected).unwrap(), first);
        selected.actor_id = ActorId::parse("actor_02JLOCAL").unwrap();
        assert_eq!(local_generation(&selected).unwrap(), first);
        selected.profile_id = ProfileId::parse("profile_02JLOCAL").unwrap();
        assert_ne!(local_generation(&selected).unwrap(), first);
        let _ = LocalOpenError::ProfileBusy.to_string();
    }
}
