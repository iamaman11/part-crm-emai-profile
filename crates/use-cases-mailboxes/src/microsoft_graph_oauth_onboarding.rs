use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_onboarding::MailboxOnboardingApplicationPort;
use application_ports::microsoft_graph_oauth_onboarding::{
    MicrosoftGraphOAuthAuthorizationCode, MicrosoftGraphOAuthCallbackTarget,
    MicrosoftGraphOAuthProvisioningError, MicrosoftGraphOAuthProvisioningErrorClass,
    MicrosoftGraphOAuthProvisioningPort, MicrosoftGraphOAuthStartReceipt, MicrosoftGraphOAuthState,
};
use identity_access_domain::MembershipRole;
use mailbox_domain::{MailboxOnboardingStatus, MailboxOnboardingVersion, MailboxProvider};
use profile_platform_primitives::{ActorContext, MailboxOnboardingId};

use crate::mailbox_onboarding::{
    ExecuteMailboxOnboardingCommand, MailboxOnboardingOperationError, execute_mailbox_onboarding,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftGraphOAuthStartOutcome {
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    receipt: MicrosoftGraphOAuthStartReceipt,
}

impl MicrosoftGraphOAuthStartOutcome {
    #[must_use]
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        &self.onboarding_id
    }

    #[must_use]
    pub const fn expected_version(&self) -> MailboxOnboardingVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn receipt(&self) -> &MicrosoftGraphOAuthStartReceipt {
        &self.receipt
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftGraphOAuthCompletionOutcome {
    onboarding_id: MailboxOnboardingId,
    status: MailboxOnboardingStatus,
    version: MailboxOnboardingVersion,
    replayed: bool,
}

impl MicrosoftGraphOAuthCompletionOutcome {
    #[must_use]
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        &self.onboarding_id
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
pub enum MicrosoftGraphOAuthOnboardingError {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    Expired,
    ReplayRejected,
    ProviderDenied,
    DependencyUnavailable,
    IntegrityFailure,
    InternalFailure,
}

impl core::fmt::Display for MicrosoftGraphOAuthOnboardingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "Microsoft Graph OAuth onboarding target not found",
            Self::VersionConflict => "Microsoft Graph OAuth onboarding version conflict",
            Self::InvalidState => "Microsoft Graph OAuth onboarding state is invalid",
            Self::Conflict => "Microsoft Graph OAuth onboarding conflict",
            Self::Expired => "Microsoft Graph OAuth ceremony expired",
            Self::ReplayRejected => "Microsoft Graph OAuth callback replay rejected",
            Self::ProviderDenied => "Microsoft Graph OAuth provider denied authorization",
            Self::DependencyUnavailable => "Microsoft Graph OAuth dependency unavailable",
            Self::IntegrityFailure => "Microsoft Graph OAuth integrity failure",
            Self::InternalFailure => "Microsoft Graph OAuth internal failure",
        })
    }
}

impl std::error::Error for MicrosoftGraphOAuthOnboardingError {}

pub async fn start_microsoft_graph_oauth_onboarding<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
) -> Result<MicrosoftGraphOAuthStartOutcome, MicrosoftGraphOAuthOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: MicrosoftGraphOAuthProvisioningPort,
{
    authorize_owner(role)?;
    validate_onboarding(actor, onboarding_port, &onboarding_id, expected_version).await?;
    let receipt = provisioning_port
        .start(actor, &onboarding_id, expected_version)
        .await
        .map_err(map_provisioning_error)?;
    Ok(MicrosoftGraphOAuthStartOutcome {
        onboarding_id,
        expected_version,
        receipt,
    })
}

pub async fn inspect_microsoft_graph_oauth_callback<P: MicrosoftGraphOAuthProvisioningPort>(
    provisioning_port: &P,
    state: &MicrosoftGraphOAuthState,
) -> Result<MicrosoftGraphOAuthCallbackTarget, MicrosoftGraphOAuthOnboardingError> {
    provisioning_port
        .inspect(state)
        .await
        .map_err(map_provisioning_error)
}

