use crate::contact_key_lifecycle::{ContactKeyLifecycleMetadata, D1ContactKeyLifecycle};
use crate::contact_protection::{
    ContactEncryptionRootKey, ContactLookupRootKey, ContactProtectionKeyring,
};
#[cfg(target_arch = "wasm32")]
use crate::contact_protection::{RustCryptoContactProtection, WorkerCryptoNonceSource};
use client_domain::{EncryptionKeyVersion, LookupKeyVersion};
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use worker::d1::D1Database;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContactKeyringConfigError;

#[cfg(target_arch = "wasm32")]
pub fn contact_protection_from_serialized_keyring(
    serialized: String,
) -> Result<RustCryptoContactProtection<WorkerCryptoNonceSource>, ContactKeyringConfigError> {
    let (keyring, _) = parse_contact_protection_keyring_with_metadata(serialized)?;
    Ok(RustCryptoContactProtection::new(
        keyring,
        WorkerCryptoNonceSource,
    ))
}

#[cfg(target_arch = "wasm32")]
pub fn contact_key_lifecycle_from_serialized_keyring(
    serialized: String,
    database: D1Database,
) -> Result<D1ContactKeyLifecycle<WorkerCryptoNonceSource>, ContactKeyringConfigError> {
    let (keyring, metadata) = parse_contact_protection_keyring_with_metadata(serialized)?;
    Ok(D1ContactKeyLifecycle::new(
        database,
        RustCryptoContactProtection::new(keyring, WorkerCryptoNonceSource),
        metadata,
    ))
}

fn parse_contact_protection_keyring(
    serialized: String,
) -> Result<ContactProtectionKeyring, ContactKeyringConfigError> {
    parse_contact_protection_keyring_with_metadata(serialized).map(|(keyring, _)| keyring)
}

fn parse_contact_protection_keyring_with_metadata(
    serialized: String,
) -> Result<(ContactProtectionKeyring, ContactKeyLifecycleMetadata), ContactKeyringConfigError> {
    let serialized = Zeroizing::new(serialized);
    let keyring: ContactProtectionKeyringSecret =
        serde_json::from_str(serialized.as_str()).map_err(|_| ContactKeyringConfigError)?;
    let active_encryption_version = EncryptionKeyVersion::new(keyring.active_encryption_version)
        .map_err(|_| ContactKeyringConfigError)?;
    let active_lookup_version = LookupKeyVersion::new(keyring.active_lookup_version)
        .map_err(|_| ContactKeyringConfigError)?;
    let mut encryption_keys = keyring
        .encryption
        .iter()
        .map(|entry| {
            Ok(ContactEncryptionRootKey::new(
                EncryptionKeyVersion::new(entry.version).map_err(|_| ContactKeyringConfigError)?,
                decode_key_hex(&entry.key_hex)?,
            ))
        })
        .collect::<Result<Vec<_>, ContactKeyringConfigError>>()?;
    let mut lookup_keys = keyring
        .lookup
        .iter()
        .map(|entry| {
            Ok(ContactLookupRootKey::new(
                LookupKeyVersion::new(entry.version).map_err(|_| ContactKeyringConfigError)?,
                decode_key_hex(&entry.key_hex)?,
            ))
        })
        .collect::<Result<Vec<_>, ContactKeyringConfigError>>()?;
    let retained_encryption_versions = encryption_keys
        .iter()
        .map(|key| key.version().value())
        .collect::<Vec<_>>();
    let retained_lookup_versions = lookup_keys
        .iter()
        .map(|key| key.version().value())
        .collect::<Vec<_>>();
    let metadata = ContactKeyLifecycleMetadata::new(
        active_encryption_version.value(),
        active_lookup_version.value(),
        retained_encryption_versions,
        retained_lookup_versions,
    )
    .map_err(|_| ContactKeyringConfigError)?;

    move_active_encryption_first(&mut encryption_keys, active_encryption_version)?;
    move_active_lookup_first(&mut lookup_keys, active_lookup_version)?;

    let protection_keyring = ContactProtectionKeyring::new(encryption_keys, lookup_keys)
        .map_err(|_| ContactKeyringConfigError)?;
    Ok((protection_keyring, metadata))
}

