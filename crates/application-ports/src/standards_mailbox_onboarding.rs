use mailbox_domain::MailboxOnboardingVersion;
use profile_platform_primitives::{
    ActorContext, ActorId, IdempotencyKey, MailboxOnboardingId, SecretHandle, TenantId, UnixMillis,
};

const MAX_HOST_LENGTH: usize = 253;
const MAX_USERNAME_LENGTH: usize = 512;
const MAX_SECRET_LENGTH: usize = 8 * 1024;
const MAX_OAUTH_STATE_LENGTH: usize = 2_048;
const MAX_AUTHORIZATION_CODE_LENGTH: usize = 8_192;
const MAX_CEREMONY_ID_LENGTH: usize = 128;
const MAX_AUTHORIZATION_URL_LENGTH: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardsMailProtocol {
    Imap,
    Smtp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardsMailTransportSecurity {
    ImplicitTls,
    StartTls,
}

impl StandardsMailTransportSecurity {
    #[must_use]
    pub const fn resolver_value(self) -> &'static str {
        match self {
            Self::ImplicitTls => "IMPLICIT_TLS",
            Self::StartTls => "STARTTLS",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardsMailEndpoint {
    protocol: StandardsMailProtocol,
    host: String,
    port: u16,
    transport_security: StandardsMailTransportSecurity,
}

impl StandardsMailEndpoint {
    pub fn parse(
        protocol: StandardsMailProtocol,
        host: impl Into<String>,
        port: u16,
        transport_security: StandardsMailTransportSecurity,
    ) -> Result<Self, StandardsMailboxInputError> {
        let host = host.into();
        if !valid_public_dns_host(&host) || !valid_port(protocol, transport_security, port) {
            return Err(StandardsMailboxInputError::InvalidEndpoint);
        }
        Ok(Self {
            protocol,
            host,
            port,
            transport_security,
        })
    }

    #[must_use]
    pub const fn protocol(&self) -> StandardsMailProtocol {
        self.protocol
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn transport_security(&self) -> StandardsMailTransportSecurity {
        self.transport_security
    }
}

#[derive(Eq, PartialEq)]
pub struct StandardsMailboxUsername(String);

impl StandardsMailboxUsername {
    pub fn parse(value: impl Into<String>) -> Result<Self, StandardsMailboxInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_USERNAME_LENGTH
            || contains_control_or_line_break(&value)
        {
            return Err(StandardsMailboxInputError::InvalidUsername);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Eq, PartialEq)]
pub struct StandardsMailboxPassword(String);

impl StandardsMailboxPassword {
    pub fn parse(value: impl Into<String>) -> Result<Self, StandardsMailboxInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_SECRET_LENGTH
            || contains_control_or_line_break(&value)
        {
            return Err(StandardsMailboxInputError::InvalidPassword);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

pub struct StandardsPasswordProtocolCredential {
    endpoint: StandardsMailEndpoint,
    username: StandardsMailboxUsername,
    password: StandardsMailboxPassword,
}

impl StandardsPasswordProtocolCredential {
    #[must_use]
    pub const fn new(
        endpoint: StandardsMailEndpoint,
        username: StandardsMailboxUsername,
        password: StandardsMailboxPassword,
    ) -> Self {
        Self {
            endpoint,
            username,
            password,
        }
    }

    #[must_use]
    pub const fn endpoint(&self) -> &StandardsMailEndpoint {
        &self.endpoint
    }

    #[must_use]
    pub const fn username(&self) -> &StandardsMailboxUsername {
        &self.username
    }

    #[must_use]
    pub const fn password(&self) -> &StandardsMailboxPassword {
        &self.password
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        StandardsMailEndpoint,
        StandardsMailboxUsername,
        StandardsMailboxPassword,
    ) {
        (self.endpoint, self.username, self.password)
    }
}

pub struct StandardsPasswordMailboxConfiguration {
    imap: StandardsPasswordProtocolCredential,
    smtp: StandardsPasswordProtocolCredential,
}

impl StandardsPasswordMailboxConfiguration {
    pub fn new(
        imap: StandardsPasswordProtocolCredential,
        smtp: StandardsPasswordProtocolCredential,
    ) -> Result<Self, StandardsMailboxInputError> {
        if imap.endpoint().protocol() != StandardsMailProtocol::Imap
            || smtp.endpoint().protocol() != StandardsMailProtocol::Smtp
        {
            return Err(StandardsMailboxInputError::InvalidEndpoint);
        }
        Ok(Self { imap, smtp })
    }

    #[must_use]
    pub const fn imap(&self) -> &StandardsPasswordProtocolCredential {
        &self.imap
    }

    #[must_use]
    pub const fn smtp(&self) -> &StandardsPasswordProtocolCredential {
        &self.smtp
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        StandardsPasswordProtocolCredential,
        StandardsPasswordProtocolCredential,
    ) {
        (self.imap, self.smtp)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardsMailboxAuthenticationMode {
    Password,
    MicrosoftOAuth2,
}

impl StandardsMailboxAuthenticationMode {
    #[must_use]
    pub const fn public_value(self) -> &'static str {
        match self {
            Self::Password => "PASSWORD",
            Self::MicrosoftOAuth2 => "MICROSOFT_OAUTH2",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardsMailboxProvisioningReceipt {
    secret_handle: SecretHandle,
    authentication_mode: StandardsMailboxAuthenticationMode,
    imap_read_search_ready: bool,
    smtp_send_ready: bool,
}

impl StandardsMailboxProvisioningReceipt {
    #[must_use]
    pub const fn new(
        secret_handle: SecretHandle,
        authentication_mode: StandardsMailboxAuthenticationMode,
        imap_read_search_ready: bool,
        smtp_send_ready: bool,
    ) -> Self {
        Self {
            secret_handle,
            authentication_mode,
            imap_read_search_ready,
            smtp_send_ready,
        }
    }

    #[must_use]
    pub const fn secret_handle(&self) -> &SecretHandle {
        &self.secret_handle
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
    pub fn into_secret_handle(self) -> SecretHandle {
        self.secret_handle
    }
}

#[derive(Eq, PartialEq)]
pub struct MicrosoftStandardsOAuthState(String);

impl MicrosoftStandardsOAuthState {
    pub fn parse(value: impl Into<String>) -> Result<Self, StandardsMailboxInputError> {
        let value = value.into();
        if value.len() < 16
            || value.len() > MAX_OAUTH_STATE_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(StandardsMailboxInputError::InvalidOAuthState);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Eq, PartialEq)]
pub struct MicrosoftStandardsOAuthAuthorizationCode(String);

impl MicrosoftStandardsOAuthAuthorizationCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, StandardsMailboxInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_AUTHORIZATION_CODE_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(StandardsMailboxInputError::InvalidAuthorizationCode);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftStandardsOAuthCeremonyId(String);

impl MicrosoftStandardsOAuthCeremonyId {
    pub fn parse(value: impl Into<String>) -> Result<Self, StandardsMailboxInputError> {
        let value = value.into();
        if value.len() < 8
            || value.len() > MAX_CEREMONY_ID_LENGTH
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(StandardsMailboxInputError::InvalidCeremonyId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftStandardsOAuthAuthorizationUrl(String);

impl MicrosoftStandardsOAuthAuthorizationUrl {
    pub fn parse(value: impl Into<String>) -> Result<Self, StandardsMailboxInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_AUTHORIZATION_URL_LENGTH
            || value.chars().any(char::is_control)
            || !value.starts_with("https://login.microsoftonline.com/")
        {
            return Err(StandardsMailboxInputError::InvalidAuthorizationUrl);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftStandardsOAuthStartReceipt {
    ceremony_id: MicrosoftStandardsOAuthCeremonyId,
    authorization_url: MicrosoftStandardsOAuthAuthorizationUrl,
    expires_at: UnixMillis,
}

impl MicrosoftStandardsOAuthStartReceipt {
    #[must_use]
    pub const fn new(
        ceremony_id: MicrosoftStandardsOAuthCeremonyId,
        authorization_url: MicrosoftStandardsOAuthAuthorizationUrl,
        expires_at: UnixMillis,
    ) -> Self {
        Self {
            ceremony_id,
            authorization_url,
            expires_at,
        }
    }

    #[must_use]
    pub const fn ceremony_id(&self) -> &MicrosoftStandardsOAuthCeremonyId {
        &self.ceremony_id
    }

    #[must_use]
    pub const fn authorization_url(&self) -> &MicrosoftStandardsOAuthAuthorizationUrl {
        &self.authorization_url
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicrosoftStandardsOAuthCallbackTarget {
    tenant_id: TenantId,
    onboarding_id: MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
    starter_actor_id: ActorId,
    expires_at: UnixMillis,
}

impl MicrosoftStandardsOAuthCallbackTarget {
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
pub enum StandardsMailboxInputError {
    InvalidEndpoint,
    InvalidUsername,
    InvalidPassword,
    InvalidOAuthState,
    InvalidAuthorizationCode,
    InvalidCeremonyId,
    InvalidAuthorizationUrl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardsMailboxProvisioningErrorClass {
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
pub struct StandardsMailboxProvisioningError {
    class: StandardsMailboxProvisioningErrorClass,
}

impl StandardsMailboxProvisioningError {
    #[must_use]
    pub const fn new(class: StandardsMailboxProvisioningErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> StandardsMailboxProvisioningErrorClass {
        self.class
    }
}

impl core::fmt::Display for StandardsMailboxProvisioningError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("standards mailbox provisioning operation failed")
    }
}

impl std::error::Error for StandardsMailboxProvisioningError {}

pub trait StandardsMailboxProvisioningPort {
    fn provision_password(
        &self,
        actor: &ActorContext,
        onboarding_id: &MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
        idempotency_key: &IdempotencyKey,
        configuration: StandardsPasswordMailboxConfiguration,
    ) -> impl core::future::Future<
        Output = Result<StandardsMailboxProvisioningReceipt, StandardsMailboxProvisioningError>,
    >;

    fn start_microsoft_oauth(
        &self,
        actor: &ActorContext,
        onboarding_id: &MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
    ) -> impl core::future::Future<
        Output = Result<MicrosoftStandardsOAuthStartReceipt, StandardsMailboxProvisioningError>,
    >;

    fn inspect_microsoft_oauth(
        &self,
        state: &MicrosoftStandardsOAuthState,
    ) -> impl core::future::Future<
        Output = Result<MicrosoftStandardsOAuthCallbackTarget, StandardsMailboxProvisioningError>,
    >;

    fn complete_microsoft_oauth(
        &self,
        actor: &ActorContext,
        state: &MicrosoftStandardsOAuthState,
        authorization_code: MicrosoftStandardsOAuthAuthorizationCode,
    ) -> impl core::future::Future<
        Output = Result<StandardsMailboxProvisioningReceipt, StandardsMailboxProvisioningError>,
    >;

    fn deny_microsoft_oauth(
        &self,
        actor: &ActorContext,
        state: &MicrosoftStandardsOAuthState,
    ) -> impl core::future::Future<Output = Result<(), StandardsMailboxProvisioningError>>;

    fn discard(
        &self,
        actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> impl core::future::Future<Output = Result<(), StandardsMailboxProvisioningError>>;
}

fn valid_port(
    protocol: StandardsMailProtocol,
    transport_security: StandardsMailTransportSecurity,
    port: u16,
) -> bool {
    matches!(
        (protocol, transport_security, port),
        (
            StandardsMailProtocol::Imap,
            StandardsMailTransportSecurity::ImplicitTls,
            993
        ) | (
            StandardsMailProtocol::Imap,
            StandardsMailTransportSecurity::StartTls,
            143
        ) | (
            StandardsMailProtocol::Smtp,
            StandardsMailTransportSecurity::ImplicitTls,
            465
        ) | (
            StandardsMailProtocol::Smtp,
            StandardsMailTransportSecurity::StartTls,
            587
        )
    )
}

fn valid_public_dns_host(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_HOST_LENGTH
        || value.eq_ignore_ascii_case("localhost")
        || value.ends_with(".local")
        || value.ends_with(".localhost")
        || value.ends_with(".internal")
        || value.parse::<std::net::IpAddr>().is_ok()
    {
        return false;
    }
    let labels: Vec<&str> = value.split('.').collect();
    labels.len() >= 2 && labels.into_iter().all(valid_dns_label)
}

fn valid_dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn contains_control_or_line_break(value: &str) -> bool {
    value.bytes().any(|byte| byte == b'\0' || byte.is_ascii_control())
}

#[cfg(test)]
mod tests {
    use super::{
        MicrosoftStandardsOAuthAuthorizationCode, MicrosoftStandardsOAuthAuthorizationUrl,
        MicrosoftStandardsOAuthState, StandardsMailEndpoint, StandardsMailProtocol,
        StandardsMailTransportSecurity, StandardsMailboxPassword, StandardsMailboxUsername,
    };

    #[test]
    fn endpoints_require_public_dns_and_governed_tls_ports() {
        for forbidden in [
            "localhost",
            "mail.local",
            "mail.internal",
            "127.0.0.1",
            "::1",
            "singlelabel",
            "-bad.example",
        ] {
            assert!(
                StandardsMailEndpoint::parse(
                    StandardsMailProtocol::Imap,
                    forbidden,
                    993,
                    StandardsMailTransportSecurity::ImplicitTls,
                )
                .is_err(),
                "accepted forbidden host {forbidden}"
            );
        }
        assert!(
            StandardsMailEndpoint::parse(
                StandardsMailProtocol::Imap,
                "imap.example.com",
                993,
                StandardsMailTransportSecurity::ImplicitTls,
            )
            .is_ok()
        );
        assert!(
            StandardsMailEndpoint::parse(
                StandardsMailProtocol::Smtp,
                "smtp.example.com",
                587,
                StandardsMailTransportSecurity::StartTls,
            )
            .is_ok()
        );
        assert!(
            StandardsMailEndpoint::parse(
                StandardsMailProtocol::Smtp,
                "smtp.example.com",
                25,
                StandardsMailTransportSecurity::StartTls,
            )
            .is_err()
        );
    }

    #[test]
    fn password_and_username_reject_protocol_injection() {
        assert!(StandardsMailboxUsername::parse("user@example.com").is_ok());
        assert!(StandardsMailboxUsername::parse("user\r\nBAD").is_err());
        assert!(StandardsMailboxPassword::parse("safe password !@#$").is_ok());
        assert!(StandardsMailboxPassword::parse("secret\nLOGIN bad").is_err());
    }

    #[test]
    fn microsoft_callback_inputs_and_authorization_url_are_bounded() {
        assert!(MicrosoftStandardsOAuthState::parse("state_0123456789abcdef").is_ok());
        assert!(MicrosoftStandardsOAuthState::parse("short").is_err());
        assert!(MicrosoftStandardsOAuthAuthorizationCode::parse("code-value").is_ok());
        assert!(MicrosoftStandardsOAuthAuthorizationCode::parse("bad\ncode").is_err());
        assert!(
            MicrosoftStandardsOAuthAuthorizationUrl::parse(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?state=opaque"
            )
            .is_ok()
        );
        assert!(
            MicrosoftStandardsOAuthAuthorizationUrl::parse("https://graph.microsoft.com/oauth")
                .is_err()
        );
    }
}
