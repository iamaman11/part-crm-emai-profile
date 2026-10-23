#![forbid(unsafe_code)]

use bridge_domain::{BridgePortError, CamouhostMessage, CamouhostPort};
use profile_bridge::browser_execution::load_materialization_binding;
use profile_bridge::camouhost_process::{
    ManagedCamouhostConfig, ManagedCamouhostProcess, RuntimeBindingSlot, RuntimeDisplayMode,
};
use profile_bridge::local_profile::{BridgeWorkspaceLock, MaterializationRoot};
use profile_bridge::operator_flow::BrowserLaunchPreflightPort;
use profile_bridge::runtime_bundle::{ApprovedRuntimeBundle, RuntimeSessionOrchestrator};
use profile_bridge::shipping_preflight::ShippingBrowserLaunchPreflight;
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, SessionId, TenantId};
#[cfg(not(windows))]
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

#[test]
fn published_windows_runtime_is_consumed_by_shipping_selector()
-> Result<(), Box<dyn std::error::Error>> {
    use profile_bridge::operator_flow::RuntimeBundleSelectionPort;
    use profile_bridge::runtime_bundle::FilesystemRuntimeBundleSelection;
    use profile_platform_primitives::{ActorContext, ActorId, CorrelationId, TenantScope};
    let Some(root) = env::var_os("PACKAGED_WINDOWS_RUNTIME_ROOT") else {
        return Ok(());
    };
    let actor = ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_packaged_runtime")?),
        ActorId::parse("actor_packaged_runtime")?,
        CorrelationId::parse("corr_packaged_runtime")?,
    );
    let mut selector = FilesystemRuntimeBundleSelection::open(PathBuf::from(root))?;
    let bundle = selector.select_bundle(
        &actor,
        &ProfileId::parse("profile_packaged_runtime")?,
        &GenerationId::parse("generation_packaged_runtime")?,
    )?;
    assert!(bundle.inventory().entries().len() >= 5);
    Ok(())
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

#[cfg(not(windows))]
fn digest_file(path: &Path) -> Result<(u64, String), Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("runtime evidence file is not regular".into());
    }
    let bytes = fs::read(path)?;
    Ok((metadata.len(), sha256_hex(&bytes)))
}

#[cfg(not(windows))]
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
    #[cfg(windows)]
    {
        use profile_bridge::operator_flow::RuntimeBundleSelectionPort;
        use profile_bridge::runtime_bundle::FilesystemRuntimeBundleSelection;
        use profile_platform_primitives::{ActorContext, ActorId, CorrelationId, TenantScope};
        let actor = ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JAR10REAL")?),
            ActorId::parse("actor_01JAR10REAL")?,
            CorrelationId::parse("corr_01JAR10REAL")?,
        );
        let mut selector = FilesystemRuntimeBundleSelection::open(runtime_root)?;
        Ok(selector.select_bundle(
            &actor,
            &ProfileId::parse("profile_01JAR10REAL")?,
            &GenerationId::parse("generation_01JAR10REAL")?,
        )?)
    }
    #[cfg(not(windows))]
    {
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
}

fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(env::temp_dir().join(format!(
        "profile-bridge-ar10-real-runtime-{}-{nonce}",
        std::process::id()
    )))
}