fn move_active_encryption_first(
    keys: &mut [ContactEncryptionRootKey],
    active_version: EncryptionKeyVersion,
) -> Result<(), ContactKeyringConfigError> {
    let index = keys
        .iter()
        .position(|key| key.version() == active_version)
        .ok_or(ContactKeyringConfigError)?;
    keys.swap(0, index);
    Ok(())
}

fn move_active_lookup_first(
    keys: &mut [ContactLookupRootKey],
    active_version: LookupKeyVersion,
) -> Result<(), ContactKeyringConfigError> {
    let index = keys
        .iter()
        .position(|key| key.version() == active_version)
        .ok_or(ContactKeyringConfigError)?;
    keys.swap(0, index);
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ContactProtectionKeyringSecret {
    active_encryption_version: u32,
    active_lookup_version: u32,
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

fn decode_key_hex(value: &str) -> Result<[u8; 32], ContactKeyringConfigError> {
    if value.len() != 64 {
        return Err(ContactKeyringConfigError);
    }
    let bytes = value.as_bytes();
    let mut decoded = Zeroizing::new([0_u8; 32]);
    for (index, byte) in decoded.iter_mut().enumerate() {
        let high = hex_nibble(bytes[index * 2]).ok_or(ContactKeyringConfigError)?;
        let low = hex_nibble(bytes[index * 2 + 1]).ok_or(ContactKeyringConfigError)?;
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
    use super::{
        ContactKeyringConfigError, parse_contact_protection_keyring,
        parse_contact_protection_keyring_with_metadata,
    };

    #[test]
    fn explicit_active_versions_do_not_depend_on_json_array_order()
    -> Result<(), ContactKeyringConfigError> {
        let valid = format!(
            "{{\"activeEncryptionVersion\":1,\"activeLookupVersion\":1,\"encryption\":[{{\"version\":2,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}],\"lookup\":[{{\"version\":3,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "ab".repeat(32),
            "cd".repeat(32),
            "ef".repeat(32),
            "12".repeat(32),
        );
        let (keyring, metadata) = parse_contact_protection_keyring_with_metadata(valid)?;
        assert_eq!(keyring.lookup_versions()[0].value(), 1);
        assert!(format!("{keyring:?}").contains("encryption_versions: [1, 2]"));
        assert!(format!("{metadata:?}").contains("active_encryption_version: 1"));
        assert!(format!("{metadata:?}").contains("active_lookup_version: 1"));
        Ok(())
    }

    #[test]
    fn keyring_requires_explicit_present_active_versions_and_valid_entries() {
        assert!(
            parse_contact_protection_keyring(
                "{\"activeEncryptionVersion\":1,\"activeLookupVersion\":1,\"encryption\":[],\"lookup\":[]}".to_owned()
            )
            .is_err()
        );
        assert!(
            parse_contact_protection_keyring("{\"encryption\":[],\"lookup\":[]}".to_owned())
                .is_err()
        );
        let missing_active = format!(
            "{{\"activeEncryptionVersion\":3,\"activeLookupVersion\":1,\"encryption\":[{{\"version\":2,\"keyHex\":\"{}\"}}],\"lookup\":[{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "ab".repeat(32),
            "cd".repeat(32),
        );
        assert!(parse_contact_protection_keyring(missing_active).is_err());
        assert!(
            parse_contact_protection_keyring(
                "{\"activeEncryptionVersion\":1,\"activeLookupVersion\":1,\"encryption\":[{\"version\":0,\"keyHex\":\"00\"}],\"lookup\":[]}".to_owned()
            )
            .is_err()
        );
    }

    #[test]
    fn duplicate_versions_remain_rejected_after_active_selection() {
        let duplicate = format!(
            "{{\"activeEncryptionVersion\":1,\"activeLookupVersion\":1,\"encryption\":[{{\"version\":1,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}],\"lookup\":[{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "ab".repeat(32),
            "cd".repeat(32),
            "ef".repeat(32),
        );
        assert!(parse_contact_protection_keyring(duplicate).is_err());
    }
}