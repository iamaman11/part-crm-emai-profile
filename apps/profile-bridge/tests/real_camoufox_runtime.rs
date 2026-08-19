#![forbid(unsafe_code)]

use bridge_domain::{ClaimUri, CamouhostPort};
use browser_execution_domain::{
    BrowserIdentityManifest, MaterializationBinding, NetworkClass, NetworkIdentityObservation,
    NetworkIdentityPolicy,
};
use profile_bridge::browser_execution::persist_materialization_binding;
use profile_bridge::browser_preflight::{
    BrowserRuntimeObservation, BrowserRuntimeObservationPort,
};
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

struct FixedObservation(NetworkIdentityObservation);

impl BrowserRuntimeObservationPort for FixedObservation {
    type Error = bridge_domain::BridgePortError;

    fn observe(
        &mut self,
        _workspace: &profile_bridge::local_profile::GenerationWorkspace,
        _device_id: &DeviceId,
    ) -> Result<BrowserRuntimeObservation, Self::Error> {
        Ok(BrowserRuntimeObservation::new(self.0.clone(), false))
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
    let entries = vec![
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

    let bundle = approved_runtime(&runtime_root)?;
    let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 9)?;
    let browser_identity = BrowserIdentityManifest::new(
        2,
        bundle.manifest().runtime_version(),
        bundle.manifest().inventory_sha256().as_str(),
        format!("profile-stability-v1-probe-{probe_sha256}"),
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
        Some("about:blank".to_owned()),
        None,
    )?;
    let (mut process, mut camouhost) = ManagedCamouhostProcess::pair(config, slot);
    let session = SessionId::parse("session_01JAR10REAL")?;
    RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost)?;
    RuntimeSessionOrchestrator::close(&bundle, &session, &mut process, &mut camouhost)?;

    lock.release()?;
    fs::remove_dir_all(&root_path)?;
    let _claim_parser_linkage = ClaimUri::parse("profilebridge://claim/claim_01JAR10REAL")?;
    let _typed_ipc_linkage: &dyn CamouhostPort = &camouhost;
    Ok(())
}
