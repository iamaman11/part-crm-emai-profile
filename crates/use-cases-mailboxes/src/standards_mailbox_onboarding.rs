use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_onboarding::MailboxOnboardingApplicationPort;
use application_ports::standards_mailbox_onboarding::{
    MicrosoftStandardsOAuthAuthorizationCode, MicrosoftStandardsOAuthCallbackTarget,
    MicrosoftStandardsOAuthStartReceipt, MicrosoftStandardsOAuthState,
    StandardsMailboxAuthenticationMode, StandardsMailboxProvisioningError,
    StandardsMailboxProvisioningErrorClass, StandardsMailboxProvisioningPort,
    StandardsMailboxProvisioningReceipt, StandardsPasswordMailboxConfiguration,
};
use identity_access_domain::MembershipRole;
use mailbox_domain::{MailboxOnboardingStatus, MailboxOnboardingVersion, MailboxProvider};
use profile_platform_primitives::{ActorContext, MailboxOnboardingId, SecretHandle};

use crate::mailbox_onboarding::{
    ExecuteMailboxOnboardingCommand, MailboxOnboardingOperationError, execute_mailbox_onboarding,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardsMailboxActivationOutcome {
    onboarding_id: MailboxOnboardingId,
    status: MailboxOnboardingStatus,
    version: MailboxOnboardingVersion,
    authentication_mode: StandardsMailboxAuthenticationMode,
    imap_read_search_ready: bool,
    smtp_send_ready: bool,
    replayed: bool,
}

impl StandardsMailboxActivationOutcome {
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
    pub const fn authentication_mode(&self) -> StandardsMailboxAuthenticationMode {
        self.authentication_mode
    }

    #[must_use]
    pub const fn imap_read_search_ready(&self) -> bool {
        self.imap_read_search_ready
    }

    #[must_use]
    pub const fn smtp_send_ready(&self) -> bool {
        self.smtp_send_ready
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftStandardsOAuthStartOutcome {
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    receipt: MicrosoftStandardsOAuthStartReceipt,
}

impl MicrosoftStandardsOAuthStartOutcome {
    #[must_use]
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        &self.onboarding_id
    }

    #[must_use]
    pub const fn expected_version(&self) -> MailboxOnboardingVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn receipt(&self) -> &MicrosoftStandardsOAuthStartReceipt {
        &self.receipt
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardsMailboxOnboardingError {
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

impl core::fmt::Display for StandardsMailboxOnboardingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "standards mailbox onboarding target not found",
            Self::VersionConflict => "standards mailbox onboarding version conflict",
            Self::InvalidState => "standards mailbox onboarding state is invalid",
            Self::Conflict => "standards mailbox onboarding conflict",
            Self::Expired => "standards mailbox OAuth ceremony expired",
            Self::ReplayRejected => "standards mailbox OAuth callback replay rejected",
            Self::ProviderDenied => "standards mailbox OAuth provider denied authorization",
            Self::DependencyUnavailable => "standards mailbox dependency unavailable",
            Self::IntegrityFailure => "standards mailbox integrity failure",
            Self::InternalFailure => "standards mailbox internal failure",
        })
    }
}

impl std::error::Error for StandardsMailboxOnboardingError {}

#[allow(clippy::too_many_arguments)]
pub async fn provision_password_standards_mailbox<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    configuration: StandardsPasswordMailboxConfiguration,
    evidence: CommandExecutionEvidence,
) -> Result<StandardsMailboxActivationOutcome, StandardsMailboxOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: StandardsMailboxProvisioningPort,
{
    authorize_owner(role)?;
    validate_onboarding(actor, onboarding_port, &onboarding_id, expected_version).await?;
    let receipt = provisioning_port
        .provision_password(
            actor,
            &onboarding_id,
            expected_version,
            evidence.idempotency_key(),
            configuration,
        )
        .await
        .map_err(map_provisioning_error)?;
    activate_provisioned(
        actor,
        role,
        onboarding_port,
        provisioning_port,
        onboarding_id,
        expected_version,
        receipt,
        StandardsMailboxAuthenticationMode::Password,
        evidence,
    )
    .await
}

pub async fn start_microsoft_standards_oauth<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
) -> Result<MicrosoftStandardsOAuthStartOutcome, StandardsMailboxOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: StandardsMailboxProvisioningPort,
{
    authorize_owner(role)?;
    validate_onboarding(actor, onboarding_port, &onboarding_id, expected_version).await?;
    let receipt = provisioning_port
        .start_microsoft_oauth(actor, &onboarding_id, expected_version)
        .await
        .map_err(map_provisioning_error)?;
    Ok(MicrosoftStandardsOAuthStartOutcome {
        onboarding_id,
        expected_version,
        receipt,
    })
}

