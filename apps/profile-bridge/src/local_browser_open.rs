//! The sole normal Windows browser Open owner: local folder + native Camoufox.
//! Cloud authorization/CURRENT/R2 belong to synchronization, not this path.
#![forbid(unsafe_code)]

use crate::generation_initialization::{InitialProfileOpen, InitialProfileStatus};
use crate::local_profile::{BridgeWorkspaceLock, LocalProfileError, MaterializationRoot};
use crate::user_diagnostics::report::{self, Cause, Cloud, Outcome, Stage};
use profile_platform_primitives::{DeviceId, GenerationId};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PYTHON: &str = "python/python.exe";
const ENTRYPOINT: &str = "camouhost/real.py";
const LOCK: &str = "camouhost/runtime-lock.json";
const CONFIG: &str = "camoufox-config.json";
const USER_DATA: &str = "user_data";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const LOCAL_UNSYNCED: &str = ".local-unsynced";
const LOCAL_UNSYNCED_CONTENT: &[u8] = b"local-unsynced-v2";
const CAMOUHOST_LAUNCH_READY_EVENT: &str = "camouhost_stage phase=launch step=ready";
const CAMOUHOST_NATIVE_CLOSE_EVENT: &str =
    "camouhost_stage phase=close step=browser_context_closed";
const IDENTITY_ACK: &[u8] = b"camouhost-identity-persisted\n";

pub(crate) type ReadyConfirmation = Box<dyn FnOnce() -> bool + Send + 'static>;
pub(crate) type BeforeLaunch =
    Box<dyn FnOnce() -> Result<Option<ReadyConfirmation>, ()> + Send + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalOpenError {
    InvalidLocalWorkspace,
    RestoreRequired,
    StaleLocalWorkspace,
    RuntimeUnavailable,
    ProfileBusy,
    IdentityPreparation,
    BrowserNotClosedCleanly,
    LaunchConfirmation,
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
            Self::LaunchConfirmation => "Browser runtime launch confirmation was not completed",
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
    // The installed local marker is the sole Open state owner. A successful
    // post-close Sync may already have advanced D1 CURRENT and this marker
    // while the CRM tab still displays the old version. Such a previously
    // admitted local copy must remain reopenable without another Worker call.
    //
    // Never open a local folder *behind* the known server projection, and
    // never reinterpret an uncommitted DRAFT as a READY profile.
    let local_version = current.profile_version();
    let matches_state = match intent.status {
        InitialProfileStatus::Draft if current.base_generation_id().is_none() => {
            local_version == intent.expected_version
        }
        InitialProfileStatus::Draft => local_version > intent.expected_version,
        InitialProfileStatus::Ready => {
            current.base_generation_id().is_some() && local_version >= intent.expected_version
        }
    };
    if !matches_state {
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
    // A Sync may have advanced the marker after our preliminary read.
    // Even if the original CRM version is old, never accept a *race* during
    // writer admission: Open must retry against one stable local snapshot.
    if current.path() != observed.path()
        || current.profile_version() != observed.profile_version()
        || current.base_generation_id() != observed.base_generation_id()
    {
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
    before_launch: Option<BeforeLaunch>,
) -> Result<bool, LocalOpenError> {
    // Never invoke a user-selected executable, shell or remote path.
    for relative in [PYTHON, ENTRYPOINT, LOCK] {
        let file = runtime_root.join(relative);
        let meta = fs::symlink_metadata(file).map_err(|_| LocalOpenError::RuntimeUnavailable)?;
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() == 0 {
            return Err(LocalOpenError::RuntimeUnavailable);
        }
    }

    let executable = runtime_root.join(PYTHON);
    if action == "--local-open" {
        let first_run = ensure_user_data_directory(folder)?;
        let config = if folder.join(CONFIG).exists() {
            Some(read_profile_config(folder)?)
        } else {
            // Official fpgen draws only when DRAFT has no existing browser bytes.
            require_empty_user_data(folder)?;
            None
        };
        let first_identity = config.is_none();
        let mut input = serde_json::to_vec(&serde_json::json!({
            "config_json": config,
            "first_run": first_run,
        }))
        .map_err(|_| LocalOpenError::IdentityPreparation)?;
        input.push(b'\n');
        return run_python_with_input(
            &executable,
            runtime_root,
            folder,
            action,
            before_launch,
            Some(&input),
            first_identity,
        );
    }
    run_python_with_input(
        &executable,
        runtime_root,
        folder,
        action,
        before_launch,
        None,
        false,
    )
}

fn ensure_user_data_directory(folder: &Path) -> Result<bool, LocalOpenError> {
    let user_data = folder.join(USER_DATA);
    match fs::symlink_metadata(&user_data) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(LocalOpenError::InvalidLocalWorkspace);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&user_data).map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
        }
        Err(_) => return Err(LocalOpenError::InvalidLocalWorkspace),
    }
    let mut entries =
        fs::read_dir(&user_data).map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(_)) => Err(LocalOpenError::InvalidLocalWorkspace),
    }
}

