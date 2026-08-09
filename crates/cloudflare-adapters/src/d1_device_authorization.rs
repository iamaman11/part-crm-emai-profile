use application_ports::{
    DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobPortError, DeviceJobPortErrorClass,
};
use device_domain::DeviceJobTarget;
use profile_platform_primitives::ActorContext;
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_AUTHORIZATION: &str = r#"
SELECT authorization.version AS grant_version, membership.role
FROM device_authorizations AS authorization
JOIN memberships AS membership
  ON membership.tenant_id = authorization.tenant_id
 AND membership.actor_id = ?
 AND membership.status = 'ACTIVE'
WHERE authorization.tenant_id = ?
  AND authorization.device_id = ?
  AND authorization.profile_id = ?
  AND authorization.generation_id = ?
  AND authorization.status = 'ACTIVE'
  AND (
      membership.role = 'TENANT_OWNER'
      OR (
          membership.role = 'MEMBER'
          AND EXISTS (
              SELECT 1
              FROM profile_grants AS grant
              WHERE grant.tenant_id = authorization.tenant_id
                AND grant.profile_id = authorization.profile_id
                AND grant.actor_id = membership.actor_id
          )
      )
  )
"#;

#[derive(Deserialize)]
struct AuthorizationRow {
    grant_version: i64,
    role: String,
}

pub struct D1DeviceJobAuthorization {
    database: D1Database,
}

impl D1DeviceJobAuthorization {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl DeviceJobAuthorizationPort for D1DeviceJobAuthorization {
    async fn is_device_job_authorized(
        &self,
        actor: &ActorContext,
        target: &DeviceJobTarget,
        capability: DeviceJobCapability,
    ) -> Result<bool, DeviceJobPortError> {
        if actor.tenant_scope().tenant_id() != target.tenant_id() {
            return Ok(false);
        }
        let row = query!(
            &self.database,
            LOAD_AUTHORIZATION,
            actor.actor_id().as_str(),
            target.tenant_id().as_str(),
            target.device_id().as_str(),
            target.profile_id().as_str(),
            target.generation_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<AuthorizationRow>(None)
        .await
        .map_err(map_worker_error)?;
        let Some(row) = row else {
            return Ok(false);
        };
        let version = u64::try_from(row.grant_version).map_err(|_| integrity_failure())?;
        if version == 0 || !matches!(row.role.as_str(), "TENANT_OWNER" | "MEMBER") {
            return Err(integrity_failure());
        }
        match capability {
            DeviceJobCapability::Issue
            | DeviceJobCapability::Claim
            | DeviceJobCapability::Heartbeat
            | DeviceJobCapability::Complete
            | DeviceJobCapability::Recover
            | DeviceJobCapability::Cancel => Ok(true),
        }
    }
}

fn integrity_failure() -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::LOAD_AUTHORIZATION;

    #[test]
    fn authorization_query_is_tenant_profile_grant_and_device_scoped() {
        for required in [
            "authorization.tenant_id = ?",
            "authorization.device_id = ?",
            "authorization.profile_id = ?",
            "authorization.generation_id = ?",
            "authorization.status = 'ACTIVE'",
            "membership.status = 'ACTIVE'",
            "profile_grants",
        ] {
            assert!(LOAD_AUTHORIZATION.contains(required));
        }
        assert!(!LOAD_AUTHORIZATION.contains("profile_assignments"));
    }
}
