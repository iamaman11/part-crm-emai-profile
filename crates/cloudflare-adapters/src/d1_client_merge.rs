use crate::d1_command_identity::command_journal_id;
use application_ports::CommandExecutionEvidence;
use application_ports::client_merge::{ClientMergeApplicationPort, ClientMergeWrite};
use application_ports::clients::{
    ClientPortError, ClientPortErrorClass, ClientReplayDecision, ClientReplayReceipt,
};
use client_domain::{ClientKind, ClientRecord, ClientStatus};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, ClientId, TenantScope, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, Result as WorkerResult, query};

const CLIENT_MERGE_COMMAND: &str = r#"
INSERT INTO client_merge_commands (
    tenant_id, command_id, command_actor_id,
    source_client_id, target_client_id,
    expected_source_version, expected_target_version,
    reason, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, request_digest,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, 'client.merge', ?, 'merged', ?, ?, ?)
"#;

const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, 'client.merge', 'client', ?, 'merged', ?)
"#;

const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, 'client', ?, ?, 'client.merged.v1', ?, ?)
"#;

pub struct D1ClientMergeRepository {
    database: D1Database,
}

impl D1ClientMergeRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn load_client(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, ClientPortError> {
        let statement = query!(
            &self.database,
            r#"
            SELECT kind, display_name, status, version
            FROM clients
            WHERE tenant_id = ? AND client_id = ?
            "#,
            scope.tenant_id().as_str(),
            client_id.as_str()
        )
        .map_err(map_dependency_error)?;
        let row = statement
            .first::<ClientRow>(None)
            .await
            .map_err(map_dependency_error)?;
        row.map(|value| restore_client(scope, client_id, value))
            .transpose()
    }

    async fn has_active_assignment(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<bool, ClientPortError> {
        let statement = query!(
            &self.database,
            r#"
            SELECT 1 AS value
            FROM profile_client_assignments
            WHERE tenant_id = ?
              AND client_id = ?
              AND closed_at_ms IS NULL
            LIMIT 1
            "#,
            scope.tenant_id().as_str(),
            client_id.as_str()
        )
        .map_err(map_dependency_error)?;
        statement
            .first::<ExistsRow>(None)
            .await
            .map(|row| row.is_some())
            .map_err(map_dependency_error)
    }

    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientPortError> {
        let statement = query!(
            &self.database,
            r#"
            SELECT command_name, request_digest, result_code,
                   result_reference, expires_at_ms
            FROM idempotency_records
            WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            evidence.idempotency_key().as_str()
        )
        .map_err(map_dependency_error)?;
        let row = statement
            .first::<IdempotencyRow>(None)
            .await
            .map_err(map_dependency_error)?;

        let Some(row) = row else {
            return Ok(ClientReplayDecision::Miss);
        };
        let expires_at = u64::try_from(row.expires_at_ms).map_err(|_| integrity_failure())?;
        if row.command_name != command_name
            || row.request_digest != evidence.request_digest()
            || evidence.now().value() >= expires_at
        {
            return Ok(ClientReplayDecision::Conflict);
        }
        Ok(ClientReplayDecision::Replay(ClientReplayReceipt::new(
            row.result_code,
            row.result_reference,
        )))
    }

    async fn persist_merge_batch(
        &self,
        actor: &ActorContext,
        write: &ClientMergeWrite,
    ) -> WorkerResult<()> {
        let plan = write.plan();
        let evidence = write.evidence();
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let source_client_id = plan.source_client_id().as_str();
        let target_client_id = plan.target_client_id().as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            evidence.idempotency_key(),
        )?;
        let now = sqlite_integer(evidence.now())?;
        let expires_at = sqlite_integer(evidence.idempotency_expires_at())?;
        let expected_source_version = sqlite_version(plan.source_expected_version())?;
        let expected_target_version = sqlite_version(plan.target_expected_version())?;
        let source_next_version = sqlite_version(plan.source_next_version())?;

        let statements = vec![
            query!(
                &self.database,
                CLIENT_MERGE_COMMAND,
                tenant_id,
                command_id.as_str(),
                actor_id,
                source_client_id,
                target_client_id,
                expected_source_version,
                expected_target_version,
                write.reason(),
                now
            )?,
            query!(
                &self.database,
                IDEMPOTENCY_CREATE,
                tenant_id,
                actor_id,
                evidence.idempotency_key().as_str(),
                evidence.request_digest(),
                target_client_id,
                now,
                expires_at
            )?,
            query!(
                &self.database,
                AUDIT_CREATE,
                tenant_id,
                evidence.audit_event_id().as_str(),
                actor.correlation_id().as_str(),
                actor_id,
                source_client_id,
                now
            )?,
            query!(
                &self.database,
                OUTBOX_CREATE,
                tenant_id,
                evidence.outbox_event_id().as_str(),
                source_client_id,
                source_next_version,
                write.event_payload_json(),
                now
            )?,
        ];
        self.database.batch(statements).await.map(|_| ())
    }
}

