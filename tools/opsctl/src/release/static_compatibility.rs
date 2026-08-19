use crate::release::digest::sha256_reader_hex;
use crate::release::model::{ReleaseModelError, ReleaseSetManifest};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::Path;

const CONTRACT_FILES: [&str; 2] = [
    "contracts/generated/control-plane.openapi.json",
    "openapi/v1/control-plane.yaml",
];
const CONTROL_PROTOCOL: &str = "crates/control-plane-contract/src/lib.rs";
const RUNTIME_LOCK: &str = "runtime/camouhost/runtime-lock.json";

/// Evaluate compatibility dimensions that are completely determined by the immutable
/// Release Set and the exact source checkout. No caller-provided decision can make
/// these dimensions compatible.
pub fn evaluate(
    root: &Path,
    manifest: &ReleaseSetManifest,
    mailbox_admin: bool,
) -> Result<Vec<String>, ReleaseModelError> {
    let mut blockers = Vec::new();
    if !contracts_match(root, &manifest.contracts)? {
        blockers.push("PROTOCOL_INCOMPATIBLE:contracts".to_owned());
    }
    if !protocols_match(root, &manifest.protocols, mailbox_admin)? {
        blockers.push("PROTOCOL_INCOMPATIBLE:runtime_protocols".to_owned());
    }
    if !runtime_matches(root, &manifest.runtime_compatibility)? {
        blockers.push("RUNTIME_INCOMPATIBLE:runtime_identity".to_owned());
    }
    blockers.sort();
    blockers.dedup();
    Ok(blockers)
}

fn contracts_match(root: &Path, value: &Value) -> Result<bool, ReleaseModelError> {
    let contracts = object(value, "contracts")?;
    let files = array(required(contracts, "files")?, "contracts.files")?;
    let observed_paths = files
        .iter()
        .map(|value| {
            object(value, "contracts.files entry")?
                .get("path")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    ReleaseModelError::new("contracts.files entry path must be a string")
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let expected_paths = CONTRACT_FILES
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if observed_paths != expected_paths {
        return Ok(false);
    }
    for value in files {
        let entry = object(value, "contracts.files entry")?;
        let path = required_string(entry, "path")?;
        let expected_hash = required_string(entry, "sha256")?;
        let expected_size = required_u64(entry, "size_bytes")?;
        if !file_matches(root.join(path), &expected_hash, expected_size)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn protocols_match(
    root: &Path,
    value: &Value,
    mailbox_admin: bool,
) -> Result<bool, ReleaseModelError> {
    let protocols = object(value, "protocols")?;
    let expected_control_hash = file_sha256(&root.join(CONTROL_PROTOCOL))?;
    if required_string(protocols, "control_plane_contract_sha256")? != expected_control_hash {
        return Ok(false);
    }

    let runtime_lock = load_json_object(&root.join(RUNTIME_LOCK), "runtime lock")?;
    let expected_ipc = required_u64(&runtime_lock, "camouhost_ipc_version")?;
    if required_u64(protocols, "camouhost_ipc_version")? != expected_ipc {
        return Ok(false);
    }
    if mailbox_admin
        && required_string(protocols, "resolver_protocol")? != "mailbox-secret-resolver-v1"
    {
        return Ok(false);
    }
    Ok(true)
}

fn runtime_matches(root: &Path, value: &Value) -> Result<bool, ReleaseModelError> {
    let runtime = object(value, "runtime_compatibility")?;
    let runtime_lock_path = root.join(RUNTIME_LOCK);
    if required_string(runtime, "runtime_lock_sha256")? != file_sha256(&runtime_lock_path)? {
        return Ok(false);
    }
    let lock = load_json_object(&runtime_lock_path, "runtime lock")?;
    for (manifest_field, lock_field) in [
        ("runtime_role", "runtime_role"),
        ("profile_format", "fingerprint_config_schema"),
        ("browser_identity_policy", "fingerprint_policy_version"),
    ] {
        if required_string(runtime, manifest_field)? != required_string(&lock, lock_field)? {
            return Ok(false);
        }
    }
    Ok(required_string(runtime, "runtime_role")? == "real_camoufox")
}

fn file_matches(
    path: impl AsRef<Path>,
    expected_hash: &str,
    expected_size: u64,
) -> Result<bool, ReleaseModelError> {
    let path = path.as_ref();
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        ReleaseModelError::new(format!(
            "static compatibility input missing at {}: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != expected_size {
        return Ok(false);
    }
    Ok(file_sha256(path)? == expected_hash)
}

fn file_sha256(path: &Path) -> Result<String, ReleaseModelError> {
    let mut file = File::open(path).map_err(|error| {
        ReleaseModelError::new(format!(
            "cannot open static compatibility input {}: {error}",
            path.display()
        ))
    })?;
    sha256_reader_hex(&mut file).map_err(|error| {
        ReleaseModelError::new(format!(
            "cannot hash static compatibility input {}: {error}",
            path.display()
        ))
    })
}

fn load_json_object(path: &Path, label: &str) -> Result<Map<String, Value>, ReleaseModelError> {
    let text = fs::read_to_string(path).map_err(|error| {
        ReleaseModelError::new(format!("cannot read {label} {}: {error}", path.display()))
    })?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ReleaseModelError::new(format!("invalid {label} JSON: {error}")))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be a JSON object")))
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object
        .get(key)
        .ok_or_else(|| ReleaseModelError::new(format!("missing static compatibility field: {key}")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?
        .as_u64()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be an unsigned integer")))
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be a JSON object")))
}

fn array<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>, ReleaseModelError> {
    value
        .as_array()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be a JSON array")))
}

#[cfg(test)]
mod tests {
    use super::CONTRACT_FILES;

    #[test]
    fn contract_inventory_is_explicit_and_stable() {
        assert_eq!(CONTRACT_FILES.len(), 2);
        assert!(CONTRACT_FILES.contains(&"openapi/v1/control-plane.yaml"));
    }
}
