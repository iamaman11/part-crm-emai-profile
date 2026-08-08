use application_ports::{
    NotificationAuthorizationPort, NotificationCapability, NotificationCatchUpRepositoryPort,
    NotificationEventPage, NotificationEventRecord, NotificationOperationsRepositoryPort,
    NotificationOperationsSnapshot, NotificationPortError, NotificationPortErrorClass,
    NotificationReplayIntent, NotificationReplayRepositoryPort, NotificationRetentionOutcome,
    NotificationRetentionRepositoryPort, PendingNotificationReplay, ReplayPreparationOutcome,
    ReplayReasonClass,
};
use notification_domain::{DeliveryFailureClass, DeliveryState, NotificationCursor};
use profile_platform_primitives::{
    ActorContext, OpaqueId, OutboxEventId, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_ACTIVE_ROLE: &str = r#"
SELECT role
FROM memberships
WHERE tenant_id = ?
  AND actor_id = ?
  AND status = 'ACTIVE'
"#;

const LOAD_AUTHORIZED_EVENTS: &str = r#"
SELECT
    notification.outbox_event_id,
    notification.aggregate_type,
    notification.aggregate_id,
    notification.event_type,
    notification.occurred_at_ms
FROM notification_events AS notification
JOIN memberships AS membership
  ON membership.tenant_id = notification.tenant_id
 AND membership.actor_id = ?
 AND membership.status = 'ACTIVE'
WHERE notification.tenant_id = ?
  AND (
      ? < 0
      OR notification.occurred_at_ms > ?
      OR (
          notification.occurred_at_ms = ?
          AND notification.outbox_event_id > ?
      )
  )
  AND (
      membership.role = 'TENANT_OWNER'
      OR (
          membership.role = 'MEMBER'
          AND notification.aggregate_type = 'client'
          AND EXISTS (
              SELECT 1
              FROM client_grants AS grant
              WHERE grant.tenant_id = notification.tenant_id
                AND grant.actor_id = membership.actor_id
                AND grant.client_id = notification.aggregate_id
          )
      )
      OR (
          membership.role = 'MEMBER'
          AND notification.aggregate_type = 'profile'
          AND EXISTS (
              SELECT 1
              FROM profile_grants AS grant
              WHERE grant.tenant_id = notification.tenant_id
                AND grant.actor_id = membership.actor_id
                AND grant.profile_id = notification.aggregate_id
          )
      )
  )
ORDER BY notification.occurred_at_ms ASC, notification.outbox_event_id ASC
LIMIT ?
"#;

const LOAD_EXISTING_REPLAY: &str = r#"
SELECT
    consumer_id,
    outbox_event_id,
    audit_event_id,
    correlation_id,
    requested_by_actor_id,
    reason_class,
    requested_at_ms
FROM notification_replay_intents
WHERE tenant_id = ? AND replay_id = ?
"#;

const LOAD_DEAD_LETTER: &str = r#"
SELECT attempt_count, terminal_at_ms, failure_class
FROM notification_deliveries
WHERE tenant_id = ?
  AND consumer_id = ?
  AND outbox_event_id = ?
  AND delivery_state = 'DEAD_LETTER'
"#;

const INSERT_REPLAY_INTENT: &str = r#"
INSERT INTO notification_replay_intents (
    tenant_id,
    replay_id,
    consumer_id,
    outbox_event_id,
    audit_event_id,
    correlation_id,
    requested_by_actor_id,
    reason_class,
    terminal_attempt_count,
    requested_at_ms
)
SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
WHERE NOT EXISTS (
    SELECT 1
    FROM notification_replay_intents
    WHERE tenant_id = ? AND replay_id = ?
)
RETURNING replay_id
"#;

const LOAD_PENDING_REPLAYS: &str = r#"
SELECT dispatch.replay_id, intent.tenant_id, intent.outbox_event_id
FROM notification_replay_dispatches AS dispatch
JOIN notification_replay_intents AS intent
  ON intent.tenant_id = dispatch.tenant_id
 AND intent.replay_id = dispatch.replay_id
WHERE dispatch.dispatch_state = 'PENDING'
ORDER BY dispatch.created_at_ms ASC, dispatch.replay_id ASC
LIMIT ?
"#;

const MARK_REPLAY_PUBLISHED: &str = r#"
UPDATE notification_replay_dispatches
SET dispatch_state = 'PUBLISHED', published_at_ms = ?
WHERE tenant_id = ?
  AND replay_id = ?
  AND dispatch_state = 'PENDING'
RETURNING replay_id
"#;

const LOAD_REPLAY_DISPATCH_STATE: &str = r#"
SELECT dispatch_state
FROM notification_replay_dispatches
WHERE tenant_id = ? AND replay_id = ?
"#;

const DELETE_DELIVERED: &str = r#"
DELETE FROM notification_deliveries
WHERE rowid IN (
    SELECT rowid
    FROM notification_deliveries
    WHERE delivery_state = 'DELIVERED'
      AND updated_at_ms < ?
    ORDER BY updated_at_ms ASC, outbox_event_id ASC
    LIMIT ?
)
RETURNING outbox_event_id AS removed_id
"#;

const DELETE_INACTIVE_CURSORS: &str = r#"
DELETE FROM user_event_cursors
WHERE rowid IN (
    SELECT cursor.rowid
    FROM user_event_cursors AS cursor
    WHERE cursor.updated_at_ms < ?
      AND NOT EXISTS (
          SELECT 1
          FROM memberships AS membership
          WHERE membership.tenant_id = cursor.tenant_id
            AND membership.actor_id = cursor.actor_id
            AND membership.status = 'ACTIVE'
      )
    ORDER BY cursor.updated_at_ms ASC, cursor.actor_id ASC
    LIMIT ?
)
RETURNING actor_id AS removed_id
"#;

const DELETE_PUBLISHED_REPLAY_DISPATCHES: &str = r#"
DELETE FROM notification_replay_dispatches
WHERE rowid IN (
    SELECT rowid
    FROM notification_replay_dispatches
    WHERE dispatch_state = 'PUBLISHED'
      AND published_at_ms < ?
    ORDER BY published_at_ms ASC, replay_id ASC
    LIMIT ?
)
RETURNING replay_id AS removed_id
"#;

const LOAD_OPERATIONS: &str = r#"
SELECT
    (SELECT COUNT(*) FROM notification_deliveries
      WHERE tenant_id = member.tenant_id AND delivery_state = 'READY') AS ready_count,
    (SELECT COUNT(*) FROM notification_deliveries
      WHERE tenant_id = member.tenant_id AND delivery_state = 'RETRY_SCHEDULED') AS retry_count,
    (SELECT COUNT(*) FROM notification_deliveries
      WHERE tenant_id = member.tenant_id AND delivery_state = 'DELIVERED') AS delivered_count,
    (SELECT COUNT(*) FROM notification_deliveries
      WHERE tenant_id = member.tenant_id AND delivery_state = 'DEAD_LETTER') AS dead_letter_count,
    (SELECT COUNT(*)
       FROM notification_replay_dispatches AS dispatch
       WHERE dispatch.tenant_id = member.tenant_id
         AND dispatch.dispatch_state = 'PENDING') AS pending_replay_count,
    COALESCE((SELECT MAX(attempt_count) FROM notification_deliveries
      WHERE tenant_id = member.tenant_id), 0) AS max_attempt_count,
    (SELECT MIN(created_at_ms) FROM notification_deliveries
      WHERE tenant_id = member.tenant_id
        AND delivery_state IN ('READY', 'RETRY_SCHEDULED')) AS oldest_open_created_at_ms,
    (SELECT COUNT(*)
       FROM notification_events AS event
       LEFT JOIN user_event_cursors AS cursor
         ON cursor.tenant_id = event.tenant_id
        AND cursor.actor_id = member.actor_id
       WHERE event.tenant_id = member.tenant_id
         AND (
             cursor.actor_id IS NULL
             OR event.occurred_at_ms > cursor.occurred_at_ms
             OR (
                 event.occurred_at_ms = cursor.occurred_at_ms
                 AND event.outbox_event_id > cursor.outbox_event_id
             )
         )) AS catch_up_lag_count
FROM memberships AS member
WHERE member.tenant_id = ?
  AND member.actor_id = ?
  AND member.status = 'ACTIVE'
  AND member.role = 'TENANT_OWNER'
"#;

#[derive(Deserialize)]
struct RoleRow {
    role: String,
}

#[derive(Deserialize)]
struct EventRow {
    outbox_event_id: String,
    aggregate_type: String,
    aggregate_id: String,
    event_type: String,
    occurred_at_ms: i64,
}

#[derive(Deserialize)]
struct ExistingReplayRow {
    consumer_id: String,
    outbox_event_id: String,
    audit_event_id: String,
    correlation_id: String,
    requested_by_actor_id: String,
    reason_class: String,
    requested_at_ms: i64,
}

#[derive(Deserialize)]
struct DeadLetterRow {
    attempt_count: i64,
    terminal_at_ms: Option<i64>,
    failure_class: Option<String>,
}

#[derive(Deserialize)]
struct PendingReplayRow {
    replay_id: String,
    tenant_id: String,
    outbox_event_id: String,
}

#[derive(Deserialize)]
struct DispatchStateRow {
    dispatch_state: String,
}

#[derive(Deserialize)]
struct RemovedRow {
    #[allow(dead_code)]
    removed_id: String,
}

#[derive(Deserialize)]
struct OperationsRow {
    ready_count: i64,
    retry_count: i64,
    delivered_count: i64,
    dead_letter_count: i64,
    pending_replay_count: i64,
    max_attempt_count: i64,
    oldest_open_created_at_ms: Option<i64>,
    catch_up_lag_count: i64,
}

pub struct D1NotificationOperationsRepository {
    database: D1Database,
}

impl D1NotificationOperationsRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn existing_replay(
        &self,
        actor: &ActorContext,
        intent: &NotificationReplayIntent,
    ) -> Result<Option<ReplayPreparationOutcome>, NotificationPortError> {
        let row = query!(
            &self.database,
            LOAD_EXISTING_REPLAY,
            actor.tenant_scope().tenant_id().as_str(),
            intent.replay_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<ExistingReplayRow>(None)
        .await
        .map_err(map_worker_error)?;
        let Some(row) = row else {
            return Ok(None);
        };

        let exact = row.consumer_id == intent.consumer_id().as_str()
            && row.outbox_event_id == intent.event_id().as_str()
            && row.audit_event_id == intent.audit_event_id().as_str()
            && row.correlation_id == actor.correlation_id().as_str()
            && row.requested_by_actor_id == actor.actor_id().as_str()
            && row.reason_class == replay_reason_to_storage(intent.reason_class())
            && row.requested_at_ms == sqlite_integer(intent.requested_at())?;
        if exact {
            Ok(Some(ReplayPreparationOutcome::Duplicate))
        } else {
            Err(NotificationPortError::new(NotificationPortErrorClass::Conflict))
        }
    }

    async fn dead_letter_attempt_count(
        &self,
        actor: &ActorContext,
        intent: &NotificationReplayIntent,
    ) -> Result<i64, NotificationPortError> {
        let row = query!(
            &self.database,
            LOAD_DEAD_LETTER,
            actor.tenant_scope().tenant_id().as_str(),
            intent.consumer_id().as_str(),
            intent.event_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<DeadLetterRow>(None)
        .await
        .map_err(map_worker_error)?
        .ok_or_else(|| NotificationPortError::new(NotificationPortErrorClass::Conflict))?;
        let attempts = u16::try_from(row.attempt_count).map_err(|_| integrity_failure())?;
        let terminal_at = unix_from_i64(row.terminal_at_ms.ok_or_else(integrity_failure)?)?;
        let failure_class = required_failure_class(row.failure_class.as_deref())?;
        let state = DeliveryState::restore_dead_letter(attempts, terminal_at, failure_class)
            .map_err(|_| integrity_failure())?;
        state.record_remediation().map_err(|_| integrity_failure())?;
        Ok(i64::from(attempts))
    }
}

impl NotificationAuthorizationPort for D1NotificationOperationsRepository {
    async fn is_authorized(
        &self,
        actor: &ActorContext,
        capability: NotificationCapability,
    ) -> Result<bool, NotificationPortError> {
        let role = query!(
            &self.database,
            LOAD_ACTIVE_ROLE,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<RoleRow>(None)
        .await
        .map_err(map_worker_error)?;
        let Some(role) = role else {
            return Ok(false);
        };
        match (role.role.as_str(), capability) {
            ("TENANT_OWNER", _) => Ok(true),
            ("MEMBER", NotificationCapability::CatchUp) => Ok(true),
            ("MEMBER", NotificationCapability::Remediate | NotificationCapability::ObserveOperations) => {
                Ok(false)
            }
            _ => Err(integrity_failure()),
        }
    }
}

impl NotificationCatchUpRepositoryPort for D1NotificationOperationsRepository {
    async fn load_authorized_event_page(
        &self,
        actor: &ActorContext,
        after: Option<&NotificationCursor>,
        limit: u32,
    ) -> Result<NotificationEventPage, NotificationPortError> {
        let (after_time, after_id) = if let Some(cursor) = after {
            (sqlite_integer(cursor.occurred_at())?, cursor.event_id().as_str())
        } else {
            (-1, "")
        };
        let rows = query!(
            &self.database,
            LOAD_AUTHORIZED_EVENTS,
            actor.actor_id().as_str(),
            actor.tenant_scope().tenant_id().as_str(),
            after_time,
            after_time,
            after_time,
            after_id,
            i64::from(limit)
        )
        .map_err(map_worker_error)?
        .all()
        .await
        .map_err(map_worker_error)?
        .results::<EventRow>()
        .map_err(map_worker_error)?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(NotificationEventRecord::new(
                    OutboxEventId::parse(row.outbox_event_id).map_err(|_| integrity_failure())?,
                    row.aggregate_type,
                    OpaqueId::parse(row.aggregate_id).map_err(|_| integrity_failure())?,
                    row.event_type,
                    unix_from_i64(row.occurred_at_ms)?,
                ))
            })
            .collect::<Result<Vec<_>, NotificationPortError>>()?;
        Ok(NotificationEventPage::new(events))
    }
}

impl NotificationReplayRepositoryPort for D1NotificationOperationsRepository {
    async fn prepare_replay(
        &self,
        actor: &ActorContext,
        intent: &NotificationReplayIntent,
    ) -> Result<ReplayPreparationOutcome, NotificationPortError> {
        if let Some(existing) = self.existing_replay(actor, intent).await? {
            return Ok(existing);
        }
        let terminal_attempt_count = self.dead_letter_attempt_count(actor, intent).await?;
        let requested_at = sqlite_integer(intent.requested_at())?;
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let returned = query!(
            &self.database,
            INSERT_REPLAY_INTENT,
            tenant_id,
            intent.replay_id().as_str(),
            intent.consumer_id().as_str(),
            intent.event_id().as_str(),
            intent.audit_event_id().as_str(),
            actor.correlation_id().as_str(),
            actor.actor_id().as_str(),
            replay_reason_to_storage(intent.reason_class()),
            terminal_attempt_count,
            requested_at,
            tenant_id,
            intent.replay_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("replay_id"))
        .await
        .map_err(map_worker_error)?;

        if returned.is_some() {
            return Ok(ReplayPreparationOutcome::Prepared);
        }
        self.existing_replay(actor, intent)
            .await?
            .ok_or_else(|| NotificationPortError::new(NotificationPortErrorClass::Conflict))
    }

    async fn load_pending_replays(
        &self,
        limit: u32,
    ) -> Result<Vec<PendingNotificationReplay>, NotificationPortError> {
        query!(&self.database, LOAD_PENDING_REPLAYS, i64::from(limit))
            .map_err(map_worker_error)?
            .all()
            .await
            .map_err(map_worker_error)?
            .results::<PendingReplayRow>()
            .map_err(map_worker_error)?
            .into_iter()
            .map(|row| {
                Ok(PendingNotificationReplay::new(
                    OpaqueId::parse(row.replay_id).map_err(|_| integrity_failure())?,
                    TenantId::parse(row.tenant_id).map_err(|_| integrity_failure())?,
                    OutboxEventId::parse(row.outbox_event_id).map_err(|_| integrity_failure())?,
                ))
            })
            .collect()
    }

    async fn mark_replay_published(
        &self,
        tenant_id: &TenantId,
        replay_id: &OpaqueId,
        published_at: UnixMillis,
    ) -> Result<(), NotificationPortError> {
        let returned = query!(
            &self.database,
            MARK_REPLAY_PUBLISHED,
            sqlite_integer(published_at)?,
            tenant_id.as_str(),
            replay_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("replay_id"))
        .await
        .map_err(map_worker_error)?;
        if returned.is_some() {
            return Ok(());
        }

        let state = query!(
            &self.database,
            LOAD_REPLAY_DISPATCH_STATE,
            tenant_id.as_str(),
            replay_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<DispatchStateRow>(None)
        .await
        .map_err(map_worker_error)?;
        match state.as_ref().map(|row| row.dispatch_state.as_str()) {
            Some("PUBLISHED") => Ok(()),
            Some("PENDING") => Err(NotificationPortError::new(NotificationPortErrorClass::Conflict)),
            Some(_) => Err(integrity_failure()),
            None => Err(integrity_failure()),
        }
    }
}

impl NotificationRetentionRepositoryPort for D1NotificationOperationsRepository {
    async fn compact_operational_state(
        &self,
        before: UnixMillis,
        limit: u32,
    ) -> Result<NotificationRetentionOutcome, NotificationPortError> {
        let before = sqlite_integer(before)?;
        let limit = i64::from(limit);
        let deliveries_removed = delete_count(&self.database, DELETE_DELIVERED, before, limit).await?;
        let cursors_removed =
            delete_count(&self.database, DELETE_INACTIVE_CURSORS, before, limit).await?;
        let replay_dispatches_removed = delete_count(
            &self.database,
            DELETE_PUBLISHED_REPLAY_DISPATCHES,
            before,
            limit,
        )
        .await?;
        Ok(NotificationRetentionOutcome::new(
            deliveries_removed,
            cursors_removed,
            replay_dispatches_removed,
        ))
    }
}

impl NotificationOperationsRepositoryPort for D1NotificationOperationsRepository {
    async fn load_operations_snapshot(
        &self,
        actor: &ActorContext,
    ) -> Result<NotificationOperationsSnapshot, NotificationPortError> {
        let row = query!(
            &self.database,
            LOAD_OPERATIONS,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<OperationsRow>(None)
        .await
        .map_err(map_worker_error)?
        .ok_or_else(|| NotificationPortError::new(NotificationPortErrorClass::Conflict))?;
        let max_attempt_count = u16::try_from(non_negative(row.max_attempt_count)?)
            .map_err(|_| integrity_failure())?;
        if max_attempt_count > 64 {
            return Err(integrity_failure());
        }
        Ok(NotificationOperationsSnapshot::new(
            non_negative(row.ready_count)?,
            non_negative(row.retry_count)?,
            non_negative(row.delivered_count)?,
            non_negative(row.dead_letter_count)?,
            non_negative(row.pending_replay_count)?,
            max_attempt_count,
            row.oldest_open_created_at_ms
                .map(unix_from_i64)
                .transpose()?,
            non_negative(row.catch_up_lag_count)?,
        ))
    }
}

async fn delete_count(
    database: &D1Database,
    sql: &str,
    before: i64,
    limit: i64,
) -> Result<u32, NotificationPortError> {
    let rows = query!(database, sql, before, limit)
        .map_err(map_worker_error)?
        .all()
        .await
        .map_err(map_worker_error)?
        .results::<RemovedRow>()
        .map_err(map_worker_error)?;
    u32::try_from(rows.len()).map_err(|_| integrity_failure())
}

fn sqlite_integer(value: UnixMillis) -> Result<i64, NotificationPortError> {
    i64::try_from(value.value()).map_err(|_| integrity_failure())
}

fn unix_from_i64(value: i64) -> Result<UnixMillis, NotificationPortError> {
    u64::try_from(value)
        .map(UnixMillis::new)
        .map_err(|_| integrity_failure())
}

fn non_negative(value: i64) -> Result<u64, NotificationPortError> {
    u64::try_from(value).map_err(|_| integrity_failure())
}

const fn replay_reason_to_storage(reason: ReplayReasonClass) -> &'static str {
    match reason {
        ReplayReasonClass::DependencyRecovered => "DEPENDENCY_RECOVERED",
        ReplayReasonClass::OperatorRemediation => "OPERATOR_REMEDIATION",
        ReplayReasonClass::IntegrityRevalidated => "INTEGRITY_REVALIDATED",
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
    use super::replay_reason_to_storage;
    use application_ports::ReplayReasonClass;

    #[test]
    fn replay_reason_storage_is_bounded_and_payload_free() {
        assert_eq!(
            replay_reason_to_storage(ReplayReasonClass::DependencyRecovered),
            "DEPENDENCY_RECOVERED"
        );
        assert_eq!(
            replay_reason_to_storage(ReplayReasonClass::OperatorRemediation),
            "OPERATOR_REMEDIATION"
        );
        assert_eq!(
            replay_reason_to_storage(ReplayReasonClass::IntegrityRevalidated),
            "INTEGRITY_REVALIDATED"
        );
    }
}
