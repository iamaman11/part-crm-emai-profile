use crate::CommandExecutionEvidence;
use crate::clients::{ClientPortError, ClientReplayDecision};
use client_domain::{ClientMergePlan, ClientRecord};
use profile_platform_primitives::{ActorContext, ClientId, TenantScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientMergeWrite {
    plan: ClientMergePlan,
    reason: String,
    evidence: CommandExecutionEvidence,
    event_payload_json: &'static str,
}

impl ClientMergeWrite {
    #[must_use]
    pub fn new(
        plan: ClientMergePlan,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
        event_payload_json: &'static str,
    ) -> Self {
        Self {
            plan,
            reason: reason.into(),
            evidence,
            event_payload_json,
        }
    }

    #[must_use]
    pub const fn plan(&self) -> &ClientMergePlan {
        &self.plan
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub const fn event_payload_json(&self) -> &'static str {
        self.event_payload_json
    }
}

#[allow(async_fn_in_trait)]
pub trait ClientMergeApplicationPort {
    async fn load_client_for_merge(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, ClientPortError>;

    async fn source_has_active_assignment(
        &self,
        scope: &TenantScope,
        source_client_id: &ClientId,
    ) -> Result<bool, ClientPortError>;

    async fn decide_client_merge_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientPortError>;

    async fn persist_client_merge(
        &self,
        actor: &ActorContext,
        write: &ClientMergeWrite,
    ) -> Result<(), ClientPortError>;
}
