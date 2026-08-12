use crate::d1_command_identity::command_journal_id;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_onboarding::{
    MailboxOnboarding, MailboxOnboardingApplicationPort, MailboxOnboardingContext,
    MailboxOnboardingPortError, MailboxOnboardingPortErrorClass, MailboxOnboardingReplayDecision,
    MailboxOnboardingReplayReceipt, MailboxOnboardingWrite,
};
use mailbox_domain::{
    MailboxOnboardingAction, MailboxOnboardingStatus, MailboxOnboardingStatusMetadata,
    MailboxOnboardingVersion, MailboxProvider,
};
use profile_platform_primitives::{ActorContext, MailboxOnboardingId, SecretHandle, TenantScope};
use serde::Deserialize;
use worker::d1::{D1Database, D1PreparedStatement};
use worker::{Error, query};

const ONBOARDING_COMMAND: &str = r#"
INSERT INTO mailbox_onboarding_commands (
    tenant_id, command_id, command_actor_id, onboarding_id, provider,
    expected_version, next_version, operation, previous_status, next_status,
    previous_credential_handle, next_credential_handle, status_metadata, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

pub struct D1MailboxOnboardingApplicationRepository {
    database: D1Database,
    idempotency: D1IdempotencyRepository,
}

impl D1MailboxOnboardingApplicationRepository {
    #[must_use]
    pub const fn new(database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            database,
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }
}

impl MailboxOnboardingApplicationPort for D1MailboxOnboardingApplicationRepository {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxOnboardingReplayDecision, MailboxOnboardingPortError> {
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
        onboarding_id: &MailboxOnboardingId,
    ) -> Result<Option<MailboxOnboardingContext>, MailboxOnboardingPortError> {
        let row = query!(
            &self.database,
            r#"
            SELECT provider, lifecycle_status, credential_handle, status_metadata, version
            FROM mailbox_onboarding_state
            WHERE tenant_id = ? AND onboarding_id = ?
            "#,
            scope.tenant_id().as_str(),
            onboarding_id.as_str()
        )
        .map_err(|_| dependency_error())?
        .first::<OnboardingContextRow>(None)
        .await
        .map_err(|_| dependency_error())?;

        row.map(|row| map_context(scope, onboarding_id, row))
            .transpose()
    }

    async fn commit(
        &self,
        actor: &ActorContext,
        write: &MailboxOnboardingWrite,
    ) -> Result<(), MailboxOnboardingPortError> {
        self.execute_commit(actor, write)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }
}

impl D1MailboxOnboardingApplicationRepository {
    async fn execute_commit(
        &self,
        actor: &ActorContext,
        write: &MailboxOnboardingWrite,
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
        let previous_status = write
            .previous_status()
            .map(MailboxOnboardingStatus::storage_value);
        let previous_handle = write.previous_credential_handle().map(SecretHandle::as_str);
        let next_handle = write.next_credential_handle().map(SecretHandle::as_str);
        let status_metadata = write
            .status_metadata()
            .map(MailboxOnboardingStatusMetadata::as_str);
        let result_code = result_code(write.action());

        let command = query!(
            &self.database,
            ONBOARDING_COMMAND,
            tenant_id,
            command_id.as_str(),
            actor_id,
            write.onboarding_id().as_str(),
            write.provider().storage_value(),
            expected_version,
            next_version,
            write.action().storage_value(),
            previous_status,
            write.next_status().storage_value(),
            previous_handle,
            next_handle,
            status_metadata,
            now
        )?;

        let statements = vec![
            command,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "mailbox.onboarding_change",
                result_code,
                write.onboarding_id().as_str(),
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
                write.onboarding_id().as_str(),
                result_code,
                evidence,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                write.onboarding_id().as_str(),
                next_version,
                event_type(write.action()),
                write.event_payload_json(),
                evidence,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }
}

#[derive(Deserialize)]
struct OnboardingContextRow {
    provider: String,
    lifecycle_status: String,
    credential_handle: Option<String>,
    status_metadata: Option<String>,
    version: i64,
}

fn map_context(
    scope: &TenantScope,
    onboarding_id: &MailboxOnboardingId,
    row: OnboardingContextRow,
) -> Result<MailboxOnboardingContext, MailboxOnboardingPortError> {
    let provider = MailboxProvider::parse_storage(&row.provider).map_err(|_| integrity_error())?;
    let status = MailboxOnboardingStatus::parse_storage(&row.lifecycle_status)
        .map_err(|_| integrity_error())?;
    let credential_handle = row
        .credential_handle
        .map(SecretHandle::parse)
        .transpose()
        .map_err(|_| integrity_error())?;
    if matches!(status, MailboxOnboardingStatus::Pending) && credential_handle.is_some() {
        return Err(integrity_error());
    }
    if matches!(
        status,
        MailboxOnboardingStatus::Active | MailboxOnboardingStatus::ReauthRequired
    ) && credential_handle.is_none()
    {
        return Err(integrity_error());
    }
    let status_metadata = row
        .status_metadata
        .map(MailboxOnboardingStatusMetadata::parse)
        .transpose()
        .map_err(|_| integrity_error())?;
    let version = u64::try_from(row.version).map_err(|_| integrity_error())?;
    if version == 0 {
        return Err(integrity_error());
    }
    Ok(MailboxOnboardingContext::new(MailboxOnboarding::restore(
        scope.tenant_id().clone(),
        onboarding_id.clone(),
        provider,
        status,
        credential_handle,
        status_metadata,
        MailboxOnboardingVersion::new(version),
    )))
}

fn map_replay_decision(decision: IdempotencyDecision) -> MailboxOnboardingReplayDecision {
    match decision {
        IdempotencyDecision::Miss => MailboxOnboardingReplayDecision::Miss,
        IdempotencyDecision::Replay(receipt) => {
            MailboxOnboardingReplayDecision::Replay(MailboxOnboardingReplayReceipt::new(
                receipt.result_code().to_owned(),
                receipt.result_reference().map(str::to_owned),
            ))
        }
        IdempotencyDecision::Conflict => MailboxOnboardingReplayDecision::Conflict,
    }
}

const fn result_code(action: MailboxOnboardingAction) -> &'static str {
    match action {
        MailboxOnboardingAction::Start => "started",
        MailboxOnboardingAction::Activate => "activated",
        MailboxOnboardingAction::RequireReauth => "reauth_required",
        MailboxOnboardingAction::Disable => "disabled",
        MailboxOnboardingAction::MarkConfigError => "config_error",
    }
}

const fn action_name(action: MailboxOnboardingAction) -> &'static str {
    match action {
        MailboxOnboardingAction::Start => "mailbox.onboarding_start",
        MailboxOnboardingAction::Activate => "mailbox.onboarding_activate",
        MailboxOnboardingAction::RequireReauth => "mailbox.onboarding_require_reauth",
        MailboxOnboardingAction::Disable => "mailbox.onboarding_disable",
        MailboxOnboardingAction::MarkConfigError => "mailbox.onboarding_config_error",
    }
}

