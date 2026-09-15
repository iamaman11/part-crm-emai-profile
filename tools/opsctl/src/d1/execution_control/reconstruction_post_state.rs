use super::{ExecutionEventKind, ExecutionReceipt, serialize_execution_receipt};
use crate::canonical::{canonical_json, sha256_hex};
use crate::d1::model::{D1Error, GateResult};
use crate::d1::reconstruction;
use crate::d1::transaction::{ProviderObservationInput, TargetIdentity, TransactionPhase};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const POST_STATE_SCHEMA_VERSION: u64 = 1;
const STALE_OBSERVATION_REMEDIATION: &str = "Discard the stale observation. Re-observe the exact staging Catalog and re-evaluate the sealed CURRENT reconstruction without inferring success from logs or a partial ledger.";
const RECONSTRUCTION_DRIFT_REMEDIATION: &str = "Do not retry or reinterpret the sealed reconstruction. Preserve the execution receipt, re-observe the exact target, and use the existing CURRENT reconstruction owner to classify the target as completed, failed-with-no-effect, or recovery-required before any further provider write.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReconstructionPostStateDisposition {
    CompletedVerified,
    RecoveryRequiredConfirmed,
    FailedNoEffectVerified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconstructionPostStateVerification {
    pub schema_version: u64,
    pub status: String,
    pub disposition: ReconstructionPostStateDisposition,
    pub reconstruction_id: String,
    pub receipt_id: String,
    pub target: TargetIdentity,
    pub terminal_state: ExecutionEventKind,
    pub reconstruction_applied: bool,
    pub expected_post_state_reached: bool,
    pub target_schema_revision: String,
    pub expected_ledger_migrations: Vec<String>,
    pub observed_migrations: Vec<String>,
    pub observed_pending_migrations: Vec<String>,
    pub post_remote_ledger_sha256: String,
    pub post_observation_source: String,
    pub post_observed_at_unix_seconds: i64,
    pub evaluated_at_unix_seconds: i64,
}

