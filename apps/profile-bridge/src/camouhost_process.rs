mod browser_visible_wire;

use self::browser_visible_wire::{HostRuntimeEvidence, verify_browser_visible_payload};
use crate::ProcessControlPort;
use crate::browser_execution::BrowserLaunchBlocker;
use crate::browser_preflight::{BoundBrowserLaunchPreflight, BrowserRuntimeObservationPort};
use crate::local_profile::GenerationWorkspace;
use crate::operator_flow::BrowserLaunchPreflightPort;
use crate::runtime_bundle::ApprovedRuntimeBundle;
use bridge_domain::{BridgePortError, CamouhostMessage, CamouhostPort};
use browser_execution_domain::host_compatibility::{
    HostArchitecture, HostCompatibilityDecision, HostCompatibilityObservation,
    HostCompatibilityPolicy, HostExecutionMode, HostPlatformClass, HostRuntimeClass,
};
use browser_execution_domain::{
    MaterializationBinding, NetworkIdentityPolicy, ProfileStableIdentity,
};
use profile_platform_primitives::{DeviceId, SessionId};
use runtime_bundle_domain::BundleRelativePath;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, RecvTimeoutError},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CONFIG_NAME: &str = "camoufox-config.json";
const REAL_ENTRYPOINT: &str = "camouhost/real.py";
const RUNTIME_LOCK_PATH: &str = "camouhost/runtime-lock.json";
#[cfg(windows)]
const WINDOWS_BROWSER_EXECUTABLE: &str = "browser/camoufox.exe";
#[cfg(windows)]
const WINDOWS_PYTHON_EXECUTABLE: &str = "python/python.exe";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_IPC_REQUEST_BYTES: usize = 8 * 1024;
const MAX_IPC_RESPONSE_BYTES: usize = 1_100_001;
#[cfg(windows)]
const HELLO_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(not(windows))]
const HELLO_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const LAUNCH_RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
const BROWSER_VISIBLE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const NAVIGATION_RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
const OBSERVE_CLOSE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESS_EXIT_TIMEOUT: Duration = Duration::from_secs(15);
const FORCE_EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
const IDENTITY_COMPATIBILITY_VERSION: u32 = 2;
const FINGERPRINT_SOURCE_PREFIX: &str = "profile-stability-v1-probe-";
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone)]
pub struct RuntimeBindingSlot {
    inner: Arc<Mutex<Option<RuntimeLaunchBinding>>>,
}

impl RuntimeBindingSlot {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    fn publish(&self, binding: RuntimeLaunchBinding) -> Result<(), BrowserLaunchBlocker> {
        let mut slot = self
            .inner
            .lock()
            .map_err(|_| BrowserLaunchBlocker::RecoveryRequired)?;
        if slot.is_some() {
            return Err(BrowserLaunchBlocker::RecoveryRequired);
        }
        *slot = Some(binding);
        Ok(())
    }

    fn take(&self) -> Result<RuntimeLaunchBinding, BridgePortError> {
        self.inner
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?
            .take()
            .ok_or(BridgePortError::InvalidResponse)
    }
}

impl Default for RuntimeBindingSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct RuntimeLaunchBinding {
    profile_root: PathBuf,
    runtime_lock_sha256: String,
    fingerprint_config_sha256: String,
    profile_stable_probe_sha256: String,
    profile_stable_identity: ProfileStableIdentity,
    entrypoint_sha256: String,
    #[cfg(windows)]
    browser_sha256: String,
    #[cfg(windows)]
    python_sha256: String,
}

/// Wraps the already accepted browser preflight and publishes one launch capability only after
/// generation, writer, network and runtime identity have all been verified.
pub struct RuntimeBindingBrowserLaunchPreflight<O> {
    inner: BoundBrowserLaunchPreflight<O>,
    expected: MaterializationBinding,
    slot: RuntimeBindingSlot,
}

impl<O> RuntimeBindingBrowserLaunchPreflight<O> {
    #[must_use]
    pub fn new(
        expected: MaterializationBinding,
        network_policy: NetworkIdentityPolicy,
        observations: O,
        slot: RuntimeBindingSlot,
    ) -> Self {
        Self {
            inner: BoundBrowserLaunchPreflight::new(expected.clone(), network_policy, observations),
            expected,
            slot,
        }
    }
}

