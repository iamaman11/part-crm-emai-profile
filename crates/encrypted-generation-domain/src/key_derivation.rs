use crate::{GenerationDek, KeyId, NoncePrefix};
use core::fmt;
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const KDF_CONTEXT_DOMAIN: &[u8] = b"profile-generation-kdf-context-v1";
const DEK_DOMAIN: &[u8] = b"profile-generation-dek-v1";
const NONCE_PREFIX_DOMAIN: &[u8] = b"profile-generation-nonce-prefix-v1";
const ROOT_KEY_ID_PREFIX: &str = "profile-generation-root-v1-";
const HMAC_SHA256_BLOCK_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenerationRootKeyVersion(u32);

impl GenerationRootKeyVersion {
    pub fn new(value: u32) -> Result<Self, GenerationKeyDerivationError> {
        if value == 0 {
            return Err(GenerationKeyDerivationError::InvalidRootKeyVersion);
        }
        Ok(Self(value))
    }

    pub fn from_key_id(key_id: &KeyId) -> Result<Self, GenerationKeyDerivationError> {
        let suffix = key_id
            .as_str()
            .strip_prefix(ROOT_KEY_ID_PREFIX)
            .ok_or(GenerationKeyDerivationError::InvalidKeyIdentity)?;
        if suffix.is_empty()
            || (suffix.len() > 1 && suffix.starts_with('0'))
            || !suffix.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(GenerationKeyDerivationError::InvalidKeyIdentity);
        }
        let value = suffix
            .parse::<u32>()
            .map_err(|_| GenerationKeyDerivationError::InvalidKeyIdentity)?;
        Self::new(value).map_err(|_| GenerationKeyDerivationError::InvalidKeyIdentity)
    }

    pub fn key_id(self) -> Result<KeyId, GenerationKeyDerivationError> {
        KeyId::parse(format!("{ROOT_KEY_ID_PREFIX}{}", self.value()))
            .map_err(|_| GenerationKeyDerivationError::InvalidKeyIdentity)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

pub struct GenerationRootKey {
    version: GenerationRootKeyVersion,
    bytes: [u8; 32],
}

impl GenerationRootKey {
    #[must_use]
    pub const fn new(version: GenerationRootKeyVersion, bytes: [u8; 32]) -> Self {
        Self { version, bytes }
    }

    #[must_use]
    pub const fn version(&self) -> GenerationRootKeyVersion {
        self.version
    }
}

impl fmt::Debug for GenerationRootKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GenerationRootKey")
            .field("version", &self.version)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl Drop for GenerationRootKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GenerationKeyDerivationContext<'a> {
    tenant_id: &'a TenantId,
    profile_id: &'a ProfileId,
    generation_id: &'a GenerationId,
    plaintext_digest: [u8; 32],
}

impl<'a> GenerationKeyDerivationContext<'a> {
    #[must_use]
    pub const fn new(
        tenant_id: &'a TenantId,
        profile_id: &'a ProfileId,
        generation_id: &'a GenerationId,
        plaintext_digest: [u8; 32],
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            generation_id,
            plaintext_digest,
        }
    }
}

pub struct DerivedGenerationMaterial {
    dek: GenerationDek,
    nonce_prefix: NoncePrefix,
}

impl DerivedGenerationMaterial {
    #[must_use]
    pub const fn key_id(&self) -> &KeyId {
        self.dek.key_id()
    }

    #[must_use]
    pub const fn nonce_prefix(&self) -> NoncePrefix {
        self.nonce_prefix
    }

    #[must_use]
    pub fn into_parts(self) -> (GenerationDek, NoncePrefix) {
        (self.dek, self.nonce_prefix)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationKeyDerivationError {
    InvalidRootKeyVersion,
    InvalidKeyIdentity,
    InvalidContext,
}

impl fmt::Display for GenerationKeyDerivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRootKeyVersion => "generation root key version must be positive",
            Self::InvalidKeyIdentity => "generation root key identity is invalid",
            Self::InvalidContext => "generation key derivation context is invalid",
        })
    }
}

