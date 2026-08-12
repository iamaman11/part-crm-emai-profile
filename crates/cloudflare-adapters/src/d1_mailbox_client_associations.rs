use crate::d1_command_identity::command_journal_id;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_client_associations::{
    MailboxClientAssociation, MailboxClientAssociationApplicationPort,
    MailboxClientAssociationContext, MailboxClientAssociationPortError,
    MailboxClientAssociationPortErrorClass, MailboxClientAssociationReplayDecision,
    MailboxClientAssociationReplayReceipt, MailboxClientAssociationVersion,
    MailboxClientAssociationWrite,
};
use mailbox_domain::MailboxClientAssociationAction;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId, TenantScope};
use serde::Deserialize;
use worker::d1::{D1Database, D1PreparedStatement};
use worker::{Error, query};

const ASSOCIATION_COMMAND: &str = r#"
INSERT INTO mailbox_client_association_commands (
    tenant_id, command_id, command_actor_id, binding_id,
    expected_relationship_version, next_relationship_version, operation,
    previous_client_id, next_client_id, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

pub struct D1MailboxClientAssociationApplicationRepository {
    database: D1Database,
    idempotency: D1IdempotencyRepository,
}

impl D1MailboxClientAssociationApplicationRepository {
    #[must_use]
    pub const fn new(database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            database,
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }
}

impl MailboxClientAssociationApplicationPort for D1MailboxClientAssociationApplicationRepository {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxClientAssociationReplayDecision, MailboxClientAssociationPortError> {
        self.idempotency
            .decide(
                actor.tenant_scope(),
                actor.actor_id(),
                evidence.idempotency_key(),
                command_name,
                evidence.request_digest(),
                evidence.now(),
            )
            .await
            .map(map_replay_decision)
            .map_err(|_| dependency_error())
    }

    async fn load_context(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        target_client_id: Option<&ClientId>,
    ) -> Result<Option<MailboxClientAssociationContext>, MailboxClientAssociationPortError> {
        let target = target_client_id.map(ClientId::as_str);
        let row = query!(
            &self.database,
            r#"
            SELECT
                binding.status AS binding_status,
                binding.execution_status AS execution_status,
                association.client_id AS current_client_id,
                association.version AS relationship_version,
                CASE
                    WHEN ? IS NULL THEN 1
                    WHEN EXISTS (
                        SELECT 1
                        FROM clients AS target
                        WHERE target.tenant_id = binding.tenant_id
                          AND target.client_id = ?
                          AND target.status = 'ACTIVE'
                    ) THEN 1
                    ELSE 0
                END AS target_client_active
            FROM mailbox_bindings AS binding
            LEFT JOIN mailbox_client_association_state AS association
              ON association.tenant_id = binding.tenant_id
             AND association.binding_id = binding.binding_id
            WHERE binding.tenant_id = ? AND binding.binding_id = ?
            "#,
            target,
            target,
            scope.tenant_id().as_str(),
            binding_id.as_str()
        )
        .map_err(|_| dependency_error())?
        .first::<AssociationContextRow>(None)
        .await
        .map_err(|_| dependency_error())?;

        row.map(|row| map_context(scope, binding_id, row))
            .transpose()
    }

    async fn change_association(
        &self,
        actor: &ActorContext,
        write: &MailboxClientAssociationWrite,
    ) -> Result<(), MailboxClientAssociationPortError> {
        self.execute_change(actor, write)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }
}

impl D1MailboxClientAssociationApplicationRepository {
    async fn execute_change(
        &self,
        actor: &ActorContext,
        write: &MailboxClientAssociationWrite,
    ) -> Result<Vec<worker::d1::D1Result>, Error> {
        let evidence = write.evidence();
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            evidence.idempotency_key(),
        )?;
        let now = sqlite_integer(evidence.now().value())?;
        let expires_at = sqlite_integer(evidence.idempotency_expires_at().value())?;
        let expected_version = sqlite_integer(write.expected_version().value())?;
        let next_version = sqlite_integer(write.next_version().value())?;
        let previous_client_id = write.previous_client_id().map(ClientId::as_str);
        let next_client_id = write.next_client_id().map(ClientId::as_str);
        let operation = operation_value(write.action());
        let result_code = result_code(write.action());
        let event_type = event_type(write.action());

        let command = query!(
            &self.database,
            ASSOCIATION_COMMAND,
            tenant_id,
            command_id.as_str(),
            actor_id,
            write.binding_id().as_str(),
            expected_version,
            next_version,
            operation,
            previous_client_id,
            next_client_id,
            now
        )?;

        let statements = vec![
            command,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "mailbox.client_association_change",
                result_code,
                write.binding_id().as_str(),
                evidence,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                action_name(write.action()),
                write.binding_id().as_str(),
                result_code,
                evidence,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                write.binding_id().as_str(),
                next_version,
                event_type,
                write.event_payload_json(),
                evidence,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }
}

#[derive(Deserialize)]
struct AssociationContextRow {
    binding_status: String,
    execution_status: String,
    current_client_id: Option<String>,
    relationship_version: Option<i64>,
    target_client_active: i64,
}

fn map_context(
    scope: &TenantScope,
    binding_id: &MailboxBindingId,
    row: AssociationContextRow,
) -> Result<MailboxClientAssociationContext, MailboxClientAssociationPortError> {
    let version = match row.relationship_version {
        Some(value) => {
            let value = u64::try_from(value).map_err(|_| integrity_error())?;
            if value == 0 {
                return Err(integrity_error());
            }
            MailboxClientAssociationVersion::new(value)
        }
        None => MailboxClientAssociationVersion::NEVER_ASSOCIATED,
    };
    if row.relationship_version.is_none() && row.current_client_id.is_some() {
        return Err(integrity_error());
    }
    let client_id = row
        .current_client_id
        .map(ClientId::parse)
        .transpose()
        .map_err(|_| integrity_error())?;
    let association = MailboxClientAssociation::restore(
        scope.tenant_id().clone(),
        binding_id.clone(),
        client_id,
        version,
    );
    Ok(MailboxClientAssociationContext::new(
        association,
        row.binding_status == "ACTIVE" && row.execution_status == "ACTIVE",
        row.target_client_active == 1,
    ))
}

fn map_replay_decision(decision: IdempotencyDecision) -> MailboxClientAssociationReplayDecision {
    match decision {
        IdempotencyDecision::Miss => MailboxClientAssociationReplayDecision::Miss,
        IdempotencyDecision::Replay(receipt) => MailboxClientAssociationReplayDecision::Replay(
            MailboxClientAssociationReplayReceipt::new(
                receipt.result_code(),
                receipt.result_reference().map(str::to_owned),
            ),
        ),
        IdempotencyDecision::Conflict => MailboxClientAssociationReplayDecision::Conflict,
    }
}

const fn operation_value(action: MailboxClientAssociationAction) -> &'static str {
    match action {
        MailboxClientAssociationAction::Bind => "BIND",
        MailboxClientAssociationAction::Rebind => "REBIND",
        MailboxClientAssociationAction::Unbind => "UNBIND",
    }
}

