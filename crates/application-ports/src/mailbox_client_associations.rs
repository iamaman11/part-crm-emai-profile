use crate::commands::CommandExecutionEvidence;
use core::fmt;
pub use mailbox_domain::{
    MailboxClientAssociation, MailboxClientAssociationAction, MailboxClientAssociationVersion,
};
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId, TenantScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxClientAssociationContext {
    association: MailboxClientAssociation,
    mailbox_executable: bool,
    target_client_active: bool,
}

impl MailboxClientAssociationContext {
    #[must_use]
    pub const fn new(
        association: MailboxClientAssociation,
        mailbox_executable: bool,
        target_client_active: bool,
    ) -> Self {
        Self {
            association,
            mailbox_executable,
            target_client_active,
        }
    }

    #[must_use]
    pub const fn association(&self) -> &MailboxClientAssociation {
        &self.association
    }

    #[must_use]
    pub const fn mailbox_executable(&self) -> bool {
        self.mailbox_executable
    }

    #[must_use]
    pub const fn target_client_active(&self) -> bool {
        self.target_client_active
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxClientAssociationWrite {
    binding_id: MailboxBindingId,
    previous_client_id: Option<ClientId>,
    next_client_id: Option<ClientId>,
    expected_version: MailboxClientAssociationVersion,
    next_version: MailboxClientAssociationVersion,
    action: MailboxClientAssociationAction,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl MailboxClientAssociationWrite {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        previous_client_id: Option<ClientId>,
        next_client_id: Option<ClientId>,
        expected_version: MailboxClientAssociationVersion,
        next_version: MailboxClientAssociationVersion,
        action: MailboxClientAssociationAction,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            binding_id,
            previous_client_id,
            next_client_id,
            expected_version,
            next_version,
            action,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn previous_client_id(&self) -> Option<&ClientId> {
        self.previous_client_id.as_ref()
    }

    #[must_use]
    pub const fn next_client_id(&self) -> Option<&ClientId> {
        self.next_client_id.as_ref()
    }

    #[must_use]
    pub const fn expected_version(&self) -> MailboxClientAssociationVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn next_version(&self) -> MailboxClientAssociationVersion {
        self.next_version
    }

    #[must_use]
    pub const fn action(&self) -> MailboxClientAssociationAction {
        self.action
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxClientAssociationReplayReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl MailboxClientAssociationReplayReceipt {
    #[must_use]
    pub fn new(result_code: impl Into<String>, result_reference: Option<String>) -> Self {
        Self {
            result_code: result_code.into(),
            result_reference,
        }
    }

    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn result_reference(&self) -> Option<&str> {
        self.result_reference.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MailboxClientAssociationReplayDecision {
    Miss,
    Replay(MailboxClientAssociationReplayReceipt),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxClientAssociationPortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxClientAssociationPortError {
    class: MailboxClientAssociationPortErrorClass,
}

impl MailboxClientAssociationPortError {
    #[must_use]
    pub const fn new(class: MailboxClientAssociationPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> MailboxClientAssociationPortErrorClass {
        self.class
    }
}

impl fmt::Display for MailboxClientAssociationPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            MailboxClientAssociationPortErrorClass::NotFound => "mailbox/client relationship target not found",
            MailboxClientAssociationPortErrorClass::VersionConflict => "mailbox/client relationship version conflict",
            MailboxClientAssociationPortErrorClass::InvalidState => "mailbox/client relationship state is invalid",
            MailboxClientAssociationPortErrorClass::Conflict => "mailbox/client relationship command conflict",
            MailboxClientAssociationPortErrorClass::IntegrityFailure => "mailbox/client relationship integrity failure",
            MailboxClientAssociationPortErrorClass::InternalFailure => "mailbox/client relationship internal failure",
            MailboxClientAssociationPortErrorClass::DependencyUnavailable => "mailbox/client relationship dependency unavailable",
        })
    }
}

impl std::error::Error for MailboxClientAssociationPortError {}

#[allow(async_fn_in_trait)]
pub trait MailboxClientAssociationApplicationPort {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxClientAssociationReplayDecision, MailboxClientAssociationPortError>;

    async fn load_context(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        target_client_id: Option<&ClientId>,
    ) -> Result<Option<MailboxClientAssociationContext>, MailboxClientAssociationPortError>;

    async fn change_association(
        &self,
        actor: &ActorContext,
        write: &MailboxClientAssociationWrite,
    ) -> Result<(), MailboxClientAssociationPortError>;
}
