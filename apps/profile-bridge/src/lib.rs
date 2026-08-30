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

#[cfg(windows)]
pub mod windows_generation_put;
#[cfg(windows)]
pub mod windows_native;

use bridge_domain::BridgePortError;
#[cfg(any(test, feature = "synthetic-test-bin"))]
use bridge_domain::{
    CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort, DeviceIdentityPort, DeviceKeyPort,
};
#[cfg(any(test, feature = "synthetic-test-bin"))]
use profile_platform_primitives::DeviceId;
use profile_platform_primitives::SessionId;
#[cfg(any(test, feature = "synthetic-test-bin"))]
use std::collections::BTreeMap;

#[cfg(any(test, feature = "synthetic-test-bin"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeDeviceIdentity {
    device_id: DeviceId,
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
impl FakeDeviceIdentity {
    #[must_use]
    pub const fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
impl DeviceIdentityPort for FakeDeviceIdentity {
    fn device_id(&self) -> Result<DeviceId, BridgePortError> {
        Ok(self.device_id.clone())
    }
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeDeviceKeyStore {
    handles: BTreeMap<DeviceId, String>,
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
impl DeviceKeyPort for FakeDeviceKeyStore {
    fn ensure_key_handle(&mut self, device_id: &DeviceId) -> Result<String, BridgePortError> {
        if let Some(handle) = self.handles.get(device_id) {
            return Ok(handle.clone());
        }
        let handle = format!("fake_key_handle_{}", device_id.as_str());
        self.handles.insert(device_id.clone(), handle.clone());
        Ok(handle)
    }
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeCamouhost {
    negotiated: bool,
    active_session: Option<SessionId>,
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
impl CamouhostPort for FakeCamouhost {
    fn exchange(
        &mut self,
        message: &CamouhostMessage,
    ) -> Result<CamouhostMessage, BridgePortError> {
        match message {
            CamouhostMessage::Hello { version } if *version == CAMOUHOST_IPC_VERSION => {
                self.negotiated = true;
                Ok(CamouhostMessage::HelloAck {
                    version: CAMOUHOST_IPC_VERSION,
                })
            }
            CamouhostMessage::Launch { session_id }
                if self.negotiated && self.active_session.is_none() =>
            {
                self.active_session = Some(session_id.clone());
                Ok(CamouhostMessage::Ready {
                    session_id: session_id.clone(),
                })
            }
            CamouhostMessage::ObserveClose { session_id }
                if self.active_session.as_ref() == Some(session_id) =>
            {
                Ok(CamouhostMessage::CloseObserved {
                    session_id: session_id.clone(),
                    controlled: false,
                })
            }
            CamouhostMessage::Close { session_id }
                if self.active_session.as_ref() == Some(session_id) =>
            {
                self.active_session = None;
                Ok(CamouhostMessage::Closed {
                    session_id: session_id.clone(),
                    clean: true,
                })
            }
            _ => Err(BridgePortError::InvalidResponse),
        }
    }
}

pub trait ProcessControlPort {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn is_running(&mut self, session_id: &SessionId) -> Result<bool, BridgePortError>;
    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessAction {
    Spawn(SessionId),
    GracefulClose(SessionId),
    ConfirmStopped(SessionId),
    ForceTerminate(SessionId),
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeProcessControl {
    active_session: Option<SessionId>,
    running: bool,
    actions: Vec<ProcessAction>,
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
impl FakeProcessControl {
    #[must_use]
    pub fn actions(&self) -> &[ProcessAction] {
        &self.actions
    }

    pub fn simulate_exit(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        self.running = false;
        Ok(())
    }
}

#[cfg(any(test, feature = "synthetic-test-bin"))]
impl ProcessControlPort for FakeProcessControl {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.active_session.is_some() {
            return Err(BridgePortError::Unavailable);
        }
        self.active_session = Some(session_id.clone());
        self.running = true;
        self.actions.push(ProcessAction::Spawn(session_id.clone()));
        Ok(())
    }

    fn is_running(&mut self, session_id: &SessionId) -> Result<bool, BridgePortError> {
        if self.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(self.running)
    }

    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        self.actions
            .push(ProcessAction::GracefulClose(session_id.clone()));
        Ok(())
    }

    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        self.actions
            .push(ProcessAction::ConfirmStopped(session_id.clone()));
        self.running = false;
        self.active_session = None;
        Ok(())
    }

    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        self.actions
            .push(ProcessAction::ForceTerminate(session_id.clone()));
        self.running = false;
        self.active_session = None;
        Ok(())
    }
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
