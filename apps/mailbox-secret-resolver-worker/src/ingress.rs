use crate::contract::validate_payload;
use crate::model::{MAX_REQUEST_BYTES, ResolverEnvelope, ResolverRoute};
use crate::protocol::{
    KEYED_SIGNATURE_VERSION, LEGACY_SIGNATURE_VERSION, SignatureError, SignatureInput, hex_decode,
    verify_versioned,
};
use serde::Deserialize;
use worker::{Env, Method, Request};
use zeroize::{Zeroize, Zeroizing};

const CALLER_AUTH_SECRET: &str = "MAILBOX_RESOLVER_CALLER_AUTH_KEY";
const SIGNATURE_VERSION_HEADER: &str = "x-resolver-signature-version";
const KEY_ID_HEADER: &str = "x-resolver-key-id";
const BODY_DIGEST_HEADER: &str = "x-resolver-body-sha256";
const TIMESTAMP_HEADER: &str = "x-resolver-timestamp-ms";
const NONCE_HEADER: &str = "x-resolver-nonce";
const SIGNATURE_HEADER: &str = "x-resolver-signature";
const LEGACY_KEY_ID: &str = "legacy-v1";
const MIN_KEY_BYTES: usize = 32;
const MAX_KEY_BYTES: usize = 128;
const MAX_RETAINED_KEYS: usize = 4;
const MAX_KEY_ID_BYTES: usize = 64;

