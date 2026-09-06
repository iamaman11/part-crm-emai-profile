use super::model::D1Error;
use super::transaction::{RecoveryStrategy, TargetIdentity, TransactionPhase};
use crate::canonical::{canonical_json, sha256_hex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const EXECUTION_CONTROL_SCHEMA_VERSION: u64 = 1;
const RELEASE_SET_PREFIX: &str = "release-set-v3-sha256-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetFenceLeaseInput {
    pub schema_version: u64,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub operation_identity: String,
    pub transaction_id: Option<String>,
    pub authorization_digest: Option<String>,
    pub source_sha: String,
    pub executor_run_id: u64,
    pub fence_epoch: u64,
    pub run_attempt: u64,
    pub acquired_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetFenceLease {
    pub schema_version: u64,
    pub status: String,
    pub fence_id: String,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub operation_identity: String,
    pub transaction_id: Option<String>,
    pub authorization_digest: Option<String>,
    pub source_sha: String,
    pub executor_run_id: u64,
    pub fence_epoch: u64,
    pub run_attempt: u64,
    pub acquired_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetFenceObservation {
    pub schema_version: u64,
    pub observed_at_unix_seconds: i64,
    pub history_complete: bool,
    pub current_marker_succeeded: bool,
    pub current_executor_run_id: u64,
    pub current_fence_epoch: u64,
    pub observed_acquired_leases: Vec<TargetFenceLease>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetFenceVerification {
    pub schema_version: u64,
    pub status: String,
    pub fence_id: String,
    pub target: TargetIdentity,
    pub operation_identity: String,
    pub executor_run_id: u64,
    pub fence_epoch: u64,
    pub observed_at_unix_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionEventKind {
    Prepared,
    Authorized,
    PrewriteFencePass,
    PrewriteAborted,
    MutationStarted,
    MigrationApplied,
    PostObserved,
    Verified,
    Completed,
    RecoveryRequired,
    FailedNoEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEventInput {
    pub kind: ExecutionEventKind,
    pub occurred_at_unix_seconds: i64,
    pub migration_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub sequence: u64,
    pub kind: ExecutionEventKind,
    pub occurred_at_unix_seconds: i64,
    pub migration_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceiptSeed {
    pub schema_version: u64,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub operation_identity: String,
    pub transaction_id: Option<String>,
    pub authorization_digest: Option<String>,
    pub source_sha: String,
    pub recovery_strategy: RecoveryStrategy,
    pub fence: TargetFenceLease,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub schema_version: u64,
    pub receipt_id: String,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub operation_identity: String,
    pub transaction_id: Option<String>,
    pub authorization_digest: Option<String>,
    pub source_sha: String,
    pub recovery_strategy: RecoveryStrategy,
    pub fence_id: String,
    pub executor_run_id: u64,
    pub fence_epoch: u64,
    pub events: Vec<ExecutionEvent>,
}

pub fn acquire_target_fence(input: TargetFenceLeaseInput) -> Result<TargetFenceLease, D1Error> {
    validate_fence_input(&input)?;
    let canonical = canonical_json(
        &serde_json::to_value(&input)
            .map_err(|error| D1Error::new(format!("cannot serialize target fence input: {error}")))?,
    )
    .map_err(D1Error::new)?;
    Ok(TargetFenceLease {
        schema_version: input.schema_version,
        status: "TARGET_FENCE_ACQUIRED".to_owned(),
        fence_id: sha256_hex(canonical.as_bytes()),
        target: input.target,
        phase: input.phase,
        operation_identity: input.operation_identity,
        transaction_id: input.transaction_id,
        authorization_digest: input.authorization_digest,
        source_sha: input.source_sha,
        executor_run_id: input.executor_run_id,
        fence_epoch: input.fence_epoch,
        run_attempt: input.run_attempt,
        acquired_at_unix_seconds: input.acquired_at_unix_seconds,
    })
}

pub fn verify_target_fence(
    lease: &TargetFenceLease,
    observation: &TargetFenceObservation,
) -> Result<TargetFenceVerification, D1Error> {
    validate_lease(lease)?;
    if observation.schema_version != EXECUTION_CONTROL_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "target fence observation schema_version must be {EXECUTION_CONTROL_SCHEMA_VERSION}"
        )));
    }
    if observation.observed_at_unix_seconds < lease.acquired_at_unix_seconds {
        return Err(D1Error::new(
            "target fence observation must not predate fence acquisition",
        ));
    }
    if !observation.history_complete {
        return Err(D1Error::new(
            "target fence history is incomplete or unknown; abort before provider mutation",
        ));
    }
    if !observation.current_marker_succeeded {
        return Err(D1Error::new(
            "current target fence acquisition marker is not durably successful",
        ));
    }
    if observation.current_executor_run_id != lease.executor_run_id
        || observation.current_fence_epoch != lease.fence_epoch
    {
        return Err(D1Error::new(
            "current executor run identity does not match the acquired target fence",
        ));
    }

    let mut fence_ids = BTreeSet::new();
    for observed in &observation.observed_acquired_leases {
        validate_lease(observed)?;
        if !fence_ids.insert(observed.fence_id.clone()) {
            return Err(D1Error::new(
                "target fence observation contains a duplicate acquired lease",
            ));
        }
        if same_target(&observed.target, &lease.target) {
            if observed.fence_epoch > lease.fence_epoch {
                return Err(D1Error::new(format!(
                    "stale executor fence rejected: newer fence epoch {} is acquired for the same target",
                    observed.fence_epoch
                )));
            }
            if observed.fence_epoch == lease.fence_epoch && observed.fence_id != lease.fence_id {
                return Err(D1Error::new(
                    "split-brain target fence rejected: same target has another lease at the current epoch",
                ));
            }
        }
    }

    Ok(TargetFenceVerification {
        schema_version: EXECUTION_CONTROL_SCHEMA_VERSION,
        status: "TARGET_FENCE_VERIFIED".to_owned(),
        fence_id: lease.fence_id.clone(),
        target: lease.target.clone(),
        operation_identity: lease.operation_identity.clone(),
        executor_run_id: lease.executor_run_id,
        fence_epoch: lease.fence_epoch,
        observed_at_unix_seconds: observation.observed_at_unix_seconds,
    })
}

pub fn initialize_execution_receipt(
    seed: ExecutionReceiptSeed,
    prepared_at_unix_seconds: i64,
    authorized_at_unix_seconds: i64,
) -> Result<ExecutionReceipt, D1Error> {
    validate_receipt_seed(&seed)?;
    if prepared_at_unix_seconds <= 0 || authorized_at_unix_seconds < prepared_at_unix_seconds {
        return Err(D1Error::new(
            "execution receipt PREPARED/AUTHORIZED timestamps must be positive and monotonic",
        ));
    }
    let canonical_seed = canonical_json(
        &serde_json::to_value(&seed)
            .map_err(|error| D1Error::new(format!("cannot serialize execution receipt seed: {error}")))?,
    )
    .map_err(D1Error::new)?;
    Ok(ExecutionReceipt {
        schema_version: EXECUTION_CONTROL_SCHEMA_VERSION,
        receipt_id: sha256_hex(canonical_seed.as_bytes()),
        target: seed.target,
        phase: seed.phase,
        operation_identity: seed.operation_identity,
        transaction_id: seed.transaction_id,
        authorization_digest: seed.authorization_digest,
        source_sha: seed.source_sha,
        recovery_strategy: seed.recovery_strategy,
        fence_id: seed.fence.fence_id,
        executor_run_id: seed.fence.executor_run_id,
        fence_epoch: seed.fence.fence_epoch,
        events: vec![
            ExecutionEvent {
                sequence: 1,
                kind: ExecutionEventKind::Prepared,
                occurred_at_unix_seconds: prepared_at_unix_seconds,
                migration_id: None,
            },
            ExecutionEvent {
                sequence: 2,
                kind: ExecutionEventKind::Authorized,
                occurred_at_unix_seconds: authorized_at_unix_seconds,
                migration_id: None,
            },
        ],
    })
}

pub fn append_execution_event(
    receipt: &ExecutionReceipt,
    input: ExecutionEventInput,
) -> Result<ExecutionReceipt, D1Error> {
    validate_receipt(receipt)?;
    validate_event_input(&input)?;
    let previous = receipt
        .events
        .last()
        .ok_or_else(|| D1Error::new("execution receipt event stream must not be empty"))?;
    if input.occurred_at_unix_seconds < previous.occurred_at_unix_seconds {
        return Err(D1Error::new(
            "execution receipt event timestamps must be monotonic",
        ));
    }
    validate_transition(receipt, previous.kind, &input)?;

    let mut next = receipt.clone();
    next.events.push(ExecutionEvent {
        sequence: u64::try_from(next.events.len() + 1)
            .map_err(|_| D1Error::new("execution receipt event sequence overflow"))?,
        kind: input.kind,
        occurred_at_unix_seconds: input.occurred_at_unix_seconds,
        migration_id: input.migration_id,
    });
    Ok(next)
}

pub fn serialize_target_fence_lease(lease: &TargetFenceLease) -> Result<String, D1Error> {
    validate_lease(lease)?;
    canonical_json(
        &serde_json::to_value(lease)
            .map_err(|error| D1Error::new(format!("cannot serialize target fence lease: {error}")))?,
    )
    .map_err(D1Error::new)
}

pub fn serialize_target_fence_verification(
    verification: &TargetFenceVerification,
) -> Result<String, D1Error> {
    canonical_json(&serde_json::to_value(verification).map_err(|error| {
        D1Error::new(format!("cannot serialize target fence verification: {error}"))
    })?)
    .map_err(D1Error::new)
}

pub fn serialize_execution_receipt(receipt: &ExecutionReceipt) -> Result<String, D1Error> {
    validate_receipt(receipt)?;
    canonical_json(
        &serde_json::to_value(receipt)
            .map_err(|error| D1Error::new(format!("cannot serialize execution receipt: {error}")))?,
    )
    .map_err(D1Error::new)
}

fn validate_fence_input(input: &TargetFenceLeaseInput) -> Result<(), D1Error> {
    if input.schema_version != EXECUTION_CONTROL_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "target fence input schema_version must be {EXECUTION_CONTROL_SCHEMA_VERSION}"
        )));
    }
    validate_target(&input.target)?;
    validate_non_empty(&input.operation_identity, "operation_identity")?;
    validate_git_object_id(&input.source_sha, "source_sha")?;
    if input.executor_run_id == 0 || input.fence_epoch == 0 {
        return Err(D1Error::new(
            "executor_run_id and fence_epoch must be positive",
        ));
    }
    if input.run_attempt != 1 {
        return Err(D1Error::new(
            "target fence acquisition requires exact workflow run_attempt=1",
        ));
    }
    if input.acquired_at_unix_seconds <= 0 {
        return Err(D1Error::new(
            "target fence acquired_at_unix_seconds must be positive",
        ));
    }
    validate_operation_binding(
        input.phase,
        &input.operation_identity,
        input.transaction_id.as_deref(),
        input.authorization_digest.as_deref(),
    )
}

