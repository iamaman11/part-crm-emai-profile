use crate::browser_execution::{BrowserLaunchBlocker, load_materialization_binding};
use crate::browser_preflight::{BrowserRuntimeObservation, BrowserRuntimeObservationPort};
use crate::camouhost_process::{RuntimeBindingBrowserLaunchPreflight, RuntimeBindingSlot};
use crate::local_profile::GenerationWorkspace;
use crate::operator_flow::BrowserLaunchPreflightPort;
use crate::runtime_bundle::ApprovedRuntimeBundle;
use browser_execution_domain::NetworkIdentityPolicy;
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
use std::path::Path;

/// Shipping adapter that resolves the exact generation binding only after redemption has selected
/// the server-authorized workspace. Policy/evaluation remain owned by the existing browser
/// preflight; this adapter only bridges the late-bound filesystem identity into that owner.
pub struct ShippingBrowserLaunchPreflight<O> {
    network_policy: NetworkIdentityPolicy,
    observations: O,
    runtime_binding_slot: RuntimeBindingSlot,
}

impl<O> ShippingBrowserLaunchPreflight<O> {
    #[must_use]
    pub const fn new(
        network_policy: NetworkIdentityPolicy,
        observations: O,
        runtime_binding_slot: RuntimeBindingSlot,
    ) -> Self {
        Self {
            network_policy,
            observations,
            runtime_binding_slot,
        }
    }
}

impl<O> BrowserLaunchPreflightPort for ShippingBrowserLaunchPreflight<O>
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
        let identity = workspace_identity(workspace.path())?;
        let expected = load_materialization_binding(
            workspace,
            &identity.tenant_id,
            &identity.profile_id,
            &identity.generation_id,
        )?;
        let mut inner = RuntimeBindingBrowserLaunchPreflight::new(
            expected,
            self.network_policy.clone(),
            BorrowedObservation(&mut self.observations),
            self.runtime_binding_slot.clone(),
        );
        inner.evaluate_before_launch(workspace, device_id, workspace_epoch, runtime_bundle)
    }
}

struct WorkspaceIdentity {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
}

fn workspace_identity(path: &Path) -> Result<WorkspaceIdentity, BrowserLaunchBlocker> {
    let generation = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let profile_path = path
        .parent()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let profile = profile_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let tenant_path = profile_path
        .parent()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let tenant = tenant_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;

    Ok(WorkspaceIdentity {
        tenant_id: TenantId::parse(tenant.to_owned())
            .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?,
        profile_id: ProfileId::parse(profile.to_owned())
            .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?,
        generation_id: GenerationId::parse(generation.to_owned())
            .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?,
    })
}

struct BorrowedObservation<'a, O>(&'a mut O);

impl<O> BrowserRuntimeObservationPort for BorrowedObservation<'_, O>
where
    O: BrowserRuntimeObservationPort,
{
    type Error = O::Error;

    fn observe(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
    ) -> Result<BrowserRuntimeObservation, Self::Error> {
        self.0.observe(workspace, device_id)
    }
}

#[cfg(test)]
mod tests {
    use super::workspace_identity;
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

        let identity = workspace_identity(workspace.path())?;
        assert_eq!(identity.tenant_id, tenant);
        assert_eq!(identity.profile_id, profile);
        assert_eq!(identity.generation_id, generation);

        std::fs::remove_dir_all(root_path)?;
        Ok(())
    }
}