impl<O> BrowserLaunchPreflightPort for RuntimeBindingBrowserLaunchPreflight<O>
where
    O: BrowserRuntimeObservationPort,
{
    type Error = BrowserLaunchBlocker;

    fn evaluate_before_launch(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        workspace_epoch: u64,
        runtime_bundle: &ApprovedRuntimeBundle,
    ) -> Result<(), Self::Error> {
        self.inner
            .evaluate_before_launch(workspace, device_id, workspace_epoch, runtime_bundle)?;
        let binding = validate_runtime_identity(workspace, &self.expected, runtime_bundle)?;
        self.slot.publish(binding)
    }
}

fn validate_runtime_identity(
    workspace: &GenerationWorkspace,
    expected: &MaterializationBinding,
    runtime_bundle: &ApprovedRuntimeBundle,
) -> Result<RuntimeLaunchBinding, BrowserLaunchBlocker> {
    let browser_identity = expected.browser_identity();
    if browser_identity.compatibility_version() != IDENTITY_COMPATIBILITY_VERSION {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }

    let probe_sha256 = browser_identity
        .fingerprint_source()
        .strip_prefix(FINGERPRINT_SOURCE_PREFIX)
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if !valid_sha256(probe_sha256) {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }

    let config_bytes = read_regular_bounded(&workspace.path().join(CONFIG_NAME), MAX_CONFIG_BYTES)?;
    let config_sha256 = sha256_hex(&config_bytes);
    if config_sha256 != browser_identity.fingerprint_config_sha256() {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }

    let runtime_lock_sha256 = inventory_digest(runtime_bundle, RUNTIME_LOCK_PATH)?.to_owned();
    if runtime_bundle.manifest().entrypoint().as_str() != REAL_ENTRYPOINT {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let entrypoint_sha256 = inventory_digest(runtime_bundle, REAL_ENTRYPOINT)?.to_owned();
    #[cfg(windows)]
    let browser_sha256 = inventory_digest(runtime_bundle, WINDOWS_BROWSER_EXECUTABLE)?.to_owned();
    #[cfg(windows)]
    let python_sha256 = inventory_digest(runtime_bundle, WINDOWS_PYTHON_EXECUTABLE)?.to_owned();

    Ok(RuntimeLaunchBinding {
        profile_root: workspace.path().to_path_buf(),
        runtime_lock_sha256,
        fingerprint_config_sha256: config_sha256,
        profile_stable_probe_sha256: probe_sha256.to_owned(),
        profile_stable_identity: browser_identity.profile_stable_identity().clone(),
        entrypoint_sha256,
        #[cfg(windows)]
        browser_sha256,
        #[cfg(windows)]
        python_sha256,
    })
}

fn inventory_digest<'a>(
    bundle: &'a ApprovedRuntimeBundle,
    relative: &str,
) -> Result<&'a str, BrowserLaunchBlocker> {
    let path = BundleRelativePath::parse(relative)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    bundle
        .inventory()
        .entries()
        .iter()
        .find(|entry| entry.path() == &path)
        .map(|entry| entry.sha256().as_str())
        .ok_or(BrowserLaunchBlocker::MaterializationStale)
}

