use crate::error::NotificationOperationError;
use crate::realtime::{publish_live_invalidation, synchronize_realtime_session};
use application_ports::{
    CursorAdvanceWriteOutcome, NotificationAuthorizationPort, NotificationCapability,
    NotificationCatchUpRepositoryPort, NotificationCursorRepositoryPort, NotificationEventPage,
    NotificationEventRecord, NotificationPortError, NotificationPortErrorClass,
    RealtimeInvalidationSignal, RealtimeNotificationAuthorizationPort,
    RealtimeNotificationSinkPort,
};
use notification_domain::NotificationCursor;
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, OpaqueId, OutboxEventId, TenantId, TenantScope, UnixMillis,
};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

struct ImmediateWake;

impl Wake for ImmediateWake {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ImmediateWake));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

struct Authorization {
    allowed: Cell<bool>,
}

impl NotificationAuthorizationPort for Authorization {
    async fn is_authorized(
        &self,
        _actor: &ActorContext,
        _capability: NotificationCapability,
    ) -> Result<bool, NotificationPortError> {
        Ok(self.allowed.get())
    }
}

struct EventAuthorization {
    allowed: Cell<bool>,
}

impl RealtimeNotificationAuthorizationPort for EventAuthorization {
    async fn is_event_authorized(
        &self,
        _actor: &ActorContext,
        _event: &NotificationEventRecord,
    ) -> Result<bool, NotificationPortError> {
        Ok(self.allowed.get())
    }
}

struct CursorRepository {
    cursor: RefCell<Option<NotificationCursor>>,
    stale_once: Cell<bool>,
    compare_and_advance_calls: Cell<u32>,
}

impl CursorRepository {
    fn new(stale_once: bool) -> Self {
        Self {
            cursor: RefCell::new(None),
            stale_once: Cell::new(stale_once),
            compare_and_advance_calls: Cell::new(0),
        }
    }
}

impl NotificationCursorRepositoryPort for CursorRepository {
    async fn load_user_cursor(
        &self,
        _scope: &TenantScope,
        _actor_id: &ActorId,
    ) -> Result<Option<NotificationCursor>, NotificationPortError> {
        Ok(self.cursor.borrow().clone())
    }

    async fn compare_and_advance_user_cursor(
        &self,
        _scope: &TenantScope,
        _actor_id: &ActorId,
        expected: Option<&NotificationCursor>,
        next: &NotificationCursor,
        _advanced_at: UnixMillis,
    ) -> Result<CursorAdvanceWriteOutcome, NotificationPortError> {
        self.compare_and_advance_calls
            .set(self.compare_and_advance_calls.get() + 1);
        if self.stale_once.replace(false) {
            return Ok(CursorAdvanceWriteOutcome::Stale);
        }
        let current = self.cursor.borrow().clone();
        if current.as_ref() != expected {
            return Ok(CursorAdvanceWriteOutcome::Stale);
        }
        if current.as_ref() == Some(next) {
            return Ok(CursorAdvanceWriteOutcome::Unchanged);
        }
        *self.cursor.borrow_mut() = Some(next.clone());
        Ok(CursorAdvanceWriteOutcome::Advanced)
    }
}

struct History {
    events: Vec<NotificationEventRecord>,
}

impl NotificationCatchUpRepositoryPort for History {
    async fn load_authorized_event_page(
        &self,
        _actor: &ActorContext,
        after: Option<&NotificationCursor>,
        limit: u32,
    ) -> Result<NotificationEventPage, NotificationPortError> {
        let mut events = self
            .events
            .iter()
            .filter(|event| is_after(event, after))
            .take(usize::try_from(limit).map_err(|_| {
                NotificationPortError::new(NotificationPortErrorClass::IntegrityFailure)
            })?)
            .cloned()
            .collect::<Vec<_>>();
        events.sort_by(|left, right| {
            left.occurred_at()
                .value()
                .cmp(&right.occurred_at().value())
                .then_with(|| left.event_id().as_str().cmp(right.event_id().as_str()))
        });
        Ok(NotificationEventPage::new(events))
    }
}

fn is_after(event: &NotificationEventRecord, after: Option<&NotificationCursor>) -> bool {
    let Some(after) = after else {
        return true;
    };
    event.occurred_at().value() > after.occurred_at().value()
        || (event.occurred_at() == after.occurred_at()
            && event.event_id().as_str() > after.event_id().as_str())
}

struct Sink {
    fail_next: Cell<bool>,
    event_ids: RefCell<Vec<String>>,
}

impl Sink {
    fn new(fail_next: bool) -> Self {
        Self {
            fail_next: Cell::new(fail_next),
            event_ids: RefCell::new(Vec::new()),
        }
    }
}

impl RealtimeNotificationSinkPort for Sink {
    async fn publish_invalidation(
        &self,
        _actor: &ActorContext,
        signal: &RealtimeInvalidationSignal,
    ) -> Result<(), NotificationPortError> {
        if self.fail_next.replace(false) {
            return Err(NotificationPortError::new(
                NotificationPortErrorClass::DependencyUnavailable,
            ));
        }
        self.event_ids
            .borrow_mut()
            .push(signal.event_id().as_str().to_owned());
        Ok(())
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_01JREALTIME")?),
        ActorId::parse("actor_01JREALTIME")?,
        CorrelationId::parse("corr_01JREALTIME")?,
    ))
}

