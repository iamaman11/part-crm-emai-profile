use application_ports::profile_launch::ProfileLaunchMachineBinding;
use cloudflare_adapters::d1_device_application_authority::D1DeviceApplicationAuthority;
use cloudflare_adapters::device_webcrypto::verify_p256_sha256;
use control_plane_contract::device_application_api::{
    DEVICE_APPLICATION_SESSION_HEADER, DEVICE_REQUEST_PROOF_EXPIRES_HEADER,
    DEVICE_REQUEST_PROOF_SIGNATURE_HEADER, OPAQUE_TOKEN_HEX_LENGTH,
    P256_SIGNATURE_P1363_HEX_LENGTH,
};
use control_plane_contract::D1_CATALOG_BINDING;
use device_domain::{
    BRIDGE_REQUEST_PROOF_MAX_LIFETIME_MS, BridgeRequestProofMethod,
    bridge_request_proof_message_v1,
};
use profile_platform_primitives::{CorrelationId, UnixMillis};
use sha2::{Digest, Sha256};
use worker::{Date, Env, Error, Method, Request, Result};

const REQUEST_PROOF_DIGEST_BYTES: usize = 32;
const REQUEST_PROOF_SIGNATURE_BYTES: usize = P256_SIGNATURE_P1363_HEX_LENGTH / 2;
const MAX_BRIDGE_PROOF_BODY_BYTES: usize = 65_536;

struct ApplicationSessionDigest {
    bytes: [u8; REQUEST_PROOF_DIGEST_BYTES],
    hex: String,
}

/// Resolve one shipping Bridge principal only when both the short-lived application session and a
/// proof from the currently registered non-exportable device key authorize this exact request.
///
/// The proof binds the server-owned tenant/actor/device identity, session digest, exact method/path,
/// correlation id, exact request-body digest and a <=30s expiry. The D1 authority independently
/// rechecks membership, user auth epoch, active P-256 binding/version and session revocation/expiry.
/// A copied bearer session therefore cannot authorize Bridge traffic without the current device key.
pub async fn resolve_bridge_machine(
    request: &Request,
    env: &Env,
    correlation_id: &CorrelationId,
) -> Result<Option<ProfileLaunchMachineBinding>> {
    let Some(session_digest) = application_session_digest(request)? else {
        return Ok(None);
    };
    let Some(proof_expires_at) = request_proof_expiry(request) else {
        return Ok(None);
    };
    let Some(signature) = request_proof_signature(request)? else {
        return Ok(None);
    };
    let Some(method) = request_proof_method(request.method()) else {
        return Ok(None);
    };
    let path = request.path();
    let Some(body_digest) = request_body_digest(request).await? else {
        return Ok(None);
    };

    let now = UnixMillis::new(Date::now().as_millis());
    if !proof_expiry_is_current(now, proof_expires_at) {
        return Ok(None);
    }

    let authority = D1DeviceApplicationAuthority::new(env.d1(D1_CATALOG_BINDING)?);
    let session = match authority
        .resolve_active_session_by_digest(&session_digest.hex, now)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return Ok(None),
        Err(_) => {
            return Err(Error::RustError(format!(
                "Bridge application session resolution failed ({})",
                correlation_id.as_str()
            )));
        }
    };
    let device = match authority
        .resolve_active_device(session.tenant_id(), session.device_id())
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return Ok(None),
        Err(_) => {
            return Err(Error::RustError(format!(
                "Bridge device proof authority resolution failed ({})",
                correlation_id.as_str()
            )));
        }
    };
    if device.actor_id() != session.actor_id()
        || device.device_id() != session.device_id()
        || device.binding_version() != session.device_binding_version()
        || device.auth_epoch() != session.user_auth_epoch()
    {
        return Ok(None);
    }

    let message = bridge_request_proof_message_v1(
        session.tenant_id(),
        session.actor_id(),
        session.device_id(),
        &session_digest.bytes,
        method,
        &path,
        correlation_id,
        &body_digest,
        proof_expires_at,
    )
    .map_err(|error| Error::RustError(error.to_string()))?;
    if !verify_p256_sha256(device.public_key(), &signature, &message).await? {
        return Ok(None);
    }

    Ok(Some(ProfileLaunchMachineBinding::new(
        session.tenant_id().clone(),
        session.actor_id().clone(),
        session.device_id().clone(),
    )))
}

fn application_session_digest(request: &Request) -> Result<Option<ApplicationSessionDigest>> {
    let Some(value) = request.headers().get(DEVICE_APPLICATION_SESSION_HEADER)? else {
        return Ok(None);
    };
    Ok(application_session_digest_value(value))
}

fn application_session_digest_value(value: String) -> Option<ApplicationSessionDigest> {
    let mut token = value.into_bytes();
    if token.len() != OPAQUE_TOKEN_HEX_LENGTH
        || !token
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        token.fill(0);
        return None;
    }
    let digest = Sha256::digest(&token);
    token.fill(0);
    let mut bytes = [0_u8; REQUEST_PROOF_DIGEST_BYTES];
    bytes.copy_from_slice(&digest);
    Some(ApplicationSessionDigest {
        hex: hex_encode(&bytes),
        bytes,
    })
}

