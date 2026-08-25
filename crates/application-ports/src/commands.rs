use profile_platform_primitives::{
    AuditEventId, IdempotencyKey, OutboxEventId, PayloadFingerprint, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandExecutionEvidence {
    idempotency_key: IdempotencyKey,
    payload_fingerprint: PayloadFingerprint,
    audit_event_id: AuditEventId,
    outbox_event_id: OutboxEventId,
    now: UnixMillis,
    idempotency_expires_at: UnixMillis,
}

impl CommandExecutionEvidence {
    #[must_use]
    pub const fn new(
        idempotency_key: IdempotencyKey,
        payload_fingerprint: PayloadFingerprint,
        audit_event_id: AuditEventId,
        outbox_event_id: OutboxEventId,
        now: UnixMillis,
        idempotency_expires_at: UnixMillis,
    ) -> Self {
        Self {
            idempotency_key,
            payload_fingerprint,
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
    pub const fn payload_fingerprint(&self) -> &PayloadFingerprint {
        &self.payload_fingerprint
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
