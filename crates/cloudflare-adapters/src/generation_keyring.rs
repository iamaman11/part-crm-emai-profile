use encrypted_generation_domain::{
    DerivedGenerationMaterial, GenerationKeyDerivationContext, GenerationRootKey,
    GenerationRootKeyVersion, KeyId, derive_generation_material,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use serde::Deserialize;
use std::fmt;
use worker::Env;
use zeroize::{Zeroize, Zeroizing};

const MAX_SERIALIZED_KEYRING_BYTES: usize = 8_192;
const MAX_ROOT_KEYS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationRootKeyringError {
    SecretUnavailable,
    InvalidConfiguration,
    KeyUnavailable,
    DerivationFailed,
}

impl fmt::Display for GenerationRootKeyringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SecretUnavailable => "generation root-key secret is unavailable",
            Self::InvalidConfiguration => "generation root-key keyring is invalid",
            Self::KeyUnavailable => "generation root-key version is unavailable",
            Self::DerivationFailed => "generation key material derivation failed",
        })
    }
}

impl std::error::Error for GenerationRootKeyringError {}

pub struct CloudflareGenerationRootKeyring {
    active_version: GenerationRootKeyVersion,
    keys: Vec<GenerationRootKey>,
}

impl CloudflareGenerationRootKeyring {
    pub fn from_env(env: &Env, secret_binding: &str) -> Result<Self, GenerationRootKeyringError> {
        let serialized = env
            .secret(secret_binding)
            .map_err(|_| GenerationRootKeyringError::SecretUnavailable)?
            .to_string();
        parse_serialized_keyring(serialized)
    }

    pub fn derive_active(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        plaintext_digest: [u8; 32],
    ) -> Result<DerivedGenerationMaterial, GenerationRootKeyringError> {
        self.derive_for_version(
            self.active_version,
            tenant_id,
            profile_id,
            generation_id,
            plaintext_digest,
        )
    }

    pub fn derive_for_key_id(
        &self,
        key_id: &KeyId,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        plaintext_digest: [u8; 32],
    ) -> Result<DerivedGenerationMaterial, GenerationRootKeyringError> {
        let version = GenerationRootKeyVersion::from_key_id(key_id)
            .map_err(|_| GenerationRootKeyringError::KeyUnavailable)?;
        self.derive_for_version(
            version,
            tenant_id,
            profile_id,
            generation_id,
            plaintext_digest,
        )
    }

    fn derive_for_version(
        &self,
        version: GenerationRootKeyVersion,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        plaintext_digest: [u8; 32],
    ) -> Result<DerivedGenerationMaterial, GenerationRootKeyringError> {
        let root = self
            .keys
            .iter()
            .find(|key| key.version() == version)
            .ok_or(GenerationRootKeyringError::KeyUnavailable)?;
        derive_generation_material(
            root,
            GenerationKeyDerivationContext::new(
                tenant_id,
                profile_id,
                generation_id,
                plaintext_digest,
            ),
        )
        .map_err(|_| GenerationRootKeyringError::DerivationFailed)
    }

    #[must_use]
    pub const fn active_version(&self) -> GenerationRootKeyVersion {
        self.active_version
    }

    #[must_use]
    pub fn retained_versions(&self) -> Vec<GenerationRootKeyVersion> {
        self.keys.iter().map(GenerationRootKey::version).collect()
    }
}

impl fmt::Debug for CloudflareGenerationRootKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CloudflareGenerationRootKeyring")
            .field("active_version", &self.active_version)
            .field("retained_versions", &self.retained_versions())
            .finish()
    }
}