pub fn verify_current_reconstruction_post_state(
    reconstruction_value: &Value,
    receipt: &ExecutionReceipt,
    post_observation: &ProviderObservationInput,
    evaluated_at_unix_seconds: i64,
) -> Result<ReconstructionPostStateVerification, D1Error> {
    let subject = reconstruction::authorization_subject(reconstruction_value)?;
    serialize_execution_receipt(receipt)?;

    if post_observation.schema_version != POST_STATE_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "CURRENT reconstruction post-state provider observation schema_version must be {POST_STATE_SCHEMA_VERSION}"
        )));
    }
    validate_target(&post_observation.target)?;
    validate_non_empty(
        &post_observation.observation_source,
        "post-state observation_source",
    )?;
    validate_sha256(
        &post_observation.remote_ledger_sha256,
        "post-state remote_ledger_sha256",
    )?;
    validate_unique_strings(
        &post_observation.remote_migrations,
        "post-state remote_migrations",
    )?;
    let normalized_ledger = serde_json::json!({
        "remote_migrations": &post_observation.remote_migrations,
    });
    let normalized_ledger_json = canonical_json(&normalized_ledger).map_err(D1Error::new)?;
    let expected_remote_ledger_sha256 = sha256_hex(normalized_ledger_json.as_bytes());
    if post_observation.remote_ledger_sha256 != expected_remote_ledger_sha256 {
        return Err(reconstruction_drift(
            "fresh reconstruction post-state remote_ledger_sha256 does not bind the exact normalized remote_migrations",
        ));
    }
    validate_unique_strings(
        &post_observation.wrangler_pending_migrations,
        "post-state wrangler_pending_migrations",
    )?;
    if post_observation
        .wrangler_pending_migrations
        .iter()
        .any(|pending| post_observation.remote_migrations.contains(pending))
    {
        return Err(reconstruction_drift(
            "fresh reconstruction post-state cannot report the same migration as both applied and pending",
        ));
    }

    let root = reconstruction_value
        .as_object()
        .ok_or_else(|| D1Error::new("CURRENT reconstruction projection must be an object"))?;
    let reconstruction_id = root
        .get("reconstruction_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            D1Error::new("CURRENT reconstruction projection is missing reconstruction_id")
        })?;
    let plan = root
        .get("plan")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("CURRENT reconstruction projection is missing plan"))?;
    let source_sha = required_string(plan.get("source_sha"), "plan.source_sha")?;
    let target_schema_revision = required_string(
        plan.get("target_schema_revision"),
        "plan.target_schema_revision",
    )?;
    let provider_observation = plan
        .get("provider_observation")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            D1Error::new("CURRENT reconstruction plan is missing provider_observation")
        })?;
    let predecessor_ledger_sha256 = required_string(
        provider_observation.get("predecessor_ledger_sha256"),
        "plan.provider_observation.predecessor_ledger_sha256",
    )?;
    validate_sha256(predecessor_ledger_sha256, "predecessor_ledger_sha256")?;
    let expected_post_state = plan
        .get("expected_post_state")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            D1Error::new("CURRENT reconstruction plan is missing expected_post_state")
        })?;
    let expected_ledger_migrations = string_array(
        expected_post_state.get("ledger_migrations"),
        "expected_post_state.ledger_migrations",
    )?;

    if receipt.phase != TransactionPhase::Ordinary {
        return Err(reconstruction_drift(
            "CURRENT reconstruction post-state requires an ORDINARY execution receipt",
        ));
    }
    if receipt.target != subject.target || post_observation.target != subject.target {
        return Err(reconstruction_drift(
            "CURRENT reconstruction receipt/post-state target drifted from the sealed reconstruction target",
        ));
    }
    if receipt.operation_identity != subject.operation_id
        || receipt.transaction_id.as_deref() != Some(subject.operation_id.as_str())
        || reconstruction_id != subject.operation_id
    {
        return Err(reconstruction_drift(
            "CURRENT reconstruction execution receipt does not bind the exact reconstruction_id",
        ));
    }
    if receipt.source_sha != source_sha {
        return Err(reconstruction_drift(
            "CURRENT reconstruction execution receipt source_sha drifted from the sealed reconstruction source",
        ));
    }

    let terminal_event = receipt.events.last().ok_or_else(|| {
        D1Error::new("CURRENT reconstruction post-state requires a terminal execution receipt")
    })?;
    let terminal_state = terminal_event.kind;
    if !matches!(
        terminal_state,
        ExecutionEventKind::Completed
            | ExecutionEventKind::RecoveryRequired
            | ExecutionEventKind::FailedNoEffect
    ) {
        return Err(reconstruction_drift(
            "CURRENT reconstruction post-state requires COMPLETED, RECOVERY_REQUIRED, or FAILED_NO_EFFECT receipt",
        ));
    }

    if evaluated_at_unix_seconds <= 0
        || post_observation.observed_at_unix_seconds <= 0
        || post_observation.observed_at_unix_seconds > evaluated_at_unix_seconds
    {
        return Err(D1Error::new(
            "CURRENT reconstruction post-state timestamps must be positive and the observation must not be from the future",
        ));
    }
    if post_observation.observed_at_unix_seconds < terminal_event.occurred_at_unix_seconds {
        return Err(stale_observation(
            "CURRENT reconstruction post-state observation predates the terminal execution receipt event",
        ));
    }
    let age = evaluated_at_unix_seconds - post_observation.observed_at_unix_seconds;
    let max_age = i64::try_from(subject.freshness_max_age_seconds).map_err(|_| {
        D1Error::new("CURRENT reconstruction freshness window exceeds supported timestamp range")
    })?;
    if age > max_age {
        return Err(stale_observation(
            "CURRENT reconstruction post-state observation is stale under the sealed freshness policy",
        ));
    }

    let migration_applied_count = receipt
        .events
        .iter()
        .filter(|event| event.kind == ExecutionEventKind::MigrationApplied)
        .count();
    if migration_applied_count != 0 {
        return Err(reconstruction_drift(
            "CURRENT reconstruction receipt must never contain MIGRATION_APPLIED; reconstruction cannot masquerade as ordinary migration",
        ));
    }
    let reconstruction_applied_count = receipt
        .events
        .iter()
        .filter(|event| event.kind == ExecutionEventKind::ReconstructionApplied)
        .count();
    if reconstruction_applied_count > 1 {
        return Err(reconstruction_drift(
            "CURRENT reconstruction receipt may contain at most one RECONSTRUCTION_APPLIED event",
        ));
    }
    let reconstruction_applied = reconstruction_applied_count == 1;
    let expected_post_state_reached = post_observation.remote_migrations
        == expected_ledger_migrations
        && post_observation
            .remote_migrations
            .last()
            .is_some_and(|revision| revision == target_schema_revision);

    let disposition = match terminal_state {
        ExecutionEventKind::Completed => {
            if !reconstruction_applied {
                return Err(reconstruction_drift(
                    "COMPLETED CURRENT reconstruction receipt requires exactly one RECONSTRUCTION_APPLIED event",
                ));
            }
            if !expected_post_state_reached {
                return Err(reconstruction_drift(
                    "COMPLETED CURRENT reconstruction disagrees with the exact sealed CURRENT ledger/target revision",
                ));
            }
            ReconstructionPostStateDisposition::CompletedVerified
        }
        ExecutionEventKind::FailedNoEffect => {
            if reconstruction_applied {
                return Err(reconstruction_drift(
                    "FAILED_NO_EFFECT CURRENT reconstruction receipt cannot contain RECONSTRUCTION_APPLIED",
                ));
            }
            if !post_observation.remote_migrations.is_empty() {
                return Err(target_prestate_drift(
                    "FAILED_NO_EFFECT CURRENT reconstruction post-state is no longer the sealed exactly-empty predecessor",
                ));
            }
            if post_observation.remote_ledger_sha256 != predecessor_ledger_sha256 {
                return Err(target_prestate_drift(
                    "FAILED_NO_EFFECT CURRENT reconstruction post-state ledger digest drifted from the sealed empty predecessor identity",
                ));
            }
            ReconstructionPostStateDisposition::FailedNoEffectVerified
        }
        ExecutionEventKind::RecoveryRequired => {
            ReconstructionPostStateDisposition::RecoveryRequiredConfirmed
        }
        _ => unreachable!("terminal reconstruction state filtered above"),
    };

    Ok(ReconstructionPostStateVerification {
        schema_version: POST_STATE_SCHEMA_VERSION,
        status: "RECONSTRUCTION_POST_STATE_VERIFIED".to_owned(),
        disposition,
        reconstruction_id: subject.operation_id,
        receipt_id: receipt.receipt_id.clone(),
        target: subject.target,
        terminal_state,
        reconstruction_applied,
        expected_post_state_reached,
        target_schema_revision: target_schema_revision.to_owned(),
        expected_ledger_migrations,
        observed_migrations: post_observation.remote_migrations.clone(),
        observed_pending_migrations: post_observation.wrangler_pending_migrations.clone(),
        post_remote_ledger_sha256: post_observation.remote_ledger_sha256.clone(),
        post_observation_source: post_observation.observation_source.clone(),
        post_observed_at_unix_seconds: post_observation.observed_at_unix_seconds,
        evaluated_at_unix_seconds,
    })
}

