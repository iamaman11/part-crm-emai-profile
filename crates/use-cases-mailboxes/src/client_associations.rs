use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_client_associations::{
    MailboxClientAssociationApplicationPort, MailboxClientAssociationPortError,
    MailboxClientAssociationPortErrorClass, MailboxClientAssociationReplayDecision,
    MailboxClientAssociationReplayReceipt, MailboxClientAssociationVersion,
    MailboxClientAssociationWrite,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use mailbox_domain::{MailboxClientAssociationAction, MailboxClientAssociationError};
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};

const ASSOCIATION_CHANGE_COMMAND: &str = "mailbox.client_association_change";
const ASSOCIATION_EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteMailboxClientAssociationCommand {
    binding_id: MailboxBindingId,
    next_client_id: Option<ClientId>,
    expected_version: MailboxClientAssociationVersion,
    evidence: CommandExecutionEvidence,
}

impl ExecuteMailboxClientAssociationCommand {
    #[must_use]
    pub const fn associate(
        binding_id: MailboxBindingId,
        client_id: ClientId,
        expected_version: MailboxClientAssociationVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            next_client_id: Some(client_id),
            expected_version,
            evidence,
        }
    }

    #[must_use]
    pub const fn unbind(
        binding_id: MailboxBindingId,
        expected_version: MailboxClientAssociationVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            next_client_id: None,
            expected_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxClientAssociationOutcome {
    result_code: String,
    binding_id: MailboxBindingId,
    client_id: Option<ClientId>,
    relationship_version: MailboxClientAssociationVersion,
    replayed: bool,
}

impl MailboxClientAssociationOutcome {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn client_id(&self) -> Option<&ClientId> {
        self.client_id.as_ref()
    }

    #[must_use]
    pub const fn relationship_version(&self) -> MailboxClientAssociationVersion {
        self.relationship_version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxClientAssociationOperationError {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for MailboxClientAssociationOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "mailbox/client relationship target not found",
            Self::VersionConflict => "mailbox/client relationship version conflict",
            Self::InvalidState => "mailbox/client relationship state is invalid",
            Self::Conflict => "mailbox/client relationship command conflict",
            Self::IntegrityFailure => "mailbox/client relationship integrity failure",
            Self::InternalFailure => "mailbox/client relationship internal failure",
            Self::DependencyUnavailable => "mailbox/client relationship dependency unavailable",
        })
    }
}

impl std::error::Error for MailboxClientAssociationOperationError {}

pub fn authorize_mailbox_client_association(
    role: MembershipRole,
) -> Result<(), MailboxClientAssociationOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(MailboxClientAssociationOperationError::NotFound)
    }
}

