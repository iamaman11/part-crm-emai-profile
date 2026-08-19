use opsctl::promotion::authority::load_closure;
use opsctl::promotion::plan::{PlanRequest, build};
use opsctl::promotion::preflight::{PreflightRequest, evaluate as preflight};
use opsctl::promotion::snapshot::DeploymentSnapshot;
use opsctl::release::compatibility::CompatibilityEvidence;
use opsctl::release::digest::{canonical_json, sha256_hex};
use opsctl::release::model::{RELEASE_SET_ID_PREFIX, ReleaseSetManifest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SHA_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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
    let mut value = json!({
        "schema_version": 1,
        "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{SHA_A}"),
        "source": {
            "repository": "iamaman11/part-crm-emai-profile",
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": SHA_A
        },
        "components": {
            "control_plane": component("control-plane-v1", "components/control-plane.tar", SHA_A, 10),
            "frontend": component("frontend-v1", "components/frontend.tar", SHA_A, 14),
            "secret_resolver": component("resolver-v1", "components/resolver.tar", SHA_B, 11),
            "runtime_bundle": component("runtime-v1", "components/runtime.tar", SHA_C, 12),
            "profile_bridge": component("bridge-v1", "components/profile-bridge.zip", SHA_D, 13)
        },
        "contracts": {"openapi_sha256": SHA_A},
        "protocols": {"bridge":"v1","camouhost_ipc":"v1","resolver":"v1"},
        "schemas": {"catalog":{"min":1,"max":26,"target":26},"resolver":{"min":1,"max":2,"target":2}},
        "runtime_compatibility": {"runtime_bundle":"v1","profile_format":"v1","browser_identity_policy":"v1"},
        "capability_profile_compatibility": ["rehearsal-core-v1", "production-core-v1"],
        "build_provenance": {"toolchain":"rust-1.97.1","lockfile_sha256":SHA_A},
        "artifact_inventory": [
            {"path":"components/control-plane.tar","sha256":SHA_A,"size_bytes":10,"kind":"component"},
            {"path":"components/frontend.tar","sha256":SHA_A,"size_bytes":14,"kind":"component"},
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