pub fn serialize_current_reconstruction_post_state_verification(
    verification: &ReconstructionPostStateVerification,
) -> Result<String, D1Error> {
    canonical_json(&serde_json::to_value(verification).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize CURRENT reconstruction post-state verification: {error}"
        ))
    })?)
    .map_err(D1Error::new)
}

fn reconstruction_drift(summary: impl Into<String>) -> D1Error {
    D1Error::blocked(GateResult::blocked(
        "POST_STATE",
        "d1.reconstruction_execution.post_state",
        "RECONSTRUCTION_POST_STATE_DRIFT",
        summary,
        Some(
            "exact receipt-bound CURRENT reconstruction state with no migration fallback"
                .to_owned(),
        ),
        None,
        RECONSTRUCTION_DRIFT_REMEDIATION,
    ))
}

fn target_prestate_drift(summary: impl Into<String>) -> D1Error {
    D1Error::blocked(GateResult::blocked(
        "POST_STATE",
        "d1.reconstruction_execution.prestate",
        "TARGET_PRESTATE_DRIFT",
        summary,
        Some("fresh provider state equal to the sealed exactly-empty predecessor when FAILED_NO_EFFECT is claimed".to_owned()),
        None,
        RECONSTRUCTION_DRIFT_REMEDIATION,
    ))
}

