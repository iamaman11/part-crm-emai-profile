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
#[path = "d1/executor_admission.rs"]
pub mod executor_admission;
#[path = "d1/model.rs"]
mod model;
#[path = "d1/plan.rs"]
mod plan;
#[path = "d1/status.rs"]
mod status;
#[path = "d1/transaction.rs"]
mod transaction_core;
#[path = "d1/transaction_api.rs"]
pub mod transaction;
#[path = "d1/transaction_integrity.rs"]
pub mod transaction_integrity;
#[path = "d1/util.rs"]
mod util;
#[path = "d1/verify.rs"]
mod verify;

use crate::canonical::{canonical_json, sha256_hex};
use authority::{load_preconditions, load_release_contract, load_wrangler_ledger};
use catalog::component_authority;
use contract_transition::{ContractTransitionInput, ContractTransitionVerificationInput};
use model::{Evaluation, Preconditions, ReleaseSchemaContract};
use plan::evaluate;
use serde_json::{Value, json};
use std::path::Path;
use util::{read_json, resolve_input};

pub use model::{D1Action, D1Error, D1RunRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct D1ReleaseSchemaIdentity {
    pub database_component: String,
    pub target_schema_revision: String,
    pub supported_schema_min: String,
    pub supported_schema_max: String,
    pub migration_history_digest: String,
    pub compatibility_policy_digest: String,
}

pub struct D1ContractTransitionRequest<'a> {
    pub root: &'a Path,
    pub ledger_json: &'a Path,
    pub release_manifest: &'a Path,
    pub evidence_json: &'a Path,
    pub evaluated_at_unix_seconds: i64,
    pub expected_source_sha: &'a str,
    pub expected_release_set_id: &'a str,
}

pub struct D1ContractTransitionVerificationRequest<'a> {
    pub root: &'a Path,
    pub predecessor_ledger_json: &'a Path,
    pub ledger_json: &'a Path,
    pub release_manifest: &'a Path,
    pub evidence_json: &'a Path,
    pub evaluated_at_unix_seconds: i64,
    pub expected_source_sha: &'a str,
    pub expected_release_set_id: &'a str,
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
    let current = load_optional_release(
        request.root,
        request.current_manifest,
        request.component,
        false,
        request.action.name(),
    )?;
    let known_good = load_optional_release(
        request.root,
        request.known_good_manifest,
        request.component,
        false,
        request.action.name(),
    )?;
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
    serialize_evaluation(&authority, request.action, evaluation)
}

pub fn contract_transition(request: D1ContractTransitionRequest<'_>) -> Result<String, D1Error> {
    let authority = component_authority(request.root, "catalog")?;
    let ledger_path = resolve_input(request.root, request.ledger_json);
    let release_path = resolve_input(request.root, request.release_manifest);
    let evidence_path = resolve_input(request.root, request.evidence_json);
    let remote_names = load_wrangler_ledger(&ledger_path)?;
    let release = load_release_contract(&release_path, "catalog")?;
    let ledger_value = read_json(&ledger_path, "D1 contract-transition ledger")?;
    let release_value = read_json(&release_path, "D1 contract-transition release manifest")?;
    let evidence = read_json(&evidence_path, "D1 contract-transition evidence")?;
    let canonical_ledger = canonical_json(&ledger_value).map_err(D1Error::new)?;
    let canonical_release = canonical_json(&release_value).map_err(D1Error::new)?;
    let ledger_sha256 = sha256_hex(canonical_ledger.as_bytes());
    let release_manifest_sha256 = sha256_hex(canonical_release.as_bytes());
    let repository_identity_sha256 = catalog::repository_identity_sha256(request.root)?;

    contract_transition::evaluate(ContractTransitionInput {
        authority: &authority,
        remote_names: &remote_names,
        release: &release,
        evidence: &evidence,
        evaluated_at_unix_seconds: request.evaluated_at_unix_seconds,
        expected_source_sha: request.expected_source_sha,
        expected_release_set_id: request.expected_release_set_id,
        release_manifest_sha256: &release_manifest_sha256,
        repository_identity_sha256: &repository_identity_sha256,
        ledger_sha256: &ledger_sha256,
    })
}

pub fn contract_transition_verify(
    request: D1ContractTransitionVerificationRequest<'_>,
) -> Result<String, D1Error> {
    let authority = component_authority(request.root, "catalog")?;
    let predecessor_ledger_path = resolve_input(request.root, request.predecessor_ledger_json);
    let ledger_path = resolve_input(request.root, request.ledger_json);
    let release_path = resolve_input(request.root, request.release_manifest);
    let evidence_path = resolve_input(request.root, request.evidence_json);
    let predecessor_remote_names = load_wrangler_ledger(&predecessor_ledger_path)?;
    let remote_names = load_wrangler_ledger(&ledger_path)?;
    let release = load_release_contract(&release_path, "catalog")?;
    let predecessor_ledger_value = read_json(
        &predecessor_ledger_path,
        "D1 contract-transition predecessor ledger",
    )?;
    let release_value = read_json(&release_path, "D1 contract-transition release manifest")?;
    let evidence = read_json(&evidence_path, "D1 contract-transition evidence")?;
    let canonical_predecessor_ledger =
        canonical_json(&predecessor_ledger_value).map_err(D1Error::new)?;
    let canonical_release = canonical_json(&release_value).map_err(D1Error::new)?;
    let predecessor_ledger_sha256 = sha256_hex(canonical_predecessor_ledger.as_bytes());
    let release_manifest_sha256 = sha256_hex(canonical_release.as_bytes());
    let repository_identity_sha256 = catalog::repository_identity_sha256(request.root)?;

    contract_transition::verify_post_transition(ContractTransitionVerificationInput {
        authority: &authority,
        predecessor_remote_names: &predecessor_remote_names,
        remote_names: &remote_names,
        release: &release,
        evidence: &evidence,
        evaluated_at_unix_seconds: request.evaluated_at_unix_seconds,
        expected_source_sha: request.expected_source_sha,
        expected_release_set_id: request.expected_release_set_id,
        release_manifest_sha256: &release_manifest_sha256,
        repository_identity_sha256: &repository_identity_sha256,
        predecessor_ledger_sha256: &predecessor_ledger_sha256,
    })
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

fn serialize_evaluation(
    authority: &model::ComponentAuthority,
    action: D1Action,
    evaluation: Evaluation,
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
    let output = json!({
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
    serde_json::to_string(&output)
        .map(|value| value + "\n")
        .map_err(|error| D1Error::new(format!("cannot serialize d1 result: {error}")))
}
