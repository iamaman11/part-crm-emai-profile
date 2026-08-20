use opsctl::promotion::authority::load_closure;
use opsctl::promotion::plan::{PlanRequest, build};
use opsctl::promotion::preflight::{PreflightRequest, evaluate as preflight};
use opsctl::promotion::snapshot::DeploymentSnapshot;
use opsctl::release::compatibility::CompatibilityEvidence;
use opsctl::release::digest::{canonical_json, sha256_hex};
use opsctl::release::input_topology::ReleaseInputTopology;
use opsctl::release::model::{RELEASE_SET_ID_PREFIX, ReleaseSetManifest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SHA_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

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

fn static_compatibility_fields() -> Result<(Value, Value, Value), Box<dyn std::error::Error>> {
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

    let runtime_input = resolved
        .iter()
        .find(|input| input.input.input_id == "camouhost_runtime_lock")
        .ok_or("canonical runtime lock input is missing")?;
    let runtime_lock_bytes = fs::read(&runtime_input.absolute_path)?;
    let runtime_lock: Value = serde_json::from_slice(&runtime_lock_bytes)?;
    let protocols = json!({
        "public_api_contract_sha256": contract_sha,
        "camouhost_ipc_version": runtime_lock["camouhost_ipc_version"],
        "resolver_protocol": "mailbox-secret-resolver-v1",
    });
    let runtime_compatibility = json!({
        "runtime_lock_sha256": runtime_input.sha256,
        "runtime_role": runtime_lock["runtime_role"],
        "profile_format": runtime_lock["fingerprint_config_schema"],
        "browser_identity_policy": runtime_lock["fingerprint_policy_version"],
    });
    Ok((contracts, protocols, runtime_compatibility))
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
    let (contracts, protocols, runtime_compatibility) = static_compatibility_fields()?;
    let mut value = json!({
        "schema_version": 1,
        "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{SHA_A}"),
        "source": {
            "repository": REPOSITORY,
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": evidence
        },
        "components": {
            "control_plane": component("control-plane-v1", "components/control-plane.tar", SHA_A, 10),
            "frontend": component("frontend-v1", "components/control-plane.tar", SHA_A, 10),
            "secret_resolver": component("resolver-v1", "components/resolver.tar", SHA_B, 11),
            "runtime_bundle": component("runtime-v1", "components/runtime.tar", SHA_C, 12),
            "profile_bridge": component("bridge-v1", "components/profile-bridge.zip", SHA_D, 13)
        },
        "contracts": contracts,
        "protocols": protocols,
        "schemas": {"d1_evolution_authority_sha256": SHA_A},
        "runtime_compatibility": runtime_compatibility,
        "capability_profile_compatibility": ["rehearsal-core-v1", "production-core-v1"],
        "build_provenance": {"toolchain":"rust-1.97.1","lockfile_sha256":SHA_A},
        "artifact_inventory": [
            {"path":"components/control-plane.tar","sha256":SHA_A,"size_bytes":10,"kind":"component"},
            {"path":"components/resolver.tar","sha256":SHA_B,"size_bytes":11,"kind":"component"},
            {"path":"components/runtime.tar","sha256":SHA_C,"size_bytes":12,"kind":"component"},
            {"path":"components/profile-bridge.zip","sha256":SHA_D,"size_bytes":13,"kind":"component"}
        ]
    });
    let mut identity = value.clone();
    identity
        .as_object_mut()
        .ok_or("release fixture must be an object")?
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
    let mut dimensions = serde_json::Map::new();
    for name in [
        "catalog_d1",
        "resolver_d1",
        "public_api",
        "frontend_api",
        "resolver_protocol",
        "bridge_protocol",
        "camouhost_ipc",
        "runtime_bundle",
        "profile_format",
        "browser_identity_policy",
        "windows_profile_bridge",
    ] {
        let decision = if name == "windows_profile_bridge" {
            windows
        } else {
            "COMPATIBLE"
        };
        let policy_source = if matches!(name, "catalog_d1" | "resolver_d1") {
            "opsctl.d1.compatibility"
        } else {
            "saved-machine-evidence"
        };
        dimensions.insert(
            name.to_owned(),
            json!({
                "decision": decision,
                "evidence_sha256": SHA_A,
                "policy_source": policy_source
            }),
        );
    }
    let value = json!({
        "schema_version": 1,
        "kind": "RELEASE_COMPATIBILITY_EVIDENCE",
        "release_set_id": release_set_id,
        "dimensions": dimensions
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
        collected_at: "2026-08-19T00:00:00Z".to_owned(),
        release_set_id: Some(release.release_set_id.clone()),
        capability_profile_id: Some(profile_id.to_owned()),
        component_release_ids,
        logical_resources: closure.required_resources,
        logical_bindings: closure.required_bindings,
        logical_credentials: closure.required_credentials,
        catalog_ledger_sha256: Some(SHA_A.to_owned()),
        resolver_ledger_sha256: None,
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
        expected_current_release_set_id: Some("release-set-v1-sha256-stale"),
    });
    assert!(result.is_err());
    Ok(())
}
