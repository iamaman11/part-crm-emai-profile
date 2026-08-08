use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::{MutationEnvelope, ResolvedMembershipRole};
use crate::d1_profile_generations::{
    ActivateGenerationMutation, D1ProfileGenerationRepository, DeactivateGenerationMutation,
    GenerationStatus as D1GenerationStatus, QuarantineGenerationMutation,
    RegisterGenerationMutation, VerifyGenerationMutation,
};
use application_ports::CommandExecutionEvidence;
use application_ports::generations::{
    GenerationApplicationPort, GenerationPortError, GenerationPortErrorClass,
    GenerationProfileVersionWrite, GenerationReadModel, GenerationReplayDecision,
    GenerationReplayReceipt, GenerationStatus, QuarantineGenerationWrite, RegisterGenerationWrite,
    VerifyGenerationWrite,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ActorId, GenerationId, ProfileId, TenantScope};
use worker::Error;
use worker::d1::D1Database;

pub struct D1ProfileGenerationApplicationRepository {
    generations: D1ProfileGenerationRepository,
    idempotency: D1IdempotencyRepository,
}

impl D1ProfileGenerationApplicationRepository {
    #[must_use]
    pub const fn new(generation_database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            generations: D1ProfileGenerationRepository::new(generation_database),
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }
}

impl GenerationApplicationPort for D1ProfileGenerationApplicationRepository {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<GenerationReplayDecision, GenerationPortError> {
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

