use cloudflare_adapters::access_identity::VerifiedExternalIdentity;
use cloudflare_adapters::contact_protection::{
    ContactEncryptionRootKey, ContactLookupRootKey, ContactProtectionKeyring,
    RustCryptoContactProtection, WorkerCryptoNonceSource,
};
use cloudflare_adapters::d1_client_merge::D1ClientMergeRepository;
use cloudflare_adapters::d1_client_persistence::D1ClientPersistenceRepository;
use cloudflare_adapters::d1_client_registry::D1ClientRegistryProjectionRepository;
use cloudflare_adapters::d1_clients::D1ClientApplicationRepository;
use cloudflare_adapters::d1_identity_ceremonies::D1IdentityCeremonyApplicationRepository;
use cloudflare_adapters::d1_identity_governance::D1IdentityGovernanceApplicationRepository;
use cloudflare_adapters::d1_mailbox_bindings::D1MailboxBindingApplicationRepository;
use cloudflare_adapters::d1_mailbox_jobs::D1MailboxJobApplicationRepository;
use cloudflare_adapters::d1_profile_application::D1ProfileApplicationBundle;
use cloudflare_adapters::d1_profile_generation_application::D1ProfileGenerationApplicationRepository;
use client_domain::{EncryptionKeyVersion, LookupKeyVersion};
use control_plane_contract::D1_CATALOG_BINDING;
use serde::Deserialize;
use worker::{Env, Error, Result};
use zeroize::{Zeroize, Zeroizing};

const CLIENT_CONTACT_PROTECTION_KEYRING_BINDING: &str = "CLIENT_CONTACT_PROTECTION_KEYRING";

pub fn client_application(env: &Env) -> Result<D1ClientApplicationRepository> {
    Ok(D1ClientApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn client_persistence_application(env: &Env) -> Result<D1ClientPersistenceRepository> {
    Ok(D1ClientPersistenceRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn client_merge_application(env: &Env) -> Result<D1ClientMergeRepository> {
    Ok(D1ClientMergeRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn client_registry_projection(env: &Env) -> Result<D1ClientRegistryProjectionRepository> {
    Ok(D1ClientRegistryProjectionRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn client_contact_protection(
    env: &Env,
) -> Result<RustCryptoContactProtection<WorkerCryptoNonceSource>> {
    let serialized = Zeroizing::new(
        env.secret(CLIENT_CONTACT_PROTECTION_KEYRING_BINDING)?
            .to_string(),
    );
    let keyring: ContactProtectionKeyringSecret = serde_json::from_str(serialized.as_str())
        .map_err(|_| Error::RustError("invalid client contact protection keyring".to_owned()))?;
    let encryption_keys = keyring
        .encryption
        .iter()
        .map(|entry| {
            Ok(ContactEncryptionRootKey::new(
                EncryptionKeyVersion::new(entry.version)
                    .map_err(|_| invalid_contact_keyring())?,
                decode_key_hex(&entry.key_hex)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let lookup_keys = keyring
        .lookup
        .iter()
        .map(|entry| {
            Ok(ContactLookupRootKey::new(
                LookupKeyVersion::new(entry.version).map_err(|_| invalid_contact_keyring())?,
                decode_key_hex(&entry.key_hex)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let keyring = ContactProtectionKeyring::new(encryption_keys, lookup_keys)
        .map_err(|_| invalid_contact_keyring())?;
    Ok(RustCryptoContactProtection::new(
        keyring,
        WorkerCryptoNonceSource,
    ))
}

pub fn identity_governance_application(
    env: &Env,
) -> Result<D1IdentityGovernanceApplicationRepository> {
    Ok(D1IdentityGovernanceApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn identity_ceremony_application(
    env: &Env,
    verified_identity: VerifiedExternalIdentity,
) -> Result<D1IdentityCeremonyApplicationRepository> {
    Ok(D1IdentityCeremonyApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        verified_identity,
    ))
}

pub fn profile_application(env: &Env) -> Result<D1ProfileApplicationBundle> {
    Ok(D1ProfileApplicationBundle::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn profile_generation_application(
    env: &Env,
) -> Result<D1ProfileGenerationApplicationRepository> {
    Ok(D1ProfileGenerationApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn mailbox_binding_application(env: &Env) -> Result<D1MailboxBindingApplicationRepository> {
    Ok(D1MailboxBindingApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn mailbox_job_application(env: &Env) -> Result<D1MailboxJobApplicationRepository> {
    Ok(D1MailboxJobApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContactProtectionKeyringSecret {
    encryption: Vec<ContactProtectionKeyEntry>,
    lookup: Vec<ContactProtectionKeyEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ContactProtectionKeyEntry {
    version: u32,
    key_hex: String,
}

impl Drop for ContactProtectionKeyEntry {
    fn drop(&mut self) {
        self.key_hex.zeroize();
    }
}

fn decode_key_hex(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        return Err(invalid_contact_keyring());
    }
    let bytes = value.as_bytes();
    let mut decoded = Zeroizing::new([0_u8; 32]);
    for (index, byte) in decoded.iter_mut().enumerate() {
        let high = hex_nibble(bytes[index * 2]).ok_or_else(invalid_contact_keyring)?;
        let low = hex_nibble(bytes[index * 2 + 1]).ok_or_else(invalid_contact_keyring)?;
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

fn invalid_contact_keyring() -> Error {
    Error::RustError("invalid client contact protection keyring".to_owned())
}

#[cfg(test)]
mod tests {
    use super::decode_key_hex;

    #[test]
    fn contact_keyring_hex_requires_exact_32_byte_key() {
        let key = decode_key_hex(&"ab".repeat(32)).expect("valid 32-byte key");
        assert_eq!(key, [0xab; 32]);
        assert!(decode_key_hex("00").is_err());
        assert!(decode_key_hex(&"gg".repeat(32)).is_err());
    }
}
