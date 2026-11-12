#![forbid(unsafe_code)]

pub mod authoritative_generation;
pub mod browser_execution;
pub mod browser_mail_query;
pub mod browser_mail_runtime;
#[cfg(any(test, feature = "synthetic-test-bin"))]
pub mod browser_preflight;
pub mod device_pairing;
pub mod dirty_close;
pub mod dirty_generation;
#[cfg(any(test, feature = "synthetic-test-bin"))]
pub mod dirty_generation_finalize;
#[cfg(any(test, feature = "synthetic-test-bin"))]
mod dirty_generation_local;
#[cfg(any(test, feature = "synthetic-test-bin"))]
pub mod dirty_generation_publish;
pub mod fake_mail_query;
pub mod generation_initialization;
pub mod generation_object_spool;
pub mod generation_reopen;
mod generation_snapshot;
pub mod installed_connection;
pub mod launch_binding;
#[cfg(windows)]
pub mod local_browser_open;
pub mod local_profile;
pub mod local_profile_delete;
pub mod operator_flow;
pub mod runtime_bundle;
pub mod shipping_composition;
pub mod shipping_control_plane;
pub mod shipping_generation_save;
pub mod shipping_generation_successor_control;
#[cfg(any(test, feature = "synthetic-test-bin"))]
pub mod shipping_network;
#[cfg(any(windows, test, feature = "synthetic-test-bin"))]
mod shipping_save_attempt;
pub mod user_diagnostics;
pub mod windows_delivery;
pub mod windows_delivery_archive;
pub mod windows_delivery_coordinator;
pub mod windows_delivery_download;
pub mod windows_delivery_handoff;
pub mod windows_delivery_install;
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
pub mod windows_device_application;
#[cfg(windows)]
mod windows_device_pairing;
#[cfg(windows)]
pub mod windows_generation_get;
#[cfg(windows)]
pub mod windows_generation_put;
#[cfg(windows)]
pub mod windows_native;

#[cfg(any(test, feature = "synthetic-test-bin"))]
use bridge_domain::BridgePortError;
#[cfg(any(test, feature = "synthetic-test-bin"))]
use profile_platform_primitives::SessionId;

#[cfg(any(test, feature = "synthetic-test-bin"))]
pub trait ProcessControlPort {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn is_running(&mut self, session_id: &SessionId) -> Result<bool, BridgePortError>;
    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    /// Whether a failed stop must preserve a still-running user-owned browser.
    /// Disposable isolated probes may be terminated; ordinary user browsers may not.
    fn preserve_live_browser_on_failure(&self) -> bool {
        false
    }
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