pub async fn execute_mailbox_client_association<P: MailboxClientAssociationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteMailboxClientAssociationCommand,
) -> Result<MailboxClientAssociationOutcome, MailboxClientAssociationOperationError> {
    authorize_mailbox_client_association(role)?;

    match port
        .decide_replay(actor, ASSOCIATION_CHANGE_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        MailboxClientAssociationReplayDecision::Miss => {}
        MailboxClientAssociationReplayDecision::Replay(receipt) => {
            return replay_outcome(command.binding_id, command.next_client_id, command.expected_version, &receipt);
        }
        MailboxClientAssociationReplayDecision::Conflict => {
            return Err(MailboxClientAssociationOperationError::Conflict);
        }
    }

    let context = port
        .load_context(
            actor.tenant_scope(),
            &command.binding_id,
            command.next_client_id.as_ref(),
        )
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxClientAssociationOperationError::NotFound)?;

    if !context.mailbox_executable() {
        return Err(MailboxClientAssociationOperationError::InvalidState);
    }
    if command.next_client_id.is_some() && !context.target_client_active() {
        return Err(MailboxClientAssociationOperationError::NotFound);
    }

    let mut association = context.association().clone();
    let previous_client_id = association.client_id().cloned();
    let action = match command.next_client_id.clone() {
        Some(client_id) => association
            .associate(command.expected_version, client_id)
            .map_err(map_domain_error)?,
        None => association
            .unbind(command.expected_version)
            .map_err(map_domain_error)?,
    };
    let next_client_id = association.client_id().cloned();
    let next_version = association.version();
    let result_code = result_code(action);

    let write = MailboxClientAssociationWrite::new(
        command.binding_id,
        previous_client_id,
        next_client_id.clone(),
        command.expected_version,
        next_version,
        action,
        command.evidence,
        ASSOCIATION_EVENT_PAYLOAD,
    );

    match port.change_association(actor, &write).await {
        Ok(()) => Ok(MailboxClientAssociationOutcome {
            result_code: result_code.to_owned(),
            binding_id: write.binding_id().clone(),
            client_id: next_client_id,
            relationship_version: next_version,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxClientAssociationPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, ASSOCIATION_CHANGE_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxClientAssociationReplayDecision::Replay(receipt) => replay_outcome(
                    write.binding_id().clone(),
                    write.next_client_id().cloned(),
                    write.next_version(),
                    &receipt,
                ),
                MailboxClientAssociationReplayDecision::Miss
                | MailboxClientAssociationReplayDecision::Conflict => {
                    Err(MailboxClientAssociationOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn replay_outcome(
    binding_id: MailboxBindingId,
    client_id: Option<ClientId>,
    version: MailboxClientAssociationVersion,
    receipt: &MailboxClientAssociationReplayReceipt,
) -> Result<MailboxClientAssociationOutcome, MailboxClientAssociationOperationError> {
    let result_code = receipt.result_code();
    if !matches!(result_code, "bound" | "rebound" | "unbound") {
        return Err(MailboxClientAssociationOperationError::IntegrityFailure);
    }
    Ok(MailboxClientAssociationOutcome {
        result_code: result_code.to_owned(),
        binding_id,
        client_id,
        relationship_version: version,
        replayed: true,
    })
}

const fn result_code(action: MailboxClientAssociationAction) -> &'static str {
    match action {
        MailboxClientAssociationAction::Bind => "bound",
        MailboxClientAssociationAction::Rebind => "rebound",
        MailboxClientAssociationAction::Unbind => "unbound",
    }
}

const fn map_domain_error(
    error: MailboxClientAssociationError,
) -> MailboxClientAssociationOperationError {
    match error {
        MailboxClientAssociationError::VersionConflict => {
            MailboxClientAssociationOperationError::VersionConflict
        }
        MailboxClientAssociationError::AlreadyAssociated
        | MailboxClientAssociationError::AlreadyUnassigned => {
            MailboxClientAssociationOperationError::InvalidState
        }
        MailboxClientAssociationError::VersionOverflow => {
            MailboxClientAssociationOperationError::InternalFailure
        }
    }
}

const fn map_port_error(
    error: MailboxClientAssociationPortError,
) -> MailboxClientAssociationOperationError {
    match error.class() {
        MailboxClientAssociationPortErrorClass::NotFound => {
            MailboxClientAssociationOperationError::NotFound
        }
        MailboxClientAssociationPortErrorClass::VersionConflict => {
            MailboxClientAssociationOperationError::VersionConflict
        }
        MailboxClientAssociationPortErrorClass::InvalidState => {
            MailboxClientAssociationOperationError::InvalidState
        }
        MailboxClientAssociationPortErrorClass::Conflict => {
            MailboxClientAssociationOperationError::Conflict
        }
        MailboxClientAssociationPortErrorClass::IntegrityFailure => {
            MailboxClientAssociationOperationError::IntegrityFailure
        }
        MailboxClientAssociationPortErrorClass::InternalFailure => {
            MailboxClientAssociationOperationError::InternalFailure
        }
        MailboxClientAssociationPortErrorClass::DependencyUnavailable => {
            MailboxClientAssociationOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{authorize_mailbox_client_association, MailboxClientAssociationOperationError};
    use identity_access_domain::MembershipRole;

    #[test]
    fn relationship_administration_remains_owner_only_and_neutral() {
        assert_eq!(
            authorize_mailbox_client_association(MembershipRole::TenantOwner),
            Ok(())
        );
        assert_eq!(
            authorize_mailbox_client_association(MembershipRole::Member),
            Err(MailboxClientAssociationOperationError::NotFound)
        );
    }
}
