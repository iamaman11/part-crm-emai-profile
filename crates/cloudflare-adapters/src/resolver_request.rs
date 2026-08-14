use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::{Date, Env, Headers, Method, RequestInit};
use zeroize::{Zeroize, Zeroizing};

const RESOLVER_ORIGIN: &str = "https://mailbox-secret-resolver.internal";
const CALLER_AUTH_SECRET: &str = "MAILBOX_RESOLVER_CALLER_AUTH_KEY";
const SIGNATURE_VERSION: &str = "hmac-sha256-v1";
const NONCE_BYTES: usize = 16;
const MAX_REQUEST_BYTES: usize = 32 * 1024;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolverRequestError {
    InvalidEndpoint,
    InvalidEnvelope,
    InvalidSecret,
    RandomnessUnavailable,
    HeaderFailure,
}

pub fn oauth_callback_tenant(state: &str) -> Result<&str, ResolverRequestError> {
    let (tenant_id, opaque_state) = state
        .split_once('.')
        .ok_or(ResolverRequestError::InvalidEnvelope)?;
    let valid_tenant = !tenant_id.is_empty()
        && tenant_id.len() <= 128
        && tenant_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':'));
    let valid_state = opaque_state.starts_with("state_")
        && opaque_state.len() >= 22
        && opaque_state.len() <= 160
        && opaque_state
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if !valid_tenant || !valid_state {
        return Err(ResolverRequestError::InvalidEnvelope);
    }
    Ok(tenant_id)
}

pub fn signed_resolver_request(
    env: &Env,
    endpoint: &str,
    tenant_id: &str,
    purpose: &str,
    mut payload: Map<String, Value>,
) -> Result<RequestInit, ResolverRequestError> {
    let path = endpoint
        .strip_prefix(RESOLVER_ORIGIN)
        .filter(|path| {
            path.starts_with("/v1/mailbox-credentials/") && !path.contains('?') && path.len() <= 160
        })
        .ok_or(ResolverRequestError::InvalidEndpoint)?;
    if payload.contains_key("tenantId") || payload.contains_key("purpose") {
        zeroize_map(&mut payload);
        return Err(ResolverRequestError::InvalidEnvelope);
    }
    payload.insert("tenantId".to_owned(), Value::String(tenant_id.to_owned()));
    payload.insert("purpose".to_owned(), Value::String(purpose.to_owned()));
    let serialized = serde_json::to_string(&payload);
    zeroize_map(&mut payload);
    let mut body = serialized.map_err(|_| ResolverRequestError::InvalidEnvelope)?;
    if body.is_empty() || body.len() > MAX_REQUEST_BYTES {
        body.zeroize();
        return Err(ResolverRequestError::InvalidEnvelope);
    }

    let timestamp_ms = Date::now().as_millis();
    let timestamp = timestamp_ms.to_string();
    let nonce = random_nonce_hex()?;
    let body_digest = hex_encode(Sha256::digest(body.as_bytes()).as_slice());
    let canonical = format!(
        "{SIGNATURE_VERSION}\nPOST\n{path}\n{body_digest}\n{tenant_id}\n{timestamp_ms}\n{nonce}"
    );
    let secret = Zeroizing::new(
        env.secret(CALLER_AUTH_SECRET)
            .map_err(|_| ResolverRequestError::InvalidSecret)?
            .to_string(),
    );
    if !(32..=128).contains(&secret.len()) {
        body.zeroize();
        return Err(ResolverRequestError::InvalidSecret);
    }
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes())
        .map_err(|_| ResolverRequestError::InvalidSecret)?;
    mac.update(canonical.as_bytes());
    let signature = hex_encode(mac.finalize().into_bytes().as_slice());

    let headers = Headers::new();
    for (name, value) in [
        ("accept", "application/json"),
        ("content-type", "application/json"),
        ("cache-control", "no-store"),
        ("x-resolver-signature-version", SIGNATURE_VERSION),
        ("x-resolver-body-sha256", body_digest.as_str()),
        ("x-resolver-timestamp-ms", timestamp.as_str()),
        ("x-resolver-nonce", nonce.as_str()),
        ("x-resolver-signature", signature.as_str()),
    ] {
        headers
            .set(name, value)
            .map_err(|_| ResolverRequestError::HeaderFailure)?;
    }
    let js_body = JsValue::from_str(&body);
    body.zeroize();
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(js_body));
    Ok(init)
}

fn zeroize_map(map: &mut Map<String, Value>) {
    for value in map.values_mut() {
        zeroize_value(value);
    }
}

fn zeroize_value(value: &mut Value) {
    match value {
        Value::String(secret) => secret.zeroize(),
        Value::Array(values) => {
            for nested in values {
                zeroize_value(nested);
            }
        }
        Value::Object(values) => zeroize_map(values),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn random_nonce_hex() -> Result<String, ResolverRequestError> {
    let scope = worker::js_sys::global()
        .dyn_into::<web_sys::WorkerGlobalScope>()
        .map_err(|_| ResolverRequestError::RandomnessUnavailable)?;
    let mut nonce = [0_u8; NONCE_BYTES];
    scope
        .crypto()
        .map_err(|_| ResolverRequestError::RandomnessUnavailable)?
        .get_random_values_with_u8_array(&mut nonce)
        .map_err(|_| ResolverRequestError::RandomnessUnavailable)?;
    Ok(hex_encode(&nonce))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{MAX_REQUEST_BYTES, RESOLVER_ORIGIN, SIGNATURE_VERSION, oauth_callback_tenant};

    #[test]
    fn signed_protocol_constants_match_resolver_ingress() {
        assert_eq!(SIGNATURE_VERSION, "hmac-sha256-v1");
        assert_eq!(MAX_REQUEST_BYTES, 32 * 1024);
        assert_eq!(RESOLVER_ORIGIN, "https://mailbox-secret-resolver.internal");
    }

    #[test]
    fn callback_state_carries_a_bounded_tenant_partition() {
        assert_eq!(
            oauth_callback_tenant("tenant_01.state_0123456789abcdef"),
            Ok("tenant_01")
        );
        assert!(oauth_callback_tenant("state_0123456789abcdef").is_err());
        assert!(oauth_callback_tenant("tenant_01.state_bad space").is_err());
    }
}
