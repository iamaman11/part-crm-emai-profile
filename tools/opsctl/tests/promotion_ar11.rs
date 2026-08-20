use opsctl::promotion::authority::load_closure;
use opsctl::promotion::plan::{PlanRequest, build};
use opsctl::promotion::preflight::{PreflightRequest, evaluate as preflight};
use opsctl::promotion::snapshot::DeploymentSnapshot;
use opsctl::release::compatibility::CompatibilityEvidence;
use opsctl::release::digest::{canonical_json, sha256_hex};
use opsctl::release::input_topology::{ReleaseInputTopology, ResolvedReleaseInput};
use opsctl::release::model::{RELEASE_SET_ID_PREFIX, ReleaseSetManifest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SHA_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
type StaticCompatibilityFields = (Value, Value, Value, Value, Value);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn accepted_main_evidence() -> Result<String, String> {
    let identity = json!({
        "authority": "accepted-main",
        "commit_sha": GIT_SHA,
        "repository": REPOSITORY,
    });
    Ok(sha256_hex(canonical_json(&identity)?.as_bytes()))
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

fn schema_window(
    authority: &Value,
    component_id: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let components = authority["components"]
        .as_array()
        .ok_or_else(|| io::Error::other("D1 authority components must be an array"))?;
    let component = components
        .iter()
        .find(|entry| entry["component_id"].as_str() == Some(component_id))
        .ok_or_else(|| io::Error::other(format!("D1 authority missing {component_id}")))?;
    let target = component["current_repository_revision"]
        .as_str()
        .ok_or_else(|| io::Error::other(format!("D1 {component_id} target revision missing")))?;
    let policy = component["compatibility_policy"].clone();
    let compatibility_policy_digest = sha256_hex(
        canonical_json(&policy)
            .map_err(io::Error::other)?
            .as_bytes(),
    );
    let history = component["historical_epoch"]["ordered_history"]
        .as_array()
        .ok_or_else(|| io::Error::other(format!("D1 {component_id} history missing")))?;
    let history_identity = Value::Array(
        history
            .iter()
            .map(|entry| json!({"name":entry["name"],"sha256":entry["sha256"]}))
            .collect(),
    );
    let migration_history_digest = sha256_hex(
        canonical_json(&history_identity)
            .map_err(io::Error::other)?
            .as_bytes(),
    );
    Ok(json!({
        "database_component": component_id,
        "target_schema_revision": target,
        "supported_schema_min": target,
        "supported_schema_max": target,
        "migration_history_digest": migration_history_digest,
        "compatibility_policy_digest": compatibility_policy_digest,
    }))
}

fn static_compatibility_fields()
-> Result<StaticCompatibilityFields, Box<dyn std::error::Error>> {
    let root = repo_root();
    let topology = ReleaseInputTopology::load(&root)?;
    let resolved = topology.resolve(&root)?;

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
    let contract_identity = Value::Array(contract_files.clone());
    let contract_sha = sha256_hex(canonical_json(&contract_identity)?.as_bytes());
    let contracts = json!({
        "files": contract_files,
        "sha256": contract_sha,
    });

    let runtime_input = resolved_input(&resolved, "camouhost_runtime_lock")?;
    let runtime_lock_bytes = fs::read(&runtime_input.absolute_path)?;
    let runtime_lock: Value = serde_json::from_slice(&runtime_lock_bytes)?;
    let ipc_version = runtime_lock["camouhost_ipc_version"]
        .as_u64()
        .ok_or_else(|| io::Error::other("canonical runtime IPC version missing"))?;
    let protocols = json!({
        "public_api_contract_sha256": contract_sha,
        "camouhost_ipc_version": ipc_version,
        "profile_bridge_protocol_version": ipc_version,
        "resolver_protocol": "mailbox-secret-resolver-v1",
    });
    let runtime_compatibility = json!({
        "runtime_lock_sha256": runtime_input.sha256,
        "runtime_role": runtime_lock["runtime_role"],
        "profile_format": runtime_lock["fingerprint_config_schema"],
        "browser_identity_policy": runtime_lock["fingerprint_policy_version"],
    });

    let d1_input = resolved_input(&resolved, "d1_evolution_authority")?;
    let d1_authority: Value = serde_json::from_slice(&fs::read(&d1_input.absolute_path)?)?;
    let schemas = json!({
        "d1_evolution_authority_sha256": d1_input.sha256,
        "catalog": schema_window(&d1_authority, "catalog")?,
        "resolver": schema_window(&d1_authority, "resolver")?,
    });
    let build_provenance = json!({
        "cargo_lock_sha256": resolved_input(&resolved, "cargo_lock")?.sha256,
        "rust_toolchain_sha256": resolved_input(&resolved, "rust_toolchain")?.sha256,
        "frontend_lock_sha256": resolved_input(&resolved, "frontend_lock")?.sha256,
        "release_architecture_sha256": resolved_input(&resolved, "release_architecture_authority")?.sha256,
    });
    Ok((
        contracts,
        protocols,
        schemas,
        runtime_compatibility,
        build_provenance,
    ))
}

fn component(release_id: &str, path: &str, digest: &str, size: u64) -> Value {
    json!({
        "release_id": release_id,
        "source_commit_sha": GIT_SHA,
        "artifact_path": path,
        "artifact_sha256": digest,
        "artifact_size_bytes": size,
        "component_manifest_sha256": SHA_A
    })
}

fn release_set() -> Result<ReleaseSetManifest, Box<dyn std::error::Error>> {
    let evidence = accepted_main_evidence()?;
    let (contracts, protocols, schemas, runtime_compatibility, build_provenance) =
        static_compatibility_fields()?;
    let mut value = json!({
        "schema_version": 2,
        "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{SHA_A}"),
        "source": {
            "repository": REPOSITORY,
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": evidence
        },
        "components": {
            "control_plane": component("control-plane-v2", "components/control-plane.tar", SHA_A, 10),
            "frontend": component("control-plane-v2", "components/control-plane.tar", SHA_A, 10),
            "secret_resolver": component("resolver-v2", "components/secret-resolver.tar", SHA_B, 11),
            "runtime_bundle": component("runtime-v2", "components/runtime-bundle.tar", SHA_C, 12),
            "profile_bridge": component("bridge-v2", "components/profile-bridge.zip", SHA_D, 13)
        },
        "contracts": contracts,
        "protocols": protocols,
        "schemas": schemas,
        "runtime_compatibility": runtime_compatibility,
        "capability_profile_compatibility": ["rehearsal-core-v1", "production-core-v1"],
        "build_provenance": build_provenance,
        "artifact_inventory": [
            {"path":"components/control-plane.tar","sha256":SHA_A,"size_bytes":10,"kind":"component"},
            {"path":"components/profile-bridge.zip","sha256":SHA_D,"size_bytes":13,"kind":"component"},
            {"path":"components/runtime-bundle.tar","sha256":SHA_C,"size_bytes":12,"kind":"component"},
            {"path":"components/secret-resolver.tar","sha256":SHA_B,"size_bytes":11,"kind":"component"}
        ]
    });
    let mut identity = value.clone();
    identity
        .as_object_mut()
        .ok_or_else(|| io::Error::other("release fixture must be an object"))?
        .remove("release_set_id");
    let digest = sha256_hex(canonical_json(&identity)?.as_bytes());
    value["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{digest}"));
    Ok(ReleaseSetManifest::parse_json(&serde_json::to_string(
        &value,
    )?)?)
}

