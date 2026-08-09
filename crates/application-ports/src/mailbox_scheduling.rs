use crate::CommandExecutionEvidence;
use crate::mailbox_jobs::{MailboxJobPortError, MailboxJobPreparedRun};
use mailbox_domain::{MailboxBinding, MailboxJob};
use profile_platform_primitives::{
    ActorId, AggregateVersion, MailboxBindingId, MailboxJobId, TenantId, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobDispatch {
    tenant_id: TenantId,
    actor_id: ActorId,
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    expected_version: AggregateVersion,
    due_at: UnixMillis,
}

impl MailboxJobDispatch {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        actor_id: ActorId,
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        expected_version: AggregateVersion,
        due_at: UnixMillis,
    ) -> Self {
        Self {
            tenant_id,
            actor_id,
            binding_id,
            job_id,
            expected_version,
            due_at,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
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
    pub const fn due_at(&self) -> UnixMillis {
        self.due_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxExecutionClaimWrite {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    expected_version: AggregateVersion,
    running_version: AggregateVersion,
    running_attempt: u32,
    claimed_at: UnixMillis,
    lease_expires_at: UnixMillis,
}

impl MailboxExecutionClaimWrite {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        expected_version: AggregateVersion,
        running_version: AggregateVersion,
        running_attempt: u32,
        claimed_at: UnixMillis,
        lease_expires_at: UnixMillis,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            expected_version,
            running_version,
            running_attempt,
            claimed_at,
            lease_expires_at,
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
    pub const fn running_version(&self) -> AggregateVersion {
        self.running_version
    }

    #[must_use]
    pub const fn running_attempt(&self) -> u32 {
        self.running_attempt
    }

    #[must_use]
    pub const fn claimed_at(&self) -> UnixMillis {
        self.claimed_at
    }

    #[must_use]
    pub const fn lease_expires_at(&self) -> UnixMillis {
        self.lease_expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxExecutionLease {
    binding: MailboxBinding,
    running_job: MailboxJob,
    base_version: AggregateVersion,
    fence: u64,
    lease_expires_at: UnixMillis,
}

impl MailboxExecutionLease {
    #[must_use]
    pub const fn new(
        binding: MailboxBinding,
        running_job: MailboxJob,
        base_version: AggregateVersion,
        fence: u64,
        lease_expires_at: UnixMillis,
    ) -> Self {
        Self {
            binding,
            running_job,
            base_version,
            fence,
            lease_expires_at,
        }
    }

    #[must_use]
    pub const fn binding(&self) -> &MailboxBinding {
        &self.binding
    }

    #[must_use]
    pub const fn running_job(&self) -> &MailboxJob {
        &self.running_job
    }

    #[must_use]
    pub const fn base_version(&self) -> AggregateVersion {
        self.base_version
    }

    #[must_use]
    pub const fn fence(&self) -> u64 {
        self.fence
    }

    #[must_use]
    pub const fn lease_expires_at(&self) -> UnixMillis {
        self.lease_expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MailboxExecutionClaimOutcome {
    Acquired(MailboxExecutionLease),
    InFlight { retry_at: UnixMillis },
    Completed,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxExecutionCompletionWrite {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    base_version: AggregateVersion,
    expected_running_version: AggregateVersion,
    fence: u64,
    prepared: MailboxJobPreparedRun,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl MailboxExecutionCompletionWrite {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        base_version: AggregateVersion,
        expected_running_version: AggregateVersion,
        fence: u64,
        prepared: MailboxJobPreparedRun,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            base_version,
            expected_running_version,
            fence,
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
    pub const fn base_version(&self) -> AggregateVersion {
        self.base_version
    }

    #[must_use]
    pub const fn expected_running_version(&self) -> AggregateVersion {
        self.expected_running_version
    }

    #[must_use]
    pub const fn fence(&self) -> u64 {
        self.fence
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

#[allow(async_fn_in_trait)]
pub trait MailboxSchedulingRepositoryPort {
    async fn load_due_dispatches(
        &self,
        now: UnixMillis,
        limit: u32,
    ) -> Result<Vec<MailboxJobDispatch>, MailboxJobPortError>;

    async fn mark_dispatched(
        &self,
        dispatch: &MailboxJobDispatch,
        published_at: UnixMillis,
    ) -> Result<(), MailboxJobPortError>;

    async fn acquire_execution(
        &self,
        dispatch: &MailboxJobDispatch,
        write: &MailboxExecutionClaimWrite,
    ) -> Result<MailboxExecutionClaimOutcome, MailboxJobPortError>;

    async fn complete_execution(
        &self,
        dispatch: &MailboxJobDispatch,
        write: &MailboxExecutionCompletionWrite,
    ) -> Result<(), MailboxJobPortError>;
}

#[allow(async_fn_in_trait)]
pub trait MailboxDispatchPublisherPort {
    async fn publish(&self, dispatch: &MailboxJobDispatch) -> Result<(), MailboxJobPortError>;
}
