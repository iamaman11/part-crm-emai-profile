use opsctl::release::digest::{canonical_json, sha256_hex};
use opsctl::release::input_topology::{ReleaseInputTopology, ResolvedReleaseInput};
use opsctl::release::model::{RELEASE_SET_ID_PREFIX, ReleaseSetManifest, parse_json};
use opsctl::release::static_compatibility;
use serde_json::{Value, json};
use std::io;
use std::path::{Path, PathBuf};

const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_GIT_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct CanonicalIdentityFields {
    contracts: Value,
    protocols: Value,
    schemas: Value,
    runtime: Value,
    build: Value,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn resolved_input<'a>(
    resolved: &'a [ResolvedReleaseInput],
    input_id: &str,
) -> Result<&'a ResolvedReleaseInput, io::Error> {
    resolved
        .iter()
        .find(|input| input.input.input_id == input_id)
        .ok_or_else(|| io::Error::other(format!("canonical release input missing: {input_id}")))
}

fn d1_schema(projection: &Value, component_id: &str) -> Result<Value, io::Error> {
    let component = projection["components"]
        .as_array()
        .ok_or_else(|| io::Error::other("D1 repository components must be an array"))?
        .iter()
        .find(|entry| entry["component_id"].as_str() == Some(component_id))
        .ok_or_else(|| io::Error::other(format!("D1 repository missing {component_id}")))?;
    component
        .get("release_schema_contract")
        .cloned()
        .ok_or_else(|| io::Error::other(format!("D1 {component_id} release contract missing")))
}

fn canonical_identity_fields(
    root: &Path,
) -> Result<CanonicalIdentityFields, Box<dyn std::error::Error>> {
    let topology = ReleaseInputTopology::load(root)?;
    let resolved = topology.resolve(root)?;

    let mut contract_files = resolved
        .iter()
        .filter(|input| input.input.consumed_by("release_set.contracts"))
        .map(|input| {
            json!({
                "path": input.input.release_identity_source,
                "sha256": input.sha256,
                "size_bytes": input.size_bytes,
            })
        })
        .collect::<Vec<_>>();
    contract_files.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    let contract_sha =
        sha256_hex(canonical_json(&Value::Array(contract_files.clone()))?.as_bytes());
    let contracts = json!({"files": contract_files, "sha256": contract_sha});

    let runtime_input = resolved_input(&resolved, "camouhost_runtime_lock")?;
    let runtime_lock: Value =
        serde_json::from_slice(&std::fs::read(&runtime_input.absolute_path)?)?;
    let protocols = json!({
        "public_api_contract_sha256": contract_sha,
        "camouhost_ipc_version": runtime_lock["camouhost_ipc_version"],
        "profile_bridge_protocol_version": runtime_lock["camouhost_ipc_version"],
        "resolver_protocol": "mailbox-secret-resolver-v1",
    });
    let runtime = json!({
        "runtime_lock_sha256": runtime_input.sha256,
        "runtime_role": runtime_lock["runtime_role"],
        "profile_format": runtime_lock["fingerprint_config_schema"],
        "browser_identity_policy": runtime_lock["fingerprint_policy_version"],
    });

    let d1_projection: Value = serde_json::from_str(&opsctl::d1::repository_projection(root)?)?;
    let schemas = json!({
        "d1_repository_identity_sha256": d1_projection["repository_identity_sha256"],
        "catalog": d1_schema(&d1_projection, "catalog")?,
        "resolver": d1_schema(&d1_projection, "resolver")?,
    });
    let build = json!({
        "cargo_lock_sha256": resolved_input(&resolved, "cargo_lock")?.sha256,
        "rust_toolchain_sha256": resolved_input(&resolved, "rust_toolchain")?.sha256,
        "frontend_lock_sha256": resolved_input(&resolved, "frontend_lock")?.sha256,
        "release_architecture_sha256": resolved_input(&resolved, "release_architecture_authority")?.sha256,
    });
    Ok(CanonicalIdentityFields {
        contracts,
        protocols,
        schemas,
        runtime,
        build,
    })
}

fn component(id: &str, path: &str, digest: &str, size: u64) -> Value {
    json!({
        "release_id": id,
        "source_commit_sha": GIT_SHA,
        "artifact_path": path,
        "artifact_sha256": digest,
        "artifact_size_bytes": size,
        "component_manifest_sha256": SHA_A,
    })
}

fn fixture_value() -> Result<Value, Box<dyn std::error::Error>> {
    let root = repo_root();
    let identity = canonical_identity_fields(&root)?;
    let accepted = sha256_hex(
        canonical_json(&json!({
            "authority": "accepted-main",
            "commit_sha": GIT_SHA,
            "repository": REPOSITORY,
        }))?
        .as_bytes(),
    );
    let mut value = json!({
        "schema_version": 3,
        "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{SHA_A}"),
        "source": {
            "repository": REPOSITORY,
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": accepted,
        },
        "components": {
            "control_plane": component("control-plane-v2", "components/control-plane.tar", SHA_A, 10),
            "frontend": component("control-plane-v2", "components/control-plane.tar", SHA_A, 10),
            "secret_resolver": component("resolver-v2", "components/secret-resolver.tar", SHA_B, 11),
            "runtime_bundle": component("runtime-v2", "components/runtime-bundle.tar", SHA_A, 12),
        },
        "contracts": identity.contracts,
        "protocols": identity.protocols,
        "schemas": identity.schemas,
        "runtime_compatibility": identity.runtime,
        "capability_profile_compatibility": ["rehearsal-core-v1"],
        "build_provenance": identity.build,
        "artifact_inventory": [
            {"path":"components/control-plane.tar","sha256":SHA_A,"size_bytes":10,"kind":"component"},
            {"path":"components/runtime-bundle.tar","sha256":SHA_A,"size_bytes":12,"kind":"component"},
            {"path":"components/secret-resolver.tar","sha256":SHA_B,"size_bytes":11,"kind":"component"},
        ],
    });
    resign(&mut value)?;
    Ok(value)
}

