#![forbid(unsafe_code)]

use bridge_domain::{BridgePortError, CamouhostMessage, CamouhostPort};
use browser_execution_domain::{
    BrowserIdentityManifest, BrowserOsIdentity, DisplayIdentity, FontIdentity, GraphicsIdentity,
    HardwareCapabilityIdentity, LocaleIdentity, MaterializationBinding, NetworkClass,
    NetworkIdentityObservation, NetworkIdentityPolicy, OriginDeterminismMode,
    OriginDeterministicIdentity, ProfileStableIdentity,
};
use profile_bridge::browser_execution::persist_materialization_binding;
use profile_bridge::browser_preflight::{BrowserRuntimeObservation, BrowserRuntimeObservationPort};
use profile_bridge::camouhost_process::{
    ManagedCamouhostConfig, ManagedCamouhostProcess, RuntimeBindingBrowserLaunchPreflight,
    RuntimeBindingSlot, RuntimeDisplayMode,
};
use profile_bridge::local_profile::{BridgeWorkspaceLock, MaterializationRoot};
use profile_bridge::operator_flow::BrowserLaunchPreflightPort;
use profile_bridge::runtime_bundle::{ApprovedRuntimeBundle, RuntimeSessionOrchestrator};
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, SessionId, TenantId};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const ENABLE_ENV: &str = "AR10_REAL_CAMOUFOX";
const PYTHON_ENV: &str = "AR10_PYTHON";
const RUNTIME_ROOT_ENV: &str = "AR10_RUNTIME_ROOT";
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
const MAX_DIAGNOSTIC_SYMLINKS: usize = 32;
const CONFIG_NAME: &str = "camoufox-config.json";

struct FixedObservation(NetworkIdentityObservation);

impl BrowserRuntimeObservationPort for FixedObservation {
    type Error = BridgePortError;

    fn observe(
        &mut self,
        _workspace: &profile_bridge::local_profile::GenerationWorkspace,
        _device_id: &DeviceId,
    ) -> Result<BrowserRuntimeObservation, Self::Error> {
        Ok(BrowserRuntimeObservation::new(self.0.clone(), false))
    }
}

struct StageTracingCamouhost<C> {
    inner: C,
}

impl<C> StageTracingCamouhost<C> {
    const fn new(inner: C) -> Self {
        Self { inner }
    }
}

impl<C> CamouhostPort for StageTracingCamouhost<C>
where
    C: CamouhostPort,
{
    fn exchange(
        &mut self,
        message: &CamouhostMessage,
    ) -> Result<CamouhostMessage, BridgePortError> {
        let stage = match message {
            CamouhostMessage::Hello { .. } => "hello",
            CamouhostMessage::Launch { .. } => "launch",
            CamouhostMessage::Close { .. } => "close",
            _ => "unexpected_request",
        };
        let result = self.inner.exchange(message);
        match &result {
            Ok(response) => {
                let outcome = match response {
                    CamouhostMessage::HelloAck { .. } => "hello_ack",
                    CamouhostMessage::Ready { .. } => "ready",
                    CamouhostMessage::Closed { clean: true, .. } => "closed_clean",
                    CamouhostMessage::Closed { clean: false, .. } => "closed_unclean",
                    _ => "unexpected_response",
                };
                eprintln!("AR10_MANAGED_IPC_STAGE={stage};OUTCOME={outcome}");
            }
            Err(error) => {
                eprintln!("AR10_MANAGED_IPC_STAGE={stage};ERROR={error:?}");
            }
        }
        result
    }
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

fn digest_file(path: &Path) -> Result<(u64, String), Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("runtime evidence file is not regular".into());
    }
    let bytes = fs::read(path)?;
    Ok((metadata.len(), sha256_hex(&bytes)))
}

fn json_field(report: &str, field: &str) -> Result<String, Box<dyn std::error::Error>> {
    let prefix = format!("\"{field}\":\"");
    let start = report
        .find(&prefix)
        .ok_or("candidate materialization report field is missing")?
        + prefix.len();
    let rest = &report[start..];
    let end = rest
        .find('"')
        .ok_or("candidate materialization report field is unterminated")?;
    let value = &rest[..end];
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("candidate materialization digest is invalid".into());
    }
    Ok(value.to_owned())
}

fn canonical_value_sha256(value: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(sha256_hex(&bytes))
}

