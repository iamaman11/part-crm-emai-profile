#![forbid(unsafe_code)]

pub mod authoritative_generation;
pub mod browser_execution;
pub mod browser_mail_query;
pub mod browser_mail_runtime;
pub mod browser_preflight;
pub mod camouhost_process;
pub mod dirty_close;
pub mod dirty_generation;
#[cfg(any(test, feature = "synthetic-test-bin"))]
pub mod dirty_generation_finalize;
#[cfg(any(test, feature = "synthetic-test-bin"))]
mod dirty_generation_local;
#[cfg(any(test, feature = "synthetic-test-bin"))]
pub mod dirty_generation_publish;
pub mod fake_mail_query;
pub mod generation_reopen;
mod generation_snapshot;
pub mod launch_binding;
pub mod local_profile;
pub mod operator_flow;
pub mod runtime_bundle;
pub mod shipping_composition;
pub mod shipping_control_plane;
pub mod shipping_generation_save;
pub mod shipping_generation_successor_control;
pub mod shipping_network;
pub mod shipping_preflight;
pub mod windows_delivery;
pub mod windows_delivery_archive;
pub mod windows_delivery_coordinator;
pub mod windows_delivery_download;
pub mod windows_delivery_handoff;
pub mod windows_delivery_recovery;
#[cfg(windows)]
pub mod windows_delivery_runtime;
#[cfg(windows)]
pub mod windows_delivery_signature;
pub mod windows_delivery_staging;
pub mod windows_delivery_store;

#[cfg(test)]
mod operator_p3_reopen_e2e_tests;
#[cfg(test)]
mod operator_p3_save_tests;
#[cfg(test)]
mod shipping_control_plane_p3_tests;
#[cfg(test)]
mod test_support;

#[cfg(any(test, feature = "synthetic-test-bin"))]
mod test_fakes;
#[cfg(any(test, feature = "synthetic-test-bin"))]
pub use test_fakes::{
    FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl, ProcessAction,
};

#[cfg(windows)]
pub mod windows_generation_put;
#[cfg(windows)]
pub mod windows_native;

use bridge_domain::BridgePortError;
use profile_platform_primitives::SessionId;

pub trait ProcessControlPort {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn is_running(&mut self, session_id: &SessionId) -> Result<bool, BridgePortError>;
    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
}

#[cfg(test)]
mod tests {
    use super::{
        FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl, ProcessAction,
        ProcessControlPort,
    };
    use bridge_domain::{
        BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort,
        DeviceIdentityPort, DeviceKeyPort,
    };
    use profile_platform_primitives::{DeviceId, SessionId};

    #[test]
    fn fake_device_identity_and_key_handle_are_deterministic()
    -> Result<(), Box<dyn std::error::Error>> {
        let device_id = DeviceId::parse("device_01JBRIDGE")?;
        let identity = FakeDeviceIdentity::new(device_id.clone());
        let mut keys = FakeDeviceKeyStore::default();
        let resolved = identity.device_id()?;
        assert_eq!(resolved, device_id);
        let first = keys.ensure_key_handle(&resolved)?;
        let second = keys.ensure_key_handle(&resolved)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn fake_camouhost_requires_version_negotiation_and_preserves_session()
    -> Result<(), Box<dyn std::error::Error>> {
        let session_id = SessionId::parse("session_01JBRIDGE")?;
        let mut runtime = FakeCamouhost::default();
        assert_eq!(
            runtime.exchange(&CamouhostMessage::Launch {
                session_id: session_id.clone(),
            }),
            Err(BridgePortError::InvalidResponse)
        );
        assert_eq!(
            runtime.exchange(&CamouhostMessage::Hello {
                version: CAMOUHOST_IPC_VERSION,
            })?,
            CamouhostMessage::HelloAck {
                version: CAMOUHOST_IPC_VERSION,
            }
        );
        assert_eq!(
            runtime.exchange(&CamouhostMessage::Launch {
                session_id: session_id.clone(),
            })?,
            CamouhostMessage::Ready {
                session_id: session_id.clone(),
            }
        );
        assert_eq!(
            runtime.exchange(&CamouhostMessage::ObserveClose {
                session_id: session_id.clone(),
            })?,
            CamouhostMessage::CloseObserved {
                session_id: session_id.clone(),
                controlled: false,
            }
        );
        assert_eq!(
            runtime.exchange(&CamouhostMessage::Close {
                session_id: session_id.clone(),
            })?,
            CamouhostMessage::Closed {
                session_id,
                clean: true,
            }
        );
        Ok(())
    }

    #[test]
    fn fake_camouhost_rejects_unsupported_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut runtime = FakeCamouhost::default();
        let unsupported = CAMOUHOST_IPC_VERSION.saturating_add(1);
        assert_eq!(
            runtime.exchange(&CamouhostMessage::Hello {
                version: unsupported,
            }),
            Err(BridgePortError::InvalidResponse)
        );
        Ok(())
    }

    #[test]
    fn fake_process_control_records_graceful_and_forced_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        let session_id = SessionId::parse("session_01JPROCESS")?;
        let mut process = FakeProcessControl::default();
        process.spawn(&session_id)?;
        assert!(process.is_running(&session_id)?);
        process.request_graceful_close(&session_id)?;
        process.force_terminate(&session_id)?;
        assert_eq!(
            process.actions(),
            [
                ProcessAction::Spawn(session_id.clone()),
                ProcessAction::GracefulClose(session_id),
                ProcessAction::ForceTerminate(SessionId::parse("session_01JPROCESS")?)
            ]
        );
        Ok(())
    }
}
