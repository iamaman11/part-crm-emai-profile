use crate::{MailboxError, MailboxProvider};
use profile_platform_primitives::{
    AggregateVersion, MailboxBindingId, SecretHandle, TenantId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxBindingStatus {
    Active,
    Revoked,
}

impl MailboxBindingStatus {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Revoked => "REVOKED",
        }
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

    pub fn revoke(&mut self) -> Result<(), MailboxError> {
        if self.status == MailboxBindingStatus::Revoked {
            return Err(MailboxError::AlreadyRevoked);
        }
        self.version = self
            .version
            .next()
            .map_err(|_| MailboxError::VersionOverflow)?;
        self.status = MailboxBindingStatus::Revoked;
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
        binding.revoke()?;
        assert_eq!(binding.status(), MailboxBindingStatus::Revoked);
        assert_eq!(binding.version().value(), 2);
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
}
