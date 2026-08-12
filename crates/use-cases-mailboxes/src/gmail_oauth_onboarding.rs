use application_ports::CommandExecutionEvidence;
use application_ports::gmail_oauth_onboarding::{
    GmailOAuthAuthorizationCode, GmailOAuthCallbackTarget, GmailOAuthProvisioningError,
    GmailOAuthProvisioningErrorClass, GmailOAuthProvisioningPort, GmailOAuthStartReceipt,
    GmailOAuthState,
};
use application_ports::mailbox_onboarding::MailboxOnboardingApplicationPort;
use identity_access_domain::MembershipRole;
use mailbox_domain::{MailboxOnboardingStatus, MailboxOnboardingVersion, MailboxProvider};
use profile_platform_primitives::{ActorContext, MailboxOnboardingId};

use crate::mailbox_onboarding::{
    ExecuteMailboxOnboardingCommand, MailboxOnboardingOperationError, execute_mailbox_onboarding,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailOAuthStartOutcome {
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    receipt: GmailOAuthStartReceipt,
}

impl GmailOAuthStartOutcome {
    #[must_use]
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        &self.onboarding_id
    }

    #[must_use]
    pub const fn expected_version(&self) -> MailboxOnboardingVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn receipt(&self) -> &GmailOAuthStartReceipt {
        &self.receipt
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailOAuthCompletionOutcome {
    onboarding_id: MailboxOnboardingId,
    status: MailboxOnboardingStatus,
    version: MailboxOnboardingVersion,
    replayed: bool,
}

impl GmailOAuthCompletionOutcome {
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
pub enum GmailOAuthOnboardingError {
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

impl core::fmt::Display for GmailOAuthOnboardingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "Gmail OAuth onboarding target not found",
            Self::VersionConflict => "Gmail OAuth onboarding version conflict",
            Self::InvalidState => "Gmail OAuth onboarding state is invalid",
            Self::Conflict => "Gmail OAuth onboarding conflict",
            Self::Expired => "Gmail OAuth ceremony expired",
            Self::ReplayRejected => "Gmail OAuth callback replay rejected",
            Self::ProviderDenied => "Gmail OAuth provider denied authorization",
            Self::DependencyUnavailable => "Gmail OAuth dependency unavailable",
            Self::IntegrityFailure => "Gmail OAuth integrity failure",
            Self::InternalFailure => "Gmail OAuth internal failure",
        })
    }
}

impl std::error::Error for GmailOAuthOnboardingError {}

pub async fn start_gmail_oauth_onboarding<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
) -> Result<GmailOAuthStartOutcome, GmailOAuthOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: GmailOAuthProvisioningPort,
{
    authorize_owner(role)?;
    validate_onboarding(
        actor,
        onboarding_port,
        &onboarding_id,
        expected_version,
    )
    .await?;
    let receipt = provisioning_port
        .start(actor, &onboarding_id, expected_version)
        .await
        .map_err(map_provisioning_error)?;
    Ok(GmailOAuthStartOutcome {
        onboarding_id,
        expected_version,
        receipt,
    })
}

pub async fn inspect_gmail_oauth_callback<P: GmailOAuthProvisioningPort>(
    provisioning_port: &P,
    state: &GmailOAuthState,
) -> Result<GmailOAuthCallbackTarget, GmailOAuthOnboardingError> {
    provisioning_port
        .inspect(state)
        .await
        .map_err(map_provisioning_error)
}

