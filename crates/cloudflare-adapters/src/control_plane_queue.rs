use crate::integration_event_queue::IntegrationEventQueueMessage;
use crate::mailbox_job_queue::MailboxJobQueueMessage;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ControlPlaneQueueMessage {
    IntegrationEvent(IntegrationEventQueueMessage),
    MailboxJob(MailboxJobQueueMessage),
}

#[cfg(test)]
mod tests {
    use super::ControlPlaneQueueMessage;
    use serde_json::json;

    #[test]
    fn integration_and_mailbox_envelopes_are_structurally_disjoint()
    -> Result<(), Box<dyn std::error::Error>> {
        let event = serde_json::from_value::<ControlPlaneQueueMessage>(json!({
            "envelope_version": 1,
            "event_id": "outbox_01JQUEUE",
            "tenant_id": "tenant_01JQUEUE",
            "aggregate_type": "client",
            "aggregate_id": "client_01JQUEUE",
            "aggregate_version": 1,
            "event_type": "client.created.v1",
            "event_version": 1,
            "payload_json": "{}",
            "occurred_at_ms": 42
        }))?;
        assert!(matches!(
            event,
            ControlPlaneQueueMessage::IntegrationEvent(_)
        ));

        let mailbox = serde_json::from_value::<ControlPlaneQueueMessage>(json!({
            "envelope_version": 1,
            "tenant_id": "tenant_01JQUEUE",
            "actor_id": "actor_01JQUEUE",
            "binding_id": "mailbox_01JQUEUE",
            "job_id": "mailjob_01JQUEUE",
            "expected_job_version": 1,
            "due_at_ms": 42
        }))?;
        assert!(matches!(mailbox, ControlPlaneQueueMessage::MailboxJob(_)));
        Ok(())
    }
}
