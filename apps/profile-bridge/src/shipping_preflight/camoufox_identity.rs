//! Camoufox owns fingerprints. Rust only checks the sealed config bytes and file safety.
use browser_execution_domain::BrowserIdentityManifest;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const CONFIG_NAME: &str = "camoufox-config.json";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CamoufoxConfigError {
    InvalidMaterialization,
    IdentityMismatch,
}

impl core::fmt::Display for CamoufoxConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidMaterialization => "Camoufox config is not a valid bounded JSON object",
            Self::IdentityMismatch => "Camoufox config bytes differ from the sealed generation",
        })
    }
}
impl std::error::Error for CamoufoxConfigError {}

/// Validates transport shape, never the set of upstream fingerprint keys.
pub fn validate_camoufox_config_bytes(raw: &[u8]) -> Result<(), CamoufoxConfigError> {
    if raw.is_empty() || raw.len() as u64 > MAX_CONFIG_BYTES {
        return Err(CamoufoxConfigError::InvalidMaterialization);
    }
    let value: Value =
        serde_json::from_slice(raw).map_err(|_| CamoufoxConfigError::InvalidMaterialization)?;
    if !value.as_object().is_some_and(|object| !object.is_empty()) {
        return Err(CamoufoxConfigError::InvalidMaterialization);
    }
    Ok(())
}

/// Pre-spawn: only exact immutable Camoufox config; no Rust fingerprint projection.
pub fn verify_materialized_camoufox_identity(
    profile_root: &Path,
    expected: &BrowserIdentityManifest,
) -> Result<(), CamoufoxConfigError> {
    let path = profile_root.join(CONFIG_NAME);
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| CamoufoxConfigError::InvalidMaterialization)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(CamoufoxConfigError::InvalidMaterialization);
    }
    let raw = fs::read(path).map_err(|_| CamoufoxConfigError::InvalidMaterialization)?;
    validate_camoufox_config_bytes(&raw)?;
    let actual = Sha256::digest(&raw);
    let mut hex = String::with_capacity(64);
    use core::fmt::Write as _;
    for byte in actual {
        let _ = write!(hex, "{byte:02x}");
    }
    if hex != expected.fingerprint_config_sha256() {
        return Err(CamoufoxConfigError::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn accepts_official_config_without_rust_fingerprint_contracts() {
        // Official Camoufox need not serialize canvas seed, DPR, navigator languages,
        // touch points, font spacing, or any particular WebGL implementation.
        let valid = br#"{"navigator.userAgent":"Mozilla/5.0 Firefox/158.0","audio:seed":123}"#;
        assert_eq!(validate_camoufox_config_bytes(valid), Ok(()));
        for bad in [b"".as_slice(), b"[]", b"{}", b"null", b"{", b"not-json"] {
            assert_eq!(
                validate_camoufox_config_bytes(bad),
                Err(CamoufoxConfigError::InvalidMaterialization)
            );
        }
    }

    #[test]
    fn exact_bytes_are_bound_and_changed_or_invalid_files_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "camoufox-config-seal-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&root)?;
        let valid = br#"{"browser":"camoufox158","audio:seed":123}"#;
        let digest = Sha256::digest(valid)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let identity = BrowserIdentityManifest::new_native(
            2,
            "profile-stability-v1",
            "158.0.1-beta.41",
            "a".repeat(64),
            "profile-stability-v1-probe-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            digest,
        )?;
        let config = root.join(CONFIG_NAME);
        assert_eq!(
            verify_materialized_camoufox_identity(&root, &identity),
            Err(CamoufoxConfigError::InvalidMaterialization)
        );
        fs::write(&config, valid)?;
        assert_eq!(
            verify_materialized_camoufox_identity(&root, &identity),
            Ok(())
        );
        fs::write(&config, br#"{"browser":"different","audio:seed":123}"#)?;
        assert_eq!(
            verify_materialized_camoufox_identity(&root, &identity),
            Err(CamoufoxConfigError::IdentityMismatch)
        );
        fs::write(&config, b"not-json")?;
        assert_eq!(
            verify_materialized_camoufox_identity(&root, &identity),
            Err(CamoufoxConfigError::InvalidMaterialization)
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