fn request_proof_expiry(request: &Request) -> Option<UnixMillis> {
    let value = request
        .headers()
        .get(DEVICE_REQUEST_PROOF_EXPIRES_HEADER)
        .ok()
        .flatten()?;
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return None;
    }
    let parsed = value.parse::<u64>().ok()?;
    if parsed == 0 || parsed.to_string() != value {
        return None;
    }
    Some(UnixMillis::new(parsed))
}

fn proof_expiry_is_current(now: UnixMillis, expires_at: UnixMillis) -> bool {
    expires_at
        .value()
        .checked_sub(now.value())
        .is_some_and(|remaining| remaining > 0 && remaining <= BRIDGE_REQUEST_PROOF_MAX_LIFETIME_MS)
}

fn request_proof_signature(request: &Request) -> Result<Option<[u8; REQUEST_PROOF_SIGNATURE_BYTES]>> {
    let Some(value) = request
        .headers()
        .get(DEVICE_REQUEST_PROOF_SIGNATURE_HEADER)?
    else {
        return Ok(None);
    };
    Ok(decode_exact_lower_hex::<REQUEST_PROOF_SIGNATURE_BYTES>(&value))
}

fn request_proof_method(method: Method) -> Option<BridgeRequestProofMethod> {
    match method {
        Method::Get => Some(BridgeRequestProofMethod::Get),
        Method::Post => Some(BridgeRequestProofMethod::PostJson),
        _ => None,
    }
}

async fn request_body_digest(request: &Request) -> Result<Option<[u8; REQUEST_PROOF_DIGEST_BYTES]>> {
    let mut clone = request.clone()?;
    let mut body = clone.bytes().await?;
    if body.len() > MAX_BRIDGE_PROOF_BODY_BYTES {
        body.fill(0);
        return Ok(None);
    }
    let digest = Sha256::digest(&body);
    body.fill(0);
    let mut bytes = [0_u8; REQUEST_PROOF_DIGEST_BYTES];
    bytes.copy_from_slice(&digest);
    Ok(Some(bytes))
}

fn decode_exact_lower_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{
        OPAQUE_TOKEN_HEX_LENGTH, application_session_digest_value, decode_exact_lower_hex,
        proof_expiry_is_current,
    };
    use control_plane_contract::device_application_api::{
        DEVICE_APPLICATION_SESSION_HEADER, DEVICE_REQUEST_PROOF_EXPIRES_HEADER,
        DEVICE_REQUEST_PROOF_SIGNATURE_HEADER,
    };
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn bridge_authentication_requires_session_and_device_proof_headers() {
        assert_eq!(
            DEVICE_APPLICATION_SESSION_HEADER,
            "X-Device-Application-Session"
        );
        assert_eq!(
            DEVICE_REQUEST_PROOF_EXPIRES_HEADER,
            "X-Device-Request-Proof-Expires-Ms"
        );
        assert_eq!(
            DEVICE_REQUEST_PROOF_SIGNATURE_HEADER,
            "X-Device-Request-Proof-Signature"
        );
    }

    #[test]
    fn raw_application_session_is_strict_and_reduced_to_digest_before_lookup() {
        let token = "ab".repeat(OPAQUE_TOKEN_HEX_LENGTH / 2);
        let digest = application_session_digest_value(token.clone()).expect("valid token");
        assert_eq!(digest.hex.len(), 64);
        assert_eq!(digest.bytes.len(), 32);
        assert_ne!(digest.hex, token);
        assert!(application_session_digest_value("AB".repeat(32)).is_none());
        assert!(application_session_digest_value("a".repeat(63)).is_none());
        assert!(application_session_digest_value(format!("{}\n", "a".repeat(64))).is_none());
    }

    #[test]
    fn request_proof_expiry_is_short_lived_and_fail_closed() {
        assert!(proof_expiry_is_current(
            UnixMillis::new(1_000),
            UnixMillis::new(31_000)
        ));
        assert!(!proof_expiry_is_current(
            UnixMillis::new(1_000),
            UnixMillis::new(31_001)
        ));
        assert!(!proof_expiry_is_current(
            UnixMillis::new(1_000),
            UnixMillis::new(1_000)
        ));
        assert!(!proof_expiry_is_current(
            UnixMillis::new(1_001),
            UnixMillis::new(1_000)
        ));
    }

    #[test]
    fn request_proof_signature_is_exact_lowercase_p1363() {
        assert!(decode_exact_lower_hex::<64>(&"ab".repeat(64)).is_some());
        assert!(decode_exact_lower_hex::<64>(&"AB".repeat(64)).is_none());
        assert!(decode_exact_lower_hex::<64>(&"a".repeat(127)).is_none());
    }
}
