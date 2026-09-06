use super::authorization::bind_transaction_authorization;
use super::model::D1Error;
use super::transaction::{
    PlannedMigrationDigest, TargetIdentity, TransactionPhase, TransactionProjection,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const EXECUTOR_ADMISSION_SCHEMA_VERSION: u64 = 1;

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
        return Err(D1Error::new(
            "executor expected_transaction_id must exactly equal prepared transaction_id",
        ));
    }
    if transaction.transaction_plan.source_sha != expectation.source_sha {
        return Err(D1Error::new(
            "executor exact checkout source_sha must equal prepared transaction source_sha",
        ));
    }
    if transaction.transaction_plan.tree_sha != expectation.tree_sha {
        return Err(D1Error::new(
            "executor exact checkout tree_sha must equal prepared transaction tree_sha",
        ));
    }
    if transaction.transaction_plan.target != expectation.target {
        return Err(D1Error::new(
            "executor exact target must equal prepared transaction target",
        ));
    }
    if transaction.transaction_plan.phase != expectation.phase {
        return Err(D1Error::new(
            "executor expected phase must equal prepared transaction phase",
        ));
    }
    if transaction.transaction_plan.release_manifest_digests.len() != 1
        || !transaction
            .transaction_plan
            .release_manifest_digests
            .contains_key(&expectation.component)
    {
        return Err(D1Error::new(
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
        return Err(D1Error::new(
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

pub fn serialize_executor_admission(binding: &ExecutorAdmissionBinding) -> Result<String, D1Error> {
    let value = serde_json::to_value(binding).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize executor admission binding: {error}"
        ))
    })?;
    crate::canonical::canonical_json(&value).map_err(D1Error::new)
}

fn validate_component(component: &str) -> Result<(), D1Error> {
    if !matches!(component, "catalog" | "resolver") {
        return Err(D1Error::new(
            "expected_component must be exactly catalog or resolver",
        ));
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity) -> Result<(), D1Error> {
    for (label, value) in [
        ("target.environment", target.environment.as_str()),
        ("target.account_id", target.account_id.as_str()),
        ("target.database_name", target.database_name.as_str()),
        ("target.database_id", target.database_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(D1Error::new(format!("{label} must not be empty")));
        }
    }
    Ok(())
}

fn validate_git_object_id(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 40 || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "{label} must be exactly 40 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 64 || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
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
            wrangler_pending_migrations: vec!["0031_device_binding_governance.sql".to_owned()],
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
        assert!(
            bind_executor_admission(
                &transaction,
                &authorization(&transaction),
                EVALUATED_AT,
                &expected,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn tree_checkout_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.tree_sha = "cd".repeat(20);
        assert!(
            bind_executor_admission(
                &transaction,
                &authorization(&transaction),
                EVALUATED_AT,
                &expected,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn target_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.target.database_id = "database-2".to_owned();
        assert!(
            bind_executor_admission(
                &transaction,
                &authorization(&transaction),
                EVALUATED_AT,
                &expected,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn transaction_id_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.transaction_id = "ef".repeat(32);
        assert!(
            bind_executor_admission(
                &transaction,
                &authorization(&transaction),
                EVALUATED_AT,
                &expected,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn component_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut expected = expectation(&transaction);
        expected.component = "resolver".to_owned();
        assert!(
            bind_executor_admission(
                &transaction,
                &authorization(&transaction),
                EVALUATED_AT,
                &expected,
            )
            .is_err()
        );
        Ok(())
    }
}