const fn result_code(action: MailboxClientAssociationAction) -> &'static str {
    match action {
        MailboxClientAssociationAction::Bind => "bound",
        MailboxClientAssociationAction::Rebind => "rebound",
        MailboxClientAssociationAction::Unbind => "unbound",
    }
}

const fn action_name(action: MailboxClientAssociationAction) -> &'static str {
    match action {
        MailboxClientAssociationAction::Bind => "mailbox.client_bind",
        MailboxClientAssociationAction::Rebind => "mailbox.client_rebind",
        MailboxClientAssociationAction::Unbind => "mailbox.client_unbind",
    }
}

const fn event_type(action: MailboxClientAssociationAction) -> &'static str {
    match action {
        MailboxClientAssociationAction::Bind => "mailbox.client_bound.v1",
        MailboxClientAssociationAction::Rebind => "mailbox.client_rebound.v1",
        MailboxClientAssociationAction::Unbind => "mailbox.client_unbound.v1",
    }
}

fn map_write_error(error: Error) -> MailboxClientAssociationPortError {
    let message = error.to_string();
    let class = if message.contains("owner_required") || message.contains("target_not_active") {
        MailboxClientAssociationPortErrorClass::NotFound
    } else if message.contains("version_mismatch") {
        MailboxClientAssociationPortErrorClass::VersionConflict
    } else if message.contains("binding_not_executable")
        || message.contains("invalid_transition")
        || message.contains("time_regression")
        || message.contains("previous_mismatch")
    {
        MailboxClientAssociationPortErrorClass::InvalidState
    } else if message.contains("UNIQUE constraint failed") {
        MailboxClientAssociationPortErrorClass::Conflict
    } else if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
        || message.contains("immutable")
    {
        MailboxClientAssociationPortErrorClass::IntegrityFailure
    } else if message.contains("next_version_invalid")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        MailboxClientAssociationPortErrorClass::InternalFailure
    } else {
        MailboxClientAssociationPortErrorClass::DependencyUnavailable
    };
    MailboxClientAssociationPortError::new(class)
}

