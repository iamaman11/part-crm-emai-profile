use crate::browser_execution::{BrowserLaunchBlocker, evaluate_browser_launch};
use crate::local_profile::GenerationWorkspace;
use crate::operator_flow::BrowserLaunchPreflightPort;
use crate::runtime_bundle::ApprovedRuntimeBundle;
use browser_execution_domain::{
    MaterializationBinding, NetworkIdentityObservation, NetworkIdentityPolicy,
};
use profile_platform_primitives::DeviceId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserRuntimeObservation {
    network: NetworkIdentityObservation,
    supervised_writer_active: bool,
}

impl BrowserRuntimeObservation {
    #[must_use]
    pub const fn new(
        network: NetworkIdentityObservation,
        supervised_writer_active: bool,
    ) -> Self {
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

impl<O> BrowserLaunchPreflightPort for BoundBrowserLaunchPreflight<O>
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
        let runtime_manifest = runtime_bundle.manifest();
        let browser_identity = self.expected.browser_identity();
        if browser_identity.runtime_version() != runtime_manifest.runtime_version()
            || browser_identity.runtime_inventory_sha256()
                != runtime_manifest.inventory_sha256().as_str()
        {
            return Err(BrowserLaunchBlocker::MaterializationStale);
        }

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

#[cfg(test)]
mod tests {
    use super::{
        BoundBrowserLaunchPreflight, BrowserRuntimeObservation, BrowserRuntimeObservationPort,
    };
    use crate::browser_execution::{
        BrowserLaunchBlocker, persist_materialization_binding,
    };
    use crate::local_profile::{BridgeWorkspaceLock, GenerationWorkspace, MaterializationRoot};
    use crate::operator_flow::BrowserLaunchPreflightPort;
    use crate::runtime_bundle::ApprovedRuntimeBundle;
    use crate::test_support::remove_test_root;
    use browser_execution_domain::{
        BrowserIdentityManifest, MaterializationBinding, NetworkClass, NetworkIdentityObservation,
        NetworkIdentityPolicy,
    };
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use runtime_bundle_domain::{
        BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
        Sha256Digest,
    };
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct FixedObservation {
        value: BrowserRuntimeObservation,
    }

    impl BrowserRuntimeObservationPort for FixedObservation {
        type Error = ();

        fn observe(
            &mut self,
            _workspace: &GenerationWorkspace,
            _device_id: &DeviceId,
        ) -> Result<BrowserRuntimeObservation, Self::Error> {
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
    fn selected_runtime_must_match_materialization_identity_before_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JPREFLIGHT")?;
        let profile = ProfileId::parse("profile_01JPREFLIGHT")?;
        let generation = GenerationId::parse("generation_01JPREFLIGHT")?;
        let device = DeviceId::parse("device_01JPREFLIGHT")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 3)?;
        let identity = BrowserIdentityManifest::new(
            1,
            "0.1.0",
            "a".repeat(64),
            "camoufox-v1",
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
        let mut preflight = BoundBrowserLaunchPreflight::new(
            binding,
            policy,
            FixedObservation {
                value: BrowserRuntimeObservation::new(network, false),
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
        preflight.evaluate_before_launch(
            &workspace,
            &device,
            3,
            &approved_bundle("0.1.0", 'a')?,
        )?;
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }
}
