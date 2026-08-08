use crate::error::NotificationOperationError;
use application_ports::{
    CursorAdvanceWriteOutcome, NotificationAuthorizationPort, NotificationCapability,
    NotificationCatchUpRepositoryPort, NotificationCursorRepositoryPort, NotificationEventRecord,
};
use notification_domain::NotificationCursor;
use profile_platform_primitives::{ActorContext, OutboxEventId, UnixMillis};

pub const MAX_CATCH_UP_PAGE_SIZE: u32 = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationCatchUpBatch {
    expected_cursor: Option<NotificationCursor>,
    next_cursor: Option<NotificationCursor>,
    events: Vec<NotificationEventRecord>,
}

impl NotificationCatchUpBatch {
    #[must_use]
    pub fn expected_cursor(&self) -> Option<&NotificationCursor> {
        self.expected_cursor.as_ref()
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&NotificationCursor> {
        self.next_cursor.as_ref()
    }

    #[must_use]
    pub fn events(&self) -> &[NotificationEventRecord] {
        &self.events
    }
}

pub async fn load_catch_up<A, C, H>(
    authorization: &A,
    cursors: &C,
    history: &H,
    actor: &ActorContext,
    limit: u32,
) -> Result<NotificationCatchUpBatch, NotificationOperationError>
where
    A: NotificationAuthorizationPort,
    C: NotificationCursorRepositoryPort,
    H: NotificationCatchUpRepositoryPort,
{
    if limit == 0 || limit > MAX_CATCH_UP_PAGE_SIZE {
        return Err(NotificationOperationError::InvalidInput);
    }
    if !authorization
        .is_authorized(actor, NotificationCapability::CatchUp)
        .await?
    {
        return Err(NotificationOperationError::Forbidden);
    }

    // Authorization deliberately precedes cursor/history reads so revoked actors cannot infer
    // durable notification position or event existence.
    let expected_cursor = cursors
        .load_user_cursor(actor.tenant_scope(), actor.actor_id())
        .await?;
    let page = history
        .load_authorized_event_page(actor, expected_cursor.as_ref(), limit)
        .await?;
    let events = page.into_events();
    let next_cursor = validate_page(expected_cursor.as_ref(), &events, limit)?;

    Ok(NotificationCatchUpBatch {
        expected_cursor,
        next_cursor,
        events,
    })
}

pub async fn commit_catch_up<C>(
    cursors: &C,
    actor: &ActorContext,
    batch: &NotificationCatchUpBatch,
    delivered_at: UnixMillis,
) -> Result<CursorAdvanceWriteOutcome, NotificationOperationError>
where
    C: NotificationCursorRepositoryPort,
{
    let Some(next_cursor) = batch.next_cursor() else {
        return Ok(CursorAdvanceWriteOutcome::Unchanged);
    };
    cursors
        .compare_and_advance_user_cursor(
            actor.tenant_scope(),
            actor.actor_id(),
            batch.expected_cursor(),
            next_cursor,
            delivered_at,
        )
        .await
        .map_err(Into::into)
}

/// Acknowledges one event only if it is still present in the actor's current authorized page.
/// The client never supplies a raw cursor, so it cannot skip hidden or unauthorized events.
pub async fn acknowledge_catch_up<A, C, H>(
    authorization: &A,
    cursors: &C,
    history: &H,
    actor: &ActorContext,
    event_id: &OutboxEventId,
    delivered_at: UnixMillis,
) -> Result<CursorAdvanceWriteOutcome, NotificationOperationError>
where
    A: NotificationAuthorizationPort,
    C: NotificationCursorRepositoryPort,
    H: NotificationCatchUpRepositoryPort,
{
    let batch = load_catch_up(
        authorization,
        cursors,
        history,
        actor,
        MAX_CATCH_UP_PAGE_SIZE,
    )
    .await?;
    let next = batch
        .events()
        .iter()
        .find(|event| event.event_id() == event_id)
        .map(NotificationEventRecord::cursor)
        .ok_or(NotificationOperationError::InvalidInput)?;

    cursors
        .compare_and_advance_user_cursor(
            actor.tenant_scope(),
            actor.actor_id(),
            batch.expected_cursor(),
            &next,
            delivered_at,
        )
        .await
        .map_err(Into::into)
}

fn validate_page(
    expected: Option<&NotificationCursor>,
    events: &[NotificationEventRecord],
    limit: u32,
) -> Result<Option<NotificationCursor>, NotificationOperationError> {
    if events.len()
        > usize::try_from(limit).map_err(|_| NotificationOperationError::InvalidInput)?
    {
        return Err(NotificationOperationError::IntegrityFailure);
    }

    let mut position = expected.cloned();
    for event in events {
        let candidate = event.cursor();
        if position.as_ref() == Some(&candidate) {
            return Err(NotificationOperationError::IntegrityFailure);
        }
        position = Some(match position.take() {
            Some(current) => current
                .advance_to(candidate)
                .map_err(|_| NotificationOperationError::IntegrityFailure)?,
            None => candidate,
        });
    }

    if events.is_empty() {
        Ok(None)
    } else {
        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CATCH_UP_PAGE_SIZE, validate_page};
    use application_ports::NotificationEventRecord;
    use notification_domain::NotificationCursor;
    use profile_platform_primitives::{OpaqueId, OutboxEventId, UnixMillis};

    fn event(time: u64, id: &str) -> Result<NotificationEventRecord, Box<dyn std::error::Error>> {
        Ok(NotificationEventRecord::new(
            OutboxEventId::parse(id)?,
            "client",
            OpaqueId::parse("client_01JCATCHUP")?,
            "client.created.v1",
            UnixMillis::new(time),
        ))
    }

    #[test]
    fn page_must_be_strictly_ordered_after_durable_cursor() -> Result<(), Box<dyn std::error::Error>>
    {
        let expected = NotificationCursor::new(
            UnixMillis::new(10),
            OutboxEventId::parse("outbox_01JCATCHUP_A")?,
        );
        let ordered = vec![
            event(10, "outbox_01JCATCHUP_B")?,
            event(11, "outbox_01JCATCHUP_C")?,
        ];
        assert_eq!(
            validate_page(Some(&expected), &ordered, MAX_CATCH_UP_PAGE_SIZE)?
                .expect("non-empty page")
                .event_id()
                .as_str(),
            "outbox_01JCATCHUP_C"
        );

        let duplicate = vec![event(10, "outbox_01JCATCHUP_A")?];
        assert!(validate_page(Some(&expected), &duplicate, MAX_CATCH_UP_PAGE_SIZE).is_err());
        let rewind = vec![event(9, "outbox_01JCATCHUP_Z")?];
        assert!(validate_page(Some(&expected), &rewind, MAX_CATCH_UP_PAGE_SIZE).is_err());
        Ok(())
    }

    #[test]
    fn page_cannot_exceed_requested_bound() -> Result<(), Box<dyn std::error::Error>> {
        let events = vec![
            event(1, "outbox_01JCATCHUP_A")?,
            event(2, "outbox_01JCATCHUP_B")?,
        ];
        assert!(validate_page(None, &events, 1).is_err());
        Ok(())
    }
}
