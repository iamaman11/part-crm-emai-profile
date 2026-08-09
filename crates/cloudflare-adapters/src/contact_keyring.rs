use crate::contact_protection::{
    ContactEncryptionRootKey, ContactLookupRootKey, ContactProtectionKeyring,
    RustCryptoContactProtection, WorkerCryptoNonceSource,
};
use client_domain::{EncryptionKeyVersion, LookupKeyVersion};
use serde::Deserialize;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContactKeyringConfigError;

pub fn contact_protection_from_serialized_keyring(
    serialized: String,
) -> Result<RustCryptoContactProtection<WorkerCryptoNonceSource>, ContactKeyringConfigError> {
    let serialized = Zeroizing::new(serialized);
    let keyring: ContactProtectionKeyringSecret =
        serde_json::from_str(serialized.as_str()).map_err(|_| ContactKeyringConfigError)?;
    let encryption_keys = keyring
        .encryption
        .iter()
        .map(|entry| {
            Ok(ContactEncryptionRootKey::new(
                EncryptionKeyVersion::new(entry.version).map_err(|_| ContactKeyringConfigError)?,
                decode_key_hex(&entry.key_hex)?,
            ))
        })
        .collect::<Result<Vec<_>, ContactKeyringConfigError>>()?;
    let lookup_keys = keyring
        .lookup
        .iter()
        .map(|entry| {
            Ok(ContactLookupRootKey::new(
                LookupKeyVersion::new(entry.version).map_err(|_| ContactKeyringConfigError)?,
                decode_key_hex(&entry.key_hex)?,
            ))
        })
        .collect::<Result<Vec<_>, ContactKeyringConfigError>>()?;
    let keyring = ContactProtectionKeyring::new(encryption_keys, lookup_keys)
        .map_err(|_| ContactKeyringConfigError)?;
    Ok(RustCryptoContactProtection::new(
        keyring,
        WorkerCryptoNonceSource,
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
    use super::contact_protection_from_serialized_keyring;

    #[test]
    fn versioned_keyring_requires_valid_nonempty_32_byte_entries() {
        let valid = format!(
            "{{\"encryption\":[{{\"version\":2,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}],\"lookup\":[{{\"version\":3,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "ab".repeat(32),
            "cd".repeat(32),
            "ef".repeat(32),
            "12".repeat(32),
        );
        assert!(contact_protection_from_serialized_keyring(valid).is_ok());
        assert!(
            contact_protection_from_serialized_keyring(
                "{\"encryption\":[],\"lookup\":[]}".to_owned()
            )
            .is_err()
        );
        assert!(
            contact_protection_from_serialized_keyring(
                "{\"encryption\":[{\"version\":0,\"keyHex\":\"00\"}],\"lookup\":[]}".to_owned()
            )
            .is_err()
        );
    }
}
