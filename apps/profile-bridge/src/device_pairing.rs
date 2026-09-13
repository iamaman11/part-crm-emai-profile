use profile_platform_primitives::{ActorId, DeviceId, TenantId, UnixMillis};
use std::fmt;
use zeroize::Zeroizing;

pub const PAIRING_START_URI_PREFIX: &str = "profilebridge://pair/start/";
pub const PAIRING_COMPLETE_URI_PREFIX: &str = "profilebridge://pair/complete/";
const MAX_START_URI_BYTES: usize = 240;
const MAX_COMPLETE_URI_BYTES: usize = 640;
const OPAQUE_TOKEN_HEX_LENGTH: usize = 64;
const NONCE_HEX_LENGTH: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicePairingStartUri {
    tenant_id: TenantId,
    device_id: DeviceId,
}

impl DevicePairingStartUri {
    pub fn parse(value: &str) -> Result<Self, DevicePairingUriError> {
        let route = exact_route(value, PAIRING_START_URI_PREFIX, MAX_START_URI_BYTES)?;
        let mut segments = route.split('/');
        let tenant = segments.next().ok_or(DevicePairingUriError)?;
        let device = segments.next().ok_or(DevicePairingUriError)?;
        if segments.next().is_some() {
            return Err(DevicePairingUriError);
        }
        Ok(Self {
            tenant_id: TenantId::parse(tenant.to_owned()).map_err(|_| DevicePairingUriError)?,
            device_id: DeviceId::parse(device.to_owned()).map_err(|_| DevicePairingUriError)?,
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

pub struct DevicePairingCompleteUri {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
    pairing_token: Zeroizing<String>,
    challenge_token: Zeroizing<String>,
    nonce_hex: String,
    expires_at: UnixMillis,
}

impl DevicePairingCompleteUri {
    pub fn parse(value: &str) -> Result<Self, DevicePairingUriError> {
        let route = exact_route(value, PAIRING_COMPLETE_URI_PREFIX, MAX_COMPLETE_URI_BYTES)?;
        let mut segments = route.split('/');
        let tenant = segments.next().ok_or(DevicePairingUriError)?;
        let actor = segments.next().ok_or(DevicePairingUriError)?;
        let device = segments.next().ok_or(DevicePairingUriError)?;
        let pairing_token = segments.next().ok_or(DevicePairingUriError)?;
        let challenge_token = segments.next().ok_or(DevicePairingUriError)?;
        let nonce_hex = segments.next().ok_or(DevicePairingUriError)?;
        let expires_at = segments.next().ok_or(DevicePairingUriError)?;
        if segments.next().is_some()
            || !valid_lower_hex(pairing_token, OPAQUE_TOKEN_HEX_LENGTH)
            || !valid_lower_hex(challenge_token, OPAQUE_TOKEN_HEX_LENGTH)
            || !valid_lower_hex(nonce_hex, NONCE_HEX_LENGTH)
        {
            return Err(DevicePairingUriError);
        }
        let expires_at = expires_at
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(DevicePairingUriError)?;
        Ok(Self {
            tenant_id: TenantId::parse(tenant.to_owned()).map_err(|_| DevicePairingUriError)?,
            actor_id: ActorId::parse(actor.to_owned()).map_err(|_| DevicePairingUriError)?,
            device_id: DeviceId::parse(device.to_owned()).map_err(|_| DevicePairingUriError)?,
            pairing_token: Zeroizing::new(pairing_token.to_owned()),
            challenge_token: Zeroizing::new(challenge_token.to_owned()),
            nonce_hex: nonce_hex.to_owned(),
            expires_at: UnixMillis::new(expires_at),
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
    pub fn pairing_token_for_transport(&self) -> &str {
        self.pairing_token.as_str()
    }

    #[must_use]
    pub fn challenge_token_for_transport(&self) -> &str {
        self.challenge_token.as_str()
    }

    #[must_use]
    pub fn nonce_hex(&self) -> &str {
        &self.nonce_hex
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}

impl fmt::Debug for DevicePairingCompleteUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DevicePairingCompleteUri")
            .field("tenant_id", &self.tenant_id)
            .field("actor_id", &self.actor_id)
            .field("device_id", &self.device_id)
            .field("pairing_token", &"[REDACTED]")
            .field("challenge_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DevicePairingUriError;

impl fmt::Display for DevicePairingUriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("device pairing URI is invalid")
    }
}

impl std::error::Error for DevicePairingUriError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevicePairingBootstrapError {
    UnsupportedPlatform,
    Failed,
}

impl fmt::Display for DevicePairingBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "device pairing bootstrap requires Windows",
            Self::Failed => "device pairing bootstrap failed closed",
        })
    }
}

impl std::error::Error for DevicePairingBootstrapError {}

pub fn run_pairing_start(uri: &DevicePairingStartUri) -> Result<(), DevicePairingBootstrapError> {
    #[cfg(windows)]
    {
        return crate::windows_device_pairing::run_start(uri)
            .map_err(|_| DevicePairingBootstrapError::Failed);
    }
    #[cfg(not(windows))]
    {
        let _ = uri;
        Err(DevicePairingBootstrapError::UnsupportedPlatform)
    }
}

pub fn run_pairing_complete(
    uri: &DevicePairingCompleteUri,
) -> Result<(), DevicePairingBootstrapError> {
    #[cfg(windows)]
    {
        return crate::windows_device_pairing::run_complete(uri)
            .map_err(|_| DevicePairingBootstrapError::Failed);
    }
    #[cfg(not(windows))]
    {
        let _ = uri;
        Err(DevicePairingBootstrapError::UnsupportedPlatform)
    }
}

fn exact_route<'a>(
    value: &'a str,
    prefix: &str,
    max_bytes: usize,
) -> Result<&'a str, DevicePairingUriError> {
    if value.len() > max_bytes || value.contains(['?', '#', '\\', '%', '\r', '\n']) {
        return Err(DevicePairingUriError);
    }
    let route = value.strip_prefix(prefix).ok_or(DevicePairingUriError)?;
    if route.is_empty() || route.starts_with('/') || route.ends_with('/') || route.contains("//") {
        return Err(DevicePairingUriError);
    }
    Ok(route)
}

fn valid_lower_hex(value: &str, expected: usize) -> bool {
    value.len() == expected
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::{DevicePairingCompleteUri, DevicePairingStartUri};

    #[test]
    fn pairing_start_is_exact_and_identity_only() -> Result<(), Box<dyn std::error::Error>> {
        let uri = DevicePairingStartUri::parse(
            "profilebridge://pair/start/tenant_01JPAIR/device_01JPAIR",
        )?;
        assert_eq!(uri.tenant_id().as_str(), "tenant_01JPAIR");
        assert_eq!(uri.device_id().as_str(), "device_01JPAIR");
        assert!(
            DevicePairingStartUri::parse(
                "profilebridge://pair/start/tenant_01JPAIR/device_01JPAIR?token=secret"
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn pairing_completion_redacts_one_time_capabilities() -> Result<(), Box<dyn std::error::Error>> {
        let pairing = "a".repeat(64);
        let challenge = "b".repeat(64);
        let nonce = "c".repeat(64);
        let raw = format!(
            "profilebridge://pair/complete/tenant_01JPAIR/actor_01JPAIR/device_01JPAIR/{pairing}/{challenge}/{nonce}/123456789"
        );
        let uri = DevicePairingCompleteUri::parse(&raw)?;
        let debug = format!("{uri:?}");
        assert!(!debug.contains(&pairing));
        assert!(!debug.contains(&challenge));
        assert_eq!(uri.nonce_hex(), nonce);
        assert_eq!(uri.expires_at().value(), 123_456_789);
        Ok(())
    }

    #[test]
    fn pairing_completion_rejects_noncanonical_tokens_and_extra_segments() {
        let token = "a".repeat(64);
        let nonce = "b".repeat(64);
        assert!(
            DevicePairingCompleteUri::parse(&format!(
                "profilebridge://pair/complete/tenant_01JPAIR/actor_01JPAIR/device_01JPAIR/{}/{token}/{nonce}/123",
                "A".repeat(64)
            ))
            .is_err()
        );
        assert!(
            DevicePairingCompleteUri::parse(&format!(
                "profilebridge://pair/complete/tenant_01JPAIR/actor_01JPAIR/device_01JPAIR/{token}/{token}/{nonce}/123/extra"
            ))
            .is_err()
        );
    }
}
