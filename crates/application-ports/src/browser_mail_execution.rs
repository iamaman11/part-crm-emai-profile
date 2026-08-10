use crate::commands::CommandExecutionEvidence;
use crate::mailboxes::{MailboxBindingPortError, MailboxReplayDecision};
use crate::query::QueryPortError;
use core::future::Future;
use profile_platform_primitives::{ActorContext, MailboxBindingId, ProfileId, TenantScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMailboxExecutionBinding {
    binding_id: MailboxBindingId,
    profile_id: ProfileId,
}

impl BrowserMailboxExecutionBinding {
    #[must_use]
    pub const fn new(binding_id: MailboxBindingId, profile_id: ProfileId) -> Self {
        Self {
            binding_id,
            profile_id,
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMailboxExecutionBindWrite {
    binding_id: MailboxBindingId,
    profile_id: ProfileId,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl BrowserMailboxExecutionBindWrite {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        profile_id: ProfileId,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            binding_id,
            profile_id,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
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

#[allow(async_fn_in_trait)]
pub trait BrowserMailboxExecutionBindingApplicationPort {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxReplayDecision, MailboxBindingPortError>;

    async fn bind_browser_mailbox_execution(
        &self,
        actor: &ActorContext,
        write: &BrowserMailboxExecutionBindWrite,
    ) -> Result<(), MailboxBindingPortError>;
}

pub trait BrowserMailboxExecutionBindingPort {
    fn resolve_browser_mailbox_execution_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> impl Future<Output = Result<Option<BrowserMailboxExecutionBinding>, QueryPortError>>;
}