fn validate_lease(lease: &TargetFenceLease) -> Result<(), D1Error> {
    if lease.status != "TARGET_FENCE_ACQUIRED" {
        return Err(D1Error::new(
            "target fence lease status must be TARGET_FENCE_ACQUIRED",
        ));
    }
    validate_sha256(&lease.fence_id, "fence_id")?;
    let input = TargetFenceLeaseInput {
        schema_version: lease.schema_version,
        target: lease.target.clone(),
        phase: lease.phase,
        operation_identity: lease.operation_identity.clone(),
        transaction_id: lease.transaction_id.clone(),
        authorization_digest: lease.authorization_digest.clone(),
        source_sha: lease.source_sha.clone(),
        executor_run_id: lease.executor_run_id,
        fence_epoch: lease.fence_epoch,
        run_attempt: lease.run_attempt,
        acquired_at_unix_seconds: lease.acquired_at_unix_seconds,
    };
    validate_fence_input(&input)?;
    let canonical = canonical_json(
        &serde_json::to_value(&input)
            .map_err(|error| D1Error::new(format!("cannot serialize target fence input: {error}")))?,
    )
    .map_err(D1Error::new)?;
    if lease.fence_id != sha256_hex(canonical.as_bytes()) {
        return Err(D1Error::new(
            "target fence_id does not match the exact canonical lease input",
        ));
    }
    Ok(())
}