impl std::error::Error for GenerationKeyDerivationError {}

pub fn derive_generation_material(
    root: &GenerationRootKey,
    context: GenerationKeyDerivationContext<'_>,
) -> Result<DerivedGenerationMaterial, GenerationKeyDerivationError> {
    let key_id = root.version().key_id()?;
    let dek_bytes = derive_prf(root, DEK_DOMAIN, context)?;
    let nonce_bytes = derive_prf(root, NONCE_PREFIX_DOMAIN, context)?;
    let mut nonce_prefix = [0_u8; 16];
    nonce_prefix.copy_from_slice(&nonce_bytes[..16]);
    Ok(DerivedGenerationMaterial {
        dek: GenerationDek::new(key_id, dek_bytes),
        nonce_prefix: NoncePrefix::new(nonce_prefix),
    })
}

fn derive_prf(
    root: &GenerationRootKey,
    purpose: &[u8],
    context: GenerationKeyDerivationContext<'_>,
) -> Result<[u8; 32], GenerationKeyDerivationError> {
    let mut inner_pad = Zeroizing::new([0x36_u8; HMAC_SHA256_BLOCK_BYTES]);
    let mut outer_pad = Zeroizing::new([0x5c_u8; HMAC_SHA256_BLOCK_BYTES]);
    for (index, key_byte) in root.bytes.iter().copied().enumerate() {
        inner_pad[index] ^= key_byte;
        outer_pad[index] ^= key_byte;
    }

    let mut inner = Sha256::new();
    inner.update(&inner_pad[..]);
    update_frame(&mut inner, KDF_CONTEXT_DOMAIN)?;
    update_frame(&mut inner, purpose)?;
    update_frame(&mut inner, &root.version().value().to_be_bytes())?;
    update_frame(&mut inner, context.tenant_id.as_str().as_bytes())?;
    update_frame(&mut inner, context.profile_id.as_str().as_bytes())?;
    update_frame(&mut inner, context.generation_id.as_str().as_bytes())?;
    update_frame(&mut inner, &context.plaintext_digest)?;
    let inner_digest = Zeroizing::new(<[u8; 32]>::from(inner.finalize()));

    let mut outer = Sha256::new();
    outer.update(&outer_pad[..]);
    outer.update(&inner_digest[..]);
    Ok(outer.finalize().into())
}

