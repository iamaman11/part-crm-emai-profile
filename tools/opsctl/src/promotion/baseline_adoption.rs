//! Bounded, staging-only adoption gate for one legacy Worker deployment.
//!
//! This is deliberately not a promotion-plan escape hatch.  A live deployment
//! without canonical Release Set metadata remains `UNKNOWN`; this gate only
//! permits replacing that explicitly fenced deployment with a verified target.

use crate::promotion::authority::load_closure;
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::release::compatibility::{CompatibilityEvidence, evaluate as compatibility_evaluate};
use crate::release::document::LoadedReleaseSet;
use crate::release::model::ReleaseModelError;
use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;

const SCHEMA_VERSION: u64 = 1;
const KIND: &str = "STAGING_BASELINE_ADOPTION_OBSERVATION";
const DEPLOY_OWNED_RESOURCES: [&str; 4] = [
    "control_plane_worker",
    "profile_coordinator",
    "notification_hub",
    "control_plane_schedule",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineAdoptionObservation {
    pub environment: String,
    pub account_id: String,
    pub worker_name: String,
    pub current_identity: String,
    pub deployment_id: String,
    pub deployment_version_id: String,
    pub deployment_percentage: u64,
    pub rollback_version_id: Option<String>,
    pub rollback_version_available: bool,
}

impl BaselineAdoptionObservation {
    pub fn load(path: &Path) -> Result<Self, ReleaseModelError> {
        let input = fs::read_to_string(path).map_err(|error| {
            ReleaseModelError::new(format!(
                "BASELINE_ADOPTION_OBSERVATION_UNAVAILABLE: {}: {error}",
                path.display()
            ))
        })?;
        Self::parse_json(&input)
    }

    pub fn parse_json(input: &str) -> Result<Self, ReleaseModelError> {
        let value: Value = serde_json::from_str(input).map_err(|error| {
            ReleaseModelError::new(format!(
                "invalid baseline adoption observation JSON: {error}"
            ))
        })?;
        let root = object(&value, "baseline adoption observation")?;
        reject_unknown_fields(
            root,
            &[
                "schema_version",
                "kind",
                "environment",
                "account_id",
                "worker_name",
                "current_identity",
                "deployment_id",
                "deployment_version_id",
                "deployment_percentage",
                "rollback_version_id",
                "rollback_version_available",
            ],
        )?;
        if required_u64(root, "schema_version")? != SCHEMA_VERSION
            || required_string(root, "kind")? != KIND
        {
            return Err(ReleaseModelError::new(
                "unsupported baseline adoption observation identity/version",
            ));
        }
        let current_identity = required_string(root, "current_identity")?;
        if current_identity != "UNKNOWN" {
            return Err(ReleaseModelError::new(
                "baseline adoption accepts only an explicitly UNKNOWN live deployment; NONE is not a live baseline",
            ));
        }
        let observation = Self {
            environment: required_string(root, "environment")?,
            account_id: required_string(root, "account_id")?,
            worker_name: required_string(root, "worker_name")?,
            current_identity,
            deployment_id: required_string(root, "deployment_id")?,
            deployment_version_id: required_string(root, "deployment_version_id")?,
            deployment_percentage: required_u64(root, "deployment_percentage")?,
            rollback_version_id: optional_string(root, "rollback_version_id")?,
            rollback_version_available: required_bool(root, "rollback_version_available")?,
        };
        for (field, value) in [
            ("environment", observation.environment.as_str()),
            ("account_id", observation.account_id.as_str()),
            ("worker_name", observation.worker_name.as_str()),
            ("deployment_id", observation.deployment_id.as_str()),
            (
                "deployment_version_id",
                observation.deployment_version_id.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(ReleaseModelError::new(format!(
                    "baseline adoption observation {field} must not be empty"
                )));
            }
        }
        if observation
            .rollback_version_id
            .as_deref()
            .is_some_and(str::is_empty)
        {
            return Err(ReleaseModelError::new(
                "baseline adoption observation rollback_version_id must not be empty",
            ));
        }
        Ok(observation)
    }
}

pub struct BaselineAdoptionRequest<'a> {
    pub root: &'a Path,
    pub source_root: &'a Path,
    pub target: &'a LoadedReleaseSet,
    pub target_profile_id: &'a str,
    pub environment: &'a str,
    pub snapshot: &'a DeploymentSnapshot,
    pub compatibility_evidence: &'a CompatibilityEvidence,
    pub observation: &'a BaselineAdoptionObservation,
    pub expected_account_id: &'a str,
    pub expected_deployment_id: &'a str,
    pub expected_version_id: &'a str,
    pub request_id: &'a str,
    pub confirmation: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineAdoptionResult {
    pub ready: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub required_steps: Vec<String>,
    pub rollback_compatibility: String,
}

impl BaselineAdoptionResult {
    #[must_use]
    pub fn machine_json(
        &self,
        target_release_set_id: &str,
        target_profile_id: &str,
        observation: &BaselineAdoptionObservation,
    ) -> Value {
        json!({
            "schema_version": 1,
            "command": "promotion.baseline-adoption-preflight",
            "decision": if self.ready { "READY" } else { "BLOCKED" },
            "ready": self.ready,
            "environment": "staging",
            "target_release_set_id": target_release_set_id,
            "target_capability_profile_id": target_profile_id,
            "observed_current_identity": observation.current_identity,
            "observed_deployment_id": observation.deployment_id,
            "observed_version_id": observation.deployment_version_id,
            "rollback_version_id": observation.rollback_version_id,
            "rollback_compatibility": self.rollback_compatibility,
            "blockers": self.blockers,
            "warnings": self.warnings,
            "required_steps": self.required_steps,
            "credential_values_accessed": false,
            "provider_mutation_executed": false,
            "mutation_executed": false
        })
    }
}

pub fn evaluate(
    request: BaselineAdoptionRequest<'_>,
) -> Result<BaselineAdoptionResult, ReleaseModelError> {
    // This is a bounded staging remediation, not a reusable cross-environment
    // transition. Refuse other targets before consulting profile policy.
    if request.environment != "staging" || request.observation.environment != "staging" {
        return Ok(BaselineAdoptionResult {
            ready: false,
            blockers: vec!["BASELINE_ADOPTION_STAGING_ONLY".to_owned()],
            warnings: Vec::new(),
            required_steps: vec!["select the fixed staging baseline target".to_owned()],
            rollback_compatibility: "UNKNOWN".to_owned(),
        });
    }
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut required_steps = Vec::new();

    if request.observation.current_identity != "UNKNOWN" {
        blockers.push("LEGACY_IDENTITY_MUST_REMAIN_UNKNOWN".to_owned());
    }
    if request.observation.account_id != request.expected_account_id {
        blockers.push("ACCOUNT_IDENTITY_MISMATCH".to_owned());
    }
    if request.observation.deployment_id != request.expected_deployment_id
        || request.observation.deployment_version_id != request.expected_version_id
    {
        blockers.push("STALE_DEPLOYMENT_OR_VERSION_FENCE".to_owned());
    }
    if request.observation.deployment_percentage != 100 {
        blockers.push("CURRENT_DEPLOYMENT_NOT_FULLY_ROUTED".to_owned());
    }
    if !request.observation.rollback_version_available
        || request.observation.rollback_version_id.as_deref()
            != Some(request.observation.deployment_version_id.as_str())
    {
        blockers.push("ROLLBACK_VERSION_UNAVAILABLE".to_owned());
    }
    if request.snapshot.environment != "staging"
        || request.snapshot.release_set_id.is_some()
        || request.snapshot.capability_profile_id.is_some()
        || !request.snapshot.component_release_ids.is_empty()
    {
        blockers.push("LEGACY_BASELINE_OBSERVATION_INVALID".to_owned());
    }

    let closure = load_closure(request.root, request.target_profile_id)?;
    let compatibility = compatibility_evaluate(
        request.root,
        request.source_root,
        request.target,
        request.compatibility_evidence,
        request.target_profile_id,
        request.environment,
        None,
    )?;
    blockers.extend(compatibility.blockers);
    required_steps.extend(compatibility.required_steps);
    warnings.extend(compatibility.warnings);

    let missing_resources = closure
        .required_resources
        .difference(&request.snapshot.logical_resources)
        .filter(|resource| !DEPLOY_OWNED_RESOURCES.contains(&resource.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_resources.is_empty() {
        blockers.push("REQUIRED_RESOURCES_NOT_READY".to_owned());
        required_steps.push(format!(
            "collect/prepare required external resources: {}",
            missing_resources.join(",")
        ));
    }
    let missing_bindings = closure
        .required_bindings
        .difference(&request.snapshot.logical_bindings)
        .cloned()
        .collect::<Vec<_>>();
    if !missing_bindings.is_empty() {
        blockers.push("REQUIRED_BINDINGS_NOT_READY".to_owned());
    }
    let missing_credentials = closure
        .required_credentials
        .difference(&request.snapshot.logical_credentials)
        .cloned()
        .collect::<Vec<_>>();
    if !missing_credentials.is_empty() {
        blockers.push("REQUIRED_CREDENTIAL_METADATA_NOT_READY".to_owned());
    }
    if request.snapshot.catalog_ledger_sha256.is_none()
        || request.snapshot.catalog_schema_revision.is_none()
    {
        blockers.push("SCHEMA_COMPATIBILITY_UNKNOWN".to_owned());
    }
    if closure.required_resources.contains("resolver_d1")
        && (request.snapshot.resolver_ledger_sha256.is_none()
            || request.snapshot.resolver_schema_revision.is_none())
    {
        blockers.push("SCHEMA_COMPATIBILITY_UNKNOWN".to_owned());
    }

    let expected_confirmation = format!(
        "{}:{}:{}:{}",
        request.target.release_set_id(),
        request.expected_deployment_id,
        request.expected_version_id,
        request.request_id
    );
    if request.request_id.trim().is_empty() || request.confirmation != expected_confirmation {
        blockers.push("ADOPTION_CONFIRMATION_INVALID".to_owned());
    }

    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();
    required_steps.sort();
    required_steps.dedup();
    let rollback_compatibility = if blockers.iter().any(|blocker| {
        matches!(
            blocker.as_str(),
            "ROLLBACK_VERSION_UNAVAILABLE" | "STALE_DEPLOYMENT_OR_VERSION_FENCE"
        )
    }) {
        "UNKNOWN"
    } else if compatibility.compatible {
        "COMPATIBLE"
    } else {
        "INCOMPATIBLE"
    }
    .to_owned();
    Ok(BaselineAdoptionResult {
        ready: blockers.is_empty(),
        blockers,
        warnings,
        required_steps,
        rollback_compatibility,
    })
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object.get(key).ok_or_else(|| {
        ReleaseModelError::new(format!("missing required baseline adoption field: {key}"))
    })
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ReleaseModelError::new(format!("baseline adoption field {key} must be a string"))
        })
}

fn optional_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ReleaseModelError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| {
                ReleaseModelError::new(format!(
                    "baseline adoption field {key} must be a string or null"
                ))
            }),
    }
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?.as_u64().ok_or_else(|| {
        ReleaseModelError::new(format!(
            "baseline adoption field {key} must be an unsigned integer"
        ))
    })
}

fn required_bool(object: &Map<String, Value>, key: &str) -> Result<bool, ReleaseModelError> {
    required(object, key)?.as_bool().ok_or_else(|| {
        ReleaseModelError::new(format!("baseline adoption field {key} must be a boolean"))
    })
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be an object")))
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), ReleaseModelError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "unknown baseline adoption observation field: {key}"
            )));
        }
    }
    Ok(())
}