fn required_config<'a>(
    config: &'a Value,
    key: &str,
) -> Result<&'a Value, Box<dyn std::error::Error>> {
    config
        .as_object()
        .and_then(|object| object.get(key))
        .ok_or_else(|| format!("materialized Camoufox config is missing {key}").into())
}

fn config_string(config: &Value, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    required_config(config, key)?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("materialized Camoufox config field {key} is not text").into())
}

fn config_integer(config: &Value, key: &str) -> Result<i64, Box<dyn std::error::Error>> {
    let value = required_config(config, key)?;
    if let Some(integer) = value.as_i64() {
        return Ok(integer);
    }
    if let Some(integer) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
        return Ok(integer);
    }
    let Some(number) = value.as_f64() else {
        return Err(format!("materialized Camoufox config field {key} is not numeric").into());
    };
    if !number.is_finite()
        || number.fract() != 0.0
        || number < i64::MIN as f64
        || number > i64::MAX as f64
    {
        return Err(format!("materialized Camoufox config field {key} is not an integer").into());
    }
    Ok(number as i64)
}

fn config_u16(config: &Value, key: &str) -> Result<u16, Box<dyn std::error::Error>> {
    Ok(u16::try_from(config_integer(config, key)?)?)
}

fn optional_config_u16(
    config: &Value,
    key: &str,
) -> Result<Option<u16>, Box<dyn std::error::Error>> {
    match config.as_object().and_then(|object| object.get(key)) {
        Some(_) => Ok(Some(config_u16(config, key)?)),
        None => Ok(None),
    }
}

fn config_u32(config: &Value, key: &str) -> Result<u32, Box<dyn std::error::Error>> {
    Ok(u32::try_from(config_integer(config, key)?)?)
}

fn config_i32(config: &Value, key: &str) -> Result<i32, Box<dyn std::error::Error>> {
    Ok(i32::try_from(config_integer(config, key)?)?)
}

fn config_dpr_milli(config: &Value) -> Result<u32, Box<dyn std::error::Error>> {
    let value = required_config(config, "window.devicePixelRatio")?
        .as_f64()
        .ok_or("materialized Camoufox devicePixelRatio is not numeric")?;
    let milli = value * 1000.0;
    if !milli.is_finite() || milli <= 0.0 || (milli.round() - milli).abs() > f64::EPSILON {
        return Err("materialized Camoufox devicePixelRatio is not millipixel-exact".into());
    }
    Ok(u32::try_from(milli.round() as u64)?)
}

fn browser_major(user_agent: &str) -> Result<u16, Box<dyn std::error::Error>> {
    let version = user_agent
        .rsplit_once("Firefox/")
        .map(|(_, version)| version)
        .ok_or("materialized Camoufox user-agent has no Firefox version")?;
    let major = version
        .split('.')
        .next()
        .ok_or("materialized Camoufox Firefox version is empty")?;
    Ok(major.parse()?)
}

fn combined_config_digest(
    config: &Value,
    webgl_key: &str,
    webgl2_key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    canonical_value_sha256(&serde_json::json!({
        "webgl": required_config(config, webgl_key)?,
        "webgl2": required_config(config, webgl2_key)?,
    }))
}

