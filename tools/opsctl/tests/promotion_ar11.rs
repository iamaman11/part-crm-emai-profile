use opsctl::promotion::authority::load_closure;
use opsctl::promotion::baseline_adoption::{
    BaselineAdoptionObservation, BaselineAdoptionRequest, evaluate as baseline_adoption,
};
use opsctl::promotion::plan::{PlanRequest, build};
use opsctl::promotion::preflight::{PreflightRequest, evaluate as preflight};
use opsctl::promotion::snapshot::DeploymentSnapshot;
use opsctl::release::compatibility::CompatibilityEvidence;
use opsctl::release::digest::{canonical_json, sha256_hex};
use opsctl::release::document::LoadedReleaseSet;
use opsctl::release::input_topology::{ReleaseInputTopology, ResolvedReleaseInput};
use opsctl::release::v3_dto::ReleaseSetV3Dto;
use opsctl::release::v3_output::render_release_set_v3;
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

struct StaticCompatibilityFields {
    contracts: Value,
    protocols: Value,
    schemas: Value,
    runtime_compatibility: Value,
    build_provenance: Value,
}

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

fn d1_schema(projection: &Value, component_id: &str) -> Result<Value, io::Error> {
    let components = projection["components"]
        .as_array()
        .ok_or_else(|| io::Error::other("D1 repository components must be an array"))?;
    let component = components
        .iter()
        .find(|entry| entry["component_id"].as_str() == Some(component_id))
        .ok_or_else(|| io::Error::other(format!("D1 repository missing {component_id}")))?;
    component
        .get("release_schema_contract")
        .cloned()
        .ok_or_else(|| io::Error::other(format!("D1 {component_id} release contract missing")))
}

