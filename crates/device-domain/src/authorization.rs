use core::fmt;
use profile_platform_primitives::{ActorId, DeviceId, TenantId, UnixMillis};

use crate::DEVICE_PROOF_NONCE_BYTES;

const DEVICE_PROOF_CONTEXT_V1: &[u8] = b"part-crm/device-proof/v1\0";

/// Canonical application proof message signed by the device's non-exportable P-256 key.
///
/// Length-prefixed tenant/actor/device identifiers prevent ambiguous concatenation. The challenge
/// expiry is part of the signed message so a nonce cannot be detached from its bounded lifetime.
pub fn device_proof_message_v1(
    tenant_id: &TenantId,
    actor_id: &ActorId,
    device_id: &DeviceId,
    nonce: &[u8],
    expires_at: UnixMillis,
) -> Result<Vec<u8>, DeviceProofMessageError> {
    if nonce.len() != DEVICE_PROOF_NONCE_BYTES {
        return Err(DeviceProofMessageError::InvalidNonceLength);
    }

    let mut message = Vec::with_capacity(
        DEVICE_PROOF_CONTEXT_V1.len()
            + 2
            + tenant_id.as_str().len()
            + 2
            + actor_id.as_str().len()
            + 2
            + device_id.as_str().len()
            + 8
            + DEVICE_PROOF_NONCE_BYTES,
    );
    message.extend_from_slice(DEVICE_PROOF_CONTEXT_V1);
    append_identifier(&mut message, tenant_id.as_str())?;
    append_identifier(&mut message, actor_id.as_str())?;
    append_identifier(&mut message, device_id.as_str())?;
    message.extend_from_slice(&expires_at.value().to_be_bytes());
    message.extend_from_slice(nonce);
    Ok(message)
}

fn append_identifier(
    message: &mut Vec<u8>,
    value: &str,
) -> Result<(), DeviceProofMessageError> {
    let length = u16::try_from(value.len())
        .map_err(|_| DeviceProofMessageError::InvalidIdentifierLength)?;
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(value.as_bytes());
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceProofMessageError {
    InvalidNonceLength,
    InvalidIdentifierLength,
}

impl fmt::Display for DeviceProofMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidNonceLength => "device proof nonce length is invalid",
            Self::InvalidIdentifierLength => "device proof identifier length is invalid",
        })
    }
}

impl std::error::Error for DeviceProofMessageError {}

/// Revocable short-lived application session bound to one user epoch and one device binding
/// version. Persistence may use an opaque session handle, but authorization always rechecks these
/// current values before the session can be renewed or used as application authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceApplicationSession {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
    user_auth_epoch: u64,
    device_binding_version: u64,
    issued_at: UnixMillis,
    expires_at: UnixMillis,
    revoked_at: Option<UnixMillis>,
}