fn evidence(
    release_set_id: &str,
    windows: &str,
) -> Result<CompatibilityEvidence, Box<dyn std::error::Error>> {
    let value = json!({
        "schema_version": 2,
        "kind": "RELEASE_COMPATIBILITY_EVIDENCE",
        "release_set_id": release_set_id,
        "dimensions": {
            "catalog_d1": {
                "decision": "COMPATIBLE",
                "evidence_sha256": SHA_A,
                "policy_source": "opsctl.d1.compatibility"
            },
            "resolver_d1": {
                "decision": "COMPATIBLE",
                "evidence_sha256": SHA_A,
                "policy_source": "opsctl.d1.compatibility"
            },
            "windows_profile_bridge": {
                "decision": windows,
                "evidence_sha256": SHA_A,
                "policy_source": "external.windows.delivery"
            }
        }
    });
    Ok(CompatibilityEvidence::parse_json(&serde_json::to_string(
        &value,
    )?)?)
}

fn converged_snapshot(
    environment: &str,
    profile_id: &str,
    release: &ReleaseSetManifest,
) -> Result<DeploymentSnapshot, Box<dyn std::error::Error>> {
    let closure = load_closure(&repo_root(), profile_id)?;
    let component_release_ids = release
        .components
        .iter()
        .map(|(id, component)| (id.clone(), component.release_id.clone()))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect();
    Ok(DeploymentSnapshot {
        environment: environment.to_owned(),
        collected_at: "2026-08-21T00:00:00Z".to_owned(),
        release_set_id: Some(release.release_set_id.clone()),
        capability_profile_id: Some(profile_id.to_owned()),
        component_release_ids,
        logical_resources: closure.required_resources,
        logical_bindings: closure.required_bindings,
        logical_credentials: closure.required_credentials,
        catalog_ledger_sha256: Some(SHA_A.to_owned()),
        catalog_schema_revision: Some(release.schemas.catalog.target_schema_revision.clone()),
        resolver_ledger_sha256: None,
        resolver_schema_revision: None,
        contracts_sha256: Some(release.contracts.sha256.clone()),
        resolver_protocol: Some(release.protocols.resolver_protocol.clone()),
        camouhost_ipc_version: Some(release.protocols.camouhost_ipc_version),
        profile_bridge_protocol_version: Some(release.protocols.profile_bridge_protocol_version),
        runtime_role: Some(release.runtime_compatibility.runtime_role.clone()),
        profile_format: Some(release.runtime_compatibility.profile_format.clone()),
        browser_identity_policy: Some(
            release
                .runtime_compatibility
                .browser_identity_policy
                .clone(),
        ),
    })
}

