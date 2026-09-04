use crate::d1_command_identity::command_journal_id;
use crate::d1_identity_acl::MutationEnvelope;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, DeviceId, MachineCertificateFingerprint,
};
use worker::d1::{D1Database, D1Result};
use worker::{Error, Result, query};

const DEVICE_BIND_COMMAND: &str = r#"
INSERT INTO device_binding_bind_commands (
    tenant_id, command_id, command_actor_id, target_actor_id, device_id,
    certificate_sha256, expected_previous_version, next_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const DEVICE_REVOKE_COMMAND: &str = r#"
INSERT INTO device_binding_revoke_commands (
    tenant_id, command_id, command_actor_id, target_actor_id,
    expected_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?)
"#;

const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, payload_fingerprint,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, ?, 'device_binding', ?, ?, ?)
"#;

const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, 'device_binding', ?, ?, ?, ?, ?)
"#;

pub struct DeviceBindingMutation<'a> {
    pub target_actor_id: &'a ActorId,
    pub device_id: &'a DeviceId,
    pub certificate_fingerprint: &'a MachineCertificateFingerprint,
    pub expected_previous_version: Option<AggregateVersion>,
    pub next_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

pub struct DeviceBindingRevokeMutation<'a> {
    pub target_actor_id: &'a ActorId,
    pub expected_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

pub struct D1DeviceBindingGovernanceRepository {
    database: D1Database,
}

impl D1DeviceBindingGovernanceRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn bind(
        &self,
        actor: &ActorContext,
        mutation: DeviceBindingMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let command_actor_id = actor.actor_id().as_str();
        let target_actor_id = mutation.target_actor_id.as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            mutation.envelope.idempotency_key,
        )?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_previous_version = mutation
            .expected_previous_version
            .map(sqlite_version)
            .transpose()?;
        let next_version = sqlite_version(mutation.next_version)?;
        let statements = vec![
            query!(
                &self.database,
                DEVICE_BIND_COMMAND,
                tenant_id,
                command_id.as_str(),
                command_actor_id,
                target_actor_id,
                mutation.device_id.as_str(),
                mutation.certificate_fingerprint.as_str(),
                expected_previous_version,
                next_version,
                now
            )?,
            query!(
                &self.database,
                IDEMPOTENCY_CREATE,
                tenant_id,
                command_actor_id,
                mutation.envelope.idempotency_key.as_str(),
                "device.binding.bind",
                mutation.envelope.payload_fingerprint.as_str(),
                "bound",
                target_actor_id,
                now,
                expires_at
            )?,
            query!(
                &self.database,
                AUDIT_CREATE,
                tenant_id,
                mutation.envelope.audit_event_id.as_str(),
                actor.correlation_id().as_str(),
                command_actor_id,
                "device.binding.bind",
                target_actor_id,
                "bound",
                now
            )?,
            query!(
                &self.database,
                OUTBOX_CREATE,
                tenant_id,
                mutation.envelope.outbox_event_id.as_str(),
                target_actor_id,
                next_version,
                "device.binding.bound.v1",
                mutation.envelope.payload_json,
                now
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn revoke(
        &self,
        actor: &ActorContext,
        mutation: DeviceBindingRevokeMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let command_actor_id = actor.actor_id().as_str();
        let target_actor_id = mutation.target_actor_id.as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            mutation.envelope.idempotency_key,
        )?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_version)?;
        let statements = vec![
            query!(
                &self.database,
                DEVICE_REVOKE_COMMAND,
                tenant_id,
                command_id.as_str(),
                command_actor_id,
                target_actor_id,
                expected_version,
                now
            )?,
            query!(
                &self.database,
                IDEMPOTENCY_CREATE,
                tenant_id,
                command_actor_id,
                mutation.envelope.idempotency_key.as_str(),
                "device.binding.revoke",
                mutation.envelope.payload_fingerprint.as_str(),
                "revoked",
                target_actor_id,
                now,
                expires_at
            )?,
            query!(
                &self.database,
                AUDIT_CREATE,
                tenant_id,
                mutation.envelope.audit_event_id.as_str(),
                actor.correlation_id().as_str(),
                command_actor_id,
                "device.binding.revoke",
                target_actor_id,
                "revoked",
                now
            )?,
            query!(
                &self.database,
                OUTBOX_CREATE,
                tenant_id,
                mutation.envelope.outbox_event_id.as_str(),
                target_actor_id,
                expected_version,
                "device.binding.revoked.v1",
                mutation.envelope.payload_json,
                now
            )?,
        ];
        self.database.batch(statements).await
    }
}

fn sqlite_version(value: AggregateVersion) -> Result<i64> {
    sqlite_integer(value.value())
}

fn sqlite_integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{AUDIT_CREATE, DEVICE_BIND_COMMAND, DEVICE_REVOKE_COMMAND, OUTBOX_CREATE};

    #[test]
    fn device_binding_adapter_writes_only_governed_command_and_metadata_evidence() {
        for required in [
            "device_binding_bind_commands",
            "certificate_sha256",
            "expected_previous_version",
            "next_version",
        ] {
            assert!(DEVICE_BIND_COMMAND.contains(required));
        }
        assert!(DEVICE_REVOKE_COMMAND.contains("device_binding_revoke_commands"));
        assert!(AUDIT_CREATE.contains("device_binding"));
        assert!(OUTBOX_CREATE.contains("device_binding"));
        for forbidden in ["private_key", "certificate_pem", "certificate_der", "pfx", "pkcs12"] {
            assert!(!DEVICE_BIND_COMMAND.contains(forbidden));
        }
    }
}
