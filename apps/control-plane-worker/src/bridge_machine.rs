use application_ports::profile_launch::ProfileLaunchMachineBinding;
use cloudflare_adapters::d1_device_application_authority::D1DeviceApplicationAuthority;
use control_plane_contract::D1_CATALOG_BINDING;
use profile_platform_primitives::{CorrelationId, UnixMillis};
use sha2::{Digest, Sha256};
use worker::{Date, Env, Error, Request, Result};

pub const BRIDGE_APPLICATION_SESSION_HEADER: &str = "X-Device-Application-Session";
const OPAQUE_SESSION_TOKEN_HEX_BYTES: usize = 64;

/// Resolve one shipping Bridge principal through the existing device-bound application-session
/// authority.
///
/// The raw opaque session token is accepted only in the dedicated header, is reduced to a SHA-256
/// digest before D1 lookup, and is never used as caller-owned tenant/actor/device identity. The D1
/// authority rechecks active membership, user auth epoch, active P-256 device binding, binding
/// version, session revocation and expiry on every Bridge request.
pub async fn resolve_bridge_machine(
    request: &Request,
    env: &Env,
    correlation_id: &CorrelationId,
) -> Result<Option<ProfileLaunchMachineBinding>> {
    let Some(session_digest) = application_session_digest(request)? else {
        return Ok(None);
    };
    let sessions = D1DeviceApplicationAuthority::new(env.d1(D1_CATALOG_BINDING)?);
    let now = UnixMillis::new(Date::now().as_millis());
    match sessions
        .resolve_active_session_by_digest(&session_digest, now)
        .await
    {
        Ok(Some(session)) => Ok(Some(ProfileLaunchMachineBinding::new(
            session.tenant_id().clone(),
            session.actor_id().clone(),
            session.device_id().clone(),
        ))),
        Ok(None) => Ok(None),
        Err(_) => Err(Error::RustError(format!(
            "Bridge application session resolution failed ({})",
            correlation_id.as_str()
        ))),
    }
}

fn application_session_digest(request: &Request) -> Result<Option<String>> {
    let Some(value) = request.headers().get(BRIDGE_APPLICATION_SESSION_HEADER)? else {
        return Ok(None);
    };
    Ok(application_session_digest_value(value))
}

fn application_session_digest_value(value: String) -> Option<String> {
    let mut token = value.into_bytes();
    if token.len() != OPAQUE_SESSION_TOKEN_HEX_BYTES
        || !token
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        token.fill(0);
        return None;
    }
    let digest = Sha256::digest(&token);
    token.fill(0);
    Some(hex_encode(&digest))
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
        BRIDGE_APPLICATION_SESSION_HEADER, OPAQUE_SESSION_TOKEN_HEX_BYTES,
        application_session_digest_value,
    };

    #[test]
    fn bridge_authentication_uses_only_the_device_application_session_header() {
        assert_eq!(
            BRIDGE_APPLICATION_SESSION_HEADER,
            "X-Device-Application-Session"
        );
    }

    #[test]
    fn raw_application_session_is_strict_and_reduced_to_digest_before_lookup() {
        let token = "ab".repeat(OPAQUE_SESSION_TOKEN_HEX_BYTES / 2);
        let digest = application_session_digest_value(token.clone()).expect("valid token");
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert_ne!(digest, token);
        assert!(application_session_digest_value("AB".repeat(32)).is_none());
        assert!(application_session_digest_value("a".repeat(63)).is_none());
        assert!(application_session_digest_value(format!("{}\n", "a".repeat(64))).is_none());
    }
}
