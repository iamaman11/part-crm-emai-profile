//! The sole normal Windows browser Open owner: local folder + native Camoufox.
//! Cloud authorization/CURRENT/R2 belong to synchronization, not this path.
#![forbid(unsafe_code)]

use crate::generation_initialization::{InitialProfileOpen, InitialProfileStatus};
use crate::local_profile::{BridgeWorkspaceLock, LocalProfileError, MaterializationRoot};
use crate::windows_delivery_runtime::PendingWindowsDeliveryHealthConfirmation;
use profile_platform_primitives::{DeviceId, GenerationId};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const PYTHON: &str = "python/python.exe";
const ENTRYPOINT: &str = "camouhost/real.py";
const LOCK: &str = "camouhost/runtime-lock.json";
const CONFIG: &str = "camoufox-config.json";
const LOCAL_UNSYNCED: &str = ".local-unsynced";
const LOCAL_UNSYNCED_CONTENT: &[u8] = b"local-unsynced-v2";
const CAMOUHOST_LAUNCH_READY_EVENT: &str = "camouhost_stage phase=launch step=ready";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalOpenError {
    InvalidLocalWorkspace,
    RestoreRequired,
    StaleLocalWorkspace,
    RuntimeUnavailable,
    ProfileBusy,
    IdentityPreparation,
    BrowserNotClosedCleanly,
    DeliveryHealth,
}
impl fmt::Display for LocalOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLocalWorkspace => "Local browser folder is unavailable",
            Self::RestoreRequired => "This READY profile has no local copy. Restore the confirmed current version before opening",
            Self::StaleLocalWorkspace => "Local profile version differs from the authorized CRM version; preserve local data and resolve Sync or Restore before opening",
            Self::RuntimeUnavailable => "Installed Camoufox runtime is unavailable",
            Self::ProfileBusy => "This local browser profile is already in use",
            Self::IdentityPreparation => "Local Camoufox identity could not be prepared",
            Self::BrowserNotClosedCleanly => {
                "Browser close was not verified; local files were kept"
            }
            Self::DeliveryHealth => "Installed browser runtime health could not be confirmed",
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

fn admit_existing_workspace(
    current: crate::local_profile::GenerationWorkspace,
    intent: &InitialProfileOpen,
) -> Result<crate::local_profile::GenerationWorkspace, LocalOpenError> {
    // A valid local folder is not automatically the Worker-selected CURRENT.
    // Use the already-authorized launch version; never silently open an older
    // generation or claim that pending local bytes can be overwritten.
    let matches_status = match intent.status {
        InitialProfileStatus::Draft => current.base_generation_id().is_none(),
        InitialProfileStatus::Ready => current.base_generation_id().is_some(),
    };
    if current.profile_version() != intent.expected_version || !matches_status {
        return Err(LocalOpenError::StaleLocalWorkspace);
    }
    Ok(current)
}

fn recheck_workspace_after_writer_admission(
    root: &MaterializationRoot,
    observed: &crate::local_profile::GenerationWorkspace,
    intent: &InitialProfileOpen,
) -> Result<crate::local_profile::GenerationWorkspace, LocalOpenError> {
    // Read the *current* marker while holding the exclusive workspace lock.
    // Sync may have advanced CURRENT between the first fast lookup and admission.
    let current = root
        .open_profile_current(&intent.tenant_id, &intent.profile_id)
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
    if current.path() != observed.path() {
        return Err(LocalOpenError::StaleLocalWorkspace);
    }
    admit_existing_workspace(current, intent)
}

fn open_or_create_workspace(
    root: &MaterializationRoot,
    intent: &InitialProfileOpen,
    generation: &GenerationId,
) -> Result<crate::local_profile::GenerationWorkspace, LocalOpenError> {
    match root.open_profile_current(&intent.tenant_id, &intent.profile_id) {
        Ok(current) => admit_existing_workspace(current, intent),
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound))
            if intent.status == InitialProfileStatus::Draft =>
        {
            match root.create_profile_current(
                &intent.tenant_id,
                &intent.profile_id,
                generation,
                intent.expected_version,
            ) {
                Ok(workspace) => Ok(workspace),
                // Another ordinary Open may have created CURRENT since our
                // first lookup. Reuse only a fully admitted existing folder;
                // the existing OS writer lock resolves competing launches.
                Err(LocalProfileError::TargetAlreadyExists) => {
                    let current = root
                        .open_profile_current(&intent.tenant_id, &intent.profile_id)
                        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
                    admit_existing_workspace(current, intent)
                }
                Err(_) => Err(LocalOpenError::InvalidLocalWorkspace),
            }
        }
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound))
            if intent.status == InitialProfileStatus::Ready =>
        {
            Err(LocalOpenError::RestoreRequired)
        }
        Err(_) => Err(LocalOpenError::InvalidLocalWorkspace),
    }
}

