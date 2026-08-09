use application_ports::mailbox_jobs::{MailboxJobPortError, MailboxJobPortErrorClass};
use application_ports::mailbox_scheduling::{MailboxDispatchPublisherPort, MailboxJobDispatch};
use profile_platform_primitives::{
    ActorId, AggregateVersion, MailboxBindingId, MailboxJobId, TenantId, UnixMillis,
};
use serde::{Deserialize, Serialize};
use worker::Queue;

const MAILBOX_JOB_QUEUE_ENVELOPE_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxJobQueueMessage {
    envelope_version: u16,
    tenant_id: String,
    actor_id: String,
    binding_id: String,
    job_id: String,
    expected_job_version: u64,
    due_at_ms: u64,
}

impl MailboxJobQueueMessage {
    #[must_use]
    pub fn from_dispatch(dispatch: &MailboxJobDispatch) -> Self {
        Self {
            envelope_version: MAILBOX_JOB_QUEUE_ENVELOPE_VERSION,
            tenant_id: dispatch.tenant_id().as_str().to_owned(),
            actor_id: dispatch.actor_id().as_str().to_owned(),
            binding_id: dispatch.binding_id().as_str().to_owned(),
            job_id: dispatch.job_id().as_str().to_owned(),
            expected_job_version: dispatch.expected_version().value(),
            due_at_ms: dispatch.due_at().value(),
        }
    }

    pub fn into_dispatch(self) -> Result<MailboxJobDispatch, MailboxJobPortError> {
        if self.envelope_version != MAILBOX_JOB_QUEUE_ENVELOPE_VERSION {
            return Err(integrity_failure());
        }
        Ok(MailboxJobDispatch::new(
            TenantId::parse(self.tenant_id).map_err(|_| integrity_failure())?,
            ActorId::parse(self.actor_id).map_err(|_| integrity_failure())?,
            MailboxBindingId::parse(self.binding_id).map_err(|_| integrity_failure())?,
            MailboxJobId::parse(self.job_id).map_err(|_| integrity_failure())?,
            AggregateVersion::new(self.expected_job_version).map_err(|_| integrity_failure())?,
            UnixMillis::new(self.due_at_ms),
        ))
    }
}

pub struct QueueMailboxJobPublisher {
    queue: Queue,
}

impl QueueMailboxJobPublisher {
    #[must_use]
    pub const fn new(queue: Queue) -> Self {
        Self { queue }
    }
}

impl MailboxDispatchPublisherPort for QueueMailboxJobPublisher {
    async fn publish(&self, dispatch: &MailboxJobDispatch) -> Result<(), MailboxJobPortError> {
        self.queue
            .send(MailboxJobQueueMessage::from_dispatch(dispatch))
            .await
            .map_err(|_| {
                MailboxJobPortError::new(MailboxJobPortErrorClass::DependencyUnavailable)
            })
    }
}

fn integrity_failure() -> MailboxJobPortError {
    MailboxJobPortError::new(MailboxJobPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::MailboxJobQueueMessage;
    use application_ports::mailbox_scheduling::MailboxJobDispatch;
    use profile_platform_primitives::{
        ActorId, AggregateVersion, MailboxBindingId, MailboxJobId, TenantId, UnixMillis,
    };

    #[test]
    fn mailbox_queue_envelope_round_trips_metadata_only()
    -> Result<(), Box<dyn std::error::Error>> {
        let dispatch = MailboxJobDispatch::new(
            TenantId::parse("tenant_01JMAILQUEUE")?,
            ActorId::parse("actor_01JMAILQUEUE")?,
            MailboxBindingId::parse("mailbox_01JMAILQUEUE")?,
            MailboxJobId::parse("mailjob_01JMAILQUEUE")?,
            AggregateVersion::new(4)?,
            UnixMillis::new(42),
        );
        let encoded = serde_json::to_value(MailboxJobQueueMessage::from_dispatch(&dispatch))?;
        for forbidden in [
            "secretHandle",
            "accessToken",
            "password",
            "subject",
            "sender",
            "recipient",
            "body",
        ] {
            assert!(encoded.get(forbidden).is_none(), "leaked {forbidden}");
        }
        let restored: MailboxJobQueueMessage = serde_json::from_value(encoded)?;
        assert_eq!(restored.into_dispatch()?, dispatch);
        Ok(())
    }
}
