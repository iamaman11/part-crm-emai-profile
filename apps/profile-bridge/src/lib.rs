#![forbid(unsafe_code)]

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

#[cfg(test)]
mod tests {
    use super::{FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore};
    use bridge_domain::{
        BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort,
        DeviceIdentityPort, DeviceKeyPort,
    };
    use profile_platform_primitives::{DeviceId, SessionId};

    #[test]
    fn fake_device_identity_and_key_handle_are_deterministic(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let device_id = DeviceId::parse("device_01JBRIDGE")?;
        let identity = FakeDeviceIdentity::new(device_id.clone());
        let mut keys = FakeDeviceKeyStore::default();
        assert_eq!(identity.device_id()?, device_id);
        let first = keys.ensure_key_handle(identity.device_id()?.borrow())?;
        let second = keys.ensure_key_handle(identity.device_id()?.borrow())?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn fake_camouhost_requires_version_negotiation_and_preserves_session(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    fn fake_camouhost_rejects_unsupported_version() {
        let mut runtime = FakeCamouhost::default();
        assert_eq!(
            runtime.exchange(&CamouhostMessage::Hello { version: 2 }),
            Err(BridgePortError::InvalidResponse)
        );
    }
}
