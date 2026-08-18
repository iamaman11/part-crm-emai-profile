#![forbid(unsafe_code)]

use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::{Zeroize, Zeroizing};

type HmacSha256 = Hmac<Sha256>;

pub const LEGACY_SIGNATURE_VERSION: &str = "hmac-sha256-v1";
pub const KEYED_SIGNATURE_VERSION: &str = "hmac-sha256-v2";
pub const LEGACY_KEY_ID: &str = "legacy-v1";
pub const MAX_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
const MIN_KEY_BYTES: usize = 32;
const MAX_KEY_BYTES: usize = 128;
const MAX_RETAINED_KEYS: usize = 4;
const MAX_KEY_ID_BYTES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureInput<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub body: &'a [u8],
    pub tenant_id: &'a str,
    pub timestamp_ms: u64,
    pub nonce: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverSignature {
    version: &'static str,
    key_id: Option<String>,
    body_digest: String,
    signature: String,
}

impl ResolverSignature {
    #[must_use]
    pub const fn version(&self) -> &'static str {
        self.version
    }

    #[must_use]
    pub fn key_id(&self) -> Option<&str> {
        self.key_id.as_deref()
    }

    #[must_use]
    pub fn body_digest(&self) -> &str {
        &self.body_digest
    }

    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceAuthError {
    InvalidKeyring,
    UnknownKey,
    InvalidMetadata,
    Stale,
    InvalidDigest,
    InvalidSignature,
}

impl fmt::Display for ServiceAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidKeyring => "resolver service-auth keyring is invalid",
            Self::UnknownKey => "resolver service-auth key is unknown or revoked",
            Self::InvalidMetadata => "resolver service-auth request metadata is invalid",
            Self::Stale => "resolver service-auth request is stale",
            Self::InvalidDigest => "resolver service-auth body digest is invalid",
            Self::InvalidSignature => "resolver service-auth signature is invalid",
        })
    }
}

impl std::error::Error for ServiceAuthError {}

pub struct ServiceAuthKeyring {
    active_key_id: String,
    keys: Vec<ServiceAuthKey>,
    legacy_serialization: bool,
}

struct ServiceAuthKey {
    id: String,
    bytes: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for ServiceAuthKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key_ids = self
            .keys
            .iter()
            .map(|key| key.id.as_str())
            .collect::<Vec<_>>();
        formatter
            .debug_struct("ServiceAuthKeyring")
            .field("active_key_id", &self.active_key_id)
            .field("key_ids", &key_ids)
            .field("legacy_serialization", &self.legacy_serialization)
            .finish()
    }
}