fn read_regular_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, BrowserLaunchBlocker> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > maximum {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    fs::read(path).map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeDisplayMode {
    Headful,
    VirtualHeadful,
}

#[derive(Clone)]
pub struct ManagedCamouhostConfig {
    python_executable: PathBuf,
    runtime_root: PathBuf,
    display_mode: RuntimeDisplayMode,
    initial_url: Option<String>,
    proxy_config_path: Option<PathBuf>,
}

impl ManagedCamouhostConfig {
    pub fn new(
        python_executable: PathBuf,
        runtime_root: PathBuf,
        display_mode: RuntimeDisplayMode,
        initial_url: Option<String>,
        proxy_config_path: Option<PathBuf>,
    ) -> Result<Self, BridgePortError> {
        if !python_executable.is_absolute() || !runtime_root.is_absolute() {
            return Err(BridgePortError::InvalidResponse);
        }
        #[cfg(windows)]
        if python_executable != runtime_root.join(WINDOWS_PYTHON_EXECUTABLE) {
            return Err(BridgePortError::InvalidResponse);
        }
        if initial_url.as_deref().is_some_and(|url| {
            url.len() > 2048
                || url.contains('\n')
                || url.contains('\r')
                || !(url.starts_with("https://")
                    || url.starts_with("http://")
                    || url.starts_with("about:"))
        }) {
            return Err(BridgePortError::InvalidResponse);
        }
        if proxy_config_path
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(Self {
            python_executable,
            runtime_root,
            display_mode,
            initial_url,
            proxy_config_path,
        })
    }
}

struct ProcessState {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    responses: Option<Receiver<Result<String, BridgePortError>>>,
    active_session: Option<SessionId>,
    profile_stable_identity: Option<ProfileStableIdentity>,
    navigation_target: Option<String>,
}

impl ProcessState {
    const fn new() -> Self {
        Self {
            child: None,
            stdin: None,
            responses: None,
            active_session: None,
            profile_stable_identity: None,
            navigation_target: None,
        }
    }

    fn clear(&mut self) {
        self.child = None;
        self.stdin = None;
        self.responses = None;
        self.active_session = None;
        self.profile_stable_identity = None;
        self.navigation_target = None;
    }
}

pub struct ManagedCamouhostProcess {
    shared: Arc<Mutex<ProcessState>>,
    config: ManagedCamouhostConfig,
    slot: RuntimeBindingSlot,
}

pub struct ManagedCamouhostIpc {
    shared: Arc<Mutex<ProcessState>>,
    display_mode: RuntimeDisplayMode,
}

#[derive(Clone)]
pub struct ManagedCamouhostCloseObserver {
    shared: Arc<Mutex<ProcessState>>,
}

impl ManagedCamouhostIpc {
    /// Read-only handle used by shipping orchestration to observe a user-controlled browser close.
    /// It shares the existing managed IPC and cannot perform the mutating close handshake.
    #[must_use]
    pub fn close_observer(&self) -> ManagedCamouhostCloseObserver {
        ManagedCamouhostCloseObserver {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl ManagedCamouhostCloseObserver {
    pub fn observe_controlled_close(
        &mut self,
        session_id: &SessionId,
    ) -> Result<bool, BridgePortError> {
        let response = exchange_shared(
            &self.shared,
            &CamouhostMessage::ObserveClose {
                session_id: session_id.clone(),
            },
        )?;
        match response {
            CamouhostMessage::CloseObserved {
                session_id: observed,
                controlled,
            } if observed == *session_id => Ok(controlled),
            _ => Err(BridgePortError::InvalidResponse),
        }
    }
}

impl ManagedCamouhostProcess {
    #[must_use]
    pub fn pair(
        config: ManagedCamouhostConfig,
        slot: RuntimeBindingSlot,
    ) -> (Self, ManagedCamouhostIpc) {
        let shared = Arc::new(Mutex::new(ProcessState::new()));
        let display_mode = config.display_mode;
        (
            Self {
                shared: Arc::clone(&shared),
                config,
                slot,
            },
            ManagedCamouhostIpc {
                shared,
                display_mode,
            },
        )
    }

    fn launch_child(
        &self,
        session_id: &SessionId,
        binding: &RuntimeLaunchBinding,
    ) -> Result<Child, BridgePortError> {
        let entrypoint = self.config.runtime_root.join(REAL_ENTRYPOINT);
        let runtime_lock = self.config.runtime_root.join(RUNTIME_LOCK_PATH);
        verify_file_digest(&entrypoint, &binding.entrypoint_sha256)?;
        verify_file_digest(&runtime_lock, &binding.runtime_lock_sha256)?;
        #[cfg(windows)]
        {
            let browser = self.config.runtime_root.join(WINDOWS_BROWSER_EXECUTABLE);
            let python = self.config.runtime_root.join(WINDOWS_PYTHON_EXECUTABLE);
            verify_file_digest(&browser, &binding.browser_sha256)?;
            verify_file_digest(&python, &binding.python_sha256)?;
        }

        #[cfg(windows)]
        let python_executable = self.config.runtime_root.join(WINDOWS_PYTHON_EXECUTABLE);
        #[cfg(not(windows))]
        let python_executable = self.config.python_executable.clone();
        let mut command = Command::new(&python_executable);
        command
            .arg(&entrypoint)
            .current_dir(&self.config.runtime_root)
            .env_clear()
            .env("CAMOUHOST_PROFILE_ROOT", &binding.profile_root)
            .env("CAMOUHOST_RUNTIME_LOCK", &runtime_lock)
            .env(
                "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256",
                &binding.runtime_lock_sha256,
            )
            .env(
                "CAMOUHOST_EXPECTED_CONFIG_SHA256",
                &binding.fingerprint_config_sha256,
            )
            .env(
                "CAMOUHOST_EXPECTED_PROBE_SHA256",
                &binding.profile_stable_probe_sha256,
            )
            .env(
                "CAMOUHOST_HEADLESS_MODE",
                match self.config.display_mode {
                    RuntimeDisplayMode::Headful => "false",
                    RuntimeDisplayMode::VirtualHeadful => "virtual",
                },
            )
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(path) = &self.config.proxy_config_path {
            command.env("CAMOUHOST_PROXY_CONFIG_PATH", path);
        }
        for key in [
            "DBUS_SESSION_BUS_ADDRESS",
            "DISPLAY",
            "GDK_BACKEND",
            "HOME",
            "LANG",
            "LC_ALL",
            "LOCALAPPDATA",
            "LOGNAME",
            "PATH",
            "SHELL",
            "SystemRoot",
            "TEMP",
            "TMP",
            "TMPDIR",
            "USER",
            "USERPROFILE",
            "WAYLAND_DISPLAY",
            "WINDIR",
            "WSL_DISTRO_NAME",
            "WSL_INTEROP",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_RUNTIME_DIR",
        ] {
            if let Some(value) = env::var_os(key) {
                command.env(key, value);
            }
        }
        let child = command.spawn().map_err(|_| BridgePortError::Unavailable)?;
        if child.id() == 0 || session_id.as_str().is_empty() {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(child)
    }
}

impl ProcessControlPort for ManagedCamouhostProcess {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let binding = self.slot.take()?;
        let mut state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.child.is_some() || state.active_session.is_some() {
            return Err(BridgePortError::Unavailable);
        }
        let mut child = self.launch_child(session_id, &binding)?;
        let stdin = child.stdin.take().ok_or(BridgePortError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(BridgePortError::Unavailable)?;
        state.stdin = Some(stdin);
        state.responses = Some(spawn_response_reader(stdout));
        state.active_session = Some(session_id.clone());
        state.profile_stable_identity = Some(binding.profile_stable_identity);
        state.navigation_target = self.config.initial_url.clone();
        state.child = Some(child);
        Ok(())
    }

    fn is_running(&mut self, session_id: &SessionId) -> Result<bool, BridgePortError> {
        let mut state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        let child = state
            .child
            .as_mut()
            .ok_or(BridgePortError::InvalidResponse)?;
        child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|_| BridgePortError::Unavailable)
    }

    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }

    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let mut state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        state.stdin = None;
        let status = wait_for_child_exit(
            state
                .child
                .as_mut()
                .ok_or(BridgePortError::InvalidResponse)?,
            PROCESS_EXIT_TIMEOUT,
        )?;
        if !status.success() {
            return Err(BridgePortError::InvalidResponse);
        }
        state.clear();
        Ok(())
    }

    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let mut state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        state.stdin = None;
        let child = state
            .child
            .as_mut()
            .ok_or(BridgePortError::InvalidResponse)?;
        if child
            .try_wait()
            .map_err(|_| BridgePortError::Unavailable)?
            .is_none()
        {
            child.kill().map_err(|_| BridgePortError::Unavailable)?;
        }
        wait_for_child_exit(child, FORCE_EXIT_TIMEOUT)?;
        state.clear();
        Ok(())
    }
}

impl CamouhostPort for ManagedCamouhostIpc {
    fn exchange(
        &mut self,
        message: &CamouhostMessage,
    ) -> Result<CamouhostMessage, BridgePortError> {
        match message {
            CamouhostMessage::Launch { session_id } => {
                launch_with_browser_visible_admission(&self.shared, self.display_mode, session_id)
            }
            _ => exchange_shared(&self.shared, message),
        }
    }
}

fn launch_with_browser_visible_admission(
    shared: &Arc<Mutex<ProcessState>>,
    display_mode: RuntimeDisplayMode,
    session_id: &SessionId,
) -> Result<CamouhostMessage, BridgePortError> {
    let ready = exchange_shared(
        shared,
        &CamouhostMessage::Launch {
            session_id: session_id.clone(),
        },
    )?;
    if ready
        != (CamouhostMessage::Ready {
            session_id: session_id.clone(),
        })
    {
        return Err(BridgePortError::InvalidResponse);
    }

    let observation = exchange_shared(
        shared,
        &CamouhostMessage::ObserveBrowserVisible {
            session_id: session_id.clone(),
        },
    )?;
    let payload_hex = match observation {
        CamouhostMessage::BrowserVisible {
            session_id: observed,
            payload_hex,
        } if observed == *session_id => payload_hex,
        _ => return Err(BridgePortError::InvalidResponse),
    };
    let payload = decode_hex(&payload_hex)?;
    let expected = {
        let state = shared.lock().map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        state
            .profile_stable_identity
            .clone()
            .ok_or(BridgePortError::InvalidResponse)?
    };
    let host_runtime_evidence = verify_browser_visible_payload(&expected, &payload)
        .map_err(|_| BridgePortError::InvalidResponse)?;
    verify_host_compatibility(display_mode, host_runtime_evidence)?;

    let navigation_target = {
        let state = shared.lock().map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        state.navigation_target.clone()
    };
    let target_hex = navigation_target
        .as_deref()
        .map(hex_encode)
        .unwrap_or_default();
    let admitted = exchange_shared(
        shared,
        &CamouhostMessage::AdmitNavigation {
            session_id: session_id.clone(),
            target_hex,
        },
    )?;
    if admitted
        != (CamouhostMessage::NavigationAdmitted {
            session_id: session_id.clone(),
        })
    {
        return Err(BridgePortError::InvalidResponse);
    }

    Ok(CamouhostMessage::Ready {
        session_id: session_id.clone(),
    })
}

fn verify_host_compatibility(
    display_mode: RuntimeDisplayMode,
    evidence: HostRuntimeEvidence,
) -> Result<(), BridgePortError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BridgePortError::Unavailable)?
        .as_millis();
    let clock_unix_ms = u64::try_from(millis).map_err(|_| BridgePortError::Unavailable)?;
    let (policy, platform, runtime_class) = host_compatibility_policy()?;
    let execution_mode = match display_mode {
        RuntimeDisplayMode::Headful => HostExecutionMode::Headful,
        RuntimeDisplayMode::VirtualHeadful => HostExecutionMode::VirtualHeadful,
    };

    // Reaching this point proves that the exact runtime/profile files were readable, the managed
    // process spawned, the canonical IPC reached Ready, and the real browser returned its bounded
    // typed observation. Those are the required filesystem/process capability facts; the domain
    // remains the sole owner of deciding whether the resulting host is admitted.
    let observation = HostCompatibilityObservation::prelaunch(
        platform,
        HostArchitecture::X86_64,
        runtime_class,
        execution_mode,
        clock_unix_ms,
        true,
    )
    .with_runtime_evidence(true, evidence.display, evidence.graphics_backend);

    if policy.evaluate(&observation) != HostCompatibilityDecision::Accepted {
        eprintln!("CAMOUHOST_HOST_COMPATIBILITY=INCOMPATIBLE");
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(())
}

