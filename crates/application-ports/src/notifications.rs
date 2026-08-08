#![allow(async_fn_in_trait)]

// Notification ports are provider-neutral contracts. Implementations may be native or Workers/WASM;
// requiring `Send` futures here would over-constrain the outer adapter boundary.

use core::fmt;
use notification_domain::{DeliveryState, NotificationCursor};
use profile_platform_primitives::{
    ActorContext, ActorId, AuditEventId, OpaqueId, OutboxEventId, TenantId, TenantScope, UnixMillis,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationPortErrorClass {
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationPortError {
    class: NotificationPortErrorClass,
}

impl NotificationPortError {
    #[must_use]
    pub const fn new(class: NotificationPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> NotificationPortErrorClass {
        self.class
    }
}

impl fmt::Display for NotificationPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            NotificationPortErrorClass::Conflict => "notification port conflict",
            NotificationPortErrorClass::IntegrityFailure => "notification port integrity failure",
            NotificationPortErrorClass::InternalFailure => "notification port internal failure",
            NotificationPortErrorClass::DependencyUnavailable => {
                "notification dependency unavailable"
            }
        })
    }
}

impl std::error::Error for NotificationPortError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationCapability {
    CatchUp,
    Remediate,
    ObserveOperations,
}

pub trait NotificationAuthorizationPort {
    /// Resolves current membership/role state. Authorization must never be inferred from stale
    /// notification state or historical grants.
    async fn is_authorized(
        &self,
        actor: &ActorContext,
        capability: NotificationCapability,
    ) -> Result<bool, NotificationPortError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryTransitionWriteOutcome {
    Applied,
    Stale,
}

pub trait NotificationDeliveryRepositoryPort {
    async fn load_or_create_delivery(
        &self,
        tenant_id: &TenantId,
        consumer_id: &OpaqueId,
        event_id: &OutboxEventId,
        created_at: UnixMillis,
    ) -> Result<DeliveryState, NotificationPortError>;

    async fn compare_and_swap_delivery(
        &self,
        tenant_id: &TenantId,
        consumer_id: &OpaqueId,
        event_id: &OutboxEventId,
        expected: DeliveryState,
        next: DeliveryState,
    ) -> Result<DeliveryTransitionWriteOutcome, NotificationPortError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorAdvanceWriteOutcome {
    Advanced,
    Unchanged,
    Stale,
}

pub trait NotificationCursorRepositoryPort {
    async fn load_user_cursor(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
    ) -> Result<Option<NotificationCursor>, NotificationPortError>;

    async fn compare_and_advance_user_cursor(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        expected: Option<&NotificationCursor>,
        next: &NotificationCursor,
        advanced_at: UnixMillis,
    ) -> Result<CursorAdvanceWriteOutcome, NotificationPortError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationEventRecord {
    event_id: OutboxEventId,
    aggregate_type: String,
    aggregate_id: OpaqueId,
    event_type: String,
    occurred_at: UnixMillis,
}

impl NotificationEventRecord {
    #[must_use]
    pub fn new(
        event_id: OutboxEventId,
        aggregate_type: impl Into<String>,
        aggregate_id: OpaqueId,
        event_type: impl Into<String>,
        occurred_at: UnixMillis,
    ) -> Self {
        Self {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            event_type: event_type.into(),
            occurred_at,
        }
    }

    #[must_use]
    pub const fn event_id(&self) -> &OutboxEventId {
        &self.event_id
    }

    #[must_use]
    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    #[must_use]
    pub const fn aggregate_id(&self) -> &OpaqueId {
        &self.aggregate_id
    }

    #[must_use]
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    #[must_use]
    pub const fn occurred_at(&self) -> UnixMillis {
        self.occurred_at
    }

