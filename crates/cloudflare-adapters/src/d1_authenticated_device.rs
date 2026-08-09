use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceJobPortError, DeviceJobPortErrorClass,
};
use profile_platform_primitives::{ActorContext, DeviceId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_ACTIVE_DEVICE_BINDING: &str = r#"
SELECT binding.device_id, binding.version
FROM device_actor_bindings AS binding
JOIN memberships AS membership
  ON membership.tenant_id = binding.tenant_id
 AND membership.actor_id = binding.actor_id
 AND membership.status = 'ACTIVE'
WHERE binding.tenant_id = ?
  AND binding.actor_id = ?
  AND binding.status = 'ACTIVE'
ORDER BY binding.version DESC
LIMIT 2
"#;

#[derive(Deserialize)]
struct DeviceBindingRow {
    device_id: String,
    version: i64,
}

pub struct D1AuthenticatedDevice {
    database: D1Database,
}

impl D1AuthenticatedDevice {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl AuthenticatedDevicePort for D1AuthenticatedDevice {
    async fn authenticated_device_id(
        &self,
        actor: &ActorContext,
    ) -> Result<DeviceId, DeviceJobPortError> {
        let result = query!(
            &self.database,
            LOAD_ACTIVE_DEVICE_BINDING,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str()
        )
        .map_err(map_worker_error)?
        .all()
        .await
        .map_err(map_worker_error)?;
        let rows = result
            .results::<DeviceBindingRow>()
            .map_err(map_worker_error)?;
        let [row] = rows.as_slice() else {
            return if rows.is_empty() {
                Err(authentication_failed())
            } else {
                Err(integrity_failure())
            };
        };
        let version = u64::try_from(row.version).map_err(|_| integrity_failure())?;
        if version == 0 {
            return Err(integrity_failure());
        }
        DeviceId::parse(row.device_id.as_str()).map_err(|_| integrity_failure())
    }
}

fn authentication_failed() -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::AuthenticationFailed)
}

fn integrity_failure() -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::LOAD_ACTIVE_DEVICE_BINDING;

    #[test]
    fn device_identity_query_is_actor_scoped_and_rechecks_live_membership() {
        for required in [
            "binding.tenant_id = ?",
            "binding.actor_id = ?",
            "binding.status = 'ACTIVE'",
            "membership.status = 'ACTIVE'",
            "LIMIT 2",
        ] {
            assert!(LOAD_ACTIVE_DEVICE_BINDING.contains(required));
        }
        assert!(!LOAD_ACTIVE_DEVICE_BINDING.contains("X-Device-Id"));
        assert!(!LOAD_ACTIVE_DEVICE_BINDING.contains("device_authorizations"));
    }
}
