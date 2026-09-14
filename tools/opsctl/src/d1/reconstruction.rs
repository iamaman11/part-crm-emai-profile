use super::authority::{load_release_contract, load_wrangler_ledger};
use super::catalog;
use super::model::{D1Error, ReleaseSchemaContract};
use super::transaction_core::TargetIdentity;
use super::util::{read_json, resolve_input};
use crate::canonical::{canonical_json, sha256_hex};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const RECONSTRUCTION_SCHEMA_VERSION: u64 = 1;
const OBSERVATION_FRESHNESS_MAX_AGE_SECONDS: u64 = 900;
const CURRENT_RECONSTRUCTION_KIND: &str = "D1_CURRENT_FRESH_ZERO_RECONSTRUCTION";
const CURRENT_DISPOSITION: &str = "PREPROD_BASELINE_DRIFT";
const ALLOWED_PROVIDER_EFFECT: &str = "D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION";
const FORBIDDEN_PROVIDER_EFFECTS: [&str; 6] = [
    "D1_MIGRATIONS_APPLY_EXACT_PLAN",
    "D1_CREATE",
    "D1_DELETE",
    "D1_TIME_TRAVEL_RESTORE",
    "RESOURCE_AUTO_PROVISION",
    "PRODUCTION_MUTATION",
];
const RELEASE_SET_PREFIX: &str = "release-set-v3-sha256-";

pub struct D1CurrentReconstructionPlanRequest<'a> {
    pub root: &'a Path,
    pub ledger_json: &'a Path,
    pub release_manifest: &'a Path,
    pub target_json: &'a Path,
    pub source_sha: &'a str,
    pub tree_sha: &'a str,
    pub release_set_id: &'a str,
    pub observed_at_unix_seconds: i64,
    pub observation_source: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconstructionAuthorizationSubject {
    pub operation_id: String,
    pub target: TargetIdentity,
    pub allowed_provider_effects: Vec<String>,
    pub observed_at_unix_seconds: i64,
    pub freshness_max_age_seconds: u64,
}

