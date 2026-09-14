use super::authorization::{
    bind_current_reconstruction_authorization, bind_transaction_authorization,
};
use super::model::{D1Error, GateResult};
use super::reconstruction;
use super::transaction::{
    PlannedMigrationDigest, TargetIdentity, TransactionPhase, TransactionProjection,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const EXECUTOR_ADMISSION_SCHEMA_VERSION: u64 = 1;
const ADMISSION_DRIFT_REMEDIATION: &str = "Do not consume or broaden provider authority. Discard the drifted admission attempt, establish fresh exact protected-main source/tree and immutable TransactionId/target identity, then re-observe/re-prepare and obtain a new exact authorization if a write is still required.";
const RECONSTRUCTION_ADMISSION_DRIFT_REMEDIATION: &str = "Do not consume or broaden provider authority. Discard the drifted reconstruction admission attempt, establish fresh exact protected-main source/tree and immutable reconstruction identity/target, then re-observe/re-prepare and obtain a new exact authorization if reconstruction is still required.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorAdmissionExpectation {
    pub transaction_id: String,
    pub source_sha: String,
    pub tree_sha: String,
    pub component: String,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedExecutionPlan {
    pub schema_version: u64,
    pub command: String,
    pub mode: String,
    pub mutation_executed: bool,
    pub component: String,
    pub allowed: bool,
    pub predecessor_ledger_sha256: String,
    pub predecessor_migrations: Vec<String>,
    pub planned_migrations: Vec<String>,
    pub planned_migration_digests: Vec<PlannedMigrationDigest>,
    pub apply_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorAdmissionBinding {
    pub schema_version: u64,
    pub status: String,
    pub mode: String,
    pub authorization_consumed: bool,
    pub mutation_executed: bool,
    pub provider_mutation_executed: bool,
    pub transaction_id: String,
    pub authorization_digest: String,
    pub source_sha: String,
    pub tree_sha: String,
    pub component: String,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub evaluated_at_unix_seconds: i64,
    pub execution_plan: SealedExecutionPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedReconstructionExecutionPlan {
    pub schema_version: u64,
    pub kind: String,
    pub mode: String,
    pub mutation_executed: bool,
    pub component: String,
    pub allowed: bool,
    pub provider_effect: String,
    pub predecessor_ledger_sha256: String,
    pub predecessor_migrations: Vec<String>,
    pub repository_identity_sha256: String,
    pub construction_sha256: String,
    pub target_schema_revision: String,
    pub expected_ledger_migrations: Vec<String>,
    pub apply_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionExecutorAdmissionBinding {
    pub schema_version: u64,
    pub status: String,
    pub mode: String,
    pub authorization_consumed: bool,
    pub mutation_executed: bool,
    pub provider_mutation_executed: bool,
    pub operation_id: String,
    pub authorization_digest: String,
    pub source_sha: String,
    pub tree_sha: String,
    pub component: String,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub evaluated_at_unix_seconds: i64,
    pub execution_plan: SealedReconstructionExecutionPlan,
}

pub fn bind_executor_admission(
    transaction: &TransactionProjection,
    authorization_value: &Value,
    evaluated_at_unix_seconds: i64,
    expectation: &ExecutorAdmissionExpectation,
) -> Result<ExecutorAdmissionBinding, D1Error> {
    validate_sha256(&expectation.transaction_id, "expected_transaction_id")?;
    validate_git_object_id(&expectation.source_sha, "expected_source_sha")?;
    validate_git_object_id(&expectation.tree_sha, "expected_tree_sha")?;
    validate_component(&expectation.component)?;
    validate_target(&expectation.target)?;

    if transaction.transaction_id != expectation.transaction_id {
        return Err(admission_drift(
            "executor expected_transaction_id must exactly equal prepared transaction_id",
        ));
    }
    if transaction.transaction_plan.source_sha != expectation.source_sha {
        return Err(admission_drift(
            "executor exact checkout source_sha must equal prepared transaction source_sha",
        ));
    }
    if transaction.transaction_plan.tree_sha != expectation.tree_sha {
        return Err(admission_drift(
            "executor exact checkout tree_sha must equal prepared transaction tree_sha",
        ));
    }
    if transaction.transaction_plan.target != expectation.target {
        return Err(admission_drift(
            "executor exact target must equal prepared transaction target",
        ));
    }
    if transaction.transaction_plan.phase != expectation.phase {
        return Err(admission_drift(
            "executor expected phase must equal prepared transaction phase",
        ));
    }
    if transaction.transaction_plan.release_manifest_digests.len() != 1
        || !transaction
            .transaction_plan
            .release_manifest_digests
            .contains_key(&expectation.component)
    {
        return Err(admission_drift(
            "executor expected component must be the sole release-manifest component sealed by the prepared transaction",
        ));
    }

    let authorization = bind_transaction_authorization(
        transaction,
        authorization_value,
        evaluated_at_unix_seconds,
    )?;
    if authorization.transaction_id != expectation.transaction_id
        || authorization.target != expectation.target
        || authorization.phase != expectation.phase
    {
        return Err(admission_drift(
            "verified authorization binding drifted from executor admission expectation",
        ));
    }

    let planned_migration_digests = transaction.transaction_plan.planned_migrations.clone();
    let planned_migrations = planned_migration_digests
        .iter()
        .map(|migration| migration.migration_file.clone())
        .collect::<Vec<_>>();
    let execution_plan = SealedExecutionPlan {
        schema_version: 1,
        command: "d1 plan".to_owned(),
        mode: "read-only".to_owned(),
        mutation_executed: false,
        component: expectation.component.clone(),
        allowed: true,
        predecessor_ledger_sha256: transaction
            .transaction_plan
            .predecessor_ledger_sha256
            .clone(),
        predecessor_migrations: transaction.provider_observation.remote_migrations.clone(),
        apply_required: !planned_migrations.is_empty(),
        planned_migrations,
        planned_migration_digests,
    };

    Ok(ExecutorAdmissionBinding {
        schema_version: EXECUTOR_ADMISSION_SCHEMA_VERSION,
        status: "EXECUTOR_ADMISSION_VERIFIED".to_owned(),
        mode: "read-only".to_owned(),
        authorization_consumed: false,
        mutation_executed: false,
        provider_mutation_executed: false,
        transaction_id: authorization.transaction_id,
        authorization_digest: authorization.authorization_digest,
        source_sha: expectation.source_sha.clone(),
        tree_sha: expectation.tree_sha.clone(),
        component: expectation.component.clone(),
        target: expectation.target.clone(),
        phase: expectation.phase,
        evaluated_at_unix_seconds,
        execution_plan,
    })
}

pub fn bind_current_reconstruction_executor_admission(
    reconstruction_value: &Value,
    authorization_value: &Value,
    evaluated_at_unix_seconds: i64,
    expectation: &ExecutorAdmissionExpectation,
) -> Result<ReconstructionExecutorAdmissionBinding, D1Error> {
    validate_reconstruction_expectation(expectation)?;
    let subject = reconstruction::authorization_subject(reconstruction_value).map_err(|_| {
        reconstruction_admission_drift(
            "prepared CURRENT reconstruction failed exact revalidation before executor admission",
        )
    })?;
    let plan = reconstruction_value
        .get("plan")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            reconstruction_admission_drift(
                "prepared CURRENT reconstruction is missing its sealed plan",
            )
        })?;
    let source_sha = reconstruction_plan_string(plan, "source_sha")?;
    let tree_sha = reconstruction_plan_string(plan, "tree_sha")?;

    if subject.operation_id != expectation.transaction_id {
        return Err(reconstruction_admission_drift(
            "executor expected_transaction_id must exactly equal prepared reconstruction_id",
        ));
    }
    if source_sha != expectation.source_sha {
        return Err(reconstruction_admission_drift(
            "executor exact checkout source_sha must equal prepared reconstruction source_sha",
        ));
    }
    if tree_sha != expectation.tree_sha {
        return Err(reconstruction_admission_drift(
            "executor exact checkout tree_sha must equal prepared reconstruction tree_sha",
        ));
    }
    if subject.target != expectation.target {
        return Err(reconstruction_admission_drift(
            "executor exact target must equal prepared reconstruction target",
        ));
    }
    if expectation.phase != TransactionPhase::Ordinary {
        return Err(reconstruction_admission_drift(
            "CURRENT reconstruction executor admission phase must be ORDINARY",
        ));
    }

    let authorization = bind_current_reconstruction_authorization(
        reconstruction_value,
        authorization_value,
        evaluated_at_unix_seconds,
    )?;
    if authorization.transaction_id != expectation.transaction_id
        || authorization.target != expectation.target
        || authorization.phase != expectation.phase
        || authorization.authorized_provider_effects != subject.allowed_provider_effects
    {
        return Err(reconstruction_admission_drift(
            "verified reconstruction authorization drifted from executor admission expectation",
        ));
    }

    let provider_observation = plan
        .get("provider_observation")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            reconstruction_admission_drift(
                "prepared CURRENT reconstruction is missing provider_observation",
            )
        })?;
    let predecessor_ledger_sha256 =
        reconstruction_plan_string(provider_observation, "predecessor_ledger_sha256")?;
    let predecessor_migrations = reconstruction_string_array(
        provider_observation.get("remote_migrations"),
        "provider_observation.remote_migrations",
    )?;
    if !predecessor_migrations.is_empty() {
        return Err(reconstruction_admission_drift(
            "CURRENT reconstruction executor admission requires an exactly empty predecessor ledger",
        ));
    }

    let expected_post_state = plan
        .get("expected_post_state")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            reconstruction_admission_drift(
                "prepared CURRENT reconstruction is missing expected_post_state",
            )
        })?;
    let expected_ledger_migrations = reconstruction_string_array(
        expected_post_state.get("ledger_migrations"),
        "expected_post_state.ledger_migrations",
    )?;
    let repository_identity_sha256 =
        reconstruction_plan_string(plan, "repository_identity_sha256")?;
    let construction_sha256 = reconstruction_plan_string(plan, "construction_sha256")?;
    let target_schema_revision = reconstruction_plan_string(plan, "target_schema_revision")?;
    let provider_effect = subject
        .allowed_provider_effects
        .first()
        .cloned()
        .ok_or_else(|| {
            reconstruction_admission_drift(
                "CURRENT reconstruction has no exact provider effect for executor admission",
            )
        })?;
    if subject.allowed_provider_effects.len() != 1
        || provider_effect != "D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION"
    {
        return Err(reconstruction_admission_drift(
            "CURRENT reconstruction executor admission provider effect drifted",
        ));
    }

    let execution_plan = SealedReconstructionExecutionPlan {
        schema_version: 1,
        kind: "D1_CURRENT_FRESH_ZERO_RECONSTRUCTION".to_owned(),
        mode: "read-only".to_owned(),
        mutation_executed: false,
        component: "catalog".to_owned(),
        allowed: true,
        provider_effect,
        predecessor_ledger_sha256: predecessor_ledger_sha256.to_owned(),
        predecessor_migrations,
        repository_identity_sha256: repository_identity_sha256.to_owned(),
        construction_sha256: construction_sha256.to_owned(),
        target_schema_revision: target_schema_revision.to_owned(),
        apply_required: !expected_ledger_migrations.is_empty(),
        expected_ledger_migrations,
    };
    if !execution_plan.apply_required {
        return Err(reconstruction_admission_drift(
            "CURRENT reconstruction executor admission cannot seal an empty target ledger",
        ));
    }

    Ok(ReconstructionExecutorAdmissionBinding {
        schema_version: EXECUTOR_ADMISSION_SCHEMA_VERSION,
        status: "RECONSTRUCTION_EXECUTOR_ADMISSION_VERIFIED".to_owned(),
        mode: "read-only".to_owned(),
        authorization_consumed: false,
        mutation_executed: false,
        provider_mutation_executed: false,
        operation_id: authorization.transaction_id,
        authorization_digest: authorization.authorization_digest,
        source_sha: expectation.source_sha.clone(),
        tree_sha: expectation.tree_sha.clone(),
        component: "catalog".to_owned(),
        target: expectation.target.clone(),
        phase: expectation.phase,
        evaluated_at_unix_seconds,
        execution_plan,
    })
}

