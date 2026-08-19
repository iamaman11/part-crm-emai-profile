use crate::browser_execution::BrowserLaunchBlocker;
use crate::browser_preflight::{
    BoundBrowserLaunchPreflight, BrowserRuntimeObservationPort,
};
use crate::local_profile::GenerationWorkspace;
use crate::operator_flow::{BrowserLaunchPreflightPort, RuntimeBundleSelectionPort};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use crate::{ProcessControlPort};
use bridge_domain::{
    BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort,
};
use browser_execution_domain::{MaterializationBinding, NetworkIdentityPolicy};
use profile_platform_primitives::{DeviceId, SessionId};
use runtime_bundle_domain::BundleRelativePath;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

const IDENTITY_NAME: &str = "camoufox-identity.json";
const CONFIG_NAME: &str = "camoufox-config.json";
const REAL_ENTRYPOINT: &str = "camouhost/real.py";
const RUNTIME_LOCK_PATH: &str = "camouhost/runtime-lock.json";
const MAX_IDENTITY_BYTES: u64 = 64 * 1024;
const MAX_IPC_RESPONSE_BYTES: usize = 1024;
const FINGERPRINT_POLICY: &str = "profile-stability-v1";
const FINGERPRINT_CONFIG_SCHEMA: &str = "camoufox-canonical-config-v1";
const IDENTITY_SOURCE_PREFIX: &str = "identity-v1-";

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
    entrypoint_sha256: String,
}

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
            inner: BoundBrowserLaunchPreflight::new(
                expected.clone(),
                network_policy,
                observations,
            ),
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
        self.inner.evaluate_before_launch(
            workspace,
            device_id,
            workspace_epoch,
            runtime_bundle,
        )?;
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
    if browser_identity.compatibility_version() < 2 {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let source = browser_identity.fingerprint_source();
    let identity_digest = source
        .strip_prefix(IDENTITY_SOURCE_PREFIX)
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if !valid_sha256(identity_digest) {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }

    let identity_path = workspace.path().join(IDENTITY_NAME);
    let identity_bytes = read_regular_bounded(&identity_path, MAX_IDENTITY_BYTES)?;
    if sha256_hex(&identity_bytes) != identity_digest {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let identity: Value = serde_json::from_slice(&identity_bytes)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let object = identity
        .as_object()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let required = [
        "browser",
        "components",
        "fingerprint_config_schema",
        "fingerprint_config_sha256",
        "fingerprint_policy_version",
        "profile_stable_probe_sha256",
        "runtime_lock_sha256",
        "schema_version",
    ];
    if object.len() != required.len() || required.iter().any(|key| !object.contains_key(*key)) {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    if object.get("schema_version").and_then(Value::as_u64) != Some(1)
        || object
            .get("fingerprint_policy_version")
            .and_then(Value::as_str)
            != Some(FINGERPRINT_POLICY)
        || object
            .get("fingerprint_config_schema")
            .and_then(Value::as_str)
            != Some(FINGERPRINT_CONFIG_SCHEMA)
    {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }

    let config_sha256 = required_digest(object, "fingerprint_config_sha256")?;
    if config_sha256 != browser_identity.fingerprint_config_sha256() {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let probe_sha256 = required_digest(object, "profile_stable_probe_sha256")?;
    let runtime_lock_sha256 = required_digest(object, "runtime_lock_sha256")?;

    let config_bytes = read_regular_bounded(&workspace.path().join(CONFIG_NAME), 1024 * 1024)?;
    if sha256_hex(&config_bytes) != config_sha256 {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }

    let runtime_lock = inventory_digest(runtime_bundle, RUNTIME_LOCK_PATH)?;
    if runtime_lock != runtime_lock_sha256 {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let entrypoint = runtime_bundle.manifest().entrypoint().as_str();
    if entrypoint != REAL_ENTRYPOINT {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let entrypoint_sha256 = inventory_digest(runtime_bundle, entrypoint)?.to_owned();

    let browser = object
        .get("browser")
        .and_then(Value::as_object)
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let components = object
        .get("components")
        .and_then(Value::as_object)
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    for key in ["repository", "release_commit", "version"] {
        if browser.get(key).and_then(Value::as_str).is_none_or(str::is_empty) {
            return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
        }
    }
    for key in ["browserforge", "camoufox_python", "playwright"] {
        if components.get(key).and_then(Value::as_str).is_none_or(str::is_empty) {
            return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
        }
    }

    Ok(RuntimeLaunchBinding {
        profile_root: workspace.path().to_path_buf(),
        runtime_lock_sha256: runtime_lock_sha256.to_owned(),
        fingerprint_config_sha256: config_sha256.to_owned(),
        profile_stable_probe_sha256: probe_sha256.to_owned(),
        entrypoint_sha256,
    })
}

fn required_digest<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, BrowserLaunchBlocker> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if !valid_sha256(value) {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    Ok(value)
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
    format!("{:x}", Sha256::digest(bytes))
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
        if initial_url.as_deref().is_some_and(|url| {
            url.len() > 2048
                || url.contains(['\n', '\r'])
                || !url.starts_with(("https://", "http://", "about:"))
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
    stdout: Option<BufReader<ChildStdout>>,
    active_session: Option<SessionId>,
}

impl ProcessState {
    const fn new() -> Self {
        Self {
            child: None,
            stdin: None,
            stdout: None,
            active_session: None,
        }
    }

    fn clear(&mut self) {
        self.child = None;
        self.stdin = None;
        self.stdout = None;
        self.active_session = None;
    }
}

pub struct ManagedCamouhostProcess {
    shared: Arc<Mutex<ProcessState>>,
    config: ManagedCamouhostConfig,
    slot: RuntimeBindingSlot,
}

pub struct ManagedCamouhostIpc {
    shared: Arc<Mutex<ProcessState>>,
}

impl ManagedCamouhostProcess {
    #[must_use]
    pub fn pair(
        config: ManagedCamouhostConfig,
        slot: RuntimeBindingSlot,
    ) -> (Self, ManagedCamouhostIpc) {
        let shared = Arc::new(Mutex::new(ProcessState::new()));
        (
            Self {
                shared: Arc::clone(&shared),
                config,
                slot,
            },
            ManagedCamouhostIpc { shared },
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

        let mut command = Command::new(&self.config.python_executable);
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
        if let Some(url) = &self.config.initial_url {
            command.env("CAMOUHOST_INITIAL_URL", url);
        }
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
            "TEMP",
            "TMP",
            "TMPDIR",
            "USER",
            "USERPROFILE",
            "WAYLAND_DISPLAY",
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
        let mut state = self.shared.lock().map_err(|_| BridgePortError::Unavailable)?;
        if state.child.is_some() || state.active_session.is_some() {
            return Err(BridgePortError::Unavailable);
        }
        let mut child = self.launch_child(session_id, &binding)?;
        let stdin = child.stdin.take().ok_or(BridgePortError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(BridgePortError::Unavailable)?;
        state.stdin = Some(stdin);
        state.stdout = Some(BufReader::new(stdout));
        state.active_session = Some(session_id.clone());
        state.child = Some(child);
        Ok(())
    }

    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let state = self.shared.lock().map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }

    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let mut state = self.shared.lock().map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        let status = state
            .child
            .as_mut()
            .ok_or(BridgePortError::InvalidResponse)?
            .wait()
            .map_err(|_| BridgePortError::Unavailable)?;
        if !status.success() {
            return Err(BridgePortError::InvalidResponse);
        }
        state.clear();
        Ok(())
    }

    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let mut state = self.shared.lock().map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        let child = state
            .child
            .as_mut()
            .ok_or(BridgePortError::InvalidResponse)?;
        child.kill().map_err(|_| BridgePortError::Unavailable)?;
        child.wait().map_err(|_| BridgePortError::Unavailable)?;
        state.clear();
        Ok(())
    }
}

impl CamouhostPort for ManagedCamouhostIpc {
    fn exchange(
        &mut self,
        message: &CamouhostMessage,
    ) -> Result<CamouhostMessage, BridgePortError> {
        let mut state = self.shared.lock().map_err(|_| BridgePortError::Unavailable)?;
        let frame = request_frame(message)?;
        let stdin = state.stdin.as_mut().ok_or(BridgePortError::Unavailable)?;
        stdin
            .write_all(frame.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .map_err(|_| BridgePortError::Unavailable)?;
        let stdout = state.stdout.as_mut().ok_or(BridgePortError::Unavailable)?;
        let mut response = String::new();
        stdout
            .read_line(&mut response)
            .map_err(|_| BridgePortError::Unavailable)?;
        if response.is_empty()
            || response.len() > MAX_IPC_RESPONSE_BYTES
            || !response.ends_with('\n')
            || response.contains('\r')
        {
            return Err(BridgePortError::InvalidResponse);
        }
        response.pop();
        let parsed = CamouhostMessage::parse(&response)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        parsed
            .validate_version()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(parsed)
    }
}

fn request_frame(message: &CamouhostMessage) -> Result<String, BridgePortError> {
    match message {
        CamouhostMessage::Hello { version } if *version == CAMOUHOST_IPC_VERSION => {
            Ok(format!("hello|{version}"))
        }
        CamouhostMessage::Launch { session_id } => {
            Ok(format!("launch|{}", session_id.as_str()))
        }
        CamouhostMessage::Close { session_id } => Ok(format!("close|{}", session_id.as_str())),
        _ => Err(BridgePortError::InvalidResponse),
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
        FINGERPRINT_CONFIG_SCHEMA, FINGERPRINT_POLICY, IDENTITY_NAME, IDENTITY_SOURCE_PREFIX,
        RuntimeBindingBrowserLaunchPreflight, RuntimeBindingSlot, sha256_hex,
    };
    use crate::browser_execution::persist_materialization_binding;
    use crate::browser_preflight::{BrowserRuntimeObservation, BrowserRuntimeObservationPort};
    use crate::local_profile::{BridgeWorkspaceLock, GenerationWorkspace, MaterializationRoot};
    use crate::operator_flow::BrowserLaunchPreflightPort;
    use crate::runtime_bundle::ApprovedRuntimeBundle;
    use crate::test_support::remove_test_root;
    use bridge_domain::BridgePortError;
    use browser_execution_domain::{
        BrowserIdentityManifest, MaterializationBinding, NetworkClass, NetworkIdentityObservation,
        NetworkIdentityPolicy,
    };
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use runtime_bundle_domain::{
        BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
        Sha256Digest,
    };
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let manifest = RuntimeManifest::new(
            "2.0.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new([
            InventoryEntry::new(entrypoint, 10, digest('e')?),
            InventoryEntry::new(
                lock_path,
                10,
                Sha256Digest::parse(runtime_lock_sha256.to_owned())?,
            ),
        ])?;
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

    #[test]
    fn runtime_identity_is_cryptographically_bound_before_slot_publication()
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
        let config_sha256 = sha256_hex(config);
        let runtime_lock_sha256 = "b".repeat(64);
        let probe_sha256 = "c".repeat(64);
        let identity = json!({
            "browser": {
                "release_commit": "0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d",
                "repository": "daijro/camoufox",
                "version": "152.0.4-beta.28"
            },
            "components": {
                "browserforge": "1.2.4",
                "camoufox_python": "0.5.5",
                "playwright": "1.60.0"
            },
            "fingerprint_config_schema": FINGERPRINT_CONFIG_SCHEMA,
            "fingerprint_config_sha256": config_sha256,
            "fingerprint_policy_version": FINGERPRINT_POLICY,
            "profile_stable_probe_sha256": probe_sha256,
            "runtime_lock_sha256": runtime_lock_sha256,
            "schema_version": 1
        });
        let mut identity_bytes = serde_json::to_vec(&identity)?;
        identity_bytes.push(b'\n');
        fs::write(workspace.path().join(IDENTITY_NAME), &identity_bytes)?;
        let source = format!("{IDENTITY_SOURCE_PREFIX}{}", sha256_hex(&identity_bytes));
        let approved = bundle(&runtime_lock_sha256)?;
        let browser_identity = BrowserIdentityManifest::new(
            2,
            approved.manifest().runtime_version(),
            approved.manifest().inventory_sha256().as_str(),
            source,
            config_sha256,
        )?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "d".repeat(64),
            workspace.inventory()?.inventory_digest(),
            browser_identity,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        let slot = RuntimeBindingSlot::new();
        let mut preflight = RuntimeBindingBrowserLaunchPreflight::new(
            binding,
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
    fn runtime_lock_substitution_is_rejected_before_slot_publication()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JAR10LOCK")?;
        let profile = ProfileId::parse("profile_01JAR10LOCK")?;
        let generation = GenerationId::parse("generation_01JAR10LOCK")?;
        let device = DeviceId::parse("device_01JAR10LOCK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 8)?;
        let config = b"{}\n";
        fs::write(workspace.path().join("camoufox-config.json"), config)?;
        let config_sha256 = sha256_hex(config);
        let identity = json!({
            "browser": {"release_commit":"x","repository":"daijro/camoufox","version":"152.0.4-beta.28"},
            "components": {"browserforge":"1.2.4","camoufox_python":"0.5.5","playwright":"1.60.0"},
            "fingerprint_config_schema": FINGERPRINT_CONFIG_SCHEMA,
            "fingerprint_config_sha256": config_sha256,
            "fingerprint_policy_version": FINGERPRINT_POLICY,
            "profile_stable_probe_sha256": "c".repeat(64),
            "runtime_lock_sha256": "b".repeat(64),
            "schema_version": 1
        });
        let mut identity_bytes = serde_json::to_vec(&identity)?;
        identity_bytes.push(b'\n');
        fs::write(workspace.path().join(IDENTITY_NAME), &identity_bytes)?;
        let approved = bundle(&"f".repeat(64))?;
        let browser_identity = BrowserIdentityManifest::new(
            2,
            approved.manifest().runtime_version(),
            approved.manifest().inventory_sha256().as_str(),
            format!("{IDENTITY_SOURCE_PREFIX}{}", sha256_hex(&identity_bytes)),
            config_sha256,
        )?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "d".repeat(64),
            workspace.inventory()?.inventory_digest(),
            browser_identity,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        let slot = RuntimeBindingSlot::new();
        let mut preflight = RuntimeBindingBrowserLaunchPreflight::new(
            binding,
            policy()?,
            FixedObservation(observation()?),
            slot.clone(),
        );
        assert!(preflight
            .evaluate_before_launch(&workspace, &device, 8, &approved)
            .is_err());
        assert!(slot.take().is_err());
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }
}
