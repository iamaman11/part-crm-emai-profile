use crate::contact_key_lifecycle::ContactKeyLifecycleMetadata;
#[cfg(target_arch = "wasm32")]
use crate::contact_key_lifecycle::D1ContactKeyLifecycle;
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
pub enum ContactKeyringConfigError {
    InvalidShape,
    InvalidVersion,
    InvalidKeyHex,
    MissingActiveKey,
    InvalidLifecycle,
    InvalidKeySet,
}

impl ContactKeyringConfigError {
    /// Fixed diagnostic vocabulary. Never serialize keyring or secret-derived bytes.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidShape => "shape",
            Self::InvalidVersion => "version",
            Self::InvalidKeyHex => "key_hex",
            Self::MissingActiveKey => "active_key",
            Self::InvalidLifecycle => "lifecycle",
            Self::InvalidKeySet => "keyset",
        }
    }
}

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

#[cfg(test)]
fn parse_contact_protection_keyring(
    serialized: String,
) -> Result<ContactProtectionKeyring, ContactKeyringConfigError> {
    parse_contact_protection_keyring_with_metadata(serialized).map(|(keyring, _)| keyring)
}

fn parse_contact_protection_keyring_with_metadata(
    serialized: String,
) -> Result<(ContactProtectionKeyring, ContactKeyLifecycleMetadata), ContactKeyringConfigError> {
    let serialized = Zeroizing::new(serialized);
    let keyring: ContactProtectionKeyringSecret = serde_json::from_str(serialized.as_str())
        .map_err(|_| ContactKeyringConfigError::InvalidShape)?;
    let active_encryption_version = EncryptionKeyVersion::new(keyring.active_encryption_version)
        .map_err(|_| ContactKeyringConfigError::InvalidVersion)?;
    let active_lookup_version = LookupKeyVersion::new(keyring.active_lookup_version)
        .map_err(|_| ContactKeyringConfigError::InvalidVersion)?;
    let mut encryption_keys = keyring
        .encryption
        .iter()
        .map(|entry| {
            Ok(ContactEncryptionRootKey::new(
                EncryptionKeyVersion::new(entry.version)
                    .map_err(|_| ContactKeyringConfigError::InvalidVersion)?,
                decode_key_hex(&entry.key_hex)?,
            ))
        })
        .collect::<Result<Vec<_>, ContactKeyringConfigError>>()?;
    let mut lookup_keys = keyring
        .lookup
        .iter()
        .map(|entry| {
            Ok(ContactLookupRootKey::new(
                LookupKeyVersion::new(entry.version)
                    .map_err(|_| ContactKeyringConfigError::InvalidVersion)?,
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
    // Distinguish a missing active key from invalid retained-key metadata.
    // This reveals only version membership, never any key or secret bytes.
    if !retained_encryption_versions.contains(&active_encryption_version.value())
        || !retained_lookup_versions.contains(&active_lookup_version.value())
    {
        return Err(ContactKeyringConfigError::MissingActiveKey);
    }
    let metadata = ContactKeyLifecycleMetadata::new(
        active_encryption_version.value(),
        active_lookup_version.value(),
        retained_encryption_versions,
        retained_lookup_versions,
    )
    .map_err(|_| ContactKeyringConfigError::InvalidLifecycle)?;

    move_active_encryption_first(&mut encryption_keys, active_encryption_version)?;
    move_active_lookup_first(&mut lookup_keys, active_lookup_version)?;

    let protection_keyring = ContactProtectionKeyring::new(encryption_keys, lookup_keys)
        .map_err(|_| ContactKeyringConfigError::InvalidKeySet)?;
    Ok((protection_keyring, metadata))
}

fn move_active_encryption_first(
    keys: &mut [ContactEncryptionRootKey],
    active_version: EncryptionKeyVersion,
) -> Result<(), ContactKeyringConfigError> {
    let index = keys
        .iter()
        .position(|key| key.version() == active_version)
        .ok_or(ContactKeyringConfigError::MissingActiveKey)?;
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
        .ok_or(ContactKeyringConfigError::MissingActiveKey)?;
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
        return Err(ContactKeyringConfigError::InvalidKeyHex);
    }
    let bytes = value.as_bytes();
    let mut decoded = Zeroizing::new([0_u8; 32]);
    for (index, byte) in decoded.iter_mut().enumerate() {
        let high = hex_nibble(bytes[index * 2]).ok_or(ContactKeyringConfigError::InvalidKeyHex)?;
        let low =
            hex_nibble(bytes[index * 2 + 1]).ok_or(ContactKeyringConfigError::InvalidKeyHex)?;
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
    fn serialized_key_rotation_preserves_legacy_decrypt_and_lookup()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::contact_protection::{
            ContactCryptoError, ContactNonceSource, RustCryptoContactProtection,
        };
        use application_ports::clients::{ContactEncryptionRequest, ContactProtectionPort};
        use client_domain::{
            ContactKind, ContactNormalizationVersion, ContactProtectionVersion,
            exact_lookup_hmac_input, normalize_contact_value,
        };
        use profile_platform_primitives::{ContactPointId, TenantId};
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        struct FixedNonce;
        impl ContactNonceSource for FixedNonce {
            fn fill_nonce(&self, nonce: &mut [u8; 24]) -> Result<(), ContactCryptoError> {
                nonce.copy_from_slice(&[0x55; 24]);
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

        // Start with v1, then rotate to v2 while retaining both old key types.
        // A newly active version must never re-encrypt historic stored bytes.
        let original = format!(
            "{{\"activeEncryptionVersion\":1,\"activeLookupVersion\":1,\"encryption\":[{{\"version\":1,\"keyHex\":\"{}\"}}],\"lookup\":[{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "11".repeat(32),
            "22".repeat(32),
        );
        let rotated = format!(
            "{{\"activeEncryptionVersion\":2,\"activeLookupVersion\":2,\"encryption\":[{{\"version\":2,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}],\"lookup\":[{{\"version\":2,\"keyHex\":\"{}\"}},{{\"version\":1,\"keyHex\":\"{}\"}}]}}",
            "33".repeat(32),
            "11".repeat(32),
            "44".repeat(32),
            "22".repeat(32),
        );
        let (original_keys, _) = parse_contact_protection_keyring_with_metadata(original)?;
        let (rotated_keys, metadata) = parse_contact_protection_keyring_with_metadata(rotated)?;
        let original_protector = RustCryptoContactProtection::new(original_keys, FixedNonce);
        let rotated_protector = RustCryptoContactProtection::new(rotated_keys, FixedNonce);
        let tenant = TenantId::parse("tenant_01JKEYROTATE")?;
        let contact = ContactPointId::parse("contact_01JKEYROTATE")?;
        let normalized = normalize_contact_value(
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            "Legacy@Example.com",
        )?;
        let old_ciphertext = block_on(original_protector.encrypt_contact_display(
            ContactEncryptionRequest::new(
                &tenant,
                &contact,
                ContactProtectionVersion::V1,
                &normalized,
            ),
        ))?;
        assert_eq!(old_ciphertext.key_version().value(), 1);
        assert_eq!(
            rotated_protector.decrypt_contact_display(
                &tenant,
                &contact,
                ContactProtectionVersion::V1,
                &old_ciphertext,
            )?.as_str(),
            "legacy@example.com",
        );
        let new_ciphertext = block_on(rotated_protector.encrypt_contact_display(
            ContactEncryptionRequest::new(
                &tenant,
                &contact,
                ContactProtectionVersion::V1,
                &normalized,
            ),
        ))?;
        assert_eq!(new_ciphertext.key_version().value(), 2);

        let lookup_input = exact_lookup_hmac_input(
            &tenant,
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            &normalized,
        );
        let old_tokens = original_protector.derive_lookup_candidates(&tenant, &lookup_input)?;
        let rotated_tokens = rotated_protector.derive_lookup_candidates(&tenant, &lookup_input)?;
        assert_eq!(old_tokens.len(), 1);
        assert_eq!(rotated_tokens.len(), 2);
        assert_eq!(rotated_tokens[0].key_version().value(), 2);
        assert_eq!(rotated_tokens[1].key_version().value(), 1);
        assert_eq!(rotated_tokens[1].bytes(), old_tokens[0].bytes());
        assert_eq!(format!("{metadata:?}").contains("retained_encryption_versions"), true);
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
    fn invalid_keyring_reports_only_a_fixed_nonsecret_failure_category() {
        assert_eq!(
            parse_contact_protection_keyring("not-json".to_owned()).err(),
            Some(ContactKeyringConfigError::InvalidShape),
        );
        assert_eq!(
            parse_contact_protection_keyring(
                r#"{"activeEncryptionVersion":0,"activeLookupVersion":1,"encryption":[],"lookup":[]}"#.to_owned(),
            ).err(),
            Some(ContactKeyringConfigError::InvalidVersion),
        );
        let key = "ab".repeat(32);
        let missing_active = format!(
            "{{\"activeEncryptionVersion\":2,\"activeLookupVersion\":1,\"encryption\":[{{\"version\":1,\"keyHex\":\"{key}\"}}],\"lookup\":[{{\"version\":1,\"keyHex\":\"{key}\"}}]}}"
        );
        assert_eq!(
            parse_contact_protection_keyring(missing_active).err(),
            Some(ContactKeyringConfigError::MissingActiveKey),
        );
        let invalid_hex = format!(
            "{{\"activeEncryptionVersion\":1,\"activeLookupVersion\":1,\"encryption\":[{{\"version\":1,\"keyHex\":\"not-hex\"}}],\"lookup\":[{{\"version\":1,\"keyHex\":\"{key}\"}}]}}"
        );
        assert_eq!(
            parse_contact_protection_keyring(invalid_hex).err(),
            Some(ContactKeyringConfigError::InvalidKeyHex),
        );
        for code in [
            ContactKeyringConfigError::InvalidShape,
            ContactKeyringConfigError::InvalidVersion,
            ContactKeyringConfigError::InvalidKeyHex,
            ContactKeyringConfigError::MissingActiveKey,
            ContactKeyringConfigError::InvalidLifecycle,
            ContactKeyringConfigError::InvalidKeySet,
        ] {
            assert!(
                code.code()
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            );
            assert!(
                !code.code().contains(&key),
                "key material leaked in status category"
            );
        }
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
