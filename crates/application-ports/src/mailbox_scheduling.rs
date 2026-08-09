use crate::mailbox_jobs::MailboxJobPortError;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxExecutionClaimWrite {
    claimed_at: UnixMillis,
    lease_expires_at: UnixMillis,
}

impl MailboxExecutionClaimWrite {
    #[must_use]
    pub const fn new(claimed_at: UnixMillis, lease_expires_at: UnixMillis) -> Self {
        Self {
            claimed_at,
            lease_expires_at,
        }
    }

    #[must_use]
    pub const fn claimed_at(self) -> UnixMillis {
        self.claimed_at
    }

    #[must_use]
    pub const fn lease_expires_at(self) -> UnixMillis {
        self.lease_expires_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxExecutionLease {
    fence: u64,
    lease_expires_at: UnixMillis,
}

impl MailboxExecutionLease {
    #[must_use]
    pub const fn new(fence: u64, lease_expires_at: UnixMillis) -> Self {
        Self {
            fence,
            lease_expires_at,
        }
    }

    #[must_use]
    pub const fn fence(self) -> u64 {
        self.fence
    }

    #[must_use]
    pub const fn lease_expires_at(self) -> UnixMillis {
        self.lease_expires_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxExecutionClaimOutcome {
    Acquired(MailboxExecutionLease),
    InFlight { retry_at: UnixMillis },
    Completed,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxExecutionCompletionOutcome {
    Applied,
    Stale,
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
        write: MailboxExecutionClaimWrite,
    ) -> Result<MailboxExecutionClaimOutcome, MailboxJobPortError>;

    async fn complete_execution(
        &self,
        dispatch: &MailboxJobDispatch,
        fence: u64,
        completed_at: UnixMillis,
    ) -> Result<MailboxExecutionCompletionOutcome, MailboxJobPortError>;
}

#[allow(async_fn_in_trait)]
pub trait MailboxDispatchPublisherPort {
    async fn publish(&self, dispatch: &MailboxJobDispatch) -> Result<(), MailboxJobPortError>;
}
