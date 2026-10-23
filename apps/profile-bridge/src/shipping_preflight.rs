mod camoufox_identity;

pub use camoufox_identity::profile_stable_identity_from_config_bytes;

use crate::browser_execution::{BrowserLaunchBlocker, load_materialization_binding};
use crate::browser_preflight::classify_browser_launch_blocker;
use crate::camouhost_process::{RuntimeBindingBrowserLaunchPreflight, RuntimeBindingSlot};
use crate::local_profile::GenerationWorkspace;
use crate::operator_flow::{BrowserLaunchPreflightPort, OperationalRejectionReason};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use crate::shipping_network::GenerationNetworkEvidence;
use camoufox_identity::{CamoufoxIdentityProjectionError, verify_materialized_camoufox_identity};
use profile_platform_primitives::DeviceId;

/// Shipping adapter that resolves the exact generation binding only after redemption has selected
/// the server-authorized workspace. Policy/evaluation remain owned by the existing browser
/// preflight; this adapter only bridges the late-bound filesystem identity into that owner.
pub struct ShippingBrowserLaunchPreflight {
    runtime_binding_slot: RuntimeBindingSlot,
}

impl ShippingBrowserLaunchPreflight {
    #[must_use]
    pub const fn new(runtime_binding_slot: RuntimeBindingSlot) -> Self {
        Self {
            runtime_binding_slot,
        }
    }
}

impl BrowserLaunchPreflightPort for ShippingBrowserLaunchPreflight {
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
        let expected = load_materialization_binding(
            workspace,
            workspace.tenant_id(),
            workspace.profile_id(),
            workspace.generation_id(),
        )?;
        verify_materialized_camoufox_identity(workspace.path(), expected.browser_identity())
            .map_err(map_identity_projection_error)?;
        let observations = GenerationNetworkEvidence::open(workspace)
            .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
        let mut inner = RuntimeBindingBrowserLaunchPreflight::new(
            expected,
            observations.policy(),
            observations,
            self.runtime_binding_slot.clone(),
        );
        inner.evaluate_before_launch(workspace, device_id, workspace_epoch, runtime_bundle)
    }
}

fn map_identity_projection_error(error: CamoufoxIdentityProjectionError) -> BrowserLaunchBlocker {
    match error {
        CamoufoxIdentityProjectionError::InvalidMaterialization => {
            BrowserLaunchBlocker::InvalidMaterializationEvidence
        }
        CamoufoxIdentityProjectionError::IdentityMismatch => {
            BrowserLaunchBlocker::MaterializationStale
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::local_profile::MaterializationRoot;
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "profile-bridge-shipping-preflight-{}-{nonce}",
            std::process::id()
        )))
    }

    #[test]
    fn materialization_root_path_recovers_exact_typed_generation_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JSHIPPINGPREFLIGHT")?;
        let profile = ProfileId::parse("profile_01JSHIPPINGPREFLIGHT")?;
        let generation = GenerationId::parse("generation_01JSHIPPINGPREFLIGHT")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;

        assert_eq!(workspace.tenant_id(), &tenant);
        assert_eq!(workspace.profile_id(), &profile);
        assert_eq!(workspace.generation_id(), &generation);

        std::fs::remove_dir_all(root_path)?;
        Ok(())
    }
}
