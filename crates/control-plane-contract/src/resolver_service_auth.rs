use serde::Deserialize;
use zeroize::{Zeroize, Zeroizing};

pub const LEGACY_SIGNATURE_VERSION: &str = "hmac-sha256-v1";
pub const KEYED_SIGNATURE_VERSION: &str = "hmac-sha256-v2";
pub const LEGACY_KEY_ID: &str = "legacy-v1";

const MIN_KEY_BYTES: usize = 32;
const MAX_KEY_BYTES: usize = 128;
const MAX_RETAINED_KEYS: usize = 4;
const MAX_KEY_ID_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceAuthContractError;

pub struct ServiceAuthKeyring {
    active_key_id: String,
    keys: Vec<ServiceAuthKey>,
    legacy_serialization: bool,
}

struct ServiceAuthKey {
    id: String,
    bytes: Zeroizing<Vec<u8>>,
}

pub struct ServiceAuthSigningKey<'a> {
    version: &'static str,
    key_id: Option<&'a str>,
    bytes: &'a [u8],
}

impl ServiceAuthSigningKey<'_> {
    #[must_use]
    pub const fn version(&self) -> &'static str {
        self.version
    }

    #[must_use]
    pub const fn key_id(&self) -> Option<&str> {
        self.key_id
    }

    #[must_use]
    pub const fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceAuthCanonicalInput<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub body_digest: &'a str,
    pub tenant_id: &'a str,
    pub timestamp_ms: u64,
    pub nonce: &'a str,
}

