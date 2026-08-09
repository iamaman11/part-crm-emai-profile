use crate::d1_command_identity::command_journal_id;
use application_ports::CommandExecutionEvidence;
use application_ports::clients::{
    ArchiveContactWrite, ClientLifecycleApplicationPort, ClientLifecycleWrite, ClientPortError,
    ClientPortErrorClass, ClientReplayDecision, ClientReplayReceipt,
    ProtectedClientContactRepositoryPort, ProtectedContactWrite,
};
use client_domain::{ClientKind, ClientRecord, ClientStatus};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, ClientId, TenantScope, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, Result as WorkerResult, query};

const CLIENT_LIFECYCLE_COMMAND: &str = r#"
INSERT INTO client_lifecycle_commands (
    tenant_id, command_id, command_actor_id, client_id,
    operation, expected_client_version, next_display_name, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#;

const CLIENT_CONTACT_UPSERT_COMMAND: &str = r#"
INSERT INTO client_contact_commands (
    tenant_id, command_id, command_actor_id, client_id, contact_point_id,
    operation, kind, expected_client_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, 'UPSERT', ?, ?, ?)
"#;

const CLIENT_CONTACT_ARCHIVE_COMMAND: &str = r#"
INSERT INTO client_contact_commands (
    tenant_id, command_id, command_actor_id, client_id, contact_point_id,
    operation, kind, expected_client_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, 'ARCHIVE', ?, ?, ?)
"#;

const CONTACT_UPSERT: &str = r#"
INSERT INTO client_contact_points (
    tenant_id, client_id, contact_point_id, kind, status,
    normalization_version, protection_version,
    ciphertext, nonce, encryption_key_version,
    exact_lookup_token, lookup_key_version,
    created_by_actor_id, updated_by_actor_id,
    created_at_ms, updated_at_ms
) VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (tenant_id, contact_point_id) DO UPDATE SET
    status = 'ACTIVE',
    normalization_version = excluded.normalization_version,
    protection_version = excluded.protection_version,
    ciphertext = excluded.ciphertext,
    nonce = excluded.nonce,
    encryption_key_version = excluded.encryption_key_version,
    exact_lookup_token = excluded.exact_lookup_token,
    lookup_key_version = excluded.lookup_key_version,
    updated_by_actor_id = excluded.updated_by_actor_id,
    updated_at_ms = excluded.updated_at_ms
"#;

const CONTACT_ARCHIVE: &str = r#"
UPDATE client_contact_points
SET status = 'ARCHIVED',
    updated_by_actor_id = ?,
    updated_at_ms = ?
WHERE tenant_id = ?
  AND client_id = ?
  AND contact_point_id = ?
  AND kind = ?
  AND status = 'ACTIVE'
"#;

const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, request_digest,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#;

pub struct D1ClientPersistenceRepository {
    database: D1Database,
}

impl D1ClientPersistenceRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
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

    async fn persist_lifecycle_batch(
        &self,
        actor: &ActorContext,
        write: &ClientLifecycleWrite,
    ) -> WorkerResult<()> {
        let evidence = write.evidence();
        let (operation, command_name, result_code, event_type, next_display_name) =
            lifecycle_metadata(write.client())?;
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            evidence.idempotency_key(),
        )?;
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let client_id = write.client().client_id().as_str();
        let now = sqlite_integer(evidence.now())?;
        let expires_at = sqlite_integer(evidence.idempotency_expires_at())?;
        let expected_version = sqlite_version(write.expected_version())?;
        let aggregate_version = sqlite_version(write.client().version())?;

        let statements = vec![
            query!(
                &self.database,
                CLIENT_LIFECYCLE_COMMAND,
                tenant_id,
                command_id.as_str(),
                actor_id,
                client_id,
                operation,
                expected_version,
                next_display_name,
                now
            )?,
            idempotency_statement(
                &self.database,
                actor,
                command_name,
                result_code,
                client_id,
                evidence,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                actor,
                command_name,
                "client",
                client_id,
                result_code,
                evidence,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "client",
                client_id,
                aggregate_version,
                event_type,
                write.event_payload_json(),
                evidence,
                now,
            )?,
        ];
        self.database.batch(statements).await.map(|_| ())
    }

    async fn persist_contact_batch(
        &self,
        actor: &ActorContext,
        write: &ProtectedContactWrite,
    ) -> WorkerResult<()> {
        let evidence = write.evidence();
        let protected = write.contact();
        let encrypted = protected.display_value();
        let exact_lookup = protected.exact_lookup();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            evidence.idempotency_key(),
        )?;
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let client_id = write.client_id().as_str();
        let contact_point_id = protected.contact_point_id().as_str();
        let now = sqlite_integer(evidence.now())?;
        let expires_at = sqlite_integer(evidence.idempotency_expires_at())?;
        let expected_version = sqlite_version(write.expected_client_version())?;
        let aggregate_version = sqlite_next_version(write.expected_client_version())?;
        let encryption_key_version = i64::from(encrypted.key_version().value());
        let lookup_key_version = i64::from(exact_lookup.key_version().value());
        let normalization_version = i64::from(protected.normalization_version().value());
        let protection_version = i64::from(protected.protection_version().value());

        let statements = vec![
            query!(
                &self.database,
                CLIENT_CONTACT_UPSERT_COMMAND,
                tenant_id,
                command_id.as_str(),
                actor_id,
                client_id,
                contact_point_id,
                protected.kind().stable_code(),
                expected_version,
                now
            )?,
            query!(
                &self.database,
                CONTACT_UPSERT,
                tenant_id,
                client_id,
                contact_point_id,
                protected.kind().stable_code(),
                normalization_version,
                protection_version,
                encrypted.ciphertext(),
                encrypted.nonce(),
                encryption_key_version,
                exact_lookup.bytes().as_slice(),
                lookup_key_version,
                actor_id,
                actor_id,
                now,
                now
            )?,
            idempotency_statement(
                &self.database,
                actor,
                "client.contact_upsert",
                "contact_saved",
                contact_point_id,
                evidence,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                actor,
                "client.contact_upsert",
                "client_contact",
                contact_point_id,
                "contact_saved",
                evidence,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "client",
                client_id,
                aggregate_version,
                "client.contact_saved.v1",
                write.event_payload_json(),
                evidence,
                now,
            )?,
        ];
        self.database.batch(statements).await.map(|_| ())
    }

    async fn archive_contact_batch(
        &self,
        actor: &ActorContext,
        write: &ArchiveContactWrite,
    ) -> WorkerResult<()> {
        let evidence = write.evidence();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            evidence.idempotency_key(),
        )?;
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let client_id = write.client_id().as_str();
        let contact_point_id = write.contact_point_id().as_str();
        let now = sqlite_integer(evidence.now())?;
        let expires_at = sqlite_integer(evidence.idempotency_expires_at())?;
        let expected_version = sqlite_version(write.expected_client_version())?;
        let aggregate_version = sqlite_next_version(write.expected_client_version())?;

        let statements = vec![
            query!(
                &self.database,
                CLIENT_CONTACT_ARCHIVE_COMMAND,
                tenant_id,
                command_id.as_str(),
                actor_id,
                client_id,
                contact_point_id,
                write.kind().stable_code(),
                expected_version,
                now
            )?,
            query!(
                &self.database,
                CONTACT_ARCHIVE,
                actor_id,
                now,
                tenant_id,
                client_id,
                contact_point_id,
                write.kind().stable_code()
            )?,
            idempotency_statement(
                &self.database,
                actor,
                "client.contact_archive",
                "contact_archived",
                contact_point_id,
                evidence,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                actor,
                "client.contact_archive",
                "client_contact",
                contact_point_id,
                "contact_archived",
                evidence,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "client",
                client_id,
                aggregate_version,
                "client.contact_archived.v1",
                write.event_payload_json(),
                evidence,
                now,
            )?,
        ];
        self.database.batch(statements).await.map(|_| ())
    }
}

