#![forbid(unsafe_code)]

use opsctl::canonical::{canonical_json, parse_strict_json, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const OBSERVATION_SCHEMA_VERSION: u64 = 1;
const TRANSACTION_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetIdentity {
    environment: String,
    account_id: String,
    database_name: String,
    database_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderObservationInput {
    schema_version: u64,
    target: TargetIdentity,
    observed_at_unix_seconds: i64,
    observation_source: String,
    remote_ledger_sha256: String,
    remote_migrations: Vec<String>,
    wrangler_pending_migrations: Vec<String>,
    deployment_identity: Option<String>,
    time_travel_bookmark_capable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProviderObservationBundle {
    schema_version: u64,
    observation_digest: String,
    target: TargetIdentity,
    observed_at_unix_seconds: i64,
    observation_source: String,
    remote_ledger_sha256: String,
    remote_migrations: Vec<String>,
    wrangler_pending_migrations: Vec<String>,
    deployment_identity: Option<String>,
    time_travel_bookmark_capable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlannedMigrationDigest {
    migration_file: String,
    content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionIdentityInput {
    schema_version: u64,
    source_sha: String,
    tree_sha: String,
    release_candidate_id: String,
    release_manifest_digests: BTreeMap<String, String>,
    planner_policy_digest: String,
    migration_lineage_digest: String,
    transaction_kind: String,
    phase: String,
    target: TargetIdentity,
    freshness_max_age_seconds: u64,
    predecessor_ledger_sha256: String,
    planned_migrations: Vec<PlannedMigrationDigest>,
    schema_target: String,
    supported_schema_min: String,
    supported_schema_max: String,
    precondition_evidence_refs: Vec<String>,
    recovery_strategy: String,
    expected_post_state: Value,
    allowed_provider_effects: Vec<String>,
    forbidden_provider_effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MigrationTransactionPlan {
    schema_version: u64,
    planner_policy_digest: String,
    transaction_kind: String,
    phase: String,
    source_sha: String,
    tree_sha: String,
    release_candidate_id: String,
    release_manifest_digests: BTreeMap<String, String>,
    migration_lineage_digest: String,
    target: TargetIdentity,
    observation_digest: String,
    observed_at_unix_seconds: i64,
    freshness_max_age_seconds: u64,
    predecessor_ledger_sha256: String,
    planned_migrations: Vec<PlannedMigrationDigest>,
    schema_target: String,
    supported_schema_min: String,
    supported_schema_max: String,
    precondition_evidence_refs: Vec<String>,
    recovery_strategy: String,
    expected_post_state: Value,
    allowed_provider_effects: Vec<String>,
    forbidden_provider_effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TransactionProjection {
    schema_version: u64,
    status: &'static str,
    mode: &'static str,
    authorization_consumed: bool,
    mutation_executed: bool,
    provider_mutation_executed: bool,
    provider_observation: ProviderObservationBundle,
    transaction_id: String,
    transaction_plan: MigrationTransactionPlan,
}

#[derive(Default)]
struct Args {
    prepare_json: Option<PathBuf>,
    observation_json: Option<PathBuf>,
    transaction_input_json: Option<PathBuf>,
}

fn next_value(
    iterator: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<OsString, Box<dyn Error>> {
    iterator
        .next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), Box<dyn Error>> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be supplied only once").into());
    }
    Ok(())
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut iterator = env::args_os();
    let _program = iterator.next();
    let mut args = Args::default();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or("transaction-plan flags must be valid UTF-8")?;
        match flag {
            "--prepare-json" => set_once(
                &mut args.prepare_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--observation-json" => set_once(
                &mut args.observation_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--transaction-input-json" => set_once(
                &mut args.transaction_input_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            other => return Err(format!("unsupported transaction-plan argument: {other}").into()),
        }
    }
    Ok(args)
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} is required").into())
}

fn read_strict_value(path: &Path, label: &str) -> Result<Value, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?;
    parse_strict_json(&raw)
        .map_err(|error| format!("{label} is not strict bounded JSON: {error}").into())
}

fn typed_from_value<T>(value: &Value, label: &str) -> Result<T, Box<dyn Error>>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value.clone())
        .map_err(|error| format!("{label} does not match the typed contract: {error}").into())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), Box<dyn Error>> {
    let valid = value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid {
        return Err(format!("{label} must be exactly 64 hexadecimal characters").into());
    }
    Ok(())
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), Box<dyn Error>> {
    if value.trim().is_empty() {
        return Err(format!("{label} must not be empty").into());
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity) -> Result<(), Box<dyn Error>> {
    validate_non_empty(&target.environment, "target.environment")?;
    validate_non_empty(&target.account_id, "target.account_id")?;
    validate_non_empty(&target.database_name, "target.database_name")?;
    validate_non_empty(&target.database_id, "target.database_id")?;
    Ok(())
}

fn validate_observation(input: &ProviderObservationInput) -> Result<(), Box<dyn Error>> {
    if input.schema_version != OBSERVATION_SCHEMA_VERSION {
        return Err(format!(
            "provider observation schema_version must be {OBSERVATION_SCHEMA_VERSION}"
        )
        .into());
    }
    validate_target(&input.target)?;
    validate_non_empty(&input.observation_source, "observation_source")?;
    validate_sha256(&input.remote_ledger_sha256, "remote_ledger_sha256")?;
    if input.observed_at_unix_seconds <= 0 {
        return Err("observed_at_unix_seconds must be positive".into());
    }
    let unique = input.remote_migrations.iter().collect::<BTreeSet<_>>();
    if unique.len() != input.remote_migrations.len() {
        return Err("remote_migrations must not contain duplicates".into());
    }
    let pending_unique = input
        .wrangler_pending_migrations
        .iter()
        .collect::<BTreeSet<_>>();
    if pending_unique.len() != input.wrangler_pending_migrations.len() {
        return Err("wrangler_pending_migrations must not contain duplicates".into());
    }
    Ok(())
}

fn seal_observation(
    input: ProviderObservationInput,
) -> Result<ProviderObservationBundle, Box<dyn Error>> {
    validate_observation(&input)?;
    let value = serde_json::to_value(&input)?;
    let canonical = canonical_json(&value)?;
    let observation_digest = sha256_hex(canonical.as_bytes());
    Ok(ProviderObservationBundle {
        schema_version: input.schema_version,
        observation_digest,
        target: input.target,
        observed_at_unix_seconds: input.observed_at_unix_seconds,
        observation_source: input.observation_source,
        remote_ledger_sha256: input.remote_ledger_sha256,
        remote_migrations: input.remote_migrations,
        wrangler_pending_migrations: input.wrangler_pending_migrations,
        deployment_identity: input.deployment_identity,
        time_travel_bookmark_capable: input.time_travel_bookmark_capable,
    })
}

fn prepare_plan(value: &Value) -> Result<&serde_json::Map<String, Value>, Box<dyn Error>> {
    if value.get("status").and_then(Value::as_str) != Some("PREPARE_READY") {
        return Err("transaction plan requires PREPARE_READY input".into());
    }
    for field in [
        "authorization_consumed",
        "mutation_executed",
        "provider_mutation_executed",
    ] {
        if value.get(field).and_then(Value::as_bool) != Some(false) {
            return Err(format!("prepare.{field} must be false before transaction identity").into());
        }
    }
    value
        .get("plan")
        .and_then(Value::as_object)
        .ok_or_else(|| "PREPARE_READY input must contain a plan object".into())
}

fn prepare_planned_migrations(plan: &serde_json::Map<String, Value>) -> Result<Vec<String>, Box<dyn Error>> {
    plan.get("planned_migrations")
        .and_then(Value::as_array)
        .ok_or("prepare plan must contain planned_migrations array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "planned migration names must be strings".into())
        })
        .collect()
}

fn validate_transaction_input(
    input: &TransactionIdentityInput,
    observation: &ProviderObservationBundle,
    prepare: &Value,
) -> Result<(), Box<dyn Error>> {
    if input.schema_version != TRANSACTION_SCHEMA_VERSION {
        return Err(format!(
            "transaction input schema_version must be {TRANSACTION_SCHEMA_VERSION}"
        )
        .into());
    }
    validate_target(&input.target)?;
    if input.target != observation.target {
        return Err("transaction target must exactly equal provider observation target".into());
    }
    for (value, label) in [
        (&input.source_sha, "source_sha"),
        (&input.tree_sha, "tree_sha"),
        (&input.planner_policy_digest, "planner_policy_digest"),
        (&input.migration_lineage_digest, "migration_lineage_digest"),
        (&input.predecessor_ledger_sha256, "predecessor_ledger_sha256"),
    ] {
        validate_sha256(value, label)?;
    }
    validate_non_empty(&input.release_candidate_id, "release_candidate_id")?;
    validate_non_empty(&input.transaction_kind, "transaction_kind")?;
    validate_non_empty(&input.phase, "phase")?;
    validate_non_empty(&input.schema_target, "schema_target")?;
    validate_non_empty(&input.supported_schema_min, "supported_schema_min")?;
    validate_non_empty(&input.supported_schema_max, "supported_schema_max")?;
    validate_non_empty(&input.recovery_strategy, "recovery_strategy")?;
    if input.freshness_max_age_seconds == 0 {
        return Err("freshness_max_age_seconds must be greater than zero".into());
    }
    if input.predecessor_ledger_sha256 != observation.remote_ledger_sha256 {
        return Err("predecessor ledger digest must equal the sealed provider observation ledger digest".into());
    }
    for (name, digest) in &input.release_manifest_digests {
        validate_non_empty(name, "release manifest digest name")?;
        validate_sha256(digest, "release manifest digest")?;
    }
    if input.release_manifest_digests.is_empty() {
        return Err("release_manifest_digests must not be empty".into());
    }
    let plan = prepare_plan(prepare)?;
    let expected = prepare_planned_migrations(plan)?;
    let supplied = input
        .planned_migrations
        .iter()
        .map(|migration| migration.migration_file.clone())
        .collect::<Vec<_>>();
    if supplied != expected {
        return Err("planned migration digest order must exactly equal PREPARE_READY planned_migrations".into());
    }
    let mut names = BTreeSet::new();
    for migration in &input.planned_migrations {
        validate_non_empty(&migration.migration_file, "planned migration filename")?;
        validate_sha256(&migration.content_sha256, "planned migration content digest")?;
        if !names.insert(&migration.migration_file) {
            return Err("planned_migrations must not contain duplicate filenames".into());
        }
    }
    if plan.get("history_digest").and_then(Value::as_str)
        != Some(input.migration_lineage_digest.as_str())
    {
        return Err("migration_lineage_digest must equal PREPARE_READY plan.history_digest".into());
    }
    if plan.get("target_revision").and_then(Value::as_str) != Some(input.schema_target.as_str()) {
        return Err("schema_target must equal PREPARE_READY plan.target_revision".into());
    }
    if input.allowed_provider_effects.is_empty() {
        return Err("allowed_provider_effects must not be empty".into());
    }
    if input.forbidden_provider_effects.is_empty() {
        return Err("forbidden_provider_effects must not be empty".into());
    }
    Ok(())
}

fn build_projection(
    prepare: &Value,
    observation_input: ProviderObservationInput,
    transaction_input: TransactionIdentityInput,
) -> Result<TransactionProjection, Box<dyn Error>> {
    let observation = seal_observation(observation_input)?;
    validate_transaction_input(&transaction_input, &observation, prepare)?;
    let transaction_plan = MigrationTransactionPlan {
        schema_version: transaction_input.schema_version,
        planner_policy_digest: transaction_input.planner_policy_digest,
        transaction_kind: transaction_input.transaction_kind,
        phase: transaction_input.phase,
        source_sha: transaction_input.source_sha,
        tree_sha: transaction_input.tree_sha,
        release_candidate_id: transaction_input.release_candidate_id,
        release_manifest_digests: transaction_input.release_manifest_digests,
        migration_lineage_digest: transaction_input.migration_lineage_digest,
        target: transaction_input.target,
        observation_digest: observation.observation_digest.clone(),
        observed_at_unix_seconds: observation.observed_at_unix_seconds,
        freshness_max_age_seconds: transaction_input.freshness_max_age_seconds,
        predecessor_ledger_sha256: transaction_input.predecessor_ledger_sha256,
        planned_migrations: transaction_input.planned_migrations,
        schema_target: transaction_input.schema_target,
        supported_schema_min: transaction_input.supported_schema_min,
        supported_schema_max: transaction_input.supported_schema_max,
        precondition_evidence_refs: transaction_input.precondition_evidence_refs,
        recovery_strategy: transaction_input.recovery_strategy,
        expected_post_state: transaction_input.expected_post_state,
        allowed_provider_effects: transaction_input.allowed_provider_effects,
        forbidden_provider_effects: transaction_input.forbidden_provider_effects,
    };
    let plan_value = serde_json::to_value(&transaction_plan)?;
    let canonical_plan = canonical_json(&plan_value)?;
    let transaction_id = sha256_hex(canonical_plan.as_bytes());
    Ok(TransactionProjection {
        schema_version: 1,
        status: "TRANSACTION_PREPARED",
        mode: "read-only",
        authorization_consumed: false,
        mutation_executed: false,
        provider_mutation_executed: false,
        provider_observation: observation,
        transaction_id,
        transaction_plan,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let prepare_path = required(args.prepare_json, "--prepare-json")?;
    let observation_path = required(args.observation_json, "--observation-json")?;
    let transaction_input_path = required(args.transaction_input_json, "--transaction-input-json")?;
    let prepare = read_strict_value(&prepare_path, "prepare input")?;
    let observation_value = read_strict_value(&observation_path, "provider observation")?;
    let transaction_value = read_strict_value(&transaction_input_path, "transaction identity input")?;
    let observation_input: ProviderObservationInput =
        typed_from_value(&observation_value, "provider observation")?;
    let transaction_input: TransactionIdentityInput =
        typed_from_value(&transaction_value, "transaction identity input")?;
    let projection = build_projection(&prepare, observation_input, transaction_input)?;
    let value = serde_json::to_value(projection)?;
    let output = canonical_json(&value)?;
    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PlannedMigrationDigest, ProviderObservationInput, TargetIdentity, TransactionIdentityInput,
        build_projection,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    fn target() -> TargetIdentity {
        TargetIdentity {
            environment: "rehearsal".to_owned(),
            account_id: "account-1".to_owned(),
            database_name: "d1-rehearsal".to_owned(),
            database_id: "database-1".to_owned(),
        }
    }

    fn prepare() -> serde_json::Value {
        json!({
            "status": "PREPARE_READY",
            "authorization_consumed": false,
            "mutation_executed": false,
            "provider_mutation_executed": false,
            "plan": {
                "history_digest": "11".repeat(32),
                "target_revision": "0031_device_binding_governance.sql",
                "planned_migrations": ["0031_device_binding_governance.sql"]
            }
        })
    }

    fn observation() -> ProviderObservationInput {
        ProviderObservationInput {
            schema_version: 1,
            target: target(),
            observed_at_unix_seconds: 1_788_640_000,
            observation_source: "fixture".to_owned(),
            remote_ledger_sha256: "22".repeat(32),
            remote_migrations: vec!["0030_profile_generation_successor_commit.sql".to_owned()],
            wrangler_pending_migrations: vec!["0031_device_binding_governance.sql".to_owned()],
            deployment_identity: Some("deployment-1".to_owned()),
            time_travel_bookmark_capable: true,
        }
    }

    fn transaction_input() -> TransactionIdentityInput {
        TransactionIdentityInput {
            schema_version: 1,
            source_sha: "33".repeat(32),
            tree_sha: "44".repeat(32),
            release_candidate_id: "release-set-v3-sha256-fixture".to_owned(),
            release_manifest_digests: BTreeMap::from([("catalog".to_owned(), "55".repeat(32))]),
            planner_policy_digest: "66".repeat(32),
            migration_lineage_digest: "11".repeat(32),
            transaction_kind: "D1_MIGRATION".to_owned(),
            phase: "ORDINARY".to_owned(),
            target: target(),
            freshness_max_age_seconds: 900,
            predecessor_ledger_sha256: "22".repeat(32),
            planned_migrations: vec![PlannedMigrationDigest {
                migration_file: "0031_device_binding_governance.sql".to_owned(),
                content_sha256: "77".repeat(32),
            }],
            schema_target: "0031_device_binding_governance.sql".to_owned(),
            supported_schema_min: "0031_device_binding_governance.sql".to_owned(),
            supported_schema_max: "0032_pas2_payload_fingerprint_contract.sql".to_owned(),
            precondition_evidence_refs: vec!["fixture:precondition".to_owned()],
            recovery_strategy: "NOOP_RETRY".to_owned(),
            expected_post_state: json!({"revision": "0031_device_binding_governance.sql"}),
            allowed_provider_effects: vec!["D1_MIGRATIONS_APPLY_EXACT_PLAN".to_owned()],
            forbidden_provider_effects: vec!["D1_CREATE".to_owned(), "PRODUCTION_MUTATION".to_owned()],
        }
    }

    #[test]
    fn identical_inputs_produce_identical_transaction_identity() -> Result<(), Box<dyn std::error::Error>> {
        let left = build_projection(&prepare(), observation(), transaction_input())?;
        let right = build_projection(&prepare(), observation(), transaction_input())?;
        assert_eq!(left.transaction_id, right.transaction_id);
        assert_eq!(left.transaction_plan, right.transaction_plan);
        assert!(!left.authorization_consumed);
        assert!(!left.mutation_executed);
        assert!(!left.provider_mutation_executed);
        Ok(())
    }

    #[test]
    fn provider_observation_drift_changes_transaction_identity() -> Result<(), Box<dyn std::error::Error>> {
        let baseline = build_projection(&prepare(), observation(), transaction_input())?;
        let mut changed_observation = observation();
        changed_observation.deployment_identity = Some("deployment-2".to_owned());
        let changed = build_projection(&prepare(), changed_observation, transaction_input())?;
        assert_ne!(baseline.transaction_id, changed.transaction_id);
        assert_ne!(
            baseline.provider_observation.observation_digest,
            changed.provider_observation.observation_digest
        );
        Ok(())
    }

    #[test]
    fn source_drift_changes_transaction_identity() -> Result<(), Box<dyn std::error::Error>> {
        let baseline = build_projection(&prepare(), observation(), transaction_input())?;
        let mut input = transaction_input();
        input.source_sha = "88".repeat(32);
        let changed = build_projection(&prepare(), observation(), input)?;
        assert_ne!(baseline.transaction_id, changed.transaction_id);
        Ok(())
    }

    #[test]
    fn plan_drift_is_rejected_instead_of_silently_replanned() {
        let mut input = transaction_input();
        input.planned_migrations[0].migration_file = "0032_pas2_payload_fingerprint_contract.sql".to_owned();
        let result = build_projection(&prepare(), observation(), input);
        assert!(result.is_err());
    }

    #[test]
    fn target_drift_is_rejected() {
        let mut input = transaction_input();
        input.target.database_id = "different-database".to_owned();
        let result = build_projection(&prepare(), observation(), input);
        assert!(result.is_err());
    }

    #[test]
    fn blocked_prepare_cannot_produce_transaction_identity() {
        let mut blocked = prepare();
        blocked["status"] = json!("PREPARE_BLOCKED");
        let result = build_projection(&blocked, observation(), transaction_input());
        assert!(result.is_err());
    }
}