pub(super) fn plan(request: D1CurrentReconstructionPlanRequest<'_>) -> Result<String, D1Error> {
    validate_git_object_id(request.source_sha, "source_sha")?;
    validate_git_object_id(request.tree_sha, "tree_sha")?;
    validate_release_set_id(request.release_set_id)?;
    validate_non_empty(request.observation_source, "observation_source")?;
    if request.observed_at_unix_seconds <= 0 {
        return Err(D1Error::new(
            "CURRENT reconstruction observed_at_unix_seconds must be positive",
        ));
    }
    let freshness_seconds = i64::try_from(OBSERVATION_FRESHNESS_MAX_AGE_SECONDS)
        .map_err(|_| D1Error::new("CURRENT reconstruction freshness window does not fit i64"))?;
    let fresh_until_unix_seconds = request
        .observed_at_unix_seconds
        .checked_add(freshness_seconds)
        .ok_or_else(|| D1Error::new("CURRENT reconstruction freshness deadline overflow"))?;

    let ledger_path = resolve_input(request.root, request.ledger_json);
    let remote_migrations = load_wrangler_ledger(&ledger_path)?;
    if !remote_migrations.is_empty() {
        return Err(D1Error::new(
            "CURRENT fresh-zero reconstruction requires an exactly empty D1 migration ledger; ordinary known-prefix drift must use the ordinary migration path",
        ));
    }
    let normalized_ledger = json!({"remote_migrations": remote_migrations});
    let predecessor_ledger_sha256 = sha256_canonical(&normalized_ledger)?;

    let target_path = resolve_input(request.root, request.target_json);
    let target_value = read_json(&target_path, "CURRENT D1 reconstruction target")?;
    let target: TargetIdentity = serde_json::from_value(target_value).map_err(|error| {
        D1Error::new(format!(
            "CURRENT D1 reconstruction target does not match the typed target contract: {error}"
        ))
    })?;
    validate_target(&target)?;

    let release_path = resolve_input(request.root, request.release_manifest);
    let release = load_release_contract(&release_path, "catalog")?;
    let release_manifest_raw = fs::read(&release_path).map_err(|error| {
        D1Error::new(format!(
            "cannot read CURRENT D1 reconstruction release manifest {}: {error}",
            release_path.display()
        ))
    })?;
    let release_manifest_sha256 = sha256_hex(&release_manifest_raw);

    let repository_text = catalog::repository_projection(request.root)?;
    let repository: Value = serde_json::from_str(&repository_text).map_err(|error| {
        D1Error::new(format!(
            "cannot parse typed D1 repository projection for CURRENT reconstruction: {error}"
        ))
    })?;
    let repository_identity_sha256 = required_sha256(
        repository.get("repository_identity_sha256"),
        "repository_identity_sha256",
    )?;
    let construction_envelope = repository
        .get("fresh_zero_construction")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            D1Error::new("typed D1 repository projection is missing fresh_zero_construction")
        })?;
    if construction_envelope.len() != 2
        || !construction_envelope.contains_key("construction")
        || !construction_envelope.contains_key("construction_sha256")
    {
        return Err(D1Error::new(
            "typed D1 fresh_zero_construction envelope has unexpected fields",
        ));
    }
    let construction = construction_envelope
        .get("construction")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("typed D1 fresh_zero_construction is not an object"))?;
    let construction_sha256 = required_sha256(
        construction_envelope.get("construction_sha256"),
        "fresh_zero_construction.construction_sha256",
    )?;
    if sha256_canonical(&Value::Object(construction.clone()))? != construction_sha256 {
        return Err(D1Error::new(
            "typed D1 fresh-zero construction digest does not bind the exact construction",
        ));
    }
    if construction.get("kind").and_then(Value::as_str)
        != Some("D1_CURRENT_FRESH_ZERO_CONSTRUCTION")
        || construction.get("component_id").and_then(Value::as_str) != Some("catalog")
        || construction
            .get("repository_identity_sha256")
            .and_then(Value::as_str)
            != Some(repository_identity_sha256.as_str())
        || construction
            .get("provider_mutation_authorized")
            .and_then(Value::as_bool)
            != Some(false)
        || construction
            .get("production_mutation_authorized")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(D1Error::new(
            "typed D1 fresh-zero construction is not the current non-mutating Catalog authority",
        ));
    }

    let target_schema_revision = construction
        .get("target_schema_revision")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| D1Error::new("typed D1 fresh-zero construction target is missing"))?
        .to_owned();
    let migration_sources = construction
        .get("migration_sources")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            D1Error::new("typed D1 fresh-zero construction migration_sources are missing")
        })?;
    if migration_sources.is_empty() {
        return Err(D1Error::new(
            "typed D1 fresh-zero construction must materialize at least one migration",
        ));
    }
    let mut expected_ledger = Vec::with_capacity(migration_sources.len());
    for source in migration_sources {
        let migration_file = source
            .get("migration_file")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                D1Error::new("typed D1 fresh-zero migration source is missing migration_file")
            })?;
        required_sha256(source.get("sha256"), "fresh-zero migration source sha256")?;
        if expected_ledger.iter().any(|value| value == migration_file) {
            return Err(D1Error::new(
                "typed D1 fresh-zero construction contains a duplicate migration source",
            ));
        }
        expected_ledger.push(migration_file.to_owned());
    }
    if expected_ledger.last().map(String::as_str) != Some(target_schema_revision.as_str()) {
        return Err(D1Error::new(
            "typed D1 fresh-zero construction does not terminate at its target schema revision",
        ));
    }
    let deferred_revisions = construction
        .get("deferred_revisions")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            D1Error::new("typed D1 fresh-zero construction deferred_revisions are missing")
        })?;
    for deferred in deferred_revisions {
        let deferred = deferred
            .as_str()
            .ok_or_else(|| D1Error::new("typed D1 deferred revision must be a string"))?;
        if expected_ledger.iter().any(|value| value == deferred) {
            return Err(D1Error::new(
                "typed D1 fresh-zero construction materializes a deferred revision",
            ));
        }
    }

    let projected_release = repository
        .get("components")
        .and_then(Value::as_array)
        .and_then(|components| {
            components.iter().find(|component| {
                component.get("component_id").and_then(Value::as_str) == Some("catalog")
            })
        })
        .and_then(|component| component.get("release_schema_contract"))
        .ok_or_else(|| {
            D1Error::new("typed D1 repository projection is missing Catalog release contract")
        })?;
    validate_release_binding(&release, projected_release)?;
    if release.target_schema_revision != target_schema_revision {
        return Err(D1Error::new(
            "CURRENT reconstruction release target differs from the typed fresh-zero construction target",
        ));
    }

    let provider_observation = json!({
        "target": target,
        "observed_at_unix_seconds": request.observed_at_unix_seconds,
        "fresh_until_unix_seconds": fresh_until_unix_seconds,
        "observation_source": request.observation_source,
        "predecessor_ledger_sha256": predecessor_ledger_sha256,
        "remote_migrations": [],
    });
    let observation_digest = sha256_canonical(&provider_observation)?;
    let expected_post_state = json!({
        "component": "catalog",
        "target_schema_revision": target_schema_revision,
        "ledger_migrations": expected_ledger,
        "construction_sha256": construction_sha256,
        "repository_identity_sha256": repository_identity_sha256,
    });
    let plan_value = json!({
        "schema_version": RECONSTRUCTION_SCHEMA_VERSION,
        "kind": CURRENT_RECONSTRUCTION_KIND,
        "disposition": CURRENT_DISPOSITION,
        "component": "catalog",
        "source_sha": request.source_sha,
        "tree_sha": request.tree_sha,
        "release_set_id": request.release_set_id,
        "release_manifest_sha256": release_manifest_sha256,
        "repository_identity_sha256": repository_identity_sha256,
        "construction_sha256": construction_sha256,
        "target_schema_revision": release.target_schema_revision,
        "supported_schema_min": release.supported_schema_min,
        "supported_schema_max": release.supported_schema_max,
        "provider_observation": provider_observation,
        "observation_digest": observation_digest,
        "freshness_max_age_seconds": OBSERVATION_FRESHNESS_MAX_AGE_SECONDS,
        "allowed_provider_effects": [ALLOWED_PROVIDER_EFFECT],
        "forbidden_provider_effects": FORBIDDEN_PROVIDER_EFFECTS,
        "expected_post_state": expected_post_state,
    });
    let reconstruction_id = sha256_canonical(&plan_value)?;
    let projection = json!({
        "schema_version": RECONSTRUCTION_SCHEMA_VERSION,
        "status": "RECONSTRUCTION_PREPARED",
        "mode": "read-only",
        "authorization_required": true,
        "authorization_consumed": false,
        "mutation_executed": false,
        "provider_mutation_executed": false,
        "reconstruction_id": reconstruction_id,
        "plan": plan_value,
    });
    canonical_json(&projection).map_err(D1Error::new)
}

