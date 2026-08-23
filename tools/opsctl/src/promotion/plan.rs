use crate::promotion::authority::{DeploymentClosure, load_closure};
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::release::compatibility::{CompatibilityEvidence, CompatibilityResult, evaluate};
use crate::release::digest::sha256_hex;
use crate::release::document::{LoadedReleaseSet, ReleaseCompatibilityView};
use crate::release::model::ReleaseModelError;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub struct PlanRequest<'a> {
    pub root: &'a Path,
    pub source_root: &'a Path,
    pub target: &'a LoadedReleaseSet,
    pub target_profile_id: &'a str,
    pub environment: &'a str,
    pub snapshot: &'a DeploymentSnapshot,
    pub compatibility_evidence: &'a CompatibilityEvidence,
    pub current_release: Option<&'a LoadedReleaseSet>,
    pub expected_current_release_set_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromotionPlan {
    pub promotion_id: String,
    pub decision: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub actions: Vec<Value>,
    pub closure: DeploymentClosure,
    pub compatibility: CompatibilityResult,
}

impl PromotionPlan {
    #[must_use]
    pub fn machine_json(
        &self,
        target_release_set_id: &str,
        target_profile_id: &str,
        environment: &str,
        observed_current_release_set_id: Option<&str>,
    ) -> Value {
        json!({
            "schema_version": 1,
            "command": "promotion.plan",
            "decision": self.decision,
            "promotion_id": self.promotion_id,
            "environment": environment,
            "observed_current_release_set_id": observed_current_release_set_id,
            "target_release_set_id": target_release_set_id,
            "target_capability_profile_id": target_profile_id,
            "blockers": self.blockers,
            "warnings": self.warnings,
            "actions": self.actions,
            "deployment_closure_id": self.closure.closure_id,
            "execution_authorized": false,
            "mutation_executed": false
        })
    }
}

pub fn build(request: PlanRequest<'_>) -> Result<PromotionPlan, ReleaseModelError> {
    if request.snapshot.environment != request.environment {
        return Err(ReleaseModelError::new(format!(
            "PROVIDER_STATE_UNKNOWN: snapshot environment {} does not match target {}",
            request.snapshot.environment, request.environment
        )));
    }
    verify_expected_current(
        request.expected_current_release_set_id,
        request.snapshot.release_set_id.as_deref(),
    )?;

    let closure = load_closure(request.root, request.target_profile_id)?;
    let target = request.target.semantic();
    validate_release_components(target, &closure)?;
    let compatibility = evaluate(
        request.root,
        request.source_root,
        request.target,
        request.compatibility_evidence,
        request.target_profile_id,
        request.environment,
        request.current_release,
    )?;

    let current_id = request.snapshot.release_set_id.as_deref().unwrap_or("NONE");
    let promotion_id = sha256_hex(
        format!(
            "environment={}\ncurrent_release_set={}\ntarget_release_set={}\ntarget_profile={}",
            request.environment,
            current_id,
            request.target.release_set_id(),
            request.target_profile_id
        )
        .as_bytes(),
    );

    let missing_resources = difference(
        &closure.required_resources,
        &request.snapshot.logical_resources,
    );
    let extra_resources = difference(
        &request.snapshot.logical_resources,
        &closure.required_resources,
    );
    let missing_bindings = difference(
        &closure.required_bindings,
        &request.snapshot.logical_bindings,
    );
    let extra_bindings = difference(
        &request.snapshot.logical_bindings,
        &closure.required_bindings,
    );
    let missing_credentials = difference(
        &closure.required_credentials,
        &request.snapshot.logical_credentials,
    );
    let extra_credentials = difference(
        &request.snapshot.logical_credentials,
        &closure.required_credentials,
    );

    let target_components = target
        .components
        .iter()
        .map(|(id, component)| (id.clone(), component.release_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let observed_components = request
        .snapshot
        .component_release_ids
        .iter()
        .cloned()
        .collect::<BTreeMap<_, _>>();
    let release_changed = request.snapshot.release_set_id.as_deref()
        != Some(request.target.release_set_id())
        || target_components != observed_components;
    let profile_changed =
        request.snapshot.capability_profile_id.as_deref() != Some(request.target_profile_id);

    let mut actions = Vec::new();
    if !compatibility.required_steps.is_empty() {
        actions.push(json!({
            "authority": "D1_MIGRATION_EXECUTOR",
            "operation": "APPLY_ACCEPTED_COMPATIBILITY_STEPS",
            "steps": compatibility.required_steps
        }));
    }
    if !missing_resources.is_empty()
        || !extra_resources.is_empty()
        || !missing_bindings.is_empty()
        || !extra_bindings.is_empty()
        || !missing_credentials.is_empty()
        || !extra_credentials.is_empty()
    {
        actions.push(json!({
            "authority": "PROVIDER_RESOURCE",
            "operation": "CONVERGE_DEPLOYMENT_CLOSURE",
            "missing_resources": missing_resources,
            "extra_resources": extra_resources,
            "missing_bindings": missing_bindings,
            "extra_bindings": extra_bindings,
            "missing_credentials": missing_credentials,
            "extra_credentials": extra_credentials
        }));
    }
    if release_changed {
        actions.push(json!({
            "authority": "WRANGLER_DEPLOY",
            "operation": "DEPLOY_EXACT_RELEASE_SET_ARTIFACTS",
            "release_set_id": request.target.release_set_id(),
            "component_release_ids": target_components
        }));
    }
    if profile_changed {
        actions.push(json!({
            "authority": "CAPABILITY_PROFILE_SWITCH",
            "operation": "SELECT_CANONICAL_PROFILE",
            "capability_profile_id": request.target_profile_id
        }));
    }
    if !actions.is_empty() {
        actions.push(json!({
            "authority": "POST_DEPLOY_VERIFY",
            "operation": "COLLECT_SNAPSHOT_AND_VERIFY"
        }));
    }

    let mut blockers = compatibility.blockers.clone();
    if request.environment == "production" {
        blockers.push("PRODUCTION_EXECUTION_BLOCKED_DURING_AR11".to_owned());
    }
    blockers.sort();
    blockers.dedup();
    let mut warnings = compatibility.warnings.clone();
    warnings.sort();
    warnings.dedup();
    let decision = if blockers.is_empty() {
        if actions.is_empty() {
            "NO_CHANGE"
        } else {
            "PLAN"
        }
    } else {
        "BLOCKED"
    }
    .to_owned();

    Ok(PromotionPlan {
        promotion_id,
        decision,
        blockers,
        warnings,
        actions,
        closure,
        compatibility,
    })
}

fn validate_release_components(
    target: &ReleaseCompatibilityView,
    closure: &DeploymentClosure,
) -> Result<(), ReleaseModelError> {
    for component in &closure.required_components {
        if !target.components.contains_key(component) {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_INCOMPLETE: deployment closure requires missing component {component}"
            )));
        }
    }
    Ok(())
}

fn verify_expected_current(
    expected: Option<&str>,
    observed: Option<&str>,
) -> Result<(), ReleaseModelError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let observed = observed.unwrap_or("NONE");
    if expected == observed {
        Ok(())
    } else {
        Err(ReleaseModelError::new(format!(
            "PROMOTION_STALE: expected current {expected}, observed {observed}"
        )))
    }
}

fn difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::verify_expected_current;

    #[test]
    fn stale_expected_current_fails_closed() {
        assert!(verify_expected_current(Some("release-a"), Some("release-c")).is_err());
        assert!(verify_expected_current(Some("NONE"), None).is_ok());
    }
}