fn validate_receipt_seed(seed: &ExecutionReceiptSeed) -> Result<(), D1Error> {
    if seed.schema_version != EXECUTION_CONTROL_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "execution receipt seed schema_version must be {EXECUTION_CONTROL_SCHEMA_VERSION}"
        )));
    }
    validate_lease(&seed.fence)?;
    validate_target(&seed.target)?;
    validate_git_object_id(&seed.source_sha, "source_sha")?;
    validate_operation_binding(
        seed.phase,
        &seed.operation_identity,
        seed.transaction_id.as_deref(),
        seed.authorization_digest.as_deref(),
    )?;
    if seed.target != seed.fence.target
        || seed.phase != seed.fence.phase
        || seed.operation_identity != seed.fence.operation_identity
        || seed.transaction_id != seed.fence.transaction_id
        || seed.authorization_digest != seed.fence.authorization_digest
        || seed.source_sha != seed.fence.source_sha
    {
        return Err(D1Error::new(
            "execution receipt seed must exactly bind the acquired target fence lease",
        ));
    }
    Ok(())
}

fn validate_receipt(receipt: &ExecutionReceipt) -> Result<(), D1Error> {
    if receipt.schema_version != EXECUTION_CONTROL_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "execution receipt schema_version must be {EXECUTION_CONTROL_SCHEMA_VERSION}"
        )));
    }
    validate_sha256(&receipt.receipt_id, "receipt_id")?;
    validate_sha256(&receipt.fence_id, "fence_id")?;
    validate_target(&receipt.target)?;
    validate_git_object_id(&receipt.source_sha, "source_sha")?;
    validate_operation_binding(
        receipt.phase,
        &receipt.operation_identity,
        receipt.transaction_id.as_deref(),
        receipt.authorization_digest.as_deref(),
    )?;
    if receipt.executor_run_id == 0 || receipt.fence_epoch == 0 {
        return Err(D1Error::new(
            "execution receipt executor_run_id and fence_epoch must be positive",
        ));
    }
    if receipt.events.len() < 2
        || receipt.events[0].kind != ExecutionEventKind::Prepared
        || receipt.events[1].kind != ExecutionEventKind::Authorized
    {
        return Err(D1Error::new(
            "execution receipt must begin with PREPARED then AUTHORIZED",
        ));
    }
    let mut previous_time = 0;
    for (index, event) in receipt.events.iter().enumerate() {
        if event.sequence != u64::try_from(index + 1).unwrap_or(u64::MAX) {
            return Err(D1Error::new(
                "execution receipt event sequence must be contiguous and one-based",
            ));
        }
        if event.occurred_at_unix_seconds <= 0 || event.occurred_at_unix_seconds < previous_time {
            return Err(D1Error::new(
                "execution receipt event timestamps must be positive and monotonic",
            ));
        }
        if event.kind == ExecutionEventKind::MigrationApplied {
            validate_non_empty(
                event.migration_id.as_deref().unwrap_or_default(),
                "MIGRATION_APPLIED migration_id",
            )?;
        } else if event.migration_id.is_some() {
            return Err(D1Error::new(
                "only MIGRATION_APPLIED may carry migration_id",
            ));
        }
        previous_time = event.occurred_at_unix_seconds;
    }
    Ok(())
}