pub async fn deny_gmail_oauth_callback<P: GmailOAuthProvisioningPort>(
    actor: &ActorContext,
    role: MembershipRole,
    provisioning_port: &P,
    target: &GmailOAuthCallbackTarget,
    state: &GmailOAuthState,
) -> Result<(), GmailOAuthOnboardingError> {
    validate_callback_actor(actor, role, target)?;
    provisioning_port
        .deny(actor, state)
        .await
        .map_err(map_provisioning_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn complete_gmail_oauth_callback<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    target: &GmailOAuthCallbackTarget,
    state: &GmailOAuthState,
    authorization_code: GmailOAuthAuthorizationCode,
    evidence: CommandExecutionEvidence,
) -> Result<GmailOAuthCompletionOutcome, GmailOAuthOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: GmailOAuthProvisioningPort,
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
        Ok(outcome) => Ok(GmailOAuthCompletionOutcome {
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
                return Err(GmailOAuthOnboardingError::IntegrityFailure);
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
) -> Result<(), GmailOAuthOnboardingError> {
    let context = onboarding_port
        .load_context(actor.tenant_scope(), onboarding_id)
        .await
        .map_err(|error| map_onboarding_port_error(error.class()))?
        .ok_or(GmailOAuthOnboardingError::NotFound)?;
    let onboarding = context.onboarding();
    if onboarding.provider() != MailboxProvider::GmailApi {
        return Err(GmailOAuthOnboardingError::InvalidState);
    }
    if onboarding.version() != expected_version {
        return Err(GmailOAuthOnboardingError::VersionConflict);
    }
    if !matches!(
        onboarding.status(),
        MailboxOnboardingStatus::Pending | MailboxOnboardingStatus::ReauthRequired
    ) {
        return Err(GmailOAuthOnboardingError::InvalidState);
    }
    Ok(())
}

fn validate_callback_actor(
    actor: &ActorContext,
    role: MembershipRole,
    target: &GmailOAuthCallbackTarget,
) -> Result<(), GmailOAuthOnboardingError> {
    authorize_owner(role)?;
    if actor.tenant_scope().tenant_id() != target.tenant_id()
        || actor.actor_id() != target.starter_actor_id()
    {
        return Err(GmailOAuthOnboardingError::NotFound);
    }
    Ok(())
}

fn authorize_owner(role: MembershipRole) -> Result<(), GmailOAuthOnboardingError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(GmailOAuthOnboardingError::NotFound)
    }
}

const fn map_provisioning_error(error: GmailOAuthProvisioningError) -> GmailOAuthOnboardingError {
    match error.class() {
        GmailOAuthProvisioningErrorClass::NotFound => GmailOAuthOnboardingError::NotFound,
        GmailOAuthProvisioningErrorClass::Expired => GmailOAuthOnboardingError::Expired,
        GmailOAuthProvisioningErrorClass::ReplayRejected => GmailOAuthOnboardingError::ReplayRejected,
        GmailOAuthProvisioningErrorClass::ProviderDenied => GmailOAuthOnboardingError::ProviderDenied,
        GmailOAuthProvisioningErrorClass::Conflict => GmailOAuthOnboardingError::Conflict,
        GmailOAuthProvisioningErrorClass::DependencyUnavailable => {
            GmailOAuthOnboardingError::DependencyUnavailable
        }
        GmailOAuthProvisioningErrorClass::IntegrityFailure => {
            GmailOAuthOnboardingError::IntegrityFailure
        }
        GmailOAuthProvisioningErrorClass::InternalFailure => GmailOAuthOnboardingError::InternalFailure,
    }
}

const fn map_onboarding_error(error: MailboxOnboardingOperationError) -> GmailOAuthOnboardingError {
    match error {
        MailboxOnboardingOperationError::NotFound => GmailOAuthOnboardingError::NotFound,
        MailboxOnboardingOperationError::VersionConflict => GmailOAuthOnboardingError::VersionConflict,
        MailboxOnboardingOperationError::InvalidState => GmailOAuthOnboardingError::InvalidState,
        MailboxOnboardingOperationError::Conflict => GmailOAuthOnboardingError::Conflict,
        MailboxOnboardingOperationError::IntegrityFailure => GmailOAuthOnboardingError::IntegrityFailure,
        MailboxOnboardingOperationError::InternalFailure => GmailOAuthOnboardingError::InternalFailure,
        MailboxOnboardingOperationError::DependencyUnavailable => {
            GmailOAuthOnboardingError::DependencyUnavailable
        }
    }
}

const fn map_onboarding_port_error(
    class: application_ports::mailbox_onboarding::MailboxOnboardingPortErrorClass,
) -> GmailOAuthOnboardingError {
    use application_ports::mailbox_onboarding::MailboxOnboardingPortErrorClass;
    match class {
        MailboxOnboardingPortErrorClass::NotFound => GmailOAuthOnboardingError::NotFound,
        MailboxOnboardingPortErrorClass::VersionConflict => GmailOAuthOnboardingError::VersionConflict,
        MailboxOnboardingPortErrorClass::InvalidState => GmailOAuthOnboardingError::InvalidState,
        MailboxOnboardingPortErrorClass::Conflict => GmailOAuthOnboardingError::Conflict,
        MailboxOnboardingPortErrorClass::IntegrityFailure => GmailOAuthOnboardingError::IntegrityFailure,
        MailboxOnboardingPortErrorClass::InternalFailure => GmailOAuthOnboardingError::InternalFailure,
        MailboxOnboardingPortErrorClass::DependencyUnavailable => {
            GmailOAuthOnboardingError::DependencyUnavailable
        }
    }
}
