#![forbid(unsafe_code)]

pub mod audit;
pub mod client_contact_lookup;
pub mod client_merge;
pub mod client_registry;
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
pub mod notifications;
pub mod profile_assignment_context;
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
    IntegrationEventPortErrorClass, IntegrationEventPublisherPort, IntegrationEventSourcePort,
    NotificationEventPort,
};
pub use mailboxes::{MailboxObservation, MailboxProviderPort};
pub use notifications::{
    CursorAdvanceWriteOutcome, DeliveryTransitionWriteOutcome, NotificationAuthorizationPort,
    NotificationCapability, NotificationCatchUpRepositoryPort, NotificationCursorRepositoryPort,
    NotificationDeliveryRepositoryPort, NotificationEventPage, NotificationEventRecord,
    NotificationOperationsRepositoryPort, NotificationOperationsSnapshot, NotificationPortError,
    NotificationPortErrorClass, NotificationReplayIntent, NotificationReplayRepositoryPort,
    NotificationRetentionOutcome, NotificationRetentionRepositoryPort, PendingNotificationReplay,
    ReplayPreparationOutcome, ReplayReasonClass,
};
pub use profiles::ProfileRepository;
pub use sessions::ProfileCoordinatorPort;