pub(crate) fn authorization_subject(
    projection: &Value,
) -> Result<ReconstructionAuthorizationSubject, D1Error> {
    let root = projection
        .as_object()
        .ok_or_else(|| D1Error::new("CURRENT reconstruction projection must be an object"))?;
    require_exact_keys(
        root,
        &[
            "schema_version",
            "status",
            "mode",
            "authorization_required",
            "authorization_consumed",
            "mutation_executed",
            "provider_mutation_executed",
            "reconstruction_id",
            "plan",
        ],
        "CURRENT reconstruction projection",
    )?;
    if root.get("schema_version").and_then(Value::as_u64) != Some(RECONSTRUCTION_SCHEMA_VERSION)
        || root.get("status").and_then(Value::as_str) != Some("RECONSTRUCTION_PREPARED")
        || root.get("mode").and_then(Value::as_str) != Some("read-only")
        || root.get("authorization_required").and_then(Value::as_bool) != Some(true)
        || root.get("authorization_consumed").and_then(Value::as_bool) != Some(false)
        || root.get("mutation_executed").and_then(Value::as_bool) != Some(false)
        || root
            .get("provider_mutation_executed")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(D1Error::new(
            "CURRENT reconstruction projection is not an unconsumed read-only prepared operation",
        ));
    }

    let reconstruction_id = required_sha256(root.get("reconstruction_id"), "reconstruction_id")?;
    let plan = root
        .get("plan")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("CURRENT reconstruction projection is missing plan"))?;
    require_exact_keys(
        plan,
        &[
            "schema_version",
            "kind",
            "disposition",
            "component",
            "source_sha",
            "tree_sha",
            "release_set_id",
            "release_manifest_sha256",
            "repository_identity_sha256",
            "construction_sha256",
            "target_schema_revision",
            "supported_schema_min",
            "supported_schema_max",
            "provider_observation",
            "observation_digest",
            "freshness_max_age_seconds",
            "allowed_provider_effects",
            "forbidden_provider_effects",
            "expected_post_state",
        ],
        "CURRENT reconstruction plan",
    )?;
    if plan.get("schema_version").and_then(Value::as_u64) != Some(RECONSTRUCTION_SCHEMA_VERSION)
        || plan.get("kind").and_then(Value::as_str) != Some(CURRENT_RECONSTRUCTION_KIND)
        || plan.get("disposition").and_then(Value::as_str) != Some(CURRENT_DISPOSITION)
        || plan.get("component").and_then(Value::as_str) != Some("catalog")
    {
        return Err(D1Error::new(
            "CURRENT reconstruction plan kind/disposition/component drifted",
        ));
    }
    if sha256_canonical(&Value::Object(plan.clone()))? != reconstruction_id {
        return Err(D1Error::new(
            "CURRENT reconstruction_id does not bind the exact canonical plan",
        ));
    }

    let source_sha = required_string(plan.get("source_sha"), "source_sha")?;
    validate_git_object_id(source_sha, "source_sha")?;
    let tree_sha = required_string(plan.get("tree_sha"), "tree_sha")?;
    validate_git_object_id(tree_sha, "tree_sha")?;
    validate_release_set_id(required_string(
        plan.get("release_set_id"),
        "release_set_id",
    )?)?;
    required_sha256(
        plan.get("release_manifest_sha256"),
        "release_manifest_sha256",
    )?;
    let repository_identity_sha256 = required_sha256(
        plan.get("repository_identity_sha256"),
        "repository_identity_sha256",
    )?;
    let construction_sha256 =
        required_sha256(plan.get("construction_sha256"), "construction_sha256")?;
    let target_schema_revision =
        required_string(plan.get("target_schema_revision"), "target_schema_revision")?;
    validate_non_empty(target_schema_revision, "target_schema_revision")?;
    validate_non_empty(
        required_string(plan.get("supported_schema_min"), "supported_schema_min")?,
        "supported_schema_min",
    )?;
    validate_non_empty(
        required_string(plan.get("supported_schema_max"), "supported_schema_max")?,
        "supported_schema_max",
    )?;

    let freshness_max_age_seconds = plan
        .get("freshness_max_age_seconds")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            D1Error::new("CURRENT reconstruction freshness_max_age_seconds is missing")
        })?;
    if freshness_max_age_seconds != OBSERVATION_FRESHNESS_MAX_AGE_SECONDS {
        return Err(D1Error::new(
            "CURRENT reconstruction freshness window drifted from the canonical bounded window",
        ));
    }

    let allowed_provider_effects = string_array(
        plan.get("allowed_provider_effects"),
        "allowed_provider_effects",
    )?;
    if allowed_provider_effects != vec![ALLOWED_PROVIDER_EFFECT.to_owned()] {
        return Err(D1Error::new(
            "CURRENT reconstruction allowed provider effect must be exactly the canonical bootstrap effect",
        ));
    }
    let forbidden_provider_effects = string_array(
        plan.get("forbidden_provider_effects"),
        "forbidden_provider_effects",
    )?;
    let expected_forbidden = FORBIDDEN_PROVIDER_EFFECTS
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    if forbidden_provider_effects != expected_forbidden {
        return Err(D1Error::new(
            "CURRENT reconstruction forbidden provider effect set drifted",
        ));
    }

    let provider_observation = plan
        .get("provider_observation")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("CURRENT reconstruction provider_observation is missing"))?;
    require_exact_keys(
        provider_observation,
        &[
            "target",
            "observed_at_unix_seconds",
            "fresh_until_unix_seconds",
            "observation_source",
            "predecessor_ledger_sha256",
            "remote_migrations",
        ],
        "CURRENT reconstruction provider observation",
    )?;
    let target: TargetIdentity = serde_json::from_value(
        provider_observation
            .get("target")
            .cloned()
            .ok_or_else(|| D1Error::new("CURRENT reconstruction target is missing"))?,
    )
    .map_err(|error| {
        D1Error::new(format!(
            "CURRENT reconstruction authorization target does not match the typed target contract: {error}"
        ))
    })?;
    validate_target(&target)?;
    let observed_at_unix_seconds = provider_observation
        .get("observed_at_unix_seconds")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            D1Error::new("CURRENT reconstruction observed_at_unix_seconds is missing")
        })?;
    if observed_at_unix_seconds <= 0 {
        return Err(D1Error::new(
            "CURRENT reconstruction observed_at_unix_seconds must be positive",
        ));
    }
    let freshness_seconds = i64::try_from(freshness_max_age_seconds)
        .map_err(|_| D1Error::new("CURRENT reconstruction freshness window does not fit i64"))?;
    let expected_fresh_until = observed_at_unix_seconds
        .checked_add(freshness_seconds)
        .ok_or_else(|| D1Error::new("CURRENT reconstruction freshness deadline overflow"))?;
    if provider_observation
        .get("fresh_until_unix_seconds")
        .and_then(Value::as_i64)
        != Some(expected_fresh_until)
    {
        return Err(D1Error::new(
            "CURRENT reconstruction provider observation freshness deadline drifted",
        ));
    }
    validate_non_empty(
        required_string(
            provider_observation.get("observation_source"),
            "observation_source",
        )?,
        "observation_source",
    )?;
    required_sha256(
        provider_observation.get("predecessor_ledger_sha256"),
        "predecessor_ledger_sha256",
    )?;
    let remote_migrations = provider_observation
        .get("remote_migrations")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("CURRENT reconstruction remote_migrations are missing"))?;
    if !remote_migrations.is_empty() {
        return Err(D1Error::new(
            "CURRENT fresh-zero reconstruction authorization requires the sealed remote ledger to remain exactly empty",
        ));
    }
    let observation_digest = required_sha256(plan.get("observation_digest"), "observation_digest")?;
    if sha256_canonical(&Value::Object(provider_observation.clone()))? != observation_digest {
        return Err(D1Error::new(
            "CURRENT reconstruction observation_digest does not bind the exact provider observation",
        ));
    }

    let expected_post_state = plan
        .get("expected_post_state")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("CURRENT reconstruction expected_post_state is missing"))?;
    require_exact_keys(
        expected_post_state,
        &[
            "component",
            "target_schema_revision",
            "ledger_migrations",
            "construction_sha256",
            "repository_identity_sha256",
        ],
        "CURRENT reconstruction expected post-state",
    )?;
    if expected_post_state.get("component").and_then(Value::as_str) != Some("catalog")
        || expected_post_state
            .get("target_schema_revision")
            .and_then(Value::as_str)
            != Some(target_schema_revision)
        || expected_post_state
            .get("construction_sha256")
            .and_then(Value::as_str)
            != Some(construction_sha256.as_str())
        || expected_post_state
            .get("repository_identity_sha256")
            .and_then(Value::as_str)
            != Some(repository_identity_sha256.as_str())
    {
        return Err(D1Error::new(
            "CURRENT reconstruction expected post-state drifted from the bound construction/repository target",
        ));
    }
    let ledger_migrations = string_array(
        expected_post_state.get("ledger_migrations"),
        "expected_post_state.ledger_migrations",
    )?;
    if ledger_migrations.is_empty()
        || ledger_migrations.last().map(String::as_str) != Some(target_schema_revision)
    {
        return Err(D1Error::new(
            "CURRENT reconstruction expected ledger must be non-empty and terminate at the target schema revision",
        ));
    }
    let mut unique_migrations = BTreeSet::new();
    if ledger_migrations
        .iter()
        .any(|migration| !unique_migrations.insert(migration.as_str()))
    {
        return Err(D1Error::new(
            "CURRENT reconstruction expected ledger contains duplicate migrations",
        ));
    }

    Ok(ReconstructionAuthorizationSubject {
        operation_id: reconstruction_id,
        target,
        allowed_provider_effects,
        observed_at_unix_seconds,
        freshness_max_age_seconds,
    })
}

