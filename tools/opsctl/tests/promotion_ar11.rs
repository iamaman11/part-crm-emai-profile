use opsctl::promotion::authority::load_closure;
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
        "capability_profile_compatibility": ["rehearsal-core-v2", "production-core-v2"],
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
    let snapshot = converged_snapshot("staging", "rehearsal-core-v2", &target)?;
    let compatibility = evidence(target.release_set_id(), "UNKNOWN")?;

    let plan = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v2",
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
        target_profile_id: "rehearsal-core-v2",
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
    let snapshot = converged_snapshot("production", "production-core-v2", &target)?;
    let compatibility = evidence(target.release_set_id(), "COMPATIBLE")?;
    let plan = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "production-core-v2",
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
    let snapshot = converged_snapshot("staging", "rehearsal-core-v2", &target)?;
    let compatibility = evidence(target.release_set_id(), "UNKNOWN")?;
    let result = build(PlanRequest {
        root: &root,
        source_root: &root,
        target: &target,
        target_profile_id: "rehearsal-core-v2",
        environment: "staging",
        snapshot: &snapshot,
        compatibility_evidence: &compatibility,
        current_release: Some(&target),
        expected_current_release_set_id: Some("release-set-v3-sha256-stale"),
    });
    assert!(result.is_err());
    Ok(())
}

#[test]
fn real_d1_verdict_survives_release_projection_and_terminal_promotion()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let target = release_set()?;
    let projection: Value = serde_json::from_str(&opsctl::d1::repository_projection(&root)?)?;
    let catalog = projection["components"]
        .as_array()
        .ok_or("components missing")?
        .iter()
        .find(|item| item["component_id"] == "catalog")
        .ok_or("catalog missing")?;
    let sources = catalog["executable_migration_sources"]
        .as_array()
        .ok_or("migration sources missing")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let scratch = std::env::temp_dir().join(format!(
        "opsctl-promotion-d1-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&scratch)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let manifest = scratch.join("release.json");
        let ledger = scratch.join("ledger.json");
        let native_path = scratch.join("d1.json");
        fs::write(
            &manifest,
            serde_json::to_vec(&json!({"schema_contract": d1_schema(&projection, "catalog")?}))?,
        )?;
        for (revision, expected) in [
            ("0034_device_application_authority.sql", false),
            ("0036_profile_generation_initialization.sql", true),
        ] {
            let count = sources
                .iter()
                .position(|source| source["migration_file"] == revision)
                .ok_or("revision missing")?
                + 1;
            let rows = sources[..count]
                .iter()
                .map(|source| json!({"name":source["migration_file"]}))
                .collect::<Vec<_>>();
            fs::write(&ledger, serde_json::to_vec(&json!({"rows":rows}))?)?;
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_opsctl"))
                .arg("--root")
                .arg(&root)
                .args([
                    "d1",
                    "compatibility",
                    "--component",
                    "catalog",
                    "--ledger-json",
                ])
                .arg(&ledger)
                .arg("--release-manifest")
                .arg(&manifest)
                .output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            fs::write(&native_path, &output.stdout)?;
            let native: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(native["schema_version"], 2);
            assert_eq!(
                native["allowed"], true,
                "migration preparation remains eligible"
            );
            assert_eq!(native["runtime_compatibility"]["compatible"], expected);

            // Use the unchanged real transport/composition functions, never a rewritten policy fixture.
            let output = std::process::Command::new("python")
                .arg("-c").arg(r#"
import json, runpy, sys
from pathlib import Path
root, native, release_id = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
adapter = runpy.run_path(str(root / 'scripts/release-compat-evidence-ar11.py'))
terminal = runpy.run_path(str(root / 'scripts/promotion-operational-outcome-ar11.py'))
values = terminal['fixture_inputs']()
values['d1'] = json.loads(native.read_text(encoding='utf-8'))
print(json.dumps({'evidence': adapter['build'](release_id, native, None, None), 'outcome': terminal['fixture_compose'](values)}))
"#).arg(&root).arg(&native_path).arg(target.release_set_id()).output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let composed: Value = serde_json::from_slice(&output.stdout)?;
            let evidence =
                CompatibilityEvidence::parse_json(&serde_json::to_string(&composed["evidence"])?)?;
            let mut snapshot = converged_snapshot("staging", "rehearsal-core-v2", &target)?;
            snapshot.catalog_schema_revision = Some(revision.to_owned());
            let plan = build(PlanRequest {
                root: &root,
                source_root: &root,
                target: &target,
                target_profile_id: "rehearsal-core-v2",
                environment: "staging",
                snapshot: &snapshot,
                compatibility_evidence: &evidence,
                current_release: Some(&target),
                expected_current_release_set_id: Some(target.release_set_id()),
            })?;
            let readiness = preflight(PreflightRequest {
                root: &root,
                source_root: &root,
                target: &target,
                target_profile_id: "rehearsal-core-v2",
                environment: "staging",
                snapshot: &snapshot,
                compatibility_evidence: &evidence,
                current_release: Some(&target),
                known_good_release: Some(&target),
                expected_current_release_set_id: Some(target.release_set_id()),
            })?;
            assert_eq!(readiness.ready, expected);
            if expected {
                assert_eq!(plan.decision, "NO_CHANGE");
                assert_eq!(composed["outcome"]["status"], "READ_ONLY_READY");
            } else {
                assert_eq!(plan.decision, "BLOCKED");
                assert!(
                    plan.blockers
                        .iter()
                        .any(|code| code == "SCHEMA_INCOMPATIBLE")
                );
                assert_eq!(composed["outcome"]["status"], "BLOCKED");
                assert_eq!(
                    composed["outcome"]["owner_reason_code"],
                    "RUNTIME_SCHEMA_MIGRATION_REQUIRED"
                );
                assert_eq!(composed["outcome"]["owner_diagnostic"], native);
                assert_eq!(
                    native["planned_migrations"],
                    json!([
                        "0035_pas2_payload_fingerprint_contract.sql",
                        "0036_profile_generation_initialization.sql"
                    ])
                );
                assert_eq!(
                    native["planned_migration_contracts"][0]["rollout_order"],
                    "SEPARATE_CONTRACT_RELEASE"
                );
                assert_eq!(
                    native["planned_migration_contracts"][0]["destructive"],
                    true
                );
            }
            assert_eq!(composed["outcome"]["provider_mutation_executed"], false);
            assert_eq!(composed["outcome"]["production_mutation_executed"], false);
        }
        Ok(())
    })();
    fs::remove_dir_all(scratch)?;
    result
}
