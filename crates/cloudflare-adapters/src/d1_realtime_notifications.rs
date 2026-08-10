use application_ports::{
    NotificationEventRecord, NotificationPortError, NotificationPortErrorClass,
    RealtimeNotificationAudiencePort, RealtimeNotificationAuthorizationPort,
};
use profile_platform_primitives::{ActorContext, ActorId, TenantId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

/// Mirrors the accepted Phase 1B catch-up authorization boundary for one exact durable event.
/// Tenant owners may observe tenant notification metadata. Members may observe only client/profile
/// events for resources with a current live grant. No historical assignment or notification state
/// is treated as authorization.
const LOAD_EVENT_AUTHORIZATION: &str = r#"
SELECT CASE
    WHEN membership.role = 'TENANT_OWNER' THEN 1
    WHEN membership.role = 'MEMBER'
      AND ? = 'client'
      AND EXISTS (
          SELECT 1
          FROM client_grants AS grant
          WHERE grant.tenant_id = membership.tenant_id
            AND grant.actor_id = membership.actor_id
            AND grant.client_id = ?
      ) THEN 1
    WHEN membership.role = 'MEMBER'
      AND ? = 'profile'
      AND EXISTS (
          SELECT 1
          FROM profile_grants AS grant
          WHERE grant.tenant_id = membership.tenant_id
            AND grant.actor_id = membership.actor_id
            AND grant.profile_id = ?
      ) THEN 1
    ELSE 0
END AS authorized
FROM memberships AS membership
WHERE membership.tenant_id = ?
  AND membership.actor_id = ?
  AND membership.status = 'ACTIVE'
"#;

const LOAD_AUTHORIZED_AUDIENCE: &str = r#"
SELECT membership.actor_id
FROM memberships AS membership
WHERE membership.tenant_id = ?
  AND membership.status = 'ACTIVE'
  AND membership.actor_id > ?
  AND (
      membership.role = 'TENANT_OWNER'
      OR (
          membership.role = 'MEMBER'
          AND ? = 'client'
          AND EXISTS (
              SELECT 1
              FROM client_grants AS grant
              WHERE grant.tenant_id = membership.tenant_id
                AND grant.actor_id = membership.actor_id
                AND grant.client_id = ?
          )
      )
      OR (
          membership.role = 'MEMBER'
          AND ? = 'profile'
          AND EXISTS (
              SELECT 1
              FROM profile_grants AS grant
              WHERE grant.tenant_id = membership.tenant_id
                AND grant.actor_id = membership.actor_id
                AND grant.profile_id = ?
          )
      )
  )
ORDER BY membership.actor_id ASC
LIMIT ?
"#;

#[derive(Deserialize)]
struct AuthorizationRow {
    authorized: i64,
}

#[derive(Deserialize)]
struct ActorRow {
    actor_id: String,
}

pub struct D1RealtimeNotificationAuthorization {
    database: D1Database,
}

impl D1RealtimeNotificationAuthorization {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl RealtimeNotificationAuthorizationPort for D1RealtimeNotificationAuthorization {
    async fn is_event_authorized(
        &self,
        actor: &ActorContext,
        event: &NotificationEventRecord,
    ) -> Result<bool, NotificationPortError> {
        let row = query!(
            &self.database,
            LOAD_EVENT_AUTHORIZATION,
            event.aggregate_type(),
            event.aggregate_id().as_str(),
            event.aggregate_type(),
            event.aggregate_id().as_str(),
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<AuthorizationRow>(None)
        .await
        .map_err(map_worker_error)?;

        match row.map(|value| value.authorized) {
            None | Some(0) => Ok(false),
            Some(1) => Ok(true),
            Some(_) => Err(integrity_failure()),
        }
    }
}

impl RealtimeNotificationAudiencePort for D1RealtimeNotificationAuthorization {
    async fn load_authorized_actor_page(
        &self,
        tenant_id: &TenantId,
        event: &NotificationEventRecord,
        after_actor_id: Option<&ActorId>,
        limit: u32,
    ) -> Result<Vec<ActorId>, NotificationPortError> {
        let after = after_actor_id.map_or("", ActorId::as_str);
        let rows = query!(
            &self.database,
            LOAD_AUTHORIZED_AUDIENCE,
            tenant_id.as_str(),
            after,
            event.aggregate_type(),
            event.aggregate_id().as_str(),
            event.aggregate_type(),
            event.aggregate_id().as_str(),
            i64::from(limit)
        )
        .map_err(map_worker_error)?
        .all()
        .await
        .map_err(map_worker_error)?
        .results::<ActorRow>()
        .map_err(map_worker_error)?;
        rows.into_iter()
            .map(|row| ActorId::parse(row.actor_id).map_err(|_| integrity_failure()))
            .collect()
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
    use super::{LOAD_AUTHORIZED_AUDIENCE, LOAD_EVENT_AUTHORIZATION};

    #[test]
    fn authorization_queries_are_live_grant_based_and_assignment_free() {
        for query in [LOAD_EVENT_AUTHORIZATION, LOAD_AUTHORIZED_AUDIENCE] {
            assert!(query.contains("membership.status = 'ACTIVE'"));
            assert!(query.contains("client_grants"));
            assert!(query.contains("profile_grants"));
            assert!(!query.contains("assignment"));
            assert!(!query.contains("notification_events"));
        }
    }

    #[test]
    fn audience_query_is_stably_paged_by_actor_id() {
        assert!(LOAD_AUTHORIZED_AUDIENCE.contains("membership.actor_id > ?"));
        assert!(LOAD_AUTHORIZED_AUDIENCE.contains("ORDER BY membership.actor_id ASC"));
        assert!(LOAD_AUTHORIZED_AUDIENCE.contains("LIMIT ?"));
    }
}