    #[must_use]
    pub fn cursor(&self) -> NotificationCursor {
        NotificationCursor::new(self.occurred_at, self.event_id.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationEventPage {
    events: Vec<NotificationEventRecord>,
}

impl NotificationEventPage {
    #[must_use]
    pub fn new(events: Vec<NotificationEventRecord>) -> Self {
        Self { events }
    }

    #[must_use]
    pub fn events(&self) -> &[NotificationEventRecord] {
        &self.events
    }

    #[must_use]
    pub fn into_events(self) -> Vec<NotificationEventRecord> {
        self.events
    }
}

pub trait NotificationCatchUpRepositoryPort {
    /// Loads only events eligible for the authenticated actor under current tenant membership/grants.
    /// Implementations must apply live authorization predicates before materializing the event page.
    async fn load_authorized_event_page(
        &self,
        actor: &ActorContext,
        after: Option<&NotificationCursor>,
        limit: u32,
    ) -> Result<NotificationEventPage, NotificationPortError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayReasonClass {
    DependencyRecovered,
    OperatorRemediation,
    IntegrityRevalidated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationReplayIntent {
    replay_id: OpaqueId,
    consumer_id: OpaqueId,
    event_id: OutboxEventId,
    audit_event_id: AuditEventId,
    reason_class: ReplayReasonClass,
    requested_at: UnixMillis,
}

impl NotificationReplayIntent {
    #[must_use]
    pub const fn new(
        replay_id: OpaqueId,
        consumer_id: OpaqueId,
        event_id: OutboxEventId,
        audit_event_id: AuditEventId,
        reason_class: ReplayReasonClass,
        requested_at: UnixMillis,
    ) -> Self {
        Self {
            replay_id,
            consumer_id,
            event_id,
            audit_event_id,
            reason_class,
            requested_at,
        }
    }

    #[must_use]
    pub const fn replay_id(&self) -> &OpaqueId {
        &self.replay_id
    }

    #[must_use]
    pub const fn consumer_id(&self) -> &OpaqueId {
        &self.consumer_id
    }

    #[must_use]
    pub const fn event_id(&self) -> &OutboxEventId {
        &self.event_id
    }

    #[must_use]
    pub const fn audit_event_id(&self) -> &AuditEventId {
        &self.audit_event_id
    }

    #[must_use]
    pub const fn reason_class(&self) -> ReplayReasonClass {
        self.reason_class
    }

    #[must_use]
    pub const fn requested_at(&self) -> UnixMillis {
        self.requested_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayPreparationOutcome {
    Prepared,
    Duplicate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingNotificationReplay {
    replay_id: OpaqueId,
    tenant_id: TenantId,
    event_id: OutboxEventId,
}

impl PendingNotificationReplay {
    #[must_use]
    pub const fn new(replay_id: OpaqueId, tenant_id: TenantId, event_id: OutboxEventId) -> Self {
        Self {
            replay_id,
            tenant_id,
            event_id,
        }
    }

    #[must_use]
    pub const fn replay_id(&self) -> &OpaqueId {
        &self.replay_id
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn event_id(&self) -> &OutboxEventId {
        &self.event_id
    }
}

pub trait NotificationReplayRepositoryPort {
    /// Atomically records immutable audit/replay evidence, creates a pending replay dispatch and
    /// reopens exactly one matching dead-letter delivery. Duplicate replay IDs are neutral.
    async fn prepare_replay(
        &self,
        actor: &ActorContext,
        intent: &NotificationReplayIntent,
    ) -> Result<ReplayPreparationOutcome, NotificationPortError>;

    async fn load_pending_replays(
        &self,
        limit: u32,
    ) -> Result<Vec<PendingNotificationReplay>, NotificationPortError>;

    async fn mark_replay_published(
        &self,
        tenant_id: &TenantId,
        replay_id: &OpaqueId,
        published_at: UnixMillis,
    ) -> Result<(), NotificationPortError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationRetentionOutcome {
    deliveries_removed: u32,
    cursors_removed: u32,
    replay_dispatches_removed: u32,
}

impl NotificationRetentionOutcome {
    #[must_use]
    pub const fn new(
        deliveries_removed: u32,
        cursors_removed: u32,
        replay_dispatches_removed: u32,
    ) -> Self {
        Self {
            deliveries_removed,
            cursors_removed,
            replay_dispatches_removed,
        }
    }

    #[must_use]
    pub const fn deliveries_removed(self) -> u32 {
        self.deliveries_removed
    }

    #[must_use]
    pub const fn cursors_removed(self) -> u32 {
        self.cursors_removed
    }

    #[must_use]
    pub const fn replay_dispatches_removed(self) -> u32 {
        self.replay_dispatches_removed
    }
}

pub trait NotificationRetentionRepositoryPort {
    /// Removes only bounded operational state. Canonical outbox/business state, immutable replay
    /// intent and audit evidence are outside this delete surface.
    async fn compact_operational_state(
        &self,
        before: UnixMillis,
        limit: u32,
    ) -> Result<NotificationRetentionOutcome, NotificationPortError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationOperationsSnapshot {
    ready_count: u64,
    retry_scheduled_count: u64,
    delivered_count: u64,
    dead_letter_count: u64,
    pending_replay_count: u64,
    max_attempt_count: u16,
    oldest_open_created_at: Option<UnixMillis>,
    catch_up_lag_count: u64,
}

impl NotificationOperationsSnapshot {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        ready_count: u64,
        retry_scheduled_count: u64,
        delivered_count: u64,
        dead_letter_count: u64,
        pending_replay_count: u64,
        max_attempt_count: u16,
        oldest_open_created_at: Option<UnixMillis>,
        catch_up_lag_count: u64,
    ) -> Self {
        Self {
            ready_count,
            retry_scheduled_count,
            delivered_count,
            dead_letter_count,
            pending_replay_count,
            max_attempt_count,
            oldest_open_created_at,
            catch_up_lag_count,
        }
    }

    #[must_use]
    pub const fn ready_count(self) -> u64 {
        self.ready_count
    }

    #[must_use]
    pub const fn retry_scheduled_count(self) -> u64 {
        self.retry_scheduled_count
    }

    #[must_use]
    pub const fn delivered_count(self) -> u64 {
        self.delivered_count
    }

    #[must_use]
    pub const fn dead_letter_count(self) -> u64 {
        self.dead_letter_count
    }

    #[must_use]
    pub const fn pending_replay_count(self) -> u64 {
        self.pending_replay_count
    }

    #[must_use]
    pub const fn max_attempt_count(self) -> u16 {
        self.max_attempt_count
    }

    #[must_use]
    pub const fn oldest_open_created_at(self) -> Option<UnixMillis> {
        self.oldest_open_created_at
    }

    #[must_use]
    pub const fn catch_up_lag_count(self) -> u64 {
        self.catch_up_lag_count
    }
}

pub trait NotificationOperationsRepositoryPort {
    /// Returns sanitizer-safe, low-cardinality operational aggregates only. Implementations must
    /// not return event IDs, payloads, credentials, mailbox content or raw errors.
    async fn load_operations_snapshot(
        &self,
        actor: &ActorContext,
    ) -> Result<NotificationOperationsSnapshot, NotificationPortError>;
}
