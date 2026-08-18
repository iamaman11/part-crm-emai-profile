use crate::contact_protection::{
    ContactEncryptionRootKey, ContactLookupRootKey, ContactProtectionKeyring,
};
#[cfg(target_arch = "wasm32")]
use crate::contact_protection::{RustCryptoContactProtection, WorkerCryptoNonceSource};
use client_domain::{EncryptionKeyVersion, LookupKeyVersion};
use serde::Deserialize;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContactKeyringConfigError;

#[cfg(target_arch = "wasm32")]
pub fn contact_protection_from_serialized_keyring(
    serialized: String,
) -> Result<RustCryptoContactProtection<WorkerCryptoNonceSource>, ContactKeyringConfigError> {
    Ok(RustCryptoContactProtection::new(
        parse_contact_protection_keyring(serialized)?,
        WorkerCryptoNonceSource,
    ))
}

fn parse_contact_protection_keyring(
    serialized: String,
) -> Result<ContactProtectionKeyring, ContactKeyringConfigError> {
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

    move_active_encryption_first(&mut encryption_keys, active_encryption_version)?;
    move_active_lookup_first(&mut lookup_keys, active_lookup_version)?;

    ContactProtectionKeyring::new(encryption_keys, lookup_keys)
        .map_err(|_| ContactKeyringConfigError)
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
    use super::{ContactKeyringConfigError, parse_contact_protection_keyring};

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
        let keyring = parse_contact_protection_keyring(valid)?;
        assert_eq!(keyring.lookup_versions()[0].value(), 1);
        assert!(format!("{keyring:?}").contains("encryption_versions: [1, 2]"));
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
