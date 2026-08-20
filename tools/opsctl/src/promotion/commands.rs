use crate::promotion::PromotionAction;
use crate::promotion::plan::{PlanRequest, build};
use crate::promotion::preflight::{PreflightRequest, evaluate as preflight};
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::promotion::verify::{VerifyRequest, verify};
use crate::release::compatibility::CompatibilityEvidence;
use crate::release::model::{ReleaseModelError, ReleaseSetManifest};
use crate::release::source::verify_release_source;
use std::fs;
use std::path::Path;

pub struct PromotionRunRequest<'a> {
    pub root: &'a Path,
    pub action: PromotionAction,
    pub release_set: &'a Path,
    pub profile_id: &'a str,
    pub environment: &'a str,
    pub snapshot: &'a Path,
    pub evidence_json: &'a Path,
    pub current_release_set: Option<&'a Path>,
    pub known_good_release_set: Option<&'a Path>,
    pub expected_current_release_set_id: Option<&'a str>,
}

pub fn run(request: PromotionRunRequest<'_>) -> Result<String, ReleaseModelError> {
    let target = load_manifest(request.release_set)?;
    verify_release_source(request.release_set, &target)?;
    let snapshot = DeploymentSnapshot::load(request.snapshot)?;
    let evidence = CompatibilityEvidence::load(request.evidence_json)?;
    let current = request.current_release_set.map(load_manifest).transpose()?;
    let known_good = request
        .known_good_release_set
        .map(load_manifest)
        .transpose()?;

    let value = match request.action {
        PromotionAction::Plan => build(PlanRequest {
            root: request.root,
            target: &target,
            target_profile_id: request.profile_id,
            environment: request.environment,
            snapshot: &snapshot,
            compatibility_evidence: &evidence,
            current_release: current.as_ref(),
            expected_current_release_set_id: request.expected_current_release_set_id,
        })?
        .machine_json(
            &target.release_set_id,
            request.profile_id,
            request.environment,
            snapshot.release_set_id.as_deref(),
        ),
        PromotionAction::Preflight => preflight(PreflightRequest {
            root: request.root,
            target: &target,
            target_profile_id: request.profile_id,
            environment: request.environment,
            snapshot: &snapshot,
            compatibility_evidence: &evidence,
            current_release: current.as_ref(),
            known_good_release: known_good.as_ref(),
            expected_current_release_set_id: request.expected_current_release_set_id,
        })?
        .machine_json(
            &target.release_set_id,
            request.profile_id,
            request.environment,
        ),
        PromotionAction::Verify => verify(VerifyRequest {
            root: request.root,
            target: &target,
            target_profile_id: request.profile_id,
            environment: request.environment,
            snapshot: &snapshot,
            compatibility_evidence: &evidence,
        })?
        .machine_json(
            &target.release_set_id,
            request.profile_id,
            request.environment,
        ),
    };

    serde_json::to_string_pretty(&value)
        .map(|output| format!("{output}\n"))
        .map_err(|error| {
            ReleaseModelError::new(format!("cannot serialize promotion output: {error}"))
        })
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