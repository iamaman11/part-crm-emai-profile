use crate::{MailboxError, MailboxProvider};
use profile_platform_primitives::{AggregateVersion, MailboxBindingId, SecretHandle, TenantId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxBindingStatus {
    Active,
    AuthRequired,
    Suspended,
    Revoked,
}

impl MailboxBindingStatus {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::Suspended => "SUSPENDED",
            Self::Revoked => "REVOKED",
        }
    }

    pub fn parse_storage(value: &str) -> Result<Self, MailboxError> {
        match value {
            "ACTIVE" => Ok(Self::Active),
            "AUTH_REQUIRED" => Ok(Self::AuthRequired),
            "SUSPENDED" => Ok(Self::Suspended),
            "REVOKED" => Ok(Self::Revoked),
            _ => Err(MailboxError::InvalidBindingStatus),
        }
    }

    #[must_use]
    pub const fn is_executable(self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxBinding {
    tenant_id: TenantId,
    binding_id: MailboxBindingId,
    provider: MailboxProvider,
    secret_handle: SecretHandle,
    status: MailboxBindingStatus,
    version: AggregateVersion,
}

impl MailboxBinding {
    #[must_use]
    pub const fn create(
        tenant_id: TenantId,
        binding_id: MailboxBindingId,
        provider: MailboxProvider,
        secret_handle: SecretHandle,
    ) -> Self {
        Self {
            tenant_id,
            binding_id,
            provider,
            secret_handle,
            status: MailboxBindingStatus::Active,
            version: AggregateVersion::INITIAL,
        }
    }

    #[must_use]
    pub const fn restore(
        tenant_id: TenantId,
        binding_id: MailboxBindingId,
        provider: MailboxProvider,
        secret_handle: SecretHandle,
        status: MailboxBindingStatus,
        version: AggregateVersion,
    ) -> Self {
        Self {
            tenant_id,
            binding_id,
            provider,
            secret_handle,
            status,
            version,
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
    pub const fn provider(&self) -> MailboxProvider {
        self.provider
    }

    #[must_use]
    pub const fn secret_handle(&self) -> &SecretHandle {
        &self.secret_handle
    }

    #[must_use]
    pub const fn status(&self) -> MailboxBindingStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn is_executable(&self) -> bool {
        self.status.is_executable()
    }

    pub fn require_auth(&mut self) -> Result<(), MailboxError> {
        self.transition_operational_status(MailboxBindingStatus::AuthRequired)
    }

    pub fn suspend(&mut self) -> Result<(), MailboxError> {
        self.transition_operational_status(MailboxBindingStatus::Suspended)
    }

    pub fn activate_with_secret_handle(
        &mut self,
        secret_handle: SecretHandle,
    ) -> Result<(), MailboxError> {
        if !matches!(
            self.status,
            MailboxBindingStatus::AuthRequired | MailboxBindingStatus::Suspended
        ) {
            return Err(MailboxError::InvalidBindingTransition);
        }
        self.bump_version()?;
        self.secret_handle = secret_handle;
        self.status = MailboxBindingStatus::Active;
        Ok(())
    }

    pub fn revoke(&mut self) -> Result<(), MailboxError> {
        if self.status == MailboxBindingStatus::Revoked {
            return Err(MailboxError::AlreadyRevoked);
        }
        self.bump_version()?;
        self.status = MailboxBindingStatus::Revoked;
        Ok(())
    }

    fn transition_operational_status(
        &mut self,
        next: MailboxBindingStatus,
    ) -> Result<(), MailboxError> {
        if self.status == MailboxBindingStatus::Revoked
            || self.status == next
            || next == MailboxBindingStatus::Active
            || next == MailboxBindingStatus::Revoked
        {
            return Err(MailboxError::InvalidBindingTransition);
        }
        self.bump_version()?;
        self.status = next;
        Ok(())
    }

    fn bump_version(&mut self) -> Result<(), MailboxError> {
        self.version = self
            .version
            .next()
            .map_err(|_| MailboxError::VersionOverflow)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MailboxBinding, MailboxBindingStatus};
    use crate::{MailboxError, MailboxJob, MailboxProvider};
    use profile_platform_primitives::{
        MailboxBindingId, MailboxJobId, SecretHandle, TenantId, UnixMillis,
    };

    fn binding() -> Result<MailboxBinding, Box<dyn std::error::Error>> {
        Ok(MailboxBinding::create(
            TenantId::parse("tenant_01JMAILBOX")?,
            MailboxBindingId::parse("mailbox_01JMAILBOX")?,
            MailboxProvider::Imap,
            SecretHandle::parse("secret_01JMAILBOX")?,
        ))
    }

    #[test]
    fn binding_contains_only_secret_handle_and_checked_version()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut binding = binding()?;
        assert_eq!(binding.secret_handle().as_str(), "secret_01JMAILBOX");
        assert_eq!(binding.status(), MailboxBindingStatus::Active);
        assert_eq!(binding.version().value(), 1);
        binding.require_auth()?;
        assert!(!binding.is_executable());
        binding.activate_with_secret_handle(SecretHandle::parse("secret_01JREFRESH")?)?;
        assert_eq!(binding.status(), MailboxBindingStatus::Active);
        assert_eq!(binding.secret_handle().as_str(), "secret_01JREFRESH");
        assert_eq!(binding.version().value(), 3);
        Ok(())
    }

    #[test]
    fn revoked_binding_cannot_start_job() -> Result<(), Box<dyn std::error::Error>> {
        let mut binding = binding()?;
        binding.revoke()?;
        assert_eq!(
            MailboxJob::create(
                &binding,
                MailboxJobId::parse("mailjob_01JREVOKED")?,
                None,
                UnixMillis::new(10),
                3,
            ),
            Err(MailboxError::BindingRevoked)
        );
        Ok(())
    }

    #[test]
    fn revoked_binding_is_terminal() -> Result<(), Box<dyn std::error::Error>> {
        let mut binding = binding()?;
        binding.suspend()?;
        binding.revoke()?;
        assert_eq!(binding.status(), MailboxBindingStatus::Revoked);
        assert_eq!(
            binding.require_auth(),
            Err(MailboxError::InvalidBindingTransition)
        );
        assert_eq!(
            binding.activate_with_secret_handle(SecretHandle::parse("secret_01JNOPE")?),
            Err(MailboxError::InvalidBindingTransition)
        );
        Ok(())
    }
}
