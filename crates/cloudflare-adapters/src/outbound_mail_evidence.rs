#![cfg(test)]

fn has(text: &str, needle: &str) {
    assert!(text.contains(needle));
}

fn lacks(text: &str, needle: &str) {
    assert!(!text.contains(needle));
}

#[test]
fn c4_outbound_mail_boundaries_are_permanent_and_retry_safe() {
    let port = include_str!("../../application-ports/src/outbound_mail.rs");
    for needle in [
        "OutboundMailOperation",
        "ReplyAll",
        "RetryableNotSent",
        "Ambiguous",
        "OutboundMailIntentApplicationPort",
        "OutboundMailProviderPort",
    ] {
        has(port, needle);
    }
    for needle in ["Gmail", "SMTP", "MicrosoftGraph", "Mail.Send"] {
        lacks(port, needle);
    }

    let use_case = include_str!("../../use-cases-mailboxes/src/outbound_mail.rs");
    let access = use_case.find("is_mailbox_accessible").unwrap_or(usize::MAX);
    let reserve = use_case.find("reserve_intent").unwrap_or(0);
    let claim = use_case.find("claim_dispatch").unwrap_or(0);
    let send = use_case.find("provider.send").unwrap_or(0);
    assert!(access < reserve);
    assert!(claim < send);
    has(use_case, "Err(_) => OutboundMailProviderOutcome::Ambiguous");
    has(use_case, "MAX_OUTBOUND_MAIL_DISPATCH_ATTEMPTS: u8 = 3");
    lacks(use_case, ".unwrap_or(receipt)");

    let sql = include_str!("d1_outbound_mail_intents/sql.rs");
    for needle in [
        "OUTBOUND_EVENT_PAYLOAD: &str = \"{}\"",
        "mail.outbound_intent_reserved.v1",
        "outbound_mail_dispatch_claims",
        "outbound_mail_dispatch_completions",
        "outbound_mail_ambiguity_marks",
    ] {
        has(sql, needle);
    }

    let migration = include_str!("../../../migrations/d1/0026_outbound_mail_intents.sql");
    for needle in [
        "'PENDING', 'DISPATCHING', 'RETRYABLE', 'SENT', 'AMBIGUOUS', 'REJECTED'",
        "outbound_mail_dispatch_claim_validate",
        "outbound_mail_dispatch_completion_validate",
        "outbound_mail_ambiguity_mark_validate",
        "association.client_id = client.client_id",
        "requester.status = 'ACTIVE'",
        "client_grants AS grant_row",
        "binding.status = 'ACTIVE'",
        "binding.execution_status = 'ACTIVE'",
        "state IN ('DISPATCHING', 'AMBIGUOUS')",
        "state = 'AMBIGUOUS'",
    ] {
        has(migration, needle);
    }
    for needle in [
        "body_text",
        "body_html",
        "subject TEXT",
        "recipient TEXT",
        "recipients TEXT",
        "to_address",
        "cc_address",
        "bcc_address",
    ] {
        lacks(migration, needle);
    }

    let eligibility = include_str!("d1_client_mail_eligibility.rs");
    has(eligibility, "impl ClientMailboxEligibilityPort");
    has(eligibility, "impl ClientMailboxAccessPort");
    assert_eq!(
        eligibility
            .matches("self.is_accessible(actor, client_id, binding_id)")
            .count(),
        2
    );
    for needle in [
        "association.client_id = client.client_id",
        "client.status = 'ACTIVE'",
        "binding.status = 'ACTIVE'",
        "binding.execution_status = 'ACTIVE'",
        "requester.status = 'ACTIVE'",
        "client_grants AS grant_row",
        "binding.provider IN ('GMAIL_API', 'IMAP')",
        "OR binding.provider = 'MICROSOFT_GRAPH'",
    ] {
        has(eligibility, needle);
    }
    lacks(eligibility, "BROWSER_FALLBACK");
}

#[test]
fn c5_gmail_send_boundaries_are_provider_local_and_retry_safe() {
    let c2 = include_str!("gmail_oauth_provisioning.rs");
    has(c2, "https://www.googleapis.com/auth/gmail.readonly");
    lacks(c2, "https://www.googleapis.com/auth/gmail.send");

    let application = include_str!("../../application-ports/src/outbound_mail.rs");
    for needle in [
        "gmail.googleapis.com",
        "users/me/messages/send",
        "threadId",
        "In-Reply-To",
        "References",
    ] {
        lacks(application, needle);
    }

    let consent = include_str!("gmail_send_capability.rs");
    has(consent, "https://www.googleapis.com/auth/gmail.send");
    has(consent, "oauthIncludeGrantedScopes");
    has(consent, "signed_resolver_request");
    has(consent, "gmail/send/oauth/start");
    has(consent, "gmail/send/oauth/complete");

    let resolver = include_str!(concat!("gmail_send_", "credential.rs"));
    has(resolver, "gmail/send/resolve");
    has(resolver, "capability");
    has(resolver, "signed_resolver_request");
    has(resolver, "\"SEND\"");

    let provider = include_str!("gmail_outbound_mail.rs");
    let send_impl = provider
        .split_once("impl OutboundMailProviderPort")
        .map(|(_, body)| body)
        .expect("missing provider implementation");
    let positions = [
        send_impl.find("find_binding"),
        send_impl.find("resolve_gmail_send_credential"),
        send_impl.find("resolve_message_context"),
        send_impl.find("render_mime"),
        send_impl.find("send_gmail_message"),
    ];
    assert!(positions.iter().all(Option::is_some));
    let positions = positions.map(Option::unwrap);
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    has(provider, "408 | 425 | 500..=599 => SendStatus::Ambiguous");
    has(provider, "429 => SendStatus::RetryableNotSent");
    has(provider, "400..=499 => SendStatus::Rejected");
    has(provider, "format!(\"gmail:{id}\")");

    let source = include_str!("gmail_outbound_mail/source.rs");
    for needle in [
        "GMAIL_PROFILE_ENDPOINT",
        "GMAIL_MESSAGES_ENDPOINT",
        "Message-ID",
        "References",
        "thread_id: Some(metadata.thread_id)",
    ] {
        has(source, needle);
    }
    lacks(source, "deny_unknown_fields");

    let mime = include_str!("gmail_outbound_mail/mime.rs");
    has(mime, "Content-Transfer-Encoding: base64");
    has(mime, "encode_base64url_unpadded");
    has(mime, "multipart/alternative");

    let c4 = include_str!("../../use-cases-mailboxes/src/outbound_mail.rs");
    let claim = c4.find("claim_dispatch").unwrap_or(usize::MAX);
    let send = c4.find("provider.send").unwrap_or(usize::MAX);
    assert!(claim < send);

    for text in [consent, resolver, provider, source] {
        for needle in ["println!", "console_log!", "console_error!", "log::"] {
            lacks(text, needle);
        }
    }
}
