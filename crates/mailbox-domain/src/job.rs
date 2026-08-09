use crate::{MailboxBinding, MailboxBindingStatus, MailboxError};
use profile_platform_primitives::{
    AggregateVersion, MailboxBindingId, MailboxJobId, TenantId, UnixMillis,
};

const MAX_CURSOR_LENGTH: usize = 512;
const MAX_JOB_ATTEMPTS: u32 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxJobStatus {
    Scheduled,
    Queued,
    Running,
    RetryPending,
    AuthRequired,
    Suspended,
    Succeeded,
    Failed,
}

impl MailboxJobStatus {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Scheduled => "SCHEDULED",
            Self::Queued => "QUEUED",
            Self::Running => "RUNNING",
            Self::RetryPending => "RETRY_PENDING",
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::Suspended => "SUSPENDED",
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
        }
    }

    pub fn parse_storage(value: &str) -> Result<Self, MailboxError> {
        match value {
            "SCHEDULED" | "PENDING" => Ok(Self::Scheduled),
            "QUEUED" => Ok(Self::Queued),
            "RUNNING" => Ok(Self::Running),
            "RETRY_PENDING" => Ok(Self::RetryPending),
            "AUTH_REQUIRED" => Ok(Self::AuthRequired),
            "SUSPENDED" => Ok(Self::Suspended),
            "SUCCEEDED" => Ok(Self::Succeeded),
            "FAILED" => Ok(Self::Failed),
            _ => Err(MailboxError::InvalidJobStatus),
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobRestore {
    pub tenant_id: TenantId,
    pub binding_id: MailboxBindingId,
    pub job_id: MailboxJobId,
    pub cursor: Option<String>,
    pub status: MailboxJobStatus,
    pub attempt: u32,
    pub max_attempts: u32,
    pub next_run_at: UnixMillis,
    pub version: AggregateVersion,
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
        match binding.status() {
            MailboxBindingStatus::Active => {}
            MailboxBindingStatus::Revoked => return Err(MailboxError::BindingRevoked),
            MailboxBindingStatus::AuthRequired | MailboxBindingStatus::Suspended => {
                return Err(MailboxError::BindingNotExecutable);
            }
        }
        validate_cursor(cursor.as_deref())?;
        validate_max_attempts(max_attempts)?;
        Ok(Self {
            tenant_id: binding.tenant_id().clone(),
            binding_id: binding.binding_id().clone(),
            job_id,
            cursor,
            status: MailboxJobStatus::Scheduled,
            attempt: 0,
            max_attempts,
            next_run_at: scheduled_at,
            version: AggregateVersion::INITIAL,
        })
    }

    pub fn restore(snapshot: MailboxJobRestore) -> Result<Self, MailboxError> {
        validate_cursor(snapshot.cursor.as_deref())?;
        validate_max_attempts(snapshot.max_attempts)?;
        if snapshot.attempt > snapshot.max_attempts {
            return Err(MailboxError::MaxAttemptsReached);
        }
        Ok(Self {
            tenant_id: snapshot.tenant_id,
            binding_id: snapshot.binding_id,
            job_id: snapshot.job_id,
            cursor: snapshot.cursor,
            status: snapshot.status,
            attempt: snapshot.attempt,
            max_attempts: snapshot.max_attempts,
            next_run_at: snapshot.next_run_at,
            version: snapshot.version,
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
            MailboxJobStatus::Scheduled | MailboxJobStatus::RetryPending
        ) && now >= self.next_run_at
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn queue(&mut self, now: UnixMillis) -> Result<(), MailboxError> {
        if !matches!(
            self.status,
            MailboxJobStatus::Scheduled | MailboxJobStatus::RetryPending
        ) {
            return Err(MailboxError::InvalidJobTransition);
        }
        if now < self.next_run_at {
            return Err(MailboxError::JobNotDue);
        }
        if self.attempt >= self.max_attempts {
            return Err(MailboxError::MaxAttemptsReached);
        }
        self.bump_version()?;
        self.status = MailboxJobStatus::Queued;
        Ok(())
    }

    pub fn start(&mut self, binding: &MailboxBinding) -> Result<(), MailboxError> {
        if self.status != MailboxJobStatus::Queued {
            return Err(MailboxError::InvalidJobTransition);
        }
        if binding.tenant_id() != &self.tenant_id || binding.binding_id() != &self.binding_id {
            return Err(MailboxError::BindingNotExecutable);
        }
        match binding.status() {
            MailboxBindingStatus::Active => {}
            MailboxBindingStatus::Revoked => return Err(MailboxError::BindingRevoked),
            MailboxBindingStatus::AuthRequired | MailboxBindingStatus::Suspended => {
                return Err(MailboxError::BindingNotExecutable);
            }
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
        self.require_running()?;
        validate_cursor(next_cursor.as_deref())?;
        self.bump_version()?;
        self.cursor = next_cursor;
        self.status = MailboxJobStatus::Succeeded;
        Ok(())
    }

    pub fn retry(&mut self, now: UnixMillis, retry_at: UnixMillis) -> Result<(), MailboxError> {
        self.require_running()?;
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

    pub fn require_auth(&mut self) -> Result<(), MailboxError> {
        self.require_running()?;
        self.bump_version()?;
        self.status = MailboxJobStatus::AuthRequired;
        Ok(())
    }

    pub fn suspend(&mut self) -> Result<(), MailboxError> {
        if self.status.is_terminal() || self.status == MailboxJobStatus::Suspended {
            return Err(MailboxError::InvalidJobTransition);
        }
        self.bump_version()?;
        self.status = MailboxJobStatus::Suspended;
        Ok(())
    }

    pub fn resume(&mut self, scheduled_at: UnixMillis) -> Result<(), MailboxError> {
        if !matches!(
            self.status,
            MailboxJobStatus::AuthRequired | MailboxJobStatus::Suspended
        ) {
            return Err(MailboxError::InvalidJobTransition);
        }
        if self.attempt >= self.max_attempts {
            return Err(MailboxError::MaxAttemptsReached);
        }
        self.bump_version()?;
        self.next_run_at = scheduled_at;
        self.status = MailboxJobStatus::Scheduled;
        Ok(())
    }

    pub fn fail(&mut self) -> Result<(), MailboxError> {
        self.require_running()?;
        self.bump_version()?;
        self.status = MailboxJobStatus::Failed;
        Ok(())
    }

    fn require_running(&self) -> Result<(), MailboxError> {
        if self.status == MailboxJobStatus::Running {
            Ok(())
        } else {
            Err(MailboxError::InvalidJobTransition)
        }
    }

    fn bump_version(&mut self) -> Result<(), MailboxError> {
        self.version = self
            .version
            .next()
            .map_err(|_| MailboxError::VersionOverflow)?;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::{MailboxJob, MailboxJobStatus};
    use crate::{MailboxBinding, MailboxError, MailboxProvider};
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
    fn scheduled_queue_run_retry_path_is_explicit_and_versioned()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let mut job = job(&binding)?;
        assert_eq!(job.status(), MailboxJobStatus::Scheduled);
        assert!(!job.is_due(UnixMillis::new(9)));
        job.queue(UnixMillis::new(10))?;
        assert_eq!(job.status(), MailboxJobStatus::Queued);
        job.start(&binding)?;
        assert_eq!(job.status(), MailboxJobStatus::Running);
        assert_eq!(job.attempt(), 1);
        job.retry(UnixMillis::new(10), UnixMillis::new(20))?;
        assert_eq!(job.status(), MailboxJobStatus::RetryPending);
        assert!(!job.is_due(UnixMillis::new(19)));
        assert!(job.is_due(UnixMillis::new(20)));
        job.queue(UnixMillis::new(20))?;
        job.start(&binding)?;
        job.succeed(Some("cursor-2".to_owned()))?;
        assert_eq!(job.status(), MailboxJobStatus::Succeeded);
        assert_eq!(job.cursor(), Some("cursor-2"));
        assert_eq!(job.attempt(), 2);
        assert_eq!(job.version().value(), 7);
        assert!(job.is_terminal());
        Ok(())
    }

    #[test]
    fn auth_and_suspended_states_fail_closed_until_resumed()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let mut job = job(&binding)?;
        job.queue(UnixMillis::new(10))?;
        job.start(&binding)?;
        job.require_auth()?;
        assert_eq!(job.status(), MailboxJobStatus::AuthRequired);
        assert_eq!(job.queue(UnixMillis::new(10)), Err(MailboxError::InvalidJobTransition));
        job.resume(UnixMillis::new(30))?;
        assert_eq!(job.status(), MailboxJobStatus::Scheduled);
        assert!(!job.is_due(UnixMillis::new(29)));
        job.suspend()?;
        assert_eq!(job.status(), MailboxJobStatus::Suspended);
        Ok(())
    }

    #[test]
    fn revoked_binding_cannot_start_queued_job() -> Result<(), Box<dyn std::error::Error>> {
        let mut binding = binding()?;
        let mut job = job(&binding)?;
        job.queue(UnixMillis::new(10))?;
        binding.revoke()?;
        assert_eq!(job.start(&binding), Err(MailboxError::BindingRevoked));
        assert_eq!(job.status(), MailboxJobStatus::Queued);
        assert_eq!(job.attempt(), 0);
        Ok(())
    }
}
