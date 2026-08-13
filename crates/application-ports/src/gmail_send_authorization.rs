use crate::gmail_oauth_onboarding::{
    GmailOAuthAuthorizationCode, GmailOAuthStartReceipt, GmailOAuthState,
};
use core::fmt;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, MailboxBindingId, TenantId, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailSendAuthorizationCallbackTarget {
    tenant_id: TenantId,
    binding_id: MailboxBindingId,
    expected_version: AggregateVersion,
    starter_actor_id: ActorId,
    expires_at: UnixMillis,
}

impl GmailSendAuthorizationCallbackTarget {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        binding_id: MailboxBindingId,
        expected_version: AggregateVersion,
        starter_actor_id: ActorId,
        expires_at: UnixMillis,
    ) -> Self {
        Self {
            tenant_id,
            binding_id,
            expected_version,
            starter_actor_id,
            expires_at,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn expected_version(&self) -> AggregateVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn starter_actor_id(&self) -> &ActorId {
        &self.starter_actor_id
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GmailSendAuthorizationErrorClass {
    NotFound,
    Expired,
    ReplayRejected,
    ProviderDenied,
    Conflict,
    DependencyUnavailable,
    IntegrityFailure,
    InternalFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GmailSendAuthorizationError {
    class: GmailSendAuthorizationErrorClass,
}

impl GmailSendAuthorizationError {
    #[must_use]
    pub const fn new(class: GmailSendAuthorizationErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> GmailSendAuthorizationErrorClass {
        self.class
    }
}

impl fmt::Display for GmailSendAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Gmail send authorization operation failed")
    }
}

impl std::error::Error for GmailSendAuthorizationError {}

pub trait GmailSendAuthorizationPort {
    fn start(
        &self,
        actor: &ActorContext,
        binding_id: &MailboxBindingId,
        expected_version: AggregateVersion,
    ) -> impl core::future::Future<Output = Result<GmailOAuthStartReceipt, GmailSendAuthorizationError>>;

    fn inspect(
        &self,
        state: &GmailOAuthState,
    ) -> impl core::future::Future<
        Output = Result<GmailSendAuthorizationCallbackTarget, GmailSendAuthorizationError>,
    >;

    fn complete(
        &self,
        actor: &ActorContext,
        state: &GmailOAuthState,
        authorization_code: GmailOAuthAuthorizationCode,
    ) -> impl core::future::Future<Output = Result<(), GmailSendAuthorizationError>>;

    fn deny(
        &self,
        actor: &ActorContext,
        state: &GmailOAuthState,
    ) -> impl core::future::Future<Output = Result<(), GmailSendAuthorizationError>>;
}