fn observe_runtime_ready_line<F>(line: &str, confirmation: &mut Option<F>) -> Option<bool>
where
    F: FnOnce() -> bool,
{
    if line != CAMOUHOST_LAUNCH_READY_EVENT {
        return None;
    }
    confirmation.take().map(|confirm| confirm())
}

fn run_python(
    runtime_root: &Path,
    folder: &Path,
    action: &str,
    pending_health: Option<PendingWindowsDeliveryHealthConfirmation>,
) -> Result<bool, LocalOpenError> {
    // Never invoke a user-selected executable, shell or remote path.
    for relative in [PYTHON, ENTRYPOINT, LOCK] {
        let file = runtime_root.join(relative);
        let meta = fs::symlink_metadata(file).map_err(|_| LocalOpenError::RuntimeUnavailable)?;
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() == 0 {
            return Err(LocalOpenError::RuntimeUnavailable);
        }
    }
    run_python_with_executable(
        &runtime_root.join(PYTHON),
        runtime_root,
        folder,
        action,
        pending_health,
    )
}

fn run_python_with_executable(
    executable: &Path,
    runtime_root: &Path,
    folder: &Path,
    action: &str,
    pending_health: Option<PendingWindowsDeliveryHealthConfirmation>,
) -> Result<bool, LocalOpenError> {
    // Begin the existing activation-health attempt only after all local
    // runtime files have been admitted and immediately before process launch.
    // If spawn or SDK launch fails, no Ready event can confirm this attempt.
    let started_health = pending_health
        .map(|pending| pending.start_before_runtime_health())
        .transpose()
        .map_err(|_| LocalOpenError::DeliveryHealth)?;
    let health_required = started_health.is_some();
    let (health_result_tx, health_result_rx) = std::sync::mpsc::channel();
    let mut health_confirmation = started_health
        .map(|confirmation| move || confirmation.confirm_after_runtime_ready().is_ok());

    let mut child = Command::new(executable)
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
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| LocalOpenError::RuntimeUnavailable)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(LocalOpenError::RuntimeUnavailable)?;
    let diagnostics = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            // Consume only the exact SDK/Playwright context-ready milestone.
            // Option::take makes confirmation one-shot even if stderr repeats it.
            if let Some(confirmed) = observe_runtime_ready_line(&line, &mut health_confirmation) {
                let _ = health_result_tx.send(confirmed);
            }
            // Runtime stderr can contain upstream/library detail. Forward only
            // the fixed, redacted milestones and typed failure categories.
            if cfg!(debug_assertions)
                && (line.starts_with("camouhost_stage ") || line.starts_with("camouhost_failure "))
            {
                let _ = writeln!(std::io::stderr().lock(), "{line}");
            }
        }
    });
    let status = child.wait();
    let _ = diagnostics.join();
    let completed = status.map_err(|_| LocalOpenError::RuntimeUnavailable)?;
    if !completed.success() {
        return Ok(false);
    }
    if health_required && !matches!(health_result_rx.try_recv(), Ok(true)) {
        return Err(LocalOpenError::DeliveryHealth);
    }
    Ok(true)
}

fn mark_local_unsynced(folder: &Path) -> Result<(), LocalOpenError> {
    // This is NOT a cloud Save receipt. Persist the existing pending-sync
    // marker before user bytes can change. The same existing Sync operation
    // owns retries; this marker adds no service or parallel retry state.
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
    open_with_health(root, intent, device, runtime_root, None)
}

