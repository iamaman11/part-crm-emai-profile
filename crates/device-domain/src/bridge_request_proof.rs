use core::fmt;
use profile_platform_primitives::{ActorId, CorrelationId, DeviceId, TenantId, UnixMillis};

const BRIDGE_REQUEST_PROOF_CONTEXT_V1: &[u8] = b"part-crm/bridge-request-proof/v1\0";
pub const BRIDGE_REQUEST_PROOF_DIGEST_BYTES: usize = 32;
pub const BRIDGE_REQUEST_PROOF_MAX_PATH_BYTES: usize = 512;
pub const BRIDGE_REQUEST_PROOF_MAX_LIFETIME_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeRequestProofMethod {
    Get,
    PostJson,
}

impl BridgeRequestProofMethod {
    const fn canonical_bytes(self) -> &'static [u8] {
        match self {
            Self::Get => b"GET",
            Self::PostJson => b"POST",
        }
    }
}

/// Canonical device proof for one exact shipping Bridge HTTP request.
///
/// The opaque application-session secret itself is never signed or logged. Its SHA-256 digest is
/// bound together with the server-owned user/device identity, exact HTTP method/path, correlation
/// id, exact request-body digest and a short proof expiry. A captured application-session bearer is
/// therefore insufficient to authorize a Bridge request without the non-exportable device key.
pub fn bridge_request_proof_message_v1(
    tenant_id: &TenantId,
    actor_id: &ActorId,
    device_id: &DeviceId,
    session_digest: &[u8; BRIDGE_REQUEST_PROOF_DIGEST_BYTES],
    method: BridgeRequestProofMethod,
    path: &str,
    correlation_id: &CorrelationId,
    body_digest: &[u8; BRIDGE_REQUEST_PROOF_DIGEST_BYTES],
    expires_at: UnixMillis,
) -> Result<Vec<u8>, BridgeRequestProofMessageError> {
    if !valid_path(path) {
        return Err(BridgeRequestProofMessageError::InvalidPath);
    }

    let mut message = Vec::with_capacity(
        BRIDGE_REQUEST_PROOF_CONTEXT_V1.len()
            + 2
            + tenant_id.as_str().len()
            + 2
            + actor_id.as_str().len()
            + 2
            + device_id.as_str().len()
            + BRIDGE_REQUEST_PROOF_DIGEST_BYTES
            + 2
            + method.canonical_bytes().len()
            + 2
            + path.len()
            + 2
            + correlation_id.as_str().len()
            + BRIDGE_REQUEST_PROOF_DIGEST_BYTES
            + 8,
    );
    message.extend_from_slice(BRIDGE_REQUEST_PROOF_CONTEXT_V1);
    append_identifier(&mut message, tenant_id.as_str())?;
    append_identifier(&mut message, actor_id.as_str())?;
    append_identifier(&mut message, device_id.as_str())?;
    message.extend_from_slice(session_digest);
    append_bytes(&mut message, method.canonical_bytes())?;
    append_bytes(&mut message, path.as_bytes())?;
    append_identifier(&mut message, correlation_id.as_str())?;
    message.extend_from_slice(body_digest);
    message.extend_from_slice(&expires_at.value().to_be_bytes());
    Ok(message)
}

fn valid_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= BRIDGE_REQUEST_PROOF_MAX_PATH_BYTES
        && path.is_ascii()
        && !path.contains(['?', '#', '\\', '\r', '\n'])
        && !path.contains("//")
        && !path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
}

fn append_identifier(
    message: &mut Vec<u8>,
    value: &str,
) -> Result<(), BridgeRequestProofMessageError> {
    append_bytes(message, value.as_bytes())
}

fn append_bytes(message: &mut Vec<u8>, value: &[u8]) -> Result<(), BridgeRequestProofMessageError> {
    let length = u16::try_from(value.len())
        .map_err(|_| BridgeRequestProofMessageError::InvalidFieldLength)?;
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(value);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeRequestProofMessageError {
    InvalidPath,
    InvalidFieldLength,
}

impl fmt::Display for BridgeRequestProofMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPath => "Bridge request proof path is invalid",
            Self::InvalidFieldLength => "Bridge request proof field length is invalid",
        })
    }
}

impl std::error::Error for BridgeRequestProofMessageError {}

#[cfg(test)]
mod tests {
    use super::{
        BRIDGE_REQUEST_PROOF_MAX_LIFETIME_MS, BridgeRequestProofMethod,
        bridge_request_proof_message_v1,
    };
    use profile_platform_primitives::{ActorId, CorrelationId, DeviceId, TenantId, UnixMillis};

    fn ids() -> Result<(TenantId, ActorId, DeviceId, CorrelationId), Box<dyn std::error::Error>> {
        Ok((
            TenantId::parse("tenant_01JBRIDGEPROOF")?,
            ActorId::parse("actor_01JBRIDGEPROOF")?,
            DeviceId::parse("device_01JBRIDGEPROOF")?,
            CorrelationId::parse("corr_01JBRIDGEPROOF")?,
        ))
    }

    #[test]
    fn proof_binds_session_identity_route_body_correlation_and_expiry()
    -> Result<(), Box<dyn std::error::Error>> {
        let (tenant, actor, device, correlation) = ids()?;
        let session = [0x11; 32];
        let body = [0x22; 32];
        let expiry = UnixMillis::new(20_000);
        let message = bridge_request_proof_message_v1(
            &tenant,
            &actor,
            &device,
            &session,
            BridgeRequestProofMethod::PostJson,
            "/bridge/api/v1/profile-launch-redemptions",
            &correlation,
            &body,
            expiry,
        )?;

        let changed_body = [0x23; 32];
        assert_ne!(
            message,
            bridge_request_proof_message_v1(
                &tenant,
                &actor,
                &device,
                &session,
                BridgeRequestProofMethod::PostJson,
                "/bridge/api/v1/profile-launch-redemptions",
                &correlation,
                &changed_body,
                expiry,
            )?
        );
        assert_ne!(
            message,
            bridge_request_proof_message_v1(
                &tenant,
                &actor,
                &device,
                &session,
                BridgeRequestProofMethod::Get,
                "/bridge/api/v1/profile-launch-redemptions",
                &correlation,
                &body,
                expiry,
            )?
        );
        let foreign_session = [0x33; 32];
        assert_ne!(
            message,
            bridge_request_proof_message_v1(
                &tenant,
                &actor,
                &device,
                &foreign_session,
                BridgeRequestProofMethod::PostJson,
                "/bridge/api/v1/profile-launch-redemptions",
                &correlation,
                &body,
                expiry,
            )?
        );
        assert_eq!(BRIDGE_REQUEST_PROOF_MAX_LIFETIME_MS, 30_000);
        Ok(())
    }

    #[test]
    fn proof_rejects_noncanonical_paths() -> Result<(), Box<dyn std::error::Error>> {
        let (tenant, actor, device, correlation) = ids()?;
        for path in [
            "bridge/api/v1/profiles",
            "/bridge//api/v1/profiles",
            "/bridge/api/../profiles",
            "/bridge/api/v1/profiles?x=1",
            "/bridge/api/v1/profiles#fragment",
        ] {
            assert!(
                bridge_request_proof_message_v1(
                    &tenant,
                    &actor,
                    &device,
                    &[0; 32],
                    BridgeRequestProofMethod::Get,
                    path,
                    &correlation,
                    &[0; 32],
                    UnixMillis::new(1),
                )
                .is_err()
            );
        }
        Ok(())
    }
}