pub fn serialize_executor_admission(binding: &ExecutorAdmissionBinding) -> Result<String, D1Error> {
    let value = serde_json::to_value(binding).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize executor admission binding: {error}"
        ))
    })?;
    crate::canonical::canonical_json(&value).map_err(D1Error::new)
}

pub fn serialize_current_reconstruction_executor_admission(
    binding: &ReconstructionExecutorAdmissionBinding,
) -> Result<String, D1Error> {
    let value = serde_json::to_value(binding).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize CURRENT reconstruction executor admission binding: {error}"
        ))
    })?;
    crate::canonical::canonical_json(&value).map_err(D1Error::new)
}

fn admission_drift(summary: impl Into<String>) -> D1Error {
    D1Error::blocked(GateResult::blocked(
        "EXECUTOR_ADMISSION",
        "d1.executor_admission.identity",
        "SOURCE_TREE_TRANSACTION_DRIFT",
        summary,
        Some("exact immutable prepared transaction identity equal to the executor admission expectation".to_owned()),
        None,
        ADMISSION_DRIFT_REMEDIATION,
    ))
}

fn reconstruction_admission_drift(summary: impl Into<String>) -> D1Error {
    D1Error::blocked(GateResult::blocked(
        "EXECUTOR_ADMISSION",
        "d1.executor_admission.reconstruction_identity",
        "SOURCE_TREE_RECONSTRUCTION_DRIFT",
        summary,
        Some(
            "exact immutable prepared CURRENT reconstruction identity equal to the executor admission expectation"
                .to_owned(),
        ),
        None,
        RECONSTRUCTION_ADMISSION_DRIFT_REMEDIATION,
    ))
}

