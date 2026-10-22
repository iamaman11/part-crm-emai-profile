#![forbid(unsafe_code)]

use bridge_domain::{BridgePortError, CamouhostMessage, CamouhostPort};
use browser_execution_domain::MaterializationBinding;
use profile_bridge::browser_execution::persist_materialization_binding;
use profile_bridge::camouhost_process::{
    ManagedCamouhostConfig, ManagedCamouhostProcess, RuntimeBindingSlot, RuntimeDisplayMode,
};
use profile_bridge::local_profile::{BridgeWorkspaceLock, MaterializationRoot};
use profile_bridge::operator_flow::BrowserLaunchPreflightPort;
use profile_bridge::runtime_bundle::{ApprovedRuntimeBundle, RuntimeSessionOrchestrator};
use profile_bridge::shipping_preflight::ShippingBrowserLaunchPreflight;
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, SessionId, TenantId};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ENABLE_ENV: &str = "AR10_REAL_CAMOUFOX";
const PYTHON_ENV: &str = "AR10_PYTHON";
const RUNTIME_ROOT_ENV: &str = "AR10_RUNTIME_ROOT";
const HEADLESS_ENV: &str = "AR10_REAL_CAMOUFOX_HEADLESS";
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
const MANAGED_COLD_LAUNCH_ATTEMPTS: usize = 3;

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
            Err(error) => eprintln!("AR10_MANAGED_IPC_STAGE={stage};ERROR={error:?}"),
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

fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(env::temp_dir().join(format!(
        "profile-bridge-ar10-real-runtime-{}-{nonce}",
        std::process::id()
    )))
}

fn run_managed_cold_launch_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let python = PathBuf::from(env::var(PYTHON_ENV)?).canonicalize()?;
    let runtime_root = PathBuf::from(env::var(RUNTIME_ROOT_ENV)?).canonicalize()?;
    let root_path = root_path()?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let tenant = TenantId::parse("tenant_01JAR10REAL")?;
    let profile = ProfileId::parse("profile_01JAR10REAL")?;
    let generation = GenerationId::parse("generation_01JAR10REAL")?;
    let device = DeviceId::parse("device_01JAR10REAL")?;
    let workspace = root.create_generation(&tenant, &profile, &generation)?;
    let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 9)?;
    let headless = env::var(HEADLESS_ENV).unwrap_or_else(|_| "virtual".to_owned());
    let display_mode = match headless.as_str() {
        "false" => RuntimeDisplayMode::Headful,
        "virtual" => RuntimeDisplayMode::VirtualHeadful,
        _ => return Err(format!("{HEADLESS_ENV} must be exactly false or virtual").into()),
    };

    let bundle = approved_runtime(&runtime_root)?;
    let config = ManagedCamouhostConfig::new(
        python.clone(),
        runtime_root.clone(),
        display_mode,
        None,
        None,
    )?;
    let browser_identity = config.materialize_initial_identity(&workspace, &bundle)?;

    let binding = MaterializationBinding::new(
        tenant,
        profile,
        generation,
        "d".repeat(64),
        workspace.materialization_inventory_digest()?,
        browser_identity,
    )?;
    persist_materialization_binding(&workspace, &binding)?;

    let slot = RuntimeBindingSlot::new();
    let mut preflight = ShippingBrowserLaunchPreflight::new(slot.clone());
    preflight.evaluate_before_launch(&workspace, &device, 9, &bundle)?;

    let config = ManagedCamouhostConfig::new(python, runtime_root, display_mode, None, None)?;
    let (mut process, camouhost) = ManagedCamouhostProcess::pair(config, slot);
    let mut camouhost = StageTracingCamouhost::new(camouhost);
    let session = SessionId::parse("session_01JAR10REAL")?;
    RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost)?;
    RuntimeSessionOrchestrator::close(&bundle, &session, &mut process, &mut camouhost)?;

    lock.release()?;
    fs::remove_dir_all(&root_path)?;
    Ok(())
}

#[test]
fn bridge_preflight_launches_real_camoufox_through_managed_ipc()
-> Result<(), Box<dyn std::error::Error>> {
    if env::var(ENABLE_ENV).as_deref() != Ok("1") {
        return Ok(());
    }

    for _ in 0..MANAGED_COLD_LAUNCH_ATTEMPTS {
        run_managed_cold_launch_cycle()?;
    }
    Ok(())
}
