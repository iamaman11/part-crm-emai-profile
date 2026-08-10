#![forbid(unsafe_code)]

pub mod browser_execution;
pub mod browser_mail_query;
pub mod browser_mail_runtime;
pub mod browser_preflight;
pub mod dirty_generation;
pub mod dirty_generation_commit;
pub mod dirty_generation_publish;
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

#[derive(Clone, Debug, Default)]
pub struct FakeDeviceKeyStore {
    available: bool,
}

impl FakeDeviceKeyStore {
    pub fn set_available(&mut self, available: bool) {
        self.available = available;
    }
}

impl DeviceKeyPort for FakeDeviceKeyStore {
    fn load_private_key(&mut self) -> Result<(), BridgePortError> {
        if self.available {
            Ok(())
        } else {
            self.available = true;
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessAction {
    Spawn,
    GracefulClose,
    ForceTerminate,
}

pub trait ProcessControlPort {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError>;
}

#[derive(Clone, Debug, Default)]
pub struct FakeProcessControl {
    actions: BTreeMap<SessionId, Vec<ProcessAction>>,
    stopped: BTreeMap<SessionId, bool>,
}

impl FakeProcessControl {
    #[must_use]
    pub fn actions(&self, session_id: &SessionId) -> &[ProcessAction] {
        self.actions
            .get(session_id)
            .map_or(&[], std::vec::Vec::as_slice)
    }
}

impl ProcessControlPort for FakeProcessControl {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        self.actions
            .entry(session_id.clone())
            .or_default()
            .push(ProcessAction::Spawn);
        self.stopped.insert(session_id.clone(), false);
        Ok(())
    }

    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        self.actions
            .entry(session_id.clone())
            .or_default()
            .push(ProcessAction::GracefulClose);
        Ok(())
    }

    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        if self.stopped.get(session_id).copied().unwrap_or(false) {
            Ok(())
        } else {
            Err(BridgePortError::Unavailable)
        }
    }

    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        self.actions
            .entry(session_id.clone())
            .or_default()
            .push(ProcessAction::ForceTerminate);
        self.stopped.insert(session_id.clone(), true);
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct FakeCamouhost {
    sessions: BTreeMap<SessionId, bool>,
}

impl CamouhostPort for FakeCamouhost {
    fn exchange(&mut self, message: &CamouhostMessage) -> Result<CamouhostMessage, BridgePortError> {
        match message {
            CamouhostMessage::Hello { version } if *version == CAMOUHOST_IPC_VERSION => {
                Ok(CamouhostMessage::HelloAck { version: *version })
            }
            CamouhostMessage::Hello { .. } => Err(BridgePortError::InvalidResponse),
            CamouhostMessage::Open { session_id } => {
                self.sessions.insert(session_id.clone(), true);
                Ok(CamouhostMessage::Opened {
                    session_id: session_id.clone(),
                })
            }
            CamouhostMessage::Close { session_id } => {
                self.sessions.insert(session_id.clone(), false);
                Ok(CamouhostMessage::Closed {
                    session_id: session_id.clone(),
                })
            }
            CamouhostMessage::HelloAck { .. }
            | CamouhostMessage::Opened { .. }
            | CamouhostMessage::Closed { .. } => Err(BridgePortError::InvalidResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FakeCamouhost, FakeProcessControl, ProcessAction, ProcessControlPort};
    use bridge_domain::{CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort};
    use profile_platform_primitives::SessionId;

    #[test]
    fn fake_camouhost_rejects_unsupported_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut runtime = FakeCamouhost::default();
        let unsupported = CAMOUHOST_IPC_VERSION.saturating_add(1);
        assert_eq!(
            runtime.exchange(&CamouhostMessage::Hello {
                version: unsupported,
            }),
            Err(bridge_domain::BridgePortError::InvalidResponse)
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
            process.actions(&session_id),
            [
                ProcessAction::Spawn,
                ProcessAction::GracefulClose,
                ProcessAction::ForceTerminate,
            ]
        );
        Ok(())
    }
}