fn validate_event_input(input: &ExecutionEventInput) -> Result<(), D1Error> {
    if input.occurred_at_unix_seconds <= 0 {
        return Err(D1Error::new(
            "execution event occurred_at_unix_seconds must be positive",
        ));
    }
    if input.kind == ExecutionEventKind::MigrationApplied {
        validate_non_empty(
            input.migration_id.as_deref().unwrap_or_default(),
            "MIGRATION_APPLIED migration_id",
        )?;
    } else if input.migration_id.is_some() {
        return Err(D1Error::new(
            "only MIGRATION_APPLIED may carry migration_id",
        ));
    }
    Ok(())
}

fn validate_transition(
    receipt: &ExecutionReceipt,
    previous: ExecutionEventKind,
    input: &ExecutionEventInput,
) -> Result<(), D1Error> {
    use ExecutionEventKind::{
        Authorized, Completed, FailedNoEffect, MigrationApplied, MutationStarted, PostObserved,
        Prepared, PrewriteAborted, PrewriteFencePass, RecoveryRequired, Verified,
    };
    let allowed = match (previous, input.kind) {
        (Authorized, PrewriteFencePass | PrewriteAborted) => true,
        (PrewriteAborted, FailedNoEffect) => true,
        (PrewriteFencePass, MutationStarted | PostObserved) => true,
        (MutationStarted, MigrationApplied | PostObserved | RecoveryRequired) => true,
        (MigrationApplied, MigrationApplied | PostObserved | RecoveryRequired) => true,
        (PostObserved, Verified | RecoveryRequired) => true,
        (Verified, Completed) => true,
        (Prepared | Completed | RecoveryRequired | FailedNoEffect, _) => false,
        _ => false,
    };
    if !allowed {
        return Err(D1Error::new(format!(
            "execution receipt event transition is not allowed: {previous:?} -> {:?}",
            input.kind
        )));
    }
    if input.kind == MigrationApplied {
        let migration_id = input.migration_id.as_deref().unwrap_or_default();
        if receipt.events.iter().any(|event| {
            event.kind == MigrationApplied && event.migration_id.as_deref() == Some(migration_id)
        }) {
            return Err(D1Error::new(format!(
                "execution receipt cannot record MIGRATION_APPLIED twice for {migration_id}"
            )));
        }
    }
    Ok(())
}