impl ServiceAuthKeyring {
    pub fn parse(serialized: &str) -> Result<Self, ServiceAuthError> {
        if serialized.is_empty() || serialized.len() > 8 * 1024 {
            return Err(ServiceAuthError::InvalidKeyring);
        }
        if serialized.trim_start().starts_with('{') {
            return Self::parse_keyring_json(serialized);
        }
        if !valid_legacy_secret(serialized.as_bytes()) {
            return Err(ServiceAuthError::InvalidKeyring);
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

    fn parse_keyring_json(serialized: &str) -> Result<Self, ServiceAuthError> {
        let document: ServiceAuthKeyringDocument =
            serde_json::from_str(serialized).map_err(|_| ServiceAuthError::InvalidKeyring)?;
        if !valid_key_id(&document.active_key_id)
            || document.keys.is_empty()
            || document.keys.len() > MAX_RETAINED_KEYS
        {
            return Err(ServiceAuthError::InvalidKeyring);
        }
        let mut keys = Vec::with_capacity(document.keys.len());
        for mut entry in document.keys {
            if !valid_key_id(&entry.id) || keys.iter().any(|key: &ServiceAuthKey| key.id == entry.id) {
                entry.key_hex.zeroize();
                return Err(ServiceAuthError::InvalidKeyring);
            }
            let bytes = decode_key_hex(&entry.key_hex)?;
            entry.key_hex.zeroize();
            keys.push(ServiceAuthKey {
                id: entry.id,
                bytes: Zeroizing::new(bytes),
            });
        }
        if !keys.iter().any(|key| key.id == document.active_key_id) {
            return Err(ServiceAuthError::InvalidKeyring);
        }
        Ok(Self {
            active_key_id: document.active_key_id,
            keys,
            legacy_serialization: false,
        })
    }

    #[must_use]
    pub fn active_key_id(&self) -> &str {
        &self.active_key_id
    }

    #[must_use]
    pub const fn is_legacy_serialization(&self) -> bool {
        self.legacy_serialization
    }

    #[must_use]
    pub fn retained_key_ids(&self) -> Vec<&str> {
        self.keys.iter().map(|key| key.id.as_str()).collect()
    }

    pub fn sign_active(
        &self,
        input: &SignatureInput<'_>,
    ) -> Result<ResolverSignature, ServiceAuthError> {
        validate_metadata(input)?;
        let key = self
            .keys
            .iter()
            .find(|key| key.id == self.active_key_id)
            .ok_or(ServiceAuthError::UnknownKey)?;
        let version = if self.legacy_serialization {
            LEGACY_SIGNATURE_VERSION
        } else {
            KEYED_SIGNATURE_VERSION
        };
        let key_id = if self.legacy_serialization {
            None
        } else {
            Some(key.id.clone())
        };
        let body_digest = body_digest_hex(input.body);
        let canonical = canonical(version, key_id.as_deref(), input, &body_digest)?;
        let signature = hmac_hex(key.bytes.as_slice(), canonical.as_bytes())?;
        Ok(ResolverSignature {
            version,
            key_id,
            body_digest,
            signature,
        })
    }

    pub fn verify(
        &self,
        version: &str,
        key_id: Option<&str>,
        input: &SignatureInput<'_>,
        supplied_body_digest: &str,
        supplied_signature: &str,
        now_ms: u64,
    ) -> Result<(), ServiceAuthError> {
        validate_metadata(input)?;
        if input.timestamp_ms.abs_diff(now_ms) > MAX_CLOCK_SKEW_MS {
            return Err(ServiceAuthError::Stale);
        }
        let digest = body_digest_hex(input.body);
        if !constant_time_hex_eq(&digest, supplied_body_digest) {
            return Err(ServiceAuthError::InvalidDigest);
        }
        let key = match version {
            LEGACY_SIGNATURE_VERSION if key_id.is_none() => self
                .keys
                .iter()
                .find(|key| key.id == LEGACY_KEY_ID)
                .ok_or(ServiceAuthError::UnknownKey)?,
            KEYED_SIGNATURE_VERSION => {
                let key_id = key_id
                    .filter(|value| valid_key_id(value))
                    .ok_or(ServiceAuthError::InvalidMetadata)?;
                self.keys
                    .iter()
                    .find(|key| key.id == key_id)
                    .ok_or(ServiceAuthError::UnknownKey)?
            }
            _ => return Err(ServiceAuthError::InvalidMetadata),
        };
        let canonical = canonical(version, key_id, input, &digest)?;
        let expected = hmac_hex(key.bytes.as_slice(), canonical.as_bytes())?;
        if !constant_time_hex_eq(&expected, supplied_signature) {
            return Err(ServiceAuthError::InvalidSignature);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceAuthKeyringDocument {
    active_key_id: String,
    keys: Vec<ServiceAuthKeyDocument>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceAuthKeyDocument {
    id: String,
    key_hex: String,
}

impl Drop for ServiceAuthKeyDocument {
    fn drop(&mut self) {
        self.key_hex.zeroize();
    }
}

#[must_use]
pub fn body_digest_hex(body: &[u8]) -> String {
    hex_encode(Sha256::digest(body).as_slice())
}

fn canonical(
    version: &str,
    key_id: Option<&str>,
    input: &SignatureInput<'_>,
    body_digest: &str,
) -> Result<String, ServiceAuthError> {
    match version {
        LEGACY_SIGNATURE_VERSION if key_id.is_none() => Ok(format!(
            "{LEGACY_SIGNATURE_VERSION}\n{}\n{}\n{body_digest}\n{}\n{}\n{}",
            input.method, input.path, input.tenant_id, input.timestamp_ms, input.nonce
        )),
        KEYED_SIGNATURE_VERSION => {
            let key_id = key_id
                .filter(|value| valid_key_id(value))
                .ok_or(ServiceAuthError::InvalidMetadata)?;
            Ok(format!(
                "{KEYED_SIGNATURE_VERSION}\n{key_id}\n{}\n{}\n{body_digest}\n{}\n{}\n{}",
                input.method, input.path, input.tenant_id, input.timestamp_ms, input.nonce
            ))
        }
        _ => Err(ServiceAuthError::InvalidMetadata),
    }
}

fn validate_metadata(input: &SignatureInput<'_>) -> Result<(), ServiceAuthError> {
    let valid_method = input.method == "POST";
    let valid_path = input.path.starts_with("/v1/mailbox-credentials/")
        && input.path.len() <= 160
        && !input.path.contains('?');
    let valid_tenant = valid_identifier(input.tenant_id, 128);
    let valid_nonce = input.nonce.len() == 32
        && input
            .nonce
            .bytes()
            .all(|value| value.is_ascii_hexdigit());
    if valid_method && valid_path && valid_tenant && valid_nonce && input.timestamp_ms > 0 {
        Ok(())
    } else {
        Err(ServiceAuthError::InvalidMetadata)
    }
}

fn valid_identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

fn valid_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_KEY_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_legacy_secret(value: &[u8]) -> bool {
    (MIN_KEY_BYTES..=MAX_KEY_BYTES).contains(&value.len())
        && !value.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
}

fn decode_key_hex(value: &str) -> Result<Vec<u8>, ServiceAuthError> {
    if !value.len().is_multiple_of(2)
        || !(MIN_KEY_BYTES * 2..=MAX_KEY_BYTES * 2).contains(&value.len())
    {
        return Err(ServiceAuthError::InvalidKeyring);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0]).ok_or(ServiceAuthError::InvalidKeyring)?;
            let low = hex_nibble(pair[1]).ok_or(ServiceAuthError::InvalidKeyring)?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hmac_hex(key: &[u8], canonical: &[u8]) -> Result<String, ServiceAuthError> {
    let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(key)
        .map_err(|_| ServiceAuthError::InvalidKeyring)?;
    mac.update(canonical);
    Ok(hex_encode(mac.finalize().into_bytes().as_slice()))
}

fn constant_time_hex_eq(expected: &str, supplied: &str) -> bool {
    if expected.len() != supplied.len() {
        return false;
    }
    expected
        .bytes()
        .zip(supplied.bytes())
        .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
        == 0
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

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KEYED_SIGNATURE_VERSION, LEGACY_KEY_ID, LEGACY_SIGNATURE_VERSION, ServiceAuthError,
        ServiceAuthKeyring, SignatureInput, body_digest_hex,
    };

    fn input<'a>(body: &'a [u8]) -> SignatureInput<'a> {
        SignatureInput {
            method: "POST",
            path: "/v1/mailbox-credentials/resolve",
            body,
            tenant_id: "tenant_01",
            timestamp_ms: 1_000_000,
            nonce: "00112233445566778899aabbccddeeff",
        }
    }

    fn migrated_keyring() -> String {
        format!(
            "{{\"activeKeyId\":\"key-2026-08\",\"keys\":[{{\"id\":\"key-2026-08\",\"keyHex\":\"{}\"}},{{\"id\":\"{LEGACY_KEY_ID}\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        )
    }

    #[test]
    fn legacy_raw_secret_preserves_v1_wire_compatibility() -> Result<(), ServiceAuthError> {
        let keyring = ServiceAuthKeyring::parse(&"legacy-caller-auth-key-material-0123456789".repeat(2))?;
        let request = input(br#"{"tenantId":"tenant_01"}"#);
        let signed = keyring.sign_active(&request)?;
        assert_eq!(signed.version(), LEGACY_SIGNATURE_VERSION);
        assert_eq!(signed.key_id(), None);
        keyring.verify(
            signed.version(),
            signed.key_id(),
            &request,
            signed.body_digest(),
            signed.signature(),
            1_000_001,
        )?;
        Ok(())
    }

    #[test]
    fn keyed_v2_binds_exact_key_id_and_supports_legacy_overlap() -> Result<(), ServiceAuthError> {
        let keyring = ServiceAuthKeyring::parse(&migrated_keyring())?;
        let request = input(br#"{"tenantId":"tenant_01"}"#);
        let signed = keyring.sign_active(&request)?;
        assert_eq!(signed.version(), KEYED_SIGNATURE_VERSION);
        assert_eq!(signed.key_id(), Some("key-2026-08"));
        keyring.verify(
            signed.version(),
            signed.key_id(),
            &request,
            signed.body_digest(),
            signed.signature(),
            1_000_001,
        )?;
        assert_eq!(
            keyring.verify(
                signed.version(),
                Some(LEGACY_KEY_ID),
                &request,
                signed.body_digest(),
                signed.signature(),
                1_000_001,
            ),
            Err(ServiceAuthError::InvalidSignature)
        );

        let legacy_only = ServiceAuthKeyring::parse(&String::from_utf8(vec![0x22; 32]).map_err(|_| ServiceAuthError::InvalidKeyring)?)?;
        let legacy_signed = legacy_only.sign_active(&request)?;
        keyring.verify(
            LEGACY_SIGNATURE_VERSION,
            None,
            &request,
            legacy_signed.body_digest(),
            legacy_signed.signature(),
            1_000_001,
        )?;
        Ok(())
    }

    #[test]
    fn unknown_key_stale_request_and_body_tamper_fail_closed() -> Result<(), ServiceAuthError> {
        let keyring = ServiceAuthKeyring::parse(&migrated_keyring())?;
        let request = input(br#"{"tenantId":"tenant_01"}"#);
        let signed = keyring.sign_active(&request)?;
        assert_eq!(
            keyring.verify(
                KEYED_SIGNATURE_VERSION,
                Some("revoked-key"),
                &request,
                signed.body_digest(),
                signed.signature(),
                1_000_001,
            ),
            Err(ServiceAuthError::UnknownKey)
        );
        assert_eq!(
            keyring.verify(
                signed.version(),
                signed.key_id(),
                &request,
                signed.body_digest(),
                signed.signature(),
                1_400_001,
            ),
            Err(ServiceAuthError::Stale)
        );
        assert_eq!(
            keyring.verify(
                signed.version(),
                signed.key_id(),
                &input(br#"{"tenantId":"tenant_01","changed":true}"#),
                signed.body_digest(),
                signed.signature(),
                1_000_001,
            ),
            Err(ServiceAuthError::InvalidDigest)
        );
        Ok(())
    }

    #[test]
    fn malformed_or_duplicate_keyrings_are_rejected() {
        assert!(ServiceAuthKeyring::parse("short").is_err());
        let duplicate = format!(
            "{{\"activeKeyId\":\"same\",\"keys\":[{{\"id\":\"same\",\"keyHex\":\"{}\"}},{{\"id\":\"same\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        );
        assert!(ServiceAuthKeyring::parse(&duplicate).is_err());
        assert!(ServiceAuthKeyring::parse("{\"activeKeyId\":\"missing\",\"keys\":[]}").is_err());
    }

    #[test]
    fn body_digest_is_lowercase_sha256() {
        let digest = body_digest_hex(b"resolver-auth");
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')));
    }
}
