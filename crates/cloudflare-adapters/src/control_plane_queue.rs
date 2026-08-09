use crate::integration_event_queue::IntegrationEventQueueMessage;
use crate::mailbox_job_queue::MailboxJobQueueMessage;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ControlPlaneQueueMessage {
    IntegrationEvent(IntegrationEventQueueMessage),
    MailboxJob(MailboxJobQueueMessage),
}
