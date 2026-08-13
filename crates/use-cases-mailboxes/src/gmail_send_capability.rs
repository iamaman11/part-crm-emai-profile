use application_ports::gmail_oauth_onboarding::{
    GmailOAuthAuthorizationCode, GmailOAuthStartReceipt, GmailOAuthState,
};
use application_ports::gmail_send_authorization::{
    GmailSendAuthorizationCallbackTarget, GmailSendAuthorizationError,
    GmailSendAuthorizationErrorClass, GmailSendAuthorizationPort,
};
use application_ports::mailboxes::{
    MailboxBindingApplicationPort, MailboxBindingPortError, MailboxBindingPortErrorClass,
    MailboxBindingStatus, MailboxProvider,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, MailboxBindingId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GmailSendAuthorizationOperationError {
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

impl core::fmt::Display for GmailSendAuthorizationOperationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "Gmail send authorization target not found",
            Self::VersionConflict => "Gmail send authorization version conflict",
            Self::InvalidState => "Gmail send authorization state is invalid",
            Self::Conflict => "Gmail send authorization conflict",
            Self::Expired => "Gmail send authorization ceremony expired",
            Self::ReplayRejected => "Gmail send authorization callback replay rejected",
            Self::ProviderDenied => "Gmail send authorization was denied",
            Self::DependencyUnavailable => "Gmail send authorization dependency unavailable",
            Self::IntegrityFailure => "Gmail send authorization integrity failure",
            Self::InternalFailure => "Gmail send authorization internal failure",
        })
    }
}

impl std::error::Error for GmailSendAuthorizationOperationError {}

pub async fn start_gmail_send_authorization<B, P>(
    actor: &ActorContext,
    role: MembershipRole,
    bindings: &B,
    provisioning: &P,
    binding_id: MailboxBindingId,
    expected_version: AggregateVersion,
) -> Result<GmailOAuthStartReceipt, GmailSendAuthorizationOperationError>
where
    B: MailboxBindingApplicationPort,
    P: GmailSendAuthorizationPort,
{
    authorize_owner(role)?;
    validate_binding(actor, bindings, &binding_id, expected_version).await?;
    provisioning
        .start(actor, &binding_id, expected_version)
        .await
        .map_err(map_provisioning_error)
}

pub async fn inspect_gmail_send_authorization<P: GmailSendAuthorizationPort>(
    provisioning: &P,
    state: &GmailOAuthState,
) -> Result<GmailSendAuthorizationCallbackTarget, GmailSendAuthorizationOperationError> {
    provisioning
        .inspect(state)
        .await
        .map_err(map_provisioning_error)
}

pub async fn complete_gmail_send_authorization<B, P>(
    actor: &ActorContext,
    role: MembershipRole,
    bindings: &B,
    provisioning: &P,
    target: &GmailSendAuthorizationCallbackTarget,
    state: &GmailOAuthState,
    code: GmailOAuthAuthorizationCode,
) -> Result<(), GmailSendAuthorizationOperationError>
where
    B: MailboxBindingApplicationPort,
    P: GmailSendAuthorizationPort,
{
    validate_callback_actor(actor, role, target)?;
    validate_binding(
        actor,
        bindings,
        target.binding_id(),
        target.expected_version(),
    )
    .await?;
    provisioning
        .complete(actor, state, code)
        .await
        .map_err(map_provisioning_error)
}

pub async fn deny_gmail_send_authorization<B, P>(
    actor: &ActorContext,
    role: MembershipRole,
    bindings: &B,
    provisioning: &P,
    target: &GmailSendAuthorizationCallbackTarget,
    state: &GmailOAuthState,
) -> Result<(), GmailSendAuthorizationOperationError>
where
    B: MailboxBindingApplicationPort,
    P: GmailSendAuthorizationPort,
{
    validate_callback_actor(actor, role, target)?;
    validate_binding(
        actor,
        bindings,
        target.binding_id(),
        target.expected_version(),
    )
    .await?;
    provisioning
        .deny(actor, state)
        .await
        .map_err(map_provisioning_error)
}

