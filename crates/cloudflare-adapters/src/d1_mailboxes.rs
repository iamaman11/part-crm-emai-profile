use crate::d1_command_identity::command_journal_id;
use crate::d1_identity_acl::MutationEnvelope;
use application_ports::mailbox_jobs::MailboxJobPreparedRun;
use mailbox_domain::{
    MailboxBinding, MailboxBindingStatus, MailboxJob, MailboxJobRestore, MailboxJobStatus,
    MailboxProvider, validate_cursor, validate_provider_status,
};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, MailboxJobId, SecretHandle, TenantScope,
    UnixMillis,
};
use serde::Deserialize;
use worker::d1::{D1Database, D1PreparedStatement, D1Result};
use worker::{Error, Result, query};

const BINDING_CREATE_COMMAND: &str = r#"
INSERT INTO mailbox_binding_create_commands (
    tenant_id, command_id, command_actor_id, binding_id, provider, secret_handle, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const BINDING_REVOKE_COMMAND: &str = r#"
INSERT INTO mailbox_binding_revoke_commands (
    tenant_id, command_id, command_actor_id, binding_id, expected_binding_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?)
"#;

const JOB_CREATE_COMMAND: &str = r#"
INSERT INTO mailbox_job_create_commands (
    tenant_id, command_id, command_actor_id, binding_id, job_id,
    cursor, scheduled_at_ms, max_attempts, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const JOB_RUN_COMMAND: &str = r#"
INSERT INTO mailbox_job_run_commands_v2 (
    tenant_id, command_id, command_actor_id, binding_id, job_id,
    expected_job_version, outcome_status, next_cursor, provider_status,
    bounded_item_count, retry_at_ms, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

pub struct CreateMailboxBindingMutation<'a> {
    pub binding_id: &'a MailboxBindingId,
    pub provider: MailboxProvider,
    pub secret_handle: &'a SecretHandle,
    pub envelope: MutationEnvelope<'a>,
}

pub struct RevokeMailboxBindingMutation<'a> {
    pub binding_id: &'a MailboxBindingId,
    pub expected_binding_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

pub struct CreateMailboxJobMutation<'a> {
    pub binding_id: &'a MailboxBindingId,
    pub job_id: &'a MailboxJobId,
    pub cursor: Option<&'a str>,
    pub scheduled_at: UnixMillis,
    pub max_attempts: u32,
    pub envelope: MutationEnvelope<'a>,
}

pub struct RunMailboxJobMutation<'a> {
    pub binding_id: &'a MailboxBindingId,
    pub job_id: &'a MailboxJobId,
    pub expected_job_version: AggregateVersion,
    pub prepared: &'a MailboxJobPreparedRun,
    pub envelope: MutationEnvelope<'a>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobProjection {
    job: MailboxJob,
    provider_status: Option<String>,
    bounded_item_count: u32,
}

impl MailboxJobProjection {
    #[must_use]
    pub const fn job(&self) -> &MailboxJob {
        &self.job
    }

    #[must_use]
    pub fn provider_status(&self) -> Option<&str> {
        self.provider_status.as_deref()
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }
}

pub struct D1MailboxRepository {
    database: D1Database,
}

