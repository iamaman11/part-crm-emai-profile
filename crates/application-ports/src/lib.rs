#![forbid(unsafe_code)]

pub mod audit;
pub mod clients;
pub mod clock;
pub mod commands;
pub mod coordinator_ingress;
pub mod generations;
pub mod identity;
pub mod identity_ceremonies;
pub mod identity_governance;
pub mod integration_events;
pub mod mailbox_jobs;
pub mod mailboxes;
pub mod profiles;
pub mod sessions;

pub use audit::{AuditPort, AuditRecord, AuditResult};
pub use clients::ClientRepository;
pub use clock::ClockPort;
pub use commands::CommandExecutionEvidence;
pub use generations::{GenerationObjectReference, GenerationObjectStorePort};
pub use identity::MembershipRepository;
pub use integration_events::{
    ConsumerClaim, ConsumerIdempotencyPort, IntegrationEventOutboxPort, IntegrationEventPortError,
    IntegrationEventPortErrorClass, IntegrationEventPublisherPort, NotificationEventPort,
};
pub use mailboxes::{MailboxObservation, MailboxProviderPort};
pub use profiles::ProfileRepository;
pub use sessions::ProfileCoordinatorPort;