pub async fn inspect_microsoft_standards_oauth_callback<P: StandardsMailboxProvisioningPort>(
    provisioning_port: &P,
    state: &MicrosoftStandardsOAuthState,
) -> Result<MicrosoftStandardsOAuthCallbackTarget, StandardsMailboxOnboardingError> {
    provisioning_port
        .inspect_microsoft_oauth(state)
        .await
        .map_err(map_provisioning_error)
}

pub async fn deny_microsoft_standards_oauth_callback<P: StandardsMailboxProvisioningPort>(
    actor: &ActorContext,
    role: MembershipRole,
    provisioning_port: &P,
    target: &MicrosoftStandardsOAuthCallbackTarget,
    state: &MicrosoftStandardsOAuthState,
) -> Result<(), StandardsMailboxOnboardingError> {
    validate_callback_actor(actor, role, target)?;
    provisioning_port
        .deny_microsoft_oauth(actor, state)
        .await
        .map_err(map_provisioning_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn complete_microsoft_standards_oauth_callback<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    target: &MicrosoftStandardsOAuthCallbackTarget,
    state: &MicrosoftStandardsOAuthState,
    authorization_code: MicrosoftStandardsOAuthAuthorizationCode,
    evidence: CommandExecutionEvidence,
) -> Result<StandardsMailboxActivationOutcome, StandardsMailboxOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: StandardsMailboxProvisioningPort,
{
    validate_callback_actor(actor, role, target)?;
    validate_onboarding(
        actor,
        onboarding_port,
        target.onboarding_id(),
        target.expected_version(),
    )
    .await?;
    let receipt = provisioning_port
        .complete_microsoft_oauth(actor, state, authorization_code)
        .await
        .map_err(map_provisioning_error)?;
    activate_provisioned(
        actor,
        role,
        onboarding_port,
        provisioning_port,
        target.onboarding_id().clone(),
        target.expected_version(),
        receipt,
        StandardsMailboxAuthenticationMode::MicrosoftOAuth2,
        evidence,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn activate_provisioned<O, P>(
    actor: &ActorContext,
    role: MembershipRole,
    onboarding_port: &O,
    provisioning_port: &P,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    receipt: StandardsMailboxProvisioningReceipt,
    expected_authentication_mode: StandardsMailboxAuthenticationMode,
    evidence: CommandExecutionEvidence,
) -> Result<StandardsMailboxActivationOutcome, StandardsMailboxOnboardingError>
where
    O: MailboxOnboardingApplicationPort,
    P: StandardsMailboxProvisioningPort,
{
    let discard_handle = receipt.secret_handle().clone();
    let imap_read_search_ready = receipt.imap_read_search_ready();
    let smtp_send_ready = receipt.smtp_send_ready();
    let authentication_mode = receipt.authentication_mode();
    if authentication_mode != expected_authentication_mode
        || !imap_read_search_ready
        || !smtp_send_ready
    {
        discard_or_integrity(actor, provisioning_port, &discard_handle).await?;
        return Err(StandardsMailboxOnboardingError::IntegrityFailure);
    }
    let credential_handle = receipt.into_secret_handle();
    let activation = execute_mailbox_onboarding(
        actor,
        role,
        onboarding_port,
        ExecuteMailboxOnboardingCommand::Activate {
            onboarding_id,
            expected_version,
            credential_handle,
            status_metadata: None,
            evidence,
        },
    )
    .await;

    match activation {
        Ok(outcome) => Ok(StandardsMailboxActivationOutcome {
            onboarding_id: outcome.onboarding_id().clone(),
            status: outcome.status(),
            version: outcome.version(),
            authentication_mode,
            imap_read_search_ready,
            smtp_send_ready,
            replayed: outcome.replayed(),
        }),
        Err(error) => {
            discard_or_integrity(actor, provisioning_port, &discard_handle).await?;
            Err(map_onboarding_error(error))
        }
    }
}

async fn discard_or_integrity<P: StandardsMailboxProvisioningPort>(
    actor: &ActorContext,
    provisioning_port: &P,
    secret_handle: &SecretHandle,
) -> Result<(), StandardsMailboxOnboardingError> {
    provisioning_port
        .discard(actor, secret_handle)
        .await
        .map_err(|_| StandardsMailboxOnboardingError::IntegrityFailure)
}

async fn validate_onboarding<O: MailboxOnboardingApplicationPort>(
    actor: &ActorContext,
    onboarding_port: &O,
    onboarding_id: &MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
) -> Result<(), StandardsMailboxOnboardingError> {
    let context = onboarding_port
        .load_context(actor.tenant_scope(), onboarding_id)
        .await
        .map_err(|error| map_onboarding_port_error(error.class()))?
        .ok_or(StandardsMailboxOnboardingError::NotFound)?;
    let onboarding = context.onboarding();
    if onboarding.provider() != MailboxProvider::Imap {
        return Err(StandardsMailboxOnboardingError::InvalidState);
    }
    if onboarding.version() != expected_version {
        return Err(StandardsMailboxOnboardingError::VersionConflict);
    }
    if !matches!(
        onboarding.status(),
        MailboxOnboardingStatus::Pending | MailboxOnboardingStatus::ReauthRequired
    ) {
        return Err(StandardsMailboxOnboardingError::InvalidState);
    }
    Ok(())
}

fn validate_callback_actor(
    actor: &ActorContext,
    role: MembershipRole,
    target: &MicrosoftStandardsOAuthCallbackTarget,
) -> Result<(), StandardsMailboxOnboardingError> {
    authorize_owner(role)?;
    if actor.tenant_scope().tenant_id() != target.tenant_id()
        || actor.actor_id() != target.starter_actor_id()
    {
        return Err(StandardsMailboxOnboardingError::NotFound);
    }
    Ok(())
}

fn authorize_owner(role: MembershipRole) -> Result<(), StandardsMailboxOnboardingError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(StandardsMailboxOnboardingError::NotFound)
    }
}

const fn map_provisioning_error(
    error: StandardsMailboxProvisioningError,
) -> StandardsMailboxOnboardingError {
    match error.class() {
        StandardsMailboxProvisioningErrorClass::NotFound => StandardsMailboxOnboardingError::NotFound,
        StandardsMailboxProvisioningErrorClass::Expired => StandardsMailboxOnboardingError::Expired,
        StandardsMailboxProvisioningErrorClass::ReplayRejected => {
            StandardsMailboxOnboardingError::ReplayRejected
        }
        StandardsMailboxProvisioningErrorClass::ProviderDenied => {
            StandardsMailboxOnboardingError::ProviderDenied
        }
        StandardsMailboxProvisioningErrorClass::Conflict => StandardsMailboxOnboardingError::Conflict,
        StandardsMailboxProvisioningErrorClass::DependencyUnavailable => {
            StandardsMailboxOnboardingError::DependencyUnavailable
        }
        StandardsMailboxProvisioningErrorClass::IntegrityFailure => {
            StandardsMailboxOnboardingError::IntegrityFailure
        }
        StandardsMailboxProvisioningErrorClass::InternalFailure => {
            StandardsMailboxOnboardingError::InternalFailure
        }
    }
}

const fn map_onboarding_error(
    error: MailboxOnboardingOperationError,
) -> StandardsMailboxOnboardingError {
    match error {
        MailboxOnboardingOperationError::NotFound => StandardsMailboxOnboardingError::NotFound,
        MailboxOnboardingOperationError::VersionConflict => {
            StandardsMailboxOnboardingError::VersionConflict
        }
        MailboxOnboardingOperationError::InvalidState => StandardsMailboxOnboardingError::InvalidState,
        MailboxOnboardingOperationError::Conflict => StandardsMailboxOnboardingError::Conflict,
        MailboxOnboardingOperationError::IntegrityFailure => {
            StandardsMailboxOnboardingError::IntegrityFailure
        }
        MailboxOnboardingOperationError::InternalFailure => {
            StandardsMailboxOnboardingError::InternalFailure
        }
        MailboxOnboardingOperationError::DependencyUnavailable => {
            StandardsMailboxOnboardingError::DependencyUnavailable
        }
    }
}

const fn map_onboarding_port_error(
    class: application_ports::mailbox_onboarding::MailboxOnboardingPortErrorClass,
) -> StandardsMailboxOnboardingError {
    use application_ports::mailbox_onboarding::MailboxOnboardingPortErrorClass;
    match class {
        MailboxOnboardingPortErrorClass::NotFound => StandardsMailboxOnboardingError::NotFound,
        MailboxOnboardingPortErrorClass::VersionConflict => {
            StandardsMailboxOnboardingError::VersionConflict
        }
        MailboxOnboardingPortErrorClass::InvalidState => StandardsMailboxOnboardingError::InvalidState,
        MailboxOnboardingPortErrorClass::Conflict => StandardsMailboxOnboardingError::Conflict,
        MailboxOnboardingPortErrorClass::IntegrityFailure => {
            StandardsMailboxOnboardingError::IntegrityFailure
        }
        MailboxOnboardingPortErrorClass::InternalFailure => {
            StandardsMailboxOnboardingError::InternalFailure
        }
        MailboxOnboardingPortErrorClass::DependencyUnavailable => {
            StandardsMailboxOnboardingError::DependencyUnavailable
        }
    }
}