const fn dependency_error() -> MailboxClientAssociationPortError {
    MailboxClientAssociationPortError::new(
        MailboxClientAssociationPortErrorClass::DependencyUnavailable,
    )
}

const fn integrity_error() -> MailboxClientAssociationPortError {
    MailboxClientAssociationPortError::new(MailboxClientAssociationPortErrorClass::IntegrityFailure)
}

#[allow(clippy::too_many_arguments)]
fn idempotency_statement(
    database: &D1Database,
    tenant_id: &str,
    actor_id: &str,
    command_name: &str,
    result_code: &str,
    result_reference: &str,
    evidence: &CommandExecutionEvidence,
    now: i64,
    expires_at: i64,
) -> Result<D1PreparedStatement, Error> {
    query!(
        database,
        IDEMPOTENCY_CREATE,
        tenant_id,
        actor_id,
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
    tenant_id: &str,
    correlation_id: &str,
    actor_id: &str,
    action: &str,
    resource_id: &str,
    result_code: &str,
    evidence: &CommandExecutionEvidence,
    now: i64,
) -> Result<D1PreparedStatement, Error> {
    query!(
        database,
        AUDIT_CREATE,
        tenant_id,
        evidence.audit_event_id().as_str(),
        correlation_id,
        actor_id,
        action,
        "mailbox_client_association",
        resource_id,
        result_code,
        now
    )
}

#[allow(clippy::too_many_arguments)]
fn outbox_statement(
    database: &D1Database,
    tenant_id: &str,
    aggregate_id: &str,
    aggregate_version: i64,
    event_type: &str,
    payload_json: &str,
    evidence: &CommandExecutionEvidence,
    now: i64,
) -> Result<D1PreparedStatement, Error> {
    query!(
        database,
        OUTBOX_CREATE,
        tenant_id,
        evidence.outbox_event_id().as_str(),
        "mailbox_client_association",
        aggregate_id,
        aggregate_version,
        event_type,
        payload_json,
        now
    )
}

fn sqlite_integer(value: u64) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::map_write_error;
    use application_ports::mailbox_client_associations::MailboxClientAssociationPortErrorClass;

    #[test]
    fn relationship_failures_keep_stable_application_classes() {
        assert_eq!(
            map_write_error(worker::Error::RustError(
                "mailbox_client_association_version_mismatch".to_owned()
            ))
            .class(),
            MailboxClientAssociationPortErrorClass::VersionConflict
        );
        assert_eq!(
            map_write_error(worker::Error::RustError(
                "mailbox_client_association_binding_not_executable".to_owned()
            ))
            .class(),
            MailboxClientAssociationPortErrorClass::InvalidState
        );
        assert_eq!(
            map_write_error(worker::Error::RustError(
                "mailbox_client_association_target_not_active".to_owned()
            ))
            .class(),
            MailboxClientAssociationPortErrorClass::NotFound
        );
    }
}
