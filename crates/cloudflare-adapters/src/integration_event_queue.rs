use application_ports::{
    IntegrationEventPortError, IntegrationEventPortErrorClass, IntegrationEventPublisherPort,
};
use contracts::{
    INTEGRATION_EVENT_ENVELOPE_VERSION, IntegrationEventEnvelope, IntegrationEventPayload,
};
use profile_platform_primitives::{
    AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
};
use serde::{Deserialize, Serialize};
use worker::Queue;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IntegrationEventQueueMessage {
    envelope_version: u16,
    event_id: String,
    tenant_id: String,
    aggregate_type: String,
    aggregate_id: String,
    aggregate_version: u64,
    event_type: String,
    event_version: u16,
    payload_json: String,
    occurred_at_ms: u64,
}

impl IntegrationEventQueueMessage {
    #[must_use]
    pub fn from_event(event: &IntegrationEventEnvelope) -> Self {
        Self {
            envelope_version: event.envelope_version(),
            event_id: event.event_id().as_str().to_owned(),
            tenant_id: event.tenant_id().as_str().to_owned(),
            aggregate_type: event.aggregate_type().to_owned(),
            aggregate_id: event.aggregate_id().as_str().to_owned(),
            aggregate_version: event.aggregate_version().value(),
            event_type: event.event_type().to_owned(),
            event_version: event.event_version(),
            payload_json: event.payload().as_str().to_owned(),
            occurred_at_ms: event.occurred_at().value(),
        }
    }

    pub fn into_event(self) -> Result<IntegrationEventEnvelope, IntegrationEventPortError> {
        if self.envelope_version != INTEGRATION_EVENT_ENVELOPE_VERSION {
            return Err(integrity_failure());
        }
        let aggregate_version =
            AggregateVersion::new(self.aggregate_version).map_err(|_| integrity_failure())?;
        let event_id = OutboxEventId::parse(self.event_id).map_err(|_| integrity_failure())?;
        let tenant_id = TenantId::parse(self.tenant_id).map_err(|_| integrity_failure())?;
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
            self.event_version,
            payload,
            UnixMillis::new(self.occurred_at_ms),
        )
        .map_err(|_| integrity_failure())
    }
}

pub struct QueueIntegrationEventPublisher {
    queue: Queue,
}

impl QueueIntegrationEventPublisher {
    #[must_use]
    pub const fn new(queue: Queue) -> Self {
        Self { queue }
    }
}

impl IntegrationEventPublisherPort for QueueIntegrationEventPublisher {
    async fn publish(
        &self,
        event: &IntegrationEventEnvelope,
    ) -> Result<(), IntegrationEventPortError> {
        self.queue
            .send(IntegrationEventQueueMessage::from_event(event))
            .await
            .map_err(|_| {
                IntegrationEventPortError::new(
                    IntegrationEventPortErrorClass::DependencyUnavailable,
                )
            })
    }
}

fn integrity_failure() -> IntegrationEventPortError {
    IntegrationEventPortError::new(IntegrationEventPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::IntegrationEventQueueMessage;
    use contracts::{IntegrationEventEnvelope, IntegrationEventPayload};
    use profile_platform_primitives::{
        AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
    };

    #[test]
    fn queue_dto_round_trips_through_typed_contract() -> Result<(), Box<dyn std::error::Error>> {
        let event = IntegrationEventEnvelope::new(
            OutboxEventId::parse("outbox_01JEVENT")?,
            TenantId::parse("tenant_01JEVENT")?,
            "client",
            OpaqueId::parse("client_01JEVENT")?,
            AggregateVersion::INITIAL,
            "client.created.v1",
            1,
            IntegrationEventPayload::empty(),
            UnixMillis::new(42),
        )?;
        let restored = IntegrationEventQueueMessage::from_event(&event).into_event()?;
        assert_eq!(restored, event);
        Ok(())
    }

    #[test]
    fn queue_dto_rejects_payload_that_fails_contract_sanitizer() {
        let message = IntegrationEventQueueMessage {
            envelope_version: 1,
            event_id: "outbox_01JEVENT".to_owned(),
            tenant_id: "tenant_01JEVENT".to_owned(),
            aggregate_type: "client".to_owned(),
            aggregate_id: "client_01JEVENT".to_owned(),
            aggregate_version: 1,
            event_type: "client.created.v1".to_owned(),
            event_version: 1,
            payload_json: r#"{"message_body":"private"}"#.to_owned(),
            occurred_at_ms: 42,
        };
        assert!(message.into_event().is_err());
    }
}
