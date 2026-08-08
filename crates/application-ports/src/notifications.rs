#![allow(async_fn_in_trait)]

// Notification ports are provider-neutral contracts. Implementations may be native or Workers/WASM;
// requiring `Send` futures here would over-constrain the outer adapter boundary.

use core::fmt;
use notification_domain::{DeliveryState, NotificationCursor};
use profile_platform_primitives::{
    ActorContext, ActorId, OpaqueId, OutboxEventId, TenantScope, UnixMillis,
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
pub enum DeliveryTransitionWriteOutcome {
    Applied,
    Stale,
}

pub trait NotificationDeliveryRepositoryPort {
    async fn load_or_create_delivery(
        &self,
        scope: &TenantScope,
        consumer_id: &OpaqueId,
        event_id: &OutboxEventId,
        created_at: UnixMillis,
    ) -> Result<DeliveryState, NotificationPortError>;

    async fn compare_and_swap_delivery(
        &self,
        scope: &TenantScope,
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
    event_id: OutboxEventId,
    reason_class: ReplayReasonClass,
    requested_at: UnixMillis,
}

impl NotificationReplayIntent {
    #[must_use]
    pub const fn new(
        replay_id: OpaqueId,
        event_id: OutboxEventId,
        reason_class: ReplayReasonClass,
        requested_at: UnixMillis,
    ) -> Self {
        Self {
            replay_id,
            event_id,
            reason_class,
            requested_at,
        }
    }

    #[must_use]
    pub const fn replay_id(&self) -> &OpaqueId {
        &self.replay_id
    }

    #[must_use]
    pub const fn event_id(&self) -> &OutboxEventId {
        &self.event_id
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
pub enum ReplayIntentWriteOutcome {
    Recorded,
    Duplicate,
}

pub trait NotificationReplayRepositoryPort {
    /// Persists immutable replay intent before any replay publication occurs.
    async fn record_replay_intent(
        &self,
        actor: &ActorContext,
        intent: &NotificationReplayIntent,
    ) -> Result<ReplayIntentWriteOutcome, NotificationPortError>;
}