impl D1MailboxRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn create_binding(
        &self,
        actor: &ActorContext,
        mutation: CreateMailboxBindingMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            mutation.envelope.idempotency_key,
        )?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let command = query!(
            &self.database,
            BINDING_CREATE_COMMAND,
            tenant_id,
            command_id.as_str(),
            actor_id,
            mutation.binding_id.as_str(),
            mutation.provider.storage_value(),
            mutation.secret_handle.as_str(),
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            MailboxEvidence {
                command_name: "mailbox.binding_create",
                result_code: "created",
                resource_type: "mailbox_binding",
                resource_id: mutation.binding_id.as_str(),
                aggregate_type: "mailbox_binding",
                aggregate_id: mutation.binding_id.as_str(),
                aggregate_version: 1,
                event_type: "mailbox.binding_created.v1",
            },
            now,
            expires_at,
        )
        .await
    }

    pub async fn revoke_binding(
        &self,
        actor: &ActorContext,
        mutation: RevokeMailboxBindingMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            mutation.envelope.idempotency_key,
        )?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let aggregate_version = next_version_value(mutation.expected_binding_version)?;
        let command = query!(
            &self.database,
            BINDING_REVOKE_COMMAND,
            tenant_id,
            command_id.as_str(),
            actor_id,
            mutation.binding_id.as_str(),
            sqlite_version(mutation.expected_binding_version)?,
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            MailboxEvidence {
                command_name: "mailbox.binding_revoke",
                result_code: "revoked",
                resource_type: "mailbox_binding",
                resource_id: mutation.binding_id.as_str(),
                aggregate_type: "mailbox_binding",
                aggregate_id: mutation.binding_id.as_str(),
                aggregate_version,
                event_type: "mailbox.binding_revoked.v1",
            },
            now,
            expires_at,
        )
        .await
    }

    pub async fn create_job(
        &self,
        actor: &ActorContext,
        mutation: CreateMailboxJobMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        validate_cursor(mutation.cursor).map_err(domain_error)?;
        if mutation.max_attempts == 0 || mutation.max_attempts > 10 {
            return Err(Error::RustError("invalid mailbox max attempts".to_owned()));
        }
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            mutation.envelope.idempotency_key,
        )?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let command = query!(
            &self.database,
            JOB_CREATE_COMMAND,
            tenant_id,
            command_id.as_str(),
            actor_id,
            mutation.binding_id.as_str(),
            mutation.job_id.as_str(),
            mutation.cursor,
            sqlite_integer(mutation.scheduled_at.value())?,
            i64::from(mutation.max_attempts),
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            MailboxEvidence {
                command_name: "mailbox.job_create",
                result_code: "created",
                resource_type: "mailbox_job",
                resource_id: mutation.job_id.as_str(),
                aggregate_type: "mailbox_job",
                aggregate_id: mutation.job_id.as_str(),
                aggregate_version: 1,
                event_type: "mailbox.job_created.v1",
            },
            now,
            expires_at,
        )
        .await
    }

    pub async fn run_job(
        &self,
        actor: &ActorContext,
        mutation: RunMailboxJobMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        validate_provider_status(mutation.prepared.provider_status()).map_err(domain_error)?;
        validate_cursor(mutation.prepared.cursor()).map_err(domain_error)?;
        let expected_result_version = mutation
            .expected_job_version
            .next()
            .and_then(AggregateVersion::next)
            .and_then(AggregateVersion::next)
            .map_err(|error| Error::RustError(error.to_string()))?;
        if expected_result_version != mutation.prepared.version() {
            return Err(Error::RustError(
                "mailbox run outcome version does not match expected version".to_owned(),
            ));
        }
        let (result_code, event_type) = match mutation.prepared.status() {
            MailboxJobStatus::Succeeded => ("succeeded", "mailbox.job_succeeded.v1"),
            MailboxJobStatus::RetryPending => {
                ("retry_pending", "mailbox.job_retry_scheduled.v1")
            }
            MailboxJobStatus::AuthRequired => {
                ("auth_required", "mailbox.job_auth_required.v1")
            }
            MailboxJobStatus::Suspended => ("suspended", "mailbox.job_suspended.v1"),
            MailboxJobStatus::Failed => ("failed", "mailbox.job_failed.v1"),
            MailboxJobStatus::Scheduled | MailboxJobStatus::Queued | MailboxJobStatus::Running => {
                return Err(Error::RustError("mailbox_run_outcome_invalid".to_owned()));
            }
        };
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            mutation.envelope.idempotency_key,
        )?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let retry_at = mutation
            .prepared
            .retry_at()
            .map(|value| sqlite_integer(value.value()))
            .transpose()?;
        let command = query!(
            &self.database,
            JOB_RUN_COMMAND,
            tenant_id,
            command_id.as_str(),
            actor_id,
            mutation.binding_id.as_str(),
            mutation.job_id.as_str(),
            sqlite_version(mutation.expected_job_version)?,
            mutation.prepared.status().storage_value(),
            mutation.prepared.cursor(),
            mutation.prepared.provider_status(),
            i64::from(mutation.prepared.bounded_item_count()),
            retry_at,
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            MailboxEvidence {
                command_name: "mailbox.job_run",
                result_code,
                resource_type: "mailbox_job",
                resource_id: mutation.job_id.as_str(),
                aggregate_type: "mailbox_job",
                aggregate_id: mutation.job_id.as_str(),
                aggregate_version: sqlite_version(expected_result_version)?,
                event_type,
            },
            now,
            expires_at,
        )
        .await
    }

    pub async fn find_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<MailboxBinding>> {
        query!(
            &self.database,
            r#"
            SELECT binding_id, provider, secret_handle, status, execution_status, version
            FROM mailbox_bindings
            WHERE tenant_id = ? AND binding_id = ?
            "#,
            scope.tenant_id().as_str(),
            binding_id.as_str()
        )?
        .first::<MailboxBindingRow>(None)
        .await?
        .map(|row| binding_from_row(scope, row))
        .transpose()
    }

    pub async fn find_job(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        job_id: &MailboxJobId,
    ) -> Result<Option<MailboxJobProjection>> {
        query!(
            &self.database,
            r#"
            SELECT
                job_id, cursor, lifecycle_status AS status, attempt, max_attempts, next_run_at_ms,
                provider_status, bounded_item_count, version
            FROM mailbox_jobs
            WHERE tenant_id = ? AND binding_id = ? AND job_id = ?
            "#,
            scope.tenant_id().as_str(),
            binding_id.as_str(),
            job_id.as_str()
        )?
        .first::<MailboxJobRow>(None)
        .await?
        .map(|row| job_from_row(scope, binding_id, row))
        .transpose()
    }

    async fn execute(
        &self,
        actor: &ActorContext,
        envelope: &MutationEnvelope<'_>,
        command: D1PreparedStatement,
        evidence: MailboxEvidence<'_>,
        now: i64,
        expires_at: i64,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let statements = vec![
            command,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                evidence.command_name,
                evidence.result_code,
                evidence.resource_id,
                envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                evidence.command_name,
                evidence.resource_type,
                evidence.resource_id,
                evidence.result_code,
                envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                evidence.aggregate_type,
                evidence.aggregate_id,
                evidence.aggregate_version,
                evidence.event_type,
                envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }
}

