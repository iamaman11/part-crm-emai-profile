use mailbox_domain::MailboxOnboardingVersion;
use profile_platform_primitives::{
    ActorContext, ActorId, MailboxOnboardingId, SecretHandle, TenantId, UnixMillis,
};

const MAX_OAUTH_STATE_LENGTH: usize = 2_048;
const MAX_AUTHORIZATION_CODE_LENGTH: usize = 8_192;
const MAX_CEREMONY_ID_LENGTH: usize = 128;
const MAX_AUTHORIZATION_URL_LENGTH: usize = 4_096;
const MICROSOFT_AUTHORIZATION_ORIGIN: &str = "https://login.microsoftonline.com/";

#[derive(Eq, PartialEq)]
pub struct MicrosoftGraphOAuthState(String);

impl MicrosoftGraphOAuthState {
    pub fn parse(value: impl Into<String>) -> Result<Self, MicrosoftGraphOAuthInputError> {
        let value = value.into();
        if value.len() < 16
            || value.len() > MAX_OAUTH_STATE_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(MicrosoftGraphOAuthInputError::InvalidState);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Eq, PartialEq)]
pub struct MicrosoftGraphOAuthAuthorizationCode(String);

impl MicrosoftGraphOAuthAuthorizationCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, MicrosoftGraphOAuthInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_AUTHORIZATION_CODE_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(MicrosoftGraphOAuthInputError::InvalidAuthorizationCode);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftGraphOAuthCeremonyId(String);

impl MicrosoftGraphOAuthCeremonyId {
    pub fn parse(value: impl Into<String>) -> Result<Self, MicrosoftGraphOAuthInputError> {
        let value = value.into();
        if value.len() < 8
            || value.len() > MAX_CEREMONY_ID_LENGTH
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(MicrosoftGraphOAuthInputError::InvalidCeremonyId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftGraphOAuthAuthorizationUrl(String);

impl MicrosoftGraphOAuthAuthorizationUrl {
    pub fn parse(value: impl Into<String>) -> Result<Self, MicrosoftGraphOAuthInputError> {
        let value = value.into();
        if value.len() > MAX_AUTHORIZATION_URL_LENGTH
            || value.chars().any(char::is_control)
            || !value.starts_with(MICROSOFT_AUTHORIZATION_ORIGIN)
        {
            return Err(MicrosoftGraphOAuthInputError::InvalidAuthorizationUrl);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftGraphOAuthStartReceipt {
    ceremony_id: MicrosoftGraphOAuthCeremonyId,
    authorization_url: MicrosoftGraphOAuthAuthorizationUrl,
    expires_at: UnixMillis,
}

impl MicrosoftGraphOAuthStartReceipt {
    #[must_use]
    pub const fn new(
        ceremony_id: MicrosoftGraphOAuthCeremonyId,
        authorization_url: MicrosoftGraphOAuthAuthorizationUrl,
        expires_at: UnixMillis,
    ) -> Self {
        Self {
            ceremony_id,
            authorization_url,
            expires_at,
        }
    }

    #[must_use]
    pub const fn ceremony_id(&self) -> &MicrosoftGraphOAuthCeremonyId {
        &self.ceremony_id
    }

    #[must_use]
    pub const fn authorization_url(&self) -> &MicrosoftGraphOAuthAuthorizationUrl {
        &self.authorization_url
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftGraphOAuthCallbackTarget {
    tenant_id: TenantId,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    starter_actor_id: ActorId,
    expires_at: UnixMillis,
}

impl MicrosoftGraphOAuthCallbackTarget {
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
pub enum MicrosoftGraphOAuthInputError {
    InvalidState,
    InvalidAuthorizationCode,
    InvalidCeremonyId,
    InvalidAuthorizationUrl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MicrosoftGraphOAuthProvisioningErrorClass {
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
pub struct MicrosoftGraphOAuthProvisioningError {
    class: MicrosoftGraphOAuthProvisioningErrorClass,
}

impl MicrosoftGraphOAuthProvisioningError {
    #[must_use]
    pub const fn new(class: MicrosoftGraphOAuthProvisioningErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> MicrosoftGraphOAuthProvisioningErrorClass {
        self.class
    }
}

impl core::fmt::Display for MicrosoftGraphOAuthProvisioningError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Microsoft Graph OAuth provisioning operation failed")
    }
}

impl std::error::Error for MicrosoftGraphOAuthProvisioningError {}

pub trait MicrosoftGraphOAuthProvisioningPort {
    fn start(
        &self,
        actor: &ActorContext,
        onboarding_id: &MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
    ) -> impl core::future::Future<
        Output = Result<MicrosoftGraphOAuthStartReceipt, MicrosoftGraphOAuthProvisioningError>,
    >;

    fn inspect(
        &self,
        state: &MicrosoftGraphOAuthState,
    ) -> impl core::future::Future<
        Output = Result<MicrosoftGraphOAuthCallbackTarget, MicrosoftGraphOAuthProvisioningError>,
    >;

    fn complete(
        &self,
        actor: &ActorContext,
        state: &MicrosoftGraphOAuthState,
        authorization_code: MicrosoftGraphOAuthAuthorizationCode,
    ) -> impl core::future::Future<Output = Result<SecretHandle, MicrosoftGraphOAuthProvisioningError>>;

    fn deny(
        &self,
        actor: &ActorContext,
        state: &MicrosoftGraphOAuthState,
    ) -> impl core::future::Future<Output = Result<(), MicrosoftGraphOAuthProvisioningError>>;

    fn discard(
        &self,
        actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> impl core::future::Future<Output = Result<(), MicrosoftGraphOAuthProvisioningError>>;
}

#[cfg(test)]
mod tests {
    use super::{
        MicrosoftGraphOAuthAuthorizationCode, MicrosoftGraphOAuthAuthorizationUrl,
        MicrosoftGraphOAuthState,
    };

    #[test]
    fn callback_inputs_are_bounded_and_non_serializable() {
        assert!(MicrosoftGraphOAuthState::parse("state_0123456789abcdef").is_ok());
        assert!(MicrosoftGraphOAuthState::parse("short").is_err());
        assert!(MicrosoftGraphOAuthAuthorizationCode::parse("code-value").is_ok());
        assert!(MicrosoftGraphOAuthAuthorizationCode::parse("bad\ncode").is_err());
        assert!(
            MicrosoftGraphOAuthAuthorizationUrl::parse(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?state=opaque"
            )
            .is_ok()
        );
        assert!(
            MicrosoftGraphOAuthAuthorizationUrl::parse(
                "https://login.microsoftonline.com.evil.example/common/oauth2/v2.0/authorize"
            )
            .is_err()
        );
        assert!(MicrosoftGraphOAuthAuthorizationUrl::parse("https://example.com/oauth").is_err());
    }
}
