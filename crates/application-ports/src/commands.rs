use profile_platform_primitives::{AuditEventId, IdempotencyKey, OutboxEventId, UnixMillis};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandExecutionEvidence {
    idempotency_key: IdempotencyKey,
    request_digest: String,
    audit_event_id: AuditEventId,
    outbox_event_id: OutboxEventId,
    now: UnixMillis,
    idempotency_expires_at: UnixMillis,
}

impl CommandExecutionEvidence {
    #[must_use]
    pub fn new(
        idempotency_key: IdempotencyKey,
        request_digest: impl Into<String>,
        audit_event_id: AuditEventId,
        outbox_event_id: OutboxEventId,
        now: UnixMillis,
        idempotency_expires_at: UnixMillis,
    ) -> Self {
        Self {
            idempotency_key,
            request_digest: request_digest.into(),
            audit_event_id,
            outbox_event_id,
            now,
            idempotency_expires_at,
        }
    }

    #[must_use]
    pub const fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    #[must_use]
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }

    #[must_use]
    pub const fn audit_event_id(&self) -> &AuditEventId {
        &self.audit_event_id
    }

    #[must_use]
    pub const fn outbox_event_id(&self) -> &OutboxEventId {
        &self.outbox_event_id
    }

    #[must_use]
    pub const fn now(&self) -> UnixMillis {
        self.now
    }

    #[must_use]
    pub const fn idempotency_expires_at(&self) -> UnixMillis {
        self.idempotency_expires_at
    }
}