fn resign(value: &mut Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut identity = value.clone();
    identity
        .as_object_mut()
        .ok_or_else(|| io::Error::other("release fixture must be an object"))?
        .remove("release_set_id");
    identity
        .as_object_mut()
        .ok_or_else(|| io::Error::other("release fixture must be an object"))?
        .remove("display_version");
    value["release_set_id"] = Value::String(format!(
        "{RELEASE_SET_ID_PREFIX}{}",
        sha256_hex(canonical_json(&identity)?.as_bytes())
    ));
    Ok(())
}

fn parse(value: &Value) -> Result<ReleaseSetManifest, String> {
    parse_json(&serde_json::to_string(value).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn require_error(
    result: Result<ReleaseSetManifest, String>,
    context: &str,
) -> Result<String, io::Error> {
    result
        .err()
        .ok_or_else(|| io::Error::other(context.to_owned()))
}

#[test]
fn artifact_from_another_sha_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    value["components"]["control_plane"]["source_commit_sha"] =
        Value::String(OTHER_GIT_SHA.to_owned());
    resign(&mut value)?;
    let error = require_error(
        parse(&value),
        "foreign-source component unexpectedly accepted",
    )?;
    assert!(error.contains("SOURCE_IDENTITY_MISMATCH"));
    Ok(())
}

#[test]
fn changed_component_digest_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    value["components"]["control_plane"]["artifact_sha256"] = Value::String(SHA_B.to_owned());
    resign(&mut value)?;
    let error = require_error(
        parse(&value),
        "component/inventory digest disagreement unexpectedly accepted",
    )?;
    assert!(error.contains("artifact identity disagrees with artifact_inventory"));
    Ok(())
}

#[test]
fn release_set_digest_mismatch_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    value["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{SHA_B}"));
    let error = require_error(
        parse(&value),
        "wrong Release Set content address unexpectedly accepted",
    )?;
    assert!(error.contains("RELEASE_IDENTITY_MISMATCH"));
    Ok(())
}

#[test]
fn missing_artifact_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    value["artifact_inventory"]
        .as_array_mut()
        .ok_or_else(|| io::Error::other("artifact inventory must be array"))?
        .retain(|row| row["path"] != "components/control-plane.tar");
    resign(&mut value)?;
    let error = require_error(
        parse(&value),
        "missing component artifact unexpectedly accepted",
    )?;
    assert!(error.contains("artifact is absent from artifact_inventory"));
    Ok(())
}

#[test]
fn duplicate_artifact_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    let duplicate = value["artifact_inventory"][0].clone();
    value["artifact_inventory"]
        .as_array_mut()
        .ok_or_else(|| io::Error::other("artifact inventory must be array"))?
        .push(duplicate);
    resign(&mut value)?;
    let error = require_error(parse(&value), "duplicate artifact unexpectedly accepted")?;
    assert!(error.contains("duplicate artifact path"));
    Ok(())
}

#[test]
fn unknown_component_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    value["components"]["rogue_component"] = value["components"]["runtime_bundle"].clone();
    resign(&mut value)?;
    let error = require_error(parse(&value), "unknown component unexpectedly accepted")?;
    assert!(error.contains("unknown component"));
    Ok(())
}

#[test]
fn contract_digest_mismatch_is_rejected_by_static_compatibility()
-> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    value["contracts"]["sha256"] = Value::String(SHA_B.to_owned());
    value["protocols"]["public_api_contract_sha256"] = Value::String(SHA_B.to_owned());
    resign(&mut value)?;
    let release = parse(&value).map_err(io::Error::other)?;
    let blockers = static_compatibility::evaluate(&repo_root(), &release, false)?;
    assert!(
        blockers
            .iter()
            .any(|value| value == "PROTOCOL_INCOMPATIBLE:contracts")
    );
    Ok(())
}

#[test]
fn unknown_capability_profile_is_rejected_by_static_compatibility()
-> Result<(), Box<dyn std::error::Error>> {
    let mut value = fixture_value()?;
    value["capability_profile_compatibility"] = json!(["unknown-profile-v1"]);
    resign(&mut value)?;
    let release = parse(&value).map_err(io::Error::other)?;
    let blockers = static_compatibility::evaluate(&repo_root(), &release, false)?;
    assert!(
        blockers
            .iter()
            .any(|value| value == "PROFILE_NOT_AUTHORIZED")
    );
    Ok(())
}
