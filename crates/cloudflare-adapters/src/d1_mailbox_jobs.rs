use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::MutationEnvelope;
use crate::d1_mailboxes::{CreateMailboxJobMutation, D1MailboxRepository, RunMailboxJobMutation};
use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_jobs::{
    MailboxBinding, MailboxJobApplicationPort, MailboxJobCreateWrite, MailboxJobPortError,
    MailboxJobPortErrorClass, MailboxJobReadModel, MailboxJobRunWrite,
};
use application_ports::mailboxes::{MailboxReplayDecision, MailboxReplayReceipt};
use profile_platform_primitives::{ActorContext, MailboxBindingId, MailboxJobId, TenantScope};
use worker::Error;
use worker::d1::D1Database;

pub struct D1MailboxJobApplicationRepository {
    mailboxes: D1MailboxRepository,
    idempotency: D1IdempotencyRepository,
}

impl D1MailboxJobApplicationRepository {
    #[must_use]
    pub const fn new(mailbox_database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            mailboxes: D1MailboxRepository::new(mailbox_database),
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }
}

impl MailboxJobApplicationPort for D1MailboxJobApplicationRepository {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<MailboxReplayDecision, MailboxJobPortError> {
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

    async fn create_job(
        &self,
        actor: &ActorContext,
        write: &MailboxJobCreateWrite,
    ) -> Result<(), MailboxJobPortError> {
        let evidence = write.evidence();
        let mutation = CreateMailboxJobMutation {
            binding_id: write.binding_id(),
            job_id: write.job_id(),
            cursor: write.cursor(),
            scheduled_at: write.scheduled_at(),
            max_attempts: write.max_attempts(),
            envelope: MutationEnvelope {
                idempotency_key: evidence.idempotency_key(),
                request_digest: evidence.request_digest(),
                audit_event_id: evidence.audit_event_id(),
                outbox_event_id: evidence.outbox_event_id(),
                payload_json: write.event_payload_json(),
                now: evidence.now(),
                idempotency_expires_at: evidence.idempotency_expires_at(),
            },
        };
        self.mailboxes
            .create_job(actor, mutation)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn run_job(
        &self,
        actor: &ActorContext,
        write: &MailboxJobRunWrite,
    ) -> Result<(), MailboxJobPortError> {
        let evidence = write.evidence();
        let mutation = RunMailboxJobMutation {
            binding_id: write.binding_id(),
            job_id: write.job_id(),
            expected_job_version: write.expected_version(),
            prepared: write.prepared(),
            envelope: MutationEnvelope {
                idempotency_key: evidence.idempotency_key(),
                request_digest: evidence.request_digest(),
                audit_event_id: evidence.audit_event_id(),
                outbox_event_id: evidence.outbox_event_id(),
                payload_json: write.event_payload_json(),
                now: evidence.now(),
                idempotency_expires_at: evidence.idempotency_expires_at(),
            },
        };
        self.mailboxes
            .run_job(actor, mutation)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn find_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<MailboxBinding>, MailboxJobPortError> {
        self.mailboxes
            .find_binding(scope, binding_id)
            .await
            .map_err(map_dependency_error)
    }

    async fn find_job(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        job_id: &MailboxJobId,
    ) -> Result<Option<MailboxJobReadModel>, MailboxJobPortError> {
        self.mailboxes
            .find_job(scope, binding_id, job_id)
            .await
            .map_err(map_dependency_error)
            .map(|projection| {
                projection.map(|projection| {
                    MailboxJobReadModel::new(
                        projection.job().clone(),
                        projection.provider_status().map(str::to_owned),
                        projection.bounded_item_count(),
                    )
                })
            })
    }
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

fn map_write_error(error: Error) -> MailboxJobPortError {
    MailboxJobPortError::new(classify_write_failure(&error.to_string()))
}

fn map_dependency_error(_error: Error) -> MailboxJobPortError {
    MailboxJobPortError::new(MailboxJobPortErrorClass::DependencyUnavailable)
}

fn classify_write_failure(message: &str) -> MailboxJobPortErrorClass {
    if message.contains("mailbox_binding_missing") || message.contains("mailbox_job_missing") {
        return MailboxJobPortErrorClass::NotFound;
    }
    if message.contains("version_mismatch") {
        return MailboxJobPortErrorClass::VersionConflict;
    }
    if message.contains("mailbox_binding_revoked")
        || message.contains("mailbox_binding_not_executable")
        || message.contains("mailbox_job_not_due")
        || message.contains("mailbox_job_attempts_exhausted")
        || message.contains("mailbox_retry_time_invalid")
    {
        return MailboxJobPortErrorClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return MailboxJobPortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
        || message.contains("mailbox_cursor_too_long")
        || message.contains("mailbox_provider_status_invalid")
        || message.contains("mailbox_run_outcome_invalid")
    {
        return MailboxJobPortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
        || message.contains("mailbox_job_version_overflow")
    {
        return MailboxJobPortErrorClass::InternalFailure;
    }
    MailboxJobPortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::classify_write_failure;
    use application_ports::mailbox_jobs::MailboxJobPortErrorClass;

    #[test]
    fn job_write_failures_keep_public_classes_stable() {
        assert_eq!(
            classify_write_failure("mailbox_job_missing"),
            MailboxJobPortErrorClass::NotFound
        );
        assert_eq!(
            classify_write_failure("mailbox_job_version_mismatch"),
            MailboxJobPortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_write_failure("mailbox_job_not_due"),
            MailboxJobPortErrorClass::InvalidState
        );
        assert_eq!(
            classify_write_failure("UNIQUE constraint failed: mailbox_jobs.tenant_id"),
            MailboxJobPortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("mailbox_provider_status_invalid"),
            MailboxJobPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("mailbox_job_version_overflow"),
            MailboxJobPortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_write_failure("network request failed"),
            MailboxJobPortErrorClass::DependencyUnavailable
        );
    }
}
