use application_ports::clients::{
    ContactEncryptionRequest, ContactExactLookupRequest, ContactProtectionPort,
    ContactProtectionPortError, ContactProtectionPortErrorClass,
};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use client_domain::{
    ContactProtectionVersion, EncryptedContactValue, EncryptionKeyVersion, ExactLookupHmacInput,
    ExactLookupToken, LookupKeyVersion,
};
use core::fmt;
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use profile_platform_primitives::{ContactPointId, TenantId};
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

const NONCE_LENGTH: usize = 24;
const ENCRYPTION_KDF_DOMAIN: &[u8] = b"client-contact-encryption-key\0v1\0";
const LOOKUP_KDF_DOMAIN: &[u8] = b"client-contact-lookup-key\0v1\0";
const AEAD_AAD_DOMAIN: &[u8] = b"client-contact-display-aead\0v1\0";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactCryptoError {
    KeyUnavailable,
    InvalidKeyring,
    RandomnessUnavailable,
    EncryptionFailed,
    AuthenticationFailed,
    InvalidProtectedValue,
    InvalidUtf8,
}

impl fmt::Display for ContactCryptoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::KeyUnavailable => "contact protection key unavailable",
            Self::InvalidKeyring => "contact protection keyring is invalid",
            Self::RandomnessUnavailable => "contact protection randomness unavailable",
            Self::EncryptionFailed => "contact protection encryption failed",
            Self::AuthenticationFailed => "contact protection authentication failed",
            Self::InvalidProtectedValue => "contact protected value is invalid",
            Self::InvalidUtf8 => "contact protected plaintext is not valid UTF-8",
        })
    }
}

impl std::error::Error for ContactCryptoError {}

pub struct ContactEncryptionRootKey {
    version: EncryptionKeyVersion,
    bytes: [u8; 32],
}

impl ContactEncryptionRootKey {
    #[must_use]
    pub const fn new(version: EncryptionKeyVersion, bytes: [u8; 32]) -> Self {
        Self { version, bytes }
    }

    #[must_use]
    pub const fn version(&self) -> EncryptionKeyVersion {
        self.version
    }
}

impl fmt::Debug for ContactEncryptionRootKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContactEncryptionRootKey")
            .field("version", &self.version)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl Drop for ContactEncryptionRootKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

pub struct ContactLookupRootKey {
    version: LookupKeyVersion,
    bytes: [u8; 32],
}

impl ContactLookupRootKey {
    #[must_use]
    pub const fn new(version: LookupKeyVersion, bytes: [u8; 32]) -> Self {
        Self { version, bytes }
    }

    #[must_use]
    pub const fn version(&self) -> LookupKeyVersion {
        self.version
    }
}

impl fmt::Debug for ContactLookupRootKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContactLookupRootKey")
            .field("version", &self.version)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl Drop for ContactLookupRootKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

pub struct ContactProtectionKeyring {
    encryption_keys: Vec<ContactEncryptionRootKey>,
    lookup_keys: Vec<ContactLookupRootKey>,
}

impl ContactProtectionKeyring {
    pub fn new(
        encryption_keys: Vec<ContactEncryptionRootKey>,
        lookup_keys: Vec<ContactLookupRootKey>,
    ) -> Result<Self, ContactCryptoError> {
        if encryption_keys.is_empty() || lookup_keys.is_empty() {
            return Err(ContactCryptoError::InvalidKeyring);
        }
        for (index, key) in encryption_keys.iter().enumerate() {
            if encryption_keys[..index]
                .iter()
                .any(|other| other.version() == key.version())
            {
                return Err(ContactCryptoError::InvalidKeyring);
            }
        }
        for (index, key) in lookup_keys.iter().enumerate() {
            if lookup_keys[..index]
                .iter()
                .any(|other| other.version() == key.version())
            {
                return Err(ContactCryptoError::InvalidKeyring);
            }
        }
        Ok(Self {
            encryption_keys,
            lookup_keys,
        })
    }