fn validate_component(component: &str) -> Result<(), D1Error> {
    if !matches!(component, "catalog" | "resolver") {
        return Err(admission_drift(
            "expected_component must be exactly catalog or resolver",
        ));
    }
    Ok(())
}

fn validate_reconstruction_expectation(
    expectation: &ExecutorAdmissionExpectation,
) -> Result<(), D1Error> {
    if validate_sha256(&expectation.transaction_id, "expected_transaction_id").is_err()
        || validate_git_object_id(&expectation.source_sha, "expected_source_sha").is_err()
        || validate_git_object_id(&expectation.tree_sha, "expected_tree_sha").is_err()
        || validate_target(&expectation.target).is_err()
    {
        return Err(reconstruction_admission_drift(
            "CURRENT reconstruction executor expectation has malformed identity fields",
        ));
    }
    if expectation.component != "catalog" {
        return Err(reconstruction_admission_drift(
            "CURRENT reconstruction executor component must be exactly catalog",
        ));
    }
    Ok(())
}

fn reconstruction_plan_string<'a>(
    plan: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str, D1Error> {
    plan.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            reconstruction_admission_drift(format!(
                "CURRENT reconstruction executor admission is missing {field}"
            ))
        })
}

fn reconstruction_string_array(
    value: Option<&Value>,
    label: &str,
) -> Result<Vec<String>, D1Error> {
    let values = value.and_then(Value::as_array).ok_or_else(|| {
        reconstruction_admission_drift(format!(
            "CURRENT reconstruction executor admission {label} must be an array"
        ))
    })?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    reconstruction_admission_drift(format!(
                        "CURRENT reconstruction executor admission {label} entries must be non-empty strings"
                    ))
                })
        })
        .collect()
}

