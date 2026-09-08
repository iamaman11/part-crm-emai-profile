use super::execution_control::{ExecutionEventKind, ExecutionReceipt, serialize_execution_receipt};
use super::model::D1Error;
use super::transaction::{
    ProviderObservationInput, TargetIdentity, TransactionPhase, TransactionProjection,
};
use super::transaction_integrity::revalidate_transaction_projection;
use crate::canonical::canonical_json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const POST_STATE_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionPostStateDisposition {
    CompletedVerified,
    RecoveryRequiredConfirmed,
    FailedNoEffectVerified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPostStateVerification {
    pub schema_version: u64,
    pub status: String,
    pub disposition: ExecutionPostStateDisposition,
    pub transaction_id: String,
    pub receipt_id: String,
    pub target: TargetIdentity,
    pub terminal_state: ExecutionEventKind,
    pub expected_post_state_reached: bool,
    pub predecessor_migrations: Vec<String>,
    pub applied_migrations: Vec<String>,
    pub observed_migrations: Vec<String>,
    pub observed_pending_migrations: Vec<String>,
    pub post_remote_ledger_sha256: String,
    pub post_observation_source: String,
    pub post_observed_at_unix_seconds: i64,
    pub evaluated_at_unix_seconds: i64,
}

pub fn verify_execution_post_state(
    transaction: &TransactionProjection,
    receipt: &ExecutionReceipt,
    post_observation: &ProviderObservationInput,
    evaluated_at_unix_seconds: i64,
) -> Result<ExecutionPostStateVerification, D1Error> {
    revalidate_transaction_projection(transaction)?;
    serialize_execution_receipt(receipt)?;

    if post_observation.schema_version != POST_STATE_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "post-state provider observation schema_version must be {POST_STATE_SCHEMA_VERSION}"
        )));
    }
    validate_target(
        &post_observation.target,
        "post-state provider observation target",
    )?;
    validate_non_empty(
        &post_observation.observation_source,
        "post-state provider observation_source",
    )?;
    validate_sha256(
        &post_observation.remote_ledger_sha256,
        "post-state remote_ledger_sha256",
    )?;
    validate_unique_strings(
        &post_observation.remote_migrations,
        "post-state remote_migrations",
    )?;
    validate_unique_strings(
        &post_observation.wrangler_pending_migrations,
        "post-state wrangler_pending_migrations",
    )?;

    let plan = &transaction.transaction_plan;
    let predecessor = &transaction.provider_observation;
    if receipt.phase != TransactionPhase::Ordinary {
        return Err(D1Error::new(
            "post-state verification requires an ORDINARY execution receipt",
        ));
    }
    if receipt.target != plan.target
        || predecessor.target != plan.target
        || post_observation.target != plan.target
    {
        return Err(D1Error::new(
            "post-state verification requires exact transaction/receipt/observation target equality",
        ));
    }
    if receipt.transaction_id.as_deref() != Some(transaction.transaction_id.as_str())
        || receipt.operation_identity != transaction.transaction_id
    {
        return Err(D1Error::new(
            "post-state execution receipt does not bind the prepared TransactionId",
        ));
    }
    if receipt.source_sha != plan.source_sha {
        return Err(D1Error::new(
            "post-state execution receipt source_sha does not match prepared transaction source",
        ));
    }
    if post_observation.deployment_identity != predecessor.deployment_identity {
        return Err(D1Error::new(
            "post-state deployment identity drifted from the sealed predecessor observation",
        ));
    }
    if post_observation.time_travel_bookmark_capable != predecessor.time_travel_bookmark_capable {
        return Err(D1Error::new(
            "post-state Time Travel capability drifted from the sealed predecessor observation",
        ));
    }

    let terminal_event = receipt.events.last().ok_or_else(|| {
        D1Error::new("post-state verification requires a terminal execution event")
    })?;
    let terminal_state = terminal_event.kind;
    if !matches!(
        terminal_state,
        ExecutionEventKind::Completed
            | ExecutionEventKind::RecoveryRequired
            | ExecutionEventKind::FailedNoEffect
    ) {
        return Err(D1Error::new(
            "post-state verification requires COMPLETED, RECOVERY_REQUIRED, or FAILED_NO_EFFECT receipt",
        ));
    }
    if evaluated_at_unix_seconds <= 0
        || post_observation.observed_at_unix_seconds <= 0
        || post_observation.observed_at_unix_seconds > evaluated_at_unix_seconds
    {
        return Err(D1Error::new(
            "post-state observation/evaluation timestamps must be positive and observation must not be from the future",
        ));
    }
    if post_observation.observed_at_unix_seconds < terminal_event.occurred_at_unix_seconds {
        return Err(D1Error::new(
            "post-state provider observation must not predate the terminal execution receipt event",
        ));
    }
    let age = evaluated_at_unix_seconds - post_observation.observed_at_unix_seconds;
    let max_age = i64::try_from(plan.freshness_max_age_seconds).map_err(|_| {
        D1Error::new("transaction freshness window exceeds supported timestamp range")
    })?;
    if age > max_age {
        return Err(D1Error::new(
            "post-state provider observation is stale under the prepared transaction freshness policy",
        ));
    }

    let planned = plan
        .planned_migrations
        .iter()
        .map(|migration| migration.migration_file.clone())
        .collect::<Vec<_>>();
    let applied = receipt
        .events
        .iter()
        .filter(|event| event.kind == ExecutionEventKind::MigrationApplied)
        .map(|event| {
            event
                .migration_id
                .clone()
                .ok_or_else(|| D1Error::new("MIGRATION_APPLIED receipt event lost migration_id"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if applied.len() > planned.len() || applied != planned[..applied.len()] {
        return Err(D1Error::new(
            "execution receipt applied migrations must be an exact prefix of the prepared migration plan",
        ));
    }
    if predecessor.wrangler_pending_migrations.len() < applied.len()
        || predecessor.wrangler_pending_migrations[..applied.len()] != applied
    {
        return Err(D1Error::new(
            "sealed predecessor Wrangler pending list does not begin with the receipt-applied migration prefix",
        ));
    }

    let mut expected_migrations = predecessor.remote_migrations.clone();
    expected_migrations.extend(applied.iter().cloned());
    let expected_pending = predecessor.wrangler_pending_migrations[applied.len()..].to_vec();
    if post_observation.remote_migrations != expected_migrations {
        return Err(D1Error::new(
            "fresh post-state remote migration ledger does not equal predecessor ledger plus receipt-applied migrations",
        ));
    }
    if post_observation.wrangler_pending_migrations != expected_pending {
        return Err(D1Error::new(
            "fresh post-state Wrangler pending list does not equal predecessor pending list minus receipt-applied migrations",
        ));
    }

    let expected_revision = plan
        .expected_post_state
        .get("revision")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            D1Error::new("prepared transaction expected_post_state.revision must be one string")
        })?;
    if expected_revision != plan.schema_target {
        return Err(D1Error::new(
            "prepared expected_post_state.revision drifted from typed transaction schema_target",
        ));
    }
    let expected_post_state_reached = post_observation
        .remote_migrations
        .last()
        .is_some_and(|revision| revision == expected_revision);

    let disposition = match terminal_state {
        ExecutionEventKind::Completed => {
            if applied != planned {
                return Err(D1Error::new(
                    "COMPLETED receipt must contain the entire prepared migration plan",
                ));
            }
            if !expected_post_state_reached {
                return Err(D1Error::new(
                    "COMPLETED receipt disagrees with fresh provider post-state revision",
                ));
            }
            ExecutionPostStateDisposition::CompletedVerified
        }
        ExecutionEventKind::FailedNoEffect => {
            if !applied.is_empty()
                || post_observation.remote_ledger_sha256 != predecessor.remote_ledger_sha256
            {
                return Err(D1Error::new(
                    "FAILED_NO_EFFECT receipt requires unchanged provider ledger identity and zero applied migrations",
                ));
            }
            ExecutionPostStateDisposition::FailedNoEffectVerified
        }
        ExecutionEventKind::RecoveryRequired => {
            ExecutionPostStateDisposition::RecoveryRequiredConfirmed
        }
        _ => unreachable!("terminal state filtered above"),
    };

    Ok(ExecutionPostStateVerification {
        schema_version: POST_STATE_SCHEMA_VERSION,
        status: "POST_STATE_VERIFIED".to_owned(),
        disposition,
        transaction_id: transaction.transaction_id.clone(),
        receipt_id: receipt.receipt_id.clone(),
        target: plan.target.clone(),
        terminal_state,
        expected_post_state_reached,
        predecessor_migrations: predecessor.remote_migrations.clone(),
        applied_migrations: applied,
        observed_migrations: post_observation.remote_migrations.clone(),
        observed_pending_migrations: post_observation.wrangler_pending_migrations.clone(),
        post_remote_ledger_sha256: post_observation.remote_ledger_sha256.clone(),
        post_observation_source: post_observation.observation_source.clone(),
        post_observed_at_unix_seconds: post_observation.observed_at_unix_seconds,
        evaluated_at_unix_seconds,
    })
}

pub fn serialize_execution_post_state_verification(
    verification: &ExecutionPostStateVerification,
) -> Result<String, D1Error> {
    canonical_json(&serde_json::to_value(verification).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize D1 execution post-state verification: {error}"
        ))
    })?)
    .map_err(D1Error::new)
}

