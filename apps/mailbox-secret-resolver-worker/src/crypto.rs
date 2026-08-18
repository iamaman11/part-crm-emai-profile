use crate::model::AES_GCM_NONCE_BYTES;
use crate::protocol::{hex_decode, hex_encode};
use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Nonce, Payload};
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

type HmacSha256 = Hmac<Sha256>;
const MAX_RETAINED_HANDLE_HMAC_KEYS: usize = 4;
const MIN_HANDLE_HMAC_KEY_BYTES: usize = 32;
const MAX_HANDLE_HMAC_KEY_BYTES: usize = 128;
const LEGACY_HANDLE_HMAC_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CryptoError {
    InvalidKeyring,
    KeyUnavailable,
    RandomnessUnavailable,
    EncryptionFailed,
    AuthenticationFailed,
    InvalidEnvelope,
}

pub trait NonceSource {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CryptoError>;
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkerNonceSource;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkerNonceSource;

#[cfg(target_arch = "wasm32")]
impl NonceSource for WorkerNonceSource {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CryptoError> {
        use worker::wasm_bindgen::JsCast;
        let scope = worker::js_sys::global()
            .dyn_into::<web_sys::WorkerGlobalScope>()
            .map_err(|_| CryptoError::RandomnessUnavailable)?;
        scope
            .crypto()
            .map_err(|_| CryptoError::RandomnessUnavailable)?
            .get_random_values_with_u8_array(bytes)
            .map_err(|_| CryptoError::RandomnessUnavailable)?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl NonceSource for WorkerNonceSource {
    fn fill(&self, _bytes: &mut [u8]) -> Result<(), CryptoError> {
        Err(CryptoError::RandomnessUnavailable)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncryptionKeyringSecret {
    active_version: u32,
    keys: Vec<EncryptionKeySecret>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncryptionKeySecret {
    version: u32,
    key_hex: String,
}

pub struct EncryptionKeyring {
    active_version: u32,
    keys: Vec<EncryptionKey>,
}

struct EncryptionKey {
    version: u32,
    bytes: Zeroizing<[u8; 32]>,
}

impl EncryptionKeyring {
    pub fn parse(json: &str) -> Result<Self, CryptoError> {
        let secret: EncryptionKeyringSecret =
            serde_json::from_str(json).map_err(|_| CryptoError::InvalidKeyring)?;
        if secret.active_version == 0 || secret.keys.is_empty() || secret.keys.len() > 4 {
            return Err(CryptoError::InvalidKeyring);
        }
        let mut keys = Vec::with_capacity(secret.keys.len());
        for entry in secret.keys {
            if entry.version == 0
                || keys
                    .iter()
                    .any(|key: &EncryptionKey| key.version == entry.version)
            {
                return Err(CryptoError::InvalidKeyring);
            }
            let mut decoded = hex_decode(&entry.key_hex).ok_or(CryptoError::InvalidKeyring)?;
            let bytes: [u8; 32] = decoded
                .as_slice()
                .try_into()
                .map_err(|_| CryptoError::InvalidKeyring)?;
            decoded.zeroize();
            keys.push(EncryptionKey {
                version: entry.version,
                bytes: Zeroizing::new(bytes),
            });
        }
        if !keys.iter().any(|key| key.version == secret.active_version) {
            return Err(CryptoError::InvalidKeyring);
        }
        Ok(Self {
            active_version: secret.active_version,
            keys,
        })
    }

    fn active(&self) -> Result<&EncryptionKey, CryptoError> {
        self.keys
            .iter()
            .find(|key| key.version == self.active_version)
            .ok_or(CryptoError::KeyUnavailable)
    }

    #[must_use]
    pub const fn active_version(&self) -> u32 {
        self.active_version
    }

    fn version(&self, version: u32) -> Result<&EncryptionKey, CryptoError> {
        self.keys
            .iter()
            .find(|key| key.version == version)
            .ok_or(CryptoError::KeyUnavailable)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HandleHmacKeyringSecret {
    active_version: u32,
    keys: Vec<HandleHmacKeySecret>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HandleHmacKeySecret {
    version: u32,
    key_hex: String,
}

impl Drop for HandleHmacKeySecret {
    fn drop(&mut self) {
        self.key_hex.zeroize();
    }
}

pub struct HandleHmacKeyring {
    active_version: u32,
    keys: Vec<HandleHmacKey>,
    legacy_serialization: bool,
}

struct HandleHmacKey {
    version: u32,
    bytes: Zeroizing<Vec<u8>>,
}

impl HandleHmacKeyring {
    pub fn parse(serialized: &str) -> Result<Self, CryptoError> {
        if serialized.trim_start().starts_with('{') {
            return Self::parse_json(serialized);
        }
        Self::legacy(serialized.as_bytes().to_vec())
    }

    pub fn legacy(mut key: Vec<u8>) -> Result<Self, CryptoError> {
        if !valid_handle_hmac_key(&key) {
            key.zeroize();
            return Err(CryptoError::InvalidKeyring);
        }
        Ok(Self {
            active_version: LEGACY_HANDLE_HMAC_VERSION,
            keys: vec![HandleHmacKey {
                version: LEGACY_HANDLE_HMAC_VERSION,
                bytes: Zeroizing::new(key),
            }],
            legacy_serialization: true,
        })
    }

    fn parse_json(serialized: &str) -> Result<Self, CryptoError> {
        let secret: HandleHmacKeyringSecret =
            serde_json::from_str(serialized).map_err(|_| CryptoError::InvalidKeyring)?;
        if secret.active_version == 0
            || secret.keys.is_empty()
            || secret.keys.len() > MAX_RETAINED_HANDLE_HMAC_KEYS
        {
            return Err(CryptoError::InvalidKeyring);
        }
        let mut keys = Vec::with_capacity(secret.keys.len());
        for mut entry in secret.keys {
            if entry.version == 0
                || keys
                    .iter()
                    .any(|key: &HandleHmacKey| key.version == entry.version)
            {
                entry.key_hex.zeroize();
                return Err(CryptoError::InvalidKeyring);
            }
            let mut decoded = hex_decode(&entry.key_hex).ok_or(CryptoError::InvalidKeyring)?;
            entry.key_hex.zeroize();
            if !valid_handle_hmac_key(&decoded) {
                decoded.zeroize();
                return Err(CryptoError::InvalidKeyring);
            }
            keys.push(HandleHmacKey {
                version: entry.version,
                bytes: Zeroizing::new(decoded),
            });
        }
        if !keys.iter().any(|key| key.version == secret.active_version) {
            return Err(CryptoError::InvalidKeyring);
        }
        keys.sort_by_key(|key| key.version);
        Ok(Self {
            active_version: secret.active_version,
            keys,
            legacy_serialization: false,
        })
    }

    fn active(&self) -> Result<&HandleHmacKey, CryptoError> {
        self.version(self.active_version)
    }

    fn version(&self, version: u32) -> Result<&HandleHmacKey, CryptoError> {
        self.keys
            .iter()
            .find(|key| key.version == version)
            .ok_or(CryptoError::KeyUnavailable)
    }

    #[must_use]
    pub const fn active_version(&self) -> u32 {
        self.active_version
    }

    #[must_use]
    pub const fn legacy_serialization(&self) -> bool {
        self.legacy_serialization
    }

    #[must_use]
    pub fn retained_versions(&self) -> Vec<u32> {
        self.keys.iter().map(|key| key.version).collect()
    }
}

fn valid_handle_hmac_key(key: &[u8]) -> bool {
    (MIN_HANDLE_HMAC_KEY_BYTES..=MAX_HANDLE_HMAC_KEY_BYTES).contains(&key.len())
        && !key.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncryptedValue {
    pub key_version: u32,
    pub nonce_hex: String,
    pub ciphertext_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LookupDigest {
    pub version: u32,
    pub digest: String,
}

pub struct ResolverCrypto<N> {
    keyring: EncryptionKeyring,
    handle_hmac_keyring: HandleHmacKeyring,
    nonce_source: N,
}

impl<N: NonceSource> ResolverCrypto<N> {
    pub fn new(
        keyring: EncryptionKeyring,
        handle_hmac_key: Vec<u8>,
        nonce_source: N,
    ) -> Result<Self, CryptoError> {
        Self::new_with_handle_keyring(
            keyring,
            HandleHmacKeyring::legacy(handle_hmac_key)?,
            nonce_source,
        )
    }

    pub fn new_with_handle_keyring(
        keyring: EncryptionKeyring,
        handle_hmac_keyring: HandleHmacKeyring,
        nonce_source: N,
    ) -> Result<Self, CryptoError> {
        handle_hmac_keyring.active()?;
        Ok(Self {
            keyring,
            handle_hmac_keyring,
            nonce_source,
        })
    }

    pub fn encrypt(
        &self,
        plaintext: &[u8],
        context: &AuthenticatedContext<'_>,
    ) -> Result<EncryptedValue, CryptoError> {
        let key = self.keyring.active()?;
        let mut nonce_bytes = [0_u8; AES_GCM_NONCE_BYTES];
        self.nonce_source.fill(&mut nonce_bytes)?;
        let cipher = Aes256Gcm::new_from_slice(key.bytes.as_slice())
            .map_err(|_| CryptoError::InvalidKeyring)?;
        let aad = context.bytes(key.version);
        let nonce = Nonce::<Aes256Gcm>::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| CryptoError::EncryptionFailed)?;
        Ok(EncryptedValue {
            key_version: key.version,
            nonce_hex: hex_encode(&nonce_bytes),
            ciphertext_hex: hex_encode(&ciphertext),
        })
    }

    pub fn decrypt(
        &self,
        value: &EncryptedValue,
        context: &AuthenticatedContext<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        if value.key_version > self.keyring.active_version() {
            return Err(CryptoError::InvalidKeyring);
        }
        let key = self.keyring.version(value.key_version)?;
        let nonce_bytes = hex_decode(&value.nonce_hex).ok_or(CryptoError::InvalidEnvelope)?;
        if nonce_bytes.len() != AES_GCM_NONCE_BYTES {
            return Err(CryptoError::InvalidEnvelope);
        }
        let ciphertext = hex_decode(&value.ciphertext_hex).ok_or(CryptoError::InvalidEnvelope)?;
        let cipher = Aes256Gcm::new_from_slice(key.bytes.as_slice())
            .map_err(|_| CryptoError::InvalidKeyring)?;
        let nonce: Nonce<Aes256Gcm> = nonce_bytes
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::InvalidEnvelope)?;
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &ciphertext,
                    aad: &context.bytes(value.key_version),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::AuthenticationFailed)
    }

    pub fn lookup_digest(&self, tenant_id: &str, handle: &str) -> Result<String, CryptoError> {
        self.active_lookup_digest(tenant_id, handle)
            .map(|candidate| candidate.digest)
    }

    pub fn active_lookup_digest(
        &self,
        tenant_id: &str,
        handle: &str,
    ) -> Result<LookupDigest, CryptoError> {
        self.lookup_digest_for_version(self.handle_hmac_keyring.active_version(), tenant_id, handle)
    }

    pub fn lookup_digest_for_version(
        &self,
        version: u32,
        tenant_id: &str,
        handle: &str,
    ) -> Result<LookupDigest, CryptoError> {
        let key = self.handle_hmac_keyring.version(version)?;
        let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(key.bytes.as_slice())
            .map_err(|_| CryptoError::InvalidKeyring)?;
        mac.update(b"mailbox-resolver-handle\0v1\0");
        mac.update(tenant_id.as_bytes());
        mac.update(b"\0");
        mac.update(handle.as_bytes());
        Ok(LookupDigest {
            version,
            digest: hex_encode(mac.finalize().into_bytes().as_slice()),
        })
    }

    pub fn lookup_candidates(
        &self,
        tenant_id: &str,
        handle: &str,
    ) -> Result<Vec<LookupDigest>, CryptoError> {
        self.handle_hmac_keyring
            .keys
            .iter()
            .map(|key| self.lookup_digest_for_version(key.version, tenant_id, handle))
            .collect()
    }

    pub fn random_handle(&self, prefix: &str, random_bytes: usize) -> Result<String, CryptoError> {
        if prefix.is_empty()
            || prefix.len() > 32
            || !(16..=48).contains(&random_bytes)
            || !prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(CryptoError::InvalidEnvelope);
        }
        let mut bytes = Zeroizing::new(vec![0_u8; random_bytes]);
        self.nonce_source.fill(&mut bytes)?;
        Ok(format!("{prefix}{}", hex_encode(&bytes)))
    }

    #[must_use]
    pub const fn active_key_version(&self) -> u32 {
        self.keyring.active_version()
    }

    #[must_use]
    pub const fn active_lookup_hmac_version(&self) -> u32 {
        self.handle_hmac_keyring.active_version()
    }

    #[must_use]
    pub fn retained_lookup_hmac_versions(&self) -> Vec<u32> {
        self.handle_hmac_keyring.retained_versions()
    }
}

pub struct AuthenticatedContext<'a> {
    pub tenant_id: &'a str,
    pub provider: &'a str,
    pub record_kind: &'a str,
    pub logical_id: &'a str,
}

impl AuthenticatedContext<'_> {
    fn bytes(&self, key_version: u32) -> Vec<u8> {
        format!(
            "mailbox-resolver-aes-gcm\0v1\0{key_version}\0{}\0{}\0{}\0{}",
            self.tenant_id, self.provider, self.record_kind, self.logical_id
        )
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthenticatedContext, CryptoError, EncryptionKeyring, HandleHmacKeyring, NonceSource,
        ResolverCrypto,
    };

    #[derive(Clone, Copy)]
    struct FixedNonce;

    impl NonceSource for FixedNonce {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), CryptoError> {
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = u8::try_from(index + 1).map_err(|_| CryptoError::RandomnessUnavailable)?;
            }
            Ok(())
        }
    }

    fn encryption_keyring() -> Result<EncryptionKeyring, CryptoError> {
        EncryptionKeyring::parse(
            r#"{"activeVersion":2,"keys":[{"version":1,"keyHex":"1111111111111111111111111111111111111111111111111111111111111111"},{"version":2,"keyHex":"2222222222222222222222222222222222222222222222222222222222222222"}]}"#,
        )
    }

    fn crypto() -> Result<ResolverCrypto<FixedNonce>, CryptoError> {
        crypto_with_active_version(2)
    }

    fn crypto_with_active_version(
        active_version: u32,
    ) -> Result<ResolverCrypto<FixedNonce>, CryptoError> {
        let keyring = EncryptionKeyring::parse(&format!(
            r#"{{"activeVersion":{active_version},"keys":[{{"version":1,"keyHex":"1111111111111111111111111111111111111111111111111111111111111111"}},{{"version":2,"keyHex":"2222222222222222222222222222222222222222222222222222222222222222"}}]}}"#,
        ))?;
        ResolverCrypto::new(keyring, vec![0x44; 32], FixedNonce)
    }

    fn crypto_with_handle_keyring(
        active_version: u32,
    ) -> Result<ResolverCrypto<FixedNonce>, CryptoError> {
        let handle_keyring = HandleHmacKeyring::parse(&format!(
            r#"{{"activeVersion":{active_version},"keys":[{{"version":1,"keyHex":"{}"}},{{"version":2,"keyHex":"{}"}}]}}"#,
            "44".repeat(32),
            "55".repeat(32),
        ))?;
        ResolverCrypto::new_with_handle_keyring(encryption_keyring()?, handle_keyring, FixedNonce)
    }

    #[test]
    fn aes_256_gcm_authenticates_full_context_and_key_version() -> Result<(), CryptoError> {
        let crypto = crypto()?;
        let context = AuthenticatedContext {
            tenant_id: "tenant_01",
            provider: "GMAIL_API",
            record_kind: "credential",
            logical_id: "secret_01",
        };
        let encrypted = crypto.encrypt(b"sensitive", &context)?;
        assert_eq!(encrypted.key_version, 2);
        assert_eq!(encrypted.nonce_hex.len(), 24);
        let decrypted = crypto.decrypt(&encrypted, &context)?;
        assert_eq!(decrypted.as_slice(), b"sensitive");
        let wrong_tenant = AuthenticatedContext {
            tenant_id: "tenant_02",
            ..context
        };
        assert_eq!(
            crypto.decrypt(&encrypted, &wrong_tenant),
            Err(CryptoError::AuthenticationFailed)
        );
        Ok(())
    }

    #[test]
    fn keyring_rollback_cannot_decrypt_or_reencrypt_newer_ciphertext() -> Result<(), CryptoError> {
        let active = crypto_with_active_version(2)?;
        let rolled_back = crypto_with_active_version(1)?;
        let context = AuthenticatedContext {
            tenant_id: "tenant_01",
            provider: "MICROSOFT_GRAPH",
            record_kind: "credential",
            logical_id: "secret_01",
        };
        let encrypted = active.encrypt(b"sensitive", &context)?;
        assert_eq!(encrypted.key_version, 2);
        assert_eq!(
            rolled_back.decrypt(&encrypted, &context),
            Err(CryptoError::InvalidKeyring)
        );
        Ok(())
    }

    #[test]
    fn lookup_digest_is_tenant_scoped_and_does_not_expose_handle() -> Result<(), CryptoError> {
        let crypto = crypto()?;
        let first = crypto.lookup_digest("tenant_01", "secret_handle")?;
        let second = crypto.lookup_digest("tenant_02", "secret_handle")?;
        assert_ne!(first, second);
        assert!(!first.contains("secret_handle"));
        Ok(())
    }

    #[test]
    fn legacy_handle_hmac_secret_maps_to_version_one_only() -> Result<(), CryptoError> {
        let keyring =
            HandleHmacKeyring::parse(&"legacy-handle-hmac-key-material-0123456789".repeat(2))?;
        assert_eq!(keyring.active_version(), 1);
        assert_eq!(keyring.retained_versions(), vec![1]);
        assert!(keyring.legacy_serialization());
        Ok(())
    }

    #[test]
    fn versioned_handle_hmac_keyring_reads_old_and_writes_active() -> Result<(), CryptoError> {
        let crypto = crypto_with_handle_keyring(2)?;
        let candidates = crypto.lookup_candidates("tenant_01", "secret_handle")?;
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.version)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(crypto.active_lookup_hmac_version(), 2);
        assert_eq!(
            crypto.lookup_digest("tenant_01", "secret_handle")?,
            candidates[1].digest
        );
        assert_ne!(candidates[0].digest, candidates[1].digest);
        Ok(())
    }

