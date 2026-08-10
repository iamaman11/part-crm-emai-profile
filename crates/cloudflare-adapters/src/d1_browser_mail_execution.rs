use crate::d1_command_identity::command_journal_id;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use application_ports::CommandExecutionEvidence;
use application_ports::browser_mail_execution::{
    BrowserMailboxExecutionBindWrite, BrowserMailboxExecutionBinding,
    BrowserMailboxExecutionBindingApplicationPort, BrowserMailboxExecutionBindingPort,
};
use application_ports::mailboxes::{
    MailboxBindingPortError, MailboxBindingPortErrorClass, MailboxReplayDecision,
    MailboxReplayReceipt,
};
use application_ports::{QueryPortError, QueryPortErrorClass};
use profile_platform_primitives::{ActorContext, MailboxBindingId, ProfileId, TenantScope};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, query};

const BIND_COMMAND: &str = r#"
INSERT INTO browser_mailbox_execution_bind_commands (
    tenant_id, command_id, command_actor_id, binding_id, profile_id, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?)
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

const RESOLVE_BINDING: &str = r#"
SELECT execution.binding_id, execution.profile_id
FROM browser_mailbox_execution_bindings AS execution
JOIN mailbox_bindings AS binding
  ON binding.tenant_id = execution.tenant_id
 AND binding.binding_id = execution.binding_id
WHERE execution.tenant_id = ?
  AND execution.binding_id = ?
  AND binding.provider = 'BROWSER_FALLBACK'
  AND binding.status = 'ACTIVE'
  AND binding.execution_status = 'ACTIVE'
LIMIT 1
"#;

pub struct D1BrowserMailboxExecutionBinding {
    database: D1Database,
    idempotency: D1IdempotencyRepository,
}

impl D1BrowserMailboxExecutionBinding {
    #[must_use]
    pub const fn new(database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            database,
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }
}

impl BrowserMailboxExecutionBindingApplicationPort for D1BrowserMailboxExecutionBinding {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxReplayDecision, MailboxBindingPortError> {
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
            .map_err(map_dependency_error)
    }

    async fn bind_browser_mailbox_execution(
        &self,
        actor: &ActorContext,
        write: &BrowserMailboxExecutionBindWrite,
    ) -> Result<(), MailboxBindingPortError> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let evidence = write.evidence();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            evidence.idempotency_key(),
        )
        .map_err(map_write_error)?;
        let now = sqlite_integer(evidence.now().value()).map_err(map_write_error)?;
        let expires_at =
            sqlite_integer(evidence.idempotency_expires_at().value()).map_err(map_write_error)?;

        let command = query!(
            &self.database,
            BIND_COMMAND,
            tenant_id,
            command_id.as_str(),
            actor_id,
            write.binding_id().as_str(),
            write.profile_id().as_str(),
            now
        )
        .map_err(map_write_error)?;
        let idempotency = query!(
            &self.database,
            IDEMPOTENCY_CREATE,
            tenant_id,
            actor_id,
            evidence.idempotency_key().as_str(),
            "mailbox.browser_execution_bind",
            evidence.request_digest(),
            "bound",
            write.binding_id().as_str(),
            now,
            expires_at
        )
        .map_err(map_write_error)?;
        let audit = query!(
            &self.database,
            AUDIT_CREATE,
            tenant_id,
            evidence.audit_event_id().as_str(),
            actor.correlation_id().as_str(),
            actor_id,
            "mailbox.browser_execution_bind",
            "browser_mailbox_execution_binding",
            write.binding_id().as_str(),
            "bound",
            now
        )
        .map_err(map_write_error)?;
        let outbox = query!(
            &self.database,
            OUTBOX_CREATE,
            tenant_id,
            evidence.outbox_event_id().as_str(),
            "browser_mailbox_execution_binding",
            write.binding_id().as_str(),
            1_i64,
            "mailbox.browser_execution_bound.v1",
            write.event_payload_json(),
            now
        )
        .map_err(map_write_error)?;

        let atomic_batch = self
            .database
            .batch(vec![command, idempotency, audit, outbox]);
        atomic_batch.await.map(|_| ()).map_err(map_write_error)
    }
}