fn stale_observation(summary: impl Into<String>) -> D1Error {
    D1Error::blocked(GateResult::blocked(
        "POST_STATE",
        "d1.reconstruction_execution.freshness",
        "STALE_OBSERVATION",
        summary,
        Some("fresh post-state observation after the terminal receipt and within the sealed reconstruction freshness window".to_owned()),
        None,
        STALE_OBSERVATION_REMEDIATION,
    ))
}

fn required_string<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str, D1Error> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new(format!("CURRENT reconstruction {label} is missing")))
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

fn validate_target(target: &TargetIdentity) -> Result<(), D1Error> {
    validate_non_empty(&target.environment, "target.environment")?;
    validate_non_empty(&target.account_id, "target.account_id")?;
    validate_non_empty(&target.database_name, "target.database_name")?;
    validate_non_empty(&target.database_id, "target.database_id")
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
    use super::*;
    use crate::canonical::{canonical_json, sha256_hex};
    use crate::d1::execution_control::{
        ExecutionEventInput, ExecutionReceiptSeed, TargetFenceLeaseInput, acquire_target_fence,
        append_execution_event, initialize_execution_receipt,
    };
    use crate::d1::transaction::RecoveryStrategy;
    use serde_json::json;

    const T0: i64 = 1_788_700_000;

    fn target() -> TargetIdentity {
        TargetIdentity {
            environment: "staging".to_owned(),
            account_id: "account-1".to_owned(),
            database_name: "catalog-staging".to_owned(),
            database_id: "database-1".to_owned(),
        }
    }

    fn canonical_sha(value: &Value) -> Result<String, D1Error> {
        let canonical = canonical_json(value).map_err(D1Error::new)?;
        Ok(sha256_hex(canonical.as_bytes()))
    }

    fn reconstruction() -> Result<Value, D1Error> {
        let predecessor = json!({"remote_migrations": []});
        let predecessor_sha = canonical_sha(&predecessor)?;
        let provider_observation = json!({
            "target": target(),
            "observed_at_unix_seconds": T0,
            "fresh_until_unix_seconds": T0 + 900,
            "observation_source": "fixture:empty-staging",
            "predecessor_ledger_sha256": predecessor_sha,
            "remote_migrations": []
        });
        let observation_digest = canonical_sha(&provider_observation)?;
        let expected_ledger = vec![
            "0032_bridge_device_enrollment_authority.sql",
            "0033_device_public_key_binding.sql",
            "0034_device_application_authority.sql",
        ];
        let plan = json!({
            "schema_version": 1,
            "kind": "D1_CURRENT_FRESH_ZERO_RECONSTRUCTION",
            "disposition": "PREPROD_BASELINE_DRIFT",
            "component": "catalog",
            "source_sha": "11".repeat(20),
            "tree_sha": "22".repeat(20),
            "release_set_id": format!("release-set-v3-sha256-{}", "33".repeat(32)),
            "release_manifest_sha256": "44".repeat(32),
            "repository_identity_sha256": "55".repeat(32),
            "construction_sha256": "66".repeat(32),
            "target_schema_revision": "0034_device_application_authority.sql",
            "supported_schema_min": "0032_bridge_device_enrollment_authority.sql",
            "supported_schema_max": "0035_pas2_payload_fingerprint_contract.sql",
            "provider_observation": provider_observation,
            "observation_digest": observation_digest,
            "freshness_max_age_seconds": 900,
            "allowed_provider_effects": ["D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION"],
            "forbidden_provider_effects": [
                "D1_MIGRATIONS_APPLY_EXACT_PLAN",
                "D1_CREATE",
                "D1_DELETE",
                "D1_TIME_TRAVEL_RESTORE",
                "RESOURCE_AUTO_PROVISION",
                "PRODUCTION_MUTATION"
            ],
            "expected_post_state": {
                "component": "catalog",
                "target_schema_revision": "0034_device_application_authority.sql",
                "ledger_migrations": expected_ledger,
                "construction_sha256": "66".repeat(32),
                "repository_identity_sha256": "55".repeat(32)
            }
        });
        let reconstruction_id = canonical_sha(&plan)?;
        Ok(json!({
            "schema_version": 1,
            "status": "RECONSTRUCTION_PREPARED",
            "mode": "read-only",
            "authorization_required": true,
            "authorization_consumed": false,
            "mutation_executed": false,
            "provider_mutation_executed": false,
            "reconstruction_id": reconstruction_id,
            "plan": plan
        }))
    }

    fn append_fixture_event(
        receipt: &ExecutionReceipt,
        kind: ExecutionEventKind,
        offset: i64,
    ) -> Result<ExecutionReceipt, D1Error> {
        append_execution_event(
            receipt,
            ExecutionEventInput {
                kind,
                occurred_at_unix_seconds: T0 + offset,
                migration_id: None,
            },
        )
    }

    fn receipt(
        reconstruction: &Value,
        terminal: ExecutionEventKind,
    ) -> Result<ExecutionReceipt, D1Error> {
        let operation_id = required_string(
            reconstruction.get("reconstruction_id"),
            "fixture.reconstruction_id",
        )?
        .to_owned();
        let lease = acquire_target_fence(TargetFenceLeaseInput {
            schema_version: 1,
            target: target(),
            phase: TransactionPhase::Ordinary,
            operation_identity: operation_id.clone(),
            transaction_id: Some(operation_id),
            authorization_digest: Some("77".repeat(32)),
            source_sha: "11".repeat(20),
            executor_run_id: 101,
            fence_epoch: 10,
            run_attempt: 1,
            acquired_at_unix_seconds: T0 + 1,
        })?;
        let mut receipt = initialize_execution_receipt(
            ExecutionReceiptSeed {
                schema_version: 1,
                target: target(),
                phase: TransactionPhase::Ordinary,
                operation_identity: lease.operation_identity.clone(),
                transaction_id: lease.transaction_id.clone(),
                authorization_digest: lease.authorization_digest.clone(),
                source_sha: "11".repeat(20),
                recovery_strategy: RecoveryStrategy::FailForwardOnly,
                fence: lease,
            },
            T0 + 2,
            T0 + 3,
        )?;
        match terminal {
            ExecutionEventKind::Completed => {
                receipt = append_fixture_event(&receipt, ExecutionEventKind::PrewriteFencePass, 4)?;
                receipt = append_fixture_event(&receipt, ExecutionEventKind::MutationStarted, 5)?;
                receipt =
                    append_fixture_event(&receipt, ExecutionEventKind::ReconstructionApplied, 6)?;
                receipt = append_fixture_event(&receipt, ExecutionEventKind::PostObserved, 7)?;
                receipt = append_fixture_event(&receipt, ExecutionEventKind::Verified, 8)?;
                receipt = append_fixture_event(&receipt, ExecutionEventKind::Completed, 9)?;
            }
            ExecutionEventKind::RecoveryRequired => {
                receipt = append_fixture_event(&receipt, ExecutionEventKind::PrewriteFencePass, 4)?;
                receipt = append_fixture_event(&receipt, ExecutionEventKind::MutationStarted, 5)?;
                receipt = append_fixture_event(&receipt, ExecutionEventKind::RecoveryRequired, 6)?;
            }
            ExecutionEventKind::FailedNoEffect => {
                receipt = append_fixture_event(&receipt, ExecutionEventKind::PrewriteAborted, 4)?;
                receipt = append_fixture_event(&receipt, ExecutionEventKind::FailedNoEffect, 5)?;
            }
            _ => return Err(D1Error::new("unsupported fixture terminal")),
        }
        Ok(receipt)
    }

    fn observation(
        reconstruction: &Value,
        completed: bool,
        observed_at: i64,
    ) -> Result<ProviderObservationInput, D1Error> {
        let migrations = if completed {
            string_array(
                Some(&reconstruction["plan"]["expected_post_state"]["ledger_migrations"]),
                "fixture.expected_post_state.ledger_migrations",
            )?
        } else {
            Vec::new()
        };
        let digest = if completed {
            canonical_sha(&json!({"remote_migrations": &migrations}))?
        } else {
            required_string(
                Some(&reconstruction["plan"]["provider_observation"]["predecessor_ledger_sha256"]),
                "fixture.provider_observation.predecessor_ledger_sha256",
            )?
            .to_owned()
        };
        Ok(ProviderObservationInput {
            schema_version: 1,
            target: target(),
            observed_at_unix_seconds: observed_at,
            observation_source: "fixture:post-state".to_owned(),
            remote_ledger_sha256: digest,
            remote_migrations: migrations,
            wrangler_pending_migrations: if completed {
                vec!["0035_pas2_payload_fingerprint_contract.sql".to_owned()]
            } else {
                Vec::new()
            },
            deployment_identity: None,
            time_travel_bookmark_capable: true,
        })
    }

    #[test]
    fn completed_reconstruction_requires_exact_current_ledger() -> Result<(), D1Error> {
        let reconstruction = reconstruction()?;
        let receipt = receipt(&reconstruction, ExecutionEventKind::Completed)?;
        let post = observation(&reconstruction, true, T0 + 10)?;
        let verified =
            verify_current_reconstruction_post_state(&reconstruction, &receipt, &post, T0 + 11)?;
        assert_eq!(
            verified.disposition,
            ReconstructionPostStateDisposition::CompletedVerified
        );
        assert!(verified.reconstruction_applied);
        assert!(verified.expected_post_state_reached);
        assert_eq!(
            verified.observed_pending_migrations,
            vec!["0035_pas2_payload_fingerprint_contract.sql"]
        );
        Ok(())
    }

    #[test]
    fn failed_no_effect_requires_exact_empty_predecessor() -> Result<(), D1Error> {
        let reconstruction = reconstruction()?;
        let receipt = receipt(&reconstruction, ExecutionEventKind::FailedNoEffect)?;
        let post = observation(&reconstruction, false, T0 + 10)?;
        let verified =
            verify_current_reconstruction_post_state(&reconstruction, &receipt, &post, T0 + 11)?;
        assert_eq!(
            verified.disposition,
            ReconstructionPostStateDisposition::FailedNoEffectVerified
        );
        assert!(!verified.reconstruction_applied);
        Ok(())
    }

    #[test]
    fn completed_with_partial_or_deferred_materialization_fails_closed() -> Result<(), D1Error> {
        let reconstruction = reconstruction()?;
        let receipt = receipt(&reconstruction, ExecutionEventKind::Completed)?;
        let mut post = observation(&reconstruction, true, T0 + 10)?;
        post.remote_migrations
            .push("0035_pas2_payload_fingerprint_contract.sql".to_owned());
        assert!(
            verify_current_reconstruction_post_state(&reconstruction, &receipt, &post, T0 + 11)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn post_state_rejects_digest_that_does_not_bind_observed_migrations() -> Result<(), D1Error> {
        let reconstruction = reconstruction()?;
        let receipt = receipt(&reconstruction, ExecutionEventKind::Completed)?;
        let mut post = observation(&reconstruction, true, T0 + 10)?;
        post.remote_ledger_sha256 = "ff".repeat(32);
        let error =
            verify_current_reconstruction_post_state(&reconstruction, &receipt, &post, T0 + 11)
                .err()
                .ok_or_else(|| {
                    D1Error::new("mismatched normalized ledger digest unexpectedly passed")
                })?;
        assert_eq!(
            error.gate_result_json()["reason_code"],
            "RECONSTRUCTION_POST_STATE_DRIFT"
        );
        Ok(())
    }

    #[test]
    fn stale_post_state_fails_closed() -> Result<(), D1Error> {
        let reconstruction = reconstruction()?;
        let receipt = receipt(&reconstruction, ExecutionEventKind::Completed)?;
        let post = observation(&reconstruction, true, T0 + 10)?;
        let Err(error) =
            verify_current_reconstruction_post_state(&reconstruction, &receipt, &post, T0 + 911)
        else {
            return Err(D1Error::new(
                "stale fixture observation unexpectedly passed",
            ));
        };
        assert_eq!(error.gate_result_json()["reason_code"], "STALE_OBSERVATION");
        Ok(())
    }

    #[test]
    fn recovery_required_preserves_ambiguous_state_without_claiming_success() -> Result<(), D1Error>
    {
        let reconstruction = reconstruction()?;
        let receipt = receipt(&reconstruction, ExecutionEventKind::RecoveryRequired)?;
        let post = observation(&reconstruction, false, T0 + 10)?;
        let verified =
            verify_current_reconstruction_post_state(&reconstruction, &receipt, &post, T0 + 11)?;
        assert_eq!(
            verified.disposition,
            ReconstructionPostStateDisposition::RecoveryRequiredConfirmed
        );
        assert!(!verified.expected_post_state_reached);
        Ok(())
    }
}