fn validate_target(target: &TargetIdentity) -> Result<(), D1Error> {
    for (label, value) in [
        ("target.environment", target.environment.as_str()),
        ("target.account_id", target.account_id.as_str()),
        ("target.database_name", target.database_name.as_str()),
        ("target.database_id", target.database_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(admission_drift(format!("{label} must not be empty")));
        }
    }
    Ok(())
}

fn validate_git_object_id(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 40 || !is_lower_hex(value) {
        return Err(admission_drift(format!(
            "{label} must be exactly 40 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 64 || !is_lower_hex(value) {
        return Err(admission_drift(format!(
            "{label} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::super::transaction::{
        MigrationTransactionPlan, PlannedMigrationDigest, ProviderObservationBundle,
        ProviderObservationInput, RecoveryStrategy, TransactionKind,
    };
    use super::*;
    use crate::canonical::{canonical_json, sha256_hex};
    use serde_json::json;
    use std::collections::BTreeMap;

    const OBSERVED_AT: i64 = 1_788_640_000;
    const EVALUATED_AT: i64 = OBSERVED_AT + 20;

    fn target() -> TargetIdentity {
        TargetIdentity {
            environment: "rehearsal".to_owned(),
            account_id: "account-1".to_owned(),
            database_name: "d1-rehearsal".to_owned(),
            database_id: "database-1".to_owned(),
        }
    }

    fn transaction() -> Result<TransactionProjection, D1Error> {
        let target = target();
        let observation_input = ProviderObservationInput {
            schema_version: 1,
            target: target.clone(),
            observed_at_unix_seconds: OBSERVED_AT,
            observation_source: "fixture".to_owned(),
            remote_ledger_sha256: "22".repeat(32),
            remote_migrations: vec!["0030_profile_generation_successor_commit.sql".to_owned()],
            wrangler_pending_migrations: vec![
                "0031_device_binding_governance.sql".to_owned(),
                "0032_pas2_payload_fingerprint_contract.sql".to_owned(),
            ],
            deployment_identity: Some("deployment-1".to_owned()),
            time_travel_bookmark_capable: true,
        };
        let observation_value = serde_json::to_value(&observation_input).map_err(|error| {
            D1Error::new(format!("cannot serialize observation fixture: {error}"))
        })?;
        let canonical_observation = canonical_json(&observation_value).map_err(D1Error::new)?;
        let observation_digest = sha256_hex(canonical_observation.as_bytes());
        let provider_observation = ProviderObservationBundle {
            schema_version: observation_input.schema_version,
            observation_digest: observation_digest.clone(),
            target: observation_input.target,
            observed_at_unix_seconds: observation_input.observed_at_unix_seconds,
            observation_source: observation_input.observation_source,
            remote_ledger_sha256: observation_input.remote_ledger_sha256,
            remote_migrations: observation_input.remote_migrations,
            wrangler_pending_migrations: observation_input.wrangler_pending_migrations,
            deployment_identity: observation_input.deployment_identity,
            time_travel_bookmark_capable: observation_input.time_travel_bookmark_capable,
        };
        let transaction_plan = MigrationTransactionPlan {
            schema_version: 1,
            repository_identity_sha256: "44".repeat(32),
            planner_policy_digest: "55".repeat(32),
            transaction_kind: TransactionKind::D1Migration,
            phase: TransactionPhase::Ordinary,
            source_sha: "66".repeat(20),
            tree_sha: "77".repeat(20),
            release_candidate_id: format!("release-set-v3-sha256-{}", "88".repeat(32)),
            release_manifest_digests: BTreeMap::from([("catalog".to_owned(), "99".repeat(32))]),
            migration_lineage_digest: "aa".repeat(32),
            target,
            observation_digest,
            observed_at_unix_seconds: OBSERVED_AT,
            freshness_max_age_seconds: 900,
            predecessor_ledger_sha256: "22".repeat(32),
            planned_migrations: vec![PlannedMigrationDigest {
                migration_file: "0031_device_binding_governance.sql".to_owned(),
                content_sha256: "bb".repeat(32),
            }],
            schema_target: "0031_device_binding_governance.sql".to_owned(),
            supported_schema_min: "0031_device_binding_governance.sql".to_owned(),
            supported_schema_max: "0032_pas2_payload_fingerprint_contract.sql".to_owned(),
            precondition_evidence_refs: vec!["fixture:precondition".to_owned()],
            recovery_strategy: RecoveryStrategy::NoopRetry,
            expected_post_state: json!({"revision": "0031_device_binding_governance.sql"}),
            allowed_provider_effects: vec!["D1_MIGRATIONS_APPLY_EXACT_PLAN".to_owned()],
            forbidden_provider_effects: vec![
                "D1_CREATE".to_owned(),
                "D1_DELETE".to_owned(),
                "PRODUCTION_MUTATION".to_owned(),
            ],
        };
        let plan_value = serde_json::to_value(&transaction_plan).map_err(|error| {
            D1Error::new(format!("cannot serialize transaction fixture: {error}"))
        })?;
        let canonical_plan = canonical_json(&plan_value).map_err(D1Error::new)?;
        Ok(TransactionProjection {
            schema_version: 1,
            status: "TRANSACTION_PREPARED".to_owned(),
            mode: "read-only".to_owned(),
            authorization_consumed: false,
            mutation_executed: false,
            provider_mutation_executed: false,
            provider_observation,
            transaction_id: sha256_hex(canonical_plan.as_bytes()),
            transaction_plan,
        })
    }

    fn authorization(transaction: &TransactionProjection) -> Value {
        json!({
            "schema_version": 1,
            "transaction_id": transaction.transaction_id,
            "target": target(),
            "phase": "ORDINARY",
            "authorized_provider_effects": ["D1_MIGRATIONS_APPLY_EXACT_PLAN"],
            "issued_at_unix_seconds": OBSERVED_AT + 10,
            "expires_at_unix_seconds": OBSERVED_AT + 600,
            "observation_fresh_until_unix_seconds": OBSERVED_AT + 900,
            "authorization_reference": "issue:597:authorization-fixture"
        })
    }

    fn expectation(transaction: &TransactionProjection) -> ExecutorAdmissionExpectation {
        ExecutorAdmissionExpectation {
            transaction_id: transaction.transaction_id.clone(),
            source_sha: transaction.transaction_plan.source_sha.clone(),
            tree_sha: transaction.transaction_plan.tree_sha.clone(),
            component: "catalog".to_owned(),
            target: transaction.transaction_plan.target.clone(),
            phase: TransactionPhase::Ordinary,
        }
    }

    fn reason_code(error: &D1Error) -> Option<&str> {
        error
            .gate_result_json()
            .get("reason_code")
            .and_then(Value::as_str)
    }

    #[test]
    fn exact_executor_admission_binds_transaction_authorization_checkout_and_plan()
    -> Result<(), D1Error> {
        let transaction = transaction()?;
        let binding = bind_executor_admission(
            &transaction,
            &authorization(&transaction),
            EVALUATED_AT,
            &expectation(&transaction),
        )?;
        assert_eq!(binding.status, "EXECUTOR_ADMISSION_VERIFIED");
        assert_eq!(binding.transaction_id, transaction.transaction_id);
        assert_eq!(binding.source_sha, transaction.transaction_plan.source_sha);
        assert_eq!(binding.tree_sha, transaction.transaction_plan.tree_sha);
        assert_eq!(binding.component, "catalog");
        assert_eq!(binding.execution_plan.command, "d1 plan");
        assert_eq!(
            binding.execution_plan.predecessor_ledger_sha256,
            transaction.transaction_plan.predecessor_ledger_sha256
        );
        assert_eq!(
            binding.execution_plan.predecessor_migrations,
            transaction.provider_observation.remote_migrations
        );
        assert_eq!(
            binding.execution_plan.planned_migrations,
            vec!["0031_device_binding_governance.sql"]
        );
        assert_eq!(
            binding.execution_plan.planned_migration_digests,
            transaction.transaction_plan.planned_migrations
        );
        assert!(binding.execution_plan.apply_required);
        assert!(!binding.authorization_consumed);
        assert!(!binding.mutation_executed);
        assert!(!binding.provider_mutation_executed);
        Ok(())
    }

    #[test]
    fn source_checkout_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.source_sha = "ab".repeat(20);
        let error = bind_executor_admission(
            &transaction,
            &authorization(&transaction),
            EVALUATED_AT,
            &expected,
        )
        .err()
        .ok_or_else(|| D1Error::new("source checkout drift unexpectedly passed"))?;
        assert_eq!(reason_code(&error), Some("SOURCE_TREE_TRANSACTION_DRIFT"));
        Ok(())
    }

    #[test]
    fn tree_checkout_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.tree_sha = "cd".repeat(20);
        let error = bind_executor_admission(
            &transaction,
            &authorization(&transaction),
            EVALUATED_AT,
            &expected,
        )
        .err()
        .ok_or_else(|| D1Error::new("tree checkout drift unexpectedly passed"))?;
        assert_eq!(reason_code(&error), Some("SOURCE_TREE_TRANSACTION_DRIFT"));
        Ok(())
    }

    #[test]
    fn target_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.target.database_id = "database-2".to_owned();
        let error = bind_executor_admission(
            &transaction,
            &authorization(&transaction),
            EVALUATED_AT,
            &expected,
        )
        .err()
        .ok_or_else(|| D1Error::new("executor target identity drift unexpectedly passed"))?;
        assert_eq!(reason_code(&error), Some("SOURCE_TREE_TRANSACTION_DRIFT"));
        Ok(())
    }

    #[test]
    fn transaction_id_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.transaction_id = "ef".repeat(32);
        let error = bind_executor_admission(
            &transaction,
            &authorization(&transaction),
            EVALUATED_AT,
            &expected,
        )
        .err()
        .ok_or_else(|| D1Error::new("transaction id drift unexpectedly passed"))?;
        assert_eq!(reason_code(&error), Some("SOURCE_TREE_TRANSACTION_DRIFT"));
        Ok(())
    }

    #[test]
    fn component_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.component = "resolver".to_owned();
        let error = bind_executor_admission(
            &transaction,
            &authorization(&transaction),
            EVALUATED_AT,
            &expected,
        )
        .err()
        .ok_or_else(|| D1Error::new("component drift unexpectedly passed"))?;
        assert_eq!(reason_code(&error), Some("SOURCE_TREE_TRANSACTION_DRIFT"));
        Ok(())
    }
}
