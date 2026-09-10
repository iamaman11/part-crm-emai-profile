use crate::promotion::PromotionAction;
use crate::promotion::plan::{PlanRequest, build};
use crate::promotion::preflight::{PreflightRequest, evaluate as preflight};
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::promotion::verify::{VerifyRequest, verify};
use crate::release::compatibility::CompatibilityEvidence;
use crate::release::document::LoadedReleaseSet;
use crate::release::model::ReleaseModelError;
use crate::release::source::verify_release_source;
use serde_json::{Value, json};
use std::path::Path;

pub struct PromotionRunRequest<'a> {
    pub root: &'a Path,
    pub source_root: &'a Path,
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

const DIRECT_ADMISSION: &str = "DIRECT";
const PREREQUISITE_RELEASE_BRIDGE: &str = "PREREQUISITE_RELEASE_BRIDGE";

fn supports_profile(release: &LoadedReleaseSet, profile_id: &str) -> bool {
    release
        .semantic()
        .capability_profile_compatibility
        .iter()
        .any(|candidate| candidate == profile_id)
}

fn select_admission_profile<'a>(
    requested_profile_id: &'a str,
    target: &'a LoadedReleaseSet,
    current: Option<&LoadedReleaseSet>,
    snapshot: &'a DeploymentSnapshot,
) -> (&'a str, &'static str) {
    let Some(observed_release_set_id) = snapshot.release_set_id.as_deref() else {
        return (requested_profile_id, DIRECT_ADMISSION);
    };
    if observed_release_set_id == target.release_set_id() {
        return (requested_profile_id, DIRECT_ADMISSION);
    }
    let Some(observed_profile_id) = snapshot.capability_profile_id.as_deref() else {
        return (requested_profile_id, DIRECT_ADMISSION);
    };
    if observed_profile_id == requested_profile_id {
        return (requested_profile_id, DIRECT_ADMISSION);
    }
    let Some(current_release) = current else {
        return (requested_profile_id, DIRECT_ADMISSION);
    };

    // When the currently deployed/rollback Release Set cannot run the requested future
    // profile, but the exact target Release Set can run the profile that is actually
    // deployed now, admission is split into two independently fenced operations:
    // first deploy the target bits while retaining the observed profile, then re-run
    // admission for the requested profile against that now-current exact Release Set.
    // This is promotion policy, so it lives here rather than in workflow orchestration.
    if !supports_profile(current_release, requested_profile_id)
        && supports_profile(target, observed_profile_id)
    {
        (observed_profile_id, PREREQUISITE_RELEASE_BRIDGE)
    } else {
        (requested_profile_id, DIRECT_ADMISSION)
    }
}

fn admission_json(
    mut value: Value,
    requested_profile_id: &str,
    effective_profile_id: &str,
    mode: &str,
) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "requested_target_capability_profile_id".to_owned(),
            json!(requested_profile_id),
        );
        object.insert(
            "target_capability_profile_id".to_owned(),
            json!(effective_profile_id),
        );
        object.insert("admission_mode".to_owned(), json!(mode));
    }
    value
}

pub fn run(request: PromotionRunRequest<'_>) -> Result<String, ReleaseModelError> {
    let target = LoadedReleaseSet::load(request.release_set)?;
    verify_release_source(request.release_set, &target)?;
    let snapshot = DeploymentSnapshot::load(request.snapshot)?;
    let evidence = CompatibilityEvidence::load(request.evidence_json)?;
    let current = request
        .current_release_set
        .map(LoadedReleaseSet::load)
        .transpose()?;
    let known_good = request
        .known_good_release_set
        .map(LoadedReleaseSet::load)
        .transpose()?;

    let value = match request.action {
        PromotionAction::Plan => {
            let (effective_profile_id, admission_mode) = select_admission_profile(
                request.profile_id,
                &target,
                current.as_ref(),
                &snapshot,
            );
            admission_json(
                build(PlanRequest {
                    root: request.root,
                    source_root: request.source_root,
                    target: &target,
                    target_profile_id: effective_profile_id,
                    environment: request.environment,
                    snapshot: &snapshot,
                    compatibility_evidence: &evidence,
                    current_release: current.as_ref(),
                    expected_current_release_set_id: request.expected_current_release_set_id,
                })?
                .machine_json(
                    target.release_set_id(),
                    effective_profile_id,
                    request.environment,
                    snapshot.release_set_id.as_deref(),
                ),
                request.profile_id,
                effective_profile_id,
                admission_mode,
            )
        }
        PromotionAction::Preflight => {
            let (effective_profile_id, admission_mode) = select_admission_profile(
                request.profile_id,
                &target,
                current.as_ref(),
                &snapshot,
            );
            admission_json(
                preflight(PreflightRequest {
                    root: request.root,
                    source_root: request.source_root,
                    target: &target,
                    target_profile_id: effective_profile_id,
                    environment: request.environment,
                    snapshot: &snapshot,
                    compatibility_evidence: &evidence,
                    current_release: current.as_ref(),
                    known_good_release: known_good.as_ref(),
                    expected_current_release_set_id: request.expected_current_release_set_id,
                })?
                .machine_json(
                    target.release_set_id(),
                    effective_profile_id,
                    request.environment,
                ),
                request.profile_id,
                effective_profile_id,
                admission_mode,
            )
        }
        PromotionAction::Verify => admission_json(
            verify(VerifyRequest {
                root: request.root,
                target: &target,
                target_profile_id: request.profile_id,
                environment: request.environment,
                snapshot: &snapshot,
                compatibility_evidence: &evidence,
            })?
            .machine_json(
                target.release_set_id(),
                request.profile_id,
                request.environment,
            ),
            request.profile_id,
            request.profile_id,
            DIRECT_ADMISSION,
        ),
    };

    serde_json::to_string_pretty(&value)
        .map(|output| format!("{output}\n"))
        .map_err(|error| {
            ReleaseModelError::new(format!("cannot serialize promotion output: {error}"))
        })
}
