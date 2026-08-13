use application_ports::CommandExecutionEvidence;
use application_ports::outbound_mail::{
    OutboundMailClaimDecision, OutboundMailIntent, OutboundMailIntentApplicationPort,
    OutboundMailIntentPortError, OutboundMailIntentPortErrorClass, OutboundMailIntentReceipt,
    OutboundMailIntentState, OutboundMailProviderOutcome, OutboundMailReserveDecision,
    ProviderMessageReference,
};
use profile_platform_primitives::{ActorContext, OutboxEventId};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct StoredIntent {
    idempotency_key: String,
    request_digest: String,
    intent_id: OutboxEventId,
    state: OutboundMailIntentState,
    attempt_count: u8,
    provider_message_reference: Option<ProviderMessageReference>,
}

impl StoredIntent {
    fn receipt(&self) -> OutboundMailIntentReceipt {
        OutboundMailIntentReceipt::new(
            self.intent_id.clone(),
            self.state,
            self.attempt_count,
            self.provider_message_reference.clone(),
        )
    }
}

pub(crate) struct FakeStore {
    stored: Mutex<Option<StoredIntent>>,
    reserve_calls: AtomicUsize,
}

impl FakeStore {
    pub(crate) const fn new() -> Self {
        Self {
            stored: Mutex::new(None),
            reserve_calls: AtomicUsize::new(0),
        }
    }

    pub(crate) fn reserve_calls(&self) -> usize {
        self.reserve_calls.load(Ordering::SeqCst)
    }
}

impl OutboundMailIntentApplicationPort for FakeStore {
    async fn reserve_intent(
        &self,
        _actor: &ActorContext,
        _intent: &OutboundMailIntent,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailReserveDecision, OutboundMailIntentPortError> {
        self.reserve_calls.fetch_add(1, Ordering::SeqCst);
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        if let Some(existing) = stored.as_ref() {
            if existing.idempotency_key == evidence.idempotency_key().as_str()
                && existing.request_digest == evidence.request_digest()
            {
                return Ok(OutboundMailReserveDecision::Existing(existing.receipt()));
            }
            return Ok(OutboundMailReserveDecision::Conflict);
        }
        *stored = Some(StoredIntent {
            idempotency_key: evidence.idempotency_key().as_str().to_owned(),
            request_digest: evidence.request_digest().to_owned(),
            intent_id: evidence.outbox_event_id().clone(),
            state: OutboundMailIntentState::Pending,
            attempt_count: 0,
            provider_message_reference: None,
        });
        Ok(OutboundMailReserveDecision::Reserved)
    }

    async fn claim_dispatch(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
        max_attempts: u8,
    ) -> Result<OutboundMailClaimDecision, OutboundMailIntentPortError> {
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        let Some(existing) = stored.as_mut() else {
            return Err(not_found());
        };
        if !matches!(
            existing.state,
            OutboundMailIntentState::Pending | OutboundMailIntentState::Retryable
        ) {
            return Ok(OutboundMailClaimDecision::Existing(existing.receipt()));
        }
        if existing.attempt_count >= max_attempts {
            existing.state = OutboundMailIntentState::Rejected;
            return Ok(OutboundMailClaimDecision::Existing(existing.receipt()));
        }
        existing.attempt_count = existing
            .attempt_count
            .checked_add(1)
            .ok_or_else(internal_failure)?;
        existing.state = OutboundMailIntentState::Dispatching;
        Ok(OutboundMailClaimDecision::Claimed {
            attempt: existing.attempt_count,
        })
    }

    async fn complete_dispatch(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
        outcome: &OutboundMailProviderOutcome,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        let Some(existing) = stored.as_mut() else {
            return Err(not_found());
        };
        match outcome {
            OutboundMailProviderOutcome::Sent {
                provider_message_reference,
            } => {
                existing.state = OutboundMailIntentState::Sent;
                existing.provider_message_reference = provider_message_reference.clone();
            }
            OutboundMailProviderOutcome::RetryableNotSent => {
                existing.state = OutboundMailIntentState::Retryable;
                existing.provider_message_reference = None;
            }
            OutboundMailProviderOutcome::Rejected => {
                existing.state = OutboundMailIntentState::Rejected;
                existing.provider_message_reference = None;
            }
            OutboundMailProviderOutcome::Ambiguous => {
                existing.state = OutboundMailIntentState::Ambiguous;
                existing.provider_message_reference = None;
            }
        }
        Ok(existing.receipt())
    }

    async fn mark_ambiguous(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        let Some(existing) = stored.as_mut() else {
            return Err(not_found());
        };
        if existing.state == OutboundMailIntentState::Dispatching {
            existing.state = OutboundMailIntentState::Ambiguous;
        }
        Ok(existing.receipt())
    }
}

const fn not_found() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::NotFound)
}

const fn internal_failure() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::InternalFailure)
}
