use crate::{GenerationDek, KeyId, NoncePrefix, PlaintextDigest};
use core::fmt;
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::Sha256;
use zeroize::Zeroize;

const KDF_CONTEXT_DOMAIN: &[u8] = b"profile-generation-kdf-context-v1";
const DEK_DOMAIN: &[u8] = b"profile-generation-dek-v1";
const NONCE_PREFIX_DOMAIN: &[u8] = b"profile-generation-nonce-prefix-v1";
const ROOT_KEY_ID_PREFIX: &str = "profile-generation-root-v1-";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenerationRootKeyVersion(u32);

impl GenerationRootKeyVersion {
    pub fn new(value: u32) -> Result<Self, GenerationKeyDerivationError> {
        if value == 0 {
            return Err(GenerationKeyDerivationError::InvalidRootKeyVersion);
        }
        Ok(Self(value))
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
    plaintext_digest: PlaintextDigest,
}

impl<'a> GenerationKeyDerivationContext<'a> {
    #[must_use]
    pub const fn new(
        tenant_id: &'a TenantId,
        profile_id: &'a ProfileId,
        generation_id: &'a GenerationId,
        plaintext_digest: PlaintextDigest,
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
    let key_id = KeyId::parse(format!(
        "{ROOT_KEY_ID_PREFIX}{}",
        root.version().value()
    ))
    .map_err(|_| GenerationKeyDerivationError::InvalidKeyIdentity)?;
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
    let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(&root.bytes)
        .map_err(|_| GenerationKeyDerivationError::InvalidContext)?;
    update_frame(&mut mac, KDF_CONTEXT_DOMAIN)?;
    update_frame(&mut mac, purpose)?;
    update_frame(&mut mac, &root.version().value().to_be_bytes())?;
    update_frame(&mut mac, context.tenant_id.as_str().as_bytes())?;
    update_frame(&mut mac, context.profile_id.as_str().as_bytes())?;
    update_frame(&mut mac, context.generation_id.as_str().as_bytes())?;
    update_frame(&mut mac, &context.plaintext_digest.bytes())?;
    let output = mac.finalize().into_bytes();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(output.as_slice());
    Ok(bytes)
}

fn update_frame(
    mac: &mut HmacSha256,
    value: &[u8],
) -> Result<(), GenerationKeyDerivationError> {
    let length = u64::try_from(value.len()).map_err(|_| GenerationKeyDerivationError::InvalidContext)?;
    mac.update(&length.to_be_bytes());
    mac.update(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DEK_DOMAIN, NONCE_PREFIX_DOMAIN, GenerationKeyDerivationContext, GenerationRootKey,
        GenerationRootKeyVersion, derive_generation_material, derive_prf,
    };
    use crate::PlaintextDigest;
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

    #[test]
    fn exact_context_is_deterministic_and_domains_are_separate()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_generation_kdf_01")?;
        let profile = ProfileId::parse("profile_generation_kdf_01")?;
        let generation = GenerationId::parse("generation_generation_kdf_01")?;
        let digest = PlaintextDigest::calculate(b"same snapshot");
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
            PlaintextDigest::calculate(b"snapshot-v1"),
        );
        let second = GenerationKeyDerivationContext::new(
            &tenant,
            &profile,
            &generation,
            PlaintextDigest::calculate(b"snapshot-v2"),
        );

        assert_ne!(derive_prf(&root, DEK_DOMAIN, first)?, derive_prf(&root, DEK_DOMAIN, second)?);
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
        let digest = PlaintextDigest::calculate(b"snapshot");
        let root_v1 = GenerationRootKey::new(GenerationRootKeyVersion::new(1)?, [0x55; 32]);
        let root_v2 = GenerationRootKey::new(GenerationRootKeyVersion::new(2)?, [0x55; 32]);
        let context_a = GenerationKeyDerivationContext::new(
            &tenant_a,
            &profile,
            &generation,
            digest,
        );
        let context_b = GenerationKeyDerivationContext::new(
            &tenant_b,
            &profile,
            &generation,
            digest,
        );

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
    fn zero_root_version_is_rejected_and_debug_redacts_secret() {
        assert!(GenerationRootKeyVersion::new(0).is_err());
        let root = GenerationRootKey::new(
            GenerationRootKeyVersion::new(1).expect("positive version"),
            [0xab; 32],
        );
        let debug = format!("{root:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("abababab"));
    }
}