fn update_frame(digest: &mut Sha256, value: &[u8]) -> Result<(), GenerationKeyDerivationError> {
    let length =
        u64::try_from(value.len()).map_err(|_| GenerationKeyDerivationError::InvalidContext)?;
    digest.update(length.to_be_bytes());
    digest.update(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DEK_DOMAIN, GenerationKeyDerivationContext, GenerationRootKey, GenerationRootKeyVersion,
        NONCE_PREFIX_DOMAIN, derive_generation_material, derive_prf,
    };
    use crate::{KeyId, PlaintextDigest};
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

    #[test]
    fn exact_context_is_deterministic_and_domains_are_separate()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_generation_kdf_01")?;
        let profile = ProfileId::parse("profile_generation_kdf_01")?;
        let generation = GenerationId::parse("generation_generation_kdf_01")?;
        let digest = PlaintextDigest::calculate(b"same snapshot").bytes();
        let version = GenerationRootKeyVersion::new(7)?;
        let root = GenerationRootKey::new(version, [0x42; 32]);
        let context = GenerationKeyDerivationContext::new(&tenant, &profile, &generation, digest);

        assert_eq!(
            derive_prf(&root, DEK_DOMAIN, context)?,
            derive_prf(&root, DEK_DOMAIN, context)?
        );
        assert_ne!(
            derive_prf(&root, DEK_DOMAIN, context)?,
            derive_prf(&root, NONCE_PREFIX_DOMAIN, context)?
        );
        let material = derive_generation_material(&root, context)?;
        assert_eq!(material.key_id().as_str(), "profile-generation-root-v1-7");
        assert_eq!(GenerationRootKeyVersion::from_key_id(material.key_id())?, version);
        Ok(())
    }

    #[test]
    fn changed_plaintext_changes_both_dek_and_nonce_material()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_generation_kdf_02")?;
        let profile = ProfileId::parse("profile_generation_kdf_02")?;
        let generation = GenerationId::parse("generation_generation_kdf_02")?;
        let root = GenerationRootKey::new(GenerationRootKeyVersion::new(1)?, [0x24; 32]);
        let first = GenerationKeyDerivationContext::new(
            &tenant,
            &profile,
            &generation,
            PlaintextDigest::calculate(b"snapshot-v1").bytes(),
        );
        let second = GenerationKeyDerivationContext::new(
            &tenant,
            &profile,
            &generation,
            PlaintextDigest::calculate(b"snapshot-v2").bytes(),
        );

        assert_ne!(
            derive_prf(&root, DEK_DOMAIN, first)?,
            derive_prf(&root, DEK_DOMAIN, second)?
        );
        assert_ne!(
            derive_prf(&root, NONCE_PREFIX_DOMAIN, first)?,
            derive_prf(&root, NONCE_PREFIX_DOMAIN, second)?
        );
        assert_ne!(
            derive_generation_material(&root, first)?.nonce_prefix(),
            derive_generation_material(&root, second)?.nonce_prefix()
        );
        Ok(())
    }

    #[test]
    fn tenant_profile_generation_and_root_version_are_bound()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant_a = TenantId::parse("tenant_generation_kdf_03a")?;
        let tenant_b = TenantId::parse("tenant_generation_kdf_03b")?;
        let profile = ProfileId::parse("profile_generation_kdf_03")?;
        let generation = GenerationId::parse("generation_generation_kdf_03")?;
        let digest = PlaintextDigest::calculate(b"snapshot").bytes();
        let root_v1 = GenerationRootKey::new(GenerationRootKeyVersion::new(1)?, [0x55; 32]);
        let root_v2 = GenerationRootKey::new(GenerationRootKeyVersion::new(2)?, [0x55; 32]);
        let context_a =
            GenerationKeyDerivationContext::new(&tenant_a, &profile, &generation, digest);
        let context_b =
            GenerationKeyDerivationContext::new(&tenant_b, &profile, &generation, digest);

        assert_ne!(
            derive_prf(&root_v1, DEK_DOMAIN, context_a)?,
            derive_prf(&root_v1, DEK_DOMAIN, context_b)?
        );
        assert_ne!(
            derive_prf(&root_v1, DEK_DOMAIN, context_a)?,
            derive_prf(&root_v2, DEK_DOMAIN, context_a)?
        );
        assert_ne!(
            derive_generation_material(&root_v1, context_a)?.key_id(),
            derive_generation_material(&root_v2, context_a)?.key_id()
        );
        Ok(())
    }

    #[test]
    fn key_identity_is_canonical_and_strict() -> Result<(), Box<dyn std::error::Error>> {
        let version = GenerationRootKeyVersion::new(42)?;
        let key_id = version.key_id()?;
        assert_eq!(key_id.as_str(), "profile-generation-root-v1-42");
        assert_eq!(GenerationRootKeyVersion::from_key_id(&key_id)?, version);
        for invalid in [
            "profile-generation-root-v1-0",
            "profile-generation-root-v1-01",
            "profile-generation-root-v1-x",
            "profile-generation-root-v2-1",
        ] {
            let key_id = KeyId::parse(invalid)?;
            assert!(GenerationRootKeyVersion::from_key_id(&key_id).is_err());
        }
        Ok(())
    }

    #[test]
    fn zero_root_version_is_rejected_and_debug_redacts_secret()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(GenerationRootKeyVersion::new(0).is_err());
        let root = GenerationRootKey::new(GenerationRootKeyVersion::new(1)?, [0xab; 32]);
        let debug = format!("{root:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("abababab"));
        Ok(())
    }
}