impl ClientMergeApplicationPort for D1ClientMergeRepository {
    async fn load_client_for_merge(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, ClientPortError> {
        self.load_client(scope, client_id).await
    }

    async fn source_has_active_assignment(
        &self,
        scope: &TenantScope,
        source_client_id: &ClientId,
    ) -> Result<bool, ClientPortError> {
        self.has_active_assignment(scope, source_client_id).await
    }

    async fn decide_client_merge_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientPortError> {
        self.decide_replay(actor, command_name, evidence).await
    }

    async fn persist_client_merge(
        &self,
        actor: &ActorContext,
        write: &ClientMergeWrite,
    ) -> Result<(), ClientPortError> {
        self.persist_merge_batch(actor, write)
            .await
            .map_err(map_write_error)
    }
}

#[derive(Deserialize)]
struct ClientRow {
    kind: String,
    display_name: String,
    status: String,
    version: i64,
}

#[derive(Deserialize)]
struct ExistsRow {
    #[allow(dead_code)]
    value: i64,
}

#[derive(Deserialize)]
struct IdempotencyRow {
    command_name: String,
    request_digest: String,
    result_code: String,
    result_reference: Option<String>,
    expires_at_ms: i64,
}

fn restore_client(
    scope: &TenantScope,
    client_id: &ClientId,
    row: ClientRow,
) -> Result<ClientRecord, ClientPortError> {
    let kind = match row.kind.as_str() {
        "PERSON" => ClientKind::Person,
        "ORGANIZATION" => ClientKind::Organization,
        _ => return Err(integrity_failure()),
    };
    let status = match row.status.as_str() {
        "ACTIVE" => ClientStatus::Active,
        "ARCHIVED" => ClientStatus::Archived,
        "MERGED" => ClientStatus::Merged,
        _ => return Err(integrity_failure()),
    };
    let version = u64::try_from(row.version)
        .ok()
        .and_then(|value| AggregateVersion::new(value).ok())
        .ok_or_else(integrity_failure)?;
    ClientRecord::restore(
        scope.tenant_id().clone(),
        client_id.clone(),
        version,
        kind,
        row.display_name,
        status,
    )
    .map_err(|_| integrity_failure())
}

fn sqlite_integer(value: UnixMillis) -> WorkerResult<i64> {
    i64::try_from(value.value())
        .map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

fn sqlite_version(version: AggregateVersion) -> WorkerResult<i64> {
    i64::try_from(version.value())
        .map_err(|_| Error::RustError("aggregate version exceeds SQLite INTEGER".to_owned()))
}

fn map_write_error(error: Error) -> ClientPortError {
    ClientPortError::new(classify_write_failure(&error.to_string()))
}

fn map_dependency_error(_error: Error) -> ClientPortError {
    ClientPortError::new(ClientPortErrorClass::DependencyUnavailable)
}

const fn integrity_failure() -> ClientPortError {
    ClientPortError::new(ClientPortErrorClass::IntegrityFailure)
}

fn classify_write_failure(message: &str) -> ClientPortErrorClass {
    if message.contains("client_merge_owner_required")
        || message.contains("client_merge_source_version_or_state_mismatch")
        || message.contains("client_merge_target_version_or_state_mismatch")
        || message.contains("client_merge_time_regression")
        || message.contains("client_merge_contact_time_regression")
        || message.contains("client_merge_active_assignment_requires_reassignment")
        || message.contains("UNIQUE constraint failed")
    {
        return ClientPortErrorClass::Conflict;
    }
    if message.contains("client_merge_not_governed")
        || message.contains("client_merge_record_not_governed")
        || message.contains("client_merge_record_source_mismatch")
        || message.contains("client_merge_record_target_mismatch")
        || message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
    {
        return ClientPortErrorClass::IntegrityFailure;
    }
    if message.contains("exceeds SQLite INTEGER") {
        return ClientPortErrorClass::InternalFailure;
    }
    ClientPortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::classify_write_failure;
    use application_ports::clients::ClientPortErrorClass;

    #[test]
    fn client_merge_write_failures_are_sanitized_and_stable() {
        assert_eq!(
            classify_write_failure("client_merge_source_version_or_state_mismatch"),
            ClientPortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("client_merge_active_assignment_requires_reassignment"),
            ClientPortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("client_merge_record_source_mismatch"),
            ClientPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("network unavailable"),
            ClientPortErrorClass::DependencyUnavailable
        );
    }
}
