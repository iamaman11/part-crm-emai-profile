use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::{Date, Env, Headers, Method, RequestInit};
use zeroize::{Zeroize, Zeroizing};

const RESOLVER_ORIGIN: &str = "https://mailbox-secret-resolver.internal";
const CALLER_AUTH_SECRET: &str = "MAILBOX_RESOLVER_CALLER_AUTH_KEY";
const LEGACY_SIGNATURE_VERSION: &str = "hmac-sha256-v1";
const KEYED_SIGNATURE_VERSION: &str = "hmac-sha256-v2";
const LEGACY_KEY_ID: &str = "legacy-v1";
const NONCE_BYTES: usize = 16;
const MAX_REQUEST_BYTES: usize = 32 * 1024;
const MIN_KEY_BYTES: usize = 32;
const MAX_KEY_BYTES: usize = 128;
const MAX_RETAINED_KEYS: usize = 4;
const MAX_KEY_ID_BYTES: usize = 64;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolverRequestError {
    InvalidEndpoint,
    InvalidEnvelope,
    InvalidSecret,
    RandomnessUnavailable,
    HeaderFailure,
}

struct ServiceAuthKeyring {
    active_key_id: String,
    keys: Vec<ServiceAuthKey>,
    legacy_serialization: bool,
}

struct ServiceAuthKey {
    id: String,
    bytes: Zeroizing<Vec<u8>>,
}

impl ServiceAuthKeyring {
    fn parse(serialized: &str) -> Result<Self, ResolverRequestError> {
        if serialized.trim_start().starts_with('{') {
            return Self::parse_json(serialized);
        }
        if !valid_legacy_key(serialized.as_bytes()) {
            return Err(ResolverRequestError::InvalidSecret);
        }
        Ok(Self {
            active_key_id: LEGACY_KEY_ID.to_owned(),
            keys: vec![ServiceAuthKey {
                id: LEGACY_KEY_ID.to_owned(),
                bytes: Zeroizing::new(serialized.as_bytes().to_vec()),
            }],
            legacy_serialization: true,
        })
    }

    fn parse_json(serialized: &str) -> Result<Self, ResolverRequestError> {
        let document: ServiceAuthKeyringSecret =
            serde_json::from_str(serialized).map_err(|_| ResolverRequestError::InvalidSecret)?;
        if !valid_key_id(&document.active_key_id)
            || document.keys.is_empty()
            || document.keys.len() > MAX_RETAINED_KEYS
        {
            return Err(ResolverRequestError::InvalidSecret);
        }
        let mut keys = Vec::with_capacity(document.keys.len());
        for mut entry in document.keys {
            if !valid_key_id(&entry.id)
                || keys.iter().any(|key: &ServiceAuthKey| key.id == entry.id)
            {
                entry.key_hex.zeroize();
                return Err(ResolverRequestError::InvalidSecret);
            }
            let mut decoded =
                hex_decode(&entry.key_hex).ok_or(ResolverRequestError::InvalidSecret)?;
            entry.key_hex.zeroize();
            if !(MIN_KEY_BYTES..=MAX_KEY_BYTES).contains(&decoded.len()) {
                decoded.zeroize();
                return Err(ResolverRequestError::InvalidSecret);
            }
            keys.push(ServiceAuthKey {
                id: entry.id.clone(),
                bytes: Zeroizing::new(decoded),
            });
        }
        if !keys.iter().any(|key| key.id == document.active_key_id) {
            return Err(ResolverRequestError::InvalidSecret);
        }
        Ok(Self {
            active_key_id: document.active_key_id,
            keys,
            legacy_serialization: false,
        })
    }

    fn active(&self) -> Result<&ServiceAuthKey, ResolverRequestError> {
        self.keys
            .iter()
            .find(|key| key.id == self.active_key_id)
            .ok_or(ResolverRequestError::InvalidSecret)
    }
}