struct MailboxEvidence<'a> {
    command_name: &'static str,
    result_code: &'static str,
    resource_type: &'static str,
    resource_id: &'a str,
    aggregate_type: &'static str,
    aggregate_id: &'a str,
    aggregate_version: i64,
    event_type: &'static str,
}

#[derive(Deserialize)]
struct MailboxBindingRow {
    binding_id: String,
    provider: String,
    secret_handle: String,
    status: String,
    execution_status: String,
    version: i64,
}

#[derive(Deserialize)]
struct MailboxJobRow {
    job_id: String,
    cursor: Option<String>,
    status: String,
    attempt: i64,
    max_attempts: i64,
    next_run_at_ms: i64,
    provider_status: Option<String>,
    bounded_item_count: i64,
    version: i64,
}

fn binding_from_row(scope: &TenantScope, row: MailboxBindingRow) -> Result<MailboxBinding> {
    let status = if row.status == "REVOKED" {
        MailboxBindingStatus::Revoked
    } else if row.status == "ACTIVE" {
        MailboxBindingStatus::parse_storage(&row.execution_status).map_err(domain_error)?
    } else {
        return Err(Error::RustError(
            "invalid mailbox binding status".to_owned(),
        ));
    };
    Ok(MailboxBinding::restore(
        scope.tenant_id().clone(),
        MailboxBindingId::parse(row.binding_id).map_err(identifier_error)?,
        MailboxProvider::parse_storage(&row.provider).map_err(domain_error)?,
        SecretHandle::parse(row.secret_handle).map_err(identifier_error)?,
        status,
        AggregateVersion::new(positive_u64(row.version)?)
            .map_err(|error| Error::RustError(error.to_string()))?,
    ))
}

