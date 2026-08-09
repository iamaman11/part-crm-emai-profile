use crate::commands::CommandExecutionEvidence;
use crate::mailboxes::{MailboxReplayDecision, MailboxReplayReceipt};
use core::fmt;
pub use mailbox_domain::{MailboxBinding, MailboxJob, MailboxJobStatus};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, MailboxJobId, TenantScope, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobCreateWrite {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    cursor: Option<String>,
    scheduled_at: UnixMillis,
    max_attempts: u32,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl MailboxJobCreateWrite {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        cursor: Option<String>,
        scheduled_at: UnixMillis,
        max_attempts: u32,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            cursor,
            scheduled_at,
            max_attempts,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
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
    pub const fn scheduled_at(&self) -> UnixMillis {
        self.scheduled_at
    }

    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobPreparedRun {
    status: MailboxJobStatus,
    attempt: u32,
    version: AggregateVersion,
    cursor: Option<String>,
    provider_status: String,
    bounded_item_count: u32,
    retry_at: Option<UnixMillis>,
}

impl MailboxJobPreparedRun {
    #[must_use]
    pub fn from_job(
        job: &MailboxJob,
        provider_status: impl Into<String>,
        bounded_item_count: u32,
    ) -> Self {
        Self {
            status: job.status(),
            attempt: job.attempt(),
            version: job.version(),
            cursor: job.cursor().map(str::to_owned),
            provider_status: provider_status.into(),
            bounded_item_count,
            retry_at: (job.status() == MailboxJobStatus::RetryPending).then_some(job.next_run_at()),
        }
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
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    #[must_use]
    pub fn provider_status(&self) -> &str {
        &self.provider_status
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }

    #[must_use]
    pub const fn retry_at(&self) -> Option<UnixMillis> {
        self.retry_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobRunWrite {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    expected_version: AggregateVersion,
    prepared: MailboxJobPreparedRun,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl MailboxJobRunWrite {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        expected_version: AggregateVersion,
        prepared: MailboxJobPreparedRun,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            expected_version,
            prepared,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
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
    pub const fn expected_version(&self) -> AggregateVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn prepared(&self) -> &MailboxJobPreparedRun {
        &self.prepared
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobReadModel {
    job: MailboxJob,
    provider_status: Option<String>,
    bounded_item_count: u32,
}

impl MailboxJobReadModel {
    #[must_use]
    pub fn new(job: MailboxJob, provider_status: Option<String>, bounded_item_count: u32) -> Self {
        Self {
            job,
            provider_status,
            bounded_item_count,
        }
    }

    #[must_use]
    pub const fn job(&self) -> &MailboxJob {
        &self.job
    }

    #[must_use]
    pub fn provider_status(&self) -> Option<&str> {
        self.provider_status.as_deref()
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxJobPortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxJobPortError {
    class: MailboxJobPortErrorClass,
}

impl MailboxJobPortError {
    #[must_use]
    pub const fn new(class: MailboxJobPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> MailboxJobPortErrorClass {
        self.class
    }
}

impl fmt::Display for MailboxJobPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            MailboxJobPortErrorClass::NotFound => "mailbox job not found",
            MailboxJobPortErrorClass::VersionConflict => "mailbox job version conflict",
            MailboxJobPortErrorClass::InvalidState => "mailbox job invalid state",
            MailboxJobPortErrorClass::Conflict => "mailbox job conflict",
            MailboxJobPortErrorClass::IntegrityFailure => "mailbox job integrity failure",
            MailboxJobPortErrorClass::InternalFailure => "mailbox job internal failure",
            MailboxJobPortErrorClass::DependencyUnavailable => "mailbox job dependency unavailable",
        })
    }
}

impl std::error::Error for MailboxJobPortError {}

#[allow(async_fn_in_trait)]
pub trait MailboxJobApplicationPort {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxReplayDecision, MailboxJobPortError>;

    async fn create_job(
        &self,
        actor: &ActorContext,
        write: &MailboxJobCreateWrite,
    ) -> Result<(), MailboxJobPortError>;

    async fn run_job(
        &self,
        actor: &ActorContext,
        write: &MailboxJobRunWrite,
    ) -> Result<(), MailboxJobPortError>;

    async fn find_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<MailboxBinding>, MailboxJobPortError>;

    async fn find_job(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        job_id: &MailboxJobId,
    ) -> Result<Option<MailboxJobReadModel>, MailboxJobPortError>;
}

#[must_use]
pub fn replay_receipt(
    result_code: impl Into<String>,
    result_reference: Option<String>,
) -> MailboxReplayDecision {
    MailboxReplayDecision::Replay(MailboxReplayReceipt::new(result_code, result_reference))
}
