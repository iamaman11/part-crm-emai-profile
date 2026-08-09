#![forbid(unsafe_code)]

pub mod browser_execution;
pub mod browser_mail_query;
pub mod browser_preflight;
pub mod fake_mail_query;
pub mod local_profile;
pub mod operator_flow;
pub mod runtime_bundle;

#[cfg(test)]
mod test_support;

#[cfg(windows)]
pub mod windows_native;

use bridge_domain::{
    BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort, DeviceIdentityPort,
    DeviceKeyPort,
};
use profile_platform_primitives::{DeviceId, SessionId};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeDeviceIdentity {
    device_id: DeviceId,
}

impl FakeDeviceIdentity {
    #[must_use]
    pub const fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }
}

impl DeviceIdentityPort for FakeDeviceIdentity {
    fn device_id(&self) -> Result<DeviceId, BridgePortError> {
        Ok(self.device_id.clone())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeDeviceKeyStore {
    handles: BTreeMap<DeviceId, String>,
}

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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeCamouhost {
    negotiated: bool,
    active_session: Option<SessionId>,
}

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
    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessAction {
    Spawn(SessionId),
    GracefulClose(SessionId),
    ConfirmStopped(SessionId),
    ForceTerminate(SessionId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeProcessControl {
    active_session: Option<SessionId>,
    actions: Vec<ProcessAction>,
}

impl FakeProcessControl {
    #[must_use]
    pub fn actions(&self) -> &[ProcessAction] {
        &self.actions
    }
}

impl ProcessControlPort for FakeProcessControl {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.active_session.is_some() {
            return Err(BridgePortError::Unavailable);
        }
        self.active_session = Some(session_id.clone());
        self.actions.push(ProcessAction::Spawn(session_id.clone()));
        Ok(())
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
        self.active_session = None;
        Ok(())
    }

    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        self.actions
            .push(ProcessAction::ForceTerminate(session_id.clone()));
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
        process.request_graceful_close(&session_id)?;
        process.force_terminate(&session_id)?;
        assert_eq!(
            process.actions(),
            [
                ProcessAction::Spawn(session_id.clone()),
                ProcessAction::GracefulClose(session_id),
                ProcessAction::ForceTerminate(
                    SessionId::parse("session_01JPROCESS")?
                )
            ]
        );
        Ok(())
    }
}
