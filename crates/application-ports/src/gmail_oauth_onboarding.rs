use mailbox_domain::MailboxOnboardingVersion;
use profile_platform_primitives::{
    ActorContext, ActorId, MailboxOnboardingId, SecretHandle, TenantId, UnixMillis,
};
use zeroize::Zeroize;

const MAX_OAUTH_STATE_LENGTH: usize = 2_048;
const MAX_AUTHORIZATION_CODE_LENGTH: usize = 8_192;
const MAX_CEREMONY_ID_LENGTH: usize = 128;
const MAX_AUTHORIZATION_URL_LENGTH: usize = 4_096;

#[derive(Eq, PartialEq)]
pub struct GmailOAuthState(String);

impl GmailOAuthState {
    pub fn parse(value: impl Into<String>) -> Result<Self, GmailOAuthInputError> {
        let value = value.into();
        if value.len() < 16
            || value.len() > MAX_OAUTH_STATE_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(GmailOAuthInputError::InvalidState);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for GmailOAuthState {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Eq, PartialEq)]
pub struct GmailOAuthAuthorizationCode(String);

impl GmailOAuthAuthorizationCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, GmailOAuthInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_AUTHORIZATION_CODE_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(GmailOAuthInputError::InvalidAuthorizationCode);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for GmailOAuthAuthorizationCode {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailOAuthCeremonyId(String);

impl GmailOAuthCeremonyId {
    pub fn parse(value: impl Into<String>) -> Result<Self, GmailOAuthInputError> {
        let value = value.into();
        if value.len() < 8
            || value.len() > MAX_CEREMONY_ID_LENGTH
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(GmailOAuthInputError::InvalidCeremonyId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailOAuthAuthorizationUrl(String);

impl GmailOAuthAuthorizationUrl {
    pub fn parse(value: impl Into<String>) -> Result<Self, GmailOAuthInputError> {
        let value = value.into();
        if value.len() > MAX_AUTHORIZATION_URL_LENGTH
            || value.chars().any(char::is_control)
            || !value.starts_with("https://accounts.google.com/")
        {
            return Err(GmailOAuthInputError::InvalidAuthorizationUrl);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailOAuthStartReceipt {
    ceremony_id: GmailOAuthCeremonyId,
    authorization_url: GmailOAuthAuthorizationUrl,
    expires_at: UnixMillis,
}

impl GmailOAuthStartReceipt {
    #[must_use]
    pub const fn new(
        ceremony_id: GmailOAuthCeremonyId,
        authorization_url: GmailOAuthAuthorizationUrl,
        expires_at: UnixMillis,
    ) -> Self {
        Self {
            ceremony_id,
            authorization_url,
            expires_at,
        }
    }

    #[must_use]
    pub const fn ceremony_id(&self) -> &GmailOAuthCeremonyId {
        &self.ceremony_id
    }

    #[must_use]
    pub const fn authorization_url(&self) -> &GmailOAuthAuthorizationUrl {
        &self.authorization_url
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailOAuthCallbackTarget {
    tenant_id: TenantId,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    starter_actor_id: ActorId,
    expires_at: UnixMillis,
}

impl GmailOAuthCallbackTarget {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        onboarding_id: MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
        starter_actor_id: ActorId,
        expires_at: UnixMillis,
    ) -> Self {
        Self {
            tenant_id,
            onboarding_id,
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
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        &self.onboarding_id
    }

    #[must_use]
    pub const fn expected_version(&self) -> MailboxOnboardingVersion {
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
pub enum GmailOAuthInputError {
    InvalidState,
    InvalidAuthorizationCode,
    InvalidCeremonyId,
    InvalidAuthorizationUrl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GmailOAuthProvisioningErrorClass {
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
pub struct GmailOAuthProvisioningError {
    class: GmailOAuthProvisioningErrorClass,
}

impl GmailOAuthProvisioningError {
    #[must_use]
    pub const fn new(class: GmailOAuthProvisioningErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> GmailOAuthProvisioningErrorClass {
        self.class
    }
}

impl core::fmt::Display for GmailOAuthProvisioningError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Gmail OAuth provisioning operation failed")
    }
}

impl std::error::Error for GmailOAuthProvisioningError {}

pub trait GmailOAuthProvisioningPort {
    fn start(
        &self,
        actor: &ActorContext,
        onboarding_id: &MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
    ) -> impl core::future::Future<Output = Result<GmailOAuthStartReceipt, GmailOAuthProvisioningError>>;

    fn inspect(
        &self,
        state: &GmailOAuthState,
    ) -> impl core::future::Future<Output = Result<GmailOAuthCallbackTarget, GmailOAuthProvisioningError>>;

    fn complete(
        &self,
        actor: &ActorContext,
        state: &GmailOAuthState,
        authorization_code: GmailOAuthAuthorizationCode,
    ) -> impl core::future::Future<Output = Result<SecretHandle, GmailOAuthProvisioningError>>;

    fn deny(
        &self,
        actor: &ActorContext,
        state: &GmailOAuthState,
    ) -> impl core::future::Future<Output = Result<(), GmailOAuthProvisioningError>>;

    fn discard(
        &self,
        actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> impl core::future::Future<Output = Result<(), GmailOAuthProvisioningError>>;
}

#[cfg(test)]
mod tests {
    use super::{GmailOAuthAuthorizationCode, GmailOAuthAuthorizationUrl, GmailOAuthState};

    #[test]
    fn sensitive_callback_inputs_are_bounded_and_not_debuggable() {
        assert!(GmailOAuthState::parse("state_0123456789abcdef").is_ok());
        assert!(GmailOAuthState::parse("short").is_err());
        assert!(GmailOAuthAuthorizationCode::parse("code-value").is_ok());
        assert!(GmailOAuthAuthorizationCode::parse("bad\ncode").is_err());
        assert!(
            GmailOAuthAuthorizationUrl::parse(
                "https://accounts.google.com/o/oauth2/v2/auth?state=opaque"
            )
            .is_ok()
        );
        assert!(GmailOAuthAuthorizationUrl::parse("https://example.com/oauth").is_err());
    }
}
