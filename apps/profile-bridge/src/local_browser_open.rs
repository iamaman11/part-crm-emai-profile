//! The sole normal Windows browser Open owner: local folder + native Camoufox.
//! Cloud authorization/CURRENT/R2 belong to synchronization, not this path.
#![forbid(unsafe_code)]

use crate::generation_initialization::{InitialProfileOpen, InitialProfileStatus};
use crate::local_profile::{
    BridgeWorkspaceLock, ClosedSyncAdmission, LocalProfileError, MaterializationRoot,
};
use crate::user_diagnostics::report::{self, Cause, Cloud, Outcome, Stage};
use profile_platform_primitives::{DeviceId, GenerationId};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::Stdio;
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
const CAMOUHOST_IDENTITY_EVENT_PREFIX: &str = "camouhost_identity ";
const CAMOUHOST_NATIVE_CLOSE_EVENT: &str =
    "camouhost_stage phase=close step=browser_context_closed";
const IDENTITY_ACK: &[u8] = b"camouhost-identity-persisted\n";
// Startup timeout is a diagnostic deadline, never permission to release the
// native profile admission while Python may still launch Firefox.
#[cfg(windows)]
const NATIVE_STARTUP_PROOF_TIMEOUT: Duration = Duration::from_secs(30);

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
    OperationInProgress,
    IdentityPreparation,
    BrowserLaunchFailed,
    BrowserNotClosedCleanly,
    LaunchConfirmation,
}
impl fmt::Display for LocalOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLocalWorkspace => "Local browser folder is unavailable",
            Self::RestoreRequired => "This READY profile has no local copy. Restore the confirmed current version before opening",
            Self::StaleLocalWorkspace => "Local profile identity changed during Open admission; retry without replacing its data",
            Self::RuntimeUnavailable => "Installed Camoufox runtime is unavailable",
            Self::ProfileBusy => "This local browser profile is already in use",
            Self::OperationInProgress => "Another native Open, Close or Sync operation still owns this profile",
            Self::IdentityPreparation => "Local Camoufox identity could not be prepared",
            Self::BrowserLaunchFailed => "Local Camoufox did not start; the existing profile was kept",
            Self::BrowserNotClosedCleanly => {
                "Browser close was not verified; local files were kept"
            }
            Self::LaunchConfirmation => "Browser runtime launch confirmation was not completed",
        })
    }
}
impl std::error::Error for LocalOpenError {}

// Startup failure and Close failure are different states. A launch failure
// is terminal only after Rust independently proves that no Firefox writer
// owns the local folder. A live/unknown writer remains admitted and unconfirmed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeBrowserCompletion {
    SdkClosed,
    NativeWriterReleased,
    RecoveryRequired,
    LaunchFailed,
    LaunchUnconfirmed,
    CloseUnconfirmed,
}

impl NativeBrowserCompletion {
    const fn confirms_closed_profile(self) -> bool {
        matches!(self, Self::SdkClosed | Self::NativeWriterReleased)
    }
}

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
    _intent: &InitialProfileOpen,
) -> Result<crate::local_profile::GenerationWorkspace, LocalOpenError> {
    // Existing local CURRENT is the source of truth for ordinary Open. The
    // CRM projection may be stale or newer after an interrupted Sync; that is
    // a Save/recovery concern, never a reason to block or replace local bytes.
    // The workspace marker and filesystem validation have already succeeded.
    Ok(current)
}

