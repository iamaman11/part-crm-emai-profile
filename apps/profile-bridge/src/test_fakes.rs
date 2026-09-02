use crate::ProcessControlPort;
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
    running: bool,
    actions: Vec<ProcessAction>,
}

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