#[derive(Debug)]
pub struct AuthenticatedResolverRequest {
    pub route: ResolverRoute,
    pub path: String,
    pub envelope: ResolverEnvelope,
    pub body_digest: String,
    pub timestamp_ms: u64,
    pub nonce: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngressError {
    MethodNotAllowed,
    RouteNotFound,
    BodyTooLarge,
    InvalidDocument,
    WrongPurpose,
    InvalidPayload,
    CrossTenantState,
    MissingAuthentication,
    InvalidAuthentication,
    StaleAuthentication,
    ConfigurationUnavailable,
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
    fn parse(serialized: &str) -> Result<Self, IngressError> {
        if serialized.trim_start().starts_with('{') {
            return Self::parse_json(serialized);
        }
        if !valid_legacy_key(serialized.as_bytes()) {
            return Err(IngressError::ConfigurationUnavailable);
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

    fn parse_json(serialized: &str) -> Result<Self, IngressError> {
        let document: ServiceAuthKeyringSecret =
            serde_json::from_str(serialized).map_err(|_| IngressError::ConfigurationUnavailable)?;
        if !valid_key_id(&document.active_key_id)
            || document.keys.is_empty()
            || document.keys.len() > MAX_RETAINED_KEYS
        {
            return Err(IngressError::ConfigurationUnavailable);
        }
        let mut keys = Vec::with_capacity(document.keys.len());
        for mut entry in document.keys {
            if !valid_key_id(&entry.id)
                || keys.iter().any(|key: &ServiceAuthKey| key.id == entry.id)
            {
                entry.key_hex.zeroize();
                return Err(IngressError::ConfigurationUnavailable);
            }
            let mut decoded =
                hex_decode(&entry.key_hex).ok_or(IngressError::ConfigurationUnavailable)?;
            entry.key_hex.zeroize();
            if !(MIN_KEY_BYTES..=MAX_KEY_BYTES).contains(&decoded.len()) {
                decoded.zeroize();
                return Err(IngressError::ConfigurationUnavailable);
            }
            keys.push(ServiceAuthKey {
                id: entry.id.clone(),
                bytes: Zeroizing::new(decoded),
            });
        }
        if !keys.iter().any(|key| key.id == document.active_key_id) {
            return Err(IngressError::ConfigurationUnavailable);
        }
        Ok(Self {
            active_key_id: document.active_key_id,
            keys,
            legacy_serialization: false,
        })
    }

    fn verification_key(&self, version: &str, key_id: Option<&str>) -> Result<&[u8], IngressError> {
        let selected_id = match (version, key_id) {
            (LEGACY_SIGNATURE_VERSION, None) => LEGACY_KEY_ID,
            (KEYED_SIGNATURE_VERSION, Some(key_id))
                if !self.legacy_serialization && valid_key_id(key_id) =>
            {
                key_id
            }
            _ => return Err(IngressError::InvalidAuthentication),
        };
        self.keys
            .iter()
            .find(|key| key.id == selected_id)
            .map(|key| key.bytes.as_slice())
            .ok_or(IngressError::InvalidAuthentication)
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

pub async fn authenticate_request(
    request: &mut Request,
    env: &Env,
    now_ms: u64,
) -> Result<AuthenticatedResolverRequest, IngressError> {
    if request.method() != Method::Post {
        return Err(IngressError::MethodNotAllowed);
    }
    let path = request.path();
    let route = ResolverRoute::parse(&path).ok_or(IngressError::RouteNotFound)?;
    reject_oversized_content_length(request)?;
    let body = Zeroizing::new(
        request
            .bytes()
            .await
            .map_err(|_| IngressError::InvalidDocument)?,
    );
    if body.is_empty() || body.len() > MAX_REQUEST_BYTES {
        return Err(if body.len() > MAX_REQUEST_BYTES {
            IngressError::BodyTooLarge
        } else {
            IngressError::InvalidDocument
        });
    }
    let envelope: ResolverEnvelope =
        serde_json::from_slice(&body).map_err(|_| IngressError::InvalidDocument)?;
    if envelope.purpose != route.purpose() {
        return Err(IngressError::WrongPurpose);
    }
    validate_payload(route, &envelope.payload).map_err(|_| IngressError::InvalidPayload)?;
    if let Some(state) = envelope
        .payload
        .get("oauthState")
        .and_then(|value| value.as_str())
        && state.split_once('.').map(|(tenant, _)| tenant) != Some(envelope.tenant_id.as_str())
    {
        return Err(IngressError::CrossTenantState);
    }

    let version = required_header(request, SIGNATURE_VERSION_HEADER)?;
    let key_id = optional_header(request, KEY_ID_HEADER)?;
    let body_digest = required_header(request, BODY_DIGEST_HEADER)?;
    let timestamp = required_header(request, TIMESTAMP_HEADER)?;
    let timestamp_ms = timestamp
        .parse::<u64>()
        .map_err(|_| IngressError::InvalidAuthentication)?;
    let nonce = required_header(request, NONCE_HEADER)?;
    let signature = required_header(request, SIGNATURE_HEADER)?;
    let caller_secret = Zeroizing::new(
        env.secret(CALLER_AUTH_SECRET)
            .map_err(|_| IngressError::ConfigurationUnavailable)?
            .to_string(),
    );
    let keyring = ServiceAuthKeyring::parse(&caller_secret)?;
    let verification_key = keyring.verification_key(&version, key_id.as_deref())?;
    let signature_input = SignatureInput {
        method: "POST",
        path: &path,
        body: &body,
        tenant_id: &envelope.tenant_id,
        timestamp_ms,
        nonce: &nonce,
    };
    verify_versioned(
        verification_key,
        &version,
        key_id.as_deref(),
        &signature_input,
        &body_digest,
        &signature,
        now_ms,
    )
    .map_err(map_signature_error)?;

    Ok(AuthenticatedResolverRequest {
        route,
        path,
        envelope,
        body_digest,
        timestamp_ms,
        nonce,
    })
}

fn reject_oversized_content_length(request: &Request) -> Result<(), IngressError> {
    let value = request
        .headers()
        .get("content-length")
        .map_err(|_| IngressError::InvalidDocument)?;
    let Some(value) = value else {
        return Ok(());
    };
    let length = value
        .parse::<usize>()
        .map_err(|_| IngressError::InvalidDocument)?;
    if length > MAX_REQUEST_BYTES {
        return Err(IngressError::BodyTooLarge);
    }
    Ok(())
}

fn required_header(request: &Request, name: &str) -> Result<String, IngressError> {
    request
        .headers()
        .get(name)
        .map_err(|_| IngressError::InvalidAuthentication)?
        .filter(|value| !value.is_empty())
        .ok_or(IngressError::MissingAuthentication)
}

fn optional_header(request: &Request, name: &str) -> Result<Option<String>, IngressError> {
    request
        .headers()
        .get(name)
        .map_err(|_| IngressError::InvalidAuthentication)
        .map(|value| value.filter(|item| !item.is_empty()))
}

const fn map_signature_error(error: SignatureError) -> IngressError {
    match error {
        SignatureError::Stale => IngressError::StaleAuthentication,
        SignatureError::InvalidMetadata
        | SignatureError::InvalidDigest
        | SignatureError::InvalidSignature => IngressError::InvalidAuthentication,
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

#[cfg(test)]
mod tests {
    use super::{
        IngressError, KEYED_SIGNATURE_VERSION, LEGACY_KEY_ID, LEGACY_SIGNATURE_VERSION,
        ServiceAuthKeyring, map_signature_error,
    };
    use crate::protocol::SignatureError;

    fn migrated_keyring() -> String {
        format!(
            "{{\"activeKeyId\":\"key-2026-08\",\"keys\":[{{\"id\":\"key-2026-08\",\"keyHex\":\"{}\"}},{{\"id\":\"{LEGACY_KEY_ID}\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        )
    }

    #[test]
    fn raw_secret_remains_legacy_v1_only() -> Result<(), IngressError> {
        let keyring =
            ServiceAuthKeyring::parse(&"legacy-caller-auth-key-material-0123456789".repeat(2))?;
        assert!(
            keyring
                .verification_key(LEGACY_SIGNATURE_VERSION, None)
                .is_ok()
        );
        assert_eq!(
            keyring.verification_key(KEYED_SIGNATURE_VERSION, Some(LEGACY_KEY_ID)),
            Err(IngressError::InvalidAuthentication)
        );
        Ok(())
    }

    #[test]
    fn migrated_keyring_accepts_named_v2_and_explicit_legacy_overlap() -> Result<(), IngressError> {
        let keyring = ServiceAuthKeyring::parse(&migrated_keyring())?;
        assert_eq!(keyring.active_key_id, "key-2026-08");
        assert!(
            keyring
                .verification_key(KEYED_SIGNATURE_VERSION, Some("key-2026-08"))
                .is_ok()
        );
        assert!(
            keyring
                .verification_key(LEGACY_SIGNATURE_VERSION, None)
                .is_ok()
        );
        assert_eq!(
            keyring.verification_key(KEYED_SIGNATURE_VERSION, Some("revoked-key")),
            Err(IngressError::InvalidAuthentication)
        );
        Ok(())
    }

    #[test]
    fn malformed_keyrings_fail_as_configuration() {
        assert_eq!(
            ServiceAuthKeyring::parse("short").err(),
            Some(IngressError::ConfigurationUnavailable)
        );
        let duplicate = format!(
            "{{\"activeKeyId\":\"same\",\"keys\":[{{\"id\":\"same\",\"keyHex\":\"{}\"}},{{\"id\":\"same\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        );
        assert_eq!(
            ServiceAuthKeyring::parse(&duplicate).err(),
            Some(IngressError::ConfigurationUnavailable)
        );
    }

    #[test]
    fn only_stale_signatures_receive_the_stale_classification() {
        assert_eq!(
            map_signature_error(SignatureError::Stale),
            IngressError::StaleAuthentication
        );
        for error in [
            SignatureError::InvalidMetadata,
            SignatureError::InvalidDigest,
            SignatureError::InvalidSignature,
        ] {
            assert_eq!(
                map_signature_error(error),
                IngressError::InvalidAuthentication
            );
        }
    }
}