#[test]
fn converged_staging_plan_is_no_change_and_preflight_ready()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let target = release_set()?;
    let snapshot = converged_snapshot("staging", "rehearsal-core-v1", &target)?;
    let compatibility = evidence(&target.release_set_id, "UNKNOWN")?;

    let plan = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        expected_current_release_set_id: Some(&target.release_set_id),
    })?;
    assert_eq!(plan.decision, "NO_CHANGE");
    assert!(plan.actions.is_empty());

    let result = preflight(PreflightRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        known_good_release: Some(&target),
        expected_current_release_set_id: Some(&target.release_set_id),
    })?;
    assert!(result.ready, "staging blockers: {:?}", result.blockers);
    Ok(())
}

#[test]
fn production_remains_blocked_even_with_compatible_saved_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let target = release_set()?;
    let snapshot = converged_snapshot("production", "production-core-v1", &target)?;
    let compatibility = evidence(&target.release_set_id, "COMPATIBLE")?;
    let plan = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "production-core-v1",
        environment: "production",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        expected_current_release_set_id: Some(&target.release_set_id),
    })?;
    assert_eq!(plan.decision, "BLOCKED");
    assert!(
        plan.blockers
            .iter()
            .any(|value| value == "PRODUCTION_EXECUTION_BLOCKED_DURING_AR11")
    );
    assert!(
        plan.blockers
            .iter()
            .any(|value| value == "PROFILE_NOT_AUTHORIZED")
    );
    Ok(())
}

#[test]
fn stale_expected_current_is_rejected_before_plan_creation()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let target = release_set()?;
    let snapshot = converged_snapshot("staging", "rehearsal-core-v1", &target)?;
    let compatibility = evidence(&target.release_set_id, "UNKNOWN")?;
    let result = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        expected_current_release_set_id: Some("release-set-v2-sha256-stale"),
    });
    assert!(result.is_err());
    Ok(())
}