const fn event_type(action: MailboxOnboardingAction) -> &'static str {
    match action {
        MailboxOnboardingAction::Start => "mailbox.onboarding_started.v1",
        MailboxOnboardingAction::Activate => "mailbox.onboarding_activated.v1",
        MailboxOnboardingAction::RequireReauth => "mailbox.onboarding_reauth_required.v1",
        MailboxOnboardingAction::Disable => "mailbox.onboarding_disabled.v1",
        MailboxOnboardingAction::MarkConfigError => "mailbox.onboarding_config_error.v1",
    }
}

fn map_write_error(error: Error) -> MailboxOnboardingPortError {
    let message = error.to_string();
    let class = if message.contains("owner_required") || message.contains("not_found") {
        MailboxOnboardingPortErrorClass::NotFound
    } else if message.contains("version_mismatch") {
        MailboxOnboardingPortErrorClass::VersionConflict
    } else if message.contains("invalid_transition")
        || message.contains("provider_mismatch")
        || message.contains("previous_mismatch")
        || message.contains("time_regression")
    {
        MailboxOnboardingPortErrorClass::InvalidState
    } else if message.contains("start_conflict") || message.contains("UNIQUE constraint failed") {
        MailboxOnboardingPortErrorClass::Conflict
    } else if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
        || message.contains("immutable")
        || message.contains("delete_forbidden")
    {
        MailboxOnboardingPortErrorClass::IntegrityFailure
    } else if message.contains("next_version_invalid")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        MailboxOnboardingPortErrorClass::InternalFailure
    } else {
        MailboxOnboardingPortErrorClass::DependencyUnavailable
    };
    MailboxOnboardingPortError::new(class)
}

const fn dependency_error() -> MailboxOnboardingPortError {
    MailboxOnboardingPortError::new(MailboxOnboardingPortErrorClass::DependencyUnavailable)
}

const fn integrity_error() -> MailboxOnboardingPortError {
    MailboxOnboardingPortError::new(MailboxOnboardingPortErrorClass::IntegrityFailure)
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
        "mailbox_onboarding",
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
        "mailbox_onboarding",
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
    use application_ports::mailbox_onboarding::MailboxOnboardingPortErrorClass;

    #[test]
    fn onboarding_failures_keep_stable_application_classes() {
        assert_eq!(
            map_write_error(worker::Error::RustError(
                "mailbox_onboarding_version_mismatch".to_owned()
            ))
            .class(),
            MailboxOnboardingPortErrorClass::VersionConflict
        );
        assert_eq!(
            map_write_error(worker::Error::RustError(
                "mailbox_onboarding_invalid_transition".to_owned()
            ))
            .class(),
            MailboxOnboardingPortErrorClass::InvalidState
        );
        assert_eq!(
            map_write_error(worker::Error::RustError(
                "mailbox_onboarding_owner_required".to_owned()
            ))
            .class(),
            MailboxOnboardingPortErrorClass::NotFound
        );
    }
}
