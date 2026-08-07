use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::MutationEnvelope;
use crate::d1_mailboxes::{
    CreateMailboxBindingMutation, D1MailboxRepository, RevokeMailboxBindingMutation,
};
use application_ports::CommandExecutionEvidence;
use application_ports::mailboxes::{
    MailboxBindingApplicationPort, MailboxBindingCreateWrite, MailboxBindingPortError,
    MailboxBindingPortErrorClass, MailboxBindingReadModel, MailboxBindingRevokeWrite,
    MailboxReplayDecision, MailboxReplayReceipt,
};
use profile_platform_primitives::{ActorContext, MailboxBindingId, TenantScope};
use worker::Error;
use worker::d1::D1Database;

pub struct D1MailboxBindingApplicationRepository {
    mailboxes: D1MailboxRepository,
    idempotency: D1IdempotencyRepository,
}

impl D1MailboxBindingApplicationRepository {
    #[must_use]
    pub const fn new(mailbox_database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            mailboxes: D1MailboxRepository::new(mailbox_database),
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }
}

impl MailboxBindingApplicationPort for D1MailboxBindingApplicationRepository {
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

    async fn create_binding(
        &self,
        actor: &ActorContext,
        write: &MailboxBindingCreateWrite,
    ) -> Result<(), MailboxBindingPortError> {
        let evidence = write.evidence();
        let mutation = CreateMailboxBindingMutation {
            binding_id: write.binding().binding_id(),
            provider: write.binding().provider(),
            secret_handle: write.binding().secret_handle(),
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
            .create_binding(actor, mutation)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn revoke_binding(
        &self,
        actor: &ActorContext,
        write: &MailboxBindingRevokeWrite,
    ) -> Result<(), MailboxBindingPortError> {
        let evidence = write.evidence();
        let mutation = RevokeMailboxBindingMutation {
            binding_id: write.binding_id(),
            expected_binding_version: write.expected_version(),
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
            .revoke_binding(actor, mutation)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn find_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<MailboxBindingReadModel>, MailboxBindingPortError> {
        self.mailboxes
            .find_binding(scope, binding_id)
            .await
            .map_err(map_dependency_error)
            .map(|binding| {
                binding.map(|binding| {
                    MailboxBindingReadModel::new(
                        binding.binding_id().clone(),
                        binding.provider(),
                        binding.status(),
                        binding.version(),
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

fn map_write_error(error: Error) -> MailboxBindingPortError {
    MailboxBindingPortError::new(classify_write_failure(&error.to_string()))
}

fn map_dependency_error(_error: Error) -> MailboxBindingPortError {
    MailboxBindingPortError::new(MailboxBindingPortErrorClass::DependencyUnavailable)
}

fn classify_write_failure(message: &str) -> MailboxBindingPortErrorClass {
    if message.contains("mailbox_binding_missing") {
        return MailboxBindingPortErrorClass::NotFound;
    }
    if message.contains("version_mismatch") {
        return MailboxBindingPortErrorClass::VersionConflict;
    }
    if message.contains("mailbox_binding_revoked")
        || message.contains("mailbox_binding_already_revoked")
    {
        return MailboxBindingPortErrorClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return MailboxBindingPortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
    {
        return MailboxBindingPortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return MailboxBindingPortErrorClass::InternalFailure;
    }
    MailboxBindingPortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::classify_write_failure;
    use application_ports::mailboxes::MailboxBindingPortErrorClass;

    #[test]
    fn binding_write_failures_keep_public_classes_stable() {
        assert_eq!(
            classify_write_failure("mailbox_binding_missing"),
            MailboxBindingPortErrorClass::NotFound
        );
        assert_eq!(
            classify_write_failure("mailbox_binding_version_mismatch"),
            MailboxBindingPortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_write_failure("mailbox_binding_already_revoked"),
            MailboxBindingPortErrorClass::InvalidState
        );
        assert_eq!(
            classify_write_failure("UNIQUE constraint failed: mailbox_bindings.tenant_id"),
            MailboxBindingPortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("CHECK constraint failed: mailbox_bindings"),
            MailboxBindingPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("value exceeds SQLite INTEGER"),
            MailboxBindingPortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_write_failure("network request failed"),
            MailboxBindingPortErrorClass::DependencyUnavailable
        );
    }
}