fn host_compatibility_policy()
-> Result<(HostCompatibilityPolicy, HostPlatformClass, HostRuntimeClass), BridgePortError> {
    if !cfg!(target_arch = "x86_64") {
        return Err(BridgePortError::InvalidResponse);
    }

    #[cfg(windows)]
    {
        let policy = HostCompatibilityPolicy::windows_first_release_headful()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok((
            policy,
            HostPlatformClass::Windows,
            HostRuntimeClass::PackagedCamoufox,
        ))
    }

    #[cfg(all(not(windows), target_os = "linux"))]
    {
        let policy = HostCompatibilityPolicy::repository_linux_headful()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok((
            policy,
            HostPlatformClass::Linux,
            HostRuntimeClass::RepositoryPinnedCamoufox,
        ))
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(BridgePortError::InvalidResponse)
    }
}

fn exchange_shared(
    shared: &Arc<Mutex<ProcessState>>,
    message: &CamouhostMessage,
) -> Result<CamouhostMessage, BridgePortError> {
    let frame = message
        .to_frame()
        .map_err(|_| BridgePortError::InvalidResponse)?;
    let response = exchange_frame_shared(shared, &frame, response_timeout(message)?)?;
    let parsed =
        CamouhostMessage::parse(&response).map_err(|_| BridgePortError::InvalidResponse)?;
    parsed
        .validate_version()
        .map_err(|_| BridgePortError::InvalidResponse)?;
    if matches!(parsed, CamouhostMessage::Error { .. }) {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(parsed)
}

fn exchange_frame_shared(
    shared: &Arc<Mutex<ProcessState>>,
    frame: &str,
    timeout: Duration,
) -> Result<String, BridgePortError> {
    if frame.is_empty() || frame.len() > MAX_IPC_REQUEST_BYTES || frame.contains(['\n', '\r', '\0'])
    {
        return Err(BridgePortError::InvalidResponse);
    }
    let mut state = shared.lock().map_err(|_| BridgePortError::Unavailable)?;
    let stdin = state.stdin.as_mut().ok_or(BridgePortError::Unavailable)?;
    stdin
        .write_all(frame.as_bytes())
        .and_then(|()| stdin.write_all(b"\n"))
        .and_then(|()| stdin.flush())
        .map_err(|_| BridgePortError::Unavailable)?;
    let responses = state
        .responses
        .as_ref()
        .ok_or(BridgePortError::Unavailable)?;
    receive_response(responses, timeout)
}

fn spawn_response_reader(stdout: ChildStdout) -> Receiver<Result<String, BridgePortError>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_bounded_line(&mut reader) {
                Ok(response) => {
                    if sender.send(Ok(response)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Err(error));
                    break;
                }
            }
        }
    });
    receiver
}

