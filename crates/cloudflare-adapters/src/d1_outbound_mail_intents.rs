use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use application_ports::CommandExecutionEvidence;
use application_ports::outbound_mail::{
    OutboundMailClaimDecision, OutboundMailIntent, OutboundMailIntentApplicationPort,
    OutboundMailIntentPortError, OutboundMailIntentPortErrorClass, OutboundMailIntentReceipt,
    OutboundMailIntentState, OutboundMailProviderOutcome, OutboundMailReserveDecision,
    ProviderMessageReference,
};
use profile_platform_primitives::{ActorContext, OutboxEventId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, query};

const OUTBOUND_COMMAND: &str = "mail.outbound_send";
const OUTBOUND_EVENT_PAYLOAD: &str = "{}";

const LOAD_INTENT: &str = r#"
SELECT intent_id, state, attempt_count, provider_message_reference
FROM outbound_mail_intents
WHERE tenant_id = ?
  AND command_actor_id = ?
  AND idempotency_key = ?
  AND request_digest = ?
LIMIT 1
"#;

const INTENT_CREATE: &str = r#"
INSERT INTO outbound_mail_intents (
    tenant_id, intent_id, command_actor_id, idempotency_key, request_digest,
    client_id, binding_id, operation, state, attempt_count,
    provider_message_reference, created_at_ms, updated_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'PENDING', 0, NULL, ?, ?)
"#;

const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, request_digest,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, ?, ?, 'reserved', ?, ?, ?)
"#;

const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, 'mail.outbound_send', 'outbound_mail_intent', ?, 'accepted', ?)
"#;

const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, 'outbound_mail_intent', ?, 1, 'mail.outbound_intent_reserved.v1', ?, ?)
"#;

const CLAIM_DISPATCH: &str = r#"
INSERT INTO outbound_mail_dispatch_claims (
    tenant_id, intent_id, attempt, claimed_at_ms
) VALUES (?, ?, ?, ?)
"#;

const REJECT_EXHAUSTED: &str = r#"
UPDATE outbound_mail_intents
SET state = 'REJECTED', updated_at_ms = ?
WHERE tenant_id = ?
  AND intent_id = ?
  AND state IN ('PENDING', 'RETRYABLE')
"#;

const COMPLETE_DISPATCH: &str = r#"
INSERT INTO outbound_mail_dispatch_completions (
    tenant_id, intent_id, attempt, outcome, provider_message_reference, completed_at_ms
) VALUES (?, ?, ?, ?, ?, ?)
"#;

const MARK_AMBIGUOUS: &str = r#"
INSERT INTO outbound_mail_ambiguity_marks (
    tenant_id, intent_id, attempt, marked_at_ms
) VALUES (?, ?, ?, ?)
"#;

pub struct D1OutboundMailIntentRepository {
    database: D1Database,
    idempotency: D1IdempotencyRepository,
}

impl D1OutboundMailIntentRepository {
    #[must_use]
    pub const fn new(database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            database,
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }

    async fn load_receipt(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
    ) -> Result<Option<OutboundMailIntentReceipt>, OutboundMailIntentPortError> {
        query!(
            &self.database,
            LOAD_INTENT,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            evidence.idempotency_key().as_str(),
            evidence.request_digest(),
        )
        .map_err(map_dependency_error)?
        .first::<OutboundMailIntentRow>(None)
        .await
        .map_err(map_dependency_error)?
        .map(receipt_from_row)
        .transpose()
    }