// Synthetic remote ports exercise the real native initializer and cipher without provider writes.
// The browser/materializer/runtime are real; this is not hosted B7 or device-pairing acceptance.
#[derive(Default)]
struct InitialObject {
    generation: Option<GenerationId>,
    container: Vec<u8>,
    verified: bool,
    committed: bool,
}
struct InitialControl(std::rc::Rc<std::cell::RefCell<InitialObject>>);
impl profile_bridge::shipping_control_plane::MachineHttpPort for InitialControl {
    type Error = BridgePortError;
    fn request(
        &mut self,
        _: profile_bridge::shipping_control_plane::MachineHttpMethod,
        _: &str,
        _: &profile_platform_primitives::CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<profile_bridge::shipping_control_plane::MachineHttpResponse, Self::Error> {
        use control_plane_contract::generation_initialization_api::{
            InitialGenerationCommitted, InitialGenerationPrepared,
            InitialGenerationRequest as Request,
        };
        use control_plane_contract::generation_key_api::{
            BridgeGenerationSealingMaterialResponse, GENERATION_SEALING_CHUNK_BYTES,
        };
        use control_plane_contract::profile_generation_api::BridgeGenerationUploadCapabilityResponse;
        let error = || BridgePortError::InvalidResponse;
        let request =
            serde_json::from_slice::<Request>(body.ok_or_else(error)?).map_err(|_| error())?;
        let mut object = self.0.borrow_mut();
        let response = match request {
            Request::Prepare { generation_id, expected_profile_version } => {
                object.generation = Some(GenerationId::parse(&generation_id).map_err(|_| error())?);
                serde_json::to_vec(&InitialGenerationPrepared::Initialize { generation_id, expected_profile_version })
            }
            Request::SealingMaterial {..} => serde_json::to_vec(&BridgeGenerationSealingMaterialResponse::new("profile-generation-root-v1-2", "ab".repeat(32), "cd".repeat(16), GENERATION_SEALING_CHUNK_BYTES)),
            Request::UploadCapability { metadata_digest, container_digest, container_bytes, .. } => {
                if object.container.is_empty() {
                    serde_json::to_vec(&BridgeGenerationUploadCapabilityResponse::upload_required("https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/test/initial?X-Amz-Signature=TEST", &[("if-none-match".to_owned(), "*".to_owned())], 300))
                } else {
                    let inspected = encrypted_generation_domain::inspect_generation_metadata_prelude(&object.container).map_err(|_| error())?;
                    let actual_metadata = inspected.metadata_digest().bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
                    if actual_metadata != metadata_digest || sha256_hex(&object.container) != container_digest || u64::try_from(object.container.len()).ok() != Some(container_bytes) || inspected.metadata().base_generation_id().is_some() { return Err(error()); }
                    let generation = object.generation.as_ref().ok_or_else(error)?;
                    let dek = encrypted_generation_domain::GenerationDek::new(encrypted_generation_domain::KeyId::parse("profile-generation-root-v1-2").map_err(|_| error())?, [0xab; 32]);
                    let opened = encrypted_generation_domain::open_generation_expected(&object.container, &dek, &TenantId::parse("tenant_01JAR10REAL").map_err(|_| error())?, &ProfileId::parse("profile_01JAR10REAL").map_err(|_| error())?, generation).map_err(|_| error())?;
                    if !opened.plaintext().starts_with(b"BPGW0001") { return Err(error()); }
                    object.verified = true;
                    serde_json::to_vec(&BridgeGenerationUploadCapabilityResponse::verified())
                }
            }
            Request::Commit { generation_id, expected_profile_version, container_digest, .. } => {
                if !object.verified || sha256_hex(&object.container) != container_digest { return Err(error()); }
                object.committed = true;
                serde_json::to_vec(&InitialGenerationCommitted { generation_id, profile_version: expected_profile_version + 1, replayed: false,
                    launch: control_plane_contract::profile_launch_api::ProfileLaunchProjection { launch_uri: "profilebridge://claim/claim_01JAR10_INITIAL_CONFIRMED".to_owned(), expires_at_ms: 1000 } })
            }
        }.map_err(|_| error())?;
        Ok(profile_bridge::shipping_control_plane::MachineHttpResponse::new(200, response))
    }
}
struct InitialUpload(std::rc::Rc<std::cell::RefCell<InitialObject>>);
impl profile_bridge::shipping_generation_save::SignedGenerationObjectPutPort for InitialUpload {
    type Error = BridgePortError;
    fn put_exact(
        &mut self,
        capability: &profile_bridge::shipping_generation_save::SignedGenerationUploadCapability,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        if capability
            .headers()
            .map_err(|_| BridgePortError::InvalidResponse)?
            != vec![("if-none-match", "*")]
            || !self.0.borrow().container.is_empty()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.0.borrow_mut().container = bytes.to_vec();
        Ok(())
    }
}
struct InitialBundle(ApprovedRuntimeBundle);
impl profile_bridge::operator_flow::RuntimeBundleSelectionPort for InitialBundle {
    type Error = BridgePortError;
    fn select_bundle(
        &mut self,
        _: &profile_platform_primitives::ActorContext,
        _: &ProfileId,
        _: &GenerationId,
    ) -> Result<ApprovedRuntimeBundle, Self::Error> {
        Ok(self.0.clone())
    }
}

fn run_managed_cold_launch_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let python = PathBuf::from(env::var(PYTHON_ENV)?).canonicalize()?;
    let runtime_root = PathBuf::from(env::var(RUNTIME_ROOT_ENV)?).canonicalize()?;
    let root_path = root_path()?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let tenant = TenantId::parse("tenant_01JAR10REAL")?;
    let profile = ProfileId::parse("profile_01JAR10REAL")?;
    let device = DeviceId::parse("device_01JAR10REAL")?;
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
    let intent = profile_bridge::generation_initialization::InitialProfileOpen::parse(
        "profilebridge://open/v1/tenant_01JAR10REAL/actor_01JAR10REAL/profile_01JAR10REAL/1",
    )?;
    let actor = profile_platform_primitives::ActorContext::new(
        profile_platform_primitives::TenantScope::new(tenant.clone()),
        profile_platform_primitives::ActorId::parse("actor_01JAR10REAL")?,
        profile_platform_primitives::CorrelationId::parse("corr_01JAR10REAL")?,
    );
    let object = std::rc::Rc::new(std::cell::RefCell::new(InitialObject::default()));
    profile_bridge::generation_initialization::initialize(
        &intent,
        &actor,
        &device,
        &root,
        |workspace, bundle| {
            config
                .materialize_initial_identity(workspace, bundle)
                .map_err(|_| {
                    profile_bridge::generation_initialization::InitialProfileError::Runtime
                })
        },
        &mut InitialBundle(bundle.clone()),
        &mut InitialControl(object.clone()),
        &mut InitialUpload(object.clone()),
    )
    .map_err(|error| format!("initial profile preparation failed at {error:?}"))?;
    let evidence = object.borrow();
    if !evidence.committed || !evidence.verified {
        return Err("initial generation was not confirmed".into());
    }
    let generation = evidence
        .generation
        .clone()
        .ok_or("missing committed initial generation")?;
    let workspace = root.open_generation(&tenant, &profile, &generation)?;
    let binding = load_materialization_binding(&workspace, &tenant, &profile, &generation)?;
    if binding.source_container_sha256() != sha256_hex(&evidence.container) {
        return Err("initial materialization lost exact confirmed container identity".into());
    }
    drop(evidence);
    let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 9)?;

    let slot = RuntimeBindingSlot::new();
    let mut preflight = ShippingBrowserLaunchPreflight::new(slot.clone());
    preflight.evaluate_before_launch(&workspace, &device, 9, &bundle)?;

    let config = ManagedCamouhostConfig::new(python, runtime_root, display_mode, None, None)?;
    let (mut process, camouhost) = ManagedCamouhostProcess::pair(config, slot);
    let mut camouhost = StageTracingCamouhost::new(camouhost);
    let session = SessionId::parse("session_01JAR10REAL")?;
    RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost)?;
    RuntimeSessionOrchestrator::close(&bundle, &session, &mut process, &mut camouhost)?;

    #[cfg(windows)]
    assert_eq!(
        approved_runtime(&PathBuf::from(env::var(RUNTIME_ROOT_ENV)?))?,
        bundle
    );

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
