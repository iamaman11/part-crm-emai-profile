use application_ports::{
    NotificationEventRecord, NotificationPortError, NotificationPortErrorClass,
    RealtimeNotificationAuthorizationPort,
};
use profile_platform_primitives::ActorContext;
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

#[derive(Deserialize)]
struct AuthorizationRow {
    authorized: i64,
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

fn integrity_failure() -> NotificationPortError {
    NotificationPortError::new(NotificationPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> NotificationPortError {
    NotificationPortError::new(NotificationPortErrorClass::InternalFailure)
}

#[cfg(test)]
mod tests {
    use super::LOAD_EVENT_AUTHORIZATION;

    #[test]
    fn authorization_query_is_live_grant_based_and_assignment_free() {
        assert!(LOAD_EVENT_AUTHORIZATION.contains("membership.status = 'ACTIVE'"));
        assert!(LOAD_EVENT_AUTHORIZATION.contains("client_grants"));
        assert!(LOAD_EVENT_AUTHORIZATION.contains("profile_grants"));
        assert!(!LOAD_EVENT_AUTHORIZATION.contains("assignment"));
        assert!(!LOAD_EVENT_AUTHORIZATION.contains("notification_events"));
    }
}