fn static_compatibility_fields() -> Result<StaticCompatibilityFields, Box<dyn std::error::Error>> {
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

    let public_api_root = resolved_input(&resolved, "public_api_root")?;
    let runtime_input = resolved_input(&resolved, "camouhost_runtime_lock")?;
    let runtime_lock_bytes = fs::read(&runtime_input.absolute_path)?;
    let runtime_lock: Value = serde_json::from_slice(&runtime_lock_bytes)?;
    let ipc_version = runtime_lock["camouhost_ipc_version"]
        .as_u64()
        .ok_or_else(|| io::Error::other("canonical runtime IPC version missing"))?;
    let protocols = json!({
        "public_api_contract_sha256": public_api_root.sha256,
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

    let d1_projection: Value = serde_json::from_str(&opsctl::d1::repository_projection(&root)?)?;
    let schemas = json!({
        "d1_repository_identity_sha256": d1_projection["repository_identity_sha256"],
        "catalog": d1_schema(&d1_projection, "catalog")?,
        "resolver": d1_schema(&d1_projection, "resolver")?,
    });
    let build_provenance = json!({
        "cargo_lock_sha256": resolved_input(&resolved, "cargo_lock")?.sha256,
        "rust_toolchain_sha256": resolved_input(&resolved, "rust_toolchain")?.sha256,
        "frontend_lock_sha256": resolved_input(&resolved, "frontend_lock")?.sha256,
        "release_architecture_sha256": resolved_input(&resolved, "release_architecture_authority")?.sha256,
    });
    Ok(StaticCompatibilityFields {
        contracts,
        protocols,
        schemas,
        runtime_compatibility,
        build_provenance,
    })
}

fn component(component_id: &str, release_id: &str, path: &str, digest: &str, size: u64) -> Value {
    json!({
        "component_id": component_id,
        "release_id": release_id,
        "source_commit_sha": GIT_SHA,
        "artifact_path": path,
        "artifact_sha256": digest,
        "artifact_size_bytes": size,
        "component_manifest_sha256": SHA_A
    })
}

fn release_set() -> Result<LoadedReleaseSet, Box<dyn std::error::Error>> {
    let evidence = accepted_main_evidence()?;
    let StaticCompatibilityFields {
        contracts,
        protocols,
        schemas,
        runtime_compatibility,
        build_provenance,
    } = static_compatibility_fields()?;
    let value = json!({
        "schema_version": 3,
        "source": {
            "repository": REPOSITORY,
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": evidence
        },
        "components": {
            "control_plane": component("control_plane", "control-plane-v2", "components/control-plane.tar", SHA_A, 10),
            "frontend": component("frontend", "control-plane-v2", "components/control-plane.tar", SHA_A, 10),
            "secret_resolver": component("secret_resolver", "resolver-v2", "components/secret-resolver.tar", SHA_B, 11),
            "runtime_bundle": component("runtime_bundle", "runtime-v2", "components/runtime-bundle.tar", SHA_C, 12),
            "profile_bridge": component("profile_bridge", "bridge-v2", "components/profile-bridge.zip", SHA_D, 13)
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
    let dto: ReleaseSetV3Dto = serde_json::from_value(value)?;
    let semantic = dto.into_core()?;
    let rendered = render_release_set_v3(&semantic)?;
    Ok(LoadedReleaseSet::parse(&rendered.canonical_document_bytes)?)
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
    release: &LoadedReleaseSet,
) -> Result<DeploymentSnapshot, Box<dyn std::error::Error>> {
    let closure = load_closure(&repo_root(), profile_id)?;
    let semantic = release.semantic();
    let component_release_ids = semantic
        .components
        .iter()
        .map(|(id, component)| (id.clone(), component.release_id.clone()))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect();
    Ok(DeploymentSnapshot {
        environment: environment.to_owned(),
        collected_at: "2026-08-21T00:00:00Z".to_owned(),
        release_set_id: Some(release.release_set_id().to_owned()),
        capability_profile_id: Some(profile_id.to_owned()),
        component_release_ids,
        logical_resources: closure.required_resources,
        logical_bindings: closure.required_bindings,
        logical_credentials: closure.required_credentials,
        catalog_ledger_sha256: Some(SHA_A.to_owned()),
        catalog_schema_revision: Some(semantic.schemas.catalog.target_schema_revision.clone()),
        resolver_ledger_sha256: None,
        resolver_schema_revision: None,
        contracts_sha256: Some(semantic.contracts.sha256.clone()),
        resolver_protocol: Some(semantic.protocols.resolver_protocol.clone()),
        camouhost_ipc_version: Some(semantic.protocols.camouhost_ipc_version),
        profile_bridge_protocol_version: Some(semantic.protocols.profile_bridge_protocol_version),
        runtime_role: Some(semantic.runtime_compatibility.runtime_role.clone()),
        profile_format: Some(semantic.runtime_compatibility.profile_format.clone()),
        browser_identity_policy: Some(
            semantic
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
    let compatibility = evidence(target.release_set_id(), "UNKNOWN")?;

    let plan = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        expected_current_release_set_id: Some(target.release_set_id()),
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
        expected_current_release_set_id: Some(target.release_set_id()),
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
    let compatibility = evidence(target.release_set_id(), "COMPATIBLE")?;
    let plan = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "production-core-v1",
        environment: "production",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        expected_current_release_set_id: Some(target.release_set_id()),
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
    let compatibility = evidence(target.release_set_id(), "UNKNOWN")?;
    let result = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        expected_current_release_set_id: Some("release-set-v3-sha256-stale"),
    });
    assert!(result.is_err());
    Ok(())
}

fn legacy_baseline_snapshot(
    target: &LoadedReleaseSet,
) -> Result<DeploymentSnapshot, Box<dyn std::error::Error>> {
    let mut snapshot = converged_snapshot("staging", "rehearsal-core-v1", target)?;
    snapshot.release_set_id = None;
    snapshot.capability_profile_id = None;
    snapshot.component_release_ids.clear();
    snapshot.contracts_sha256 = None;
    snapshot.resolver_protocol = None;
    snapshot.camouhost_ipc_version = None;
    snapshot.profile_bridge_protocol_version = None;
    snapshot.runtime_role = None;
    snapshot.profile_format = None;
    snapshot.browser_identity_policy = None;
    Ok(snapshot)
}

fn legacy_baseline_observation() -> BaselineAdoptionObservation {
    BaselineAdoptionObservation {
        environment: "staging".to_owned(),
        account_id: "4426df1449e417511bc7697d60b7f62f".to_owned(),
        worker_name: "browser-profile-control-plane-staging".to_owned(),
        current_identity: "UNKNOWN".to_owned(),
        deployment_id: "854b371e-69b7-4002-961a-5e9f785fb3f2".to_owned(),
        deployment_version_id: "6ab13548-0b74-487b-a8cf-bc00f0858b36".to_owned(),
        deployment_percentage: 100,
        rollback_version_id: Some("6ab13548-0b74-487b-a8cf-bc00f0858b36".to_owned()),
        rollback_version_available: true,
    }
}

#[test]
fn baseline_adoption_requires_unknown_legacy_identity_and_all_fences()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let target = release_set()?;
    let snapshot = legacy_baseline_snapshot(&target)?;
    let compatible = evidence(target.release_set_id(), "UNKNOWN")?;
    let observation = legacy_baseline_observation();
    let request_id = "ADOPT-20260825";
    let confirmation = format!(
        "{}:{}:{}:{}",
        target.release_set_id(),
        observation.deployment_id,
        observation.deployment_version_id,
        request_id
    );
    let result = baseline_adoption(BaselineAdoptionRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatible,
        observation: &observation,
        expected_account_id: &observation.account_id,
        expected_deployment_id: &observation.deployment_id,
        expected_version_id: &observation.deployment_version_id,
        request_id,
        confirmation: &confirmation,
    })?;
    assert!(result.ready, "baseline blockers: {:?}", result.blockers);
    assert_eq!(result.rollback_compatibility, "COMPATIBLE");
    let machine = result.machine_json(target.release_set_id(), "rehearsal-core-v1", &observation);
    assert_eq!(machine["credential_values_accessed"], false);
    assert_eq!(machine["provider_mutation_executed"], false);

    let stale = baseline_adoption(BaselineAdoptionRequest {
        expected_deployment_id: "stale-deployment",
        ..BaselineAdoptionRequest {
            root: &root,
            source_root: &root,
            target: &target,
            target_profile_id: "rehearsal-core-v1",
            environment: "staging",
            snapshot: &snapshot,
            compatibility_evidence: &compatible,
            observation: &observation,
            expected_account_id: &observation.account_id,
            expected_deployment_id: &observation.deployment_id,
            expected_version_id: &observation.deployment_version_id,
            request_id,
            confirmation: &confirmation,
        }
    })?;
    assert!(
        stale
            .blockers
            .contains(&"STALE_DEPLOYMENT_OR_VERSION_FENCE".to_owned())
    );

    let wrong_account = baseline_adoption(BaselineAdoptionRequest {
        expected_account_id: "wrong-account",
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatible,
        observation: &observation,
        expected_deployment_id: &observation.deployment_id,
        expected_version_id: &observation.deployment_version_id,
        request_id,
        confirmation: &confirmation,
    })?;
    assert!(
        wrong_account
            .blockers
            .contains(&"ACCOUNT_IDENTITY_MISMATCH".to_owned())
    );

    let mut unavailable = observation.clone();
    unavailable.rollback_version_available = false;
    unavailable.rollback_version_id = None;
    let missing_rollback = baseline_adoption(BaselineAdoptionRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatible,
        observation: &unavailable,
        expected_account_id: &unavailable.account_id,
        expected_deployment_id: &unavailable.deployment_id,
        expected_version_id: &unavailable.deployment_version_id,
        request_id,
        confirmation: &confirmation,
    })?;
    assert!(
        missing_rollback
            .blockers
            .contains(&"ROLLBACK_VERSION_UNAVAILABLE".to_owned())
    );
    assert_eq!(missing_rollback.rollback_compatibility, "UNKNOWN");

    let production = baseline_adoption(BaselineAdoptionRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "production",
        snapshot: &snapshot,
        compatibility_evidence: &compatible,
        observation: &observation,
        expected_account_id: &observation.account_id,
        expected_deployment_id: &observation.deployment_id,
        expected_version_id: &observation.deployment_version_id,
        request_id,
        confirmation: &confirmation,
    })?;
    assert!(
        production
            .blockers
            .contains(&"BASELINE_ADOPTION_STAGING_ONLY".to_owned())
    );
    Ok(())
}

#[test]
fn baseline_adoption_rejects_none_and_unknown_target_compatibility()
-> Result<(), Box<dyn std::error::Error>> {
    let none_identity = json!({
        "schema_version": 1,
        "kind": "STAGING_BASELINE_ADOPTION_OBSERVATION",
        "environment": "staging",
        "account_id": "account",
        "worker_name": "worker",
        "current_identity": "NONE",
        "deployment_id": "deployment",
        "deployment_version_id": "version",
        "deployment_percentage": 100,
        "rollback_version_id": "version",
        "rollback_version_available": true
    });
    assert!(BaselineAdoptionObservation::parse_json(&none_identity.to_string()).is_err());

    let root = repo_root();
    let target = release_set()?;
    let snapshot = legacy_baseline_snapshot(&target)?;
    let unknown = evidence(target.release_set_id(), "UNKNOWN")?;
    let observation = legacy_baseline_observation();
    let request_id = "ADOPT-20260825";
    let confirmation = format!(
        "{}:{}:{}:{}",
        target.release_set_id(),
        observation.deployment_id,
        observation.deployment_version_id,
        request_id
    );
    let result = baseline_adoption(BaselineAdoptionRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &unknown,
        observation: &observation,
        expected_account_id: &observation.account_id,
        expected_deployment_id: &observation.deployment_id,
        expected_version_id: &observation.deployment_version_id,
        request_id,
        confirmation: &confirmation,
    })?;
    assert!(result.ready, "windows evidence is outside staging closure");

    let unknown_catalog = CompatibilityEvidence::parse_json(&json!({
        "schema_version": 2,
        "kind": "RELEASE_COMPATIBILITY_EVIDENCE",
        "release_set_id": target.release_set_id(),
        "dimensions": {
            "catalog_d1": {"decision":"UNKNOWN","evidence_sha256":SHA_A,"policy_source":"opsctl.d1.compatibility"},
            "resolver_d1": {"decision":"COMPATIBLE","evidence_sha256":SHA_A,"policy_source":"opsctl.d1.compatibility"},
            "windows_profile_bridge": {"decision":"UNKNOWN","evidence_sha256":SHA_A,"policy_source":"external.windows.delivery"}
        }
    }).to_string())?;
    let blocked = baseline_adoption(BaselineAdoptionRequest {
        compatibility_evidence: &unknown_catalog,
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v1",
        environment: "staging",
        snapshot: &snapshot,
        observation: &observation,
        expected_account_id: &observation.account_id,
        expected_deployment_id: &observation.deployment_id,
        expected_version_id: &observation.deployment_version_id,
        request_id,
        confirmation: &confirmation,
    })?;
    assert!(
        blocked
            .blockers
            .contains(&"SCHEMA_COMPATIBILITY_UNKNOWN".to_owned())
    );
    Ok(())
}