fn receive_response(
    responses: &Receiver<Result<String, BridgePortError>>,
    timeout: Duration,
) -> Result<String, BridgePortError> {
    match responses.recv_timeout(timeout) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(BridgePortError::Unavailable),
        Err(RecvTimeoutError::Disconnected) => Err(BridgePortError::InvalidResponse),
    }
}

fn response_timeout(message: &CamouhostMessage) -> Result<Duration, BridgePortError> {
    match message {
        CamouhostMessage::Hello { .. } => Ok(HELLO_RESPONSE_TIMEOUT),
        CamouhostMessage::Launch { .. } => Ok(LAUNCH_RESPONSE_TIMEOUT),
        CamouhostMessage::ObserveBrowserVisible { .. } => Ok(BROWSER_VISIBLE_RESPONSE_TIMEOUT),
        CamouhostMessage::AdmitNavigation { .. } => Ok(NAVIGATION_RESPONSE_TIMEOUT),
        CamouhostMessage::ObserveClose { .. } => Ok(OBSERVE_CLOSE_RESPONSE_TIMEOUT),
        CamouhostMessage::Close { .. } => Ok(CLOSE_RESPONSE_TIMEOUT),
        _ => Err(BridgePortError::InvalidResponse),
    }
}

fn wait_for_child_exit(
    child: &mut Child,
    timeout: Duration,
) -> Result<ExitStatus, BridgePortError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| BridgePortError::Unavailable)? {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            return Err(BridgePortError::Unavailable);
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

fn read_bounded_line(reader: &mut BufReader<ChildStdout>) -> Result<String, BridgePortError> {
    let mut bytes = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .map_err(|_| BridgePortError::Unavailable)?;
        if available.is_empty() {
            return Err(BridgePortError::InvalidResponse);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if bytes.len().saturating_add(consumed) > MAX_IPC_RESPONSE_BYTES {
            return Err(BridgePortError::InvalidResponse);
        }
        bytes.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if bytes.last() != Some(&b'\n') || bytes.contains(&b'\r') || bytes.contains(&b'\0') {
        return Err(BridgePortError::InvalidResponse);
    }
    bytes.pop();
    String::from_utf8(bytes).map_err(|_| BridgePortError::InvalidResponse)
}

fn hex_encode(bytes: &str) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes.as_bytes() {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, BridgePortError> {
    if !value.len().is_multiple_of(2) {
        return Err(BridgePortError::InvalidResponse);
    }
    let mut decoded = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0]).ok_or(BridgePortError::InvalidResponse)?;
        let low = hex_nibble(pair[1]).ok_or(BridgePortError::InvalidResponse)?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn verify_file_digest(path: &Path, expected: &str) -> Result<(), BridgePortError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| BridgePortError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BridgePortError::InvalidResponse);
    }
    let bytes = fs::read(path).map_err(|_| BridgePortError::Unavailable)?;
    if sha256_hex(&bytes) != expected {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        FINGERPRINT_SOURCE_PREFIX, RuntimeBindingBrowserLaunchPreflight, RuntimeBindingSlot,
        decode_hex, hex_encode, receive_response, sha256_hex,
    };
    use crate::browser_execution::persist_materialization_binding;
    use crate::browser_preflight::{BrowserRuntimeObservation, BrowserRuntimeObservationPort};
    use crate::local_profile::{BridgeWorkspaceLock, GenerationWorkspace, MaterializationRoot};
    use crate::operator_flow::BrowserLaunchPreflightPort;
    use crate::runtime_bundle::ApprovedRuntimeBundle;
    use crate::test_support::{browser_identity_fixture, remove_test_root};
    use bridge_domain::{BridgePortError, CamouhostMessage};
    use browser_execution_domain::{
        MaterializationBinding, NetworkClass, NetworkIdentityObservation, NetworkIdentityPolicy,
    };
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, SessionId, TenantId};
    use runtime_bundle_domain::{
        BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
        Sha256Digest,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct FixedObservation(NetworkIdentityObservation);

    impl BrowserRuntimeObservationPort for FixedObservation {
        type Error = BridgePortError;

        fn observe(
            &mut self,
            _workspace: &GenerationWorkspace,
            _device_id: &DeviceId,
        ) -> Result<BrowserRuntimeObservation, Self::Error> {
            Ok(BrowserRuntimeObservation::new(self.0.clone(), false))
        }
    }

    fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "profile-bridge-ar10-runtime-{}-{nonce}",
            std::process::id()
        )))
    }

    fn digest(character: char) -> Result<Sha256Digest, Box<dyn std::error::Error>> {
        Ok(Sha256Digest::parse(character.to_string().repeat(64))?)
    }

    fn bundle(
        runtime_lock_sha256: &str,
    ) -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
        let calculated = digest('a')?;
        let entrypoint = BundleRelativePath::parse("camouhost/real.py")?;
        let lock_path = BundleRelativePath::parse("camouhost/runtime-lock.json")?;
        let entries = vec![
            InventoryEntry::new(entrypoint.clone(), 10, digest('e')?),
            InventoryEntry::new(
                lock_path,
                10,
                Sha256Digest::parse(runtime_lock_sha256.to_owned())?,
            ),
        ];
        #[cfg(windows)]
        let entries = {
            let mut entries = entries;
            entries.push(InventoryEntry::new(
                BundleRelativePath::parse(super::WINDOWS_BROWSER_EXECUTABLE)?,
                10,
                digest('f')?,
            ));
            entries.push(InventoryEntry::new(
                BundleRelativePath::parse(super::WINDOWS_PYTHON_EXECUTABLE)?,
                10,
                digest('9')?,
            ));
            entries
        };
        let manifest = RuntimeManifest::new(
            "2.0.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint,
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new(entries)?;
        Ok(ApprovedRuntimeBundle::validate(
            manifest,
            inventory,
            &calculated,
        )?)
    }

    fn policy() -> Result<NetworkIdentityPolicy, Box<dyn std::error::Error>> {
        Ok(NetworkIdentityPolicy::new(
            Some("PL".to_owned()),
            Some("Mazowieckie".to_owned()),
            Some("Europe/Warsaw".to_owned()),
            [NetworkClass::Mobile],
            [5617],
            Some("route-a".to_owned()),
        )?)
    }

    fn observation() -> Result<NetworkIdentityObservation, Box<dyn std::error::Error>> {
        Ok(NetworkIdentityObservation::new(
            "PL",
            "Mazowieckie",
            "Europe/Warsaw",
            NetworkClass::Mobile,
            5617,
            "route-a",
        )?)
    }

    fn binding(
        workspace: &GenerationWorkspace,
        tenant: TenantId,
        profile: ProfileId,
        generation: GenerationId,
        approved: &ApprovedRuntimeBundle,
        config_sha256: String,
        probe_sha256: &str,
    ) -> Result<MaterializationBinding, Box<dyn std::error::Error>> {
        let fingerprint_source = format!("{FINGERPRINT_SOURCE_PREFIX}{probe_sha256}");
        let browser_identity = browser_identity_fixture(
            approved.manifest().runtime_version(),
            approved.manifest().inventory_sha256().as_str(),
            &fingerprint_source,
            config_sha256,
        )?;
        Ok(MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "d".repeat(64),
            workspace.inventory()?.inventory_digest(),
            browser_identity,
        )?)
    }

    #[test]
    fn ipc_response_wait_is_bounded() {
        let (_sender, receiver) = mpsc::channel::<Result<String, BridgePortError>>();
        assert_eq!(
            receive_response(&receiver, Duration::from_millis(1)),
            Err(BridgePortError::Unavailable)
        );
    }

    #[test]
    fn controlled_close_observation_uses_canonical_ipc_frame()
    -> Result<(), Box<dyn std::error::Error>> {
        let session_id = SessionId::parse("session_01JAR10CLOSE")?;
        assert_eq!(
            CamouhostMessage::ObserveClose {
                session_id: session_id.clone(),
            }
            .to_frame()?,
            format!("observe_close|{}", session_id.as_str())
        );
        assert_eq!(
            CamouhostMessage::CloseObserved {
                session_id,
                controlled: true,
            }
            .to_frame()?,
            "close_observed|session_01JAR10CLOSE|true"
        );
        Ok(())
    }

    #[test]
    fn browser_visible_and_navigation_frames_are_canonical_and_session_bound()
    -> Result<(), Box<dyn std::error::Error>> {
        let session_id = SessionId::parse("session_01JAR10VISIBLE")?;
        let payload = br#"{"user_agent":"ua"}"#;
        let payload_hex = hex_encode(std::str::from_utf8(payload)?);
        let frame = CamouhostMessage::BrowserVisible {
            session_id: session_id.clone(),
            payload_hex: payload_hex.clone(),
        }
        .to_frame()?;
        let parsed = CamouhostMessage::parse(&frame)?;
        let CamouhostMessage::BrowserVisible {
            session_id: observed,
            payload_hex: observed_hex,
        } = parsed
        else {
            return Err(std::io::Error::other("unexpected canonical IPC message").into());
        };
        assert_eq!(observed, session_id);
        assert_eq!(decode_hex(&observed_hex)?, payload);
        let target = "https://example.test/private-path";
        let encoded = hex_encode(target);
        let navigation = CamouhostMessage::AdmitNavigation {
            session_id,
            target_hex: encoded.clone(),
        }
        .to_frame()?;
        assert!(navigation.starts_with("admit_navigation|"));
        assert!(!encoded.contains(target));
        assert!(!encoded.contains('/'));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_config_rejects_non_packaged_python_path() {
        let runtime_root = PathBuf::from(r"C:\profile-bridge\runtime");
        let python = PathBuf::from(r"C:\other\python.exe");
        assert!(
            super::ManagedCamouhostConfig::new(
                python,
                runtime_root,
                super::RuntimeDisplayMode::Headful,
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn exact_config_and_probe_are_bound_before_slot_publication()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JAR10RUNTIME")?;
        let profile = ProfileId::parse("profile_01JAR10RUNTIME")?;
        let generation = GenerationId::parse("generation_01JAR10RUNTIME")?;
        let device = DeviceId::parse("device_01JAR10RUNTIME")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 7)?;

        let config = b"{}\n";
        fs::write(workspace.path().join("camoufox-config.json"), config)?;
        let approved = bundle(&"b".repeat(64))?;
        let expected = binding(
            &workspace,
            tenant,
            profile,
            generation,
            &approved,
            sha256_hex(config),
            &"c".repeat(64),
        )?;
        persist_materialization_binding(&workspace, &expected)?;
        let slot = RuntimeBindingSlot::new();
        let mut preflight = RuntimeBindingBrowserLaunchPreflight::new(
            expected,
            policy()?,
            FixedObservation(observation()?),
            slot.clone(),
        );
        preflight.evaluate_before_launch(&workspace, &device, 7, &approved)?;
        assert!(slot.take().is_ok());
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn config_substitution_is_rejected_before_slot_publication()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JAR10DRIFT")?;
        let profile = ProfileId::parse("profile_01JAR10DRIFT")?;
        let generation = GenerationId::parse("generation_01JAR10DRIFT")?;
        let device = DeviceId::parse("device_01JAR10DRIFT")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 8)?;
        let config = b"{}\n";
        fs::write(workspace.path().join("camoufox-config.json"), config)?;
        let approved = bundle(&"b".repeat(64))?;
        let expected = binding(
            &workspace,
            tenant,
            profile,
            generation,
            &approved,
            sha256_hex(config),
            &"c".repeat(64),
        )?;
        persist_materialization_binding(&workspace, &expected)?;
        fs::write(
            workspace.path().join("camoufox-config.json"),
            b"{\"drift\":true}\n",
        )?;
        let slot = RuntimeBindingSlot::new();
        let mut preflight = RuntimeBindingBrowserLaunchPreflight::new(
            expected,
            policy()?,
            FixedObservation(observation()?),
            slot.clone(),
        );
        assert!(
            preflight
                .evaluate_before_launch(&workspace, &device, 8, &approved)
                .is_err()
        );
        assert!(slot.take().is_err());
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }
}
