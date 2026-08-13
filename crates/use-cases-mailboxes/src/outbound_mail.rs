use application_ports::CommandExecutionEvidence;
use application_ports::client_mail_access::{
    ClientMailboxAccessPort, ClientMailboxAccessPortError, ClientMailboxAccessPortErrorClass,
};
use application_ports::outbound_mail::{
    OutboundMailClaimDecision, OutboundMailInputError, OutboundMailIntent,
    OutboundMailIntentApplicationPort, OutboundMailIntentPortError, OutboundMailIntentPortErrorClass,
    OutboundMailIntentReceipt, OutboundMailIntentState, OutboundMailProviderOutcome,
    OutboundMailProviderPort, OutboundMailReserveDecision,
};
use core::fmt;
use profile_platform_primitives::{ActorContext, OutboxEventId};

pub const MAX_OUTBOUND_MAIL_DISPATCH_ATTEMPTS: u8 = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundMailOutcome {
    intent_id: OutboxEventId,
    state: OutboundMailIntentState,
    attempt_count: u8,
    replayed: bool,
}

impl OutboundMailOutcome {
    #[must_use]
    pub const fn intent_id(&self) -> &OutboxEventId {
        &self.intent_id
    }

    #[must_use]
    pub const fn state(&self) -> OutboundMailIntentState {
        self.state
    }

    #[must_use]
    pub const fn attempt_count(&self) -> u8 {
        self.attempt_count
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundMailOperationError {
    NotFound,
    InvalidInput,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for OutboundMailOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "outbound mailbox target not found",
            Self::InvalidInput => "outbound mail input is invalid",
            Self::Conflict => "outbound mail command conflict",
            Self::IntegrityFailure => "outbound mail integrity failure",
            Self::InternalFailure => "outbound mail internal failure",
            Self::DependencyUnavailable => "outbound mail dependency unavailable",
        })
    }
}

impl std::error::Error for OutboundMailOperationError {}

pub async fn execute_outbound_mail<A, S, P>(
    actor: &ActorContext,
    access: &A,
    store: &S,
    provider: &P,
    intent: &OutboundMailIntent,
    evidence: &CommandExecutionEvidence,
) -> Result<OutboundMailOutcome, OutboundMailOperationError>
where
    A: ClientMailboxAccessPort,
    S: OutboundMailIntentApplicationPort,
    P: OutboundMailProviderPort,
{
    let accessible = access
        .is_mailbox_accessible(actor, intent.client_id(), intent.binding_id())
        .await
        .map_err(map_access_error)?;
    if !accessible {
        return Err(OutboundMailOperationError::NotFound);
    }

    intent
        .validate_source_binding()
        .map_err(map_input_error)?;

    let reserve = store
        .reserve_intent(actor, intent, evidence)
        .await
        .map_err(map_store_error)?;

    match reserve {
        OutboundMailReserveDecision::Conflict => Err(OutboundMailOperationError::Conflict),
        OutboundMailReserveDecision::Reserved => {
            dispatch(actor, store, provider, intent, evidence, false).await
        }
        OutboundMailReserveDecision::Existing(receipt) => {
            resume_or_replay(actor, store, provider, intent, evidence, receipt).await
        }
    }
}

async fn resume_or_replay<S, P>(
    actor: &ActorContext,
    store: &S,
    provider: &P,
    intent: &OutboundMailIntent,
    evidence: &CommandExecutionEvidence,
    receipt: OutboundMailIntentReceipt,
) -> Result<OutboundMailOutcome, OutboundMailOperationError>
where
    S: OutboundMailIntentApplicationPort,
    P: OutboundMailProviderPort,
{
    match receipt.state() {
        OutboundMailIntentState::Pending | OutboundMailIntentState::Retryable => {
            dispatch(actor, store, provider, intent, evidence, true).await
        }
        OutboundMailIntentState::Dispatching => {
            let receipt = store
                .mark_ambiguous(actor, evidence)
                .await
                .unwrap_or(receipt);
            Ok(outcome_from_receipt(receipt, true))
        }
        OutboundMailIntentState::Sent
        | OutboundMailIntentState::Ambiguous
        | OutboundMailIntentState::Rejected => Ok(outcome_from_receipt(receipt, true)),
    }
}

async fn dispatch<S, P>(
    actor: &ActorContext,
    store: &S,
    provider: &P,
    intent: &OutboundMailIntent,
    evidence: &CommandExecutionEvidence,
    replayed: bool,
) -> Result<OutboundMailOutcome, OutboundMailOperationError>
where
    S: OutboundMailIntentApplicationPort,
    P: OutboundMailProviderPort,
{
    let claim = store
        .claim_dispatch(actor, evidence, MAX_OUTBOUND_MAIL_DISPATCH_ATTEMPTS)
        .await
        .map_err(map_store_error)?;

    let attempt = match claim {
        OutboundMailClaimDecision::Claimed { attempt } => attempt,
        OutboundMailClaimDecision::Existing(receipt) => {
            if receipt.state() == OutboundMailIntentState::Dispatching {
                let receipt = store
                    .mark_ambiguous(actor, evidence)
                    .await
                    .unwrap_or(receipt);
                return Ok(outcome_from_receipt(receipt, true));
            }
            return Ok(outcome_from_receipt(receipt, true));
        }
    };

    let provider_outcome = match provider.send(actor, intent).await {
        Ok(outcome) => outcome,
        Err(_) => OutboundMailProviderOutcome::Ambiguous,
    };

    match store
        .complete_dispatch(actor, evidence, &provider_outcome)
        .await
    {
        Ok(receipt) => Ok(outcome_from_receipt(receipt, replayed)),
        Err(_) => {
            let fallback = store.mark_ambiguous(actor, evidence).await.ok();
            Ok(fallback.map_or_else(
                || OutboundMailOutcome {
                    intent_id: evidence.outbox_event_id().clone(),
                    state: OutboundMailIntentState::Ambiguous,
                    attempt_count: attempt,
                    replayed,
                },
                |receipt| outcome_from_receipt(receipt, replayed),
            ))
        }
    }
}

fn outcome_from_receipt(
    receipt: OutboundMailIntentReceipt,
    replayed: bool,
) -> OutboundMailOutcome {
    OutboundMailOutcome {
        intent_id: receipt.intent_id().clone(),
        state: receipt.state(),
        attempt_count: receipt.attempt_count(),
        replayed,
    }
}

const fn map_input_error(_error: OutboundMailInputError) -> OutboundMailOperationError {
    OutboundMailOperationError::InvalidInput
}

const fn map_access_error(error: ClientMailboxAccessPortError) -> OutboundMailOperationError {
    match error.class() {
        ClientMailboxAccessPortErrorClass::IntegrityFailure => {
            OutboundMailOperationError::IntegrityFailure
        }
        ClientMailboxAccessPortErrorClass::DependencyUnavailable => {
            OutboundMailOperationError::DependencyUnavailable
        }
    }
}

const fn map_store_error(error: OutboundMailIntentPortError) -> OutboundMailOperationError {
    match error.class() {
        OutboundMailIntentPortErrorClass::NotFound => OutboundMailOperationError::NotFound,
        OutboundMailIntentPortErrorClass::Conflict => OutboundMailOperationError::Conflict,
        OutboundMailIntentPortErrorClass::IntegrityFailure => {
            OutboundMailOperationError::IntegrityFailure
        }
        OutboundMailIntentPortErrorClass::InternalFailure => {
            OutboundMailOperationError::InternalFailure
        }
        OutboundMailIntentPortErrorClass::DependencyUnavailable => {
            OutboundMailOperationError::DependencyUnavailable
        }
    }
}