pub(crate) fn open_with_health(
    root: &MaterializationRoot,
    intent: &InitialProfileOpen,
    device: &DeviceId,
    runtime_root: &Path,
    pending_health: Option<PendingWindowsDeliveryHealthConfirmation>,
) -> Result<(), LocalOpenError> {
    let generation = local_generation(intent)?;
    let workspace = open_or_create_workspace(root, intent, &generation)?;
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?
        .as_millis();
    let epoch = u64::try_from(epoch).map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
    let writer = BridgeWorkspaceLock::acquire_or_recover_interrupted(&workspace, device, epoch)
        .map_err(|error| {
            if error == LocalProfileError::LockBusy {
                LocalOpenError::ProfileBusy
            } else {
                LocalOpenError::InvalidLocalWorkspace
            }
        })?;
    // Re-read the single existing generation marker under the acquired OS
    // writer lock. The earlier lookup is only a fast preliminary check.
    let workspace = match recheck_workspace_after_writer_admission(root, &workspace, intent) {
        Ok(current) => current,
        Err(error) => {
            // Browser has not started: never leave a synthetic stale writer
            // marker behind after a rejected launch intent.
            writer
                .release()
                .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
            return Err(error);
        }
    };
    // Rust owns the native Firefox file-access proof before any
    // identity materialization or browser launch.
    if !matches!(
        crate::browser_execution::browser_writer_locks_absent(&workspace),
        Ok(true)
    ) {
        return Err(LocalOpenError::ProfileBusy);
    }
    let config = workspace.path().join(CONFIG);
    if !config.exists() {
        // No R2 snapshot, cloud claim or generation initialization. Official
        // Camoufox fpgen creates the only local v2 identity once.
        if !run_python(
            runtime_root,
            workspace.path(),
            "--materialize-identity",
            None,
        )? {
            return Err(LocalOpenError::IdentityPreparation);
        }
    }
    // Rust owns the native Firefox file-access proof. Camouhost only uses
    // the official SDK and reports the Playwright close event.
    if !matches!(
        crate::browser_execution::browser_writer_locks_absent(&workspace),
        Ok(true)
    ) {
        return Err(LocalOpenError::ProfileBusy);
    }
    mark_local_unsynced(workspace.path())?;
    if !run_python(
        runtime_root,
        workspace.path(),
        "--local-open",
        pending_health,
    )? {
        // Adapter failure is not proof that the browser released its files.
        // The caller starts the existing Sync recovery; its Rust file gate
        // must still block snapshot while Firefox owns the profile.
        return Err(LocalOpenError::BrowserNotClosedCleanly);
    }
    if !matches!(
        crate::browser_execution::browser_writer_locks_absent(&workspace),
        Ok(true)
    ) {
        return Err(LocalOpenError::BrowserNotClosedCleanly);
    }
    writer
        .release()
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)
}