    async fn reserve_new(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailReserveDecision, OutboundMailIntentPortError> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let intent_id = evidence.outbox_event_id().as_str();
        let now = sqlite_integer(evidence.now().value())?;
        let expires_at = sqlite_integer(evidence.idempotency_expires_at().value())?;

        let intent_create = query!(
            &self.database,
            INTENT_CREATE,
            tenant_id,
            intent_id,
            actor_id,
            evidence.idempotency_key().as_str(),
            evidence.request_digest(),
            intent.client_id().as_str(),
            intent.binding_id().as_str(),
            intent.operation().kind_code(),
            now,
            now,
        )
        .map_err(map_dependency_error)?;
        let idempotency_create = query!(
            &self.database,
            IDEMPOTENCY_CREATE,
            tenant_id,
            actor_id,
            evidence.idempotency_key().as_str(),
            OUTBOUND_COMMAND,
            evidence.request_digest(),
            intent_id,
            now,
            expires_at,
        )
        .map_err(map_dependency_error)?;
        let audit_create = query!(
            &self.database,
            AUDIT_CREATE,
            tenant_id,
            evidence.audit_event_id().as_str(),
            actor.correlation_id().as_str(),
            actor_id,
            intent_id,
            now,
        )
        .map_err(map_dependency_error)?;
        let outbox_create = query!(
            &self.database,
            OUTBOX_CREATE,
            tenant_id,
            intent_id,
            intent_id,
            OUTBOUND_EVENT_PAYLOAD,
            now,
        )
        .map_err(map_dependency_error)?;

        match self
            .database
            .batch(vec![intent_create, idempotency_create, audit_create, outbox_create])
            .await
        {
            Ok(_) => Ok(OutboundMailReserveDecision::Reserved),
            Err(error) if is_unique_conflict(&error) => self.replay_after_reservation_race(actor, evidence).await,
            Err(error) => Err(map_write_error(error)),
        }
    }

