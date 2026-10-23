#[path = "d1/authority.rs"]
mod authority;
#[path = "d1/authorization.rs"]
pub mod authorization;
#[path = "d1/catalog_successor.rs"]
mod catalog;
#[path = "d1/catalog.rs"]
mod catalog_legacy;
#[path = "d1/compatibility.rs"]
mod compatibility;
#[path = "d1/contract_transition.rs"]
mod contract_transition;
#[path = "d1/execution_control.rs"]
pub mod execution_control;
#[path = "d1/execution_post_state.rs"]
mod execution_post_state;
#[path = "d1/executor_admission.rs"]
pub mod executor_admission;
#[path = "d1/model.rs"]
mod model;
#[path = "d1/operator_outcome.rs"]
pub mod operator_outcome;
#[path = "d1/plan.rs"]
mod plan;
#[path = "d1/reconstruction.rs"]
mod reconstruction;
#[path = "d1/rollback_schema_tests.rs"]
mod rollback_schema_tests;
#[path = "d1/runtime_manifest.rs"]
mod runtime_manifest;
#[path = "d1/status.rs"]
mod status;
#[path = "d1/transaction_api.rs"]
pub mod transaction;
#[path = "d1/transaction.rs"]
mod transaction_core;
#[path = "d1/transaction_integrity.rs"]
pub mod transaction_integrity;
#[path = "d1/transaction_policy.rs"]
mod transaction_policy;
#[path = "d1/util.rs"]
mod util;
#[path = "d1/verify.rs"]
mod verify;

use crate::canonical::{canonical_json, sha256_hex};
use authority::{load_preconditions, load_release_contract, load_wrangler_ledger};
use catalog::{component_authority, observed_runtime_context};
use compatibility::RuntimeSchemaContext;
use contract_transition::{CurrentTransitionInput, TransitionEvidence};
use model::{Evaluation, Preconditions, ReleaseSchemaContract};
use plan::evaluate;
use serde_json::{Value, json};
use std::path::Path;
use util::{read_json, resolve_input};