fn validate_operation_binding(
    phase: TransactionPhase,
    operation_identity: &str,
    transaction_id: Option<&str>,
    authorization_digest: Option<&str>,
) -> Result<(), D1Error> {
    match phase {
        TransactionPhase::Ordinary => {
            let transaction_id = transaction_id.ok_or_else(|| {
                D1Error::new("ordinary target fence requires transaction_id")
            })?;
            let authorization_digest = authorization_digest.ok_or_else(|| {
                D1Error::new("ordinary target fence requires authorization_digest")
            })?;
            validate_sha256(transaction_id, "transaction_id")?;
            validate_sha256(authorization_digest, "authorization_digest")?;
            if operation_identity != transaction_id {
                return Err(D1Error::new(
                    "ordinary operation_identity must exactly equal transaction_id",
                ));
            }
        }
        TransactionPhase::Contract => {
            if transaction_id.is_some() || authorization_digest.is_some() {
                return Err(D1Error::new(
                    "separately governed CONTRACT fence must not masquerade as ordinary transaction authorization",
                ));
            }
            let digest = operation_identity.strip_prefix(RELEASE_SET_PREFIX).ok_or_else(|| {
                D1Error::new(format!(
                    "CONTRACT operation_identity must be an exact {RELEASE_SET_PREFIX}<sha256> Release Set identity"
                ))
            })?;
            validate_sha256(digest, "CONTRACT operation_identity digest")?;
        }
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity) -> Result<(), D1Error> {
    validate_non_empty(&target.environment, "target.environment")?;
    validate_non_empty(&target.account_id, "target.account_id")?;
    validate_non_empty(&target.database_name, "target.database_name")?;
    validate_non_empty(&target.database_id, "target.database_id")
}

fn same_target(left: &TargetIdentity, right: &TargetIdentity) -> bool {
    left.account_id == right.account_id && left.database_id == right.database_id
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), D1Error> {
    if value.trim().is_empty() {
        return Err(D1Error::new(format!("{label} must not be empty")));
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

fn validate_git_object_id(value: &str, label: &str) -> Result<(), D1Error> {
    if !matches!(value.len(), 40 | 64) || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "{label} must be a 40- or 64-character lowercase Git object id"
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
    use super::*;

    const T0: i64 = 1_788_700_000;

    fn target(database_id: &str) -> TargetIdentity {
        TargetIdentity {
            environment: "staging".to_owned(),
            account_id: "account-1".to_owned(),
            database_name: format!("database-{database_id}"),
            database_id: database_id.to_owned(),
        }
    }

    fn ordinary_input(epoch: u64, database_id: &str) -> TargetFenceLeaseInput {
        let transaction_id = "11".repeat(32);
        TargetFenceLeaseInput {
            schema_version: 1,
            target: target(database_id),
            phase: TransactionPhase::Ordinary,
            operation_identity: transaction_id.clone(),
            transaction_id: Some(transaction_id),
            authorization_digest: Some("22".repeat(32)),
            source_sha: "33".repeat(20),
            executor_run_id: 10_000 + epoch,
            fence_epoch: epoch,
            run_attempt: 1,
            acquired_at_unix_seconds: T0 + i64::try_from(epoch).unwrap_or_default(),
        }
    }

    fn observation(lease: &TargetFenceLease) -> TargetFenceObservation {
        TargetFenceObservation {
            schema_version: 1,
            observed_at_unix_seconds: lease.acquired_at_unix_seconds + 10,
            history_complete: true,
            current_marker_succeeded: true,
            current_executor_run_id: lease.executor_run_id,
            current_fence_epoch: lease.fence_epoch,
            observed_acquired_leases: Vec::new(),
        }
    }

    fn receipt_seed(lease: TargetFenceLease) -> ExecutionReceiptSeed {
        ExecutionReceiptSeed {
            schema_version: 1,
            target: lease.target.clone(),
            phase: lease.phase,
            operation_identity: lease.operation_identity.clone(),
            transaction_id: lease.transaction_id.clone(),
            authorization_digest: lease.authorization_digest.clone(),
            source_sha: lease.source_sha.clone(),
            recovery_strategy: RecoveryStrategy::NoopRetry,
            fence: lease,
        }
    }

    fn append(
        receipt: &ExecutionReceipt,
        kind: ExecutionEventKind,
        offset: i64,
        migration_id: Option<&str>,
    ) -> Result<ExecutionReceipt, D1Error> {
        append_execution_event(
            receipt,
            ExecutionEventInput {
                kind,
                occurred_at_unix_seconds: T0 + offset,
                migration_id: migration_id.map(str::to_owned),
            },
        )
    }

    #[test]
    fn exact_target_fence_is_verified_without_newer_holder() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let verified = verify_target_fence(&lease, &observation(&lease))?;
        assert_eq!(verified.status, "TARGET_FENCE_VERIFIED");
        assert_eq!(verified.fence_id, lease.fence_id);
        Ok(())
    }

    #[test]
    fn newer_same_target_fence_rejects_stale_executor() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let newer = acquire_target_fence(ordinary_input(8, "db-1"))?;
        let mut observed = observation(&lease);
        observed.observed_acquired_leases.push(newer);
        assert!(verify_target_fence(&lease, &observed).is_err());
        Ok(())
    }

    #[test]
    fn newer_other_target_does_not_steal_fence() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let newer = acquire_target_fence(ordinary_input(8, "db-2"))?;
        let mut observed = observation(&lease);
        observed.observed_acquired_leases.push(newer);
        verify_target_fence(&lease, &observed)?;
        Ok(())
    }

    #[test]
    fn unknown_history_fails_closed_before_write() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let mut observed = observation(&lease);
        observed.history_complete = false;
        assert!(verify_target_fence(&lease, &observed).is_err());
        Ok(())
    }

    #[test]
    fn rerun_cannot_acquire_target_fence() {
        let mut input = ordinary_input(7, "db-1");
        input.run_attempt = 2;
        assert!(acquire_target_fence(input).is_err());
    }

    #[test]
    fn successful_mutation_receipt_is_strictly_append_only() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let mut receipt = initialize_execution_receipt(receipt_seed(lease), T0 + 20, T0 + 21)?;
        receipt = append(&receipt, ExecutionEventKind::PrewriteFencePass, 22, None)?;
        receipt = append(&receipt, ExecutionEventKind::MutationStarted, 23, None)?;
        receipt = append(
            &receipt,
            ExecutionEventKind::MigrationApplied,
            24,
            Some("0031_device_binding_governance.sql"),
        )?;
        receipt = append(&receipt, ExecutionEventKind::PostObserved, 25, None)?;
        receipt = append(&receipt, ExecutionEventKind::Verified, 26, None)?;
        receipt = append(&receipt, ExecutionEventKind::Completed, 27, None)?;
        assert_eq!(receipt.events.len(), 8);
        assert_eq!(receipt.events.last().map(|event| event.kind), Some(ExecutionEventKind::Completed));
        Ok(())
    }

    #[test]
    fn proven_prewrite_abort_can_only_end_failed_no_effect() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let receipt = initialize_execution_receipt(receipt_seed(lease), T0 + 20, T0 + 21)?;
        let receipt = append(&receipt, ExecutionEventKind::PrewriteAborted, 22, None)?;
        let receipt = append(&receipt, ExecutionEventKind::FailedNoEffect, 23, None)?;
        assert_eq!(receipt.events.last().map(|event| event.kind), Some(ExecutionEventKind::FailedNoEffect));
        assert!(append(&receipt, ExecutionEventKind::Completed, 24, None).is_err());
        Ok(())
    }

    #[test]
    fn mutation_started_failure_requires_recovery_terminal() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let mut receipt = initialize_execution_receipt(receipt_seed(lease), T0 + 20, T0 + 21)?;
        receipt = append(&receipt, ExecutionEventKind::PrewriteFencePass, 22, None)?;
        receipt = append(&receipt, ExecutionEventKind::MutationStarted, 23, None)?;
        assert!(append(&receipt, ExecutionEventKind::FailedNoEffect, 24, None).is_err());
        let receipt = append(&receipt, ExecutionEventKind::RecoveryRequired, 24, None)?;
        assert_eq!(receipt.events.last().map(|event| event.kind), Some(ExecutionEventKind::RecoveryRequired));
        Ok(())
    }

    #[test]
    fn migration_applied_requires_mutation_started_and_unique_id() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let mut receipt = initialize_execution_receipt(receipt_seed(lease), T0 + 20, T0 + 21)?;
        receipt = append(&receipt, ExecutionEventKind::PrewriteFencePass, 22, None)?;
        assert!(append(&receipt, ExecutionEventKind::MigrationApplied, 23, Some("0031.sql")).is_err());
        receipt = append(&receipt, ExecutionEventKind::MutationStarted, 23, None)?;
        receipt = append(&receipt, ExecutionEventKind::MigrationApplied, 24, Some("0031.sql"))?;
        assert!(append(&receipt, ExecutionEventKind::MigrationApplied, 25, Some("0031.sql")).is_err());
        Ok(())
    }

    #[test]
    fn no_op_receipt_can_verify_without_mutation_started() -> Result<(), D1Error> {
        let lease = acquire_target_fence(ordinary_input(7, "db-1"))?;
        let mut receipt = initialize_execution_receipt(receipt_seed(lease), T0 + 20, T0 + 21)?;
        receipt = append(&receipt, ExecutionEventKind::PrewriteFencePass, 22, None)?;
        receipt = append(&receipt, ExecutionEventKind::PostObserved, 23, None)?;
        receipt = append(&receipt, ExecutionEventKind::Verified, 24, None)?;
        receipt = append(&receipt, ExecutionEventKind::Completed, 25, None)?;
        assert_eq!(receipt.events.last().map(|event| event.kind), Some(ExecutionEventKind::Completed));
        Ok(())
    }

    #[test]
    fn contract_fence_keeps_separate_authority_shape() -> Result<(), D1Error> {
        let release_id = format!("{RELEASE_SET_PREFIX}{}", "44".repeat(32));
        let lease = acquire_target_fence(TargetFenceLeaseInput {
            schema_version: 1,
            target: target("db-1"),
            phase: TransactionPhase::Contract,
            operation_identity: release_id,
            transaction_id: None,
            authorization_digest: None,
            source_sha: "33".repeat(20),
            executor_run_id: 100,
            fence_epoch: 50,
            run_attempt: 1,
            acquired_at_unix_seconds: T0,
        })?;
        assert_eq!(lease.phase, TransactionPhase::Contract);
        assert!(lease.transaction_id.is_none());
        Ok(())
    }
}