fn require_empty_user_data(folder: &Path) -> Result<(), LocalOpenError> {
    if ensure_user_data_directory(folder)? {
        Ok(())
    } else {
        Err(LocalOpenError::IdentityPreparation)
    }
}

fn read_profile_config(folder: &Path) -> Result<String, LocalOpenError> {
    let path = folder.join(CONFIG);
    let metadata = fs::symlink_metadata(&path).map_err(|_| LocalOpenError::IdentityPreparation)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(LocalOpenError::IdentityPreparation);
    }
    let bytes = fs::read(path).map_err(|_| LocalOpenError::IdentityPreparation)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| LocalOpenError::IdentityPreparation)?;
    if !value.is_object() {
        return Err(LocalOpenError::IdentityPreparation);
    }
    String::from_utf8(bytes).map_err(|_| LocalOpenError::IdentityPreparation)
}

/// Rust owns the first local config write; the official SDK generated it
/// inside the same process that waits for an acknowledgment before launching.
fn persist_sdk_identity(folder: &Path, raw: &[u8]) -> Result<(), LocalOpenError> {
    let report: serde_json::Value =
        serde_json::from_slice(raw).map_err(|_| LocalOpenError::IdentityPreparation)?;
    let config_json = report
        .get("camoufox_config_json")
        .and_then(serde_json::Value::as_str)
        .ok_or(LocalOpenError::IdentityPreparation)?;
    let expected_digest = report
        .get("fingerprint_config_sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or(LocalOpenError::IdentityPreparation)?;
    let config_bytes = config_json.as_bytes();
    if config_bytes.is_empty()
        || config_bytes.len() as u64 > MAX_CONFIG_BYTES
        || !config_bytes.ends_with(b"\n")
    {
        return Err(LocalOpenError::IdentityPreparation);
    }
    let actual_digest = Sha256::digest(config_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("");
    if actual_digest != expected_digest {
        return Err(LocalOpenError::IdentityPreparation);
    }
    let value: serde_json::Value =
        serde_json::from_slice(config_bytes).map_err(|_| LocalOpenError::IdentityPreparation)?;
    if !value.is_object() {
        return Err(LocalOpenError::IdentityPreparation);
    }

    let destination = folder.join(CONFIG);
    match fs::symlink_metadata(&destination) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(LocalOpenError::IdentityPreparation),
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(folder).map_err(|_| LocalOpenError::IdentityPreparation)?;
    temporary
        .write_all(config_bytes)
        .map_err(|_| LocalOpenError::IdentityPreparation)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| LocalOpenError::IdentityPreparation)?;
    temporary
        .persist_noclobber(&destination)
        .map_err(|_| LocalOpenError::IdentityPreparation)?;
    Ok(())
}

fn run_python_with_executable(
    executable: &Path,
    runtime_root: &Path,
    folder: &Path,
    action: &str,
    before_launch: Option<BeforeLaunch>,
) -> Result<bool, LocalOpenError> {
    run_python_with_input(
        executable,
        runtime_root,
        folder,
        action,
        before_launch,
        None,
        false,
    )
}

// Official Playwright close confirmation can precede a driver teardown error.
// The native Firefox OS file lock still has to be independently released by
// Rust before the caller treats this as a safe post-close filesystem state.
fn adapter_completed_after_native_close(
    action: &str,
    process_success: bool,
    close_seen: bool,
) -> bool {
    process_success || (action == "--local-open" && close_seen)
}

#[cfg(windows)]
fn native_adapter_shutdown_allowed(
    browser_ready: bool,
    writer_was_observed: bool,
    no_writer_for: Option<Duration>,
) -> bool {
    browser_ready
        && writer_was_observed
        && no_writer_for.is_some_and(|elapsed| elapsed >= Duration::from_secs(2))
}

// A process exiting just before termination is a normal SDK completion,
// not evidence that the native fallback stopped it. Failed termination of a
// still-running adapter must never turn into a synthetic close receipt.
#[cfg(windows)]
fn stop_stuck_native_adapter(
    child: &mut std::process::Child,
) -> Result<(std::process::ExitStatus, bool), LocalOpenError> {
    if let Some(status) = child
        .try_wait()
        .map_err(|_| LocalOpenError::RuntimeUnavailable)?
    {
        return Ok((status, false));
    }
    match child.kill() {
        Ok(()) => {
            let status = child
                .wait()
                .map_err(|_| LocalOpenError::RuntimeUnavailable)?;
            Ok((status, true))
        }
        Err(_) => match child
            .try_wait()
            .map_err(|_| LocalOpenError::RuntimeUnavailable)?
        {
            Some(status) => Ok((status, false)),
            None => Err(LocalOpenError::RuntimeUnavailable),
        },
    }
}

// Persistent BrowserContext.close may be lost when Firefox has already exited.
// The existing OS file-lock proof determines whether native bytes are safe;
// no browser PID, window, or Firefox close command is used. Once the verified
// READY native writer is gone, stop only our stuck Python adapter child.
#[cfg(windows)]
fn await_adapter_or_native_release(
    child: &mut std::process::Child,
    folder: &Path,
    browser_ready: &AtomicBool,
) -> Result<(std::process::ExitStatus, bool), LocalOpenError> {
    let mut writer_was_observed = false;
    let mut released_since: Option<Instant> = None;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| LocalOpenError::RuntimeUnavailable)?
        {
            return Ok((status, false));
        }
        let ready = browser_ready.load(Ordering::Acquire);
        if ready {
            match crate::browser_execution::browser_writer_locks_absent_at(folder) {
                Ok(false) => {
                    writer_was_observed = true;
                    released_since = None;
                }
                Ok(true) if writer_was_observed => {
                    let since = released_since.get_or_insert_with(Instant::now);
                    if native_adapter_shutdown_allowed(
                        ready,
                        writer_was_observed,
                        Some(since.elapsed()),
                    ) {
                        // Firefox already released the Windows writer lock.
                        // Never terminate or close the browser process here.
                        return stop_stuck_native_adapter(child);
                    }
                }
                _ => released_since = None,
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn run_python_with_input(
    executable: &Path,
    runtime_root: &Path,
    folder: &Path,
    action: &str,
    before_launch: Option<BeforeLaunch>,
    stdin_payload: Option<&[u8]>,
    generate_identity: bool,
) -> Result<bool, LocalOpenError> {
    // Delivery health is supplied by the composition owner as a one-shot
    // callback. Local Open knows only when the native runtime is launched and
    // when the official Playwright Ready event arrives.
    let mut health_confirmation = before_launch
        .map(|before| before().map_err(|_| LocalOpenError::LaunchConfirmation))
        .transpose()?
        .flatten();
    let health_required = health_confirmation.is_some();
    let (health_result_tx, health_result_rx) = std::sync::mpsc::channel();
    let native_close_seen = Arc::new(AtomicBool::new(false));
    let close_seen_by_reader = Arc::clone(&native_close_seen);
    let native_ready_seen = Arc::new(AtomicBool::new(false));
    let ready_seen_by_reader = Arc::clone(&native_ready_seen);

    let native_started = Instant::now();
    let mut child = Command::new(executable)
        .arg("-B")
        .arg(runtime_root.join(ENTRYPOINT))
        .arg(action)
        .arg(folder)
        .current_dir(runtime_root)
        .env("CAMOUHOST_RUNTIME_LOCK", runtime_root.join(LOCK))
        .env("CAMOUHOST_EXECUTION_MODE", "user-native")
        .env("CAMOUHOST_HEADLESS_MODE", "false")
        .stdin(if stdin_payload.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(if generate_identity {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| LocalOpenError::RuntimeUnavailable)?;
    let mut stdin = child.stdin.take();
    if let Some(payload) = stdin_payload {
        let written = stdin
            .as_mut()
            .is_some_and(|stream| stream.write_all(payload).is_ok());
        if !written {
            let _ = child.kill();
            let _ = child.wait();
            return Err(LocalOpenError::RuntimeUnavailable);
        }
    }
    let stderr = child
        .stderr
        .take()
        .ok_or(LocalOpenError::RuntimeUnavailable)?;
    let diagnostics = std::thread::spawn(move || {
        let mut native_ready_observed = false;
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if line == CAMOUHOST_LAUNCH_READY_EVENT && !native_ready_observed {
                native_ready_observed = true;
                ready_seen_by_reader.store(true, Ordering::Release);
                let duration = u64::try_from(native_started.elapsed().as_millis())
                    .unwrap_or(u64::MAX)
                    .min(86_400_000);
                report::record_current(
                    Stage::BrowserReady,
                    Outcome::Observed,
                    Cloud::Unknown,
                    Cause::None,
                    duration,
                );
            }
            if line == CAMOUHOST_NATIVE_CLOSE_EVENT {
                close_seen_by_reader.store(true, Ordering::Release);
            }
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
    if generate_identity {
        // One bounded SDK result, persisted by the Rust file owner before
        // Camoufox begins launching. Fail closed on a partial IPC exchange.
        let persisted = (|| -> Result<(), LocalOpenError> {
            let output = child
                .stdout
                .take()
                .ok_or(LocalOpenError::IdentityPreparation)?;
            let mut stream = BufReader::new(output).take(MAX_CONFIG_BYTES * 8);
            let mut line = Vec::new();
            stream
                .read_until(b'\n', &mut line)
                .map_err(|_| LocalOpenError::IdentityPreparation)?;
            if !line.ends_with(b"\n") {
                return Err(LocalOpenError::IdentityPreparation);
            }
            persist_sdk_identity(folder, &line)?;
            stdin
                .as_mut()
                .ok_or(LocalOpenError::IdentityPreparation)?
                .write_all(IDENTITY_ACK)
                .map_err(|_| LocalOpenError::IdentityPreparation)
        })();
        if let Err(error) = persisted {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            let _ = diagnostics.join();
            return Err(error);
        }
    }
    drop(stdin);
    #[cfg(windows)]
    let (completed, native_adapter_released) = if action == "--local-open" {
        await_adapter_or_native_release(&mut child, folder, &native_ready_seen)?
    } else {
        (
            child
                .wait()
                .map_err(|_| LocalOpenError::RuntimeUnavailable)?,
            false,
        )
    };
    #[cfg(not(windows))]
    let (completed, native_adapter_released) = (
        child
            .wait()
            .map_err(|_| LocalOpenError::RuntimeUnavailable)?,
        false,
    );
    if !native_adapter_released {
        let _ = diagnostics.join();
    }
    // The stderr reader must not block Open on an abandoned driver pipe
    // after the existing OS proof has already allowed adapter shutdown.
    if health_required && !matches!(health_result_rx.try_recv(), Ok(true)) {
        return Err(LocalOpenError::LaunchConfirmation);
    }
    Ok(native_adapter_released
        || adapter_completed_after_native_close(
            action,
            completed.success(),
            native_close_seen.load(Ordering::Acquire),
        ))
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
    open_with_before_launch(root, intent, device, runtime_root, None)
}

pub(crate) fn open_with_before_launch(
    root: &MaterializationRoot,
    intent: &InitialProfileOpen,
    device: &DeviceId,
    runtime_root: &Path,
    before_launch: Option<BeforeLaunch>,
) -> Result<(), LocalOpenError> {
    let local_started = Instant::now();
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
    // One Python process draws a new official identity only for an empty
    // profile; existing saved SDK options go directly to native Camoufox.
    mark_local_unsynced(workspace.path())?;
    let local_ms = u64::try_from(local_started.elapsed().as_millis())
        .unwrap_or(u64::MAX)
        .min(86_400_000);
    report::record_current(
        Stage::CamouhostLaunch,
        Outcome::Started,
        Cloud::Unknown,
        Cause::None,
        local_ms,
    );
    if !run_python(
        runtime_root,
        workspace.path(),
        "--local-open",
        before_launch,
    )? {
        // Adapter failure is not proof that the browser released its files.
        // The caller starts the existing Sync recovery; its Rust file gate
        // must still block snapshot while Firefox owns the profile.
        return Err(LocalOpenError::BrowserNotClosedCleanly);
    }
    // Firefox may release its last Windows parent.lock handle just after the
    // official Playwright close event. Read the SAME OS proof a few more times
    // rather than misreporting an already closed browser as unverified.
    let mut native_released = false;
    for attempt in 0..20 {
        match crate::browser_execution::browser_writer_locks_absent(&workspace) {
            Ok(true) => {
                native_released = true;
                break;
            }
            Ok(false) if attempt < 19 => std::thread::sleep(Duration::from_millis(50)),
            _ => break,
        }
    }
    if !native_released {
        return Err(LocalOpenError::BrowserNotClosedCleanly);
    }
    writer
        .release()
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)
}

#[cfg(test)]
mod tests {
    use super::{
        BeforeLaunch, CAMOUHOST_LAUNCH_READY_EVENT, ENTRYPOINT, LOCK, LocalOpenError,
        ReadyConfirmation, local_generation, mark_local_unsynced, observe_runtime_ready_line,
        open_or_create_workspace, recheck_workspace_after_writer_admission,
        run_python_with_executable,
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
    fn rust_owns_user_data_creation_and_profile_config_read()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;

        let directory = tempfile::tempdir()?;
        let folder = directory.path();
        assert!(super::ensure_user_data_directory(folder)?);
        assert!(folder.join(super::USER_DATA).is_dir());
        assert!(super::ensure_user_data_directory(folder)?);
        fs::write(
            folder.join(super::USER_DATA).join("prefs.js"),
            b"native browser state",
        )?;
        assert!(!super::ensure_user_data_directory(folder)?);

        let config = b"{\"fingerprint\":\"test\"}\n";
        fs::write(folder.join(super::CONFIG), config)?;
        assert_eq!(
            super::read_profile_config(folder)?,
            String::from_utf8(config.to_vec())?
        );
        Ok(())
    }

    #[test]
    fn identity_generation_requires_empty_user_data() -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;

        let directory = tempfile::tempdir()?;
        let folder = directory.path();
        assert_eq!(super::require_empty_user_data(folder), Ok(()));
        fs::write(
            folder.join(super::USER_DATA).join("prefs.js"),
            b"existing browser state",
        )?;
        assert_eq!(
            super::require_empty_user_data(folder),
            Err(LocalOpenError::IdentityPreparation)
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn native_adapter_release_requires_ready_writer_and_stable_absence() {
        use super::{Duration, native_adapter_shutdown_allowed};
        let quiet = Some(Duration::from_secs(3));
        assert!(!native_adapter_shutdown_allowed(false, true, quiet));
        assert!(!native_adapter_shutdown_allowed(true, false, quiet));
        assert!(!native_adapter_shutdown_allowed(
            true,
            true,
            Some(Duration::from_millis(1999)),
        ));
        assert!(!native_adapter_shutdown_allowed(true, true, None));
        assert!(native_adapter_shutdown_allowed(
            true,
            true,
            Some(Duration::from_secs(2)),
        ));
    }

    // Exercise the actual Windows child-process boundary, not just a boolean
    // predicate. This filtered test binary is the stand-in for a stuck Python
    // adapter; its sleep is enabled only in a separately spawned child.
    #[cfg(windows)]
    #[test]
    fn native_adapter_child_fixture() {
        if let Ok(delay_ms) = std::env::var("PROFILE_BRIDGE_TEST_CHILD_SLEEP_MS") {
            let delay_ms: u64 = delay_ms.parse().expect("test child delay");
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }

    #[cfg(windows)]
    fn spawn_native_adapter_fixture(
        delay_ms: u64,
    ) -> Result<std::process::Child, Box<dyn std::error::Error>> {
        use std::process::{Command, Stdio};

        Ok(Command::new(std::env::current_exe()?)
            .arg("native_adapter_child_fixture")
            .env("PROFILE_BRIDGE_TEST_CHILD_SLEEP_MS", delay_ms.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?)
    }

    #[cfg(windows)]
    fn hold_native_writer_lock(
        folder: &std::path::Path,
    ) -> Result<std::fs::File, Box<dyn std::error::Error>> {
        use std::fs::{self, OpenOptions};
        use std::os::windows::fs::OpenOptionsExt;

        let browser_data = folder.join("user_data");
        fs::create_dir_all(&browser_data)?;
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .share_mode(0)
            .open(browser_data.join("parent.lock"))?;
        assert!(!crate::browser_execution::browser_writer_locks_absent_at(
            folder
        )?);
        Ok(lock)
    }

    #[cfg(windows)]
    #[test]
    fn native_adapter_release_ends_only_stuck_child_after_real_windows_lock_release()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::AtomicBool;
        use std::time::{Duration, Instant};

        let root = tempfile::tempdir()?;
        let writer = hold_native_writer_lock(root.path())?;
        let mut child = spawn_native_adapter_fixture(10_000)?;
        let ready = AtomicBool::new(true);
        let released = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            drop(writer);
        });
        let started = Instant::now();
        let (status, adapter_stopped) =
            super::await_adapter_or_native_release(&mut child, root.path(), &ready)?;
        released.join().expect("native writer release thread");
        assert!(
            adapter_stopped,
            "only the proven released adapter may be stopped"
        );
        assert!(!status.success(), "the sleeping adapter was terminated");
        assert!(started.elapsed() >= Duration::from_secs(2));
        assert!(crate::browser_execution::browser_writer_locks_absent_at(
            root.path()
        )?);
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn native_adapter_release_never_stops_child_without_complete_writer_proof()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::AtomicBool;

        let root = tempfile::tempdir()?;
        let writer = hold_native_writer_lock(root.path())?;
        let mut child = spawn_native_adapter_fixture(400)?;
        let ready = AtomicBool::new(true);
        let (status, stopped) =
            super::await_adapter_or_native_release(&mut child, root.path(), &ready)?;
        assert!(
            status.success(),
            "live writer requires natural child completion"
        );
        assert!(
            !stopped,
            "an active Firefox writer must never authorize stop"
        );

        let mut child = spawn_native_adapter_fixture(400)?;
        let ready = AtomicBool::new(false);
        let (status, stopped) =
            super::await_adapter_or_native_release(&mut child, root.path(), &ready)?;
        assert!(status.success());
        assert!(!stopped, "missing READY must not authorize stop");
        drop(writer);

        let mut child = spawn_native_adapter_fixture(400)?;
        let ready = AtomicBool::new(true);
        let (status, stopped) =
            super::await_adapter_or_native_release(&mut child, root.path(), &ready)?;
        assert!(status.success());
        assert!(!stopped, "unobserved writer must not authorize stop");
        Ok(())
    }

    #[test]
    fn post_close_playwright_driver_failure_does_not_invalidate_native_close_proof() {
        assert!(super::adapter_completed_after_native_close(
            "--local-open",
            true,
            false
        ));
        // Driver teardown failed after the official BrowserContext.close event;
        // Rust still gates cloud Sync on the native Windows Firefox file lock.
        assert!(super::adapter_completed_after_native_close(
            "--local-open",
            false,
            true
        ));
        // Launch/runtime failures without the exact native close event remain errors.
        assert!(!super::adapter_completed_after_native_close(
            "--local-open",
            false,
            false
        ));
        assert!(!super::adapter_completed_after_native_close(
            "--other", false, true
        ));
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
    #[ignore = "requires PROFILE_BRIDGE_TEST_PYTHON"]
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

        let before_launch: BeforeLaunch = Box::new(move || {
            let started = pending.start_before_runtime_health().map_err(|_| ())?;
            let ready: ReadyConfirmation =
                Box::new(move || started.confirm_after_runtime_ready().is_ok());
            Ok(Some(ready))
        });
        assert!(run_python_with_executable(
            &python,
            &runtime_root,
            &folder,
            "--local-open",
            Some(before_launch),
        )?);
        assert!(
            crate::windows_delivery_runtime::tests::active_health_confirmed_for_test(&state_root,)?
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires PROFILE_BRIDGE_TEST_PYTHON"]
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

        let before_launch: BeforeLaunch = Box::new(move || {
            let started = pending.start_before_runtime_health().map_err(|_| ())?;
            let ready: ReadyConfirmation =
                Box::new(move || started.confirm_after_runtime_ready().is_ok());
            Ok(Some(ready))
        });
        assert_eq!(
            run_python_with_executable(
                &python,
                &runtime_root,
                &folder,
                "--local-open",
                Some(before_launch),
            ),
            Err(LocalOpenError::LaunchConfirmation)
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
    fn locally_committed_current_reopens_with_stale_crm_projection_without_cloud() {
        let directory = tempfile::tempdir().unwrap();
        let root =
            crate::local_profile::MaterializationRoot::open_or_create(directory.path()).unwrap();
        let draft = intent();
        let generation = local_generation(&draft).unwrap();
        let first = open_or_create_workspace(&root, &draft, &generation).unwrap();
        root.update_profile_current_sync_base(&first, &generation, 2)
            .unwrap();
        // The browser was closed and Sync finished, but the web tab still
        // displays DRAFT v1. Do not require an extra cloud launch query.
        let advanced = open_or_create_workspace(&root, &draft, &generation).unwrap();
        assert_eq!(advanced.profile_version(), 2);
        assert_eq!(advanced.base_generation_id(), Some(&generation));
        assert_eq!(advanced.path(), first.path());

        let mut cached_ready = draft.clone();
        cached_ready.status = InitialProfileStatus::Ready;
        cached_ready.expected_version = 1;
        assert_eq!(
            recheck_workspace_after_writer_admission(&root, &advanced, &cached_ready)
                .unwrap()
                .path(),
            first.path()
        );
        // If the CRM projection is *newer* than local CURRENT, opening
        // cannot silently select old bytes instead of requiring Restore.
        cached_ready.expected_version = 3;
        assert!(matches!(
            open_or_create_workspace(&root, &cached_ready, &generation),
            Err(LocalOpenError::StaleLocalWorkspace)
        ));
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
        // After a concurrent Sync, a new Open must begin with a new exact
        // local-marker observation. Reusing the pre-Sync marker is not valid
        // even when the caller refreshed the cloud version separately.
        let refreshed = root
            .open_profile_current(&draft.tenant_id, &draft.profile_id)
            .unwrap();
        assert_eq!(
            recheck_workspace_after_writer_admission(&root, &refreshed, &ready)
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
