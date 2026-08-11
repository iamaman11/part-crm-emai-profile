use crate::ClockPort;
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, FencingToken, LaunchIntentId, OutboxEventId,
    ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
};
use session_domain::coordinator::{CoordinatorCommandEnvelope, CoordinatorStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinatorIngressPortErrorClass {
    NotFound,
    InvalidRequest,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoordinatorIngressPortError {
    class: CoordinatorIngressPortErrorClass,
}

impl CoordinatorIngressPortError {
    #[must_use]
    pub const fn new(class: CoordinatorIngressPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> CoordinatorIngressPortErrorClass {
        self.class
    }
}

impl fmt::Display for CoordinatorIngressPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            CoordinatorIngressPortErrorClass::NotFound => "coordinator resource not found",
            CoordinatorIngressPortErrorClass::InvalidRequest => "coordinator request is invalid",
            CoordinatorIngressPortErrorClass::Conflict => "coordinator conflict",
            CoordinatorIngressPortErrorClass::IntegrityFailure => "coordinator integrity failure",
            CoordinatorIngressPortErrorClass::InternalFailure => "coordinator internal failure",
            CoordinatorIngressPortErrorClass::DependencyUnavailable => {
                "coordinator dependency unavailable"
            }
        })
    }
}

impl std::error::Error for CoordinatorIngressPortError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorProfileAccess {
    status: String,
    has_active_generation: bool,
}

impl CoordinatorProfileAccess {
    #[must_use]
    pub fn new(status: impl Into<String>, has_active_generation: bool) -> Self {
        Self {
            status: status.into(),
            has_active_generation,
        }
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    pub const fn has_active_generation(&self) -> bool {
        self.has_active_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinatorRuntimeOutcome {
    Snapshot,
    LaunchIntentIssued,
    LeaseClaimed,
    HeartbeatAccepted,
    Released,
    DrainStarted,
    TimedOut,
    LaunchIntentExpired,
    Recovered,
    NoChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorProjectionSnapshot {
    tenant_id: TenantId,
    profile_id: ProfileId,
    status: CoordinatorStatus,
    version: AggregateVersion,
    sequence: u64,
    next_epoch: u64,
    active_session_id: Option<SessionId>,
    active_device_id: Option<DeviceId>,
    active_epoch: Option<u64>,
    idle_expires_at: Option<UnixMillis>,
    hard_expires_at: Option<UnixMillis>,
    drain_deadline: Option<UnixMillis>,
    pending_launch_intent_id: Option<LaunchIntentId>,
    pending_intent_expires_at: Option<UnixMillis>,
}

impl CoordinatorProjectionSnapshot {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        profile_id: ProfileId,
        status: CoordinatorStatus,
        version: AggregateVersion,
        sequence: u64,
        next_epoch: u64,
        active_session_id: Option<SessionId>,
        active_device_id: Option<DeviceId>,
        active_epoch: Option<u64>,
        idle_expires_at: Option<UnixMillis>,
        hard_expires_at: Option<UnixMillis>,
        drain_deadline: Option<UnixMillis>,
        pending_launch_intent_id: Option<LaunchIntentId>,
        pending_intent_expires_at: Option<UnixMillis>,
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            status,
            version,
            sequence,
            next_epoch,
            active_session_id,
            active_device_id,
            active_epoch,
            idle_expires_at,
            hard_expires_at,
            drain_deadline,
            pending_launch_intent_id,
            pending_intent_expires_at,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn status(&self) -> CoordinatorStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn next_epoch(&self) -> u64 {
        self.next_epoch
    }

    #[must_use]
    pub const fn active_session_id(&self) -> Option<&SessionId> {
        self.active_session_id.as_ref()
    }

    #[must_use]
    pub const fn active_device_id(&self) -> Option<&DeviceId> {
        self.active_device_id.as_ref()
    }

    #[must_use]
    pub const fn active_epoch(&self) -> Option<u64> {
        self.active_epoch
    }

    #[must_use]
    pub const fn idle_expires_at(&self) -> Option<UnixMillis> {
        self.idle_expires_at
    }

    #[must_use]
    pub const fn hard_expires_at(&self) -> Option<UnixMillis> {
        self.hard_expires_at
    }

    #[must_use]
    pub const fn drain_deadline(&self) -> Option<UnixMillis> {
        self.drain_deadline
    }

    #[must_use]
    pub const fn pending_launch_intent_id(&self) -> Option<&LaunchIntentId> {
        self.pending_launch_intent_id.as_ref()
    }

    #[must_use]
    pub const fn pending_intent_expires_at(&self) -> Option<UnixMillis> {
        self.pending_intent_expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorRuntimeResult {
    outcome: CoordinatorRuntimeOutcome,
    version: AggregateVersion,
    sequence: u64,
    replayed: bool,
    fencing_token: Option<FencingToken>,
    epoch: Option<u64>,
    projection: CoordinatorProjectionSnapshot,
}

impl CoordinatorRuntimeResult {
    #[must_use]
    pub const fn new(
        outcome: CoordinatorRuntimeOutcome,
        version: AggregateVersion,
        sequence: u64,
        replayed: bool,
        fencing_token: Option<FencingToken>,
        epoch: Option<u64>,
        projection: CoordinatorProjectionSnapshot,
    ) -> Self {
        Self {
            outcome,
            version,
            sequence,
            replayed,
            fencing_token,
            epoch,
            projection,
        }
    }

    #[must_use]
    pub const fn outcome(&self) -> CoordinatorRuntimeOutcome {
        self.outcome
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }

    #[must_use]
    pub const fn fencing_token(&self) -> Option<&FencingToken> {
        self.fencing_token.as_ref()
    }

    #[must_use]
    pub const fn epoch(&self) -> Option<u64> {
        self.epoch
    }

    #[must_use]
    pub const fn projection(&self) -> &CoordinatorProjectionSnapshot {
        &self.projection
    }
}

#[allow(async_fn_in_trait)]
pub trait CoordinatorIngressApplicationPort {
    async fn find_visible_profile(
        &self,
        actor: &ActorContext,
        role: MembershipRole,
        profile_id: &ProfileId,
    ) -> Result<Option<CoordinatorProfileAccess>, CoordinatorIngressPortError>;

    fn new_fencing_token(&self) -> Result<FencingToken, CoordinatorIngressPortError>;

    fn new_outbox_event_id(&self) -> Result<OutboxEventId, CoordinatorIngressPortError>;

    async fn snapshot(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError>;

    async fn execute(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        envelope: &CoordinatorCommandEnvelope,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError>;

    async fn project(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        result: &CoordinatorRuntimeResult,
        outbox_event_id: &OutboxEventId,
        projected_at: UnixMillis,
    ) -> Result<(), CoordinatorIngressPortError>;
}

pub trait CoordinatorIngressClockPort: ClockPort {}

impl<T: ClockPort> CoordinatorIngressClockPort for T {}
