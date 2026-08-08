use application_ports::CommandExecutionEvidence;
use application_ports::identity_ceremonies::{
    BootstrapOwnerWrite, IdentityCeremonyApplicationPort, InvitationAcceptWrite,
    VerifiedIdentityCeremonyContext, VerifiedIdentitySnapshot,
};
use application_ports::identity_governance::{
    IdentityGovernancePortError, IdentityGovernancePortErrorClass, IdentityReplayDecision,
    IdentityReplayReceipt,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorId, AggregateVersion, CorrelationId, IdentityId, InvitationId, TenantScope,
};

use crate::identity_governance::IdentityGovernanceOperationError;

const OWNER_BOOTSTRAP_COMMAND: &str = "tenant.owner_bootstrap";
const INVITATION_ACCEPT_COMMAND: &str = "invitation.accept";
const EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteOwnerBootstrapCommand {
    actor_id: ActorId,
    identity_id: IdentityId,
    tenant_display_name: String,
    evidence: CommandExecutionEvidence,
}

impl ExecuteOwnerBootstrapCommand {
    #[must_use]
    pub fn new(
        actor_id: ActorId,
        identity_id: IdentityId,
        tenant_display_name: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            actor_id,
            identity_id,
            tenant_display_name: tenant_display_name.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteInvitationAcceptCommand {
    actor_id: ActorId,
    identity_id: IdentityId,
    invitation_id: InvitationId,
    evidence: CommandExecutionEvidence,
}

impl ExecuteInvitationAcceptCommand {
    #[must_use]
    pub fn new(
        actor_id: ActorId,
        identity_id: IdentityId,
        invitation_id: InvitationId,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            actor_id,
            identity_id,
            invitation_id,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityCeremonyOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl IdentityCeremonyOutcome {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

pub async fn execute_owner_bootstrap<P: IdentityCeremonyApplicationPort>(
    scope: TenantScope,
    correlation_id: CorrelationId,
    identity: VerifiedIdentitySnapshot,
    port: &P,
    command: ExecuteOwnerBootstrapCommand,
) -> Result<IdentityCeremonyOutcome, IdentityGovernanceOperationError> {
    if let Some(existing) = port
        .find_active_identity_binding(&scope, &identity, &correlation_id)
        .await
        .map_err(map_port_error)?
    {
        if existing.role() != MembershipRole::TenantOwner
            || existing.actor_id() != &command.actor_id
        {
            return Err(IdentityGovernanceOperationError::NotFound);
        }
        return existing_replay(
            &scope,
            &command.actor_id,
            port,
            OWNER_BOOTSTRAP_COMMAND,
            &command.evidence,
            scope.tenant_id().as_str(),
        )
        .await;
    }

    let boundary = port
        .tenant_identity_boundary(&scope)
        .await
        .map_err(map_port_error)?;
    if boundary.membership_count() != 0 || boundary.active_owner_count() != 0 {
        return Err(IdentityGovernanceOperationError::Conflict);
    }

    let context = VerifiedIdentityCeremonyContext::new(
        scope,
        command.actor_id,
        correlation_id,
        identity,
    );
    let resource_id = context.scope().tenant_id().as_str().to_owned();
    let write = BootstrapOwnerWrite::new(
        command.identity_id,
        command.tenant_display_name,
        command.evidence,
        EVENT_PAYLOAD,
    );
    match port.bootstrap_owner(&context, &write).await {
        Ok(()) => Ok(fresh_outcome("bootstrapped", resource_id)),
        Err(error) if error.class() == IdentityGovernancePortErrorClass::Conflict => {
            conflict_replay(
                context.scope(),
                context.actor_id(),
                port,
                OWNER_BOOTSTRAP_COMMAND,
                write.evidence(),
                &resource_id,
            )
            .await
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn execute_invitation_accept<P: IdentityCeremonyApplicationPort>(
    scope: TenantScope,
    correlation_id: CorrelationId,
    identity: VerifiedIdentitySnapshot,
    port: &P,
    command: ExecuteInvitationAcceptCommand,
) -> Result<IdentityCeremonyOutcome, IdentityGovernanceOperationError> {
    if let Some(existing) = port
        .find_active_identity_binding(&scope, &identity, &correlation_id)
        .await
        .map_err(map_port_error)?
    {
        if existing.actor_id() != &command.actor_id {
            return Err(IdentityGovernanceOperationError::NotFound);
        }
        return existing_replay(
            &scope,
            &command.actor_id,
            port,
            INVITATION_ACCEPT_COMMAND,
            &command.evidence,
            command.actor_id.as_str(),
        )
        .await;
    }

    let context = VerifiedIdentityCeremonyContext::new(
        scope,
        command.actor_id,
        correlation_id,
        identity,
    );
    let resource_id = context.actor_id().as_str().to_owned();
    let write = InvitationAcceptWrite::new(
        command.invitation_id,
        command.identity_id,
        command.evidence,
        EVENT_PAYLOAD,
    );
    match port.accept_invitation(&context, &write).await {
        Ok(()) => Ok(fresh_outcome("accepted", resource_id)),
        Err(error) if error.class() == IdentityGovernancePortErrorClass::Conflict => {
            conflict_replay(
                context.scope(),
                context.actor_id(),
                port,
                INVITATION_ACCEPT_COMMAND,
                write.evidence(),
                &resource_id,
            )
            .await
        }
        Err(error) => Err(map_port_error(error)),
    }
}

async fn existing_replay<P: IdentityCeremonyApplicationPort>(
    scope: &TenantScope,
    actor_id: &ActorId,
    port: &P,
    command_name: &str,
    evidence: &CommandExecutionEvidence,
    resource_id: &str,
) -> Result<IdentityCeremonyOutcome, IdentityGovernanceOperationError> {
    match port
        .decide_ceremony_replay(scope, actor_id, command_name, evidence)
        .await
        .map_err(map_port_error)?
    {
        IdentityReplayDecision::Replay(receipt) => Ok(replay_outcome(resource_id, &receipt)),
        IdentityReplayDecision::Miss | IdentityReplayDecision::Conflict => {
            Err(IdentityGovernanceOperationError::Conflict)
        }
    }
}

async fn conflict_replay<P: IdentityCeremonyApplicationPort>(
    scope: &TenantScope,
    actor_id: &ActorId,
    port: &P,
    command_name: &str,
    evidence: &CommandExecutionEvidence,
    resource_id: &str,
) -> Result<IdentityCeremonyOutcome, IdentityGovernanceOperationError> {
    existing_replay(scope, actor_id, port, command_name, evidence, resource_id).await
}

fn fresh_outcome(result_code: &str, resource_id: String) -> IdentityCeremonyOutcome {
    IdentityCeremonyOutcome {
        result_code: result_code.to_owned(),
        resource_id,
        aggregate_version: AggregateVersion::INITIAL,
        replayed: false,
    }
}

fn replay_outcome(
    resource_id: &str,
    receipt: &IdentityReplayReceipt,
) -> IdentityCeremonyOutcome {
    IdentityCeremonyOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt.result_reference().unwrap_or(resource_id).to_owned(),
        aggregate_version: AggregateVersion::INITIAL,
        replayed: true,
    }
}

fn map_port_error(error: IdentityGovernancePortError) -> IdentityGovernanceOperationError {
    match error.class() {
        IdentityGovernancePortErrorClass::NotFound => IdentityGovernanceOperationError::NotFound,
        IdentityGovernancePortErrorClass::VersionConflict => {
            IdentityGovernanceOperationError::VersionConflict
        }
        IdentityGovernancePortErrorClass::InvalidState => {
            IdentityGovernanceOperationError::InvalidState
        }
        IdentityGovernancePortErrorClass::Conflict => IdentityGovernanceOperationError::Conflict,
        IdentityGovernancePortErrorClass::IntegrityFailure => {
            IdentityGovernanceOperationError::IntegrityFailure
        }
        IdentityGovernancePortErrorClass::InternalFailure => {
            IdentityGovernanceOperationError::InternalFailure
        }
        IdentityGovernancePortErrorClass::DependencyUnavailable => {
            IdentityGovernanceOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
#[path = "identity_ceremonies_tests.rs"]
mod identity_ceremonies_tests;