fn event(time: u64, suffix: &str) -> Result<NotificationEventRecord, Box<dyn std::error::Error>> {
    Ok(NotificationEventRecord::new(
        OutboxEventId::parse(format!("outbox_01JREALTIME_{suffix}"))?,
        "client",
        OpaqueId::parse("client_01JREALTIME")?,
        "client.changed.v1",
        UnixMillis::new(time),
    ))
}

#[test]
fn socket_failure_preserves_cursor_and_fresh_session_replays_without_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let authorization = Authorization {
        allowed: Cell::new(true),
    };
    let cursors = CursorRepository::new(false);
    let history = History {
        events: vec![event(10, "A")?],
    };
    let failed_socket = Sink::new(true);

    let first = block_on(synchronize_realtime_session(
        &authorization,
        &cursors,
        &history,
        &failed_socket,
        &actor,
        200,
        UnixMillis::new(20),
    ));
    assert_eq!(first, Err(NotificationOperationError::DependencyUnavailable));
    assert!(cursors.cursor.borrow().is_none());
    assert_eq!(cursors.compare_and_advance_calls.get(), 0);

    let fresh_socket = Sink::new(false);
    let second = block_on(synchronize_realtime_session(
        &authorization,
        &cursors,
        &history,
        &fresh_socket,
        &actor,
        200,
        UnixMillis::new(21),
    ))?;
    assert_eq!(second.cursor_outcome(), CursorAdvanceWriteOutcome::Advanced);
    assert_eq!(fresh_socket.event_ids.borrow().as_slice(), ["outbox_01JREALTIME_A"]);
    assert_eq!(
        cursors.cursor.borrow().as_ref().map(|value| value.event_id().as_str()),
        Some("outbox_01JREALTIME_A")
    );
    Ok(())
}

#[test]
fn cas_race_replays_duplicate_but_never_skips_durable_event()
-> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let authorization = Authorization {
        allowed: Cell::new(true),
    };
    let cursors = CursorRepository::new(true);
    let history = History {
        events: vec![event(10, "A")?],
    };
    let socket = Sink::new(false);

    let first = block_on(synchronize_realtime_session(
        &authorization,
        &cursors,
        &history,
        &socket,
        &actor,
        200,
        UnixMillis::new(20),
    ))?;
    assert_eq!(first.cursor_outcome(), CursorAdvanceWriteOutcome::Stale);
    assert!(cursors.cursor.borrow().is_none());

    let second = block_on(synchronize_realtime_session(
        &authorization,
        &cursors,
        &history,
        &socket,
        &actor,
        200,
        UnixMillis::new(21),
    ))?;
    assert_eq!(second.cursor_outcome(), CursorAdvanceWriteOutcome::Advanced);
    assert_eq!(
        socket.event_ids.borrow().as_slice(),
        ["outbox_01JREALTIME_A", "outbox_01JREALTIME_A"]
    );
    assert_eq!(
        cursors.cursor.borrow().as_ref().map(|value| value.event_id().as_str()),
        Some("outbox_01JREALTIME_A")
    );
    Ok(())
}

#[test]
fn cursor_gap_drains_in_order_across_bounded_pages()
-> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let authorization = Authorization {
        allowed: Cell::new(true),
    };
    let cursors = CursorRepository::new(false);
    let history = History {
        events: vec![event(10, "A")?, event(11, "B")?, event(12, "C")?],
    };
    let socket = Sink::new(false);

    let first = block_on(synchronize_realtime_session(
        &authorization,
        &cursors,
        &history,
        &socket,
        &actor,
        2,
        UnixMillis::new(20),
    ))?;
    assert_eq!(first.delivered_count(), 2);
    let second = block_on(synchronize_realtime_session(
        &authorization,
        &cursors,
        &history,
        &socket,
        &actor,
        2,
        UnixMillis::new(21),
    ))?;
    assert_eq!(second.delivered_count(), 1);
    assert_eq!(
        socket.event_ids.borrow().as_slice(),
        [
            "outbox_01JREALTIME_A",
            "outbox_01JREALTIME_B",
            "outbox_01JREALTIME_C",
        ]
    );
    assert_eq!(
        cursors.cursor.borrow().as_ref().map(|value| value.event_id().as_str()),
        Some("outbox_01JREALTIME_C")
    );
    Ok(())
}

#[test]
fn live_delivery_rechecks_membership_and_exact_event_grant()
-> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let event = event(10, "A")?;
    let authorization = Authorization {
        allowed: Cell::new(false),
    };
    let event_authorization = EventAuthorization {
        allowed: Cell::new(true),
    };
    let socket = Sink::new(false);

    let revoked_membership = block_on(publish_live_invalidation(
        &authorization,
        &event_authorization,
        &socket,
        &actor,
        &event,
    ));
    assert_eq!(revoked_membership, Err(NotificationOperationError::Forbidden));
    assert!(socket.event_ids.borrow().is_empty());

    authorization.allowed.set(true);
    event_authorization.allowed.set(false);
    let revoked_grant = block_on(publish_live_invalidation(
        &authorization,
        &event_authorization,
        &socket,
        &actor,
        &event,
    ));
    assert_eq!(revoked_grant, Err(NotificationOperationError::Forbidden));
    assert!(socket.event_ids.borrow().is_empty());

    event_authorization.allowed.set(true);
    block_on(publish_live_invalidation(
        &authorization,
        &event_authorization,
        &socket,
        &actor,
        &event,
    ))?;
    assert_eq!(socket.event_ids.borrow().as_slice(), ["outbox_01JREALTIME_A"]);
    Ok(())
}
