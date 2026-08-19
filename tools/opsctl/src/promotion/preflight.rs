use crate::promotion::plan::{PlanRequest, PromotionPlan, build};
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::release::compatibility::CompatibilityEvidence;
use crate::release::model::{ReleaseModelError, ReleaseSetManifest};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

pub struct PreflightRequest<'a> {
    pub root: &'a Path,
    pub target: &'a ReleaseSetManifest,
    pub target_profile_id: &'a str,
    pub environment: &'a str,
    pub snapshot: &'a DeploymentSnapshot,
    pub compatibility_evidence: &'a CompatibilityEvidence,
    pub current_release: Option<&'a ReleaseSetManifest>,
    pub known_good_release: Option<&'a ReleaseSetManifest>,
    pub expected_current_release_set_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreflightResult {
    pub ready: bool,
    pub promotion_id: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub required_steps: Vec<String>,
}

impl PreflightResult {
    #[must_use]
    pub fn machine_json(
        &self,
        target_release_set_id: &str,
        target_profile_id: &str,
        environment: &str,
    ) -> Value {
        json!({
            "schema_version": 1,
            "command": "promotion.preflight",
            "decision": if self.ready { "READY" } else { "BLOCKED" },
            "ready": self.ready,
            "promotion_id": self.promotion_id,
            "environment": environment,
            "target_release_set_id": target_release_set_id,
            "target_capability_profile_id": target_profile_id,
            "blockers": self.blockers,
            "warnings": self.warnings,
            "required_steps": self.required_steps,
            "credential_values_accessed": false,
            "provider_mutation_executed": false,
            "mutation_executed": false
        })
    }
}

pub fn evaluate(request: PreflightRequest<'_>) -> Result<PreflightResult, ReleaseModelError> {
    let plan = build(PlanRequest {
        root: request.root,
        target: request.target,
        target_profile_id: request.target_profile_id,
        environment: request.environment,
        snapshot: request.snapshot,
        compatibility_evidence: request.compatibility_evidence,
        current_release: request.current_release,
        expected_current_release_set_id: request.expected_current_release_set_id,
    })?;
    preflight_from_plan(request, plan)
}

fn preflight_from_plan(
    request: PreflightRequest<'_>,
    plan: PromotionPlan,
) -> Result<PreflightResult, ReleaseModelError> {
    let mut blockers = plan.blockers.clone();
    let mut warnings = plan.warnings.clone();
    let mut required_steps = plan.compatibility.required_steps.clone();

    let missing_resources = difference(
        &plan.closure.required_resources,
        &request.snapshot.logical_resources,
    );
    let missing_bindings = difference(
        &plan.closure.required_bindings,
        &request.snapshot.logical_bindings,
    );
    let missing_credentials = difference(
        &plan.closure.required_credentials,
        &request.snapshot.logical_credentials,
    );
    if !missing_resources.is_empty() {
        blockers.push("REQUIRED_RESOURCES_NOT_READY".to_owned());
        required_steps.push(format!(
            "provision/identify required resources: {}",
            missing_resources.join(",")
        ));
    }
    if !missing_bindings.is_empty() {
        blockers.push("REQUIRED_BINDINGS_NOT_READY".to_owned());
        required_steps.push(format!(
            "prepare required bindings: {}",
            missing_bindings.join(",")
        ));
    }
    if !missing_credentials.is_empty() {
        blockers.push("REQUIRED_CREDENTIAL_METADATA_NOT_READY".to_owned());
        required_steps.push(format!(
            "prepare credential metadata identities: {}",
            missing_credentials.join(",")
        ));
    }

    match request.known_good_release {
        Some(known_good) => {
            if !known_good
                .capability_profile_compatibility
                .iter()
                .any(|profile| profile == request.target_profile_id)
            {
                blockers.push("ROLLBACK_INCOMPATIBLE".to_owned());
            }
        }
        None => {
            blockers.push("ROLLBACK_CANDIDATE_UNAVAILABLE".to_owned());
        }
    }

    if request.snapshot.catalog_ledger_sha256.is_none() {
        blockers.push("PROVIDER_STATE_UNKNOWN".to_owned());
        required_steps.push("collect Catalog D1 ledger evidence".to_owned());
    }
    if plan.closure.required_resources.contains("resolver_d1")
        && request.snapshot.resolver_ledger_sha256.is_none()
    {
        blockers.push("PROVIDER_STATE_UNKNOWN".to_owned());
        required_steps.push("collect Resolver D1 ledger evidence".to_owned());
    }

    if request.environment == "production" {
        blockers.push("PRODUCTION_EXECUTION_BLOCKED_DURING_AR11".to_owned());
        required_steps
            .push("AR-17 authorization and PC-1 workflow authority are required".to_owned());
    }
    if plan.decision == "NO_CHANGE" {
        warnings.push("target already converged; provider mutation is unnecessary".to_owned());
    }

    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();
    required_steps.sort();
    required_steps.dedup();
    Ok(PreflightResult {
        ready: blockers.is_empty(),
        promotion_id: plan.promotion_id,
        blockers,
        warnings,
        required_steps,
    })
}

fn difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}