pub use execution_post_state::{
    ExecutionPostStateDisposition, ExecutionPostStateVerification,
    serialize_execution_post_state_verification, verify_execution_post_state,
    verify_migration_ledger,
};
pub use model::{D1Action, D1Error, D1RunRequest};
pub use reconstruction::D1CurrentReconstructionPlanRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct D1ReleaseSchemaIdentity {
    pub database_component: String,
    pub target_schema_revision: String,
    pub supported_schema_min: String,
    pub supported_schema_max: String,
    pub migration_history_digest: String,
    pub compatibility_policy_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum D1RollbackSchemaDecision {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct D1RollbackSchemaVerdict {
    pub decision: D1RollbackSchemaDecision,
    pub reason_code: String,
    pub summary: String,
    pub remediation: String,
}

impl D1RollbackSchemaVerdict {
    fn compatible(reason_code: &str, summary: String) -> Self {
        Self {
            decision: D1RollbackSchemaDecision::Compatible,
            reason_code: reason_code.to_owned(),
            summary,
            remediation: "NONE".to_owned(),
        }
    }

    fn incompatible(observed_revision: &str) -> Self {
        Self {
            decision: D1RollbackSchemaDecision::Incompatible,
            reason_code: "CATALOG_SCHEMA_UNSUPPORTED".to_owned(),
            summary: format!(
                "rollback Release Set is not compatible with observed Catalog D1 schema revision {observed_revision} under the current typed D1 authority"
            ),
            remediation: "select a verified rollback Release Set supported by the current typed D1 authority, or use the separately authorized schema recovery procedure before promotion".to_owned(),
        }
    }

    fn unknown(error: &D1Error) -> Self {
        Self {
            decision: D1RollbackSchemaDecision::Unknown,
            reason_code: "D1_ROLLBACK_SCHEMA_AUTHORITY_UNAVAILABLE".to_owned(),
            summary: format!(
                "typed D1 rollback-schema authority could not evaluate the observed Catalog schema: {error}"
            ),
            remediation: "repair or restore the canonical typed D1 repository authority, then rerun read-only promotion preflight".to_owned(),
        }
    }
}

pub fn run(request: D1RunRequest<'_>) -> Result<String, D1Error> {
    let authority = component_authority(request.root, request.component)?;
    let ledger_path = resolve_input(request.root, request.ledger_json);
    let remote_names = load_wrangler_ledger(&ledger_path)?;
    let target = load_optional_release(
        request.root,
        request.release_manifest,
        request.component,
        request.action.requires_release_manifest(),
        request.action.name(),
    )?;
    let current = load_optional_runtime_context(
        request.root,
        request.current_manifest,
        request.component,
        request.action.name(),
    )?;
    let known_good = load_optional_runtime_context(
        request.root,
        request.known_good_manifest,
        request.component,
        request.action.name(),
    )?;
    if request.action == D1Action::Plan
        && let Some(path) = request.preconditions_json
    {
        let evidence_value = read_json(&resolve_input(request.root, path), "D1 preconditions")?;
        if evidence_value.get("kind").and_then(Value::as_str)
            == Some("D1_CURRENT_TRANSITION_EVIDENCE")
        {
            let evidence: TransitionEvidence = serde_json::from_value(evidence_value)
                .map_err(|e| D1Error::new(format!("CURRENT transition evidence schema: {e}")))?;
            let current_path = request
                .current_manifest
                .ok_or_else(|| D1Error::new("CURRENT transition requires deployed manifest"))?;
            let target_path = request
                .release_manifest
                .ok_or_else(|| D1Error::new("CURRENT transition requires target manifest"))?;
            let current_value = read_json(
                &resolve_input(request.root, current_path),
                "deployed manifest",
            )?;
            let target_value =
                read_json(&resolve_input(request.root, target_path), "target manifest")?;
            let manifest_source = |v: &Value| -> Result<String, D1Error> {
                let source = v.get("source").ok_or_else(|| {
                    D1Error::new(
                        "CURRENT transition requires original accepted-main manifest source",
                    )
                })?;
                if source.get("authority").and_then(Value::as_str) != Some("accepted-main")
                    || source.get("repository").and_then(Value::as_str)
                        != Some("iamaman11/part-crm-emai-profile")
                    || source.get("ref").and_then(Value::as_str) != Some("refs/heads/main")
                    || source.get("event").and_then(Value::as_str) != Some("push")
                {
                    return Err(D1Error::new(
                        "CURRENT transition manifest is not published accepted-main source",
                    ));
                }
                source
                    .get("commit_sha")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| D1Error::new("CURRENT transition manifest lacks source SHA"))
            };
            return contract_transition::evaluate(CurrentTransitionInput {
                authority: &authority,
                remote_names: &remote_names,
                current: current
                    .as_ref()
                    .ok_or_else(|| D1Error::new("missing deployed runtime context"))?,
                target: target
                    .as_ref()
                    .ok_or_else(|| D1Error::new("missing CURRENT target"))?,
                evidence: &evidence,
                current_manifest_sha256: &sha256_hex(
                    canonical_json(&current_value)
                        .map_err(D1Error::new)?
                        .as_bytes(),
                ),
                target_manifest_sha256: &sha256_hex(
                    canonical_json(&target_value)
                        .map_err(D1Error::new)?
                        .as_bytes(),
                ),
                repository_identity_sha256: &catalog::repository_identity_sha256(request.root)?,
                ledger_sha256: &sha256_hex(
                    canonical_json(&json!({"remote_migrations":remote_names}))
                        .map_err(D1Error::new)?
                        .as_bytes(),
                ),
                current_source_sha: &manifest_source(&current_value)?,
                target_source_sha: &manifest_source(&target_value)?,
            });
        }
    }
    let preconditions = match request.preconditions_json {
        Some(path) => load_preconditions(&resolve_input(request.root, path), request.component)?,
        None => Preconditions::default(),
    };

    let evaluation = evaluate(
        request.action,
        &authority,
        &remote_names,
        target.as_ref(),
        current.as_ref(),
        known_good.as_ref(),
        &preconditions,
    )?;
    let runtime_compatibility = if request.action == D1Action::Compatibility {
        let contract = target
            .as_ref()
            .ok_or_else(|| D1Error::new("d1 compatibility requires a release schema contract"))?;
        Some(compatibility::runtime_compatibility(
            &authority,
            contract,
            remote_names.len(),
            &evaluation,
        )?)
    } else {
        None
    };
    serialize_evaluation(
        &authority,
        request.action,
        evaluation,
        runtime_compatibility,
    )
}

pub fn current_reconstruction_plan(
    request: D1CurrentReconstructionPlanRequest<'_>,
) -> Result<String, D1Error> {
    reconstruction::plan(request)
}

pub fn repository_projection(root: &std::path::Path) -> Result<String, D1Error> {
    catalog::repository_projection(root)
}

pub(crate) use catalog::{release_contract, repository_identity_sha256};

pub(crate) fn release_schema_identity(
    root: &Path,
    component: &str,
) -> Result<D1ReleaseSchemaIdentity, D1Error> {
    let projection = release_contract(root, component)?;
    let required = |field: &str| -> Result<String, D1Error> {
        projection
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                D1Error::new(format!(
                    "typed D1 release schema projection is missing {field}"
                ))
            })
    };
    Ok(D1ReleaseSchemaIdentity {
        database_component: required("database_component")?,
        target_schema_revision: required("target_schema_revision")?,
        supported_schema_min: required("supported_schema_min")?,
        supported_schema_max: required("supported_schema_max")?,
        migration_history_digest: required("migration_history_digest")?,
        compatibility_policy_digest: required("compatibility_policy_digest")?,
    })
}

