use crate::mailboxes::{MailboxBindingOperationError, authorize_mailbox_binding};
use application_ports::CommandExecutionEvidence;
use application_ports::browser_mail_execution::{
    BrowserMailboxExecutionBindWrite, BrowserMailboxExecutionBindingApplicationPort,
};
use application_ports::mailboxes::{
    MailboxBindingPortError, MailboxBindingPortErrorClass, MailboxReplayDecision,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, MailboxBindingId, ProfileId};

const BROWSER_MAILBOX_EXECUTION_BIND_COMMAND: &str = "mailbox.browser_execution_bind";
const BROWSER_MAILBOX_EXECUTION_EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindBrowserMailboxExecutionCommand {
    binding_id: MailboxBindingId,
    profile_id: ProfileId,
    evidence: CommandExecutionEvidence,
}

impl BindBrowserMailboxExecutionCommand {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        profile_id: ProfileId,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            profile_id,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMailboxExecutionBindingOutcome {
    binding_id: MailboxBindingId,
    profile_id: ProfileId,
    replayed: bool,
}

impl BrowserMailboxExecutionBindingOutcome {
    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

pub async fn execute_bind_browser_mailbox_execution<
    P: BrowserMailboxExecutionBindingApplicationPort,
>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: BindBrowserMailboxExecutionCommand,
) -> Result<BrowserMailboxExecutionBindingOutcome, MailboxBindingOperationError> {
    authorize_mailbox_binding(role)?;

    match port
        .decide_replay(
            actor,
            BROWSER_MAILBOX_EXECUTION_BIND_COMMAND,
            &command.evidence,
        )
        .await
        .map_err(map_port_error)?
    {
        MailboxReplayDecision::Miss => {}
        MailboxReplayDecision::Replay(_) => {
            return Ok(BrowserMailboxExecutionBindingOutcome {
                binding_id: command.binding_id,
                profile_id: command.profile_id,
                replayed: true,
            });
        }
        MailboxReplayDecision::Conflict => return Err(MailboxBindingOperationError::Conflict),
    }

    let write = BrowserMailboxExecutionBindWrite::new(
        command.binding_id,
        command.profile_id,
        command.evidence,
        BROWSER_MAILBOX_EXECUTION_EVENT_PAYLOAD,
    );
    match port.bind_browser_mailbox_execution(actor, &write).await {
        Ok(()) => Ok(BrowserMailboxExecutionBindingOutcome {
            binding_id: write.binding_id().clone(),
            profile_id: write.profile_id().clone(),
            replayed: false,
        }),
        Err(error) if error.class() == MailboxBindingPortErrorClass::Conflict => {
            match port
                .decide_replay(
                    actor,
                    BROWSER_MAILBOX_EXECUTION_BIND_COMMAND,
                    write.evidence(),
                )
                .await
                .map_err(map_port_error)?
            {
                MailboxReplayDecision::Replay(_) => Ok(BrowserMailboxExecutionBindingOutcome {
                    binding_id: write.binding_id().clone(),
                    profile_id: write.profile_id().clone(),
                    replayed: true,
                }),
                MailboxReplayDecision::Miss | MailboxReplayDecision::Conflict => {
                    Err(MailboxBindingOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn map_port_error(error: MailboxBindingPortError) -> MailboxBindingOperationError {
    match error.class() {
        MailboxBindingPortErrorClass::NotFound => MailboxBindingOperationError::NotFound,
        MailboxBindingPortErrorClass::VersionConflict => {
            MailboxBindingOperationError::VersionConflict
        }
        MailboxBindingPortErrorClass::InvalidState => MailboxBindingOperationError::InvalidState,
        MailboxBindingPortErrorClass::Conflict => MailboxBindingOperationError::Conflict,
        MailboxBindingPortErrorClass::IntegrityFailure => {
            MailboxBindingOperationError::IntegrityFailure
        }
        MailboxBindingPortErrorClass::InternalFailure => {
            MailboxBindingOperationError::InternalFailure
        }
        MailboxBindingPortErrorClass::DependencyUnavailable => {
            MailboxBindingOperationError::DependencyUnavailable
        }
    }
}
