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
    assert!(!use_case.contains(".unwrap_or(receipt)"));

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
    assert_eq!(
        eligibility
            .matches("self.is_accessible(actor, client_id, binding_id)")
            .count(),
        2
    );
    for required in [
        "association.client_id = client.client_id",
        "client.status = 'ACTIVE'",
        "binding.status = 'ACTIVE'",
        "binding.execution_status = 'ACTIVE'",
        "requester.status = 'ACTIVE'",
        "client_grants AS grant_row",
        "binding.provider IN ('GMAIL_API', 'IMAP')",
        "OR binding.provider = 'MICROSOFT_GRAPH'",
    ] {
        assert!(
            eligibility.contains(required),
            "missing shared Client Mail access predicate: {required}"
        );
    }
    assert!(!eligibility.contains("BROWSER_FALLBACK"));
}

#[test]
fn c5_gmail_send_boundaries_are_provider_local_and_retry_safe() {
    let c2 = include_str!("gmail_oauth_provisioning.rs");
    assert!(c2.contains("https://www.googleapis.com/auth/gmail.readonly"));
    assert!(!c2.contains("https://www.googleapis.com/auth/gmail.send"));

    let application = include_str!("../../application-ports/src/outbound_mail.rs");
    for forbidden in [
        "gmail.googleapis.com",
        "users/me/messages/send",
        "threadId",
        "In-Reply-To",
        "References",
    ] {
        assert!(
            !application.contains(forbidden),
            "Gmail send transport leaked inward: {forbidden}"
        );
    }

    let consent = include_str!("gmail_send_capability.rs");
    assert!(consent.contains("https://www.googleapis.com/auth/gmail.send"));
    assert!(consent.contains("x-profile-oauth-include-granted-scopes"));
    assert!(consent.contains("gmail/send/oauth/start"));
    assert!(consent.contains("gmail/send/oauth/complete"));

    let credential = include_str!("gmail_send_credential.rs");
    assert!(credential.contains("gmail/send/resolve"));
    assert!(credential.contains("x-profile-mailbox-capability"));
    assert!(credential.contains("\"SEND\""));

    let provider = include_str!("gmail_outbound_mail.rs");
    assert!(provider.contains("impl OutboundMailProviderPort"));
    let binding = provider.find("find_binding").unwrap_or(usize::MAX);
    let secret = provider
        .find("resolve_gmail_send_credential")
        .unwrap_or(usize::MAX);
    let source = provider
        .find("resolve_message_context")
        .unwrap_or(usize::MAX);
    let render = provider.find("render_mime").unwrap_or(usize::MAX);
    let send = provider.find("send_gmail_message").unwrap_or(usize::MAX);
    assert!(binding < secret && secret < source && source < render && render < send);
    assert!(provider.contains("408 | 425 | 500..=599 => SendStatus::Ambiguous"));
    assert!(provider.contains("429 => SendStatus::RetryableNotSent"));
    assert!(provider.contains("400..=499 => SendStatus::Rejected"));
    assert!(provider.contains("format!(\"gmail:{id}\")"));

    let source_translation = include_str!("gmail_outbound_mail/source.rs");
    assert!(source_translation.contains("GMAIL_PROFILE_ENDPOINT"));
    assert!(source_translation.contains("GMAIL_MESSAGES_ENDPOINT"));
    assert!(source_translation.contains("Message-ID"));
    assert!(source_translation.contains("References"));
    assert!(source_translation.contains("thread_id: Some(metadata.thread_id)"));
    assert!(!source_translation.contains("deny_unknown_fields"));

    let mime = include_str!("gmail_outbound_mail/mime.rs");
    assert!(mime.contains("Content-Transfer-Encoding: base64"));
    assert!(mime.contains("encode_base64url_unpadded"));
    assert!(mime.contains("multipart/alternative"));

    let c4 = include_str!("../../use-cases-mailboxes/src/outbound_mail.rs");
    let claim = c4.find("claim_dispatch").unwrap_or(usize::MAX);
    let provider_call = c4.find("provider.send").unwrap_or(usize::MAX);
    assert!(claim < provider_call, "durable claim must precede Gmail send");

    for content in [consent, credential, provider, source_translation] {
        for forbidden in ["println!", "console_log!", "console_error!", "log::"] {
            assert!(!content.contains(forbidden));
        }
    }
}

fn required_in(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}
