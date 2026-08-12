use profile_platform_primitives::{
    ActorContext, AggregateVersion, AuditEventId, ClientId, IdempotencyKey, OutboxEventId,
    TenantScope, UnixMillis,
};
use worker::d1::{D1Database, D1Result};
use worker::{Error, Result, query};

const CLIENT_CREATE: &str = r#"
INSERT INTO clients (
    tenant_id, client_id, kind, display_name, status, version,
    created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
) VALUES (?, ?, ?, ?, 'ACTIVE', 1, ?, ?, ?, ?)
"#;

const CLIENT_CREATOR_GRANT: &str = r#"
INSERT INTO client_grants (
    tenant_id, actor_id, client_id, role, granted_by_actor_id, reason, created_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, request_digest,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, 'client.create', ?, 'created', ?, ?, ?)
"#;

const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, 'client.create', 'client', ?, 'created', ?)
"#;

const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, 'client', ?, 1, 'client.created.v1', ?, ?)
"#;

const CLIENT_UPDATE_CAS: &str = r#"
UPDATE clients
SET display_name = ?,
    version = version + 1,
    updated_by_actor_id = ?,
    updated_at_ms = ?
WHERE tenant_id = ?
  AND client_id = ?
  AND version = ?
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogClientKind {
    Person,
    Organization,
}

impl CatalogClientKind {
    #[must_use]
    pub const fn database_value(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Organization => "ORGANIZATION",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogClientGrantRole {
    Viewer,
    Editor,
}

impl CatalogClientGrantRole {
    #[must_use]
    pub const fn database_value(self) -> &'static str {
        match self {
            Self::Viewer => "CLIENT_VIEWER",
            Self::Editor => "CLIENT_EDITOR",
        }
    }
}

pub struct CreateClientMutation<'a> {
    pub client_id: &'a ClientId,
    pub kind: CatalogClientKind,
    pub display_name: &'a str,
    pub creator_grant_role: CatalogClientGrantRole,
    pub creator_grant_reason: &'a str,
    pub idempotency_key: &'a IdempotencyKey,
    pub request_digest: &'a str,
    pub audit_event_id: &'a AuditEventId,
    pub outbox_event_id: &'a OutboxEventId,
    pub event_payload_json: &'a str,
    pub now: UnixMillis,
    pub idempotency_expires_at: UnixMillis,
}

pub struct UpdateClientMutation<'a> {
    pub client_id: &'a ClientId,
    pub expected_version: AggregateVersion,
    pub display_name: &'a str,
    pub now: UnixMillis,
}

pub struct D1CatalogRepository {
    database: D1Database,
}

impl D1CatalogRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn client_exists(&self, scope: &TenantScope, client_id: &ClientId) -> Result<bool> {
        let statement = query!(
            &self.database,
            "SELECT client_id FROM clients WHERE tenant_id = ? AND client_id = ?",
            scope.tenant_id().as_str(),
            client_id.as_str()
        )?;
        Ok(statement
            .first::<String>(Some("client_id"))
            .await?
            .is_some())
    }

    pub async fn client_version(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<i64>> {
        let statement = query!(
            &self.database,
            "SELECT version FROM clients WHERE tenant_id = ? AND client_id = ?",
            scope.tenant_id().as_str(),
            client_id.as_str()
        )?;
        statement.first::<i64>(Some("version")).await
    }

    pub async fn create_client(
        &self,
        actor: &ActorContext,
        mutation: CreateClientMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let now = sqlite_integer(mutation.now)?;
        let expires_at = sqlite_integer(mutation.idempotency_expires_at)?;
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();

        let statements = vec![
            query!(
                &self.database,
                CLIENT_CREATE,
                tenant_id,
                mutation.client_id.as_str(),
                mutation.kind.database_value(),
                mutation.display_name,
                actor_id,
                actor_id,
                now,
                now
            )?,
            query!(
                &self.database,
                CLIENT_CREATOR_GRANT,
                tenant_id,
                actor_id,
                mutation.client_id.as_str(),
                mutation.creator_grant_role.database_value(),
                actor_id,
                mutation.creator_grant_reason,
                now
            )?,
            query!(
                &self.database,
                IDEMPOTENCY_CREATE,
                tenant_id,
                actor_id,
                mutation.idempotency_key.as_str(),
                mutation.request_digest,
                mutation.client_id.as_str(),
                now,
                expires_at
            )?,
            query!(
                &self.database,
                AUDIT_CREATE,
                tenant_id,
                mutation.audit_event_id.as_str(),
                actor.correlation_id().as_str(),
                actor_id,
                mutation.client_id.as_str(),
                now
            )?,
            query!(
                &self.database,
                OUTBOX_CREATE,
                tenant_id,
                mutation.outbox_event_id.as_str(),
                mutation.client_id.as_str(),
                mutation.event_payload_json,
                now
            )?,
        ];

        self.database.batch(statements).await
    }

    pub async fn update_client_display_name_cas(
        &self,
        actor: &ActorContext,
        mutation: UpdateClientMutation<'_>,
    ) -> Result<D1Result> {
        let now = sqlite_integer(mutation.now)?;
        let expected_version = sqlite_integer_value(mutation.expected_version.value())?;
        let statement = query!(
            &self.database,
            CLIENT_UPDATE_CAS,
            mutation.display_name,
            actor.actor_id().as_str(),
            now,
            actor.tenant_scope().tenant_id().as_str(),
            mutation.client_id.as_str(),
            expected_version
        )?;
        statement.run().await
    }
}

fn sqlite_integer(value: UnixMillis) -> Result<i64> {
    sqlite_integer_value(value.value())
}

fn sqlite_integer_value(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}
