#![forbid(unsafe_code)]

pub mod audit;
pub mod browser_mail_execution;
pub mod client_contact_lookup;
pub mod client_merge;
pub mod client_registry;
pub mod clients;
pub mod clock;
pub mod commands;
pub mod coordinator_ingress;
pub mod device_generation_commit;
pub mod device_jobs;
pub mod generation_objects;
pub mod generations;
pub mod identity;
pub mod identity_ceremonies;
pub mod identity_governance;
pub mod integration_events;
pub mod mailbox_jobs;
pub mod mailbox_scheduling;
pub mod mailboxes;
pub mod notifications;
pub mod profile_assignment_context;
pub mod profiles;
pub mod query;
pub mod query_clients;
pub mod query_global;
pub mod query_mail;
pub mod query_mail_provider;
pub mod query_mailboxes;
pub mod query_members;
pub mod query_profiles;
pub mod sessions;

pub use audit::{AuditPort, AuditRecord, AuditResult};
pub use clients::ClientRepository;
pub use clock::ClockPort;
pub use commands::CommandExecutionEvidence;
pub use device_generation_commit::{
    CoordinatorGenerationCommitWitness, DeviceGenerationCommitError,
    DeviceGenerationCommitErrorClass, DeviceGenerationCommitOutcome, DeviceGenerationCommitPort,
    DeviceGenerationCommitRequest,
};
pub use device_jobs::{
    AuthenticatedDevicePort, DeviceExecutionBlocker, DeviceExecutionPreconditionPort,
    DeviceExecutionReadiness, DeviceJobAuthorizationPort, DeviceJobCapability,
    DeviceJobInsertOutcome, DeviceJobPortError, DeviceJobPortErrorClass, DeviceJobQueryPort,
    DeviceJobRepositoryPort, DeviceJobWriteOutcome,
};
pub use generation_objects::{
    GenerationObjectDescriptor, GenerationObjectDescriptorVerifyPort,
    GenerationObjectExactVerifyPort, GenerationObjectUploadOutcome, GenerationObjectUploadPort,
    ImmutableGenerationObject,
};
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
pub use query::{
    QueryAuthorizationPort, QueryCapability, QueryCursor, QueryInputError, QueryPage,
    QueryPageRequest, QueryPageSize, QueryPortError, QueryPortErrorClass,
};
pub use sessions::ProfileCoordinatorPort;

impl core::fmt::Display for mailboxes::MailboxProviderPortError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Failure(_) => "mailbox provider failure",
            Self::IntegrityFailure => "mailbox provider integrity failure",
        })
    }
}

impl std::error::Error for mailboxes::MailboxProviderPortError {}
