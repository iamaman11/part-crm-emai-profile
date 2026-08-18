#[path = "d1/authority.rs"]
mod authority;
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

use authority::{
    load_component_authority, load_preconditions, load_release_contract, load_wrangler_ledger,
};
use model::{Evaluation, Preconditions, ReleaseSchemaContract};
use plan::evaluate;
use serde_json::json;
use std::path::Path;
use util::resolve_input;

pub use model::{D1Action, D1Error, D1RunRequest};

pub const DEFAULT_AUTHORITY: &str = "architecture/d1-evolution-ar9.json";

/// Canonical AR-9 policy vocabulary. Keeping the vocabulary at the facade makes the
/// operator surface reviewable while implementation details live in focused modules.
pub const POLICY_VOCABULARY: &[&str] = &[
    "EXACT",
    "BEHIND_KNOWN_PREFIX",
    "AHEAD_KNOWN_COMPATIBLE",
    "AHEAD_KNOWN_INCOMPATIBLE",
    "DIVERGED",
    "UNKNOWN_MIGRATION",
    "CORRUPT_LEDGER",
    "SAFE",
    "MIGRATION_REQUIRED",
    "DEPLOY_FIRST",
    "MIGRATE_FIRST",
    "CODE_ROLLBACK_SAFE",
    "CODE_ROLLBACK_BLOCKED",
    "FAIL_FORWARD_REQUIRED",
    "CONTRACT_BLOCKED",
    "RECOVERY_REQUIRED",
];

pub fn run(request: D1RunRequest<'_>) -> Result<String, D1Error> {
    let authority = load_component_authority(
        request.root,
        request
            .authority_path
            .unwrap_or_else(|| Path::new(DEFAULT_AUTHORITY)),
        request.component,
    )?;
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
        "planned_migration_contracts": evaluation.planned_contracts,
        "rollback_context_complete": evaluation.rollback_context_complete,
        "reason_codes": evaluation.reason_codes,
        "allowed": evaluation.allowed
    });
    serde_json::to_string(&output)
        .map(|value| value + "\n")
        .map_err(|error| D1Error::new(format!("cannot serialize d1 result: {error}")))
}