fn typed_identity_from_materialized_config(
    config_path: &Path,
) -> Result<ProfileStableIdentity, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(config_path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("materialized Camoufox config is not a regular file".into());
    }
    let raw = fs::read(config_path)?;
    let config: Value = serde_json::from_slice(&raw)?;
    if !config.is_object() {
        return Err("materialized Camoufox config is not an object".into());
    }

    let user_agent = config_string(&config, "navigator.userAgent")?;
    let voices_sha256 = config
        .as_object()
        .and_then(|object| object.get("voices"))
        .map(canonical_value_sha256)
        .transpose()?;

    ProfileStableIdentity::new(
        1,
        BrowserOsIdentity::new(
            user_agent.clone(),
            browser_major(&user_agent)?,
            config_string(&config, "navigator.platform")?,
            config_string(&config, "navigator.oscpu")?,
        )?,
        HardwareCapabilityIdentity::new(
            config_u16(&config, "navigator.hardwareConcurrency")?,
            optional_config_u16(&config, "navigator.deviceMemory")?,
            config_u16(&config, "navigator.maxTouchPoints")?,
        )?,
        DisplayIdentity::new(
            config_u32(&config, "screen.width")?,
            config_u32(&config, "screen.height")?,
            config_u32(&config, "screen.availWidth")?,
            config_u32(&config, "screen.availHeight")?,
            config_i32(&config, "screen.availLeft")?,
            config_i32(&config, "screen.availTop")?,
            config_u16(&config, "screen.colorDepth")?,
            config_u16(&config, "screen.pixelDepth")?,
            config_dpr_milli(&config)?,
        )?,
        GraphicsIdentity::new(
            config_string(&config, "webGl:vendor")?,
            config_string(&config, "webGl:renderer")?,
            combined_config_digest(
                &config,
                "webGl:supportedExtensions",
                "webGl2:supportedExtensions",
            )?,
            canonical_value_sha256(required_config(&config, "webGl:parameters")?)?,
            canonical_value_sha256(required_config(&config, "webGl2:parameters")?)?,
            combined_config_digest(
                &config,
                "webGl:shaderPrecisionFormats",
                "webGl2:shaderPrecisionFormats",
            )?,
            combined_config_digest(
                &config,
                "webGl:contextAttributes",
                "webGl2:contextAttributes",
            )?,
        )?,
        FontIdentity::new(
            canonical_value_sha256(required_config(&config, "fonts")?)?,
            canonical_value_sha256(required_config(&config, "fonts:spacing_seed")?)?,
        )?,
        OriginDeterministicIdentity::new(
            OriginDeterminismMode::ProfileGenerationSeed,
            canonical_value_sha256(required_config(&config, "canvas:seed")?)?,
            canonical_value_sha256(required_config(&config, "audio:seed")?)?,
        )?,
        LocaleIdentity::new(
            config_string(&config, "navigator.language")?,
            canonical_value_sha256(required_config(&config, "navigator.languages")?)?,
            voices_sha256,
        )?,
    )
    .map_err(Into::into)
}

fn inventory_digest(entries: &[(String, u64, String)]) -> String {
    let mut canonical = String::from("[");
    for (index, (path, length, sha256)) in entries.iter().enumerate() {
        if index > 0 {
            canonical.push(',');
        }
        canonical.push_str(&format!(
            "{{\"length\":{length},\"path\":\"{path}\",\"sha256\":\"{sha256}\"}}"
        ));
    }
    canonical.push_str("]\n");
    sha256_hex(canonical.as_bytes())
}

fn approved_runtime(
    runtime_root: &Path,
) -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
    let real_path = runtime_root.join("camouhost/real.py");
    let lock_path = runtime_root.join("camouhost/runtime-lock.json");
    let (real_length, real_sha256) = digest_file(&real_path)?;
    let (lock_length, lock_sha256) = digest_file(&lock_path)?;
    let entries = [
        (
            "camouhost/real.py".to_owned(),
            real_length,
            real_sha256.clone(),
        ),
        (
            "camouhost/runtime-lock.json".to_owned(),
            lock_length,
            lock_sha256.clone(),
        ),
    ];
    let inventory_sha256 = inventory_digest(&entries);
    let manifest = RuntimeManifest::new(
        "2.0.0",
        "3.12",
        RuntimePlatform::WindowsX86_64,
        BundleRelativePath::parse("camouhost/real.py")?,
        Sha256Digest::parse(inventory_sha256.clone())?,
    )?;
    let inventory = RuntimeInventory::new([
        InventoryEntry::new(
            BundleRelativePath::parse("camouhost/real.py")?,
            real_length,
            Sha256Digest::parse(real_sha256)?,
        ),
        InventoryEntry::new(
            BundleRelativePath::parse("camouhost/runtime-lock.json")?,
            lock_length,
            Sha256Digest::parse(lock_sha256)?,
        ),
    ])?;
    Ok(ApprovedRuntimeBundle::validate(
        manifest,
        inventory,
        &Sha256Digest::parse(inventory_sha256)?,
    )?)
}

fn collect_symlink_paths(
    root: &Path,
    current: &Path,
    output: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.len() >= MAX_DIAGNOSTIC_SYMLINKS {
        return Ok(());
    }
    let mut children = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(fs::DirEntry::file_name);
    for child in children {
        if output.len() >= MAX_DIAGNOSTIC_SYMLINKS {
            break;
        }
        let path = child.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            output.push(relative);
        } else if metadata.is_dir() {
            collect_symlink_paths(root, &path, output)?;
        }
    }
    Ok(())
}

fn diagnostic_symlink_paths(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    collect_symlink_paths(root, root, &mut paths)?;
    Ok(paths)
}

fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(env::temp_dir().join(format!(
        "profile-bridge-ar10-real-runtime-{}-{nonce}",
        std::process::id()
    )))
}

#[test]
fn bridge_preflight_launches_real_camoufox_through_managed_ipc()
-> Result<(), Box<dyn std::error::Error>> {
    if env::var(ENABLE_ENV).as_deref() != Ok("1") {
        return Ok(());
    }

    let python = PathBuf::from(env::var(PYTHON_ENV)?).canonicalize()?;
    let runtime_root = PathBuf::from(env::var(RUNTIME_ROOT_ENV)?).canonicalize()?;
    let real_runtime = runtime_root.join("camouhost/real.py");
    let runtime_lock = runtime_root.join("camouhost/runtime-lock.json");
    let root_path = root_path()?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let tenant = TenantId::parse("tenant_01JAR10REAL")?;
    let profile = ProfileId::parse("profile_01JAR10REAL")?;
    let generation = GenerationId::parse("generation_01JAR10REAL")?;
    let device = DeviceId::parse("device_01JAR10REAL")?;
    let workspace = root.create_generation(&tenant, &profile, &generation)?;
    let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 9)?;

    let output = Command::new(&python)
        .arg(&real_runtime)
        .arg("--materialize-identity")
        .arg(workspace.path())
        .env("CAMOUHOST_RUNTIME_LOCK", &runtime_lock)
        .env("CAMOUHOST_HEADLESS_MODE", "virtual")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "real Camoufox candidate materialization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let report = String::from_utf8(output.stdout)?;
    let config_sha256 = json_field(&report, "fingerprint_config_sha256")?;
    let probe_sha256 = json_field(&report, "profile_stable_probe_sha256")?;
    let runtime_lock_sha256 = json_field(&report, "runtime_lock_sha256")?;
    let (_, actual_runtime_lock_sha256) = digest_file(&runtime_lock)?;
    if runtime_lock_sha256 != actual_runtime_lock_sha256 {
        return Err("candidate report runtime-lock identity drifted".into());
    }

    let symlinks = diagnostic_symlink_paths(workspace.path())?;
    if !symlinks.is_empty() {
        eprintln!("AR10_DIAGNOSTIC_SYMLINKS={symlinks:?}");
    }

    let bundle = approved_runtime(&runtime_root)?;
    let typed_identity =
        typed_identity_from_materialized_config(&workspace.path().join(CONFIG_NAME))?;
    let browser_identity = BrowserIdentityManifest::new(
        2,
        "profile-stability-v1",
        bundle.manifest().runtime_version(),
        bundle.manifest().inventory_sha256().as_str(),
        format!("profile-stability-v1-probe-{probe_sha256}"),
        config_sha256,
        typed_identity,
    )?;
    let binding = MaterializationBinding::new(
        tenant,
        profile,
        generation,
        "d".repeat(64),
        workspace.materialization_inventory_digest()?,
        browser_identity,
    )?;
    persist_materialization_binding(&workspace, &binding)?;
    let network_policy = NetworkIdentityPolicy::new(
        Some("PL".to_owned()),
        Some("Mazowieckie".to_owned()),
        Some("Europe/Warsaw".to_owned()),
        [NetworkClass::Mobile],
        [5617],
        Some("ar10-local-evidence".to_owned()),
    )?;
    let observation = NetworkIdentityObservation::new(
        "PL",
        "Mazowieckie",
        "Europe/Warsaw",
        NetworkClass::Mobile,
        5617,
        "ar10-local-evidence",
    )?;
    let slot = RuntimeBindingSlot::new();
    let mut preflight = RuntimeBindingBrowserLaunchPreflight::new(
        binding,
        network_policy,
        FixedObservation(observation),
        slot.clone(),
    );
    preflight.evaluate_before_launch(&workspace, &device, 9, &bundle)?;

    let config = ManagedCamouhostConfig::new(
        python,
        runtime_root,
        RuntimeDisplayMode::VirtualHeadful,
        None,
        None,
    )?;
    let (mut process, camouhost) = ManagedCamouhostProcess::pair(config, slot);
    let mut camouhost = StageTracingCamouhost::new(camouhost);
    let session = SessionId::parse("session_01JAR10REAL")?;
    RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost)?;
    RuntimeSessionOrchestrator::close(&bundle, &session, &mut process, &mut camouhost)?;

    lock.release()?;
    fs::remove_dir_all(&root_path)?;
    Ok(())
}
