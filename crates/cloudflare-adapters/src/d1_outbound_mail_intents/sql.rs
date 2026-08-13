pub(super) const OUTBOUND_COMMAND: &str = "mail.outbound_send";
pub(super) const OUTBOUND_EVENT_PAYLOAD: &str = "{}";

pub(super) const LOAD_INTENT: &str = r#"
SELECT intent_id, state, attempt_count, provider_message_reference
FROM outbound_mail_intents
WHERE tenant_id = ?
  AND command_actor_id = ?
  AND idempotency_key = ?
  AND request_digest = ?
LIMIT 1
"#;

pub(super) const INTENT_CREATE: &str = r#"
INSERT INTO outbound_mail_intents (
    tenant_id, intent_id, command_actor_id, idempotency_key, request_digest,
    client_id, binding_id, operation, state, attempt_count,
    provider_message_reference, created_at_ms, updated_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'PENDING', 0, NULL, ?, ?)
"#;

pub(super) const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, request_digest,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, ?, ?, 'reserved', ?, ?, ?)
"#;

pub(super) const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, 'mail.outbound_send', 'outbound_mail_intent', ?, 'accepted', ?)
"#;

pub(super) const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, 'outbound_mail_intent', ?, 1, 'mail.outbound_intent_reserved.v1', ?, ?)
"#;

pub(super) const CLAIM_DISPATCH: &str = r#"
INSERT INTO outbound_mail_dispatch_claims (
    tenant_id, intent_id, attempt, claimed_at_ms
) VALUES (?, ?, ?, ?)
"#;

pub(super) const REJECT_EXHAUSTED: &str = r#"
UPDATE outbound_mail_intents
SET state = 'REJECTED', updated_at_ms = ?
WHERE tenant_id = ?
  AND intent_id = ?
  AND state IN ('PENDING', 'RETRYABLE')
"#;

pub(super) const COMPLETE_DISPATCH: &str = r#"
INSERT INTO outbound_mail_dispatch_completions (
    tenant_id, intent_id, attempt, outcome, provider_message_reference, completed_at_ms
) VALUES (?, ?, ?, ?, ?, ?)
"#;

pub(super) const MARK_AMBIGUOUS: &str = r#"
INSERT INTO outbound_mail_ambiguity_marks (
    tenant_id, intent_id, attempt, marked_at_ms
) VALUES (?, ?, ?, ?)
"#;