    async fn replay_after_reservation_race(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailReserveDecision, OutboundMailIntentPortError> {
        match self
            .idempotency
            .decide(
                actor.tenant_scope(),
                actor.actor_id(),
                evidence.idempotency_key(),
                OUTBOUND_COMMAND,
                evidence.request_digest(),
                evidence.now(),
            )
            .await
            .map_err(map_dependency_error)?
        {
            IdempotencyDecision::Conflict => Ok(OutboundMailReserveDecision::Conflict),
            IdempotencyDecision::Miss => Err(OutboundMailIntentPortError::new(
                OutboundMailIntentPortErrorClass::Conflict,
            )),
            IdempotencyDecision::Replay(_) => self
                .load_receipt(actor, evidence)
                .await?
                .map(OutboundMailReserveDecision::Existing)
                .ok_or_else(integrity_failure),
        }
    }

    async fn reject_exhausted(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
        receipt: &OutboundMailIntentReceipt,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        let now = sqlite_integer(evidence.now().value())?;
        let statement = query!(
            &self.database,
            REJECT_EXHAUSTED,
            now,
            actor.tenant_scope().tenant_id().as_str(),
            receipt.intent_id().as_str(),
        )
        .map_err(map_dependency_error)?;
        self.database
            .batch(vec![statement])
            .await
            .map_err(map_write_error)?;
        self.load_receipt(actor, evidence)
            .await?
            .ok_or_else(integrity_failure)
    }
}

impl OutboundMailIntentApplicationPort for D1OutboundMailIntentRepository {
    async fn reserve_intent(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailReserveDecision, OutboundMailIntentPortError> {
        match self
            .idempotency
            .decide(
                actor.tenant_scope(),
                actor.actor_id(),
                evidence.idempotency_key(),
                OUTBOUND_COMMAND,
                evidence.request_digest(),
                evidence.now(),
            )
            .await
            .map_err(map_dependency_error)?
        {
            IdempotencyDecision::Miss => self.reserve_new(actor, intent, evidence).await,
            IdempotencyDecision::Conflict => Ok(OutboundMailReserveDecision::Conflict),
            IdempotencyDecision::Replay(_) => self
                .load_receipt(actor, evidence)
                .await?
                .map(OutboundMailReserveDecision::Existing)
                .ok_or_else(integrity_failure),
        }
    }

    async fn claim_dispatch(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
        max_attempts: u8,
    ) -> Result<OutboundMailClaimDecision, OutboundMailIntentPortError> {
        let receipt = self
            .load_receipt(actor, evidence)
            .await?
            .ok_or_else(not_found)?;
        if !matches!(
            receipt.state(),
            OutboundMailIntentState::Pending | OutboundMailIntentState::Retryable
        ) {
            return Ok(OutboundMailClaimDecision::Existing(receipt));
        }
        if receipt.attempt_count() >= max_attempts {
            return self
                .reject_exhausted(actor, evidence, &receipt)
                .await
                .map(OutboundMailClaimDecision::Existing);
        }

        let attempt = receipt
            .attempt_count()
            .checked_add(1)
            .ok_or_else(internal_failure)?;
        let claimed_at = sqlite_integer(evidence.now().value())?;
        let statement = query!(
            &self.database,
            CLAIM_DISPATCH,
            actor.tenant_scope().tenant_id().as_str(),
            receipt.intent_id().as_str(),
            i64::from(attempt),
            claimed_at,
        )
        .map_err(map_dependency_error)?;

        match self.database.batch(vec![statement]).await {
            Ok(_) => Ok(OutboundMailClaimDecision::Claimed { attempt }),
            Err(error) if is_claim_race(&error) => self
                .load_receipt(actor, evidence)
                .await?
                .map(OutboundMailClaimDecision::Existing)
                .ok_or_else(integrity_failure),
            Err(error) => Err(map_write_error(error)),
        }
    }

    async fn complete_dispatch(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
        outcome: &OutboundMailProviderOutcome,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        let receipt = self
            .load_receipt(actor, evidence)
            .await?
            .ok_or_else(not_found)?;
        if !matches!(
            receipt.state(),
            OutboundMailIntentState::Dispatching | OutboundMailIntentState::Ambiguous
        ) {
            return Ok(receipt);
        }

        let (outcome_code, provider_reference) = provider_outcome_fields(outcome);
        let completed_at = sqlite_integer(evidence.now().value())?;
        let statement = query!(
            &self.database,
            COMPLETE_DISPATCH,
            actor.tenant_scope().tenant_id().as_str(),
            receipt.intent_id().as_str(),
            i64::from(receipt.attempt_count()),
            outcome_code,
            provider_reference,
            completed_at,
        )
        .map_err(map_dependency_error)?;

        match self.database.batch(vec![statement]).await {
            Ok(_) => self
                .load_receipt(actor, evidence)
                .await?
                .ok_or_else(integrity_failure),
            Err(error) if is_completion_race(&error) => self
                .load_receipt(actor, evidence)
                .await?
                .ok_or_else(integrity_failure),
            Err(error) => Err(map_write_error(error)),
        }
    }

    async fn mark_ambiguous(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        let receipt = self
            .load_receipt(actor, evidence)
            .await?
            .ok_or_else(not_found)?;
        if receipt.state() != OutboundMailIntentState::Dispatching {
            return Ok(receipt);
        }

        let marked_at = sqlite_integer(evidence.now().value())?;
        let statement = query!(
            &self.database,
            MARK_AMBIGUOUS,
            actor.tenant_scope().tenant_id().as_str(),
            receipt.intent_id().as_str(),
            i64::from(receipt.attempt_count()),
            marked_at,
        )
        .map_err(map_dependency_error)?;

        match self.database.batch(vec![statement]).await {
            Ok(_) => self
                .load_receipt(actor, evidence)
                .await?
                .ok_or_else(integrity_failure),
            Err(error) if is_ambiguity_race(&error) => self
                .load_receipt(actor, evidence)
                .await?
                .ok_or_else(integrity_failure),
            Err(error) => Err(map_write_error(error)),
        }
    }
}

#[derive(Deserialize)]
struct OutboundMailIntentRow {
    intent_id: String,
    state: String,
    attempt_count: i64,
    provider_message_reference: Option<String>,
}

fn receipt_from_row(
    row: OutboundMailIntentRow,
) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
    let intent_id = OutboxEventId::parse(row.intent_id).map_err(|_| integrity_failure())?;
    let state = parse_state(&row.state)?;
    let attempt_count = u8::try_from(row.attempt_count).map_err(|_| integrity_failure())?;
    let provider_message_reference = row
        .provider_message_reference
        .map(ProviderMessageReference::parse)
        .transpose()
        .map_err(|_| integrity_failure())?;
    Ok(OutboundMailIntentReceipt::new(
        intent_id,
        state,
        attempt_count,
        provider_message_reference,
    ))
}

fn parse_state(value: &str) -> Result<OutboundMailIntentState, OutboundMailIntentPortError> {
    match value {
        "PENDING" => Ok(OutboundMailIntentState::Pending),
        "DISPATCHING" => Ok(OutboundMailIntentState::Dispatching),
        "RETRYABLE" => Ok(OutboundMailIntentState::Retryable),
        "SENT" => Ok(OutboundMailIntentState::Sent),
        "AMBIGUOUS" => Ok(OutboundMailIntentState::Ambiguous),
        "REJECTED" => Ok(OutboundMailIntentState::Rejected),
        _ => Err(integrity_failure()),
    }
}

fn provider_outcome_fields(outcome: &OutboundMailProviderOutcome) -> (&'static str, Option<&str>) {
    match outcome {
        OutboundMailProviderOutcome::Sent {
            provider_message_reference,
        } => (
            "SENT",
            provider_message_reference
                .as_ref()
                .map(ProviderMessageReference::as_str),
        ),
        OutboundMailProviderOutcome::RetryableNotSent => ("RETRYABLE", None),
        OutboundMailProviderOutcome::Rejected => ("REJECTED", None),
        OutboundMailProviderOutcome::Ambiguous => ("AMBIGUOUS", None),
    }
}

fn is_unique_conflict(error: &Error) -> bool {
    error.to_string().contains("UNIQUE constraint failed")
}

fn is_claim_race(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("UNIQUE constraint failed")
        || message.contains("outbound_mail_claim_state_invalid")
}

fn is_completion_race(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("UNIQUE constraint failed")
        || message.contains("outbound_mail_completion_state_invalid")
}

fn is_ambiguity_race(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("UNIQUE constraint failed")
        || message.contains("outbound_mail_ambiguity_state_invalid")
}

fn map_write_error(error: Error) -> OutboundMailIntentPortError {
    let message = error.to_string();
    let class = if message.contains("outbound_mail_access_denied") {
        OutboundMailIntentPortErrorClass::NotFound
    } else if message.contains("UNIQUE constraint failed")
        || message.contains("outbound_mail_claim_state_invalid")
        || message.contains("outbound_mail_completion_state_invalid")
        || message.contains("outbound_mail_ambiguity_state_invalid")
    {
        OutboundMailIntentPortErrorClass::Conflict
    } else if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
    {
        OutboundMailIntentPortErrorClass::IntegrityFailure
    } else if message.contains("value exceeds SQLite INTEGER") {
        OutboundMailIntentPortErrorClass::InternalFailure
    } else {
        OutboundMailIntentPortErrorClass::DependencyUnavailable
    };
    OutboundMailIntentPortError::new(class)
}

fn map_dependency_error(_error: Error) -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::DependencyUnavailable)
}

