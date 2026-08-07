#![forbid(unsafe_code)]

use core::fmt;
use profile_platform_primitives::{
    AggregateVersion, MailboxBindingId, MailboxJobId, SecretHandle, TenantId, UnixMillis,
};

const MAX_CURSOR_LENGTH: usize = 512;
const MAX_PROVIDER_STATUS_LENGTH: usize = 64;
const MAX_JOB_ATTEMPTS: u32 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxProvider {
    GmailApi,
    Imap,
    BrowserFallback,
}

impl MailboxProvider {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::GmailApi => "GMAIL_API",
            Self::Imap => "IMAP",
            Self::BrowserFallback => "BROWSER_FALLBACK",
        }
    }

    pub fn parse_storage(value: &str) -> Result<Self, MailboxError> {
        match value {
            "GMAIL_API" => Ok(Self::GmailApi),
            "IMAP" => Ok(Self::Imap),
            "BROWSER_FALLBACK" => Ok(Self::BrowserFallback),
            _ => Err(MailboxError::InvalidProvider),
        }
    }
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxJobStatus {
    Pending,
    Running,
    RetryPending,
    Succeeded,
    Failed,
}

impl MailboxJobStatus {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::RetryPending => "RETRY_PENDING",
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
        }
    }

    pub fn parse_storage(value: &str) -> Result<Self, MailboxError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "RUNNING" => Ok(Self::Running),
            "RETRY_PENDING" => Ok(Self::RetryPending),
            "SUCCEEDED" => Ok(Self::Succeeded),
            "FAILED" => Ok(Self::Failed),
            _ => Err(MailboxError::InvalidJobStatus),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJob {
    tenant_id: TenantId,
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    cursor: Option<String>,
    status: MailboxJobStatus,
    attempt: u32,
    max_attempts: u32,
    next_run_at: UnixMillis,
    version: AggregateVersion,
}

