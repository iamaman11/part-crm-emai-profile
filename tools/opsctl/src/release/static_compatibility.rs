use crate::release::authority::ReleaseArchitecture;
use crate::d1;
use crate::release::digest::{canonical_json, sha256_hex};
use crate::release::input_topology::{ReleaseInputTopology, ResolvedReleaseInput};
use crate::release::model::{ReleaseModelError, ReleaseSetManifest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const VERIFIED_PROVENANCE_DIMENSIONS: [&str; 8] = [
    "contracts",
    "protocols",
    "schemas",
    "runtime_compatibility",
    "cargo_lock",
    "rust_toolchain",
    "frontend_lock",
    "release_architecture",
];

/// Evaluate release-critical identities that are fully determined by the immutable
/// Release Set and the exact source checkout. Every field accepted by the v2 Release
/// Set model is either verified here/artifact verification or is explicit external
/// provider evidence handled by release compatibility.
pub fn evaluate(
    root: &Path,
    manifest: &ReleaseSetManifest,
    mailbox_admin: bool,
) -> Result<Vec<String>, ReleaseModelError> {
    let topology = ReleaseInputTopology::load(root)?;
    let resolved = topology.resolve(root)?;
    let mut blockers = Vec::new();

    if !contracts_match(&resolved, manifest)? {
        blockers.push("PROTOCOL_INCOMPATIBLE:contracts".to_owned());
    }
    if !protocols_match(&resolved, manifest, mailbox_admin)? {
        blockers.push("PROTOCOL_INCOMPATIBLE:runtime_protocols".to_owned());
    }
    if !schemas_match(root, manifest)? {
        blockers.push("SCHEMA_IDENTITY_MISMATCH".to_owned());
    }
    if !runtime_matches(&resolved, manifest)? {
        blockers.push("RUNTIME_INCOMPATIBLE:runtime_identity".to_owned());
    }
    if !build_provenance_matches(&resolved, manifest)? {
        blockers.push("PROVENANCE_IDENTITY_MISMATCH".to_owned());
    }
    if !profiles_match(root, manifest)? {
        blockers.push("PROFILE_NOT_AUTHORIZED".to_owned());
    }

    blockers.sort();
    blockers.dedup();
    Ok(blockers)
}

fn contracts_match(
    resolved: &[ResolvedReleaseInput],
    manifest: &ReleaseSetManifest,
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
    if manifest.contracts.files.len() != expected.len() {
        return Ok(false);
    }
    for entry in &manifest.contracts.files {
        let Some(expected_entry) = expected.get(entry.path.as_str()) else {
            return Ok(false);
        };
        if entry.sha256 != expected_entry.sha256 || entry.size_bytes != expected_entry.size_bytes {
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
    let canonical =
        canonical_json(&Value::Array(canonical_entries)).map_err(ReleaseModelError::new)?;
    Ok(manifest.contracts.sha256 == sha256_hex(canonical.as_bytes()))
}

fn protocols_match(
    resolved: &[ResolvedReleaseInput],
    manifest: &ReleaseSetManifest,
    mailbox_admin: bool,
) -> Result<bool, ReleaseModelError> {
    if manifest.protocols.public_api_contract_sha256 != manifest.contracts.sha256 {
        return Ok(false);
    }
    let runtime_lock = resolved_input(resolved, "camouhost_runtime_lock")?;
    let lock: Value =
        serde_json::from_slice(&fs::read(&runtime_lock.absolute_path).map_err(|error| {
            ReleaseModelError::new(format!("cannot read runtime lock: {error}"))
        })?)
        .map_err(|error| ReleaseModelError::new(format!("invalid runtime lock JSON: {error}")))?;
    let expected_ipc = lock["camouhost_ipc_version"].as_u64().ok_or_else(|| {
        ReleaseModelError::new("runtime lock camouhost_ipc_version must be unsigned")
    })?;
    if manifest.protocols.camouhost_ipc_version != expected_ipc
        || manifest.protocols.profile_bridge_protocol_version != expected_ipc
    {
        return Ok(false);
    }
    if mailbox_admin && manifest.protocols.resolver_protocol != "mailbox-secret-resolver-v1" {
        return Ok(false);
    }
    Ok(true)
}

fn schemas_match(root: &Path, manifest: &ReleaseSetManifest) -> Result<bool, ReleaseModelError> {
    let repository_identity = d1::repository_identity_sha256(root)
        .map_err(|error| ReleaseModelError::new(error.to_string()))?;
    if manifest.schemas.d1_repository_identity_sha256 != repository_identity {
        return Ok(false);
    }
    for (id, window) in [
        ("catalog", &manifest.schemas.catalog),
        ("resolver", &manifest.schemas.resolver),
    ] {
        let expected = d1::release_contract(root, id)
            .map_err(|error| ReleaseModelError::new(error.to_string()))?;
        if expected["database_component"].as_str() != Some(window.database_component.as_str())
            || expected["target_schema_revision"].as_str()
                != Some(window.target_schema_revision.as_str())
            || expected["supported_schema_min"].as_str()
                != Some(window.supported_schema_min.as_str())
            || expected["supported_schema_max"].as_str()
                != Some(window.supported_schema_max.as_str())
            || expected["migration_history_digest"].as_str()
                != Some(window.migration_history_digest.as_str())
            || expected["compatibility_policy_digest"].as_str()
                != Some(window.compatibility_policy_digest.as_str())
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn runtime_matches(
    resolved: &[ResolvedReleaseInput],
    manifest: &ReleaseSetManifest,
) -> Result<bool, ReleaseModelError> {
    let runtime_lock = resolved_input(resolved, "camouhost_runtime_lock")?;
    if manifest.runtime_compatibility.runtime_lock_sha256 != runtime_lock.sha256 {
        return Ok(false);
    }
    let lock: Value =
        serde_json::from_slice(&fs::read(&runtime_lock.absolute_path).map_err(|error| {
            ReleaseModelError::new(format!("cannot read runtime lock: {error}"))
        })?)
        .map_err(|error| ReleaseModelError::new(format!("invalid runtime lock JSON: {error}")))?;
    Ok(manifest.runtime_compatibility.runtime_role
        == lock["runtime_role"].as_str().unwrap_or_default()
        && manifest.runtime_compatibility.profile_format
            == lock["fingerprint_config_schema"]
                .as_str()
                .unwrap_or_default()
        && manifest.runtime_compatibility.browser_identity_policy
            == lock["fingerprint_policy_version"]
                .as_str()
                .unwrap_or_default()
        && manifest.runtime_compatibility.runtime_role == "real_camoufox")
}

fn build_provenance_matches(
    resolved: &[ResolvedReleaseInput],
    manifest: &ReleaseSetManifest,
) -> Result<bool, ReleaseModelError> {
    let expected = [
        ("cargo_lock", &manifest.build_provenance.cargo_lock_sha256),
        (
            "rust_toolchain",
            &manifest.build_provenance.rust_toolchain_sha256,
        ),
        (
            "frontend_lock",
            &manifest.build_provenance.frontend_lock_sha256,
        ),
        (
            "release_architecture_authority",
            &manifest.build_provenance.release_architecture_sha256,
        ),
    ];
    for (input_id, observed) in expected {
        if resolved_input(resolved, input_id)?.sha256 != *observed {
            return Ok(false);
        }
    }
    Ok(true)
}

fn profiles_match(root: &Path, manifest: &ReleaseSetManifest) -> Result<bool, ReleaseModelError> {
    let authority = ReleaseArchitecture::load(root)
        .map_err(|error| ReleaseModelError::new(format!("release authority invalid: {error}")))?;
    Ok(manifest
        .capability_profile_compatibility
        .iter()
        .all(|profile| authority.profiles.contains_key(profile)))
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

#[cfg(test)]
mod tests {
    use crate::release::input_topology::ReleaseInputTopology;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn contract_inventory_is_owned_by_canonical_topology() -> Result<(), Box<dyn std::error::Error>>
    {
        let topology = ReleaseInputTopology::load(&root())?;
        let contracts = topology.inputs_for_consumer("release_set.contracts");
        assert_eq!(contracts.len(), 10);
        assert!(
            contracts
                .iter()
                .any(|input| input.release_identity_source == "openapi/v1/openapi.json")
        );
        assert!(
            contracts
                .iter()
                .all(|input| input.release_identity_source != "openapi/v1/control-plane.yaml")
        );
        Ok(())
    }
}