fn require_exact_keys(
    value: &Map<String, Value>,
    expected: &[&str],
    label: &str,
) -> Result<(), D1Error> {
    if value.len() != expected.len() || expected.iter().any(|key| !value.contains_key(*key)) {
        return Err(D1Error::new(format!(
            "{label} has missing or unexpected fields"
        )));
    }
    Ok(())
}

fn string_array(value: Option<&Value>, label: &str) -> Result<Vec<String>, D1Error> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new(format!("CURRENT reconstruction {label} must be an array")))?;
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let value = value.as_str().ok_or_else(|| {
            D1Error::new(format!(
                "CURRENT reconstruction {label} entries must be strings"
            ))
        })?;
        validate_non_empty(value, label)?;
        output.push(value.to_owned());
    }
    Ok(output)
}

fn required_string<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str, D1Error> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new(format!("CURRENT reconstruction {label} is missing")))
}

fn validate_release_binding(
    release: &ReleaseSchemaContract,
    projected: &Value,
) -> Result<(), D1Error> {
    let expected = [
        ("database_component", release.database_component.as_str()),
        (
            "target_schema_revision",
            release.target_schema_revision.as_str(),
        ),
        (
            "supported_schema_min",
            release.supported_schema_min.as_str(),
        ),
        (
            "supported_schema_max",
            release.supported_schema_max.as_str(),
        ),
        (
            "migration_history_digest",
            release.migration_history_digest.as_str(),
        ),
        (
            "compatibility_policy_digest",
            release.compatibility_policy_digest.as_str(),
        ),
    ];
    for (field, actual) in expected {
        if projected.get(field).and_then(Value::as_str) != Some(actual) {
            return Err(D1Error::new(format!(
                "CURRENT reconstruction release manifest drifted from typed Catalog release contract at {field}"
            )));
        }
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity) -> Result<(), D1Error> {
    if target.environment != "staging" {
        return Err(D1Error::new(
            "CURRENT fresh-zero reconstruction is restricted to the exact non-Production staging environment",
        ));
    }
    for (label, value) in [
        ("target.account_id", target.account_id.as_str()),
        ("target.database_name", target.database_name.as_str()),
        ("target.database_id", target.database_id.as_str()),
    ] {
        validate_non_empty(value, label)?;
    }
    Ok(())
}

fn validate_release_set_id(value: &str) -> Result<(), D1Error> {
    let digest = value.strip_prefix(RELEASE_SET_PREFIX).ok_or_else(|| {
        D1Error::new(format!(
            "CURRENT reconstruction release_set_id must start with {RELEASE_SET_PREFIX}"
        ))
    })?;
    validate_sha256(digest, "release_set_id digest")
}

fn validate_git_object_id(value: &str, label: &str) -> Result<(), D1Error> {
    if !matches!(value.len(), 40 | 64) || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "CURRENT reconstruction {label} must be a 40- or 64-character lowercase Git object id"
        )));
    }
    Ok(())
}

fn required_sha256(value: Option<&Value>, label: &str) -> Result<String, D1Error> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new(format!("{label} is missing")))?;
    validate_sha256(value, label)?;
    Ok(value.to_owned())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 64 || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "CURRENT reconstruction {label} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), D1Error> {
    if value.trim().is_empty() {
        return Err(D1Error::new(format!(
            "CURRENT reconstruction {label} must not be empty"
        )));
    }
    Ok(())
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_canonical(value: &Value) -> Result<String, D1Error> {
    let canonical = canonical_json(value).map_err(D1Error::new)?;
    Ok(sha256_hex(canonical.as_bytes()))
}
