use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_onboarding::{
    MailboxOnboardingApplicationPort, MailboxOnboardingPortError, MailboxOnboardingPortErrorClass,
    MailboxOnboardingReplayDecision, MailboxOnboardingReplayReceipt, MailboxOnboardingWrite,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use mailbox_domain::{
    MailboxOnboarding, MailboxOnboardingAction, MailboxOnboardingError, MailboxOnboardingStatus,
    MailboxOnboardingStatusMetadata, MailboxOnboardingVersion, MailboxProvider,
};
use profile_platform_primitives::{ActorContext, MailboxOnboardingId, SecretHandle};

const ONBOARDING_COMMAND: &str = "mailbox.onboarding_change";
const ONBOARDING_EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecuteMailboxOnboardingCommand {
    Start {
        onboarding_id: MailboxOnboardingId,
        provider: MailboxProvider,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
        evidence: CommandExecutionEvidence,
    },
    Activate {
        onboarding_id: MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
        credential_handle: SecretHandle,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
        evidence: CommandExecutionEvidence,
    },
    RequireReauth {
        onboarding_id: MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
        evidence: CommandExecutionEvidence,
    },
    Disable {
        onboarding_id: MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
        evidence: CommandExecutionEvidence,
    },
    MarkConfigError {
        onboarding_id: MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
        evidence: CommandExecutionEvidence,
    },
}

impl ExecuteMailboxOnboardingCommand {
    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        match self {
            Self::Start { evidence, .. }
            | Self::Activate { evidence, .. }
            | Self::RequireReauth { evidence, .. }
            | Self::Disable { evidence, .. }
            | Self::MarkConfigError { evidence, .. } => evidence,
        }
    }

    #[must_use]
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        match self {
            Self::Start { onboarding_id, .. }
            | Self::Activate { onboarding_id, .. }
            | Self::RequireReauth { onboarding_id, .. }
            | Self::Disable { onboarding_id, .. }
            | Self::MarkConfigError { onboarding_id, .. } => onboarding_id,
        }
    }

    #[must_use]
    pub const fn action(&self) -> MailboxOnboardingAction {
        match self {
            Self::Start { .. } => MailboxOnboardingAction::Start,
            Self::Activate { .. } => MailboxOnboardingAction::Activate,
            Self::RequireReauth { .. } => MailboxOnboardingAction::RequireReauth,
            Self::Disable { .. } => MailboxOnboardingAction::Disable,
            Self::MarkConfigError { .. } => MailboxOnboardingAction::MarkConfigError,
        }
    }

    fn replay_version(&self) -> Result<MailboxOnboardingVersion, MailboxOnboardingOperationError> {
        match self {
            Self::Start { .. } => Ok(MailboxOnboardingVersion::INITIAL),
            Self::Activate {
                expected_version, ..
            }
            | Self::RequireReauth {
                expected_version, ..
            }
            | Self::Disable {
                expected_version, ..
            }
            | Self::MarkConfigError {
                expected_version, ..
            } => expected_version.next().map_err(map_domain_error),
        }
    }

    #[must_use]
    pub const fn replay_status(&self) -> MailboxOnboardingStatus {
        match self {
            Self::Start { .. } => MailboxOnboardingStatus::Pending,
            Self::Activate { .. } => MailboxOnboardingStatus::Active,
            Self::RequireReauth { .. } => MailboxOnboardingStatus::ReauthRequired,
            Self::Disable { .. } => MailboxOnboardingStatus::Disabled,
            Self::MarkConfigError { .. } => MailboxOnboardingStatus::ConfigError,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxOnboardingOutcome {
    onboarding_id: MailboxOnboardingId,
    provider: MailboxProvider,
    status: MailboxOnboardingStatus,
    version: MailboxOnboardingVersion,
    replayed: bool,
}

impl MailboxOnboardingOutcome {
    #[must_use]
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        &self.onboarding_id
    }

    #[must_use]
    pub const fn provider(&self) -> MailboxProvider {
        self.provider
    }

    #[must_use]
    pub const fn status(&self) -> MailboxOnboardingStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> MailboxOnboardingVersion {
        self.version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxOnboardingOperationError {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for MailboxOnboardingOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "mailbox onboarding target not found",
            Self::VersionConflict => "mailbox onboarding version conflict",
            Self::InvalidState => "mailbox onboarding state is invalid",
            Self::Conflict => "mailbox onboarding command conflict",
            Self::IntegrityFailure => "mailbox onboarding integrity failure",
            Self::InternalFailure => "mailbox onboarding internal failure",
            Self::DependencyUnavailable => "mailbox onboarding dependency unavailable",
        })
    }
}

impl std::error::Error for MailboxOnboardingOperationError {}

