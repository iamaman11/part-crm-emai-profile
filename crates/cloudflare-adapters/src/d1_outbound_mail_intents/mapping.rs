use application_ports::outbound_mail::{
    OutboundMailIntentPortError, OutboundMailIntentPortErrorClass, OutboundMailIntentReceipt,
    OutboundMailIntentState, OutboundMailProviderOutcome, ProviderMessageReference,
};
use profile_platform_primitives::OutboxEventId;
use serde::Deserialize;
use worker::Error;

#[derive(Deserialize)]
pub(super) struct OutboundMailIntentRow {
    intent_id: String,
    state: String,
    attempt_count: i64,
    provider_message_reference: Option<String>,
}

pub(super) fn receipt_from_row(
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

pub(super) fn parse_state(
    value: &str,
) -> Result<OutboundMailIntentState, OutboundMailIntentPortError> {
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

pub(super) fn provider_outcome_fields(
    outcome: &OutboundMailProviderOutcome,
) -> (&'static str, Option<&str>) {
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

pub(super) fn is_unique_conflict(error: &Error) -> bool {
    error.to_string().contains("UNIQUE constraint failed")
}

pub(super) fn is_claim_race(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("UNIQUE constraint failed")
        || message.contains("outbound_mail_claim_state_invalid")
}

pub(super) fn is_completion_race(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("UNIQUE constraint failed")
        || message.contains("outbound_mail_completion_state_invalid")
}

pub(super) fn is_ambiguity_race(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("UNIQUE constraint failed")
        || message.contains("outbound_mail_ambiguity_state_invalid")
}

pub(super) fn map_write_error(error: Error) -> OutboundMailIntentPortError {
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

pub(super) fn map_dependency_error(_error: Error) -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::DependencyUnavailable)
}

pub(super) fn sqlite_integer(value: u64) -> Result<i64, OutboundMailIntentPortError> {
    i64::try_from(value).map_err(|_| internal_failure())
}

pub(super) const fn not_found() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::NotFound)
}

pub(super) const fn integrity_failure() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::IntegrityFailure)
}

pub(super) const fn internal_failure() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::InternalFailure)
}