fn job_from_row(
    scope: &TenantScope,
    binding_id: &MailboxBindingId,
    row: MailboxJobRow,
) -> Result<MailboxJobProjection> {
    let bounded_item_count = bounded_u32(row.bounded_item_count, 10_000)?;
    if let Some(status) = row.provider_status.as_deref() {
        validate_provider_status(status).map_err(domain_error)?;
    }
    let job = MailboxJob::restore(MailboxJobRestore {
        tenant_id: scope.tenant_id().clone(),
        binding_id: binding_id.clone(),
        job_id: MailboxJobId::parse(row.job_id).map_err(identifier_error)?,
        cursor: row.cursor,
        status: MailboxJobStatus::parse_storage(&row.status).map_err(domain_error)?,
        attempt: bounded_u32(row.attempt, 10)?,
        max_attempts: bounded_u32(row.max_attempts, 10)?,
        next_run_at: UnixMillis::new(non_negative_u64(row.next_run_at_ms)?),
        version: AggregateVersion::new(positive_u64(row.version)?)
            .map_err(|error| Error::RustError(error.to_string()))?,
    })
    .map_err(domain_error)?;
    Ok(MailboxJobProjection {
        job,
        provider_status: row.provider_status,
        bounded_item_count,
    })
}

#[allow(clippy::too_many_arguments)]
fn idempotency_statement(
    database: &D1Database,
    tenant_id: &str,
    actor_id: &str,
    command_name: &str,
    result_code: &str,
    result_reference: &str,
    envelope: &MutationEnvelope<'_>,
    now: i64,
    expires_at: i64,
) -> Result<D1PreparedStatement> {
    query!(
        database,
        IDEMPOTENCY_CREATE,
        tenant_id,
        actor_id,
        envelope.idempotency_key.as_str(),
        command_name,
        envelope.request_digest,
        result_code,
        result_reference,
        now,
        expires_at
    )
}

#[allow(clippy::too_many_arguments)]
fn audit_statement(
    database: &D1Database,
    tenant_id: &str,
    correlation_id: &str,
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    result_code: &str,
    envelope: &MutationEnvelope<'_>,
    now: i64,
) -> Result<D1PreparedStatement> {
    query!(
        database,
        AUDIT_CREATE,
        tenant_id,
        envelope.audit_event_id.as_str(),
        correlation_id,
        actor_id,
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
    envelope: &MutationEnvelope<'_>,
    now: i64,
) -> Result<D1PreparedStatement> {
    query!(
        database,
        OUTBOX_CREATE,
        tenant_id,
        envelope.outbox_event_id.as_str(),
        aggregate_type,
        aggregate_id,
        aggregate_version,
        event_type,
        envelope.payload_json,
        now
    )
}

fn sqlite_integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

fn sqlite_version(value: AggregateVersion) -> Result<i64> {
    sqlite_integer(value.value())
}

fn next_version_value(value: AggregateVersion) -> Result<i64> {
    value
        .next()
        .map_err(|error| Error::RustError(error.to_string()))
        .and_then(sqlite_version)
}

fn positive_u64(value: i64) -> Result<u64> {
    let value = u64::try_from(value)
        .map_err(|_| Error::RustError("mailbox numeric value is negative".to_owned()))?;
    if value == 0 {
        return Err(Error::RustError(
            "mailbox aggregate version must be positive".to_owned(),
        ));
    }
    Ok(value)
}

fn non_negative_u64(value: i64) -> Result<u64> {
    u64::try_from(value)
        .map_err(|_| Error::RustError("mailbox numeric value is negative".to_owned()))
}

fn bounded_u32(value: i64, maximum: u32) -> Result<u32> {
    let value = u32::try_from(value)
        .map_err(|_| Error::RustError("mailbox numeric value is out of range".to_owned()))?;
    if value > maximum {
        return Err(Error::RustError(
            "mailbox numeric value exceeds bounded range".to_owned(),
        ));
    }
    Ok(value)
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

fn domain_error(error: mailbox_domain::MailboxError) -> Error {
    Error::RustError(error.to_string())
}
