use crate::release::digest::{canonical_json, sha256_hex};
use crate::release::input_topology::{ReleaseInputTopology, ResolvedReleaseInput};
use crate::release::model::{ReleaseModelError, ReleaseSetManifest};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Evaluate compatibility dimensions that are completely determined by the immutable
/// Release Set and the exact source checkout. Release-critical repository paths are
/// resolved exclusively from the canonical AR-11 release input topology.
pub fn evaluate(
    root: &Path,
    manifest: &ReleaseSetManifest,
    mailbox_admin: bool,
) -> Result<Vec<String>, ReleaseModelError> {
    let topology = ReleaseInputTopology::load(root)?;
    let resolved = topology.resolve(root)?;

    let mut blockers = Vec::new();
    if !contracts_match(&resolved, &manifest.contracts)? {
        blockers.push("PROTOCOL_INCOMPATIBLE:contracts".to_owned());
    }
    if !protocols_match(&resolved, &manifest.contracts, &manifest.protocols, mailbox_admin)? {
        blockers.push("PROTOCOL_INCOMPATIBLE:runtime_protocols".to_owned());
    }
    if !runtime_matches(&resolved, &manifest.runtime_compatibility)? {
        blockers.push("RUNTIME_INCOMPATIBLE:runtime_identity".to_owned());
    }
    blockers.sort();
    blockers.dedup();
    Ok(blockers)
}

fn contracts_match(
    resolved: &[ResolvedReleaseInput],
    value: &Value,
) -> Result<bool, ReleaseModelError> {
    let expected = resolved
        .iter()
        .filter(|input| input.input.consumed_by("release_set.contracts"))
        .map(|input| (input.input.release_identity_source.as_str(), input))
        .collect::<BTreeMap<_, _>>();
    if expected.is_empty() {
        return Err(ReleaseModelError::new(
            "canonical release input topology has no release_set.contracts inputs",
        ));
    }

    let contracts = object(value, "contracts")?;
    let files = array(required(contracts, "files")?, "contracts.files")?;
    if files.len() != expected.len() {
        return Ok(false);
    }

    for value in files {
        let entry = object(value, "contracts.files entry")?;
        let path = required_string(entry, "path")?;
        let Some(expected_entry) = expected.get(path.as_str()) else {
            return Ok(false);
        };
        if required_string(entry, "sha256")? != expected_entry.sha256
            || required_u64(entry, "size_bytes")? != expected_entry.size_bytes
        {
            return Ok(false);
        }
    }

    let canonical_entries = expected
        .values()
        .map(|entry| {
            json!({
                "path": entry.input.release_identity_source,
                "sha256": entry.sha256,
                "size_bytes": entry.size_bytes,
            })
        })
        .collect::<Vec<_>>();
    let canonical = canonical_json(&Value::Array(canonical_entries)).map_err(ReleaseModelError::new)?;
    let expected_digest = sha256_hex(canonical.as_bytes());
    Ok(required_string(contracts, "sha256")? == expected_digest)
}

fn protocols_match(
    resolved: &[ResolvedReleaseInput],
    contracts: &Value,
    value: &Value,
    mailbox_admin: bool,
) -> Result<bool, ReleaseModelError> {
    let protocols = object(value, "protocols")?;
    let contract_object = object(contracts, "contracts")?;
    if required_string(protocols, "public_api_contract_sha256")?
        != required_string(contract_object, "sha256")?
    {
        return Ok(false);
    }

    let runtime_lock = resolved_input(resolved, "camouhost_runtime_lock")?;
    let runtime_lock = load_json_object(&runtime_lock.absolute_path, "runtime lock")?;
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

fn runtime_matches(
    resolved: &[ResolvedReleaseInput],
    value: &Value,
) -> Result<bool, ReleaseModelError> {
    let runtime = object(value, "runtime_compatibility")?;
    let runtime_lock = resolved_input(resolved, "camouhost_runtime_lock")?;
    if required_string(runtime, "runtime_lock_sha256")? != runtime_lock.sha256 {
        return Ok(false);
    }
    let lock = load_json_object(&runtime_lock.absolute_path, "runtime lock")?;
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

fn resolved_input<'a>(
    resolved: &'a [ResolvedReleaseInput],
    input_id: &str,
) -> Result<&'a ResolvedReleaseInput, ReleaseModelError> {
    resolved
        .iter()
        .find(|input| input.input.input_id == input_id)
        .ok_or_else(|| {
            ReleaseModelError::new(format!(
                "canonical release input topology is missing {input_id}"
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
    use crate::release::input_topology::ReleaseInputTopology;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn contract_inventory_is_owned_by_canonical_topology()
    -> Result<(), Box<dyn std::error::Error>> {
        let topology = ReleaseInputTopology::load(&root())?;
        let contracts = topology.inputs_for_consumer("release_set.contracts");
        assert_eq!(contracts.len(), 10);
        assert!(
            contracts
                .iter()
                .any(|input| input.release_identity_source == "openapi/v1/openapi.json")
        );
        assert!(contracts.iter().all(|input| {
            input.release_identity_source != "openapi/v1/control-plane.yaml"
        }));
        Ok(())
    }
}