fn parse_serialized_keyring(
    serialized: String,
) -> Result<CloudflareGenerationRootKeyring, GenerationRootKeyringError> {
    if serialized.is_empty() || serialized.len() > MAX_SERIALIZED_KEYRING_BYTES {
        return Err(GenerationRootKeyringError::InvalidConfiguration);
    }
    let serialized = Zeroizing::new(serialized);
    let secret: GenerationRootKeyringSecret = serde_json::from_str(serialized.as_str())
        .map_err(|_| GenerationRootKeyringError::InvalidConfiguration)?;
    if secret.keys.is_empty() || secret.keys.len() > MAX_ROOT_KEYS {
        return Err(GenerationRootKeyringError::InvalidConfiguration);
    }
    let active_version = GenerationRootKeyVersion::new(secret.active_version)
        .map_err(|_| GenerationRootKeyringError::InvalidConfiguration)?;
    let mut keys = Vec::with_capacity(secret.keys.len());
    for entry in &secret.keys {
        let version = GenerationRootKeyVersion::new(entry.version)
            .map_err(|_| GenerationRootKeyringError::InvalidConfiguration)?;
        if keys
            .iter()
            .any(|existing: &GenerationRootKey| existing.version() == version)
        {
            return Err(GenerationRootKeyringError::InvalidConfiguration);
        }
        keys.push(GenerationRootKey::new(
            version,
            decode_key_hex(&entry.key_hex)?,
        ));
    }
    if !keys.iter().any(|key| key.version() == active_version) {
        return Err(GenerationRootKeyringError::InvalidConfiguration);
    }
    Ok(CloudflareGenerationRootKeyring {
        active_version,
        keys,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct GenerationRootKeyringSecret {
    active_version: u32,
    keys: Vec<GenerationRootKeyEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct GenerationRootKeyEntry {
    version: u32,
    key_hex: String,
}

impl Drop for GenerationRootKeyEntry {
    fn drop(&mut self) {
        self.key_hex.zeroize();
    }
}

fn decode_key_hex(value: &str) -> Result<[u8; 32], GenerationRootKeyringError> {
    if value.len() != 64 {
        return Err(GenerationRootKeyringError::InvalidConfiguration);
    }
    let bytes = value.as_bytes();
    let mut decoded = Zeroizing::new([0_u8; 32]);
    for (index, byte) in decoded.iter_mut().enumerate() {
        let high = hex_nibble(bytes[index * 2])
            .ok_or(GenerationRootKeyringError::InvalidConfiguration)?;
        let low = hex_nibble(bytes[index * 2 + 1])
            .ok_or(GenerationRootKeyringError::InvalidConfiguration)?;
        *byte = (high << 4) | low;
    }
    Ok(*decoded)
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
    use super::{GenerationRootKeyringError, parse_serialized_keyring};
    use encrypted_generation_domain::{GenerationRootKeyVersion, KeyId};
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

    fn valid_keyring() -> String {
        format!(
            "{{\"activeVersion\":2,\"keys\":[{{\"version\":1,\"keyHex\":\"{}\"}},{{\"version\":2,\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        )
    }

    #[test]
    fn explicit_active_and_retained_versions_drive_exact_derivation()
    -> Result<(), Box<dyn std::error::Error>> {
        let keyring = parse_serialized_keyring(valid_keyring())?;
        assert_eq!(keyring.active_version(), GenerationRootKeyVersion::new(2)?);
        assert_eq!(
            keyring.retained_versions(),
            vec![
                GenerationRootKeyVersion::new(1)?,
                GenerationRootKeyVersion::new(2)?,
            ]
        );
        let tenant = TenantId::parse("tenant_generation_keyring_01")?;
        let profile = ProfileId::parse("profile_generation_keyring_01")?;
        let generation = GenerationId::parse("generation_generation_keyring_01")?;
        let digest = [0x33; 32];
        let active = keyring.derive_active(&tenant, &profile, &generation, digest)?;
        assert_eq!(active.key_id().as_str(), "profile-generation-root-v1-2");
        let historical = keyring.derive_for_key_id(
            &KeyId::parse("profile-generation-root-v1-1")?,
            &tenant,
            &profile,
            &generation,
            digest,
        )?;
        assert_eq!(historical.key_id().as_str(), "profile-generation-root-v1-1");
        assert_ne!(active.nonce_prefix(), historical.nonce_prefix());
        let debug = format!("{keyring:?}");
        assert!(debug.contains("retained_versions"));
        assert!(!debug.contains(&"22".repeat(32)));
        Ok(())
    }

    #[test]
    fn malformed_duplicate_missing_active_and_unknown_versions_fail_closed() {
        let duplicate = format!(
            "{{\"activeVersion\":1,\"keys\":[{{\"version\":1,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        );
        let missing_active = format!(
            "{{\"activeVersion\":2,\"keys\":[{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
        );
        for invalid in [
            "{}".to_owned(),
            "{\"activeVersion\":1,\"keys\":[]}".to_owned(),
            duplicate,
            missing_active,
            "{\"activeVersion\":1,\"keys\":[{\"version\":1,\"keyHex\":\"00\"}]}"
                .to_owned(),
        ] {
            assert_eq!(
                parse_serialized_keyring(invalid).err(),
                Some(GenerationRootKeyringError::InvalidConfiguration)
            );
        }
        let keyring = parse_serialized_keyring(valid_keyring())
            .map_err(|error| format!("unexpected keyring error: {error}"))?;
        let tenant = TenantId::parse("tenant_generation_keyring_02")?;
        let profile = ProfileId::parse("profile_generation_keyring_02")?;
        let generation = GenerationId::parse("generation_generation_keyring_02")?;
        assert_eq!(
            keyring
                .derive_for_key_id(
                    &KeyId::parse("profile-generation-root-v1-3")?,
                    &tenant,
                    &profile,
                    &generation,
                    [0x44; 32],
                )
                .err(),
            Some(GenerationRootKeyringError::KeyUnavailable)
        );
        Ok(())
    }
}