pub(crate) fn rollback_schema_compatibility(
    root: &Path,
    release: &opsctl_core::release::SchemaCompatibilityWindow,
    observed_revision: &str,
) -> D1RollbackSchemaVerdict {
    if release.supports(observed_revision) {
        return D1RollbackSchemaVerdict::compatible(
            "IMMUTABLE_RELEASE_SCHEMA_WINDOW",
            format!(
                "rollback Release Set immutable Catalog schema window supports observed revision {observed_revision}"
            ),
        );
    }

    match historical_catalog_runtime_supports(root, release, observed_revision) {
        Ok(true) => D1RollbackSchemaVerdict::compatible(
            "D1_PRESENT_TIME_ROLLBACK_SCHEMA_COMPATIBLE",
            format!(
                "typed D1 authority proves the immutable historical Catalog runtime is code-rollback safe through observed revision {observed_revision}"
            ),
        ),
        Ok(false) => D1RollbackSchemaVerdict::incompatible(observed_revision),
        Err(error) => D1RollbackSchemaVerdict::unknown(&error),
    }
}

fn historical_catalog_runtime_supports(
    root: &Path,
    release: &opsctl_core::release::SchemaCompatibilityWindow,
    observed_revision: &str,
) -> Result<bool, D1Error> {
    if release.database_component != "catalog" {
        return Ok(false);
    }

    let projection: Value =
        serde_json::from_str(&catalog::repository_projection(root)?).map_err(|error| {
            D1Error::new(format!(
                "cannot parse typed D1 repository projection: {error}"
            ))
        })?;
    let catalog_projection = projection
        .get("components")
        .and_then(Value::as_array)
        .and_then(|components| {
            components.iter().find(|component| {
                component.get("component_id").and_then(Value::as_str) == Some("catalog")
            })
        })
        .ok_or_else(|| {
            D1Error::new("typed D1 repository projection is missing Catalog component")
        })?;
    let historical = catalog_projection
        .get("historical_epoch")
        .ok_or_else(|| D1Error::new("typed D1 Catalog projection is missing historical_epoch"))?;
    let runtime = catalog_projection
        .get("pre_migration_runtime_schema_contract")
        .ok_or_else(|| {
            D1Error::new("typed D1 Catalog projection is missing pre-migration runtime contract")
        })?;

    let required_string = |value: &Value, field: &str, label: &str| -> Result<String, D1Error> {
        value
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| D1Error::new(format!("{label} is missing {field}")))
    };
    let historical_revision = required_string(historical, "final_revision", "historical_epoch")?;
    let historical_digest =
        required_string(historical, "accepted_history_digest", "historical_epoch")?;
    let policy_digest = required_string(
        catalog_projection,
        "compatibility_policy_digest",
        "Catalog projection",
    )?;
    let current_history_digest =
        required_string(catalog_projection, "history_digest", "Catalog projection")?;
    let runtime_target = required_string(
        runtime,
        "target_schema_revision",
        "pre-migration runtime contract",
    )?;
    let runtime_min = required_string(
        runtime,
        "supported_schema_min",
        "pre-migration runtime contract",
    )?;
    let runtime_max = required_string(
        runtime,
        "supported_schema_max",
        "pre-migration runtime contract",
    )?;
    let runtime_history_digest = required_string(
        runtime,
        "migration_history_digest",
        "pre-migration runtime contract",
    )?;
    let runtime_policy_digest = required_string(
        runtime,
        "compatibility_policy_digest",
        "pre-migration runtime contract",
    )?;

    if historical
        .get("retroactive_runtime_compatibility_claims")
        .and_then(Value::as_bool)
        != Some(false)
        || catalog_projection
            .get("legacy_history")
            .and_then(|legacy| legacy.get("immutable"))
            .and_then(Value::as_bool)
            != Some(true)
        || runtime_target != historical_revision
        || runtime_min != historical_revision
        || runtime_history_digest != current_history_digest
        || runtime_policy_digest != policy_digest
    {
        return Err(D1Error::new(
            "typed D1 historical/pre-migration rollback authority is internally inconsistent",
        ));
    }

    let exact_historical_release = release.target_schema_revision == historical_revision
        && release.supported_schema_min == historical_revision
        && release.supported_schema_max == historical_revision
        && release.migration_history_digest == historical_digest
        && release.compatibility_policy_digest == policy_digest;
    if !exact_historical_release {
        return Ok(false);
    }

    let sources = catalog_projection
        .get("executable_migration_sources")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            D1Error::new("typed D1 Catalog projection is missing executable migration sources")
        })?;
    let position = |revision: &str| {
        sources
            .iter()
            .position(|entry| entry.get("migration_file").and_then(Value::as_str) == Some(revision))
    };
    let Some(historical_index) = position(&historical_revision) else {
        return Err(D1Error::new(
            "typed D1 Catalog lineage is missing historical runtime revision",
        ));
    };
    let Some(runtime_max_index) = position(&runtime_max) else {
        return Err(D1Error::new(
            "typed D1 Catalog lineage is missing pre-migration supported maximum",
        ));
    };
    let Some(observed_index) = position(observed_revision) else {
        return Ok(false);
    };

    Ok(observed_index >= historical_index && observed_index <= runtime_max_index)
}