fn sqlite_integer(value: u64) -> Result<i64, OutboundMailIntentPortError> {
    i64::try_from(value).map_err(|_| internal_failure())
}

const fn not_found() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::NotFound)
}

const fn integrity_failure() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::IntegrityFailure)
}

const fn internal_failure() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::InternalFailure)
}

#[cfg(test)]
mod tests {
    use super::{
        AUDIT_CREATE, INTENT_CREATE, LOAD_INTENT, OUTBOX_CREATE, parse_state,
        provider_outcome_fields,
    };
    use application_ports::outbound_mail::{
        OutboundMailIntentState, OutboundMailProviderOutcome, ProviderMessageReference,
    };

    #[test]
    fn durable_queries_are_metadata_only() {
        for query in [INTENT_CREATE, LOAD_INTENT, AUDIT_CREATE, OUTBOX_CREATE] {
            let normalized = query.to_ascii_lowercase();
            assert!(!normalized.contains("body"));
            assert!(!normalized.contains("subject"));
            assert!(!normalized.contains("recipient"));
            assert!(!normalized.contains("html"));
        }
        assert_eq!(OUTBOX_CREATE.matches('?').count(), 5);
    }

    #[test]
    fn persisted_states_are_provider_neutral() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(parse_state("PENDING")?, OutboundMailIntentState::Pending);
        assert_eq!(parse_state("AMBIGUOUS")?, OutboundMailIntentState::Ambiguous);
        assert_eq!(parse_state("SENT")?, OutboundMailIntentState::Sent);
        assert!(parse_state("GMAIL_SENT").is_err());
        Ok(())
    }

    #[test]
    fn only_confirmed_sent_outcome_carries_provider_reference(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let reference = ProviderMessageReference::parse("provider-message-01")?;
        let sent = OutboundMailProviderOutcome::Sent {
            provider_message_reference: Some(reference),
        };
        assert_eq!(provider_outcome_fields(&sent).0, "SENT");
        assert_eq!(
            provider_outcome_fields(&OutboundMailProviderOutcome::RetryableNotSent),
            ("RETRYABLE", None)
        );
        assert_eq!(
            provider_outcome_fields(&OutboundMailProviderOutcome::Ambiguous),
            ("AMBIGUOUS", None)
        );
        Ok(())
    }
}