    fn current_encryption(&self) -> Result<&ContactEncryptionRootKey, ContactCryptoError> {
        self.encryption_keys
            .first()
            .ok_or(ContactCryptoError::KeyUnavailable)
    }

    fn current_lookup(&self) -> Result<&ContactLookupRootKey, ContactCryptoError> {
        self.lookup_keys
            .first()
            .ok_or(ContactCryptoError::KeyUnavailable)
    }

    fn encryption_by_version(
        &self,
        version: EncryptionKeyVersion,
    ) -> Result<&ContactEncryptionRootKey, ContactCryptoError> {
        self.encryption_keys
            .iter()
            .find(|key| key.version() == version)
            .ok_or(ContactCryptoError::KeyUnavailable)
    }

    #[must_use]
    pub fn lookup_versions(&self) -> Vec<LookupKeyVersion> {
        self.lookup_keys
            .iter()
            .map(ContactLookupRootKey::version)
            .collect()
    }
}

impl fmt::Debug for ContactProtectionKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encryption_versions: Vec<u32> = self
            .encryption_keys
            .iter()
            .map(|key| key.version().value())
            .collect();
        let lookup_versions: Vec<u32> = self
            .lookup_keys
            .iter()
            .map(|key| key.version().value())
            .collect();
        formatter
            .debug_struct("ContactProtectionKeyring")
            .field("encryption_versions", &encryption_versions)
            .field("lookup_versions", &lookup_versions)
            .finish()
    }
}

pub trait ContactNonceSource {
    fn fill_nonce(&self, nonce: &mut [u8; NONCE_LENGTH]) -> Result<(), ContactCryptoError>;
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkerCryptoNonceSource;

#[cfg(target_arch = "wasm32")]
impl ContactNonceSource for WorkerCryptoNonceSource {
    fn fill_nonce(&self, nonce: &mut [u8; NONCE_LENGTH]) -> Result<(), ContactCryptoError> {
        use worker::wasm_bindgen::JsCast;

        let scope = worker::js_sys::global()
            .dyn_into::<web_sys::WorkerGlobalScope>()
            .map_err(|_| ContactCryptoError::RandomnessUnavailable)?;
        let crypto = scope
            .crypto()
            .map_err(|_| ContactCryptoError::RandomnessUnavailable)?;
        crypto
            .get_random_values_with_u8_array(nonce)
            .map_err(|_| ContactCryptoError::RandomnessUnavailable)?;
        Ok(())
    }
}

pub struct RustCryptoContactProtection<N> {
    keyring: ContactProtectionKeyring,
    nonce_source: N,
}

impl<N> RustCryptoContactProtection<N> {
    #[must_use]
    pub const fn new(keyring: ContactProtectionKeyring, nonce_source: N) -> Self {
        Self {
            keyring,
            nonce_source,
        }
    }

    #[must_use]
    pub const fn keyring(&self) -> &ContactProtectionKeyring {
        &self.keyring
    }

    pub fn derive_lookup_candidates(
        &self,
        tenant_id: &TenantId,
        hmac_input: &ExactLookupHmacInput,
    ) -> Result<Vec<ExactLookupToken>, ContactCryptoError> {
        self.keyring
            .lookup_keys
            .iter()
            .map(|root| derive_lookup_token(root, tenant_id, hmac_input))
            .collect()
    }