async fn validate_binding<B: MailboxBindingApplicationPort>(
    actor: &ActorContext,
    bindings: &B,
    binding_id: &MailboxBindingId,
    expected_version: AggregateVersion,
) -> Result<(), GmailSendAuthorizationOperationError> {
    let binding = bindings
        .find_binding(actor.tenant_scope(), binding_id)
        .await
        .map_err(map_binding_error)?
        .ok_or(GmailSendAuthorizationOperationError::NotFound)?;
    if binding.provider() != MailboxProvider::GmailApi
        || binding.status() != MailboxBindingStatus::Active
    {
        return Err(GmailSendAuthorizationOperationError::InvalidState);
    }
    if binding.version() != expected_version {
        return Err(GmailSendAuthorizationOperationError::VersionConflict);
    }
    Ok(())
}

fn validate_callback_actor(
    actor: &ActorContext,
    role: MembershipRole,
    target: &GmailSendAuthorizationCallbackTarget,
) -> Result<(), GmailSendAuthorizationOperationError> {
    authorize_owner(role)?;
    if actor.tenant_scope().tenant_id() != target.tenant_id()
        || actor.actor_id() != target.starter_actor_id()
    {
        return Err(GmailSendAuthorizationOperationError::NotFound);
    }
    Ok(())
}

fn authorize_owner(role: MembershipRole) -> Result<(), GmailSendAuthorizationOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(GmailSendAuthorizationOperationError::NotFound)
    }
}

const fn map_provisioning_error(
    error: GmailSendAuthorizationError,
) -> GmailSendAuthorizationOperationError {
    match error.class() {
        GmailSendAuthorizationErrorClass::NotFound => GmailSendAuthorizationOperationError::NotFound,
        GmailSendAuthorizationErrorClass::Expired => GmailSendAuthorizationOperationError::Expired,
        GmailSendAuthorizationErrorClass::ReplayRejected => {
            GmailSendAuthorizationOperationError::ReplayRejected
        }
        GmailSendAuthorizationErrorClass::ProviderDenied => {
            GmailSendAuthorizationOperationError::ProviderDenied
        }
        GmailSendAuthorizationErrorClass::Conflict => GmailSendAuthorizationOperationError::Conflict,
        GmailSendAuthorizationErrorClass::DependencyUnavailable => {
            GmailSendAuthorizationOperationError::DependencyUnavailable
        }
        GmailSendAuthorizationErrorClass::IntegrityFailure => {
            GmailSendAuthorizationOperationError::IntegrityFailure
        }
        GmailSendAuthorizationErrorClass::InternalFailure => {
            GmailSendAuthorizationOperationError::InternalFailure
        }
    }
}

const fn map_binding_error(error: MailboxBindingPortError) -> GmailSendAuthorizationOperationError {
    match error.class() {
        MailboxBindingPortErrorClass::NotFound => GmailSendAuthorizationOperationError::NotFound,
        MailboxBindingPortErrorClass::VersionConflict => {
            GmailSendAuthorizationOperationError::VersionConflict
        }
        MailboxBindingPortErrorClass::InvalidState => {
            GmailSendAuthorizationOperationError::InvalidState
        }
        MailboxBindingPortErrorClass::Conflict => GmailSendAuthorizationOperationError::Conflict,
        MailboxBindingPortErrorClass::IntegrityFailure => {
            GmailSendAuthorizationOperationError::IntegrityFailure
        }
        MailboxBindingPortErrorClass::InternalFailure => {
            GmailSendAuthorizationOperationError::InternalFailure
        }
        MailboxBindingPortErrorClass::DependencyUnavailable => {
            GmailSendAuthorizationOperationError::DependencyUnavailable
        }
    }
}