impl MailboxJob {
    pub fn create(
        binding: &MailboxBinding,
        job_id: MailboxJobId,
        cursor: Option<String>,
        scheduled_at: UnixMillis,
        max_attempts: u32,
    ) -> Result<Self, MailboxError> {
        if binding.status() != MailboxBindingStatus::Active {
            return Err(MailboxError::BindingRevoked);
        }
        validate_cursor(cursor.as_deref())?;
        validate_max_attempts(max_attempts)?;
        Ok(Self {
            tenant_id: binding.tenant_id().clone(),
            binding_id: binding.binding_id().clone(),
            job_id,
            cursor,
            status: MailboxJobStatus::Pending,
            attempt: 0,
            max_attempts,
            next_run_at: scheduled_at,
            version: AggregateVersion::INITIAL,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        tenant_id: TenantId,
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        cursor: Option<String>,
        status: MailboxJobStatus,
        attempt: u32,
        max_attempts: u32,
        next_run_at: UnixMillis,
        version: AggregateVersion,
    ) -> Result<Self, MailboxError> {
        validate_cursor(cursor.as_deref())?;
        validate_max_attempts(max_attempts)?;
        if attempt > max_attempts {
            return Err(MailboxError::MaxAttemptsReached);
        }
        Ok(Self {
            tenant_id,
            binding_id,
            job_id,
            cursor,
            status,
            attempt,
            max_attempts,
            next_run_at,
            version,
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
    pub const fn job_id(&self) -> &MailboxJobId {
        &self.job_id
    }

    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    #[must_use]
    pub const fn status(&self) -> MailboxJobStatus {
        self.status
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    #[must_use]
    pub const fn next_run_at(&self) -> UnixMillis {
        self.next_run_at
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub fn is_due(&self, now: UnixMillis) -> bool {
        matches!(
            self.status,
            MailboxJobStatus::Pending | MailboxJobStatus::RetryPending
        ) && now >= self.next_run_at
    }

    pub fn start(&mut self, now: UnixMillis) -> Result<(), MailboxError> {
        if !matches!(
            self.status,
            MailboxJobStatus::Pending | MailboxJobStatus::RetryPending
        ) {
            return Err(MailboxError::InvalidJobTransition);
        }
        if now < self.next_run_at {
            return Err(MailboxError::JobNotDue);
        }
        if self.attempt >= self.max_attempts {
            return Err(MailboxError::MaxAttemptsReached);
        }
        self.attempt = self
            .attempt
            .checked_add(1)
            .ok_or(MailboxError::MaxAttemptsReached)?;
        self.bump_version()?;
        self.status = MailboxJobStatus::Running;
        Ok(())
    }

    pub fn succeed(&mut self, next_cursor: Option<String>) -> Result<(), MailboxError> {
        if self.status != MailboxJobStatus::Running {
            return Err(MailboxError::InvalidJobTransition);
        }
        validate_cursor(next_cursor.as_deref())?;
        self.bump_version()?;
        self.cursor = next_cursor;
        self.status = MailboxJobStatus::Succeeded;
        Ok(())
    }

    pub fn retry(&mut self, now: UnixMillis, retry_at: UnixMillis) -> Result<(), MailboxError> {
        if self.status != MailboxJobStatus::Running {
            return Err(MailboxError::InvalidJobTransition);
        }
        if self.attempt >= self.max_attempts {
            return Err(MailboxError::MaxAttemptsReached);
        }
        if retry_at <= now {
            return Err(MailboxError::InvalidRetryTime);
        }
        self.bump_version()?;
        self.next_run_at = retry_at;
        self.status = MailboxJobStatus::RetryPending;
        Ok(())
    }

    pub fn fail(&mut self) -> Result<(), MailboxError> {
        if self.status != MailboxJobStatus::Running {
            return Err(MailboxError::InvalidJobTransition);
        }
        self.bump_version()?;
        self.status = MailboxJobStatus::Failed;
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

pub fn validate_provider_status(value: &str) -> Result<(), MailboxError> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_STATUS_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(MailboxError::InvalidProviderStatus);
    }
    Ok(())
}

pub fn validate_cursor(value: Option<&str>) -> Result<(), MailboxError> {
    if value.is_some_and(|cursor| cursor.len() > MAX_CURSOR_LENGTH) {
        return Err(MailboxError::CursorTooLong);
    }
    Ok(())
}

fn validate_max_attempts(value: u32) -> Result<(), MailboxError> {
    if value == 0 || value > MAX_JOB_ATTEMPTS {
        return Err(MailboxError::InvalidMaxAttempts);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxError {
    AlreadyRevoked,
    BindingRevoked,
    CursorTooLong,
    InvalidJobTransition,
    InvalidProvider,
    InvalidJobStatus,
    InvalidProviderStatus,
    InvalidMaxAttempts,
    JobNotDue,
    MaxAttemptsReached,
    InvalidRetryTime,
    VersionOverflow,
}

impl fmt::Display for MailboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyRevoked => "mailbox binding already revoked",
            Self::BindingRevoked => "mailbox binding is revoked",
            Self::CursorTooLong => "mailbox cursor exceeds bounded length",
            Self::InvalidJobTransition => "mailbox job transition is invalid",
            Self::InvalidProvider => "mailbox provider is invalid",
            Self::InvalidJobStatus => "mailbox job status is invalid",
            Self::InvalidProviderStatus => "mailbox provider status is invalid",
            Self::InvalidMaxAttempts => "mailbox job max attempts are invalid",
            Self::JobNotDue => "mailbox job is not due",
            Self::MaxAttemptsReached => "mailbox job attempts are exhausted",
            Self::InvalidRetryTime => "mailbox retry time must be in the future",
            Self::VersionOverflow => "mailbox aggregate version overflow",
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

    fn job(binding: &MailboxBinding) -> Result<MailboxJob, Box<dyn std::error::Error>> {
        Ok(MailboxJob::create(
            binding,
            MailboxJobId::parse("mailjob_01JMAILBOX")?,
            Some("cursor-1".to_owned()),
            UnixMillis::new(10),
            3,
        )?)
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

    #[test]
    fn retry_path_is_due_bounded_and_versioned() -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let mut job = job(&binding)?;
        assert!(!job.is_due(UnixMillis::new(9)));
        job.start(UnixMillis::new(10))?;
        assert_eq!(job.status(), MailboxJobStatus::Running);
        assert_eq!(job.attempt(), 1);
        assert_eq!(job.version().value(), 2);
        job.retry(UnixMillis::new(10), UnixMillis::new(20))?;
        assert_eq!(job.status(), MailboxJobStatus::RetryPending);
        assert!(!job.is_due(UnixMillis::new(19)));
        assert!(job.is_due(UnixMillis::new(20)));
        job.start(UnixMillis::new(20))?;
        job.succeed(Some("cursor-2".to_owned()))?;
        assert_eq!(job.status(), MailboxJobStatus::Succeeded);
        assert_eq!(job.cursor(), Some("cursor-2"));
        assert_eq!(job.attempt(), 2);
        assert_eq!(job.version().value(), 5);
        Ok(())
    }

    #[test]
    fn retry_cannot_exceed_attempt_budget() -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let mut job = MailboxJob::create(
            &binding,
            MailboxJobId::parse("mailjob_01JEXHAUST")?,
            None,
            UnixMillis::new(1),
            1,
        )?;
        job.start(UnixMillis::new(1))?;
        assert_eq!(
            job.retry(UnixMillis::new(1), UnixMillis::new(2)),
            Err(MailboxError::MaxAttemptsReached)
        );
        job.fail()?;
        assert_eq!(job.status(), MailboxJobStatus::Failed);
        Ok(())
    }
}
