use crate::model::AES_GCM_NONCE_BYTES;
use crate::protocol::{hex_decode, hex_encode};
use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Nonce, Payload};
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

type HmacSha256 = Hmac<Sha256>;

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncryptedValue {
    pub key_version: u32,
    pub nonce_hex: String,
    pub ciphertext_hex: String,
}

pub struct ResolverCrypto<N> {
    keyring: EncryptionKeyring,
    handle_hmac_key: Zeroizing<Vec<u8>>,
    nonce_source: N,
}

impl<N: NonceSource> ResolverCrypto<N> {
    pub fn new(
        keyring: EncryptionKeyring,
        handle_hmac_key: Vec<u8>,
        nonce_source: N,
    ) -> Result<Self, CryptoError> {
        if handle_hmac_key.len() < 32 || handle_hmac_key.len() > 128 {
            return Err(CryptoError::InvalidKeyring);
        }
        Ok(Self {
            keyring,
            handle_hmac_key: Zeroizing::new(handle_hmac_key),
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
        let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(self.handle_hmac_key.as_slice())
            .map_err(|_| CryptoError::InvalidKeyring)?;
        mac.update(b"mailbox-resolver-handle\0v1\0");
        mac.update(tenant_id.as_bytes());
        mac.update(b"\0");
        mac.update(handle.as_bytes());
        Ok(hex_encode(mac.finalize().into_bytes().as_slice()))
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
        AuthenticatedContext, CryptoError, EncryptionKeyring, NonceSource, ResolverCrypto,
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