fn recheck_workspace_after_writer_admission(
    root: &MaterializationRoot,
    observed: &crate::local_profile::GenerationWorkspace,
    intent: &InitialProfileOpen,
) -> Result<crate::local_profile::GenerationWorkspace, LocalOpenError> {
    // Read the *current* marker while holding the exclusive workspace lock.
    // Reuse that local CURRENT directly; ordinary Open must not run cloud Sync
    // or compare its version with the CRM projection.
    let current = root
        .open_profile_current(&intent.tenant_id, &intent.profile_id)
        .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
    // The marker may legitimately advance after the preliminary lookup and
    // before this process acquires the OS admission. Reuse the freshly opened
    // local CURRENT; only a changed physical profile path is an identity error.
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
    before_launch: Option<BeforeLaunch>,
) -> Result<NativeBrowserCompletion, LocalOpenError> {
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
) -> Result<NativeBrowserCompletion, LocalOpenError> {
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

fn classify_native_browser_completion(
    action: &str,
    browser_ready: bool,
    process_success: bool,
    native_close_seen: bool,
    native_writer_released: bool,
    native_writer_absent: Option<bool>,
) -> NativeBrowserCompletion {
    if native_writer_released {
        return NativeBrowserCompletion::NativeWriterReleased;
    }
    if browser_ready {
        return if adapter_completed_after_native_close(action, process_success, native_close_seen) {
            NativeBrowserCompletion::SdkClosed
        } else {
            NativeBrowserCompletion::CloseUnconfirmed
        };
    }
    if action == "--local-open" {
        return if native_writer_absent == Some(true) {
            NativeBrowserCompletion::LaunchFailed
        } else {
            NativeBrowserCompletion::LaunchUnconfirmed
        };
    }
    NativeBrowserCompletion::CloseUnconfirmed
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
            None => child
                .wait()
                .map(|status| (status, false))
                .map_err(|_| LocalOpenError::RuntimeUnavailable),
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
    startup_proof_timeout: Duration,
    current_run_reset: &AtomicBool,
) -> Result<(std::process::ExitStatus, bool), LocalOpenError> {
    let proof_started = Instant::now();
    let mut writer_was_observed = false;
    let mut released_since: Option<Instant> = None;
    loop {
        // An OS query failure is not evidence that this child exited: wait
        // using the same owned process handle instead of detaching a launcher.
        let state = match child.try_wait() {
            Ok(state) => state,
            Err(_) => Some(
                child
                    .wait()
                    .map_err(|_| LocalOpenError::RuntimeUnavailable)?,
            ),
        };
        if let Some(status) = state {
            return Ok((status, false));
        }
        let ready = browser_ready.load(Ordering::Acquire);
        if ready {
            match crate::browser_execution::browser_writer_locks_absent_at(folder) {
                Ok(false) => {
                    writer_was_observed = true;
                    // Only Rust observes Firefox's existing on-disk checkpoint,
                    // while the actual Windows native writer is still active.
                    if crate::close_witness::live_run_checkpoint_reset(folder) {
                        current_run_reset.store(true, Ordering::Release);
                    }
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
                // An unreadable lock cannot certify native release. Continue
                // owning the profile while Python might still launch Firefox.
                Err(_) => released_since = None,
                _ => released_since = None,
            }
        }
        // The child inherited its OWN Windows share-deny launch admission at
        // CreateProcess. It survives parent death and prevents another
        // Open/Sync/Restore. Only with this independent OS witness can the
        // foreground request return promptly while startup remains uncertain.
        if (!ready || !writer_was_observed) && proof_started.elapsed() >= startup_proof_timeout {
            eprintln!(
                "BRIDGE_LOCAL_STARTUP_UNCONFIRMED;NEXT_SAFE_STEP=RETAIN_NATIVE_PROFILE_OWNER"
            );
            // Report a pending, not a failed, native startup. This is
            // observable diagnostics only: it cannot release either OS lock
            // or authorize a cloud Save while the adapter may still launch.
            report::record_current(
                Stage::CamouhostLaunch,
                Outcome::Unconfirmed,
                Cloud::Unknown,
                Cause::RuntimeTimeout,
                u64::try_from(proof_started.elapsed().as_millis())
                    .unwrap_or(u64::MAX)
                    .min(86_400_000),
            );
            return Err(LocalOpenError::LaunchConfirmation);
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
) -> Result<NativeBrowserCompletion, LocalOpenError> {
    // Delivery health is supplied by the composition owner as a one-shot
    // callback. Local Open knows only when the native runtime is launched and
    // when the official Playwright Ready event arrives.
    let mut health_confirmation = before_launch
        .map(|before| before().map_err(|_| LocalOpenError::LaunchConfirmation))
        .transpose()?
        .flatten();
    let health_required = health_confirmation.is_some();
    let (health_result_tx, health_result_rx) = std::sync::mpsc::channel();
    // First DRAFT identity uses the same bounded diagnostic pipe, not
    // stdout: stdout is an inherited kernel-only launch gate on Windows.
    let (identity_tx, identity_rx) = std::sync::mpsc::sync_channel(1);
    let native_close_seen = Arc::new(AtomicBool::new(false));
    let close_seen_by_reader = Arc::clone(&native_close_seen);
    let native_ready_seen = Arc::new(AtomicBool::new(false));
    let ready_seen_by_reader = Arc::clone(&native_ready_seen);

    let native_started = Instant::now();
    let native_started_wall = SystemTime::now();
    let current_run_reset = AtomicBool::new(false);
    let mut child = crate::windows_native::background_command(executable)
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
        .stdout({
            #[cfg(windows)]
            {
                crate::local_profile::native_child_launch_guard_stdout(folder)
                    .map_err(|_| LocalOpenError::RuntimeUnavailable)?
            }
            #[cfg(not(windows))]
            {
                Stdio::null()
            }
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
            // A partial request might already have reached the launcher.
            // Do not detach or kill an adapter that could own a live browser.
            drop(stdin);
            child
                .wait()
                .map_err(|_| LocalOpenError::RuntimeUnavailable)?;
            return Err(LocalOpenError::RuntimeUnavailable);
        }
    }
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            drop(stdin);
            child
                .wait()
                .map_err(|_| LocalOpenError::RuntimeUnavailable)?;
            return Err(LocalOpenError::RuntimeUnavailable);
        }
    };
    let diagnostics = std::thread::spawn(move || {
        let mut native_ready_observed = false;
        let mut first_identity_sender = Some(identity_tx);
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if generate_identity {
                if let Some(payload) = line.strip_prefix(CAMOUHOST_IDENTITY_EVENT_PREFIX) {
                    if let Some(sender) = first_identity_sender.take() {
                        // Enforce the old stdout IPC size bound on stderr.
                        if payload.len() <= (MAX_CONFIG_BYTES * 8) as usize {
                            let mut message = payload.as_bytes().to_vec();
                            message.push(b'\n');
                            let _ = sender.send(message);
                        }
                    }
                    continue;
                }
            }
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
            let line = identity_rx
                .recv_timeout(Duration::from_secs(120))
                .map_err(|_| LocalOpenError::IdentityPreparation)?;
            persist_sdk_identity(folder, &line)?;
            stdin
                .as_mut()
                .ok_or(LocalOpenError::IdentityPreparation)?
                .write_all(IDENTITY_ACK)
                .map_err(|_| LocalOpenError::IdentityPreparation)
        })();
        if let Err(error) = persisted {
            drop(stdin);
            // The independent inherited Windows gate survives this error.
            // Without our exact ACK, this Camouhost never reaches SDK launch.
            return Err(error);
        }
    }
    drop(stdin);
    #[cfg(windows)]
    let (completed, native_adapter_released) = if action == "--local-open" {
        await_adapter_or_native_release(
            &mut child,
            folder,
            &native_ready_seen,
            NATIVE_STARTUP_PROOF_TIMEOUT,
            &current_run_reset,
        )?
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
    let browser_ready = native_ready_seen.load(Ordering::Acquire);
    let native_writer_absent = if action == "--local-open" && !browser_ready {
        Some(matches!(
            crate::browser_execution::browser_writer_locks_absent_at(folder),
            Ok(true)
        ))
    } else {
        None
    };
    let completion = classify_native_browser_completion(
        action,
        browser_ready,
        completed.success(),
        native_close_seen.load(Ordering::Acquire),
        native_adapter_released,
        native_writer_absent,
    );
    if action != "--local-open"
        || !matches!(
            completion,
            NativeBrowserCompletion::SdkClosed | NativeBrowserCompletion::NativeWriterReleased
        )
    {
        return Ok(completion);
    }
    // A crashed Firefox produces the same SDK close event, Python exit=0,
    // and released parent.lock as a graceful user Close. The sole Rust owner
    // must independently prove an actual Firefox final state from THIS run.
    let writer_released = (0..20).any(|attempt| {
        match crate::browser_execution::browser_writer_locks_absent_at(folder) {
            Ok(true) => true,
            _ if attempt < 19 => {
                std::thread::sleep(Duration::from_millis(50));
                false
            }
            _ => false,
        }
    });
    if !writer_released {
        return Ok(NativeBrowserCompletion::CloseUnconfirmed);
    }
    Ok(
        match crate::close_witness::verify_final_close(
            folder,
            native_started_wall,
            current_run_reset.load(Ordering::Acquire),
        ) {
            crate::close_witness::CloseWitness::Clean => completion,
            crate::close_witness::CloseWitness::RecoveryRequired => {
                NativeBrowserCompletion::RecoveryRequired
            }
            crate::close_witness::CloseWitness::Unconfirmed => {
                NativeBrowserCompletion::CloseUnconfirmed
            }
        },
    )
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
    open_with_before_launch_and_after_close(
        root,
        intent,
        device,
        runtime_root,
        before_launch,
        |_| {},
    )
}

/// Retain the SAME native admission while the caller finishes the post-Close
/// Sync. No OS lock handoff, second Sync process or browser manager is needed.
/// This callback must not report a Save receipt until D1 confirms CURRENT.
pub(crate) fn open_with_before_launch_and_after_close(
    root: &MaterializationRoot,
    intent: &InitialProfileOpen,
    device: &DeviceId,
    runtime_root: &Path,
    before_launch: Option<BeforeLaunch>,
    after_close: impl FnOnce(Result<(), LocalOpenError>),
) -> Result<(), LocalOpenError> {
    let local_started = Instant::now();
    let generation = local_generation(intent)?;
    let workspace = open_or_create_workspace(root, intent, &generation)?;
    // Keep the same profile-scoped OS gate from browser Open through native
    // Close. A previous post-close Sync cannot run concurrently with a new
    // Firefox session and commit a snapshot of changing browser bytes.
    // Hold this guard through the inline post-Close Sync callback. A new
    // Open cannot win between Firefox releasing its files and cloud Commit.
    let _operation = match ClosedSyncAdmission::try_acquire(&workspace) {
        Ok(Some(owner)) => owner,
        Ok(None) => return Err(LocalOpenError::OperationInProgress),
        Err(_) => return Err(LocalOpenError::InvalidLocalWorkspace),
    };
    let result = open_under_profile_admission(
        root,
        intent,
        device,
        runtime_root,
        before_launch,
        workspace,
        local_started,
    );
    after_close(result);
    result
}

fn open_under_profile_admission(
    root: &MaterializationRoot,
    intent: &InitialProfileOpen,
    device: &DeviceId,
    runtime_root: &Path,
    before_launch: Option<BeforeLaunch>,
    workspace: crate::local_profile::GenerationWorkspace,
    local_started: Instant,
) -> Result<(), LocalOpenError> {
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
    // Persist dirty evidence before Camouhost can launch Firefox. A failed
    // launch keeps this marker; only the existing verified Sync may retire it.
    if let Err(error) = mark_local_unsynced(workspace.path()) {
        if matches!(
            crate::browser_execution::browser_writer_locks_absent(&workspace),
            Ok(true)
        ) {
            writer
                .release()
                .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
        }
        return Err(error);
    }
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
    let completion = match run_python(
        runtime_root,
        workspace.path(),
        "--local-open",
        before_launch,
    ) {
        Ok(completion) => completion,
        Err(error) => {
            // Pre-launch failures must not strand the durable Bridge writer.
            // Release it only after the same independent native-lock proof
            // used by the successful Close path; unknown/live writers retain it.
            if matches!(
                crate::browser_execution::browser_writer_locks_absent(&workspace),
                Ok(true)
            ) {
                writer
                    .release()
                    .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
            }
            return Err(error);
        }
    };
    match completion {
        NativeBrowserCompletion::LaunchFailed => {
            // The child exited before BrowserReady. Release the Bridge writer
            // only after a second independent native-lock absence observation.
            if !matches!(
                crate::browser_execution::browser_writer_locks_absent(&workspace),
                Ok(true)
            ) {
                return Err(LocalOpenError::LaunchConfirmation);
            }
            writer
                .release()
                .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
            return Err(LocalOpenError::BrowserLaunchFailed);
        }
        NativeBrowserCompletion::LaunchUnconfirmed => {
            // Keep the durable writer admission: Firefox may still own bytes.
            return Err(LocalOpenError::LaunchConfirmation);
        }
        NativeBrowserCompletion::CloseUnconfirmed | NativeBrowserCompletion::RecoveryRequired => {
            // A crashed Firefox has genuinely released its kernel file handle.
            // Retain every original browser byte and .local-unsynced marker,
            // but free the Bridge admission to permit the SAME profile to
            // reopen/recover without a machine restart or manual deletion.
            if matches!(
                crate::browser_execution::browser_writer_locks_absent(&workspace),
                Ok(true)
            ) {
                writer
                    .release()
                    .map_err(|_| LocalOpenError::InvalidLocalWorkspace)?;
            }
            return Err(LocalOpenError::BrowserNotClosedCleanly);
        }
        NativeBrowserCompletion::SdkClosed | NativeBrowserCompletion::NativeWriterReleased => {}
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
    fn launch_and_close_failures_are_distinct_native_states() {
        use super::{NativeBrowserCompletion as Completion, classify_native_browser_completion};

        assert_eq!(
            classify_native_browser_completion(
                "--local-open",
                false,
                true,
                false,
                false,
                Some(true)
            ),
            Completion::LaunchFailed
        );
        assert_eq!(
            classify_native_browser_completion(
                "--local-open",
                false,
                false,
                false,
                false,
                Some(false)
            ),
            Completion::LaunchUnconfirmed
        );
        assert_eq!(
            classify_native_browser_completion("--local-open", false, false, false, false, None),
            Completion::LaunchUnconfirmed
        );
        assert_eq!(
            classify_native_browser_completion("--local-open", true, false, false, false, None),
            Completion::CloseUnconfirmed
        );
        assert_eq!(
            classify_native_browser_completion("--local-open", true, true, false, false, None),
            Completion::SdkClosed
        );
        assert_eq!(
            classify_native_browser_completion("--local-open", true, false, false, true, None),
            Completion::NativeWriterReleased
        );
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
        if let Some(root) = std::env::var_os("PROFILE_BRIDGE_TEST_CHILD_LATE_WRITER_ROOT") {
            // A separate native Windows process acquires the real Firefox
            // parent.lock AFTER the initial startup-proof diagnostic deadline.
            std::thread::sleep(std::time::Duration::from_millis(250));
            let writer = hold_native_writer_lock(std::path::Path::new(&root))
                .expect("late native Firefox writer must acquire the OS handle");
            std::thread::sleep(std::time::Duration::from_millis(800));
            drop(writer);
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
        let (status, adapter_stopped) = super::await_adapter_or_native_release(
            &mut child,
            root.path(),
            &ready,
            Duration::from_secs(30),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
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
        use std::time::Duration;

        let root = tempfile::tempdir()?;
        let writer = hold_native_writer_lock(root.path())?;
        let mut child = spawn_native_adapter_fixture(400)?;
        let ready = AtomicBool::new(true);
        let (status, stopped) = super::await_adapter_or_native_release(
            &mut child,
            root.path(),
            &ready,
            Duration::from_secs(30),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
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
        let (status, stopped) = super::await_adapter_or_native_release(
            &mut child,
            root.path(),
            &ready,
            Duration::from_secs(30),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
        assert!(status.success());
        assert!(!stopped, "missing READY must not authorize stop");
        drop(writer);

        let mut child = spawn_native_adapter_fixture(400)?;
        let ready = AtomicBool::new(true);
        let (status, stopped) = super::await_adapter_or_native_release(
            &mut child,
            root.path(),
            &ready,
            Duration::from_secs(30),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
        assert!(status.success());
        assert!(!stopped, "unobserved writer must not authorize stop");
        Ok(())
    }

    // Physical OS-kill test: native Rust Bridge owns a child's inherited
    // share-deny handle, then is forcibly terminated BEFORE Firefox acquires
    // its parent.lock. The grandchild must still block Open/Sync/Restore.
    #[cfg(windows)]
    #[test]
    fn native_launch_parent_crash_fixture() {
        let Some(root) = std::env::var_os("PROFILE_BRIDGE_TEST_NATIVE_PARENT_CRASH_ROOT") else {
            return;
        };
        use std::process::{Command, Stdio};
        let path = std::path::Path::new(&root);
        let launch_guard = crate::local_profile::native_child_launch_guard_stdout(path)
            .expect("acquire native share-deny guard");
        let _child = Command::new(std::env::current_exe().expect("test executable"))
            .arg("native_adapter_child_fixture")
            .env("PROFILE_BRIDGE_TEST_CHILD_SLEEP_MS", "6000")
            .stdin(Stdio::null())
            .stdout(launch_guard)
            .stderr(Stdio::null())
            .spawn()
            .expect("launch isolated Python stand-in holding inherited OS guard");
        std::fs::write(
            path.parent()
                .expect("profile folder")
                .join(".native-parent-ready"),
            b"child_launched",
        )
        .expect("published parent fixture readiness");
        std::thread::sleep(std::time::Duration::from_secs(15));
    }

    #[cfg(windows)]
    #[test]
    fn os_kill_bridge_parent_keeps_native_child_launch_gate_until_child_exits()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::local_profile::{
            BridgeWorkspaceLock, ClosedSyncAdmission, DeliveryActivationGuard, MaterializationRoot,
        };
        use profile_platform_primitives::DeviceId;
        use std::fs;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let intent = intent();
        let workspace = open_or_create_workspace(&root, &intent, &local_generation(&intent)?)?;
        let handshake = workspace
            .path()
            .parent()
            .ok_or("profile parent directory")?
            .join(".native-parent-ready");

        let mut owner = Command::new(std::env::current_exe()?)
            .arg("native_launch_parent_crash_fixture")
            .env(
                "PROFILE_BRIDGE_TEST_NATIVE_PARENT_CRASH_ROOT",
                workspace.path(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let started = Instant::now();
        while !handshake.exists() && started.elapsed() < Duration::from_secs(6) {
            std::thread::sleep(Duration::from_millis(25));
        }
        if !handshake.exists() {
            let _ = owner.kill();
            let _ = owner.wait();
            return Err("native test child failed to inherit its OS guard".into());
        }
        // Actual TerminateProcess via std::process::Child::kill, not an
        // in-process simulated drop or mocked file-lock predicate.
        owner.kill()?;
        owner.wait()?;
        assert!(
            crate::browser_execution::browser_writer_locks_absent_at(workspace.path())?,
            "Firefox writer is not yet present; the inherited guard is decisive"
        );
        assert!(
            ClosedSyncAdmission::try_acquire(&workspace)?.is_none(),
            "Rust parent is dead but native child must still exclude new Open/Sync"
        );
        let device = DeviceId::parse("device_native_os_kill")?;
        assert!(matches!(
            BridgeWorkspaceLock::acquire_or_recover_interrupted(&workspace, &device, 1),
            Err(crate::local_profile::LocalProfileError::LockBusy)
        ));
        assert!(
            DeliveryActivationGuard::acquire(&root).is_err(),
            "installer or local Delete cannot bypass a live orphan launch"
        );

        // The kernel share-deny handle vanishes when the child exits; neither
        // PID polling nor a new persistent state machine is used to recover.
        let deadline = Instant::now() + Duration::from_secs(12);
        let _recovered = loop {
            if let Some(admission) = ClosedSyncAdmission::try_acquire(&workspace)? {
                break admission;
            }
            if Instant::now() >= deadline {
                return Err("OS gate did not release when the native child exited".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        fs::remove_file(handshake)?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn lost_sdk_ready_after_native_writer_observation_is_bounded()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::AtomicBool;
        use std::time::{Duration, Instant};

        let directory = tempfile::tempdir()?;
        let writer = hold_native_writer_lock(directory.path())?;
        let mut child = spawn_native_adapter_fixture(1500)?;
        let ready = AtomicBool::new(false);
        let started = Instant::now();
        let outcome = super::await_adapter_or_native_release(
            &mut child,
            directory.path(),
            &ready,
            Duration::from_millis(50),
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert_eq!(outcome, Err(LocalOpenError::LaunchConfirmation));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the missing SDK Ready must not wait for an active native writer forever"
        );
        assert!(
            child.try_wait()?.is_none(),
            "missing Ready cannot authorize killing an active browser"
        );
        // The synthetic child does not inherit the shipping Windows
        // share-deny gate; terminate only this deliberately isolated fixture.
        child.kill()?;
        child.wait()?;
        drop(writer);
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn startup_diagnostic_deadline_never_detaches_an_unconfirmed_adapter()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::AtomicBool;
        use std::time::{Duration, Instant};

        let root = tempfile::tempdir()?;
        for ready_seen in [false, true] {
            let mut child = spawn_native_adapter_fixture(600)?;
            let ready = AtomicBool::new(ready_seen);
            let started = Instant::now();
            let outcome = super::await_adapter_or_native_release(
                &mut child,
                root.path(),
                &ready,
                Duration::from_millis(50),
                &std::sync::atomic::AtomicBool::new(false),
            );
            assert_eq!(outcome, Err(LocalOpenError::LaunchConfirmation));
            assert!(
                started.elapsed() < Duration::from_millis(450),
                "the foreground owner must return after bounded launch uncertainty"
            );
            assert!(
                child.try_wait()?.is_none(),
                "unconfirmed startup must not force-terminate the adapter"
            );
            // This isolated fixture has no inherited shipping gate; explicitly
            // clean up only its own test process after the assertion.
            child.kill()?;
            child.wait()?;
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn late_firefox_writer_cannot_start_after_native_profile_admission_is_released()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::local_profile::{ClosedSyncAdmission, MaterializationRoot};
        use std::process::{Command, Stdio};
        use std::sync::atomic::AtomicBool;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let intent = intent();
        let workspace = open_or_create_workspace(&root, &intent, &local_generation(&intent)?)?;
        let owner_workspace = workspace.clone();
        let (admitted_tx, admitted_rx) = mpsc::channel();
        let owner = std::thread::spawn(move || {
            // This is the same kernel admission held across ordinary Open and
            // its post-Close callback in the shipping path.
            let _admission = ClosedSyncAdmission::try_acquire(&owner_workspace)
                .expect("kernel admission")
                .expect("exclusive native profile owner");
            admitted_tx.send(()).expect("admitted signal");
            let native_child_guard =
                crate::local_profile::native_child_launch_guard_stdout(owner_workspace.path())
                    .expect("inherited Windows launch admission");
            let mut child = Command::new(std::env::current_exe().expect("test executable"))
                .arg("native_adapter_child_fixture")
                .env(
                    "PROFILE_BRIDGE_TEST_CHILD_LATE_WRITER_ROOT",
                    owner_workspace.path(),
                )
                .stdin(Stdio::null())
                .stdout(native_child_guard)
                .stderr(Stdio::null())
                .spawn()
                .expect("real Windows child");
            let ready = AtomicBool::new(false);
            let outcome = super::await_adapter_or_native_release(
                &mut child,
                owner_workspace.path(),
                &ready,
                Duration::from_millis(50),
                &std::sync::atomic::AtomicBool::new(false),
            );
            // Rust deliberately releases the profile admission on timeout.
            // The child, not this parent thread, retains kernel ownership.
            (outcome, child)
        });
        admitted_rx.recv_timeout(Duration::from_secs(5))?;
        let (outcome, mut child) = owner.join().expect("native owner thread");
        assert_eq!(outcome, Err(LocalOpenError::LaunchConfirmation));
        assert!(
            ClosedSyncAdmission::try_acquire(&workspace)?.is_none(),
            "child-inherited OS guard must exclude Open/Sync after Rust returns"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut late_writer_observed = false;
        while Instant::now() < deadline {
            if !crate::browser_execution::browser_writer_locks_absent_at(workspace.path())? {
                late_writer_observed = true;
                assert!(
                    ClosedSyncAdmission::try_acquire(&workspace)?.is_none(),
                    "a late Firefox writer must never race a second Open or Sync"
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            late_writer_observed,
            "child did not acquire the real native lock"
        );
        assert!(
            child.wait()?.success(),
            "late Firefox writer must exit naturally without Bridge termination"
        );
        assert!(crate::browser_execution::browser_writer_locks_absent_at(
            workspace.path()
        )?);
        let _next = ClosedSyncAdmission::try_acquire(&workspace)?
            .ok_or("owner did not release after child exit")?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires PROFILE_BRIDGE_TEST_PYTHON"]
    fn successful_python_exit_without_sdk_ready_never_confirms_close()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use std::path::PathBuf;

        let python = std::env::var_os("PROFILE_BRIDGE_TEST_PYTHON")
            .map(PathBuf::from)
            .ok_or("PROFILE_BRIDGE_TEST_PYTHON is required")?;
        let directory = tempfile::tempdir()?;
        let runtime_root = directory.path().join("runtime");
        fs::create_dir_all(runtime_root.join("camouhost"))?;
        fs::write(runtime_root.join(ENTRYPOINT), b"import sys\nsys.exit(0)\n")?;
        let folder = directory.path().join("profile");
        fs::create_dir(&folder)?;
        fs::create_dir(folder.join("user_data"))?;
        assert_eq!(
            run_python_with_executable(&python, &runtime_root, &folder, "--local-open", None)?,
            super::NativeBrowserCompletion::LaunchFailed,
            "process exit before BrowserReady is a launch failure, not a Close failure"
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires PROFILE_BRIDGE_TEST_PYTHON"]
    fn windows_python_missing_playwright_close_event_uses_native_release_gate()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use std::path::PathBuf;
        use std::time::{Duration, Instant};

        let python = std::env::var_os("PROFILE_BRIDGE_TEST_PYTHON")
            .map(PathBuf::from)
            .ok_or("PROFILE_BRIDGE_TEST_PYTHON is required")?;
        let directory = tempfile::tempdir()?;
        let runtime_root = directory.path().join("runtime");
        fs::create_dir_all(runtime_root.join("camouhost"))?;
        fs::write(
            runtime_root.join(ENTRYPOINT),
            format!(
                r#"import sys
import time
from pathlib import Path
root = Path(sys.argv[2])
lock = root / 'user_data' / 'parent.lock'
lock.parent.mkdir(parents=True, exist_ok=True)
with lock.open('ab', buffering=0):
    sys.stderr.write({:?} + '\n')
    sys.stderr.flush()
    time.sleep(1.0)
# Deliberately emit no Playwright BrowserContext.close event.
# The driver process remains stuck after the native writer is gone.
time.sleep(15)
(root / 'adapter_was_not_stopped').write_text('unsafe')
"#,
                CAMOUHOST_LAUNCH_READY_EVENT
            ),
        )?;
        fs::write(runtime_root.join(LOCK), b"test-runtime-lock")?;
        let folder = directory.path().join("profile");
        fs::create_dir(&folder)?;
        let started = Instant::now();
        assert_eq!(
            run_python_with_executable(&python, &runtime_root, &folder, "--local-open", None,)?,
            super::NativeBrowserCompletion::NativeWriterReleased
        );
        assert!(started.elapsed() >= Duration::from_secs(2));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!folder.join("adapter_was_not_stopped").exists());
        assert!(crate::browser_execution::browser_writer_locks_absent_at(
            &folder
        )?);
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
        assert!(
            run_python_with_executable(
                &python,
                &runtime_root,
                &folder,
                "--local-open",
                Some(before_launch),
            )?
            .confirms_closed_profile()
        );
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
    fn open_rejects_live_closed_sync_before_touching_browser_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::local_profile::ClosedSyncAdmission;
        use profile_platform_primitives::DeviceId;

        let directory = tempfile::tempdir()?;
        let root = crate::local_profile::MaterializationRoot::open_or_create(directory.path())?;
        let intent = intent();
        let generation = local_generation(&intent)?;
        let current = open_or_create_workspace(&root, &intent, &generation)?;
        let _sync = ClosedSyncAdmission::try_acquire(&current)?
            .ok_or("native Sync admission not acquired for test")?;
        let device = DeviceId::parse("device_local_sync_open")?;
        assert_eq!(
            super::open_with_before_launch(&root, &intent, &device, directory.path(), None),
            Err(LocalOpenError::OperationInProgress),
        );
        assert!(
            !current.path().join(".local-unsynced").exists(),
            "an Open refused by active Sync must never dirty native browser bytes"
        );
        assert!(!current.path().join("camoufox-config.json").exists());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn post_close_sync_callback_cannot_lose_profile_gate() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::local_profile::{ClosedSyncAdmission, MaterializationRoot};
        use profile_platform_primitives::DeviceId;

        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let intent = intent();
        let device = DeviceId::parse("device_local_sync_open")?;
        let mut callback_ran = false;

        // The fixture has no packaged Python. Browser initialization fails
        // safely, but the same close/recovery callback still holds the native
        // gate. Another Open cannot claim this profile in that interval.
        let result = super::open_with_before_launch_and_after_close(
            &root,
            &intent,
            &device,
            directory.path(),
            None,
            |open_result| {
                assert_eq!(open_result, Err(LocalOpenError::RuntimeUnavailable));
                let workspace = root
                    .open_profile_current(&intent.tenant_id, &intent.profile_id)
                    .expect("existing local profile");
                assert!(
                    ClosedSyncAdmission::try_acquire(&workspace)
                        .expect("kernel admission")
                        .is_none(),
                    "callback must execute before the profile admission is released"
                );
                callback_ran = true;
            },
        );
        assert_eq!(result, Err(LocalOpenError::RuntimeUnavailable));
        assert!(callback_ran);
        let workspace = root.open_profile_current(&intent.tenant_id, &intent.profile_id)?;
        let _after = ClosedSyncAdmission::try_acquire(&workspace)?
            .ok_or("the same profile gate did not release after callback")?;
        assert!(workspace.path().join(".local-unsynced").exists());
        Ok(())
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
        root.update_profile_current_sync_base(&first, &generation, 11)
            .unwrap();
        std::fs::write(first.path().join(".local-unsynced"), b"local-unsynced-v2").unwrap();
        // Local v11 is a dirty working copy while the CRM projection has
        // advanced to v12. Open must use the local marker without cloud I/O.
        let advanced = open_or_create_workspace(&root, &draft, &generation).unwrap();
        assert_eq!(advanced.profile_version(), 11);
        assert_eq!(advanced.base_generation_id(), Some(&generation));
        assert_eq!(advanced.path(), first.path());

        let mut cached_ready = draft.clone();
        cached_ready.status = InitialProfileStatus::Ready;
        cached_ready.expected_version = 12;
        assert_eq!(
            recheck_workspace_after_writer_admission(&root, &advanced, &cached_ready)
                .unwrap()
                .path(),
            first.path()
        );
        // A newer CRM projection does not block an existing local copy.
        // Local Open stays offline and never replaces the local working folder.
        let reopened = open_or_create_workspace(&root, &cached_ready, &generation).unwrap();
        assert_eq!(reopened.profile_version(), 11);
        assert_eq!(reopened.path(), first.path());
        assert_eq!(
            std::fs::read(reopened.path().join(".local-unsynced")).unwrap(),
            b"local-unsynced-v2"
        );
    }

    #[test]
    fn recheck_uses_the_latest_local_marker_after_a_completed_sync() {
        let directory = tempfile::tempdir().unwrap();
        let root =
            crate::local_profile::MaterializationRoot::open_or_create(directory.path()).unwrap();
        let draft = intent();
        let generation = local_generation(&draft).unwrap();
        let observed = open_or_create_workspace(&root, &draft, &generation).unwrap();
        // Simulate a completed Sync between preliminary lookup and OS admission.
        root.update_profile_current_sync_base(&observed, &generation, 2)
            .unwrap();
        let current = recheck_workspace_after_writer_admission(&root, &observed, &draft).unwrap();
        assert_eq!(current.path(), observed.path());
        assert_eq!(current.profile_version(), 2);
        assert_eq!(current.base_generation_id(), Some(&generation));
    }

    #[test]
    fn existing_local_workspace_opens_despite_stale_projection_and_keeps_dirty_bytes() {
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
        // The UI projection may say READY v2 while the local DRAFT is still
        // v1 and has unsynced browser data. Open must reuse, not reject/reset it.
        let opened = open_or_create_workspace(&root, &advanced, &generation).unwrap();
        assert_eq!(opened.path(), workspace.path());
        assert_eq!(opened.profile_version(), draft.expected_version);
        assert_eq!(
            std::fs::read(workspace.path().join(".local-unsynced")).unwrap(),
            b"local-unsynced-v2"
        );
        // Even a newer version projection cannot replace the same local folder.
        advanced.expected_version = 99;
        let opened_again = open_or_create_workspace(&root, &advanced, &generation).unwrap();
        assert_eq!(opened_again.path(), workspace.path());
        assert_eq!(
            std::fs::read(workspace.path().join(".local-unsynced")).unwrap(),
            b"local-unsynced-v2"
        );
    }
}
