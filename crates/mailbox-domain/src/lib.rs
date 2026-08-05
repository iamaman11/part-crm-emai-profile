#![forbid(unsafe_code)]

use core::fmt;
use profile_platform_primitives::{MailboxBindingId, SecretHandle, TenantId};

const MAX_CURSOR_LENGTH: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxProvider {
    GmailApi,
    Imap,
    BrowserFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxBindingStatus {
    Active,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxBinding {
    tenant_id: TenantId,
    binding_id: MailboxBindingId,
    provider: MailboxProvider,
    secret_handle: SecretHandle,
    status: MailboxBindingStatus,
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

    pub fn revoke(&mut self) -> Result<(), MailboxError> {
        if self.status == MailboxBindingStatus::Revoked {
            return Err(MailboxError::AlreadyRevoked);
        }
        self.status = MailboxBindingStatus::Revoked;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxJobStatus {
    Pending,
    Running,
    RetryPending,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJob {
    tenant_id: TenantId,
    binding_id: MailboxBindingId,
    cursor: Option<String>,
    status: MailboxJobStatus,
}

impl MailboxJob {
    pub fn create(binding: &MailboxBinding, cursor: Option<String>) -> Result<Self, MailboxError> {
        if binding.status() != MailboxBindingStatus::Active {
            return Err(MailboxError::BindingRevoked);
        }
        if cursor.as_ref().is_some_and(|value| value.len() > MAX_CURSOR_LENGTH) {
            return Err(MailboxError::CursorTooLong);
        }

        Ok(Self {
            tenant_id: binding.tenant_id().clone(),
            binding_id: binding.binding_id().clone(),
            cursor,
            status: MailboxJobStatus::Pending,
        })
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
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    #[must_use]
    pub const fn status(&self) -> MailboxJobStatus {
        self.status
    }

    pub fn transition(&mut self, next: MailboxJobStatus) -> Result<(), MailboxError> {
        if !matches!(
            (self.status, next),
            (MailboxJobStatus::Pending, MailboxJobStatus::Running)
                | (
                    MailboxJobStatus::Running,
                    MailboxJobStatus::Succeeded
                        | MailboxJobStatus::RetryPending
                        | MailboxJobStatus::Failed
                )
                | (MailboxJobStatus::RetryPending, MailboxJobStatus::Running)
        ) {
            return Err(MailboxError::InvalidJobTransition);
        }
        self.status = next;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxError {
    AlreadyRevoked,
    BindingRevoked,
    CursorTooLong,
    InvalidJobTransition,
}

impl fmt::Display for MailboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyRevoked => "mailbox binding already revoked",
            Self::BindingRevoked => "mailbox binding is revoked",
            Self::CursorTooLong => "mailbox cursor exceeds bounded length",
            Self::InvalidJobTransition => "mailbox job transition is invalid",
        })
    }
}

impl std::error::Error for MailboxError {}

#[cfg(test)]
mod tests {
    use super::{
        MailboxBinding, MailboxBindingStatus, MailboxError, MailboxJob, MailboxJobStatus,
        MailboxProvider,
    };
    use profile_platform_primitives::{MailboxBindingId, SecretHandle, TenantId};

    fn binding() -> Result<MailboxBinding, Box<dyn std::error::Error>> {
        Ok(MailboxBinding::create(
            TenantId::parse("tenant_01JMAILBOX")?,
            MailboxBindingId::parse("mailbox_01JMAILBOX")?,
            MailboxProvider::Imap,
            SecretHandle::parse("secret_01JMAILBOX")?,
        ))
    }

    #[test]
    fn binding_contains_only_secret_handle() -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        assert_eq!(binding.secret_handle().as_str(), "secret_01JMAILBOX");
        assert_eq!(binding.status(), MailboxBindingStatus::Active);
        Ok(())
    }

    #[test]
    fn revoked_binding_cannot_start_job() -> Result<(), Box<dyn std::error::Error>> {
        let mut binding = binding()?;
        binding.revoke()?;
        assert_eq!(MailboxJob::create(&binding, None), Err(MailboxError::BindingRevoked));
        Ok(())
    }

    #[test]
    fn retry_path_is_explicit() -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let mut job = MailboxJob::create(&binding, Some("cursor-1".to_owned()))?;
        job.transition(MailboxJobStatus::Running)?;
        job.transition(MailboxJobStatus::RetryPending)?;
        job.transition(MailboxJobStatus::Running)?;
        assert_eq!(job.status(), MailboxJobStatus::Running);
        Ok(())
    }
}