pub async fn deny_microsoft_graph_oauth_callback<P: MicrosoftGraphOAuthProvisioningPort>(
    actor: &ActorContext,
    role: MembershipRole,
    provisioning_port: &P,
    target: &MicrosoftGraphOAuthCallbackTarget,
    state: &MicrosoftGraphOAuthState,
) -> Result<(), MicrosoftGraphOAuthOnboardingError> {
    validate_callback_actor(actor, role, target)?;
    provisioning_port
        .deny(actor, state)
        .await
        .map_err(map_provisioning_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn complete_microsoft_graph_oauth_callback<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    target: &MicrosoftGraphOAuthCallbackTarget,
    state: &MicrosoftGraphOAuthState,
    authorization_code: MicrosoftGraphOAuthAuthorizationCode,
    evidence: CommandExecutionEvidence,
) -> Result<MicrosoftGraphOAuthCompletionOutcome, MicrosoftGraphOAuthOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: MicrosoftGraphOAuthProvisioningPort,
{
    validate_callback_actor(actor, role, target)?;
    validate_onboarding(
        actor,
        onboarding_port,
        target.onboarding_id(),
        target.expected_version(),
    )
    .await?;

    let credential_handle = provisioning_port
        .complete(actor, state, authorization_code)
        .await
        .map_err(map_provisioning_error)?;
    let discard_handle = credential_handle.clone();
    let activation = execute_mailbox_onboarding(
        actor,
        role,
        onboarding_port,
        ExecuteMailboxOnboardingCommand::Activate {
            onboarding_id: target.onboarding_id().clone(),
            expected_version: target.expected_version(),
            credential_handle,
            status_metadata: None,
            evidence,
        },
    )
    .await;

    match activation {
        Ok(outcome) => Ok(MicrosoftGraphOAuthCompletionOutcome {
            onboarding_id: outcome.onboarding_id().clone(),
            status: outcome.status(),
            version: outcome.version(),
            replayed: outcome.replayed(),
        }),
        Err(error) => {
            if provisioning_port
                .discard(actor, &discard_handle)
                .await
                .is_err()
            {
                return Err(MicrosoftGraphOAuthOnboardingError::IntegrityFailure);
            }
            Err(map_onboarding_error(error))
        }
    }
}

async fn validate_onboarding<O: MailboxOnboardingApplicationPort>(
    actor: &ActorContext,
    onboarding_port: &O,
    onboarding_id: &MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
) -> Result<(), MicrosoftGraphOAuthOnboardingError> {
    let context = onboarding_port
        .load_context(actor.tenant_scope(), onboarding_id)
        .await
        .map_err(|error| map_onboarding_port_error(error.class()))?
        .ok_or(MicrosoftGraphOAuthOnboardingError::NotFound)?;
    let onboarding = context.onboarding();
    if onboarding.provider() != MailboxProvider::MicrosoftGraph {
        return Err(MicrosoftGraphOAuthOnboardingError::InvalidState);
    }
    if onboarding.version() != expected_version {
        return Err(MicrosoftGraphOAuthOnboardingError::VersionConflict);
    }
    if !matches!(
        onboarding.status(),
        MailboxOnboardingStatus::Pending | MailboxOnboardingStatus::ReauthRequired
    ) {
        return Err(MicrosoftGraphOAuthOnboardingError::InvalidState);
    }
    Ok(())
}

fn validate_callback_actor(
    actor: &ActorContext,
    role: MembershipRole,
    target: &MicrosoftGraphOAuthCallbackTarget,
) -> Result<(), MicrosoftGraphOAuthOnboardingError> {
    authorize_owner(role)?;
    if actor.tenant_scope().tenant_id() != target.tenant_id()
        || actor.actor_id() != target.starter_actor_id()
    {
        return Err(MicrosoftGraphOAuthOnboardingError::NotFound);
    }
    Ok(())
}

fn authorize_owner(role: MembershipRole) -> Result<(), MicrosoftGraphOAuthOnboardingError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(MicrosoftGraphOAuthOnboardingError::NotFound)
    }
}