    async fn register_generation(
        &self,
        actor: &ActorContext,
        write: &RegisterGenerationWrite,
    ) -> Result<(), GenerationPortError> {
        let evidence = write.evidence();
        self.generations
            .register(
                actor,
                RegisterGenerationMutation {
                    profile_id: write.profile_id(),
                    generation_id: write.generation_id(),
                    object_key: write.object_key(),
                    metadata_digest: write.metadata_digest(),
                    container_digest: write.container_digest(),
                    envelope: mutation_envelope(evidence, write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn verify_generation(
        &self,
        actor: &ActorContext,
        write: &VerifyGenerationWrite,
    ) -> Result<(), GenerationPortError> {
        let evidence = write.evidence();
        self.generations
            .verify(
                actor,
                VerifyGenerationMutation {
                    profile_id: write.profile_id(),
                    generation_id: write.generation_id(),
                    expected_generation_version: write.expected_generation_version(),
                    verification_reference: write.verification_reference(),
                    envelope: mutation_envelope(evidence, write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn activate_generation(
        &self,
        actor: &ActorContext,
        write: &GenerationProfileVersionWrite,
    ) -> Result<(), GenerationPortError> {
        let evidence = write.evidence();
        self.generations
            .activate(
                actor,
                ActivateGenerationMutation {
                    profile_id: write.profile_id(),
                    generation_id: write.generation_id(),
                    expected_profile_version: write.expected_profile_version(),
                    envelope: mutation_envelope(evidence, write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn deactivate_generation(
        &self,
        actor: &ActorContext,
        write: &GenerationProfileVersionWrite,
    ) -> Result<(), GenerationPortError> {
        let evidence = write.evidence();
        self.generations
            .deactivate(
                actor,
                DeactivateGenerationMutation {
                    profile_id: write.profile_id(),
                    generation_id: write.generation_id(),
                    expected_profile_version: write.expected_profile_version(),
                    envelope: mutation_envelope(evidence, write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn quarantine_generation(
        &self,
        actor: &ActorContext,
        write: &QuarantineGenerationWrite,
    ) -> Result<(), GenerationPortError> {
        let evidence = write.evidence();
        self.generations
            .quarantine(
                actor,
                QuarantineGenerationMutation {
                    profile_id: write.profile_id(),
                    generation_id: write.generation_id(),
                    expected_generation_version: write.expected_generation_version(),
                    envelope: mutation_envelope(evidence, write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn find_visible_generation(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<Option<GenerationReadModel>, GenerationPortError> {
        self.generations
            .find_visible(scope, actor_id, map_role(role), profile_id, generation_id)
            .await
            .map_err(map_dependency_error)
            .map(|projection| {
                projection.map(|projection| {
                    GenerationReadModel::new(
                        projection.generation_id().clone(),
                        projection.metadata_digest(),
                        projection.container_digest(),
                        map_status(projection.status()),
                        projection.version(),
                        projection.verification_reference().map(str::to_owned),
                    )
                })
            })
    }
}

fn mutation_envelope<'a>(
    evidence: &'a CommandExecutionEvidence,
    payload_json: &'a str,
) -> MutationEnvelope<'a> {
    MutationEnvelope {
        idempotency_key: evidence.idempotency_key(),
        request_digest: evidence.request_digest(),
        audit_event_id: evidence.audit_event_id(),
        outbox_event_id: evidence.outbox_event_id(),
        payload_json,
        now: evidence.now(),
        idempotency_expires_at: evidence.idempotency_expires_at(),
    }
}

fn map_role(role: MembershipRole) -> ResolvedMembershipRole {
    match role {
        MembershipRole::TenantOwner => ResolvedMembershipRole::TenantOwner,
        MembershipRole::Member => ResolvedMembershipRole::Member,
    }
}

fn map_status(status: D1GenerationStatus) -> GenerationStatus {
    match status {
        D1GenerationStatus::Registered => GenerationStatus::Registered,
        D1GenerationStatus::Verified => GenerationStatus::Verified,
        D1GenerationStatus::Quarantined => GenerationStatus::Quarantined,
    }
}

fn map_replay_decision(decision: IdempotencyDecision) -> GenerationReplayDecision {
    match decision {
        IdempotencyDecision::Miss => GenerationReplayDecision::Miss,
        IdempotencyDecision::Replay(receipt) => {
            GenerationReplayDecision::Replay(GenerationReplayReceipt::new(
                receipt.result_code(),
                receipt.result_reference().map(str::to_owned),
            ))
        }
        IdempotencyDecision::Conflict => GenerationReplayDecision::Conflict,
    }
}

fn map_write_error(error: Error) -> GenerationPortError {
    GenerationPortError::new(classify_write_failure(&error.to_string()))
}

fn map_dependency_error(_error: Error) -> GenerationPortError {
    GenerationPortError::new(GenerationPortErrorClass::DependencyUnavailable)
}

fn classify_write_failure(message: &str) -> GenerationPortErrorClass {
    if message.contains("owner_required")
        || message.contains("profile_missing")
        || message.contains("generation_missing")
        || message.contains("profile_generation_missing")
        || message.contains("not_found")
    {
        return GenerationPortErrorClass::NotFound;
    }
    if message.contains("state_mismatch") || message.contains("version_mismatch") {
        return GenerationPortErrorClass::VersionConflict;
    }
    if message.contains("not_verified")
        || message.contains("active_profile_generation_cannot_be_quarantined")
        || message.contains("invalid_state")
        || message.contains("active_generation_forbidden")
        || message.contains("profile_generation_relation_invalid")
    {
        return GenerationPortErrorClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return GenerationPortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
    {
        return GenerationPortErrorClass::IntegrityFailure;
    }
    if message.contains("value exceeds SQLite INTEGER")
        || message.contains("aggregate version overflow")
    {
        return GenerationPortErrorClass::InternalFailure;
    }
    GenerationPortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::classify_write_failure;
    use application_ports::generations::GenerationPortErrorClass;

    #[test]
    fn generation_write_failure_mapping_matches_worker_taxonomy() {
        assert_eq!(
            classify_write_failure("profile_generation_register_profile_missing"),
            GenerationPortErrorClass::NotFound
        );
        assert_eq!(
            classify_write_failure("profile_generation_activate_profile_state_mismatch"),
            GenerationPortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_write_failure("profile_generation_not_verified"),
            GenerationPortErrorClass::InvalidState
        );
        assert_eq!(
            classify_write_failure("active_profile_generation_cannot_be_quarantined"),
            GenerationPortErrorClass::InvalidState
        );
        assert_eq!(
            classify_write_failure("UNIQUE constraint failed: profile_generations.tenant_id"),
            GenerationPortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("CHECK constraint failed"),
            GenerationPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("aggregate version overflow"),
            GenerationPortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_write_failure("network unavailable"),
            GenerationPortErrorClass::DependencyUnavailable
        );
    }
}