impl ServiceAuthKeyring {
    pub fn parse(serialized: &str) -> Result<Self, ServiceAuthContractError> {
        if serialized.trim_start().starts_with('{') {
            return Self::parse_json(serialized);
        }
        if !valid_legacy_key(serialized.as_bytes()) {
            return Err(ServiceAuthContractError);
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

    fn parse_json(serialized: &str) -> Result<Self, ServiceAuthContractError> {
        let document: ServiceAuthKeyringSecret =
            serde_json::from_str(serialized).map_err(|_| ServiceAuthContractError)?;
        if !valid_key_id(&document.active_key_id)
            || document.keys.is_empty()
            || document.keys.len() > MAX_RETAINED_KEYS
        {
            return Err(ServiceAuthContractError);
        }
        let mut keys = Vec::with_capacity(document.keys.len());
        for mut entry in document.keys {
            if !valid_key_id(&entry.id)
                || keys.iter().any(|key: &ServiceAuthKey| key.id == entry.id)
            {
                entry.key_hex.zeroize();
                return Err(ServiceAuthContractError);
            }
            let mut decoded = hex_decode(&entry.key_hex).ok_or(ServiceAuthContractError)?;
            entry.key_hex.zeroize();
            if !valid_key(&decoded) {
                decoded.zeroize();
                return Err(ServiceAuthContractError);
            }
            keys.push(ServiceAuthKey {
                id: entry.id.clone(),
                bytes: Zeroizing::new(decoded),
            });
        }
        if !keys.iter().any(|key| key.id == document.active_key_id) {
            return Err(ServiceAuthContractError);
        }
        Ok(Self {
            active_key_id: document.active_key_id,
            keys,
            legacy_serialization: false,
        })
    }

    pub fn active_signing_key(
        &self,
    ) -> Result<ServiceAuthSigningKey<'_>, ServiceAuthContractError> {
        let active = self
            .keys
            .iter()
            .find(|key| key.id == self.active_key_id)
            .ok_or(ServiceAuthContractError)?;
        if self.legacy_serialization {
            Ok(ServiceAuthSigningKey {
                version: LEGACY_SIGNATURE_VERSION,
                key_id: None,
                bytes: active.bytes.as_slice(),
            })
        } else {
            Ok(ServiceAuthSigningKey {
                version: KEYED_SIGNATURE_VERSION,
                key_id: Some(active.id.as_str()),
                bytes: active.bytes.as_slice(),
            })
        }
    }

    pub fn verification_key(
        &self,
        version: &str,
        key_id: Option<&str>,
    ) -> Result<&[u8], ServiceAuthContractError> {
        let selected_id = match (version, key_id) {
            (LEGACY_SIGNATURE_VERSION, None) => LEGACY_KEY_ID,
            (KEYED_SIGNATURE_VERSION, Some(key_id))
                if !self.legacy_serialization && valid_key_id(key_id) =>
            {
                key_id
            }
            _ => return Err(ServiceAuthContractError),
        };
        self.keys
            .iter()
            .find(|key| key.id == selected_id)
            .map(|key| key.bytes.as_slice())
            .ok_or(ServiceAuthContractError)
    }

    #[must_use]
    pub fn active_key_id(&self) -> &str {
        &self.active_key_id
    }

    #[must_use]
    pub const fn legacy_serialization(&self) -> bool {
        self.legacy_serialization
    }

    #[must_use]
    pub fn retained_key_ids(&self) -> Vec<&str> {
        self.keys.iter().map(|key| key.id.as_str()).collect()
    }
}

impl core::fmt::Debug for ServiceAuthKeyring {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ServiceAuthKeyring")
            .field("active_key_id", &self.active_key_id)
            .field("retained_key_ids", &self.retained_key_ids())
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

pub fn canonical_signature_input(
    version: &str,
    key_id: Option<&str>,
    input: &ServiceAuthCanonicalInput<'_>,
) -> Result<String, ServiceAuthContractError> {
    match (version, key_id) {
        (LEGACY_SIGNATURE_VERSION, None) => Ok(format!(
            "{LEGACY_SIGNATURE_VERSION}\n{}\n{}\n{}\n{}\n{}\n{}",
            input.method,
            input.path,
            input.body_digest,
            input.tenant_id,
            input.timestamp_ms,
            input.nonce
        )),
        (KEYED_SIGNATURE_VERSION, Some(key_id)) if valid_key_id(key_id) => Ok(format!(
            "{KEYED_SIGNATURE_VERSION}\n{key_id}\n{}\n{}\n{}\n{}\n{}\n{}",
            input.method,
            input.path,
            input.body_digest,
            input.tenant_id,
            input.timestamp_ms,
            input.nonce
        )),
        _ => Err(ServiceAuthContractError),
    }
}

fn valid_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_KEY_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_key(value: &[u8]) -> bool {
    (MIN_KEY_BYTES..=MAX_KEY_BYTES).contains(&value.len())
        && !value.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
}

fn valid_legacy_key(value: &[u8]) -> bool {
    valid_key(value)
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

#[cfg(test)]
mod tests {
    use super::{
        KEYED_SIGNATURE_VERSION, LEGACY_KEY_ID, LEGACY_SIGNATURE_VERSION,
        ServiceAuthCanonicalInput, ServiceAuthContractError, ServiceAuthKeyring,
        canonical_signature_input,
    };

    fn migrated_keyring() -> String {
        format!(
            "{{\"activeKeyId\":\"key-2026-08\",\"keys\":[{{\"id\":\"key-2026-08\",\"keyHex\":\"{}\"}},{{\"id\":\"{LEGACY_KEY_ID}\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        )
    }

    fn canonical_input<'a>(body_digest: &'a str) -> ServiceAuthCanonicalInput<'a> {
        ServiceAuthCanonicalInput {
            method: "POST",
            path: "/v1/mailbox-credentials/resolve",
            body_digest,
            tenant_id: "tenant_01",
            timestamp_ms: 100,
            nonce: "00112233445566778899aabbccddeeff",
        }
    }

    #[test]
    fn raw_secret_is_legacy_v1_only() -> Result<(), ServiceAuthContractError> {
        let keyring =
            ServiceAuthKeyring::parse(&"legacy-caller-auth-key-material-0123456789".repeat(2))?;
        let signing = keyring.active_signing_key()?;
        assert_eq!(signing.version(), LEGACY_SIGNATURE_VERSION);
        assert_eq!(signing.key_id(), None);
        assert_eq!(keyring.active_key_id(), LEGACY_KEY_ID);
        assert!(
            keyring
                .verification_key(LEGACY_SIGNATURE_VERSION, None)
                .is_ok()
        );
        assert!(
            keyring
                .verification_key(KEYED_SIGNATURE_VERSION, Some(LEGACY_KEY_ID))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn migrated_keyring_signs_active_and_verifies_named_overlap()
    -> Result<(), ServiceAuthContractError> {
        let keyring = ServiceAuthKeyring::parse(&migrated_keyring())?;
        let signing = keyring.active_signing_key()?;
        assert_eq!(signing.version(), KEYED_SIGNATURE_VERSION);
        assert_eq!(signing.key_id(), Some("key-2026-08"));
        assert_eq!(keyring.active_key_id(), "key-2026-08");
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
        assert!(
            keyring
                .verification_key(KEYED_SIGNATURE_VERSION, Some("revoked-key"))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn keyring_and_key_id_shapes_fail_closed() {
        assert!(ServiceAuthKeyring::parse("short").is_err());
        let duplicate = format!(
            "{{\"activeKeyId\":\"same\",\"keys\":[{{\"id\":\"same\",\"keyHex\":\"{}\"}},{{\"id\":\"same\",\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        );
        assert!(ServiceAuthKeyring::parse(&duplicate).is_err());
        assert!(ServiceAuthKeyring::parse("{\"activeKeyId\":\"missing\",\"keys\":[]}").is_err());
        let body_digest = "a".repeat(64);
        assert!(
            canonical_signature_input(
                KEYED_SIGNATURE_VERSION,
                Some("bad key id"),
                &canonical_input(&body_digest),
            )
            .is_err()
        );
    }

    #[test]
    fn canonical_v1_and_v2_are_version_bound() -> Result<(), ServiceAuthContractError> {
        let body_digest = "a".repeat(64);
        let input = canonical_input(&body_digest);
        let legacy = canonical_signature_input(LEGACY_SIGNATURE_VERSION, None, &input)?;
        let keyed =
            canonical_signature_input(KEYED_SIGNATURE_VERSION, Some("key-2026-08"), &input)?;
        assert_ne!(legacy, keyed);
        assert!(legacy.starts_with("hmac-sha256-v1\nPOST\n"));
        assert!(keyed.starts_with("hmac-sha256-v2\nkey-2026-08\nPOST\n"));
        Ok(())
    }
}
