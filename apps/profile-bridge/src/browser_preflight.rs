use crate::browser_execution::{BrowserLaunchBlocker, evaluate_browser_launch};
use crate::local_profile::GenerationWorkspace;
use crate::operator_flow::{BrowserLaunchPreflightPort, OperationalRejectionReason};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use browser_execution_domain::{
    MaterializationBinding, NetworkIdentityObservation, NetworkIdentityPolicy,
};
use profile_platform_primitives::DeviceId;
use serde_json::{Map, Value};
use std::fs;

const CAMOUFOX_CONFIG_FILE: &str = "camoufox-config.json";
const WEBGL_PARAMETER_MAPS: [&str; 2] = ["webGl:parameters", "webGl2:parameters"];

#[derive(Clone, Copy)]
enum WebGlValueContract {
    F32Array(usize),
    I32Array(usize),
    BoolArray(usize),
    U32Vector,
}

const WEBGL_ARRAY_CONTRACTS: [(&str, WebGlValueContract); 10] = [
    ("2928", WebGlValueContract::F32Array(2)),
    ("33901", WebGlValueContract::F32Array(2)),
    ("33902", WebGlValueContract::F32Array(2)),
    ("3106", WebGlValueContract::F32Array(4)),
    ("32773", WebGlValueContract::F32Array(4)),
    ("3386", WebGlValueContract::I32Array(2)),
    ("3088", WebGlValueContract::I32Array(4)),
    ("2978", WebGlValueContract::I32Array(4)),
    ("3107", WebGlValueContract::BoolArray(4)),
    ("34467", WebGlValueContract::U32Vector),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserRuntimeObservation {
    network: NetworkIdentityObservation,
    supervised_writer_active: bool,
}

impl BrowserRuntimeObservation {
    #[must_use]
    pub const fn new(network: NetworkIdentityObservation, supervised_writer_active: bool) -> Self {
        Self {
            network,
            supervised_writer_active,
        }
    }
}

pub trait BrowserRuntimeObservationPort {
    type Error;

    fn observe(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
    ) -> Result<BrowserRuntimeObservation, Self::Error>;
}

pub struct BoundBrowserLaunchPreflight<O> {
    expected: MaterializationBinding,
    network_policy: NetworkIdentityPolicy,
    observations: O,
}

impl<O> BoundBrowserLaunchPreflight<O> {
    #[must_use]
    pub const fn new(
        expected: MaterializationBinding,
        network_policy: NetworkIdentityPolicy,
        observations: O,
    ) -> Self {
        Self {
            expected,
            network_policy,
            observations,
        }
    }
}

pub(crate) fn classify_browser_launch_blocker(
    error: &BrowserLaunchBlocker,
) -> OperationalRejectionReason {
    match error {
        BrowserLaunchBlocker::MaterializationStale => {
            OperationalRejectionReason::RuntimeCandidateMismatch
        }
        BrowserLaunchBlocker::InvalidMaterializationEvidence => {
            OperationalRejectionReason::ConfigIntegrityMismatch
        }
        BrowserLaunchBlocker::NetworkPolicyMismatch => {
            OperationalRejectionReason::IdentityObservationMismatch
        }
        BrowserLaunchBlocker::ProfileBusy => OperationalRejectionReason::ProfileBusy,
        BrowserLaunchBlocker::RecoveryRequired => OperationalRejectionReason::RecoveryRequired,
        BrowserLaunchBlocker::RetryableNetworkRouteChurn => {
            OperationalRejectionReason::RetryableNetworkRouteChurn
        }
        BrowserLaunchBlocker::LocalProfile(_) => {
            OperationalRejectionReason::FilesystemProcessCapabilityUnavailable
        }
    }
}

impl<O> BrowserLaunchPreflightPort for BoundBrowserLaunchPreflight<O>
where
    O: BrowserRuntimeObservationPort,
{
    type Error = BrowserLaunchBlocker;

    fn operational_rejection_reason(error: &Self::Error) -> OperationalRejectionReason {
        classify_browser_launch_blocker(error)
    }

    fn evaluate_before_launch(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        workspace_epoch: u64,
        runtime_bundle: &ApprovedRuntimeBundle,
    ) -> Result<(), Self::Error> {
        let runtime_manifest = runtime_bundle.manifest();
        let browser_identity = self.expected.browser_identity();
        if browser_identity.runtime_version() != runtime_manifest.runtime_version()
            || browser_identity.runtime_inventory_sha256()
                != runtime_manifest.inventory_sha256().as_str()
        {
            return Err(BrowserLaunchBlocker::MaterializationStale);
        }

        validate_camoufox_webgl_config(workspace)?;

        let observation = self
            .observations
            .observe(workspace, device_id)
            .map_err(|_| BrowserLaunchBlocker::RecoveryRequired)?;
        evaluate_browser_launch(
            workspace,
            device_id,
            workspace_epoch,
            &self.expected,
            &self.network_policy,
            &observation.network,
            observation.supervised_writer_active,
        )
    }
}

fn validate_camoufox_webgl_config(
    workspace: &GenerationWorkspace,
) -> Result<(), BrowserLaunchBlocker> {
    let path = workspace.path().join(CAMOUFOX_CONFIG_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    let bytes = fs::read(path).map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let config: Value = serde_json::from_slice(&bytes)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    validate_camoufox_webgl_value(&config)
}

fn validate_camoufox_webgl_value(config: &Value) -> Result<(), BrowserLaunchBlocker> {
    let root = config
        .as_object()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    for map_name in WEBGL_PARAMETER_MAPS {
        let Some(raw_parameters) = root.get(map_name) else {
            continue;
        };
        let parameters = raw_parameters
            .as_object()
            .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
        validate_known_webgl_parameters(parameters)?;
    }
    Ok(())
}

fn validate_known_webgl_parameters(
    parameters: &Map<String, Value>,
) -> Result<(), BrowserLaunchBlocker> {
    for (pname, contract) in WEBGL_ARRAY_CONTRACTS {
        let Some(raw) = parameters.get(pname) else {
            continue;
        };
        match contract {
            WebGlValueContract::F32Array(length) => {
                validate_fixed_array(raw, length, validate_f32)?;
            }
            WebGlValueContract::I32Array(length) => {
                validate_fixed_array(raw, length, validate_i32)?;
            }
            WebGlValueContract::BoolArray(length) => {
                validate_fixed_array(raw, length, validate_bool)?;
            }
            WebGlValueContract::U32Vector => {
                let values = raw
                    .as_array()
                    .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
                if !values.iter().all(validate_u32) {
                    return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
                }
            }
        }
    }
    Ok(())
}

fn validate_fixed_array(
    raw: &Value,
    expected_length: usize,
    validate_item: fn(&Value) -> bool,
) -> Result<(), BrowserLaunchBlocker> {
    let values = raw
        .as_array()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if values.len() != expected_length || !values.iter().all(validate_item) {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    Ok(())
}

fn validate_f32(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && number.abs() <= f64::from(f32::MAX))
}

fn validate_i32(value: &Value) -> bool {
    value
        .as_i64()
        .is_some_and(|number| i32::try_from(number).is_ok())
        || value
            .as_u64()
            .is_some_and(|number| number <= i32::MAX as u64)
}

fn validate_bool(value: &Value) -> bool {
    value.is_boolean()
}

fn validate_u32(value: &Value) -> bool {
    value
        .as_u64()
        .is_some_and(|number| u32::try_from(number).is_ok())
}

#[cfg(test)]
mod tests {
    use super::{
        BoundBrowserLaunchPreflight, BrowserRuntimeObservation, BrowserRuntimeObservationPort,
        CAMOUFOX_CONFIG_FILE, classify_browser_launch_blocker, validate_camoufox_webgl_value,
    };
    use crate::browser_execution::{BrowserLaunchBlocker, persist_materialization_binding};
    use crate::local_profile::{BridgeWorkspaceLock, GenerationWorkspace, MaterializationRoot};
    use crate::operator_flow::{BrowserLaunchPreflightPort, OperationalRejectionReason};
    use crate::runtime_bundle::ApprovedRuntimeBundle;
    use crate::test_support::{browser_identity_fixture, remove_test_root};
    use browser_execution_domain::{
        MaterializationBinding, NetworkClass, NetworkIdentityObservation, NetworkIdentityPolicy,
    };
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use runtime_bundle_domain::{
        BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
        Sha256Digest,
    };
    use serde_json::json;
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct FixedObservation {
        value: BrowserRuntimeObservation,
        calls: Rc<Cell<u32>>,
    }

    impl BrowserRuntimeObservationPort for FixedObservation {
        type Error = ();

        fn observe(
            &mut self,
            _workspace: &GenerationWorkspace,
            _device_id: &DeviceId,
        ) -> Result<BrowserRuntimeObservation, Self::Error> {
            self.calls.set(self.calls.get().saturating_add(1));
            Ok(self.value.clone())
        }
    }

    fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "profile-bridge-preflight-{}-{nonce}",
            std::process::id()
        )))
    }

    fn digest(character: char) -> Result<Sha256Digest, Box<dyn std::error::Error>> {
        Ok(Sha256Digest::parse(character.to_string().repeat(64))?)
    }

    fn approved_bundle(
        runtime_version: &str,
        inventory_digest: char,
    ) -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
        let calculated = digest(inventory_digest)?;
        let entrypoint = BundleRelativePath::parse("camouhost/main.py")?;
        let manifest = RuntimeManifest::new(
            runtime_version,
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(entrypoint, 10, digest('d')?)])?;
        Ok(ApprovedRuntimeBundle::validate(
            manifest,
            inventory,
            &calculated,
        )?)
    }

    #[test]
    fn operational_rejection_categories_are_stable_and_non_secret() {
        let cases = [
            (
                BrowserLaunchBlocker::MaterializationStale,
                OperationalRejectionReason::RuntimeCandidateMismatch,
                "runtime_candidate_mismatch",
            ),
            (
                BrowserLaunchBlocker::InvalidMaterializationEvidence,
                OperationalRejectionReason::ConfigIntegrityMismatch,
                "config_integrity_mismatch",
            ),
            (
                BrowserLaunchBlocker::NetworkPolicyMismatch,
                OperationalRejectionReason::IdentityObservationMismatch,
                "identity_observation_mismatch",
            ),
            (
                BrowserLaunchBlocker::ProfileBusy,
                OperationalRejectionReason::ProfileBusy,
                "profile_busy",
            ),
            (
                BrowserLaunchBlocker::RecoveryRequired,
                OperationalRejectionReason::RecoveryRequired,
                "recovery_required",
            ),
            (
                BrowserLaunchBlocker::RetryableNetworkRouteChurn,
                OperationalRejectionReason::RetryableNetworkRouteChurn,
                "retryable_network_route_churn",
            ),
        ];
        for (blocker, expected, code) in cases {
            let observed = classify_browser_launch_blocker(&blocker);
            assert_eq!(observed, expected);
            assert_eq!(observed.code(), code);
        }
    }

    #[test]
    fn selected_runtime_and_webgl_config_are_validated_before_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JPREFLIGHT")?;
        let profile = ProfileId::parse("profile_01JPREFLIGHT")?;
        let generation = GenerationId::parse("generation_01JPREFLIGHT")?;
        let device = DeviceId::parse("device_01JPREFLIGHT")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 3)?;
        let canonical_config = r#"{"webGl:parameters":{"2928":[0.0,1.0],"3107":[true,true,true,true],"34467":[33776,33777]}}"#;
        fs::write(
            workspace.path().join(CAMOUFOX_CONFIG_FILE),
            canonical_config,
        )?;
        let identity = browser_identity_fixture(
            "0.1.0",
            "a".repeat(64),
            "profile-stability-v1-probe-test",
            "b".repeat(64),
        )?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.inventory()?.inventory_digest(),
            identity,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        let policy = NetworkIdentityPolicy::new(
            Some("PL".to_owned()),
            Some("Mazowieckie".to_owned()),
            Some("Europe/Warsaw".to_owned()),
            [NetworkClass::Mobile],
            [5617],
            Some("route-a".to_owned()),
        )?;
        let network = NetworkIdentityObservation::new(
            "PL",
            "Mazowieckie",
            "Europe/Warsaw",
            NetworkClass::Mobile,
            5617,
            "route-a",
        )?;
        let calls = Rc::new(Cell::new(0));
        let mut preflight = BoundBrowserLaunchPreflight::new(
            binding,
            policy,
            FixedObservation {
                value: BrowserRuntimeObservation::new(network, false),
                calls: Rc::clone(&calls),
            },
        );

        assert_eq!(
            preflight.evaluate_before_launch(
                &workspace,
                &device,
                3,
                &approved_bundle("0.2.0", 'a')?,
            ),
            Err(BrowserLaunchBlocker::MaterializationStale)
        );
        assert_eq!(calls.get(), 0);

        fs::write(
            workspace.path().join(CAMOUFOX_CONFIG_FILE),
            r#"{"webGl:parameters":{"2928":[NaN,1.0]}}"#,
        )?;
        assert_eq!(
            preflight.evaluate_before_launch(
                &workspace,
                &device,
                3,
                &approved_bundle("0.1.0", 'a')?,
            ),
            Err(BrowserLaunchBlocker::InvalidMaterializationEvidence)
        );
        assert_eq!(calls.get(), 0);

        fs::write(
            workspace.path().join(CAMOUFOX_CONFIG_FILE),
            canonical_config,
        )?;
        preflight.evaluate_before_launch(
            &workspace,
            &device,
            3,
            &approved_bundle("0.1.0", 'a')?,
        )?;
        assert_eq!(calls.get(), 1);
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn patched_webgl_array_contract_rejects_shape_type_range_and_vector_drift() {
        let invalid = [
            json!({"webGl:parameters": {"2928": [0.0]}}),
            json!({"webGl:parameters": {"3107": [true, true, true, 1]}}),
            json!({"webGl:parameters": {"3386": [2_147_483_648_u64, 1]}}),
            json!({"webGl:parameters": {"2928": [3.5e38, 1.0]}}),
            json!({"webGl:parameters": {"34467": null}}),
            json!({"webGl:parameters": {"34467": 33776}}),
            json!({"webGl:parameters": {"34467": {"unexpected": 33776}}}),
            json!({"webGl:parameters": {"34467": [-1, 4_294_967_296_u64]}}),
            json!({"webGl2:parameters": {"3088": [0, 0, 1]}}),
        ];
        for config in invalid {
            assert_eq!(
                validate_camoufox_webgl_value(&config),
                Err(BrowserLaunchBlocker::InvalidMaterializationEvidence)
            );
        }
    }

    #[test]
    fn patched_webgl_array_contract_accepts_exact_native_shapes() {
        let config = json!({
            "webGl:parameters": {
                "2928": [0.0, 1.0],
                "33901": [1.0, 8192.0],
                "33902": [1.0, 1.0],
                "3106": [0.0, 0.0, 0.0, 0.0],
                "32773": [0.0, 0.0, 0.0, 0.0],
                "3386": [1920, 1080],
                "3088": [0, 0, 1920, 1080],
                "2978": [0, 0, 1920, 1080],
                "3107": [true, true, true, true],
                "34467": [33776, 33777]
            },
            "webGl2:parameters": {
                "2928": [0.0, 1.0],
                "34467": []
            }
        });
        assert_eq!(validate_camoufox_webgl_value(&config), Ok(()));
    }
}
