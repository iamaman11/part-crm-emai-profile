use crate::catch_up::{commit_catch_up, load_catch_up};
use crate::error::NotificationOperationError;
use application_ports::{
    CursorAdvanceWriteOutcome, NotificationAuthorizationPort, NotificationCapability,
    NotificationCatchUpRepositoryPort, NotificationCursorRepositoryPort, NotificationEventRecord,
    RealtimeNotificationAudiencePort, RealtimeNotificationAuthorizationPort,
    RealtimeNotificationSinkPort,
};
use contracts::{RealtimeInvalidationSignal, RealtimeResourceKind};
use profile_platform_primitives::{ActorContext, ActorId, TenantId, UnixMillis};

pub const MAX_REALTIME_AUDIENCE_PAGE_SIZE: u32 = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeSynchronizationOutcome {
    delivered_count: u32,
    cursor_outcome: CursorAdvanceWriteOutcome,
}

impl RealtimeSynchronizationOutcome {
    #[must_use]
    pub const fn new(delivered_count: u32, cursor_outcome: CursorAdvanceWriteOutcome) -> Self {
        Self {
            delivered_count,
            cursor_outcome,
        }
    }

    #[must_use]
    pub const fn delivered_count(self) -> u32 {
        self.delivered_count
    }

    #[must_use]
    pub const fn cursor_outcome(self) -> CursorAdvanceWriteOutcome {
        self.cursor_outcome
    }
}

/// Reconnect synchronization is durable-first: load the actor's currently authorized Phase 1B
/// catch-up page, emit only canonical invalidation signals, then advance the durable cursor after
/// the whole page has been handed to the realtime sink. A sink failure leaves the cursor unchanged,
/// so reconnect repeats the page instead of losing events.
pub async fn synchronize_realtime_session<A, C, H, S>(
    authorization: &A,
    cursors: &C,
    history: &H,
    sink: &S,
    actor: &ActorContext,
    limit: u32,
    delivered_at: UnixMillis,
) -> Result<RealtimeSynchronizationOutcome, NotificationOperationError>
where
    A: NotificationAuthorizationPort,
    C: NotificationCursorRepositoryPort,
    H: NotificationCatchUpRepositoryPort,
    S: RealtimeNotificationSinkPort,
{
    let batch = load_catch_up(authorization, cursors, history, actor, limit).await?;
    for event in batch.events() {
        let signal = invalidation_signal_for_event(event);
        sink.publish_invalidation(actor, &signal).await?;
    }

    let delivered_count = u32::try_from(batch.events().len())
        .map_err(|_| NotificationOperationError::IntegrityFailure)?;
    let cursor_outcome = commit_catch_up(cursors, actor, &batch, delivered_at).await?;
    Ok(RealtimeSynchronizationOutcome::new(
        delivered_count,
        cursor_outcome,
    ))
}

pub async fn load_realtime_audience_page<P>(
    audience: &P,
    tenant_id: &TenantId,
    event: &NotificationEventRecord,
    after_actor_id: Option<&ActorId>,
    limit: u32,
) -> Result<Vec<ActorId>, NotificationOperationError>
where
    P: RealtimeNotificationAudiencePort,
{
    if limit == 0 || limit > MAX_REALTIME_AUDIENCE_PAGE_SIZE {
        return Err(NotificationOperationError::InvalidInput);
    }
    let actors = audience
        .load_authorized_actor_page(tenant_id, event, after_actor_id, limit)
        .await?;
    let maximum = usize::try_from(limit).map_err(|_| NotificationOperationError::InvalidInput)?;
    if actors.len() > maximum || !strictly_ordered_after(after_actor_id, &actors) {
        return Err(NotificationOperationError::IntegrityFailure);
    }
    Ok(actors)
}

