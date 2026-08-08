use application_ports::{
    ConsumerClaim, ConsumerIdempotencyPort, IntegrationEventOutboxPort, IntegrationEventPortError,
    IntegrationEventPortErrorClass, IntegrationEventSourcePort, NotificationEventPort,
};
use contracts::{
    INTEGRATION_EVENT_ENVELOPE_VERSION, IntegrationEventEnvelope, IntegrationEventPayload,
    is_foundation_event_type,
};
use profile_platform_primitives::{
    AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_PENDING: &str = r#"
SELECT
    tenant_id,
    outbox_event_id,
    aggregate_type,
    aggregate_id,
    aggregate_version,
    event_type,
    event_version,
    envelope_version,
    payload_json,
    created_at_ms
FROM outbox_events
WHERE published_at_ms IS NULL
ORDER BY created_at_ms ASC, outbox_event_id ASC
LIMIT ?
"#;

const LOAD_EVENT: &str = r#"
SELECT
    tenant_id,
    outbox_event_id,
    aggregate_type,
    aggregate_id,
    aggregate_version,
    event_type,
    event_version,
    envelope_version,
    payload_json,
    created_at_ms
FROM outbox_events
WHERE tenant_id = ?
  AND outbox_event_id = ?
"#;

const MARK_PUBLISHED: &str = r#"
UPDATE outbox_events
SET published_at_ms = ?
WHERE tenant_id = ?
  AND outbox_event_id = ?
  AND published_at_ms IS NULL
"#;

const PERSIST_NOTIFICATION: &str = r#"
INSERT INTO notification_events (
    tenant_id,
    outbox_event_id,
    envelope_version,
    aggregate_type,
    aggregate_id,
    aggregate_version,
    event_type,
    event_version,
    payload_json,
    occurred_at_ms,
    persisted_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (tenant_id, outbox_event_id) DO NOTHING
"#;

const CLAIM_CONSUMER: &str = r#"
INSERT INTO consumer_idempotency (
    tenant_id,
    consumer_id,
    outbox_event_id,
    event_type,
    event_version,
    consumed_at_ms
) VALUES (?, ?, ?, ?, ?, ?)
ON CONFLICT (tenant_id, consumer_id, outbox_event_id) DO NOTHING
RETURNING outbox_event_id
"#;

#[derive(Debug, Deserialize)]
struct PendingOutboxRow {
    tenant_id: String,
    outbox_event_id: String,
    aggregate_type: String,
    aggregate_id: String,
    aggregate_version: i64,
    event_type: String,
    event_version: i64,
    envelope_version: i64,
    payload_json: String,
    created_at_ms: i64,
}

impl PendingOutboxRow {
    fn into_event(self) -> Result<IntegrationEventEnvelope, IntegrationEventPortError> {
        if self.envelope_version != i64::from(INTEGRATION_EVENT_ENVELOPE_VERSION)
            || !is_foundation_event_type(&self.event_type)
        {
            return Err(integrity_failure());
        }
        let aggregate_version = u64::try_from(self.aggregate_version)
            .ok()
            .and_then(|value| AggregateVersion::new(value).ok())
            .ok_or_else(integrity_failure)?;
        let event_version = u16::try_from(self.event_version).map_err(|_| integrity_failure())?;
        let occurred_at = u64::try_from(self.created_at_ms).map_err(|_| integrity_failure())?;
        let tenant_id = TenantId::parse(self.tenant_id).map_err(|_| integrity_failure())?;
        let event_id =
            OutboxEventId::parse(self.outbox_event_id).map_err(|_| integrity_failure())?;
        let aggregate_id = OpaqueId::parse(self.aggregate_id).map_err(|_| integrity_failure())?;
        let payload = IntegrationEventPayload::metadata_json(self.payload_json)
            .map_err(|_| integrity_failure())?;

        IntegrationEventEnvelope::new(
            event_id,
            tenant_id,
            self.aggregate_type,
            aggregate_id,
            aggregate_version,
            self.event_type,
            event_version,
            payload,
            UnixMillis::new(occurred_at),
        )
        .map_err(|_| integrity_failure())
    }
}

pub struct D1IntegrationEventRepository {
    database: D1Database,
}

impl D1IntegrationEventRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl IntegrationEventOutboxPort for D1IntegrationEventRepository {
    async fn load_pending(
        &self,
        limit: u32,
    ) -> Result<Vec<IntegrationEventEnvelope>, IntegrationEventPortError> {
        let limit = i64::from(limit);
        let statement = query!(&self.database, LOAD_PENDING, limit).map_err(map_worker_error)?;
        let rows = statement
            .all()
            .await
            .map_err(map_worker_error)?
            .results::<PendingOutboxRow>()
            .map_err(map_worker_error)?;
        rows.into_iter().map(PendingOutboxRow::into_event).collect()
    }

    async fn mark_published(
        &self,
        tenant_id: &TenantId,
        event_id: &OutboxEventId,
        published_at: UnixMillis,
    ) -> Result<(), IntegrationEventPortError> {
        let published_at = sqlite_integer(published_at)?;
        query!(
            &self.database,
            MARK_PUBLISHED,
            published_at,
            tenant_id.as_str(),
            event_id.as_str()
        )
        .map_err(map_worker_error)?
        .run()
        .await
        .map_err(map_worker_error)?;
        Ok(())
    }
}

impl IntegrationEventSourcePort for D1IntegrationEventRepository {
    async fn load_event(
        &self,
        tenant_id: &TenantId,
        event_id: &OutboxEventId,
    ) -> Result<Option<IntegrationEventEnvelope>, IntegrationEventPortError> {
        query!(
            &self.database,
            LOAD_EVENT,
            tenant_id.as_str(),
            event_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<PendingOutboxRow>(None)
        .await
        .map_err(map_worker_error)?
        .map(PendingOutboxRow::into_event)
        .transpose()
    }
}

impl NotificationEventPort for D1IntegrationEventRepository {
    async fn persist_notification_event(
        &self,
        event: &IntegrationEventEnvelope,
        persisted_at: UnixMillis,
    ) -> Result<(), IntegrationEventPortError> {
        let aggregate_version =
            i64::try_from(event.aggregate_version().value()).map_err(|_| integrity_failure())?;
        let occurred_at = sqlite_integer(event.occurred_at())?;
        let persisted_at = sqlite_integer(persisted_at)?;
        query!(
            &self.database,
            PERSIST_NOTIFICATION,
            event.tenant_id().as_str(),
            event.event_id().as_str(),
            i64::from(event.envelope_version()),
            event.aggregate_type(),
            event.aggregate_id().as_str(),
            aggregate_version,
            event.event_type(),
            i64::from(event.event_version()),
            event.payload().as_str(),
            occurred_at,
            persisted_at
        )
        .map_err(map_worker_error)?
        .run()
        .await
        .map_err(map_worker_error)?;
        Ok(())
    }
}

impl ConsumerIdempotencyPort for D1IntegrationEventRepository {
    async fn claim(
        &self,
        consumer_id: &OpaqueId,
        event: &IntegrationEventEnvelope,
        consumed_at: UnixMillis,
    ) -> Result<ConsumerClaim, IntegrationEventPortError> {
        let consumed_at = sqlite_integer(consumed_at)?;
        let row = query!(
            &self.database,
            CLAIM_CONSUMER,
            event.tenant_id().as_str(),
            consumer_id.as_str(),
            event.event_id().as_str(),
            event.event_type(),
            i64::from(event.event_version()),
            consumed_at
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("outbox_event_id"))
        .await
        .map_err(map_worker_error)?;
        Ok(if row.is_some() {
            ConsumerClaim::Claimed
        } else {
            ConsumerClaim::Duplicate
        })
    }
}

fn sqlite_integer(value: UnixMillis) -> Result<i64, IntegrationEventPortError> {
    i64::try_from(value.value()).map_err(|_| integrity_failure())
}

fn integrity_failure() -> IntegrationEventPortError {
    IntegrationEventPortError::new(IntegrationEventPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> IntegrationEventPortError {
    IntegrationEventPortError::new(IntegrationEventPortErrorClass::InternalFailure)
}

#[cfg(test)]
mod tests {
    use super::PendingOutboxRow;

    fn row(event_type: &str, payload_json: &str) -> PendingOutboxRow {
        PendingOutboxRow {
            tenant_id: "tenant_01JEVENT".to_owned(),
            outbox_event_id: "outbox_01JEVENT".to_owned(),
            aggregate_type: "client".to_owned(),
            aggregate_id: "client_01JEVENT".to_owned(),
            aggregate_version: 1,
            event_type: event_type.to_owned(),
            event_version: 1,
            envelope_version: 1,
            payload_json: payload_json.to_owned(),
            created_at_ms: 42,
        }
    }

    #[test]
    fn pending_row_reconstructs_typed_sanitized_envelope() -> Result<(), Box<dyn std::error::Error>>
    {
        let event = row("client.created.v1", "{}").into_event()?;
        assert_eq!(event.event_type(), "client.created.v1");
        assert_eq!(event.payload().as_str(), "{}");
        Ok(())
    }

    #[test]
    fn pending_row_rejects_prohibited_payload() {
        assert!(
            row("client.created.v1", r#"{"email":"private"}"#)
                .into_event()
                .is_err()
        );
    }

    #[test]
    fn pending_row_rejects_unknown_event_before_publish() {
        assert!(row("unknown.event.v1", "{}").into_event().is_err());
    }
}
