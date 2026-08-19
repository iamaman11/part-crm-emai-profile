use crate::release::artifact::verify_artifacts;
use crate::release::compatibility::{CompatibilityEvidence, evaluate};
use crate::release::model::{ReleaseModelError, ReleaseSetManifest};
use crate::release::ReleaseAction;
use serde_json::json;
use std::fs;
use std::path::Path;

pub struct ReleaseRunRequest<'a> {
    pub root: &'a Path,
    pub action: ReleaseAction,
    pub release_set: &'a Path,
    pub artifact_root: Option<&'a Path>,
    pub profile_id: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub evidence_json: Option<&'a Path>,
    pub current_release_set: Option<&'a Path>,
}

pub fn run(request: ReleaseRunRequest<'_>) -> Result<String, ReleaseModelError> {
    let manifest = load_manifest(request.release_set)?;
    let value = match request.action {
        ReleaseAction::Inspect => inspect(&manifest),
        ReleaseAction::Verify => {
            let artifact_root = request.artifact_root.ok_or_else(|| {
                ReleaseModelError::new("release verify requires --artifact-root")
            })?;
            verify(&manifest, artifact_root)?
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
                .map(load_manifest)
                .transpose()?;
            evaluate(
                request.root,
                &manifest,
                &evidence,
                profile_id,
                environment,
                current.as_ref(),
            )?
            .machine_json(&manifest.release_set_id, profile_id, environment)
        }
    };
    serde_json::to_string_pretty(&value)
        .map(|output| format!("{output}\n"))
        .map_err(|error| ReleaseModelError::new(format!("cannot serialize release output: {error}")))
}

fn load_manifest(path: &Path) -> Result<ReleaseSetManifest, ReleaseModelError> {
    let input = fs::read_to_string(path).map_err(|error| {
        ReleaseModelError::new(format!(
            "RELEASE_SET_UNAVAILABLE: {}: {error}",
            path.display()
        ))
    })?;
    ReleaseSetManifest::parse_json(&input)
}

fn inspect(manifest: &ReleaseSetManifest) -> serde_json::Value {
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
    json!({
        "schema_version": 1,
        "command": "release.inspect",
        "decision": "VALID",
        "release_set_id": manifest.release_set_id,
        "display_version": manifest.display_version,
        "source": {
            "repository": manifest.source.repository,
            "commit_sha": manifest.source.commit_sha,
            "accepted_main": manifest.source.accepted_main,
            "accepted_main_evidence_sha256": manifest.source.accepted_main_evidence_sha256,
        },
        "components": components,
        "capability_profile_compatibility": manifest.capability_profile_compatibility,
        "artifact_count": manifest.artifact_inventory.len(),
        "mutation_executed": false
    })
}

fn verify(
    manifest: &ReleaseSetManifest,
    artifact_root: &Path,
) -> Result<serde_json::Value, ReleaseModelError> {
    let artifacts = verify_artifacts(manifest, artifact_root)?;
    Ok(json!({
        "schema_version": 1,
        "command": "release.verify",
        "decision": "VALID",
        "release_set_id": manifest.release_set_id,
        "source_commit_sha": manifest.source.commit_sha,
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