    pub fn decrypt_contact_display(
        &self,
        tenant_id: &TenantId,
        contact_point_id: &ContactPointId,
        protection_version: ContactProtectionVersion,
        encrypted: &EncryptedContactValue,
    ) -> Result<Zeroizing<String>, ContactCryptoError> {
        let root = self
            .keyring
            .encryption_by_version(encrypted.key_version())?;
        let tenant_key = derive_tenant_key(
            &root.bytes,
            ENCRYPTION_KDF_DOMAIN,
            tenant_id,
            root.version().value(),
        )?;
        let cipher = XChaCha20Poly1305::new_from_slice(tenant_key.as_slice())
            .map_err(|_| ContactCryptoError::InvalidKeyring)?;
        let nonce_bytes: [u8; NONCE_LENGTH] = encrypted
            .nonce()
            .try_into()
            .map_err(|_| ContactCryptoError::InvalidProtectedValue)?;
        let nonce = XNonce::try_from(nonce_bytes.as_slice())
            .map_err(|_| ContactCryptoError::InvalidProtectedValue)?;
        let aad = encryption_aad(
            tenant_id,
            contact_point_id,
            protection_version,
            root.version(),
        );
        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: encrypted.ciphertext(),
                    aad: &aad,
                },
            )
            .map_err(|_| ContactCryptoError::AuthenticationFailed)?;
        let decoded = String::from_utf8(plaintext).map_err(|error| {
            let mut bytes = error.into_bytes();
            bytes.zeroize();
            ContactCryptoError::InvalidUtf8
        })?;
        Ok(Zeroizing::new(decoded))
    }
}

impl<N: ContactNonceSource> ContactProtectionPort for RustCryptoContactProtection<N> {
    async fn encrypt_contact_display(
        &self,
        request: ContactEncryptionRequest<'_>,
    ) -> Result<EncryptedContactValue, ContactProtectionPortError> {
        self.encrypt(request).map_err(map_crypto_error)
    }

    async fn derive_exact_lookup_token(
        &self,
        request: ContactExactLookupRequest<'_>,
    ) -> Result<ExactLookupToken, ContactProtectionPortError> {
        let root = self.keyring.current_lookup().map_err(map_crypto_error)?;
        derive_lookup_token(root, request.tenant_id(), request.hmac_input())
            .map_err(map_crypto_error)
    }
}

impl<N: ContactNonceSource> RustCryptoContactProtection<N> {
    fn encrypt(
        &self,
        request: ContactEncryptionRequest<'_>,
    ) -> Result<EncryptedContactValue, ContactCryptoError> {
        let root = self.keyring.current_encryption()?;
        let tenant_key = derive_tenant_key(
            &root.bytes,
            ENCRYPTION_KDF_DOMAIN,
            request.tenant_id(),
            root.version().value(),
        )?;
        let cipher = XChaCha20Poly1305::new_from_slice(tenant_key.as_slice())
            .map_err(|_| ContactCryptoError::InvalidKeyring)?;
        let mut nonce = [0_u8; NONCE_LENGTH];
        self.nonce_source.fill_nonce(&mut nonce)?;
        let aead_nonce = XNonce::try_from(nonce.as_slice())
            .map_err(|_| ContactCryptoError::EncryptionFailed)?;
        let aad = encryption_aad(
            request.tenant_id(),
            request.contact_point_id(),
            request.protection_version(),
            root.version(),
        );
        let ciphertext = cipher
            .encrypt(
                &aead_nonce,
                Payload {
                    msg: request.normalized_value().expose().as_bytes(),
                    aad: &aad,
                },
            )
            .map_err(|_| ContactCryptoError::EncryptionFailed)?;
        EncryptedContactValue::new(ciphertext, nonce.to_vec(), root.version())
            .map_err(|_| ContactCryptoError::InvalidProtectedValue)
    }
}

fn derive_lookup_token(
    root: &ContactLookupRootKey,
    tenant_id: &TenantId,
    hmac_input: &ExactLookupHmacInput,
) -> Result<ExactLookupToken, ContactCryptoError> {
    let tenant_key = derive_tenant_key(
        &root.bytes,
        LOOKUP_KDF_DOMAIN,
        tenant_id,
        root.version().value(),
    )?;
    let bytes = hmac_sha256(tenant_key.as_slice(), &[hmac_input.expose_bytes()])?;
    Ok(ExactLookupToken::new(bytes, root.version()))
}

fn derive_tenant_key(
    root: &[u8; 32],
    domain: &[u8],
    tenant_id: &TenantId,
    key_version: u32,
) -> Result<Zeroizing<[u8; 32]>, ContactCryptoError> {
    let version = key_version.to_be_bytes();
    hmac_sha256(
        root,
        &[domain, &version, b"\0", tenant_id.as_str().as_bytes()],
    )
    .map(Zeroizing::new)
}

fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> Result<[u8; 32], ContactCryptoError> {
    let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(key)
        .map_err(|_| ContactCryptoError::InvalidKeyring)?;
    for part in parts {
        mac.update(part);
    }
    let output = mac.finalize().into_bytes();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(output.as_slice());
    Ok(bytes)
}

fn encryption_aad(
    tenant_id: &TenantId,
    contact_point_id: &ContactPointId,
    protection_version: ContactProtectionVersion,
    key_version: EncryptionKeyVersion,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(
        AEAD_AAD_DOMAIN.len() + tenant_id.as_str().len() + contact_point_id.as_str().len() + 16,
    );
    aad.extend_from_slice(AEAD_AAD_DOMAIN);
    aad.extend_from_slice(&protection_version.value().to_be_bytes());
    aad.extend_from_slice(&key_version.value().to_be_bytes());
    aad.push(0);
    aad.extend_from_slice(tenant_id.as_str().as_bytes());
    aad.push(0);
    aad.extend_from_slice(contact_point_id.as_str().as_bytes());
    aad
}

fn map_crypto_error(error: ContactCryptoError) -> ContactProtectionPortError {
    let class = match error {
        ContactCryptoError::KeyUnavailable | ContactCryptoError::InvalidKeyring => {
            ContactProtectionPortErrorClass::KeyUnavailable
        }
        ContactCryptoError::InvalidProtectedValue
        | ContactCryptoError::AuthenticationFailed
        | ContactCryptoError::InvalidUtf8 => ContactProtectionPortErrorClass::InvalidProtectedValue,
        ContactCryptoError::RandomnessUnavailable | ContactCryptoError::EncryptionFailed => {
            ContactProtectionPortErrorClass::InternalFailure
        }
    };
    ContactProtectionPortError::new(class)
}

#[cfg(test)]
mod tests {
    use super::{
        ContactCryptoError, ContactEncryptionRootKey, ContactLookupRootKey, ContactNonceSource,
        ContactProtectionKeyring, RustCryptoContactProtection,
    };
    use application_ports::clients::{
        ContactEncryptionRequest, ContactExactLookupRequest, ContactProtectionPort,
    };
    use client_domain::{
        ContactKind, ContactNormalizationVersion, ContactProtectionVersion, EncryptionKeyVersion,
        LookupKeyVersion, exact_lookup_hmac_input, normalize_contact_value,
    };
    use profile_platform_primitives::{ContactPointId, TenantId};
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    struct FixedNonce([u8; 24]);

