use super::model::D1Error;
use crate::canonical::{canonical_json, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const OBSERVATION_SCHEMA_VERSION: u64 = 1;
const TRANSACTION_SCHEMA_VERSION: u64 = 1;
const RELEASE_SET_PREFIX: &str = "release-set-v3-sha256-";
const ALLOWED_PROVIDER_EFFECT: &str = "D1_MIGRATIONS_APPLY_EXACT_PLAN";
const FORBIDDEN_PROVIDER_EFFECTS: [&str; 5] = [
    "D1_CREATE",
    "D1_DELETE",
    "D1_TIME_TRAVEL_RESTORE",
    "RESOURCE_AUTO_PROVISION",
    "PRODUCTION_MUTATION",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentity {
    pub environment: String,
    pub account_id: String,
    pub database_name: String,
    pub database_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionKind {
    D1Migration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionPhase {
    Ordinary,
    Contract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryStrategy {
    NoopRetry,
    RollForward,
    FailForwardOnly,
    TimeTravelRestoreRequiresSeparateAuth,
    ManualRepairRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderObservationInput {
    pub schema_version: u64,
    pub target: TargetIdentity,
    pub observed_at_unix_seconds: i64,
    pub observation_source: String,
    pub remote_ledger_sha256: String,
    pub remote_migrations: Vec<String>,
    pub wrangler_pending_migrations: Vec<String>,
    pub deployment_identity: Option<String>,
    pub time_travel_bookmark_capable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedMigrationDigest {
    pub migration_file: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionIdentityInput {
    pub schema_version: u64,
    pub source_sha: String,
    pub tree_sha: String,
    pub release_candidate_id: String,
    pub release_manifest_digests: BTreeMap<String, String>,
    pub transaction_kind: TransactionKind,
    pub phase: TransactionPhase,
    pub target: TargetIdentity,
    pub freshness_max_age_seconds: u64,
    pub predecessor_ledger_sha256: String,
    pub planned_migrations: Vec<PlannedMigrationDigest>,
    pub precondition_evidence_refs: Vec<String>,
    pub recovery_strategy: RecoveryStrategy,
    pub expected_post_state: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderObservationBundle {
    pub schema_version: u64,
    pub observation_digest: String,
    pub target: TargetIdentity,
    pub observed_at_unix_seconds: i64,
    pub observation_source: String,
    pub remote_ledger_sha256: String,
    pub remote_migrations: Vec<String>,
    pub wrangler_pending_migrations: Vec<String>,
    pub deployment_identity: Option<String>,
    pub time_travel_bookmark_capable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationTransactionPlan {
    pub schema_version: u64,
    pub repository_identity_sha256: String,
    pub planner_policy_digest: String,
    pub transaction_kind: TransactionKind,
    pub phase: TransactionPhase,
    pub source_sha: String,
    pub tree_sha: String,
    pub release_candidate_id: String,
    pub release_manifest_digests: BTreeMap<String, String>,
    pub migration_lineage_digest: String,
    pub target: TargetIdentity,
    pub observation_digest: String,
    pub observed_at_unix_seconds: i64,
    pub freshness_max_age_seconds: u64,
    pub predecessor_ledger_sha256: String,
    pub planned_migrations: Vec<PlannedMigrationDigest>,
    pub schema_target: String,
    pub supported_schema_min: String,
    pub supported_schema_max: String,
    pub precondition_evidence_refs: Vec<String>,
    pub recovery_strategy: RecoveryStrategy,
    pub expected_post_state: Value,
    pub allowed_provider_effects: Vec<String>,
    pub forbidden_provider_effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionProjection {
    pub schema_version: u64,
    pub status: String,
    pub mode: String,
    pub authorization_consumed: bool,
    pub mutation_executed: bool,
    pub provider_mutation_executed: bool,
    pub provider_observation: ProviderObservationBundle,
    pub transaction_id: String,
    pub transaction_plan: MigrationTransactionPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepositoryIdentity {
    repository_identity_sha256: String,
    component: String,
    history_digest: String,
    policy_digest: String,
    schema_target: String,
    supported_schema_min: String,
    supported_schema_max: String,
}

pub fn build_transaction_projection(
    prepare: &Value,
    observation_value: &Value,
    repository_value: &Value,
    transaction_value: &Value,
) -> Result<TransactionProjection, D1Error> {
    let observation_input: ProviderObservationInput =
        typed(observation_value, "provider observation")?;
    let transaction_input: TransactionIdentityInput =
        typed(transaction_value, "transaction identity input")?;
    let observation = seal_observation(observation_input)?;
    let plan = prepare_plan(prepare)?;
    let repository = repository_identity(repository_value, plan)?;
    validate_transaction_input(&transaction_input, &observation, plan, &repository)?;

    let transaction_plan = MigrationTransactionPlan {
        schema_version: transaction_input.schema_version,
        repository_identity_sha256: repository.repository_identity_sha256,
        planner_policy_digest: repository.policy_digest,
        transaction_kind: transaction_input.transaction_kind,
        phase: transaction_input.phase,
        source_sha: transaction_input.source_sha,
        tree_sha: transaction_input.tree_sha,
        release_candidate_id: transaction_input.release_candidate_id,
        release_manifest_digests: transaction_input.release_manifest_digests,
        migration_lineage_digest: repository.history_digest,
        target: transaction_input.target,
        observation_digest: observation.observation_digest.clone(),
        observed_at_unix_seconds: observation.observed_at_unix_seconds,
        freshness_max_age_seconds: transaction_input.freshness_max_age_seconds,
        predecessor_ledger_sha256: transaction_input.predecessor_ledger_sha256,
        planned_migrations: transaction_input.planned_migrations,
        schema_target: repository.schema_target,
        supported_schema_min: repository.supported_schema_min,
        supported_schema_max: repository.supported_schema_max,
        precondition_evidence_refs: transaction_input.precondition_evidence_refs,
        recovery_strategy: transaction_input.recovery_strategy,
        expected_post_state: transaction_input.expected_post_state,
        allowed_provider_effects: vec![ALLOWED_PROVIDER_EFFECT.to_owned()],
        forbidden_provider_effects: FORBIDDEN_PROVIDER_EFFECTS
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    };
    let canonical_plan =
        canonical_json(&serde_json::to_value(&transaction_plan).map_err(|error| {
            D1Error::new(format!(
                "cannot serialize migration transaction plan: {error}"
            ))
        })?)
        .map_err(D1Error::new)?;
    let transaction_id = sha256_hex(canonical_plan.as_bytes());

    Ok(TransactionProjection {
        schema_version: TRANSACTION_SCHEMA_VERSION,
        status: "TRANSACTION_PREPARED".to_owned(),
        mode: "read-only".to_owned(),
        authorization_consumed: false,
        mutation_executed: false,
        provider_mutation_executed: false,
        provider_observation: observation,
        transaction_id,
        transaction_plan,
    })
}

pub fn serialize_transaction_projection(
    projection: &TransactionProjection,
) -> Result<String, D1Error> {
    let value = serde_json::to_value(projection).map_err(|error| {
        D1Error::new(format!("cannot serialize transaction projection: {error}"))
    })?;
    canonical_json(&value).map_err(D1Error::new)
}

fn typed<T>(value: &Value, label: &str) -> Result<T, D1Error>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value.clone()).map_err(|error| {
        D1Error::new(format!(
            "{label} does not match the typed contract: {error}"
        ))
    })
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_sha256(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 64 || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "{label} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_git_object_id(value: &str, label: &str) -> Result<(), D1Error> {
    if !matches!(value.len(), 40 | 64) || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "{label} must be a 40- or 64-character lowercase Git object id"
        )));
    }
    Ok(())
}

fn validate_release_candidate_id(value: &str) -> Result<(), D1Error> {
    let digest = value.strip_prefix(RELEASE_SET_PREFIX).ok_or_else(|| {
        D1Error::new(format!(
            "release_candidate_id must start with {RELEASE_SET_PREFIX}"
        ))
    })?;
    validate_sha256(digest, "release_candidate_id digest")
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), D1Error> {
    if value.trim().is_empty() {
        return Err(D1Error::new(format!("{label} must not be empty")));
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity) -> Result<(), D1Error> {
    validate_non_empty(&target.environment, "target.environment")?;
    validate_non_empty(&target.account_id, "target.account_id")?;
    validate_non_empty(&target.database_name, "target.database_name")?;
    validate_non_empty(&target.database_id, "target.database_id")?;
    Ok(())
}

fn seal_observation(input: ProviderObservationInput) -> Result<ProviderObservationBundle, D1Error> {
    if input.schema_version != OBSERVATION_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "provider observation schema_version must be {OBSERVATION_SCHEMA_VERSION}"
        )));
    }
    validate_target(&input.target)?;
    validate_non_empty(&input.observation_source, "observation_source")?;
    validate_sha256(&input.remote_ledger_sha256, "remote_ledger_sha256")?;
    if input.observed_at_unix_seconds <= 0 {
        return Err(D1Error::new("observed_at_unix_seconds must be positive"));
    }
    if input
        .remote_migrations
        .iter()
        .collect::<BTreeSet<_>>()
        .len()
        != input.remote_migrations.len()
    {
        return Err(D1Error::new(
            "remote_migrations must not contain duplicates",
        ));
    }
    if input
        .wrangler_pending_migrations
        .iter()
        .collect::<BTreeSet<_>>()
        .len()
        != input.wrangler_pending_migrations.len()
    {
        return Err(D1Error::new(
            "wrangler_pending_migrations must not contain duplicates",
        ));
    }
    let canonical = canonical_json(&serde_json::to_value(&input).map_err(|error| {
        D1Error::new(format!("cannot serialize provider observation: {error}"))
    })?)
    .map_err(D1Error::new)?;
    Ok(ProviderObservationBundle {
        schema_version: input.schema_version,
        observation_digest: sha256_hex(canonical.as_bytes()),
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

fn prepare_plan(value: &Value) -> Result<&serde_json::Map<String, Value>, D1Error> {
    if value.get("status").and_then(Value::as_str) != Some("PREPARE_READY") {
        return Err(D1Error::new(
            "transaction identity requires PREPARE_READY input",
        ));
    }
    for field in [
        "authorization_consumed",
        "mutation_executed",
        "provider_mutation_executed",
    ] {
        if value.get(field).and_then(Value::as_bool) != Some(false) {
            return Err(D1Error::new(format!(
                "prepare.{field} must be false before transaction identity"
            )));
        }
    }
    value
        .get("plan")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("PREPARE_READY input must contain a plan object"))
}

fn repository_identity(
    repository: &Value,
    plan: &serde_json::Map<String, Value>,
) -> Result<RepositoryIdentity, D1Error> {
    let repository_identity_sha256 =
        required_str(repository, "repository_identity_sha256")?.to_owned();
    validate_sha256(&repository_identity_sha256, "repository_identity_sha256")?;
    let component = required_str_from_map(plan, "component")?;
    let components = repository
        .get("components")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("D1 repository projection is missing components"))?;
    let authority = components
        .iter()
        .find(|candidate| candidate.get("component_id").and_then(Value::as_str) == Some(component))
        .ok_or_else(|| D1Error::new(format!("D1 repository projection is missing {component}")))?;
    let release = authority
        .get("release_schema_contract")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("component projection is missing release_schema_contract"))?;
    let history_digest = required_str(authority, "history_digest")?.to_owned();
    let policy_digest = required_str(authority, "compatibility_policy_digest")?.to_owned();
    validate_sha256(&history_digest, "repository history_digest")?;
    validate_sha256(&policy_digest, "repository compatibility_policy_digest")?;
    Ok(RepositoryIdentity {
        repository_identity_sha256,
        component: component.to_owned(),
        history_digest,
        policy_digest,
        schema_target: required_str_from_map(release, "target_schema_revision")?.to_owned(),
        supported_schema_min: required_str_from_map(release, "supported_schema_min")?.to_owned(),
        supported_schema_max: required_str_from_map(release, "supported_schema_max")?.to_owned(),
    })
}

fn validate_transaction_input(
    input: &TransactionIdentityInput,
    observation: &ProviderObservationBundle,
    plan: &serde_json::Map<String, Value>,
    repository: &RepositoryIdentity,
) -> Result<(), D1Error> {
    if input.schema_version != TRANSACTION_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "transaction input schema_version must be {TRANSACTION_SCHEMA_VERSION}"
        )));
    }
    if input.phase != TransactionPhase::Ordinary {
        return Err(D1Error::new(
            "ordinary PREPARE_READY input cannot be sealed as a CONTRACT transaction",
        ));
    }
    validate_target(&input.target)?;
    if input.target != observation.target {
        return Err(D1Error::new(
            "transaction target must exactly equal provider observation target",
        ));
    }
    validate_git_object_id(&input.source_sha, "source_sha")?;
    validate_git_object_id(&input.tree_sha, "tree_sha")?;
    validate_release_candidate_id(&input.release_candidate_id)?;
    validate_sha256(
        &input.predecessor_ledger_sha256,
        "predecessor_ledger_sha256",
    )?;
    if input.predecessor_ledger_sha256 != observation.remote_ledger_sha256 {
        return Err(D1Error::new(
            "predecessor ledger digest must equal the sealed provider observation ledger digest",
        ));
    }
    if input.freshness_max_age_seconds == 0 {
        return Err(D1Error::new(
            "freshness_max_age_seconds must be greater than zero",
        ));
    }
    if input.release_manifest_digests.is_empty() {
        return Err(D1Error::new("release_manifest_digests must not be empty"));
    }
    for (name, digest) in &input.release_manifest_digests {
        validate_non_empty(name, "release manifest digest name")?;
        validate_sha256(digest, "release manifest digest")?;
    }
    if required_str_from_map(plan, "history_digest")? != repository.history_digest {
        return Err(D1Error::new(
            "PREPARE_READY history_digest drifted from typed repository authority",
        ));
    }
    if required_str_from_map(plan, "target_revision")? != repository.schema_target {
        return Err(D1Error::new(
            "PREPARE_READY target_revision drifted from typed release schema authority",
        ));
    }
    if required_str_from_map(plan, "component")? != repository.component {
        return Err(D1Error::new(
            "PREPARE_READY component drifted from typed repository authority",
        ));
    }
    let expected = expected_planned_migrations(plan)?;
    if observation.wrangler_pending_migrations != expected {
        return Err(D1Error::new(
            "sealed Wrangler pending migrations must exactly equal PREPARE_READY planned_migrations",
        ));
    }
    let supplied = input
        .planned_migrations
        .iter()
        .map(|migration| migration.migration_file.clone())
        .collect::<Vec<_>>();
    if supplied != expected {
        return Err(D1Error::new(
            "planned migration digest order must exactly equal PREPARE_READY planned_migrations",
        ));
    }
    let mut names = BTreeSet::new();
    for migration in &input.planned_migrations {
        validate_non_empty(&migration.migration_file, "planned migration filename")?;
        validate_sha256(
            &migration.content_sha256,
            "planned migration content digest",
        )?;
        if !names.insert(&migration.migration_file) {
            return Err(D1Error::new(
                "planned_migrations must not contain duplicate filenames",
            ));
        }
    }
    let mut evidence = BTreeSet::new();
    for evidence_ref in &input.precondition_evidence_refs {
        validate_non_empty(evidence_ref, "precondition evidence reference")?;
        if !evidence.insert(evidence_ref) {
            return Err(D1Error::new(
                "precondition_evidence_refs must not contain duplicates",
            ));
        }
    }
    if !input.expected_post_state.is_object() {
        return Err(D1Error::new("expected_post_state must be a JSON object"));
    }
    Ok(())
}