const fn map_provisioning_error(
    error: MicrosoftGraphOAuthProvisioningError,
) -> MicrosoftGraphOAuthOnboardingError {
    match error.class() {
        MicrosoftGraphOAuthProvisioningErrorClass::NotFound => {
            MicrosoftGraphOAuthOnboardingError::NotFound
        }
        MicrosoftGraphOAuthProvisioningErrorClass::Expired => {
            MicrosoftGraphOAuthOnboardingError::Expired
        }
        MicrosoftGraphOAuthProvisioningErrorClass::ReplayRejected => {
            MicrosoftGraphOAuthOnboardingError::ReplayRejected
        }
        MicrosoftGraphOAuthProvisioningErrorClass::ProviderDenied => {
            MicrosoftGraphOAuthOnboardingError::ProviderDenied
        }
        MicrosoftGraphOAuthProvisioningErrorClass::Conflict => {
            MicrosoftGraphOAuthOnboardingError::Conflict
        }
        MicrosoftGraphOAuthProvisioningErrorClass::DependencyUnavailable => {
            MicrosoftGraphOAuthOnboardingError::DependencyUnavailable
        }
        MicrosoftGraphOAuthProvisioningErrorClass::IntegrityFailure => {
            MicrosoftGraphOAuthOnboardingError::IntegrityFailure
        }
        MicrosoftGraphOAuthProvisioningErrorClass::InternalFailure => {
            MicrosoftGraphOAuthOnboardingError::InternalFailure
        }
    }
}

const fn map_onboarding_error(
    error: MailboxOnboardingOperationError,
) -> MicrosoftGraphOAuthOnboardingError {
    match error {
        MailboxOnboardingOperationError::NotFound => MicrosoftGraphOAuthOnboardingError::NotFound,
        MailboxOnboardingOperationError::VersionConflict => {
            MicrosoftGraphOAuthOnboardingError::VersionConflict
        }
        MailboxOnboardingOperationError::InvalidState => {
            MicrosoftGraphOAuthOnboardingError::InvalidState
        }
        MailboxOnboardingOperationError::Conflict => MicrosoftGraphOAuthOnboardingError::Conflict,
        MailboxOnboardingOperationError::IntegrityFailure => {
            MicrosoftGraphOAuthOnboardingError::IntegrityFailure
        }
        MailboxOnboardingOperationError::InternalFailure => {
            MicrosoftGraphOAuthOnboardingError::InternalFailure
        }
        MailboxOnboardingOperationError::DependencyUnavailable => {
            MicrosoftGraphOAuthOnboardingError::DependencyUnavailable
        }
    }
}

const fn map_onboarding_port_error(
    class: application_ports::mailbox_onboarding::MailboxOnboardingPortErrorClass,
) -> MicrosoftGraphOAuthOnboardingError {
    use application_ports::mailbox_onboarding::MailboxOnboardingPortErrorClass;
    match class {
        MailboxOnboardingPortErrorClass::NotFound => MicrosoftGraphOAuthOnboardingError::NotFound,
        MailboxOnboardingPortErrorClass::VersionConflict => {
            MicrosoftGraphOAuthOnboardingError::VersionConflict
        }
        MailboxOnboardingPortErrorClass::InvalidState => {
            MicrosoftGraphOAuthOnboardingError::InvalidState
        }
        MailboxOnboardingPortErrorClass::Conflict => MicrosoftGraphOAuthOnboardingError::Conflict,
        MailboxOnboardingPortErrorClass::IntegrityFailure => {
            MicrosoftGraphOAuthOnboardingError::IntegrityFailure
        }
        MailboxOnboardingPortErrorClass::InternalFailure => {
            MicrosoftGraphOAuthOnboardingError::InternalFailure
        }
        MailboxOnboardingPortErrorClass::DependencyUnavailable => {
            MicrosoftGraphOAuthOnboardingError::DependencyUnavailable
        }
    }
}
