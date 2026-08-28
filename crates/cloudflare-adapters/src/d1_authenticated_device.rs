use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceJobPortError, DeviceJobPortErrorClass,
};
use application_ports::profile_launch::ProfileLaunchMachineBinding;
use profile_platform_primitives::{ActorContext, ActorId, DeviceId, TenantId};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::d1::D1Database;
use worker::query;

const MACHINE_EVIDENCE_PREFIX: &str = "cf_access_sub_sha256:";

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

    /// Resolve a cryptographically verified Cloudflare Access machine subject through the
    /// existing device-principal binding owner. Raw Access subjects/tokens are never persisted:
    /// only a domain-tagged SHA-256 evidence reference is compared in D1.
    pub async fn resolve_machine_subject(
        &self,
        verified_subject: &str,
    ) -> Result<Option<ProfileLaunchMachineBinding>, DeviceJobPortError> {
        if verified_subject.trim().is_empty() || verified_subject.len() > 512 {
            return Ok(None);
        }
        let evidence_reference = machine_evidence_reference(verified_subject);
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

fn machine_evidence_reference(subject: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(subject.as_bytes());
    let mut output = String::with_capacity(MACHINE_EVIDENCE_PREFIX.len() + digest.len() * 2);
    output.push_str(MACHINE_EVIDENCE_PREFIX);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
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
        machine_evidence_reference,
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
    fn machine_evidence_reference_is_domain_tagged_digest_not_raw_subject() {
        let subject = "service-token-machine-01";
        let reference = machine_evidence_reference(subject);
        assert!(reference.starts_with(MACHINE_EVIDENCE_PREFIX));
        assert_eq!(reference.len(), MACHINE_EVIDENCE_PREFIX.len() + 64);
        assert!(!reference.contains(subject));
        assert_eq!(reference, machine_evidence_reference(subject));
        assert_ne!(
            reference,
            machine_evidence_reference("service-token-machine-02")
        );
    }
}