fn validate_target(target: &TargetIdentity, label: &str) -> Result<(), D1Error> {
    validate_non_empty(&target.environment, &format!("{label}.environment"))?;
    validate_non_empty(&target.account_id, &format!("{label}.account_id"))?;
    validate_non_empty(&target.database_name, &format!("{label}.database_name"))?;
    validate_non_empty(&target.database_id, &format!("{label}.database_id"))
}

fn validate_unique_strings(values: &[String], label: &str) -> Result<(), D1Error> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_non_empty(value, label)?;
        if !unique.insert(value) {
            return Err(D1Error::new(format!("{label} must not contain duplicates")));
        }
    }
    Ok(())
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), D1Error> {
    if value.trim().is_empty() {
        return Err(D1Error::new(format!("{label} must not be empty")));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(D1Error::new(format!(
            "{label} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::execution_control::{
        ExecutionEventInput, ExecutionReceiptSeed, TargetFenceLeaseInput, acquire_target_fence,
        append_execution_event, initialize_execution_receipt,
    };
    use super::super::transaction::{RecoveryStrategy, TransactionPhase};
    use super::super::transaction_core::build_transaction_projection;
    use super::*;
    use serde_json::json;

    const T0: i64 = 1_788_700_000;

    fn transaction() -> Result<TransactionProjection, D1Error> {
        let prepare = json!({
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
        });
        let observation = json!({
            "schema_version": 1,
            "target": {
                "environment": "staging",
                "account_id": "account-1",
                "database_name": "d1-rehearsal",
                "database_id": "database-1"
            },
            "observed_at_unix_seconds": T0,
            "observation_source": "fixture:predecessor",
            "remote_ledger_sha256": "22".repeat(32),
            "remote_migrations": ["0030_profile_generation_successor_commit.sql"],
            "wrangler_pending_migrations": [
                "0031_device_binding_governance.sql",
                "0032_pas2_payload_fingerprint_contract.sql"
            ],
            "deployment_identity": null,
            "time_travel_bookmark_capable": true
        });
        let repository = json!({
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
        });
        let input = json!({
            "schema_version": 1,
            "source_sha": "33".repeat(20),
            "tree_sha": "44".repeat(20),
            "release_candidate_id": format!("release-set-v3-sha256-{}", "55".repeat(32)),
            "release_manifest_digests": {"catalog": "66".repeat(32)},
            "transaction_kind": "D1_MIGRATION",
            "phase": "ORDINARY",
            "target": {
                "environment": "staging",
                "account_id": "account-1",
                "database_name": "d1-rehearsal",
                "database_id": "database-1"
            },
            "freshness_max_age_seconds": 900,
            "predecessor_ledger_sha256": "22".repeat(32),
            "planned_migrations": [{
                "migration_file": "0031_device_binding_governance.sql",
                "content_sha256": "88".repeat(32)
            }],
            "precondition_evidence_refs": ["fixture:precondition"],
            "recovery_strategy": "NOOP_RETRY",
            "expected_post_state": {"revision": "0031_device_binding_governance.sql"}
        });
        build_transaction_projection(&prepare, &observation, &repository, &input)
    }

    fn append_fixture_event(
        receipt: &ExecutionReceipt,
        kind: ExecutionEventKind,
        occurred_at_unix_seconds: i64,
        migration_id: Option<&str>,
    ) -> Result<ExecutionReceipt, D1Error> {
        append_execution_event(
            receipt,
            ExecutionEventInput {
                kind,
                occurred_at_unix_seconds,
                migration_id: migration_id.map(str::to_owned),
            },
        )
    }

    fn receipt(
        transaction: &TransactionProjection,
        terminal: ExecutionEventKind,
    ) -> Result<ExecutionReceipt, D1Error> {
        let plan = &transaction.transaction_plan;
        let lease = acquire_target_fence(TargetFenceLeaseInput {
            schema_version: 1,
            target: plan.target.clone(),
            phase: TransactionPhase::Ordinary,
            operation_identity: transaction.transaction_id.clone(),
            transaction_id: Some(transaction.transaction_id.clone()),
            authorization_digest: Some("77".repeat(32)),
            source_sha: plan.source_sha.clone(),
            executor_run_id: 1234,
            fence_epoch: 42,
            run_attempt: 1,
            acquired_at_unix_seconds: T0 + 10,
        })?;
        let seed = ExecutionReceiptSeed {
            schema_version: 1,
            target: plan.target.clone(),
            phase: TransactionPhase::Ordinary,
            operation_identity: transaction.transaction_id.clone(),
            transaction_id: Some(transaction.transaction_id.clone()),
            authorization_digest: Some("77".repeat(32)),
            source_sha: plan.source_sha.clone(),
            recovery_strategy: RecoveryStrategy::NoopRetry,
            fence: lease,
        };
        let mut value = initialize_execution_receipt(seed, T0 + 20, T0 + 21)?;
        match terminal {
            ExecutionEventKind::Completed => {
                value = append_fixture_event(
                    &value,
                    ExecutionEventKind::PrewriteFencePass,
                    T0 + 22,
                    None,
                )?;
                value = append_fixture_event(
                    &value,
                    ExecutionEventKind::MutationStarted,
                    T0 + 23,
                    None,
                )?;
                value = append_fixture_event(
                    &value,
                    ExecutionEventKind::MigrationApplied,
                    T0 + 24,
                    Some("0031_device_binding_governance.sql"),
                )?;
                value =
                    append_fixture_event(&value, ExecutionEventKind::PostObserved, T0 + 25, None)?;
                value = append_fixture_event(&value, ExecutionEventKind::Verified, T0 + 26, None)?;
                append_fixture_event(&value, ExecutionEventKind::Completed, T0 + 27, None)
            }
            ExecutionEventKind::RecoveryRequired => {
                value = append_fixture_event(
                    &value,
                    ExecutionEventKind::PrewriteFencePass,
                    T0 + 22,
                    None,
                )?;
                value = append_fixture_event(
                    &value,
                    ExecutionEventKind::MutationStarted,
                    T0 + 23,
                    None,
                )?;
                value = append_fixture_event(
                    &value,
                    ExecutionEventKind::MigrationApplied,
                    T0 + 24,
                    Some("0031_device_binding_governance.sql"),
                )?;
                append_fixture_event(&value, ExecutionEventKind::RecoveryRequired, T0 + 25, None)
            }
            ExecutionEventKind::FailedNoEffect => {
                value = append_fixture_event(
                    &value,
                    ExecutionEventKind::PrewriteAborted,
                    T0 + 22,
                    None,
                )?;
                append_fixture_event(&value, ExecutionEventKind::FailedNoEffect, T0 + 23, None)
            }
            _ => Err(D1Error::new(
                "test receipt fixture supports only terminal execution states",
            )),
        }
    }

    fn post_observation(applied: bool) -> ProviderObservationInput {
        ProviderObservationInput {
            schema_version: 1,
            target: TargetIdentity {
                environment: "staging".to_owned(),
                account_id: "account-1".to_owned(),
                database_name: "d1-rehearsal".to_owned(),
                database_id: "database-1".to_owned(),
            },
            observed_at_unix_seconds: T0 + 40,
            observation_source: "fixture:post-state".to_owned(),
            remote_ledger_sha256: if applied {
                "33".repeat(32)
            } else {
                "22".repeat(32)
            },
            remote_migrations: if applied {
                vec![
                    "0030_profile_generation_successor_commit.sql".to_owned(),
                    "0031_device_binding_governance.sql".to_owned(),
                ]
            } else {
                vec!["0030_profile_generation_successor_commit.sql".to_owned()]
            },
            wrangler_pending_migrations: if applied {
                vec!["0032_pas2_payload_fingerprint_contract.sql".to_owned()]
            } else {
                vec![
                    "0031_device_binding_governance.sql".to_owned(),
                    "0032_pas2_payload_fingerprint_contract.sql".to_owned(),
                ]
            },
            deployment_identity: None,
            time_travel_bookmark_capable: true,
        }
    }

    #[test]
    fn completed_receipt_requires_exact_fresh_post_state() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let receipt = receipt(&transaction, ExecutionEventKind::Completed)?;
        let verified =
            verify_execution_post_state(&transaction, &receipt, &post_observation(true), T0 + 50)?;
        assert_eq!(
            verified.disposition,
            ExecutionPostStateDisposition::CompletedVerified
        );
        assert!(verified.expected_post_state_reached);
        Ok(())
    }

    #[test]
    fn completed_receipt_rejects_unchanged_provider_state() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let receipt = receipt(&transaction, ExecutionEventKind::Completed)?;
        assert!(
            verify_execution_post_state(&transaction, &receipt, &post_observation(false), T0 + 50,)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn failed_no_effect_requires_exact_unchanged_state() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let receipt = receipt(&transaction, ExecutionEventKind::FailedNoEffect)?;
        let verified =
            verify_execution_post_state(&transaction, &receipt, &post_observation(false), T0 + 50)?;
        assert_eq!(
            verified.disposition,
            ExecutionPostStateDisposition::FailedNoEffectVerified
        );
        Ok(())
    }

    #[test]
    fn recovery_required_never_promotes_to_completed_when_target_revision_is_present()
    -> Result<(), D1Error> {
        let transaction = transaction()?;
        let receipt = receipt(&transaction, ExecutionEventKind::RecoveryRequired)?;
        let verified =
            verify_execution_post_state(&transaction, &receipt, &post_observation(true), T0 + 50)?;
        assert_eq!(
            verified.disposition,
            ExecutionPostStateDisposition::RecoveryRequiredConfirmed
        );
        assert!(verified.expected_post_state_reached);
        Ok(())
    }

    #[test]
    fn post_observation_must_be_after_terminal_receipt() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let receipt = receipt(&transaction, ExecutionEventKind::Completed)?;
        let mut observation = post_observation(true);
        observation.observed_at_unix_seconds = T0 + 26;
        assert!(
            verify_execution_post_state(&transaction, &receipt, &observation, T0 + 50).is_err()
        );
        Ok(())
    }

    #[test]
    fn stale_post_observation_fails_closed() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let receipt = receipt(&transaction, ExecutionEventKind::Completed)?;
        assert!(
            verify_execution_post_state(
                &transaction,
                &receipt,
                &post_observation(true),
                T0 + 1_000,
            )
            .is_err()
        );
        Ok(())
    }
}