#[cfg(test)]
mod tests {
    use super::{
        CAMOUHOST_LAUNCH_READY_EVENT, ENTRYPOINT, LOCK, LocalOpenError, local_generation,
        mark_local_unsynced, observe_runtime_ready_line, open_or_create_workspace,
        recheck_workspace_after_writer_admission, run_python_with_executable,
    };
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
    fn only_the_exact_playwright_ready_event_confirms_health_once() {
        use std::cell::Cell;

        let calls = Cell::new(0);
        let mut confirmation = Some(|| {
            calls.set(calls.get() + 1);
            true
        });

        assert_eq!(
            observe_runtime_ready_line(
                "camouhost_stage phase=launch step=ready-extra",
                &mut confirmation
            ),
            None
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(
            observe_runtime_ready_line(CAMOUHOST_LAUNCH_READY_EVENT, &mut confirmation),
            Some(true)
        );
        assert_eq!(
            observe_runtime_ready_line(CAMOUHOST_LAUNCH_READY_EVENT, &mut confirmation),
            None
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn failed_health_confirmation_is_reported_and_not_retried() {
        use std::cell::Cell;

        let calls = Cell::new(0);
        let mut confirmation = Some(|| {
            calls.set(calls.get() + 1);
            false
        });
        assert_eq!(
            observe_runtime_ready_line(CAMOUHOST_LAUNCH_READY_EVENT, &mut confirmation),
            Some(false)
        );
        assert_eq!(
            observe_runtime_ready_line(CAMOUHOST_LAUNCH_READY_EVENT, &mut confirmation),
            None
        );
        assert_eq!(calls.get(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn windows_python_stderr_ready_event_confirms_pending_delivery_health()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use std::path::PathBuf;

        let python = std::env::var_os("PROFILE_BRIDGE_TEST_PYTHON")
            .map(PathBuf::from)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "PROFILE_BRIDGE_TEST_PYTHON was not set by the Windows test job",
                )
            })?;
        let directory = tempfile::tempdir()?;
        let runtime_root = directory.path().join("runtime");
        fs::create_dir_all(runtime_root.join("camouhost"))?;
        fs::write(
            runtime_root.join(ENTRYPOINT),
            format!(
                r#"import sys
sys.stderr.write({:?} + '\n')
sys.stderr.flush()
"#,
                CAMOUHOST_LAUNCH_READY_EVENT
            ),
        )?;
        fs::write(runtime_root.join(LOCK), b"test-runtime-lock")?;
        let folder = directory.path().join("profile");
        fs::create_dir(&folder)?;
        let state_root = directory.path().join("delivery-state");
        let pending = crate::windows_delivery_runtime::tests::pending_health_confirmation_for_test(
            &state_root,
        )?;

        assert!(run_python_with_executable(
            &python,
            &runtime_root,
            &folder,
            "--local-open",
            Some(pending),
        )?);
        assert!(
            crate::windows_delivery_runtime::tests::active_health_confirmed_for_test(&state_root,)?
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_without_ready_event_cannot_confirm_delivery_health()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use std::path::PathBuf;

        let python = std::env::var_os("PROFILE_BRIDGE_TEST_PYTHON")
            .map(PathBuf::from)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "PROFILE_BRIDGE_TEST_PYTHON was not set by the Windows test job",
                )
            })?;
        let directory = tempfile::tempdir()?;
        let runtime_root = directory.path().join("runtime");
        fs::create_dir_all(runtime_root.join("camouhost"))?;
        fs::write(
            runtime_root.join(ENTRYPOINT),
            r#"import sys
sys.stderr.write('camouhost_stage phase=launch step=sdk_enter\n')
sys.stderr.flush()
"#,
        )?;
        fs::write(runtime_root.join(LOCK), b"test-runtime-lock")?;
        let folder = directory.path().join("profile");
        fs::create_dir(&folder)?;
        let state_root = directory.path().join("delivery-state");
        let pending = crate::windows_delivery_runtime::tests::pending_health_confirmation_for_test(
            &state_root,
        )?;

        assert_eq!(
            run_python_with_executable(
                &python,
                &runtime_root,
                &folder,
                "--local-open",
                Some(pending),
            ),
            Err(LocalOpenError::DeliveryHealth)
        );
        assert!(
            !crate::windows_delivery_runtime::tests::active_health_confirmed_for_test(&state_root,)?
        );
        Ok(())
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

    #[test]
    fn readback_under_writer_lock_rejects_cloud_base_advanced_after_first_lookup() {
        let directory = tempfile::tempdir().unwrap();
        let root =
            crate::local_profile::MaterializationRoot::open_or_create(directory.path()).unwrap();
        let draft = intent();
        let generation = local_generation(&draft).unwrap();
        let observed = open_or_create_workspace(&root, &draft, &generation).unwrap();
        // Simulate a completed Sync between preliminary lookup and OS admission,
        // using the existing atomic generation marker update.
        root.update_profile_current_sync_base(&observed, &generation, 2)
            .unwrap();
        assert!(matches!(
            recheck_workspace_after_writer_admission(&root, &observed, &draft),
            Err(LocalOpenError::StaleLocalWorkspace)
        ));
        let mut ready = draft.clone();
        ready.status = InitialProfileStatus::Ready;
        ready.expected_version = 2;
        assert_eq!(
            recheck_workspace_after_writer_admission(&root, &observed, &ready)
                .unwrap()
                .profile_version(),
            2
        );
    }

    #[test]
    fn stale_launch_cannot_open_or_replace_an_existing_local_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let root =
            crate::local_profile::MaterializationRoot::open_or_create(directory.path()).unwrap();
        let draft = intent();
        let generation = local_generation(&draft).unwrap();
        let workspace = open_or_create_workspace(&root, &draft, &generation).unwrap();
        std::fs::write(
            workspace.path().join(".local-unsynced"),
            b"local-unsynced-v2",
        )
        .unwrap();
        let mut advanced = draft.clone();
        advanced.status = InitialProfileStatus::Ready;
        advanced.expected_version = 2;
        assert!(matches!(
            open_or_create_workspace(&root, &advanced, &generation),
            Err(LocalOpenError::StaleLocalWorkspace)
        ));
        assert_eq!(
            std::fs::read(workspace.path().join(".local-unsynced")).unwrap(),
            b"local-unsynced-v2"
        );
        // A stale READY status may not admit an uncommitted local DRAFT either.
        advanced.expected_version = draft.expected_version;
        assert!(matches!(
            open_or_create_workspace(&root, &advanced, &generation),
            Err(LocalOpenError::StaleLocalWorkspace)
        ));
        // The original DRAFT remains the same local folder; no cloud is needed.
        assert_eq!(
            open_or_create_workspace(&root, &draft, &generation)
                .unwrap()
                .path(),
            workspace.path()
        );
    }
}
