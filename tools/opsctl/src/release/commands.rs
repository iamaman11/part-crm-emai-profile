use crate::release::ReleaseAction;
use crate::release::artifact::verify_artifacts;
use crate::release::compatibility::{CompatibilityEvidence, evaluate};
use crate::release::document::LoadedReleaseSet;
use crate::release::input_topology::ReleaseInputTopology;
use crate::release::model::ReleaseModelError;
use crate::release::source::{AcceptedSourceVerification, verify_release_source};
use crate::release::static_compatibility::{self, VERIFIED_PROVENANCE_DIMENSIONS};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

pub struct ReleaseRunRequest<'a> {
    pub root: &'a Path,
    pub source_root: &'a Path,
    pub action: ReleaseAction,
    pub release_set: &'a Path,
    pub artifact_root: Option<&'a Path>,
    pub profile_id: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub evidence_json: Option<&'a Path>,
    pub current_release_set: Option<&'a Path>,
}

pub fn run(request: ReleaseRunRequest<'_>) -> Result<String, ReleaseModelError> {
    let release_set = LoadedReleaseSet::load(request.release_set)?;
    let value = match request.action {
        ReleaseAction::Inspect => {
            let topology = ReleaseInputTopology::load(request.root)?;
            let resolved = topology.resolve(request.root)?;
            inspect(&release_set, resolved.len())
        }
        ReleaseAction::Verify => {
            let artifact_root = request
                .artifact_root
                .ok_or_else(|| ReleaseModelError::new("release verify requires --artifact-root"))?;
            let (_, source_verification) =
                verify_release_source(request.release_set, &release_set)?;
            let static_blockers =
                static_compatibility::evaluate(request.source_root, release_set.semantic(), false)?;
            if !static_blockers.is_empty() {
                return Err(ReleaseModelError::new(format!(
                    "RELEASE_STATIC_IDENTITY_MISMATCH: {}",
                    static_blockers.join(",")
                )));
            }
            verify(&release_set, artifact_root, &source_verification)?
        }
        ReleaseAction::Compatibility => {
            let profile_id = required_text(request.profile_id, "--profile")?;
            let environment = required_text(request.environment, "--environment")?;
            let evidence_path = request.evidence_json.ok_or_else(|| {
                ReleaseModelError::new("release compatibility requires --evidence-json")
            })?;
            let evidence = CompatibilityEvidence::load(evidence_path)?;
            let current = request
                .current_release_set
                .map(LoadedReleaseSet::load)
                .transpose()?;
            evaluate(
                request.root,
                request.source_root,
                &release_set,
                &evidence,
                profile_id,
                environment,
                current.as_ref(),
            )?
            .machine_json(release_set.release_set_id(), profile_id, environment)
        }
    };
    serde_json::to_string_pretty(&value)
        .map(|output| format!("{output}\n"))
        .map_err(|error| {
            ReleaseModelError::new(format!("cannot serialize release output: {error}"))
        })
}

fn inspect(release_set: &LoadedReleaseSet, release_input_count: usize) -> serde_json::Value {
    let manifest = release_set.semantic();
    let components = manifest
        .components
        .values()
        .map(|component| {
            json!({
                "component_id": component.component_id,
                "release_id": component.release_id,
                "artifact_sha256": component.artifact_sha256,
                "artifact_size_bytes": component.artifact_size_bytes,
                "component_manifest_sha256": component.component_manifest_sha256,
            })
        })
        .collect::<Vec<_>>();
    let component_release_ids = manifest
        .components
        .iter()
        .map(|(component_id, component)| (component_id.clone(), component.release_id.clone()))
        .collect::<BTreeMap<_, _>>();
    json!({
        "schema_version": 1,
        "command": "release.inspect",
        "decision": "VALID",
        "release_set_schema_version": release_set.external_schema_version(),
        "release_set_id": release_set.release_set_id(),
        "display_version": release_set.display_version(),
        "source": {
            "repository": manifest.source.repository,
            "commit_sha": manifest.source.commit_sha,
            "accepted_main": manifest.source.accepted_main,
            "accepted_main_evidence_sha256": manifest.source.accepted_main_evidence_sha256,
            "accepted_main_evidence_role": "IDENTITY_BINDING_ONLY; AcceptedSourceEvidence is acceptance authority"
        },
        "components": components,
        "component_release_ids": component_release_ids,
        "compatibility_identity": {
            "contracts_sha256": manifest.contracts.sha256,
            "resolver_protocol": manifest.protocols.resolver_protocol,
            "camouhost_ipc_version": manifest.protocols.camouhost_ipc_version,
            "profile_bridge_protocol_version": manifest.protocols.profile_bridge_protocol_version,
            "runtime_role": manifest.runtime_compatibility.runtime_role,
            "profile_format": manifest.runtime_compatibility.profile_format,
            "browser_identity_policy": manifest.runtime_compatibility.browser_identity_policy,
        },
        "capability_profile_compatibility": manifest.capability_profile_compatibility,
        "artifact_count": manifest.artifact_inventory.len(),
        "release_input_count": release_input_count,
        "mutation_executed": false
    })
}

fn verify(
    release_set: &LoadedReleaseSet,
    artifact_root: &Path,
    source: &AcceptedSourceVerification,
) -> Result<serde_json::Value, ReleaseModelError> {
    let artifacts = verify_artifacts(release_set, artifact_root)?;
    let manifest = release_set.semantic();
    Ok(json!({
        "schema_version": 1,
        "command": "release.verify",
        "decision": "VALID",
        "release_set_schema_version": release_set.external_schema_version(),
        "release_set_id": release_set.release_set_id(),
        "source_commit_sha": manifest.source.commit_sha,
        "source_accepted": true,
        "accepted_source_evidence_sha256": source.evidence_sha256,
        "observed_protected_main_sha": source.observed_protected_main_sha,
        "source_lineage_status": source.lineage_status,
        "verified_components": artifacts.verified_components,
        "verified_provenance_dimensions": VERIFIED_PROVENANCE_DIMENSIONS,
        "verified_files": artifacts.verified_files,
        "verified_bytes": artifacts.verified_bytes,
        "mutation_executed": false
    }))
}

fn required_text<'a>(value: Option<&'a str>, flag: &str) -> Result<&'a str, ReleaseModelError> {
    let value = value.ok_or_else(|| ReleaseModelError::new(format!("missing required {flag}")))?;
    if value.trim().is_empty() {
        return Err(ReleaseModelError::new(format!("{flag} must not be empty")));
    }
    Ok(value)
}