impl core::fmt::Debug for ServiceAuthKeyring {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let retained_ids = self
            .keys
            .iter()
            .map(|key| key.id.as_str())
            .collect::<Vec<_>>();
        formatter
            .debug_struct("ServiceAuthKeyring")
            .field("active_key_id", &self.active_key_id)
            .field("retained_ids", &retained_ids)
            .field("legacy_serialization", &self.legacy_serialization)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceAuthKeyringSecret {
    active_key_id: String,
    keys: Vec<ServiceAuthKeySecret>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceAuthKeySecret {
    id: String,
    key_hex: String,
}

impl Drop for ServiceAuthKeySecret {
    fn drop(&mut self) {
        self.key_hex.zeroize();
    }
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
    let caller_secret = Zeroizing::new(
        env.secret(CALLER_AUTH_SECRET)
            .map_err(|_| ResolverRequestError::InvalidSecret)?
            .to_string(),
    );
    let keyring = ServiceAuthKeyring::parse(&caller_secret)?;
    let key = keyring.active()?;
    let (signature_version, key_id) = if keyring.legacy_serialization {
        (LEGACY_SIGNATURE_VERSION, None)
    } else {
        (KEYED_SIGNATURE_VERSION, Some(key.id.as_str()))
    };
    let canonical = canonical_signature_input(
        signature_version,
        key_id,
        &path,
        &body_digest,
        tenant_id,
        timestamp_ms,
        &nonce,
    )?;
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key.bytes.as_slice())
        .map_err(|_| ResolverRequestError::InvalidSecret)?;
    mac.update(canonical.as_bytes());
    let signature = hex_encode(mac.finalize().into_bytes().as_slice());

    let headers = Headers::new();
    for (name, value) in [
        ("accept", "application/json"),
        ("content-type", "application/json"),
        ("cache-control", "no-store"),
        ("x-resolver-signature-version", signature_version),
        ("x-resolver-body-sha256", body_digest.as_str()),
        ("x-resolver-timestamp-ms", timestamp.as_str()),
        ("x-resolver-nonce", nonce.as_str()),
        ("x-resolver-signature", signature.as_str()),
    ] {
        headers
            .set(name, value)
            .map_err(|_| ResolverRequestError::HeaderFailure)?;
    }
    if let Some(key_id) = key_id {
        headers
            .set("x-resolver-key-id", key_id)
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

fn canonical_signature_input(
    version: &str,
    key_id: Option<&str>,
    path: &str,
    body_digest: &str,
    tenant_id: &str,
    timestamp_ms: u64,
    nonce: &str,
) -> Result<String, ResolverRequestError> {
    match (version, key_id) {
        (LEGACY_SIGNATURE_VERSION, None) => Ok(format!(
            "{LEGACY_SIGNATURE_VERSION}\nPOST\n{path}\n{body_digest}\n{tenant_id}\n{timestamp_ms}\n{nonce}"
        )),
        (KEYED_SIGNATURE_VERSION, Some(key_id)) if valid_key_id(key_id) => Ok(format!(
            "{KEYED_SIGNATURE_VERSION}\n{key_id}\nPOST\n{path}\n{body_digest}\n{tenant_id}\n{timestamp_ms}\n{nonce}"
        )),
        _ => Err(ResolverRequestError::InvalidSecret),
    }
}

fn valid_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_KEY_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_legacy_key(value: &[u8]) -> bool {
    (MIN_KEY_BYTES..=MAX_KEY_BYTES).contains(&value.len())
        && !value.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Some((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
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
    use super::{
        KEYED_SIGNATURE_VERSION, LEGACY_KEY_ID, LEGACY_SIGNATURE_VERSION, ResolverRequestError,
        ServiceAuthKeyring, canonical_signature_input, oauth_callback_tenant,
    };

    fn migrated_keyring() -> String {
        format!(
            "{{\"activeKeyId\":\"key-2026-08\",\"keys\":[{{\"id\":\"key-2026-08\",\"keyHex\":\"{}\"}},{{\"id\":\"{LEGACY_KEY_ID}\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        )
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

    #[test]
    fn raw_secret_preserves_v1_signing_mode() -> Result<(), ResolverRequestError> {
        let keyring =
            ServiceAuthKeyring::parse(&"legacy-caller-auth-key-material-0123456789".repeat(2))?;
        assert!(keyring.legacy_serialization);
        assert_eq!(keyring.active()?.id, LEGACY_KEY_ID);
        assert!(
            canonical_signature_input(
                LEGACY_SIGNATURE_VERSION,
                None,
                "/v1/mailbox-credentials/resolve",
                &"a".repeat(64),
                "tenant_01",
                100,
                "00112233445566778899aabbccddeeff",
            )
            .is_ok()
        );
        Ok(())
    }

    #[test]
    fn keyed_secret_uses_exact_active_id() -> Result<(), ResolverRequestError> {
        let keyring = ServiceAuthKeyring::parse(&migrated_keyring())?;
        assert!(!keyring.legacy_serialization);
        assert_eq!(keyring.active()?.id, "key-2026-08");
        assert!(
            canonical_signature_input(
                KEYED_SIGNATURE_VERSION,
                Some("key-2026-08"),
                "/v1/mailbox-credentials/resolve",
                &"a".repeat(64),
                "tenant_01",
                100,
                "00112233445566778899aabbccddeeff",
            )
            .is_ok()
        );
        assert!(
            canonical_signature_input(
                KEYED_SIGNATURE_VERSION,
                None,
                "/v1/mailbox-credentials/resolve",
                &"a".repeat(64),
                "tenant_01",
                100,
                "00112233445566778899aabbccddeeff",
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn duplicate_or_missing_active_keys_fail_closed() {
        let duplicate = format!(
            "{{\"activeKeyId\":\"same\",\"keys\":[{{\"id\":\"same\",\"keyHex\":\"{}\"}},{{\"id\":\"same\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        );
        assert!(ServiceAuthKeyring::parse(&duplicate).is_err());
        assert!(ServiceAuthKeyring::parse("{\"activeKeyId\":\"missing\",\"keys\":[]}").is_err());
    }
}