impl DeviceApplicationSession {
    pub fn issue(
        tenant_id: TenantId,
        actor_id: ActorId,
        device_id: DeviceId,
        user_auth_epoch: u64,
        device_binding_version: u64,
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Self, DeviceApplicationSessionError> {
        if user_auth_epoch == 0 || device_binding_version == 0 {
            return Err(DeviceApplicationSessionError::InvalidEpoch);
        }
        if expires_at <= issued_at {
            return Err(DeviceApplicationSessionError::InvalidTimeline);
        }
        Ok(Self {
            tenant_id,
            actor_id,
            device_id,
            user_auth_epoch,
            device_binding_version,
            issued_at,
            expires_at,
            revoked_at: None,
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn user_auth_epoch(&self) -> u64 {
        self.user_auth_epoch
    }

    #[must_use]
    pub const fn device_binding_version(&self) -> u64 {
        self.device_binding_version
    }

    #[must_use]
    pub const fn issued_at(&self) -> UnixMillis {
        self.issued_at
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn revoked_at(&self) -> Option<UnixMillis> {
        self.revoked_at
    }

    pub fn revoke(&mut self, now: UnixMillis) -> Result<(), DeviceApplicationSessionError> {
        if now < self.issued_at {
            return Err(DeviceApplicationSessionError::InvalidTimeline);
        }
        if self.revoked_at.is_none() {
            self.revoked_at = Some(now);
        }
        Ok(())
    }

    pub fn require_authorized(
        &self,
        user_enabled: bool,
        current_user_auth_epoch: u64,
        device_enabled: bool,
        current_device_binding_version: u64,
        now: UnixMillis,
    ) -> Result<(), DeviceApplicationSessionError> {
        if now < self.issued_at {
            return Err(DeviceApplicationSessionError::InvalidTimeline);
        }
        if now >= self.expires_at {
            return Err(DeviceApplicationSessionError::Expired);
        }
        if self.revoked_at.is_some() {
            return Err(DeviceApplicationSessionError::Revoked);
        }
        if !user_enabled {
            return Err(DeviceApplicationSessionError::UserDisabled);
        }
        if !device_enabled {
            return Err(DeviceApplicationSessionError::DeviceDisabled);
        }
        if current_user_auth_epoch == 0 || current_user_auth_epoch != self.user_auth_epoch {
            return Err(DeviceApplicationSessionError::StaleUserAuthEpoch);
        }
        if current_device_binding_version == 0
            || current_device_binding_version != self.device_binding_version
        {
            return Err(DeviceApplicationSessionError::StaleDeviceBinding);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceApplicationSessionError {
    InvalidEpoch,
    InvalidTimeline,
    Expired,
    Revoked,
    UserDisabled,
    DeviceDisabled,
    StaleUserAuthEpoch,
    StaleDeviceBinding,
}

impl fmt::Display for DeviceApplicationSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEpoch => "device application session epoch is invalid",
            Self::InvalidTimeline => "device application session timeline is invalid",
            Self::Expired => "device application session expired",
            Self::Revoked => "device application session was revoked",
            Self::UserDisabled => "device application session user is disabled",
            Self::DeviceDisabled => "device application session device is disabled",
            Self::StaleUserAuthEpoch => "device application session user auth epoch is stale",
            Self::StaleDeviceBinding => "device application session device binding is stale",
        })
    }
}

impl std::error::Error for DeviceApplicationSessionError {}

#[cfg(test)]
mod tests {
    use super::{
        DEVICE_PROOF_CONTEXT_V1, DeviceApplicationSession, DeviceApplicationSessionError,
        DeviceProofMessageError, device_proof_message_v1,
    };
    use crate::DEVICE_PROOF_NONCE_BYTES;
    use profile_platform_primitives::{ActorId, DeviceId, TenantId, UnixMillis};

    fn ids() -> Result<(TenantId, ActorId, DeviceId), Box<dyn std::error::Error>> {
        Ok((
            TenantId::parse("tenant_01JSESSION")?,
            ActorId::parse("actor_01JSESSION")?,
            DeviceId::parse("device_01JSESSION")?,
        ))
    }

    #[test]
    fn proof_message_binds_context_actor_device_expiry_and_exact_nonce()
    -> Result<(), Box<dyn std::error::Error>> {
        let (tenant, actor, device) = ids()?;
        let nonce = [0x5a; DEVICE_PROOF_NONCE_BYTES];
        let message = device_proof_message_v1(
            &tenant,
            &actor,
            &device,
            &nonce,
            UnixMillis::new(9_999),
        )?;
        assert!(message.starts_with(DEVICE_PROOF_CONTEXT_V1));
        assert!(
            message
                .windows(tenant.as_str().len())
                .any(|window| window == tenant.as_str().as_bytes())
        );
        assert!(
            message
                .windows(actor.as_str().len())
                .any(|window| window == actor.as_str().as_bytes())
        );
        assert!(
            message
                .windows(device.as_str().len())
                .any(|window| window == device.as_str().as_bytes())
        );
        assert!(message.ends_with(&nonce));

        let mut changed_nonce = nonce;
        changed_nonce[0] ^= 1;
        assert_ne!(
            message,
            device_proof_message_v1(
                &tenant,
                &actor,
                &device,
                &changed_nonce,
                UnixMillis::new(9_999),
            )?
        );
        assert_ne!(
            message,
            device_proof_message_v1(
                &tenant,
                &actor,
                &device,
                &nonce,
                UnixMillis::new(10_000),
            )?
        );
        assert_eq!(
            device_proof_message_v1(&tenant, &actor, &device, &[0; 31], UnixMillis::new(9_999)),
            Err(DeviceProofMessageError::InvalidNonceLength)
        );
        Ok(())
    }

    #[test]
    fn session_fails_closed_on_every_revocation_and_epoch_boundary()
    -> Result<(), Box<dyn std::error::Error>> {
        let (tenant, actor, device) = ids()?;
        let mut session = DeviceApplicationSession::issue(
            tenant,
            actor,
            device,
            7,
            4,
            UnixMillis::new(100),
            UnixMillis::new(200),
        )?;
        assert_eq!(
            session.require_authorized(true, 7, true, 4, UnixMillis::new(150)),
            Ok(())
        );
        assert_eq!(
            session.require_authorized(false, 7, true, 4, UnixMillis::new(150)),
            Err(DeviceApplicationSessionError::UserDisabled)
        );
        assert_eq!(
            session.require_authorized(true, 8, true, 4, UnixMillis::new(150)),
            Err(DeviceApplicationSessionError::StaleUserAuthEpoch)
        );
        assert_eq!(
            session.require_authorized(true, 7, false, 4, UnixMillis::new(150)),
            Err(DeviceApplicationSessionError::DeviceDisabled)
        );
        assert_eq!(
            session.require_authorized(true, 7, true, 5, UnixMillis::new(150)),
            Err(DeviceApplicationSessionError::StaleDeviceBinding)
        );
        assert_eq!(
            session.require_authorized(true, 7, true, 4, UnixMillis::new(200)),
            Err(DeviceApplicationSessionError::Expired)
        );
        session.revoke(UnixMillis::new(160))?;
        assert_eq!(
            session.require_authorized(true, 7, true, 4, UnixMillis::new(170)),
            Err(DeviceApplicationSessionError::Revoked)
        );
        Ok(())
    }

    #[test]
    fn session_rejects_zero_epochs_and_invalid_timeline()
    -> Result<(), Box<dyn std::error::Error>> {
        let (tenant, actor, device) = ids()?;
        assert_eq!(
            DeviceApplicationSession::issue(
                tenant.clone(),
                actor.clone(),
                device.clone(),
                0,
                1,
                UnixMillis::new(1),
                UnixMillis::new(2),
            ),
            Err(DeviceApplicationSessionError::InvalidEpoch)
        );
        assert_eq!(
            DeviceApplicationSession::issue(
                tenant,
                actor,
                device,
                1,
                1,
                UnixMillis::new(2),
                UnixMillis::new(2),
            ),
            Err(DeviceApplicationSessionError::InvalidTimeline)
        );
        Ok(())
    }
}
