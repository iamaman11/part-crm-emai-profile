use super::mapping::{parse_state, provider_outcome_fields};
use super::sql::{AUDIT_CREATE, INTENT_CREATE, LOAD_INTENT, OUTBOX_CREATE};
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
    assert_eq!(
        parse_state("AMBIGUOUS")?,
        OutboundMailIntentState::Ambiguous
    );
    assert_eq!(parse_state("SENT")?, OutboundMailIntentState::Sent);
    assert!(parse_state("GMAIL_SENT").is_err());
    Ok(())
}

#[test]
fn only_confirmed_sent_outcome_carries_provider_reference() -> Result<(), Box<dyn std::error::Error>>
{
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