    impl ContactNonceSource for FixedNonce {
        fn fill_nonce(&self, nonce: &mut [u8; 24]) -> Result<(), ContactCryptoError> {
            nonce.copy_from_slice(&self.0);
            Ok(())
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    fn protection() -> Result<RustCryptoContactProtection<FixedNonce>, Box<dyn std::error::Error>> {
        let encryption_current =
            ContactEncryptionRootKey::new(EncryptionKeyVersion::new(2)?, [0x11; 32]);
        let encryption_legacy =
            ContactEncryptionRootKey::new(EncryptionKeyVersion::new(1)?, [0x22; 32]);
        let lookup_current = ContactLookupRootKey::new(LookupKeyVersion::new(3)?, [0x33; 32]);
        let lookup_legacy = ContactLookupRootKey::new(LookupKeyVersion::new(2)?, [0x44; 32]);
        let keyring = ContactProtectionKeyring::new(
            vec![encryption_current, encryption_legacy],
            vec![lookup_current, lookup_legacy],
        )?;
        Ok(RustCryptoContactProtection::new(
            keyring,
            FixedNonce([0x55; 24]),
        ))
    }

    #[test]
    fn encryption_uses_current_version_and_round_trips_with_bound_aad()
    -> Result<(), Box<dyn std::error::Error>> {
        let protection = protection()?;
        let tenant = TenantId::parse("tenant_01JCRYPTOA")?;
        let contact_id = ContactPointId::parse("contact_01JCRYPTOA")?;
        let normalized = normalize_contact_value(
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            "Person@Example.COM",
        )?;
        let encrypted = block_on(protection.encrypt_contact_display(
            ContactEncryptionRequest::new(
                &tenant,
                &contact_id,
                ContactProtectionVersion::V1,
                &normalized,
            ),
        ))?;
        assert_eq!(encrypted.key_version().value(), 2);
        assert_eq!(encrypted.nonce(), &[0x55; 24]);
        assert!(
            !encrypted
                .ciphertext()
                .windows(normalized.expose().len())
                .any(|window| window == normalized.expose().as_bytes())
        );
        let opened = protection.decrypt_contact_display(
            &tenant,
            &contact_id,
            ContactProtectionVersion::V1,
            &encrypted,
        )?;
        assert_eq!(opened.as_str(), "person@example.com");
        Ok(())
    }

    #[test]
    fn ciphertext_authentication_binds_tenant_and_contact_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let protection = protection()?;
        let tenant = TenantId::parse("tenant_01JCRYPTOA")?;
        let other_tenant = TenantId::parse("tenant_01JCRYPTOB")?;
        let contact_id = ContactPointId::parse("contact_01JCRYPTOA")?;
        let normalized = normalize_contact_value(
            ContactKind::Phone,
            ContactNormalizationVersion::V1,
            "+48 123 456 789",
        )?;
        let encrypted = block_on(protection.encrypt_contact_display(
            ContactEncryptionRequest::new(
                &tenant,
                &contact_id,
                ContactProtectionVersion::V1,
                &normalized,
            ),
        ))?;
        assert!(matches!(
            protection.decrypt_contact_display(
                &other_tenant,
                &contact_id,
                ContactProtectionVersion::V1,
                &encrypted,
            ),
            Err(ContactCryptoError::AuthenticationFailed)
        ));
        Ok(())
    }

    #[test]
    fn lookup_tokens_are_tenant_separated_and_rotation_candidates_are_versioned()
    -> Result<(), Box<dyn std::error::Error>> {
        let protection = protection()?;
        let tenant_a = TenantId::parse("tenant_01JCRYPTOA")?;
        let tenant_b = TenantId::parse("tenant_01JCRYPTOB")?;
        let normalized = normalize_contact_value(
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            "person@example.com",
        )?;
        let input_a = exact_lookup_hmac_input(
            &tenant_a,
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            &normalized,
        );
        let input_b = exact_lookup_hmac_input(
            &tenant_b,
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            &normalized,
        );
        let token_a = block_on(protection.derive_exact_lookup_token(
            ContactExactLookupRequest::new(
                &tenant_a,
                ContactKind::Email,
                ContactNormalizationVersion::V1,
                &input_a,
            ),
        ))?;
        let token_b = block_on(protection.derive_exact_lookup_token(
            ContactExactLookupRequest::new(
                &tenant_b,
                ContactKind::Email,
                ContactNormalizationVersion::V1,
                &input_b,
            ),
        ))?;
        assert_eq!(token_a.key_version().value(), 3);
        assert_ne!(token_a.bytes(), token_b.bytes());

        let candidates = protection.derive_lookup_candidates(&tenant_a, &input_a)?;
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].key_version().value(), 3);
        assert_eq!(candidates[1].key_version().value(), 2);
        assert_ne!(candidates[0].bytes(), candidates[1].bytes());
        Ok(())
    }

    #[test]
    fn keyring_rejects_duplicate_versions() -> Result<(), Box<dyn std::error::Error>> {
        let encryption_version = EncryptionKeyVersion::new(1)?;
        let lookup_version = LookupKeyVersion::new(1)?;
        let result = ContactProtectionKeyring::new(
            vec![
                ContactEncryptionRootKey::new(encryption_version, [1; 32]),
                ContactEncryptionRootKey::new(encryption_version, [2; 32]),
            ],
            vec![ContactLookupRootKey::new(lookup_version, [3; 32])],
        );
        assert!(matches!(result, Err(ContactCryptoError::InvalidKeyring)));
        Ok(())
    }
}