/// Publishes one live continuation signal only after both current membership/capability
/// authorization and event-specific current-grant authorization succeed. Live delivery does not
/// advance the durable catch-up cursor: reconnect may intentionally repeat live signals, and the
/// opaque event id is the duplicate-suppression key. This avoids skipping durable events if live
/// producers arrive out of order or process memory disappears.
pub async fn publish_live_invalidation<A, E, S>(
    authorization: &A,
    event_authorization: &E,
    sink: &S,
    actor: &ActorContext,
    event: &NotificationEventRecord,
) -> Result<(), NotificationOperationError>
where
    A: NotificationAuthorizationPort,
    E: RealtimeNotificationAuthorizationPort,
    S: RealtimeNotificationSinkPort,
{
    if !authorization
        .is_authorized(actor, NotificationCapability::CatchUp)
        .await?
    {
        return Err(NotificationOperationError::Forbidden);
    }
    if !event_authorization
        .is_event_authorized(actor, event)
        .await?
    {
        return Err(NotificationOperationError::Forbidden);
    }

    let signal = invalidation_signal_for_event(event);
    sink.publish_invalidation(actor, &signal).await?;
    Ok(())
}

#[must_use]
pub fn invalidation_signal_for_event(
    event: &NotificationEventRecord,
) -> RealtimeInvalidationSignal {
    let resource = match event.aggregate_type() {
        "client" => RealtimeResourceKind::Clients,
        "profile" => RealtimeResourceKind::Profiles,
        "mailbox" => RealtimeResourceKind::Mailboxes,
        "membership" | "member" | "identity" => RealtimeResourceKind::Memberships,
        "device" | "device_job" => RealtimeResourceKind::Devices,
        _ => RealtimeResourceKind::Platform,
    };
    RealtimeInvalidationSignal::new(event.event_id().clone(), resource, event.occurred_at())
}

fn strictly_ordered_after(after: Option<&ActorId>, actors: &[ActorId]) -> bool {
    let mut previous = after;
    for actor in actors {
        if previous.is_some_and(|value| value >= actor) {
            return false;
        }
        previous = Some(actor);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{invalidation_signal_for_event, strictly_ordered_after};
    use application_ports::NotificationEventRecord;
    use contracts::{REALTIME_INVALIDATION_VERSION, RealtimeResourceKind};
    use profile_platform_primitives::{ActorId, OpaqueId, OutboxEventId, UnixMillis};

    #[test]
    fn event_is_reduced_to_low_cardinality_invalidation_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let event = NotificationEventRecord::new(
            OutboxEventId::parse("outbox_01JREALTIME")?,
            "client",
            OpaqueId::parse("client_01JCONFIDENTIAL")?,
            "client.contact.changed.v1",
            UnixMillis::new(99),
        );
        let signal = invalidation_signal_for_event(&event);
        assert_eq!(signal.version(), REALTIME_INVALIDATION_VERSION);
        assert_eq!(signal.event_id().as_str(), "outbox_01JREALTIME");
        assert_eq!(signal.resource(), RealtimeResourceKind::Clients);
        assert_eq!(signal.occurred_at(), UnixMillis::new(99));
        Ok(())
    }

    #[test]
    fn unknown_aggregate_type_cannot_expand_realtime_cardinality()
    -> Result<(), Box<dyn std::error::Error>> {
        let event = NotificationEventRecord::new(
            OutboxEventId::parse("outbox_01JREALTIME_UNKNOWN")?,
            "future_sensitive_capability",
            OpaqueId::parse("opaque_01JREALTIME_UNKNOWN")?,
            "future.changed.v1",
            UnixMillis::new(100),
        );
        let signal = invalidation_signal_for_event(&event);
        assert_eq!(signal.resource(), RealtimeResourceKind::Platform);
        Ok(())
    }

    #[test]
    fn audience_page_order_cannot_rewind_or_duplicate() -> Result<(), Box<dyn std::error::Error>> {
        let after = ActorId::parse("actor_01JREALTIME_A")?;
        let ordered = vec![
            ActorId::parse("actor_01JREALTIME_B")?,
            ActorId::parse("actor_01JREALTIME_C")?,
        ];
        assert!(strictly_ordered_after(Some(&after), &ordered));
        assert!(!strictly_ordered_after(Some(&after), &[after.clone()]));
        assert!(!strictly_ordered_after(
            Some(&after),
            &[ActorId::parse("actor_01JREALTIME_0")?]
        ));
        assert!(!strictly_ordered_after(
            None,
            &[
                ActorId::parse("actor_01JREALTIME_B")?,
                ActorId::parse("actor_01JREALTIME_B")?,
            ]
        ));
        Ok(())
    }
}