    #[test]
    fn handle_hmac_unknown_duplicate_and_missing_active_versions_fail_closed()
    -> Result<(), CryptoError> {
        let duplicate = format!(
            r#"{{"activeVersion":1,"keys":[{{"version":1,"keyHex":"{}"}},{{"version":1,"keyHex":"{}"}}]}}"#,
            "44".repeat(32),
            "55".repeat(32),
        );
        assert!(HandleHmacKeyring::parse(&duplicate).is_err());
        let missing_active = format!(
            r#"{{"activeVersion":3,"keys":[{{"version":1,"keyHex":"{}"}},{{"version":2,"keyHex":"{}"}}]}}"#,
            "44".repeat(32),
            "55".repeat(32),
        );
        assert!(HandleHmacKeyring::parse(&missing_active).is_err());
        let crypto = crypto_with_handle_keyring(2)?;
        assert_eq!(
            crypto.lookup_digest_for_version(3, "tenant_01", "secret_handle"),
            Err(CryptoError::KeyUnavailable)
        );
        Ok(())
    }

    #[test]
    fn opaque_handles_are_prefixed_and_randomness_bounded() -> Result<(), CryptoError> {
        let value = crypto()?.random_handle("secret_", 24)?;
        assert!(value.starts_with("secret_"));
        assert_eq!(value.len(), 7 + 48);
        assert_eq!(
            crypto()?.random_handle("bad prefix", 24),
            Err(CryptoError::InvalidEnvelope)
        );
        Ok(())
    }
}