impl BrowserMailboxExecutionBindingPort for D1BrowserMailboxExecutionBinding {
    async fn resolve_browser_mailbox_execution_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<BrowserMailboxExecutionBinding>, QueryPortError> {
        query!(
            &self.database,
            RESOLVE_BINDING,
            scope.tenant_id().as_str(),
            binding_id.as_str()
        )
        .map_err(|_| dependency_unavailable())?
        .first::<BrowserMailboxExecutionBindingRow>(None)
        .await
        .map_err(|_| dependency_unavailable())?
        .map(binding_from_row)
        .transpose()
    }
}

#[derive(Deserialize)]
struct BrowserMailboxExecutionBindingRow {
    binding_id: String,
    profile_id: String,
}

fn binding_from_row(
    row: BrowserMailboxExecutionBindingRow,
) -> Result<BrowserMailboxExecutionBinding, QueryPortError> {
    let binding_id = MailboxBindingId::parse(row.binding_id).map_err(|_| integrity_failure())?;
    let profile_id = ProfileId::parse(row.profile_id).map_err(|_| integrity_failure())?;
    Ok(BrowserMailboxExecutionBinding::new(binding_id, profile_id))
}

fn map_replay_decision(decision: IdempotencyDecision) -> MailboxReplayDecision {
    match decision {
        IdempotencyDecision::Miss => MailboxReplayDecision::Miss,
        IdempotencyDecision::Replay(receipt) => {
            MailboxReplayDecision::Replay(MailboxReplayReceipt::new(
                receipt.result_code(),
                receipt.result_reference().map(str::to_owned),
            ))
        }
        IdempotencyDecision::Conflict => MailboxReplayDecision::Conflict,
    }
}

fn map_dependency_error(_error: Error) -> MailboxBindingPortError {
    MailboxBindingPortError::new(MailboxBindingPortErrorClass::DependencyUnavailable)
}

fn map_write_error(error: Error) -> MailboxBindingPortError {
    let message = error.to_string();
    let class = if message.contains("browser_mailbox_binding_not_executable")
        || message.contains("browser_mailbox_profile_missing")
    {
        MailboxBindingPortErrorClass::NotFound
    } else if message.contains("UNIQUE constraint failed") {
        MailboxBindingPortErrorClass::Conflict
    } else if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
    {
        MailboxBindingPortErrorClass::IntegrityFailure
    } else if message.contains("value exceeds SQLite INTEGER") {
        MailboxBindingPortErrorClass::InternalFailure
    } else {
        MailboxBindingPortErrorClass::DependencyUnavailable
    };
    MailboxBindingPortError::new(class)
}

fn sqlite_integer(value: u64) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

const fn dependency_unavailable() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}

const fn integrity_failure() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::{RESOLVE_BINDING, map_write_error};
    use application_ports::mailboxes::MailboxBindingPortErrorClass;
    use worker::Error;

    #[test]
    fn resolver_is_browser_only_active_and_assignment_independent() {
        assert!(RESOLVE_BINDING.contains("binding.provider = 'BROWSER_FALLBACK'"));
        assert!(RESOLVE_BINDING.contains("binding.status = 'ACTIVE'"));
        assert!(RESOLVE_BINDING.contains("binding.execution_status = 'ACTIVE'"));
        assert!(!RESOLVE_BINDING.contains("profile_client_assignments"));
    }

    #[test]
    fn governed_binding_failures_keep_public_classes_stable() {
        assert_eq!(
            map_write_error(Error::RustError(
                "browser_mailbox_binding_not_executable".to_owned()
            ))
            .class(),
            MailboxBindingPortErrorClass::NotFound
        );
        assert_eq!(
            map_write_error(Error::RustError(
                "browser_mailbox_execution_binding_not_governed".to_owned()
            ))
            .class(),
            MailboxBindingPortErrorClass::IntegrityFailure
        );
    }
}
