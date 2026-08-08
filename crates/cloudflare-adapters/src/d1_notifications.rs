use application_ports::{
    CursorAdvanceWriteOutcome, DeliveryTransitionWriteOutcome, NotificationCursorRepositoryPort,
    NotificationDeliveryRepositoryPort, NotificationPortError, NotificationPortErrorClass,
};
use notification_domain::{DeliveryFailureClass, DeliveryState, NotificationCursor};
use profile_platform_primitives::{
    ActorId, OpaqueId, OutboxEventId, TenantId, TenantScope, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const NULL_INTEGER_SENTINEL: i64 = -1;
const NULL_TEXT_SENTINEL: &str = "";

const INSERT_READY: &str = r#"
INSERT INTO notification_deliveries (
    tenant_id,
    consumer_id,
    outbox_event_id,
    delivery_state,
    attempt_count,
    created_at_ms,
    updated_at_ms
) VALUES (?, ?, ?, 'READY', 0, ?, ?)
ON CONFLICT (tenant_id, consumer_id, outbox_event_id) DO NOTHING
"#;

const LOAD_DELIVERY: &str = r#"
SELECT
    delivery_state,
    attempt_count,
    last_attempt_at_ms,
    next_attempt_at_ms,
    delivered_at_ms,
    terminal_at_ms,
    failure_class
FROM notification_deliveries
WHERE tenant_id = ?
  AND consumer_id = ?
  AND outbox_event_id = ?
"#;

const UPDATE_TO_RETRY: &str = r#"
UPDATE notification_deliveries
SET delivery_state = 'RETRY_SCHEDULED',
    attempt_count = ?,
    last_attempt_at_ms = ?,
    next_attempt_at_ms = ?,
    delivered_at_ms = NULL,
    terminal_at_ms = NULL,
    failure_class = ?,
    updated_at_ms = ?
WHERE tenant_id = ?
  AND consumer_id = ?
  AND outbox_event_id = ?
  AND delivery_state = ?
  AND attempt_count = ?
  AND COALESCE(last_attempt_at_ms, -1) = ?
  AND COALESCE(next_attempt_at_ms, -1) = ?
  AND COALESCE(delivered_at_ms, -1) = ?
  AND COALESCE(terminal_at_ms, -1) = ?
  AND COALESCE(failure_class, '') = ?
RETURNING outbox_event_id
"#;

const UPDATE_TO_DELIVERED: &str = r#"
UPDATE notification_deliveries
SET delivery_state = 'DELIVERED',
    attempt_count = ?,
    last_attempt_at_ms = ?,
    next_attempt_at_ms = NULL,
    delivered_at_ms = ?,
    terminal_at_ms = NULL,
    failure_class = NULL,
    updated_at_ms = ?
WHERE tenant_id = ?
  AND consumer_id = ?
  AND outbox_event_id = ?
  AND delivery_state = ?
  AND attempt_count = ?
  AND COALESCE(last_attempt_at_ms, -1) = ?
  AND COALESCE(next_attempt_at_ms, -1) = ?
  AND COALESCE(delivered_at_ms, -1) = ?
  AND COALESCE(terminal_at_ms, -1) = ?
  AND COALESCE(failure_class, '') = ?
RETURNING outbox_event_id
"#;

const UPDATE_TO_DEAD_LETTER: &str = r#"
UPDATE notification_deliveries
SET delivery_state = 'DEAD_LETTER',
    attempt_count = ?,
    last_attempt_at_ms = ?,
    next_attempt_at_ms = NULL,
    delivered_at_ms = NULL,
    terminal_at_ms = ?,
    failure_class = ?,
    updated_at_ms = ?
WHERE tenant_id = ?
  AND consumer_id = ?
  AND outbox_event_id = ?
  AND delivery_state = ?
  AND attempt_count = ?
  AND COALESCE(last_attempt_at_ms, -1) = ?
  AND COALESCE(next_attempt_at_ms, -1) = ?
  AND COALESCE(delivered_at_ms, -1) = ?
  AND COALESCE(terminal_at_ms, -1) = ?
  AND COALESCE(failure_class, '') = ?
RETURNING outbox_event_id
"#;

const LOAD_CURSOR: &str = r#"
SELECT occurred_at_ms, outbox_event_id
FROM user_event_cursors
WHERE tenant_id = ? AND actor_id = ?
"#;

const INSERT_CURSOR: &str = r#"
INSERT INTO user_event_cursors (
    tenant_id, actor_id, occurred_at_ms, outbox_event_id, updated_at_ms
) VALUES (?, ?, ?, ?, ?)
ON CONFLICT (tenant_id, actor_id) DO NOTHING
RETURNING outbox_event_id
"#;

const UPDATE_CURSOR: &str = r#"
UPDATE user_event_cursors
SET occurred_at_ms = ?,
    outbox_event_id = ?,
    updated_at_ms = ?
WHERE tenant_id = ?
  AND actor_id = ?
  AND occurred_at_ms = ?
  AND outbox_event_id = ?
RETURNING outbox_event_id
"#;

#[derive(Debug, Deserialize)]
struct DeliveryRow {
    delivery_state: String,
    attempt_count: i64,
    last_attempt_at_ms: Option<i64>,
    next_attempt_at_ms: Option<i64>,
    delivered_at_ms: Option<i64>,
    terminal_at_ms: Option<i64>,
    failure_class: Option<String>,
}

impl DeliveryRow {
    fn into_state(self) -> Result<DeliveryState, NotificationPortError> {
        let attempts = u16::try_from(self.attempt_count).map_err(|_| integrity_failure())?;
        match self.delivery_state.as_str() {
            "READY"
                if self.last_attempt_at_ms.is_none()
                    && self.next_attempt_at_ms.is_none()
                    && self.delivered_at_ms.is_none()
                    && self.terminal_at_ms.is_none()
                    && self.failure_class.is_none() =>
            {
                DeliveryState::restore_ready(attempts).map_err(|_| integrity_failure())
            }
            "RETRY_SCHEDULED"
                if self.delivered_at_ms.is_none() && self.terminal_at_ms.is_none() =>
            {
                let last_attempt_at = required_unix(self.last_attempt_at_ms)?;
                let next_attempt_at = required_unix(self.next_attempt_at_ms)?;
                let failure_class = required_failure_class(self.failure_class.as_deref())?;
                DeliveryState::restore_retry_scheduled(
                    attempts,
                    last_attempt_at,
                    next_attempt_at,
                    failure_class,
                )
                .map_err(|_| integrity_failure())
            }
            "DELIVERED"
                if self.next_attempt_at_ms.is_none()
                    && self.terminal_at_ms.is_none()
                    && self.failure_class.is_none() =>
            {
                let last_attempt_at = required_unix(self.last_attempt_at_ms)?;
                let delivered_at = required_unix(self.delivered_at_ms)?;
                if last_attempt_at != delivered_at {
                    return Err(integrity_failure());
                }
                DeliveryState::restore_delivered(attempts, delivered_at)
                    .map_err(|_| integrity_failure())
            }
            "DEAD_LETTER"
                if self.next_attempt_at_ms.is_none() && self.delivered_at_ms.is_none() =>
            {
                let last_attempt_at = required_unix(self.last_attempt_at_ms)?;
                let terminal_at = required_unix(self.terminal_at_ms)?;
                if last_attempt_at != terminal_at {
                    return Err(integrity_failure());
                }
                let failure_class = required_failure_class(self.failure_class.as_deref())?;
                DeliveryState::restore_dead_letter(attempts, terminal_at, failure_class)
                    .map_err(|_| integrity_failure())
            }
            _ => Err(integrity_failure()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CursorRow {
    occurred_at_ms: i64,
    outbox_event_id: String,
}

impl CursorRow {
    fn into_cursor(self) -> Result<NotificationCursor, NotificationPortError> {
        Ok(NotificationCursor::new(
            unix_from_i64(self.occurred_at_ms)?,
            OutboxEventId::parse(self.outbox_event_id).map_err(|_| integrity_failure())?,
        ))
    }
}

#[derive(Clone, Copy)]
struct ExpectedDeliveryShape {
    state: &'static str,
    attempts: i64,
    last_attempt_at: i64,
    next_attempt_at: i64,
    delivered_at: i64,
    terminal_at: i64,
    failure_class: &'static str,
}

impl ExpectedDeliveryShape {
    fn from_state(state: DeliveryState) -> Result<Self, NotificationPortError> {
        let attempts = i64::from(state.attempts().value());
        match state {
            DeliveryState::Ready { .. } => Ok(Self {
                state: "READY",
                attempts,
                last_attempt_at: NULL_INTEGER_SENTINEL,
                next_attempt_at: NULL_INTEGER_SENTINEL,
                delivered_at: NULL_INTEGER_SENTINEL,
                terminal_at: NULL_INTEGER_SENTINEL,
                failure_class: NULL_TEXT_SENTINEL,
            }),
            DeliveryState::RetryScheduled {
                last_attempt_at,
                next_attempt_at,
                failure_class,
                ..
            } => Ok(Self {
                state: "RETRY_SCHEDULED",
                attempts,
                last_attempt_at: sqlite_integer(last_attempt_at)?,
                next_attempt_at: sqlite_integer(next_attempt_at)?,
                delivered_at: NULL_INTEGER_SENTINEL,
                terminal_at: NULL_INTEGER_SENTINEL,
                failure_class: failure_class_to_storage(failure_class),
            }),
            DeliveryState::Delivered { delivered_at, .. } => {
                let delivered_at = sqlite_integer(delivered_at)?;
                Ok(Self {
                    state: "DELIVERED",
                    attempts,
                    last_attempt_at: delivered_at,
                    next_attempt_at: NULL_INTEGER_SENTINEL,
                    delivered_at,
                    terminal_at: NULL_INTEGER_SENTINEL,
                    failure_class: NULL_TEXT_SENTINEL,
                })
            }
            DeliveryState::DeadLetter {
                terminal_at,
                failure_class,
                ..
            } => {
                let terminal_at = sqlite_integer(terminal_at)?;
                Ok(Self {
                    state: "DEAD_LETTER",
                    attempts,
                    last_attempt_at: terminal_at,
                    next_attempt_at: NULL_INTEGER_SENTINEL,
                    delivered_at: NULL_INTEGER_SENTINEL,
                    terminal_at,
                    failure_class: failure_class_to_storage(failure_class),
                })
            }
        }
    }
}

pub struct D1NotificationRepository {
    database: D1Database,
}

impl D1NotificationRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn load_delivery(
        &self,
        tenant_id: &TenantId,
        consumer_id: &OpaqueId,
        event_id: &OutboxEventId,
    ) -> Result<DeliveryState, NotificationPortError> {
        let row = query!(
            &self.database,
            LOAD_DELIVERY,
            tenant_id.as_str(),
            consumer_id.as_str(),
            event_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<DeliveryRow>(None)
        .await
        .map_err(map_worker_error)?
        .ok_or_else(integrity_failure)?;
        row.into_state()
    }
}

impl NotificationDeliveryRepositoryPort for D1NotificationRepository {
    async fn load_or_create_delivery(
        &self,
        tenant_id: &TenantId,
        consumer_id: &OpaqueId,
        event_id: &OutboxEventId,
        created_at: UnixMillis,
    ) -> Result<DeliveryState, NotificationPortError> {
        let created_at = sqlite_integer(created_at)?;
        query!(
            &self.database,
            INSERT_READY,
            tenant_id.as_str(),
            consumer_id.as_str(),
            event_id.as_str(),
            created_at,
            created_at
        )
        .map_err(map_worker_error)?
        .run()
        .await
        .map_err(map_worker_error)?;
        self.load_delivery(tenant_id, consumer_id, event_id).await
    }

    async fn compare_and_swap_delivery(
        &self,
        tenant_id: &TenantId,
        consumer_id: &OpaqueId,
        event_id: &OutboxEventId,
        expected: DeliveryState,
        next: DeliveryState,
    ) -> Result<DeliveryTransitionWriteOutcome, NotificationPortError> {
        let expected = ExpectedDeliveryShape::from_state(expected)?;
        let returned = match next {
            DeliveryState::Ready { .. } => return Err(integrity_failure()),
            DeliveryState::RetryScheduled {
                attempts,
                last_attempt_at,
                next_attempt_at,
                failure_class,
            } => {
                let attempt_count = i64::from(attempts.value());
                let last_attempt_at = sqlite_integer(last_attempt_at)?;
                let next_attempt_at = sqlite_integer(next_attempt_at)?;
                query!(
                    &self.database,
                    UPDATE_TO_RETRY,
                    attempt_count,
                    last_attempt_at,
                    next_attempt_at,
                    failure_class_to_storage(failure_class),
                    last_attempt_at,
                    tenant_id.as_str(),
                    consumer_id.as_str(),
                    event_id.as_str(),
                    expected.state,
                    expected.attempts,
                    expected.last_attempt_at,
                    expected.next_attempt_at,
                    expected.delivered_at,
                    expected.terminal_at,
                    expected.failure_class
                )
                .map_err(map_worker_error)?
                .first::<String>(Some("outbox_event_id"))
                .await
                .map_err(map_worker_error)?
            }
            DeliveryState::Delivered {
                attempts,
                delivered_at,
            } => {
                let attempt_count = i64::from(attempts.value());
                let delivered_at = sqlite_integer(delivered_at)?;
                query!(
                    &self.database,
                    UPDATE_TO_DELIVERED,
                    attempt_count,
                    delivered_at,
                    delivered_at,
                    delivered_at,
                    tenant_id.as_str(),
                    consumer_id.as_str(),
                    event_id.as_str(),
                    expected.state,
                    expected.attempts,
                    expected.last_attempt_at,
                    expected.next_attempt_at,
                    expected.delivered_at,
                    expected.terminal_at,
                    expected.failure_class
                )
                .map_err(map_worker_error)?
                .first::<String>(Some("outbox_event_id"))
                .await
                .map_err(map_worker_error)?
            }
            DeliveryState::DeadLetter {
                attempts,
                terminal_at,
                failure_class,
            } => {
                let attempt_count = i64::from(attempts.value());
                let terminal_at = sqlite_integer(terminal_at)?;
                query!(
                    &self.database,
                    UPDATE_TO_DEAD_LETTER,
                    attempt_count,
                    terminal_at,
                    terminal_at,
                    failure_class_to_storage(failure_class),
                    terminal_at,
                    tenant_id.as_str(),
                    consumer_id.as_str(),
                    event_id.as_str(),
                    expected.state,
                    expected.attempts,
                    expected.last_attempt_at,
                    expected.next_attempt_at,
                    expected.delivered_at,
                    expected.terminal_at,
                    expected.failure_class
                )
                .map_err(map_worker_error)?
                .first::<String>(Some("outbox_event_id"))
                .await
                .map_err(map_worker_error)?
            }
        };

        Ok(if returned.is_some() {
            DeliveryTransitionWriteOutcome::Applied
        } else {
            DeliveryTransitionWriteOutcome::Stale
        })
    }
}

impl NotificationCursorRepositoryPort for D1NotificationRepository {
    async fn load_user_cursor(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
    ) -> Result<Option<NotificationCursor>, NotificationPortError> {
        query!(
            &self.database,
            LOAD_CURSOR,
            scope.tenant_id().as_str(),
            actor_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<CursorRow>(None)
        .await
        .map_err(map_worker_error)?
        .map(CursorRow::into_cursor)
        .transpose()
    }

    async fn compare_and_advance_user_cursor(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        expected: Option<&NotificationCursor>,
        next: &NotificationCursor,
        advanced_at: UnixMillis,
    ) -> Result<CursorAdvanceWriteOutcome, NotificationPortError> {
        let occurred_at = sqlite_integer(next.occurred_at())?;
        let advanced_at = sqlite_integer(advanced_at)?;
        let returned = if let Some(expected) = expected {
            expected
                .clone()
                .advance_to(next.clone())
                .map_err(|_| integrity_failure())?;
            let expected_occurred_at = sqlite_integer(expected.occurred_at())?;
            query!(
                &self.database,
                UPDATE_CURSOR,
                occurred_at,
                next.event_id().as_str(),
                advanced_at,
                scope.tenant_id().as_str(),
                actor_id.as_str(),
                expected_occurred_at,
                expected.event_id().as_str()
            )
            .map_err(map_worker_error)?
            .first::<String>(Some("outbox_event_id"))
            .await
            .map_err(map_worker_error)?
        } else {
            query!(
                &self.database,
                INSERT_CURSOR,
                scope.tenant_id().as_str(),
                actor_id.as_str(),
                occurred_at,
                next.event_id().as_str(),
                advanced_at
            )
            .map_err(map_worker_error)?
            .first::<String>(Some("outbox_event_id"))
            .await
            .map_err(map_worker_error)?
        };

        if returned.is_none() {
            return Ok(CursorAdvanceWriteOutcome::Stale);
        }
        Ok(if expected == Some(next) {
            CursorAdvanceWriteOutcome::Unchanged
        } else {
            CursorAdvanceWriteOutcome::Advanced
        })
    }
}

fn sqlite_integer(value: UnixMillis) -> Result<i64, NotificationPortError> {
    i64::try_from(value.value()).map_err(|_| integrity_failure())
}

fn unix_from_i64(value: i64) -> Result<UnixMillis, NotificationPortError> {
    u64::try_from(value)
        .map(UnixMillis::new)
        .map_err(|_| integrity_failure())
}

fn required_unix(value: Option<i64>) -> Result<UnixMillis, NotificationPortError> {
    unix_from_i64(value.ok_or_else(integrity_failure)?)
}

const fn failure_class_to_storage(value: DeliveryFailureClass) -> &'static str {
    match value {
        DeliveryFailureClass::DependencyUnavailable => "DEPENDENCY_UNAVAILABLE",
        DeliveryFailureClass::Rejected => "REJECTED",
        DeliveryFailureClass::IntegrityFailure => "INTEGRITY_FAILURE",
        DeliveryFailureClass::InternalFailure => "INTERNAL_FAILURE",
    }
}

fn required_failure_class(
    value: Option<&str>,
) -> Result<DeliveryFailureClass, NotificationPortError> {
    match value {
        Some("DEPENDENCY_UNAVAILABLE") => Ok(DeliveryFailureClass::DependencyUnavailable),
        Some("REJECTED") => Ok(DeliveryFailureClass::Rejected),
        Some("INTEGRITY_FAILURE") => Ok(DeliveryFailureClass::IntegrityFailure),
        Some("INTERNAL_FAILURE") => Ok(DeliveryFailureClass::InternalFailure),
        _ => Err(integrity_failure()),
    }
}

fn integrity_failure() -> NotificationPortError {
    NotificationPortError::new(NotificationPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> NotificationPortError {
    NotificationPortError::new(NotificationPortErrorClass::InternalFailure)
}

#[cfg(test)]
mod tests {
    use super::{CursorRow, DeliveryRow};
    use notification_domain::{DeliveryFailureClass, DeliveryState};

    #[test]
    fn persisted_retry_row_restores_through_domain_invariants()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = DeliveryRow {
            delivery_state: "RETRY_SCHEDULED".to_owned(),
            attempt_count: 2,
            last_attempt_at_ms: Some(100),
            next_attempt_at_ms: Some(200),
            delivered_at_ms: None,
            terminal_at_ms: None,
            failure_class: Some("DEPENDENCY_UNAVAILABLE".to_owned()),
        }
        .into_state()?;
        assert_eq!(state.attempts().value(), 2);
        assert_eq!(
            state.failure_class(),
            Some(DeliveryFailureClass::DependencyUnavailable)
        );
        Ok(())
    }

    #[test]
    fn malformed_persisted_delivery_row_is_rejected() {
        assert!(
            DeliveryRow {
                delivery_state: "RETRY_SCHEDULED".to_owned(),
                attempt_count: 64,
                last_attempt_at_ms: Some(100),
                next_attempt_at_ms: Some(200),
                delivered_at_ms: None,
                terminal_at_ms: None,
                failure_class: Some("REJECTED".to_owned()),
            }
            .into_state()
            .is_err()
        );
        assert!(
            DeliveryRow {
                delivery_state: "DELIVERED".to_owned(),
                attempt_count: 1,
                last_attempt_at_ms: Some(100),
                next_attempt_at_ms: None,
                delivered_at_ms: Some(101),
                terminal_at_ms: None,
                failure_class: None,
            }
            .into_state()
            .is_err()
        );
    }

    #[test]
    fn persisted_cursor_rejects_invalid_source_identity() {
        assert!(
            CursorRow {
                occurred_at_ms: 10,
                outbox_event_id: "bad/id".to_owned(),
            }
            .into_cursor()
            .is_err()
        );
    }

    #[test]
    fn terminal_row_restores_without_retry_time() -> Result<(), Box<dyn std::error::Error>> {
        let state = DeliveryRow {
            delivery_state: "DEAD_LETTER".to_owned(),
            attempt_count: 3,
            last_attempt_at_ms: Some(300),
            next_attempt_at_ms: None,
            delivered_at_ms: None,
            terminal_at_ms: Some(300),
            failure_class: Some("INTERNAL_FAILURE".to_owned()),
        }
        .into_state()?;
        assert!(matches!(state, DeliveryState::DeadLetter { .. }));
        assert!(state.next_attempt_at().is_none());
        Ok(())
    }
}
