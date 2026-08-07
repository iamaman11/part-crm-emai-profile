use crate::commands::CommandExecutionEvidence;
use core::fmt;
use identity_access_domain::MembershipRole;
pub use mailbox_domain::{MailboxBinding, MailboxBindingStatus, MailboxProvider};
use mailbox_domain::MailboxJob;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, MailboxBindingId, SecretHandle, TenantScope,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxObservation {
    binding_id: MailboxBindingId,
    provider_status: String,
    bounded_item_count: u32,
    next_cursor: Option<String>,
}

impl MailboxObservation {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        provider_status: impl Into<String>,
        bounded_item_count: u32,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            binding_id,
            provider_status: provider_status.into(),
            bounded_item_count,
            next_cursor,
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub fn provider_status(&self) -> &str {
        &self.provider_status
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

pub trait MailboxProviderPort {
    type Error;

    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Result<MailboxObservation, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxBindingCreateWrite {
    binding: MailboxBinding,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl MailboxBindingCreateWrite {
    #[must_use]
    pub fn new(
        binding: MailboxBinding,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            binding,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn binding(&self) -> &MailboxBinding {
        &self.binding
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
pub struct MailboxBindingRevokeWrite {
    binding_id: MailboxBindingId,
    expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl MailboxBindingRevokeWrite {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            binding_id,
            expected_version,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn expected_version(&self) -> AggregateVersion {
        self.expected_version
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
pub struct MailboxReplayReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl MailboxReplayReceipt {
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
pub enum MailboxReplayDecision {
    Miss,
    Replay(MailboxReplayReceipt),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxBindingPortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxBindingPortError {
    class: MailboxBindingPortErrorClass,
}

impl MailboxBindingPortError {
    #[must_use]
    pub const fn new(class: MailboxBindingPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> MailboxBindingPortErrorClass {
        self.class
    }
}

impl fmt::Display for MailboxBindingPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            MailboxBindingPortErrorClass::NotFound => "mailbox binding not found",
            MailboxBindingPortErrorClass::VersionConflict => "mailbox binding version conflict",
            MailboxBindingPortErrorClass::InvalidState => "mailbox binding invalid state",
            MailboxBindingPortErrorClass::Conflict => "mailbox binding conflict",
            MailboxBindingPortErrorClass::IntegrityFailure => "mailbox binding integrity failure",
            MailboxBindingPortErrorClass::InternalFailure => "mailbox binding internal failure",
            MailboxBindingPortErrorClass::DependencyUnavailable => {
                "mailbox binding dependency unavailable"
            }
        })
    }
}

impl std::error::Error for MailboxBindingPortError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxBindingReadModel {
    binding_id: MailboxBindingId,
    provider: MailboxProvider,
    status: MailboxBindingStatus,
    version: AggregateVersion,
}

impl MailboxBindingReadModel {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        provider: MailboxProvider,
        status: MailboxBindingStatus,
        version: AggregateVersion,
    ) -> Self {
        Self {
            binding_id,
            provider,
            status,
            version,
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn provider(&self) -> MailboxProvider {
        self.provider
    }

    #[must_use]
    pub const fn status(&self) -> MailboxBindingStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

#[allow(async_fn_in_trait)]
pub trait MailboxBindingApplicationPort {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxReplayDecision, MailboxBindingPortError>;

    async fn create_binding(
        &self,
        actor: &ActorContext,
        write: &MailboxBindingCreateWrite,
    ) -> Result<(), MailboxBindingPortError>;

    async fn revoke_binding(
        &self,
        actor: &ActorContext,
        write: &MailboxBindingRevokeWrite,
    ) -> Result<(), MailboxBindingPortError>;

    async fn find_binding(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<MailboxBindingReadModel>, MailboxBindingPortError>;
}

#[must_use]
pub fn binding_from_parts(
    tenant_id: profile_platform_primitives::TenantId,
    binding_id: MailboxBindingId,
    provider: MailboxProvider,
    secret_handle: SecretHandle,
) -> MailboxBinding {
    MailboxBinding::create(tenant_id, binding_id, provider, secret_handle)
}
