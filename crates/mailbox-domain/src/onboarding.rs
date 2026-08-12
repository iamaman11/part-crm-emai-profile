use crate::MailboxProvider;
use core::fmt;
use profile_platform_primitives::{MailboxOnboardingId, SecretHandle, TenantId};

const MAX_STATUS_METADATA_LEN: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxOnboardingVersion(u64);

impl MailboxOnboardingVersion {
    pub const NONE: Self = Self(0);
    pub const INITIAL: Self = Self(1);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Result<Self, MailboxOnboardingError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(MailboxOnboardingError::VersionOverflow)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxOnboardingStatus {
    Pending,
    Active,
    ReauthRequired,
    Disabled,
    ConfigError,
}

impl MailboxOnboardingStatus {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Active => "ACTIVE",
            Self::ReauthRequired => "REAUTH_REQUIRED",
            Self::Disabled => "DISABLED",
            Self::ConfigError => "CONFIG_ERROR",
        }
    }

    pub fn parse_storage(value: &str) -> Result<Self, MailboxOnboardingError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "ACTIVE" => Ok(Self::Active),
            "REAUTH_REQUIRED" => Ok(Self::ReauthRequired),
            "DISABLED" => Ok(Self::Disabled),
            "CONFIG_ERROR" => Ok(Self::ConfigError),
            _ => Err(MailboxOnboardingError::InvalidStatus),
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Disabled | Self::ConfigError)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxOnboardingAction {
    Start,
    Activate,
    RequireReauth,
    Disable,
    MarkConfigError,
}

impl MailboxOnboardingAction {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Start => "START",
            Self::Activate => "ACTIVATE",
            Self::RequireReauth => "REQUIRE_REAUTH",
            Self::Disable => "DISABLE",
            Self::MarkConfigError => "CONFIG_ERROR",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxOnboardingStatusMetadata(String);

impl MailboxOnboardingStatusMetadata {
    pub fn parse(value: impl Into<String>) -> Result<Self, MailboxOnboardingError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_STATUS_METADATA_LEN
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/')
            })
        {
            return Err(MailboxOnboardingError::InvalidStatusMetadata);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxOnboardingError {
    VersionConflict,
    VersionOverflow,
    InvalidStatus,
    InvalidStatusMetadata,
    InvalidTransition,
    CredentialHandleRequired,
    CredentialHandleChangeNotAllowed,
}

impl fmt::Display for MailboxOnboardingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::VersionConflict => "mailbox onboarding version conflict",
            Self::VersionOverflow => "mailbox onboarding version overflow",
            Self::InvalidStatus => "mailbox onboarding status is invalid",
            Self::InvalidStatusMetadata => "mailbox onboarding status metadata is invalid",
            Self::InvalidTransition => "mailbox onboarding transition is invalid",
            Self::CredentialHandleRequired => "mailbox onboarding activation requires a credential handle",
            Self::CredentialHandleChangeNotAllowed => {
                "mailbox onboarding credential handle may only change on activation"
            }
        })
    }
}

impl std::error::Error for MailboxOnboardingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxOnboarding {
    tenant_id: TenantId,
    onboarding_id: MailboxOnboardingId,
    provider: MailboxProvider,
    status: MailboxOnboardingStatus,
    credential_handle: Option<SecretHandle>,
    status_metadata: Option<MailboxOnboardingStatusMetadata>,
    version: MailboxOnboardingVersion,
}

impl MailboxOnboarding {
    #[must_use]
    pub const fn start(
        tenant_id: TenantId,
        onboarding_id: MailboxOnboardingId,
        provider: MailboxProvider,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
    ) -> Self {
        Self {
            tenant_id,
            onboarding_id,
            provider,
            status: MailboxOnboardingStatus::Pending,
            credential_handle: None,
            status_metadata,
            version: MailboxOnboardingVersion::INITIAL,
        }
    }

