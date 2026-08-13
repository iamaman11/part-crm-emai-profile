use super::mapping::{
    OutboundMailIntentRow, integrity_failure, internal_failure, is_ambiguity_race, is_claim_race,
    is_completion_race, is_unique_conflict, map_dependency_error, map_write_error, not_found,
    provider_outcome_fields, receipt_from_row, sqlite_integer,
};
use super::sql::{
    AUDIT_CREATE, CLAIM_DISPATCH, COMPLETE_DISPATCH, IDEMPOTENCY_CREATE, INTENT_CREATE, LOAD_INTENT,
    MARK_AMBIGUOUS, OUTBOUND_COMMAND, OUTBOUND_EVENT_PAYLOAD, OUTBOX_CREATE, REJECT_EXHAUSTED,
};
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use application_ports::CommandExecutionEvidence;
use application_ports::outbound_mail::{
    OutboundMailClaimDecision, OutboundMailIntent, OutboundMailIntentApplicationPort,
    OutboundMailIntentPortError, OutboundMailIntentPortErrorClass, OutboundMailIntentReceipt,
    OutboundMailIntentState, OutboundMailProviderOutcome, OutboundMailReserveDecision,
};
use profile_platform_primitives::ActorContext;
use worker::d1::D1Database;
use worker::query;

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
            .batch(vec![
                intent_create,
                idempotency_create,
                audit_create,
                outbox_create,
            ])
            .await
        {
            Ok(_) => Ok(OutboundMailReserveDecision::Reserved),
            Err(error) if is_unique_conflict(&error) => {
                self.replay_after_reservation_race(actor, evidence).await
            }
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