fn load_optional_release(
    root: &Path,
    path: Option<&Path>,
    component: &str,
    required: bool,
    action: &str,
) -> Result<Option<ReleaseSchemaContract>, D1Error> {
    match path {
        Some(value) => {
            let resolved = resolve_input(root, value);
            Ok(Some(load_release_contract(&resolved, component)?))
        }
        None if required => Err(D1Error::new(format!(
            "d1 {action} requires --release-manifest"
        ))),
        None => Ok(None),
    }
}

fn load_optional_runtime_context(
    root: &Path,
    path: Option<&Path>,
    component: &str,
    action: &str,
) -> Result<Option<RuntimeSchemaContext>, D1Error> {
    load_optional_release(root, path, component, false, action)?
        .zip(path)
        .map(|(schema, path)| {
            let manifest = read_json(&resolve_input(root, path), "observed runtime manifest")?;
            observed_runtime_context(root, component, schema, &manifest)
        })
        .transpose()
}

fn serialize_evaluation(
    authority: &model::ComponentAuthority,
    action: D1Action,
    evaluation: Evaluation,
    runtime_compatibility: Option<compatibility::RuntimeCompatibility>,
) -> Result<String, D1Error> {
    let planned_contracts = evaluation
        .planned_contracts
        .iter()
        .map(|contract| {
            json!({
                "migration_file": contract.migration_file,
                "migration_class": contract.migration_class.as_str(),
                "rollout_order": contract.rollout_order.as_str(),
                "fail_forward_required": contract.fail_forward_required,
                "destructive": contract.destructive,
                "code_rollback_allowed": contract.code_rollback_allowed,
                "contract_preconditions": contract.contract_preconditions,
            })
        })
        .collect::<Vec<_>>();
    let mut output = json!({
        "schema_version": 1,
        "command": format!("d1 {}", action.name()),
        "status": if evaluation.allowed { "ok" } else { "blocked" },
        "mode": "read-only",
        "mutation_executed": false,
        "component": authority.component_id,
        "ledger_state": evaluation.ledger_state.as_str(),
        "decision": evaluation.decision.as_str(),
        "remote_revision": evaluation.remote_revision,
        "target_revision": evaluation.target_revision,
        "current_repository_revision": authority.current_repository_revision,
        "history_digest": authority.history_digest,
        "planned_migrations": evaluation.planned_migrations,
        "planned_migration_contracts": planned_contracts,
        "rollback_context_complete": evaluation.rollback_context_complete,
        "reason_codes": evaluation.reason_codes,
        "allowed": evaluation.allowed
    });
    if action == D1Action::Plan {
        output["transaction_policy"] = transaction_policy::ordinary_projection();
    }
    if let Some(runtime) = runtime_compatibility {
        output["schema_version"] = json!(2);
        output["runtime_compatibility"] = json!({
            "compatible": runtime.compatible,
            "reason_codes": runtime.reason_codes,
            "required_steps": runtime.required_steps,
        });
    }
    serde_json::to_string(&output)
        .map(|value| value + "\n")
        .map_err(|error| D1Error::new(format!("cannot serialize d1 result: {error}")))
}
