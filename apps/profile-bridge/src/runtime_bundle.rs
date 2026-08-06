use crate::ProcessControlPort;
use bridge_domain::{
    BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort,
};
use profile_platform_primitives::SessionId;
use runtime_bundle_domain::{
    InventoryError, RuntimeInventory, RuntimeManifest, RuntimeManifestError, Sha256Digest,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovedRuntimeBundle {
    manifest: RuntimeManifest,
    inventory: RuntimeInventory,
}

impl ApprovedRuntimeBundle {
    pub fn validate(
        manifest: RuntimeManifest,
        inventory: RuntimeInventory,
        calculated_inventory_sha256: &Sha256Digest,
    ) -> Result<Self, RuntimeBundleApprovalError> {
        manifest
            .validate_inventory_digest(calculated_inventory_sha256)
            .map_err(RuntimeBundleApprovalError::Manifest)?;
        inventory
            .validate_entrypoint(&manifest)
            .map_err(RuntimeBundleApprovalError::Inventory)?;
        Ok(Self {
            manifest,
            inventory,
        })
    }

    #[must_use]
    pub const fn manifest(&self) -> &RuntimeManifest {
        &self.manifest
    }

    #[must_use]
    pub const fn inventory(&self) -> &RuntimeInventory {
        &self.inventory
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeBundleApprovalError {
    Manifest(RuntimeManifestError),
    Inventory(InventoryError),
}

impl fmt::Display for RuntimeBundleApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => error.fmt(formatter),
            Self::Inventory(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RuntimeBundleApprovalError {}

pub struct RuntimeSessionOrchestrator;

impl RuntimeSessionOrchestrator {
    pub fn launch<P, C>(
        _bundle: &ApprovedRuntimeBundle,
        session_id: &SessionId,
        process: &mut P,
        camouhost: &mut C,
    ) -> Result<(), RuntimeLaunchError>
    where
        P: ProcessControlPort,
        C: CamouhostPort,
    {
        process
            .spawn(session_id)
            .map_err(RuntimeLaunchError::Process)?;

        let hello = camouhost
            .exchange(&CamouhostMessage::Hello {
                version: CAMOUHOST_IPC_VERSION,
            })
            .map_err(|error| rollback_process(process, session_id, error))?;
        if hello
            != (CamouhostMessage::HelloAck {
                version: CAMOUHOST_IPC_VERSION,
            })
        {
            return Err(rollback_process(
                process,
                session_id,
                BridgePortError::InvalidResponse,
            ));
        }

        let ready = camouhost
            .exchange(&CamouhostMessage::Launch {
                session_id: session_id.clone(),
            })
            .map_err(|error| rollback_process(process, session_id, error))?;
        if ready
            != (CamouhostMessage::Ready {
                session_id: session_id.clone(),
            })
        {
            return Err(rollback_process(
                process,
                session_id,
                BridgePortError::InvalidResponse,
            ));
        }
        Ok(())
    }

    pub fn close<P, C>(
        _bundle: &ApprovedRuntimeBundle,
        session_id: &SessionId,
        process: &mut P,
        camouhost: &mut C,
    ) -> Result<(), RuntimeLaunchError>
    where
        P: ProcessControlPort,
        C: CamouhostPort,
    {
        process
            .request_graceful_close(session_id)
            .map_err(RuntimeLaunchError::Process)?;
        let closed = camouhost
            .exchange(&CamouhostMessage::Close {
                session_id: session_id.clone(),
            })
            .map_err(RuntimeLaunchError::Camouhost)?;
        if closed
            != (CamouhostMessage::Closed {
                session_id: session_id.clone(),
                clean: true,
            })
        {
            return Err(RuntimeLaunchError::Camouhost(
                BridgePortError::InvalidResponse,
            ));
        }
        Ok(())
    }
}

fn rollback_process<P: ProcessControlPort>(
    process: &mut P,
    session_id: &SessionId,
    source: BridgePortError,
) -> RuntimeLaunchError {
    match process.force_terminate(session_id) {
        Ok(()) => RuntimeLaunchError::Camouhost(source),
        Err(rollback) => RuntimeLaunchError::Rollback {
            source,
            rollback,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLaunchError {
    Process(BridgePortError),
    Camouhost(BridgePortError),
    Rollback {
        source: BridgePortError,
        rollback: BridgePortError,
    },
}

impl fmt::Display for RuntimeLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Process(error) => write!(formatter, "runtime process error: {error}"),
            Self::Camouhost(error) => write!(formatter, "Camouhost protocol error: {error}"),
            Self::Rollback { source, rollback } => write!(
                formatter,
                "Camouhost protocol error: {source}; process rollback failed: {rollback}"
            ),
        }
    }
}

impl std::error::Error for RuntimeLaunchError {}

#[cfg(test)]
mod tests {
    use super::{
        ApprovedRuntimeBundle, RuntimeBundleApprovalError, RuntimeSessionOrchestrator,
    };
    use crate::{FakeCamouhost, FakeProcessControl, ProcessAction};
    use profile_platform_primitives::SessionId;
    use runtime_bundle_domain::{
        BundleRelativePath, InventoryEntry, InventoryError, RuntimeInventory, RuntimeManifest,
        RuntimeManifestError, RuntimePlatform, Sha256Digest,
    };

    fn digest(character: char) -> Result<Sha256Digest, Box<dyn std::error::Error>> {
        Ok(Sha256Digest::parse(character.to_string().repeat(64))?)
    }

    fn approved_bundle() -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
        let calculated = digest('a')?;
        let entrypoint = BundleRelativePath::parse("camouhost/main.py")?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(
            entrypoint,
            10,
            digest('b')?,
        )])?;
        Ok(ApprovedRuntimeBundle::validate(
            manifest,
            inventory,
            &calculated,
        )?)
    }

    #[test]
    fn digest_mismatch_is_rejected_before_process_spawn(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let expected = digest('a')?;
        let entrypoint = BundleRelativePath::parse("camouhost/main.py")?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            expected,
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(
            entrypoint,
            10,
            digest('b')?,
        )])?;
        let result = ApprovedRuntimeBundle::validate(manifest, inventory, &digest('c')?);
        assert_eq!(
            result,
            Err(RuntimeBundleApprovalError::Manifest(
                RuntimeManifestError::InventoryDigestMismatch
            ))
        );
        let process = FakeProcessControl::default();
        assert!(process.actions().is_empty());
        Ok(())
    }

    #[test]
    fn missing_entrypoint_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let calculated = digest('a')?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse("camouhost/main.py")?,
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(
            BundleRelativePath::parse("camouhost/other.py")?,
            10,
            digest('b')?,
        )])?;
        assert_eq!(
            ApprovedRuntimeBundle::validate(manifest, inventory, &calculated),
            Err(RuntimeBundleApprovalError::Inventory(
                InventoryError::EntrypointMissing
            ))
        );
        Ok(())
    }

    #[test]
    fn approved_bundle_launches_and_closes_exact_session(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bundle = approved_bundle()?;
        let session_id = SessionId::parse("session_01JSTEP7RUNTIME")?;
        let mut process = FakeProcessControl::default();
        let mut camouhost = FakeCamouhost::default();
        RuntimeSessionOrchestrator::launch(
            &bundle,
            &session_id,
            &mut process,
            &mut camouhost,
        )?;
        RuntimeSessionOrchestrator::close(
            &bundle,
            &session_id,
            &mut process,
            &mut camouhost,
        )?;
        assert_eq!(
            process.actions(),
            [
                ProcessAction::Spawn(session_id.clone()),
                ProcessAction::GracefulClose(session_id),
            ]
        );
        Ok(())
    }
}
