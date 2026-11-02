use crate::browser_execution::{BrowserLaunchBlocker, evaluate_browser_launch};
use crate::local_profile::GenerationWorkspace;
use crate::operator_flow::{BrowserLaunchPreflightPort, OperationalRejectionReason};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use browser_execution_domain::{
    MaterializationBinding, NetworkIdentityObservation, NetworkIdentityPolicy,
};
use profile_platform_primitives::DeviceId;
use serde_json::Value;
use std::fs;

const CAMOUFOX_CONFIG_FILE: &str = "camoufox-config.json";
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserRuntimeObservation {
    network: Option<NetworkIdentityObservation>,
    supervised_writer_active: bool,
}

impl BrowserRuntimeObservation {
    #[must_use]
    pub const fn new(network: NetworkIdentityObservation, supervised_writer_active: bool) -> Self {
        Self {
            network: Some(network),
            supervised_writer_active,
        }
    }

    #[must_use]
    pub const fn without_network_observation(supervised_writer_active: bool) -> Self {
        Self {
            network: None,
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
        BrowserLaunchBlocker::LocalProfile(crate::local_profile::LocalProfileError::Io(
            std::io::ErrorKind::NotFound,
        )) => OperationalRejectionReason::LocalCacheIncomplete,
        BrowserLaunchBlocker::LocalProfile(crate::local_profile::LocalProfileError::Io(
            std::io::ErrorKind::PermissionDenied,
        )) => OperationalRejectionReason::LocalCacheInaccessible,
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
        // An approved runtime may be repackaged (e.g. Camouhost diagnostics)
        // without changing the profile's browser identity contract. The full
        // inventory is verified by the runtime bundle owner, while the pinned
        // config, writer, network and browser-visible identity remain checked
        // by their respective launch owners. Keep the original runtime digest
        // in the generation identity as provenance; never rewrite it on Open.
        if browser_identity.runtime_version() != runtime_manifest.runtime_version() {
            return Err(BrowserLaunchBlocker::MaterializationStale);
        }

        validate_camoufox_config(workspace)?;

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
            observation.network.as_ref(),
            observation.supervised_writer_active,
        )
    }
}

fn validate_camoufox_config(
    workspace: &GenerationWorkspace,
) -> Result<(), BrowserLaunchBlocker> {
    // The official Camoufox/fpgen stack owns WebGL and all fingerprint values.
    // Preflight only rejects missing/malformed identity material before spawn.
    const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
    let path = workspace.path().join(CAMOUFOX_CONFIG_FILE);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    let bytes = fs::read(path).map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let config: Value = serde_json::from_slice(&bytes)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if !config.is_object() || config.as_object().is_none_or(serde_json::Map::is_empty) {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        BoundBrowserLaunchPreflight, BrowserRuntimeObservation, BrowserRuntimeObservationPort,
        CAMOUFOX_CONFIG_FILE, classify_browser_launch_blocker, validate_camoufox_config,
    };
    use crate::browser_execution::{
        BrowserLaunchBlocker, load_materialization_binding, persist_materialization_binding,
    };
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
    fn selected_runtime_and_config_integrity_are_validated_before_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JPREFLIGHT")?;
        let profile = ProfileId::parse("profile_01JPREFLIGHT")?;
        let generation = GenerationId::parse("generation_01JPREFLIGHT")?;
        let device = DeviceId::parse("device_01JPREFLIGHT")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 3)?;
        let canonical_config =
            r#"{"webGl:parameters":{"2928":[0.0,1.0],"3107":[true,true,true,true],"34467":null}}"#;
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
        let original_binding = binding.clone();
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

        // A maintenance release can change the signed runtime file inventory
        // without changing the browser identity/compatibility version. Reopen
        // must not rewrite the generation-owned identity or its local binding.
        preflight.evaluate_before_launch(
            &workspace,
            &device,
            3,
            &approved_bundle("0.1.0", 'f')?,
        )?;
        assert_eq!(calls.get(), 1);
        assert_eq!(
            load_materialization_binding(
                &workspace,
                original_binding.tenant_id(),
                original_binding.profile_id(),
                original_binding.generation_id(),
            )?,
            original_binding,
        );

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
        assert_eq!(calls.get(), 1);

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
        assert_eq!(calls.get(), 2);
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn the_official_fingerprint_payload_is_owned_by_camoufox() -> Result<(), Box<dyn std::error::Error>> {
        let path = root_path()?;
        let root = MaterializationRoot::open_or_create(&path)?;
        let tenant = TenantId::parse("tenant_01JPREFLIGHT")?;
        let profile = ProfileId::parse("profile_01JPREFLIGHT")?;
        let generation = GenerationId::parse("generation_01JPREFLIGHT")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        // Arbitrary official WebGL values are not re-interpreted by Rust.
        fs::write(
            workspace.path().join(CAMOUFOX_CONFIG_FILE),
            r#"{"webGl2:parameters":{"2928":[23,42],"unknown_from_future_fpgen":{"gpu":"native"}}}"#,
        )?;
        assert_eq!(validate_camoufox_config(&workspace), Ok(()));
        fs::write(workspace.path().join(CAMOUFOX_CONFIG_FILE), b"{")?;
        assert_eq!(
            validate_camoufox_config(&workspace),
            Err(BrowserLaunchBlocker::InvalidMaterializationEvidence)
        );
        fs::remove_file(workspace.path().join(CAMOUFOX_CONFIG_FILE))?;
        assert_eq!(
            validate_camoufox_config(&workspace),
            Err(BrowserLaunchBlocker::InvalidMaterializationEvidence)
        );
        remove_test_root(&path)?;
        Ok(())
    }
}