impl ClientLifecycleApplicationPort for D1ClientPersistenceRepository {
    async fn load_client_for_mutation(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, ClientPortError> {
        self.load_client(scope, client_id).await
    }

    async fn decide_client_lifecycle_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientPortError> {
        self.decide_replay(actor, command_name, evidence).await
    }

    async fn persist_client_lifecycle(
        &self,
        actor: &ActorContext,
        write: &ClientLifecycleWrite,
    ) -> Result<(), ClientPortError> {
        self.persist_lifecycle_batch(actor, write)
            .await
            .map_err(map_write_error)
    }
}

impl ProtectedClientContactRepositoryPort for D1ClientPersistenceRepository {
    type Error = ClientPortError;

    async fn load_client_for_contact_mutation(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, Self::Error> {
        self.load_client(scope, client_id).await
    }

    async fn decide_client_contact_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, Self::Error> {
        self.decide_replay(actor, command_name, evidence).await
    }

    async fn persist_protected_contact(
        &self,
        actor: &ActorContext,
        write: &ProtectedContactWrite,
    ) -> Result<(), Self::Error> {
        self.persist_contact_batch(actor, write)
            .await
            .map_err(map_write_error)
    }

    async fn archive_contact(
        &self,
        actor: &ActorContext,
        write: &ArchiveContactWrite,
    ) -> Result<(), Self::Error> {
        self.archive_contact_batch(actor, write)
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

fn lifecycle_metadata(
    client: &ClientRecord,
) -> WorkerResult<(&'static str, &'static str, &'static str, &'static str, Option<&str>)> {
    match client.status() {
        ClientStatus::Active => Ok((
            "UPDATE",
            "client.update",
            "updated",
            "client.updated.v1",
            Some(client.display_name()),
        )),
        ClientStatus::Archived => Ok((
            "ARCHIVE",
            "client.archive",
            "archived",
            "client.archived.v1",
            None,
        )),
        ClientStatus::Merged => Err(Error::RustError(
            "merged client lifecycle belongs to Phase 2C".to_owned(),
        )),
    }
}

fn idempotency_statement(
    database: &D1Database,
    actor: &ActorContext,
    command_name: &str,
    result_code: &str,
    result_reference: &str,
    evidence: &CommandExecutionEvidence,
    now: i64,
    expires_at: i64,
) -> WorkerResult<worker::d1::D1PreparedStatement> {
    query!(
        database,
        IDEMPOTENCY_CREATE,
        actor.tenant_scope().tenant_id().as_str(),
        actor.actor_id().as_str(),
        evidence.idempotency_key().as_str(),
        command_name,
        evidence.request_digest(),
        result_code,
        result_reference,
        now,
        expires_at
    )
}

#[allow(clippy::too_many_arguments)]
fn audit_statement(
    database: &D1Database,
    actor: &ActorContext,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    result_code: &str,
    evidence: &CommandExecutionEvidence,
    now: i64,
) -> WorkerResult<worker::d1::D1PreparedStatement> {
    query!(
        database,
        AUDIT_CREATE,
        actor.tenant_scope().tenant_id().as_str(),
        evidence.audit_event_id().as_str(),
        actor.correlation_id().as_str(),
        actor.actor_id().as_str(),
        action,
        resource_type,
        resource_id,
        result_code,
        now
    )
}

#[allow(clippy::too_many_arguments)]
fn outbox_statement(
    database: &D1Database,
    tenant_id: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    aggregate_version: i64,
    event_type: &str,
    payload_json: &str,
    evidence: &CommandExecutionEvidence,
    now: i64,
) -> WorkerResult<worker::d1::D1PreparedStatement> {
    query!(
        database,
        OUTBOX_CREATE,
        tenant_id,
        evidence.outbox_event_id().as_str(),
        aggregate_type,
        aggregate_id,
        aggregate_version,
        event_type,
        payload_json,
        now
    )
}

fn sqlite_integer(value: UnixMillis) -> WorkerResult<i64> {
    i64::try_from(value.value())
        .map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

fn sqlite_version(version: AggregateVersion) -> WorkerResult<i64> {
    i64::try_from(version.value())
        .map_err(|_| Error::RustError("aggregate version exceeds SQLite INTEGER".to_owned()))
}

fn sqlite_next_version(version: AggregateVersion) -> WorkerResult<i64> {
    let next = version
        .next()
        .map_err(|_| Error::RustError("aggregate version overflow".to_owned()))?;
    sqlite_version(next)
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
    if message.contains("version_mismatch") {
        return ClientPortErrorClass::VersionConflict;
    }
    if message.contains("owner_required")
        || message.contains("time_regression")
        || message.contains("archived_immutable")
        || message.contains("contact_missing")
        || message.contains("identity_mismatch")
        || message.contains("UNIQUE constraint failed")
    {
        return ClientPortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("client_contact_delete_forbidden")
    {
        return ClientPortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("exceeds SQLite INTEGER")
        || message.contains("merged client lifecycle")
    {
        return ClientPortErrorClass::InternalFailure;
    }
    ClientPortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::classify_write_failure;
    use application_ports::clients::ClientPortErrorClass;

    #[test]
    fn protected_client_write_failures_are_sanitized_and_stable() {
        assert_eq!(
            classify_write_failure("client_contact_client_version_mismatch"),
            ClientPortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_write_failure("client_contact_archived_immutable"),
            ClientPortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("CHECK constraint failed: exact_lookup_token"),
            ClientPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("aggregate version overflow"),
            ClientPortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_write_failure("network request failed"),
            ClientPortErrorClass::DependencyUnavailable
        );
    }
}