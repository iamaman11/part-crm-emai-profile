use application_ports::NotificationEventRecord;
use profile_platform_primitives::{OpaqueId, OutboxEventId, UnixMillis};
use serde::{Deserialize, Serialize};

pub const INTERNAL_TENANT_HEADER: &str = "X-Internal-Realtime-Tenant-Id";
pub const INTERNAL_ACTOR_HEADER: &str = "X-Internal-Realtime-Actor-Id";
pub const INTERNAL_CORRELATION_HEADER: &str = "X-Internal-Realtime-Correlation-Id";
pub const INTERNAL_PUBLISH_PATH: &str = "/internal/realtime/publish";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeInternalEvent {
    pub event_id: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub occurred_at_ms: u64,
}

impl RealtimeInternalEvent {
    #[must_use]
    pub fn from_record(event: &NotificationEventRecord) -> Self {
        Self {
            event_id: event.event_id().as_str().to_owned(),
            aggregate_type: event.aggregate_type().to_owned(),
            aggregate_id: event.aggregate_id().as_str().to_owned(),
            event_type: event.event_type().to_owned(),
            occurred_at_ms: event.occurred_at().value(),
        }
    }

    pub fn into_record(self) -> Result<NotificationEventRecord, RealtimeInternalContractError> {
        if !valid_symbol(&self.aggregate_type, 80) || !valid_symbol(&self.event_type, 160) {
            return Err(RealtimeInternalContractError);
        }
        Ok(NotificationEventRecord::new(
            OutboxEventId::parse(self.event_id).map_err(|_| RealtimeInternalContractError)?,
            self.aggregate_type,
            OpaqueId::parse(self.aggregate_id).map_err(|_| RealtimeInternalContractError)?,
            self.event_type,
            UnixMillis::new(self.occurred_at_ms),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeInternalContractError;

impl core::fmt::Display for RealtimeInternalContractError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("invalid internal realtime event contract")
    }
}

impl std::error::Error for RealtimeInternalContractError {}

fn valid_symbol(value: &str, maximum: usize) -> bool {
    let length = value.len();
    (1..=maximum).contains(&length)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::RealtimeInternalEvent;
    use application_ports::NotificationEventRecord;
    use profile_platform_primitives::{OpaqueId, OutboxEventId, UnixMillis};

    #[test]
    fn internal_event_round_trip_contains_no_integration_payload()
    -> Result<(), Box<dyn std::error::Error>> {
        let record = NotificationEventRecord::new(
            OutboxEventId::parse("outbox_01JREALTIME")?,
            "client",
            OpaqueId::parse("client_01JREALTIME")?,
            "client.changed.v1",
            UnixMillis::new(42),
        );
        let internal = RealtimeInternalEvent::from_record(&record);
        let encoded = serde_json::to_string(&internal)?;
        assert!(!encoded.contains("payload"));
        assert!(!encoded.contains("subject"));
        assert!(!encoded.contains("body"));
        let restored = serde_json::from_str::<RealtimeInternalEvent>(&encoded)?.into_record()?;
        assert_eq!(restored, record);
        Ok(())
    }
}
