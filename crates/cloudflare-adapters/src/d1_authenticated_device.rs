use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceJobPortError, DeviceJobPortErrorClass,
};
use application_ports::profile_launch::ProfileLaunchMachineBinding;
use profile_platform_primitives::{ActorContext, ActorId, DeviceId, TenantId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const MACHINE_EVIDENCE_PREFIX: &str = "mtls_cert_sha256:";

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

const LOAD_ACTIVE_MACHINE_BINDING: &str = r#"
SELECT binding.tenant_id, binding.actor_id, binding.device_id, binding.version
FROM device_actor_bindings AS binding
JOIN memberships AS membership
  ON membership.tenant_id = binding.tenant_id
 AND membership.actor_id = binding.actor_id
 AND membership.status = 'ACTIVE'
WHERE binding.evidence_reference = ?
  AND binding.status = 'ACTIVE'
ORDER BY binding.version DESC
LIMIT 2
"#;

#[derive(Deserialize)]
struct DeviceBindingRow {
    device_id: String,
    version: i64,
}

#[derive(Deserialize)]
struct MachineBindingRow {
    tenant_id: String,
    actor_id: String,
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

    /// Resolve an edge-verified mTLS client certificate through the existing device-principal
    /// binding owner. The evidence reference is only the domain-tagged SHA-256 certificate
    /// fingerprint; raw certificate material and private keys never enter D1 or this adapter.
    pub async fn resolve_machine_certificate_fingerprint(
        &self,
        verified_fingerprint_sha256: &str,
    ) -> Result<Option<ProfileLaunchMachineBinding>, DeviceJobPortError> {
        if !valid_sha256_fingerprint(verified_fingerprint_sha256) {
            return Ok(None);
        }
        let evidence_reference = format!("{MACHINE_EVIDENCE_PREFIX}{verified_fingerprint_sha256}");
        let result = query!(
            &self.database,
            LOAD_ACTIVE_MACHINE_BINDING,
            evidence_reference.as_str(),
        )
        .map_err(map_worker_error)?
        .all()
        .await
        .map_err(map_worker_error)?;
        let rows = result
            .results::<MachineBindingRow>()
            .map_err(map_worker_error)?;
        let [row] = rows.as_slice() else {
            return if rows.is_empty() {
                Ok(None)
            } else {
                Err(integrity_failure())
            };
        };
        let version = u64::try_from(row.version).map_err(|_| integrity_failure())?;
        if version == 0 {
            return Err(integrity_failure());
        }
        Ok(Some(ProfileLaunchMachineBinding::new(
            TenantId::parse(row.tenant_id.clone()).map_err(|_| integrity_failure())?,
            ActorId::parse(row.actor_id.clone()).map_err(|_| integrity_failure())?,
            DeviceId::parse(row.device_id.clone()).map_err(|_| integrity_failure())?,
        )))
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

fn valid_sha256_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
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
    use super::{
        LOAD_ACTIVE_DEVICE_BINDING, LOAD_ACTIVE_MACHINE_BINDING, MACHINE_EVIDENCE_PREFIX,
        valid_sha256_fingerprint,
    };

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

    #[test]
    fn machine_identity_reuses_existing_active_device_binding_owner() {
        for required in [
            "binding.evidence_reference = ?",
            "binding.status = 'ACTIVE'",
            "membership.status = 'ACTIVE'",
            "LIMIT 2",
        ] {
            assert!(LOAD_ACTIVE_MACHINE_BINDING.contains(required));
        }
        assert!(!LOAD_ACTIVE_MACHINE_BINDING.contains("profile_launch_claims"));
        assert!(!LOAD_ACTIVE_MACHINE_BINDING.contains("device_authorizations"));
    }

    #[test]
    fn machine_evidence_is_domain_tagged_verified_certificate_fingerprint() {
        assert_eq!(MACHINE_EVIDENCE_PREFIX, "mtls_cert_sha256:");
        assert!(valid_sha256_fingerprint(&"a1".repeat(32)));
        assert!(!valid_sha256_fingerprint(&"A1".repeat(32)));
        assert!(!valid_sha256_fingerprint(&"a".repeat(63)));
        assert!(!valid_sha256_fingerprint(&"g".repeat(64)));
    }
}