    #[must_use]
    pub const fn restore(
        tenant_id: TenantId,
        onboarding_id: MailboxOnboardingId,
        provider: MailboxProvider,
        status: MailboxOnboardingStatus,
        credential_handle: Option<SecretHandle>,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
        version: MailboxOnboardingVersion,
    ) -> Self {
        Self {
            tenant_id,
            onboarding_id,
            provider,
            status,
            credential_handle,
            status_metadata,
            version,
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
    pub const fn provider(&self) -> MailboxProvider {
        self.provider
    }

    #[must_use]
    pub const fn status(&self) -> MailboxOnboardingStatus {
        self.status
    }

    #[must_use]
    pub const fn credential_handle(&self) -> Option<&SecretHandle> {
        self.credential_handle.as_ref()
    }

    #[must_use]
    pub const fn status_metadata(&self) -> Option<&MailboxOnboardingStatusMetadata> {
        self.status_metadata.as_ref()
    }

    #[must_use]
    pub const fn version(&self) -> MailboxOnboardingVersion {
        self.version
    }

    pub fn transition(
        &mut self,
        expected_version: MailboxOnboardingVersion,
        action: MailboxOnboardingAction,
        credential_handle: Option<SecretHandle>,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
    ) -> Result<(), MailboxOnboardingError> {
        if self.version != expected_version {
            return Err(MailboxOnboardingError::VersionConflict);
        }

        let (next_status, next_credential_handle) = match action {
            MailboxOnboardingAction::Start => {
                return Err(MailboxOnboardingError::InvalidTransition);
            }
            MailboxOnboardingAction::Activate
                if matches!(
                    self.status,
                    MailboxOnboardingStatus::Pending | MailboxOnboardingStatus::ReauthRequired
                ) =>
            {
                let handle = credential_handle
                    .ok_or(MailboxOnboardingError::CredentialHandleRequired)?;
                (MailboxOnboardingStatus::Active, Some(handle))
            }
            MailboxOnboardingAction::RequireReauth
                if self.status == MailboxOnboardingStatus::Active =>
            {
                if credential_handle.is_some() {
                    return Err(MailboxOnboardingError::CredentialHandleChangeNotAllowed);
                }
                (
                    MailboxOnboardingStatus::ReauthRequired,
                    self.credential_handle.clone(),
                )
            }
            MailboxOnboardingAction::Disable
                if matches!(
                    self.status,
                    MailboxOnboardingStatus::Pending
                        | MailboxOnboardingStatus::Active
                        | MailboxOnboardingStatus::ReauthRequired
                ) =>
            {
                if credential_handle.is_some() {
                    return Err(MailboxOnboardingError::CredentialHandleChangeNotAllowed);
                }
                (MailboxOnboardingStatus::Disabled, self.credential_handle.clone())
            }
            MailboxOnboardingAction::MarkConfigError
                if matches!(
                    self.status,
                    MailboxOnboardingStatus::Pending
                        | MailboxOnboardingStatus::Active
                        | MailboxOnboardingStatus::ReauthRequired
                ) =>
            {
                if credential_handle.is_some() {
                    return Err(MailboxOnboardingError::CredentialHandleChangeNotAllowed);
                }
                (
                    MailboxOnboardingStatus::ConfigError,
                    self.credential_handle.clone(),
                )
            }
            _ => return Err(MailboxOnboardingError::InvalidTransition),
        };

        self.version = self.version.next()?;
        self.status = next_status;
        self.credential_handle = next_credential_handle;
        self.status_metadata = status_metadata;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MailboxOnboarding, MailboxOnboardingAction, MailboxOnboardingError,
        MailboxOnboardingStatus, MailboxOnboardingStatusMetadata, MailboxOnboardingVersion,
    };
    use crate::MailboxProvider;
    use profile_platform_primitives::{MailboxOnboardingId, SecretHandle, TenantId};

    fn pending() -> Result<MailboxOnboarding, Box<dyn std::error::Error>> {
        Ok(MailboxOnboarding::start(
            TenantId::parse("tenant_C1")?,
            MailboxOnboardingId::parse("onboarding_C1")?,
            MailboxProvider::GmailApi,
            Some(MailboxOnboardingStatusMetadata::parse("ceremony.started")?),
        ))
    }

    #[test]
    fn valid_reauth_lifecycle_is_versioned_and_opaque() -> Result<(), Box<dyn std::error::Error>> {
        let mut onboarding = pending()?;
        assert_eq!(onboarding.status(), MailboxOnboardingStatus::Pending);
        assert_eq!(onboarding.version(), MailboxOnboardingVersion::INITIAL);
        assert!(onboarding.credential_handle().is_none());

        onboarding.transition(
            MailboxOnboardingVersion::INITIAL,
            MailboxOnboardingAction::Activate,
            Some(SecretHandle::parse("secret_C1")?),
            Some(MailboxOnboardingStatusMetadata::parse("credential.accepted")?),
        )?;
        assert_eq!(onboarding.status(), MailboxOnboardingStatus::Active);
        assert_eq!(onboarding.version().value(), 2);
        assert_eq!(
            onboarding.credential_handle().map(SecretHandle::as_str),
            Some("secret_C1")
        );

        onboarding.transition(
            MailboxOnboardingVersion::new(2),
            MailboxOnboardingAction::RequireReauth,
            None,
            Some(MailboxOnboardingStatusMetadata::parse("credential.expired")?),
        )?;
        onboarding.transition(
            MailboxOnboardingVersion::new(3),
            MailboxOnboardingAction::Activate,
            Some(SecretHandle::parse("secret_C1_rotated")?),
            None,
        )?;
        onboarding.transition(
            MailboxOnboardingVersion::new(4),
            MailboxOnboardingAction::Disable,
            None,
            Some(MailboxOnboardingStatusMetadata::parse("operator.disabled")?),
        )?;
        assert_eq!(onboarding.status(), MailboxOnboardingStatus::Disabled);
        assert!(onboarding.status().is_terminal());
        assert_eq!(
            onboarding.transition(
                MailboxOnboardingVersion::new(5),
                MailboxOnboardingAction::Activate,
                Some(SecretHandle::parse("secret_C1_nope")?),
                None,
            ),
            Err(MailboxOnboardingError::InvalidTransition)
        );
        Ok(())
    }

    #[test]
    fn stale_cas_and_invalid_transition_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let mut onboarding = pending()?;
        assert_eq!(
            onboarding.transition(
                MailboxOnboardingVersion::NONE,
                MailboxOnboardingAction::Disable,
                None,
                None,
            ),
            Err(MailboxOnboardingError::VersionConflict)
        );
        assert_eq!(
            onboarding.transition(
                MailboxOnboardingVersion::INITIAL,
                MailboxOnboardingAction::RequireReauth,
                None,
                None,
            ),
            Err(MailboxOnboardingError::InvalidTransition)
        );
        assert_eq!(onboarding.status(), MailboxOnboardingStatus::Pending);
        assert_eq!(onboarding.version(), MailboxOnboardingVersion::INITIAL);
        Ok(())
    }

    #[test]
    fn activation_requires_handle_and_config_error_is_terminal()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut onboarding = pending()?;
        assert_eq!(
            onboarding.transition(
                MailboxOnboardingVersion::INITIAL,
                MailboxOnboardingAction::Activate,
                None,
                None,
            ),
            Err(MailboxOnboardingError::CredentialHandleRequired)
        );
        onboarding.transition(
            MailboxOnboardingVersion::INITIAL,
            MailboxOnboardingAction::MarkConfigError,
            None,
            Some(MailboxOnboardingStatusMetadata::parse("provider.config_invalid")?),
        )?;
        assert_eq!(onboarding.status(), MailboxOnboardingStatus::ConfigError);
        assert_eq!(
            onboarding.transition(
                MailboxOnboardingVersion::new(2),
                MailboxOnboardingAction::Disable,
                None,
                None,
            ),
            Err(MailboxOnboardingError::InvalidTransition)
        );
        Ok(())
    }

    #[test]
    fn metadata_is_bounded_and_machine_safe() {
        assert!(MailboxOnboardingStatusMetadata::parse("oauth code leaked").is_err());
        assert!(MailboxOnboardingStatusMetadata::parse("x".repeat(129)).is_err());
        assert!(MailboxOnboardingStatusMetadata::parse("provider.reason-1").is_ok());
    }
}