fn expected_planned_migrations(
    plan: &serde_json::Map<String, Value>,
) -> Result<Vec<String>, D1Error> {
    plan.get("planned_migrations")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("prepare plan must contain planned_migrations array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| D1Error::new("planned migration names must be strings"))
        })
        .collect()
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, D1Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new(format!("typed projection is missing {field}")))
}

fn required_str_from_map<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str, D1Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new(format!("typed projection is missing {field}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn target() -> TargetIdentity {
        TargetIdentity {
            environment: "rehearsal".to_owned(),
            account_id: "account-1".to_owned(),
            database_name: "d1-rehearsal".to_owned(),
            database_id: "database-1".to_owned(),
        }
    }

    fn prepare() -> Value {
        json!({
            "status": "PREPARE_READY",
            "authorization_consumed": false,
            "mutation_executed": false,
            "provider_mutation_executed": false,
            "plan": {
                "component": "catalog",
                "history_digest": "11".repeat(32),
                "target_revision": "0031_device_binding_governance.sql",
                "planned_migrations": ["0031_device_binding_governance.sql"]
            }
        })
    }

    fn repository() -> Value {
        json!({
            "repository_identity_sha256": "99".repeat(32),
            "components": [{
                "component_id": "catalog",
                "history_digest": "11".repeat(32),
                "compatibility_policy_digest": "aa".repeat(32),
                "release_schema_contract": {
                    "target_schema_revision": "0031_device_binding_governance.sql",
                    "supported_schema_min": "0031_device_binding_governance.sql",
                    "supported_schema_max": "0032_pas2_payload_fingerprint_contract.sql"
                }
            }]
        })
    }

    fn observation() -> Value {
        serde_json::to_value(ProviderObservationInput {
            schema_version: 1,
            target: target(),
            observed_at_unix_seconds: 1_788_640_000,
            observation_source: "fixture".to_owned(),
            remote_ledger_sha256: "22".repeat(32),
            remote_migrations: vec!["0030_profile_generation_successor_commit.sql".to_owned()],
            wrangler_pending_migrations: vec!["0031_device_binding_governance.sql".to_owned()],
            deployment_identity: Some("deployment-1".to_owned()),
            time_travel_bookmark_capable: true,
        })
        .unwrap()
    }

    fn transaction_input() -> Value {
        serde_json::to_value(TransactionIdentityInput {
            schema_version: 1,
            source_sha: "33".repeat(20),
            tree_sha: "44".repeat(20),
            release_candidate_id: format!("release-set-v3-sha256-{}", "55".repeat(32)),
            release_manifest_digests: BTreeMap::from([("catalog".to_owned(), "66".repeat(32))]),
            transaction_kind: TransactionKind::D1Migration,
            phase: TransactionPhase::Ordinary,
            target: target(),
            freshness_max_age_seconds: 900,
            predecessor_ledger_sha256: "22".repeat(32),
            planned_migrations: vec![PlannedMigrationDigest {
                migration_file: "0031_device_binding_governance.sql".to_owned(),
                content_sha256: "88".repeat(32),
            }],
            precondition_evidence_refs: vec!["fixture:precondition".to_owned()],
            recovery_strategy: RecoveryStrategy::NoopRetry,
            expected_post_state: json!({"revision": "0031_device_binding_governance.sql"}),
        })
        .unwrap()
    }

    fn build() -> TransactionProjection {
        build_transaction_projection(
            &prepare(),
            &observation(),
            &repository(),
            &transaction_input(),
        )
        .unwrap()
    }

    #[test]
    fn identical_inputs_produce_identical_transaction_identity() {
        let left = build();
        let right = build();
        assert_eq!(left.transaction_id, right.transaction_id);
        assert_eq!(left.transaction_plan, right.transaction_plan);
        assert_eq!(
            left.transaction_plan.repository_identity_sha256,
            "99".repeat(32)
        );
        assert_eq!(left.transaction_plan.planner_policy_digest, "aa".repeat(32));
        assert_eq!(
            left.transaction_plan.migration_lineage_digest,
            "11".repeat(32)
        );
        assert!(!left.authorization_consumed);
        assert!(!left.mutation_executed);
        assert!(!left.provider_mutation_executed);
    }

    #[test]
    fn provider_observation_drift_changes_transaction_identity() {
        let baseline = build();
        let mut changed = observation();
        changed["deployment_identity"] = json!("deployment-2");
        let changed =
            build_transaction_projection(&prepare(), &changed, &repository(), &transaction_input())
                .unwrap();
        assert_ne!(baseline.transaction_id, changed.transaction_id);
    }

    #[test]
    fn source_drift_changes_transaction_identity() {
        let baseline = build();
        let mut changed = transaction_input();
        changed["source_sha"] = json!("77".repeat(20));
        let changed =
            build_transaction_projection(&prepare(), &observation(), &repository(), &changed)
                .unwrap();
        assert_ne!(baseline.transaction_id, changed.transaction_id);
    }

    #[test]
    fn repository_identity_drift_changes_transaction_identity() {
        let baseline = build();
        let mut changed = repository();
        changed["repository_identity_sha256"] = json!("bb".repeat(32));
        let changed = build_transaction_projection(
            &prepare(),
            &observation(),
            &changed,
            &transaction_input(),
        )
        .unwrap();
        assert_ne!(baseline.transaction_id, changed.transaction_id);
    }

    #[test]
    fn repository_policy_drift_changes_transaction_identity() {
        let baseline = build();
        let mut changed = repository();
        changed["components"][0]["compatibility_policy_digest"] = json!("bb".repeat(32));
        let changed = build_transaction_projection(
            &prepare(),
            &observation(),
            &changed,
            &transaction_input(),
        )
        .unwrap();
        assert_ne!(baseline.transaction_id, changed.transaction_id);
    }

    #[test]
    fn repository_lineage_drift_is_rejected() {
        let mut changed = repository();
        changed["components"][0]["history_digest"] = json!("cc".repeat(32));
        assert!(
            build_transaction_projection(
                &prepare(),
                &observation(),
                &changed,
                &transaction_input(),
            )
            .is_err()
        );
    }

    #[test]
    fn plan_drift_is_rejected_instead_of_silently_replanned() {
        let mut changed = transaction_input();
        changed["planned_migrations"][0]["migration_file"] =
            json!("0032_pas2_payload_fingerprint_contract.sql");
        assert!(
            build_transaction_projection(&prepare(), &observation(), &repository(), &changed)
                .is_err()
        );
    }

    #[test]
    fn wrangler_pending_drift_is_rejected() {
        let mut changed = observation();
        changed["wrangler_pending_migrations"] = json!([]);
        assert!(
            build_transaction_projection(
                &prepare(),
                &changed,
                &repository(),
                &transaction_input(),
            )
            .is_err()
        );
    }

    #[test]
    fn target_drift_is_rejected() {
        let mut changed = transaction_input();
        changed["target"]["database_id"] = json!("different-database");
        assert!(
            build_transaction_projection(&prepare(), &observation(), &repository(), &changed)
                .is_err()
        );
    }

    #[test]
    fn contract_phase_cannot_be_forged_from_ordinary_prepare() {
        let mut changed = transaction_input();
        changed["phase"] = json!("CONTRACT");
        assert!(
            build_transaction_projection(&prepare(), &observation(), &repository(), &changed)
                .is_err()
        );
    }

    #[test]
    fn blocked_prepare_cannot_produce_transaction_identity() {
        let mut blocked = prepare();
        blocked["status"] = json!("PREPARE_BLOCKED");
        assert!(
            build_transaction_projection(
                &blocked,
                &observation(),
                &repository(),
                &transaction_input(),
            )
            .is_err()
        );
    }

    #[test]
    fn provider_observation_rejects_policy_verdict_fields() {
        let mut changed = observation();
        changed["allowed"] = json!(true);
        assert!(
            build_transaction_projection(
                &prepare(),
                &changed,
                &repository(),
                &transaction_input(),
            )
            .is_err()
        );
    }
}
