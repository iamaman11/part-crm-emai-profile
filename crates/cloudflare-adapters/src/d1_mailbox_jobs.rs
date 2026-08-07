use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::MutationEnvelope;
use crate::d1_mailboxes::{CreateMailboxJobMutation, D1MailboxRepository, RunMailboxJobMutation};
use crate::mailbox_provider::{
    MailboxProviderAdapterError, MailboxRunDecision, MetadataMailboxProviderAdapter,
    decide_mailbox_run,
};
use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_jobs::{
    MailboxBinding, MailboxJob, MailboxJobApplicationPort, MailboxJobCreateWrite,
    MailboxJobPortError, MailboxJobPortErrorClass, MailboxJobPreparedRun, MailboxJobReadModel,
    MailboxJobRunWrite,
};
use application_ports::mailboxes::{MailboxReplayDecision, MailboxReplayReceipt};
use profile_platform_primitives::{
    ActorContext, MailboxBindingId, MailboxJobId, TenantScope, UnixMillis,
};
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
    type RunDecision = MailboxRunDecision;

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
        write: &MailboxJobRunWrite<Self::RunDecision>,
    ) -> Result<(), MailboxJobPortError> {
        let evidence = write.evidence();
        let mutation = RunMailboxJobMutation {
            binding_id: write.binding_id(),
            job_id: write.job_id(),
            expected_job_version: write.expected_version(),
            decision: write.prepared().decision(),
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

    fn prepare_run(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
        now: UnixMillis,
    ) -> Result<MailboxJobPreparedRun<Self::RunDecision>, MailboxJobPortError> {
        let next_attempt = job
            .attempt()
            .checked_add(1)
            .ok_or_else(|| MailboxJobPortError::new(MailboxJobPortErrorClass::InternalFailure))?;
        let next_cursor = format!("meta_{}_{}", job.job_id().as_str(), next_attempt);
        let mut provider = MetadataMailboxProviderAdapter::new(
            binding.provider(),
            "SYNTHETIC_OK",
            0,
            Some(next_cursor),
        )
        .map_err(|_| MailboxJobPortError::new(MailboxJobPortErrorClass::InternalFailure))?;
        let decision =
            decide_mailbox_run(binding, job, now, &mut provider).map_err(map_provider_error)?;
        let status = decision.status();
        let attempt = decision.attempt();
        let version = decision.version();
        let cursor = decision.cursor().map(str::to_owned);
        let provider_status = decision.provider_status().to_owned();
        let bounded_item_count = decision.bounded_item_count();
        let retry_at = decision.retry_at();
        Ok(MailboxJobPreparedRun::new(
            decision,
            status,
            attempt,
            version,
            cursor,
            provider_status,
            bounded_item_count,
            retry_at,
        ))
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

fn map_provider_error(error: MailboxProviderAdapterError) -> MailboxJobPortError {
    let class = if error == MailboxProviderAdapterError::InvalidJobState {
        MailboxJobPortErrorClass::InvalidState
    } else {
        MailboxJobPortErrorClass::DependencyUnavailable
    };
    MailboxJobPortError::new(class)
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
    use super::{classify_write_failure, map_provider_error};
    use crate::mailbox_provider::MailboxProviderAdapterError;
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

    #[test]
    fn provider_failure_mapping_matches_legacy_worker_contract() {
        assert_eq!(
            map_provider_error(MailboxProviderAdapterError::InvalidJobState).class(),
            MailboxJobPortErrorClass::InvalidState
        );
        assert_eq!(
            map_provider_error(MailboxProviderAdapterError::RetryableFailure).class(),
            MailboxJobPortErrorClass::DependencyUnavailable
        );
        assert_eq!(
            map_provider_error(MailboxProviderAdapterError::BindingRevoked).class(),
            MailboxJobPortErrorClass::DependencyUnavailable
        );
    }
}
