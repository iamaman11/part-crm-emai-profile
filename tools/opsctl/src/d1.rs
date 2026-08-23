#[path = "d1/authority.rs"]
mod authority;
#[path = "d1/catalog.rs"]
mod catalog;
#[path = "d1/compatibility.rs"]
mod compatibility;
#[path = "d1/model.rs"]
mod model;
#[path = "d1/plan.rs"]
mod plan;
#[path = "d1/status.rs"]
mod status;
#[path = "d1/util.rs"]
mod util;
#[path = "d1/verify.rs"]
mod verify;

use authority::{load_preconditions, load_release_contract, load_wrangler_ledger};
use catalog::component_authority;
use model::{Evaluation, Preconditions, ReleaseSchemaContract};
use plan::evaluate;
use serde_json::json;
use std::path::Path;
use util::resolve_input;

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

pub fn repository_projection(root: &std::path::Path) -> Result<String, D1Error> {
    catalog::repository_projection(root)
}

pub(crate) use catalog::{release_contract, repository_identity_sha256};

pub(crate) fn release_schema_identity(
    root: &Path,
    component: &str,
) -> Result<D1ReleaseSchemaIdentity, D1Error> {
    let authority = component_authority(root, component)?;
    Ok(D1ReleaseSchemaIdentity {
        database_component: authority.component_id,
        target_schema_revision: authority.current_repository_revision.clone(),
        supported_schema_min: authority.current_repository_revision.clone(),
        supported_schema_max: authority.current_repository_revision,
        migration_history_digest: authority.history_digest,
        compatibility_policy_digest: authority.policy_digest,
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