pub fn authorize_mailbox_onboarding(
    role: MembershipRole,
) -> Result<(), MailboxOnboardingOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(MailboxOnboardingOperationError::NotFound)
    }
}

pub async fn execute_mailbox_onboarding<P: MailboxOnboardingApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteMailboxOnboardingCommand,
) -> Result<MailboxOnboardingOutcome, MailboxOnboardingOperationError> {
    authorize_mailbox_onboarding(role)?;

    match port
        .decide_replay(actor, ONBOARDING_COMMAND, command.evidence())
        .await
        .map_err(map_port_error)?
    {
        MailboxOnboardingReplayDecision::Miss => {}
        MailboxOnboardingReplayDecision::Replay(receipt) => {
            let provider = match &command {
                ExecuteMailboxOnboardingCommand::Start { provider, .. } => *provider,
                _ => {
                    port.load_context(actor.tenant_scope(), command.onboarding_id())
                        .await
                        .map_err(map_port_error)?
                        .ok_or(MailboxOnboardingOperationError::NotFound)?
                        .onboarding()
                        .provider()
                }
            };
            return replay_outcome(&command, &receipt, provider);
        }
        MailboxOnboardingReplayDecision::Conflict => {
            return Err(MailboxOnboardingOperationError::Conflict);
        }
    }

    let write = prepare_write(actor, port, command).await?;
    match port.commit(actor, &write).await {
        Ok(()) => Ok(outcome_from_write(&write, false)),
        Err(error) if error.class() == MailboxOnboardingPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, ONBOARDING_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxOnboardingReplayDecision::Replay(receipt) => {
                    validate_receipt(write.action(), write.onboarding_id(), &receipt)?;
                    Ok(outcome_from_write(&write, true))
                }
                MailboxOnboardingReplayDecision::Miss
                | MailboxOnboardingReplayDecision::Conflict => {
                    Err(MailboxOnboardingOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

async fn prepare_write<P: MailboxOnboardingApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command: ExecuteMailboxOnboardingCommand,
) -> Result<MailboxOnboardingWrite, MailboxOnboardingOperationError> {
    match command {
        ExecuteMailboxOnboardingCommand::Start {
            onboarding_id,
            provider,
            status_metadata,
            evidence,
        } => {
            if port
                .load_context(actor.tenant_scope(), &onboarding_id)
                .await
                .map_err(map_port_error)?
                .is_some()
            {
                return Err(MailboxOnboardingOperationError::InvalidState);
            }
            let onboarding = MailboxOnboarding::start(
                actor.tenant_scope().tenant_id().clone(),
                onboarding_id.clone(),
                provider,
                status_metadata.clone(),
            );
            Ok(MailboxOnboardingWrite::new(
                onboarding_id,
                provider,
                None,
                onboarding.status(),
                None,
                None,
                status_metadata,
                MailboxOnboardingVersion::NONE,
                onboarding.version(),
                MailboxOnboardingAction::Start,
                evidence,
                ONBOARDING_EVENT_PAYLOAD,
            ))
        }
        ExecuteMailboxOnboardingCommand::Activate {
            onboarding_id,
            expected_version,
            credential_handle,
            status_metadata,
            evidence,
        } => {
            prepare_transition(
                actor,
                port,
                onboarding_id,
                expected_version,
                MailboxOnboardingAction::Activate,
                Some(credential_handle),
                status_metadata,
                evidence,
            )
            .await
        }
        ExecuteMailboxOnboardingCommand::RequireReauth {
            onboarding_id,
            expected_version,
            status_metadata,
            evidence,
        } => {
            prepare_transition(
                actor,
                port,
                onboarding_id,
                expected_version,
                MailboxOnboardingAction::RequireReauth,
                None,
                status_metadata,
                evidence,
            )
            .await
        }
        ExecuteMailboxOnboardingCommand::Disable {
            onboarding_id,
            expected_version,
            status_metadata,
            evidence,
        } => {
            prepare_transition(
                actor,
                port,
                onboarding_id,
                expected_version,
                MailboxOnboardingAction::Disable,
                None,
                status_metadata,
                evidence,
            )
            .await
        }
        ExecuteMailboxOnboardingCommand::MarkConfigError {
            onboarding_id,
            expected_version,
            status_metadata,
            evidence,
        } => {
            prepare_transition(
                actor,
                port,
                onboarding_id,
                expected_version,
                MailboxOnboardingAction::MarkConfigError,
                None,
                status_metadata,
                evidence,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn prepare_transition<P: MailboxOnboardingApplicationPort>(
    actor: &ActorContext,
    port: &P,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    action: MailboxOnboardingAction,
    credential_handle: Option<SecretHandle>,
    status_metadata: Option<MailboxOnboardingStatusMetadata>,
    evidence: CommandExecutionEvidence,
) -> Result<MailboxOnboardingWrite, MailboxOnboardingOperationError> {
    let context = port
        .load_context(actor.tenant_scope(), &onboarding_id)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxOnboardingOperationError::NotFound)?;
    let mut onboarding = context.onboarding().clone();
    let provider = onboarding.provider();
    let previous_status = onboarding.status();
    let previous_credential_handle = onboarding.credential_handle().cloned();
    onboarding
        .transition(
            expected_version,
            action,
            credential_handle,
            status_metadata.clone(),
        )
        .map_err(map_domain_error)?;
    Ok(MailboxOnboardingWrite::new(
        onboarding_id,
        provider,
        Some(previous_status),
        onboarding.status(),
        previous_credential_handle,
        onboarding.credential_handle().cloned(),
        status_metadata,
        expected_version,
        onboarding.version(),
        action,
        evidence,
        ONBOARDING_EVENT_PAYLOAD,
    ))
}

fn replay_outcome(
    command: &ExecuteMailboxOnboardingCommand,
    receipt: &MailboxOnboardingReplayReceipt,
    provider: MailboxProvider,
) -> Result<MailboxOnboardingOutcome, MailboxOnboardingOperationError> {
    validate_receipt(command.action(), command.onboarding_id(), receipt)?;
    Ok(MailboxOnboardingOutcome {
        onboarding_id: command.onboarding_id().clone(),
        provider,
        status: command.replay_status(),
        version: command.replay_version()?,
        replayed: true,
    })
}

fn outcome_from_write(write: &MailboxOnboardingWrite, replayed: bool) -> MailboxOnboardingOutcome {
    MailboxOnboardingOutcome {
        onboarding_id: write.onboarding_id().clone(),
        provider: write.provider(),
        status: write.next_status(),
        version: write.next_version(),
        replayed,
    }
}

fn validate_receipt(
    action: MailboxOnboardingAction,
    onboarding_id: &MailboxOnboardingId,
    receipt: &MailboxOnboardingReplayReceipt,
) -> Result<(), MailboxOnboardingOperationError> {
    if receipt.result_code() != result_code(action)
        || receipt
            .result_reference()
            .is_some_and(|reference| reference != onboarding_id.as_str())
    {
        Err(MailboxOnboardingOperationError::IntegrityFailure)
    } else {
        Ok(())
    }
}

const fn result_code(action: MailboxOnboardingAction) -> &'static str {
    match action {
        MailboxOnboardingAction::Start => "started",
        MailboxOnboardingAction::Activate => "activated",
        MailboxOnboardingAction::RequireReauth => "reauth_required",
        MailboxOnboardingAction::Disable => "disabled",
        MailboxOnboardingAction::MarkConfigError => "config_error",
    }
}

const fn map_domain_error(error: MailboxOnboardingError) -> MailboxOnboardingOperationError {
    match error {
        MailboxOnboardingError::VersionConflict => MailboxOnboardingOperationError::VersionConflict,
        MailboxOnboardingError::VersionOverflow => MailboxOnboardingOperationError::InternalFailure,
        MailboxOnboardingError::InvalidStatus
        | MailboxOnboardingError::InvalidStatusMetadata
        | MailboxOnboardingError::InvalidTransition
        | MailboxOnboardingError::CredentialHandleRequired
        | MailboxOnboardingError::CredentialHandleChangeNotAllowed => {
            MailboxOnboardingOperationError::InvalidState
        }
    }
}

const fn map_port_error(error: MailboxOnboardingPortError) -> MailboxOnboardingOperationError {
    match error.class() {
        MailboxOnboardingPortErrorClass::NotFound => MailboxOnboardingOperationError::NotFound,
        MailboxOnboardingPortErrorClass::VersionConflict => {
            MailboxOnboardingOperationError::VersionConflict
        }
        MailboxOnboardingPortErrorClass::InvalidState => {
            MailboxOnboardingOperationError::InvalidState
        }
        MailboxOnboardingPortErrorClass::Conflict => MailboxOnboardingOperationError::Conflict,
        MailboxOnboardingPortErrorClass::IntegrityFailure => {
            MailboxOnboardingOperationError::IntegrityFailure
        }
        MailboxOnboardingPortErrorClass::InternalFailure => {
            MailboxOnboardingOperationError::InternalFailure
        }
        MailboxOnboardingPortErrorClass::DependencyUnavailable => {
            MailboxOnboardingOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MailboxOnboardingOperationError, authorize_mailbox_onboarding};
    use identity_access_domain::MembershipRole;

    #[test]
    fn onboarding_administration_is_owner_only_and_neutral() {
        assert_eq!(
            authorize_mailbox_onboarding(MembershipRole::TenantOwner),
            Ok(())
        );
        assert_eq!(
            authorize_mailbox_onboarding(MembershipRole::Member),
            Err(MailboxOnboardingOperationError::NotFound)
        );
    }
}
