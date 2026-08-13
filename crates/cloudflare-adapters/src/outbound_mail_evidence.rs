#![cfg(test)]

#[test]
fn c4_outbound_mail_boundaries_are_permanent_and_retry_safe() {
    let port = include_str!("../../application-ports/src/outbound_mail.rs");
    for required in [
        "OutboundMailOperation",
        "ReplyAll",
        "RetryableNotSent",
        "Ambiguous",
        "OutboundMailIntentApplicationPort",
        "OutboundMailProviderPort",
    ] {
        assert!(
            port.contains(required),
            "missing outbound contract: {required}"
        );
    }
    for forbidden in ["Gmail", "SMTP", "MicrosoftGraph", "Mail.Send"] {
        assert!(
            !port.contains(forbidden),
            "provider-specific outbound type leaked inward: {forbidden}"
        );
    }

    let use_case = include_str!("../../use-cases-mailboxes/src/outbound_mail.rs");
    let access = use_case.find("is_mailbox_accessible").unwrap_or(usize::MAX);
    let reserve = use_case.find("reserve_intent").unwrap_or(0);
    let claim = use_case.find("claim_dispatch").unwrap_or(0);
    let send = use_case.find("provider.send").unwrap_or(0);
    assert!(
        access < reserve,
        "live Client/mailbox access must precede reserve"
    );
    assert!(
        claim < send,
        "durable dispatch claim must precede provider send"
    );
    assert!(use_case.contains("Err(_) => OutboundMailProviderOutcome::Ambiguous"));
    assert!(use_case.contains("MAX_OUTBOUND_MAIL_DISPATCH_ATTEMPTS: u8 = 3"));

    let adapter_facade = include_str!("d1_outbound_mail_intents.rs");
    assert!(adapter_facade.contains("pub use repository::D1OutboundMailIntentRepository"));
    let adapter_sql = include_str!("d1_outbound_mail_intents/sql.rs");
    assert!(adapter_sql.contains("OUTBOUND_EVENT_PAYLOAD: &str = \"{}\""));
    assert!(adapter_sql.contains("mail.outbound_intent_reserved.v1"));
    assert!(adapter_sql.contains("outbound_mail_dispatch_claims"));
    assert!(adapter_sql.contains("outbound_mail_dispatch_completions"));
    assert!(adapter_sql.contains("outbound_mail_ambiguity_marks"));

    let migration = include_str!("../../../migrations/d1/0026_outbound_mail_intents.sql");
    for required in [
        "'PENDING', 'DISPATCHING', 'RETRYABLE', 'SENT', 'AMBIGUOUS', 'REJECTED'",
        "outbound_mail_dispatch_claim_validate",
        "outbound_mail_dispatch_completion_validate",
        "outbound_mail_ambiguity_mark_validate",
        "association.client_id = client.client_id",
        "requester.status = 'ACTIVE'",
        "requester.role = 'TENANT_OWNER'",
        "requester.role = 'MEMBER'",
        "client_grants AS grant_row",
        "binding.status = 'ACTIVE'",
        "binding.execution_status = 'ACTIVE'",
    ] {
        assert!(
            required_in(migration, required),
            "missing C4 D1 invariant: {required}"
        );
    }
    for forbidden_column in [
        "body_text",
        "body_html",
        "subject TEXT",
        "recipient TEXT",
        "recipients TEXT",
        "to_address",
        "cc_address",
        "bcc_address",
    ] {
        assert!(
            !migration.contains(forbidden_column),
            "outbound durable state leaked message content: {forbidden_column}"
        );
    }
    assert!(migration.contains("state IN ('DISPATCHING', 'AMBIGUOUS')"));
    assert!(migration.contains("state = 'AMBIGUOUS'"));

    let eligibility = include_str!("d1_client_mail_eligibility.rs");
    assert!(eligibility.contains("impl ClientMailboxEligibilityPort"));
    assert!(eligibility.contains("impl ClientMailboxAccessPort"));
    assert!(eligibility.contains("CLIENT_MAILBOX_ACCESS"));
    assert!(!eligibility.contains("BROWSER_FALLBACK"));
}

fn required_in(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}
