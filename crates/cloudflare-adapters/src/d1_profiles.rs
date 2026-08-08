use crate::d1_governed_commands::D1GovernedCommandRepository;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::{
    AssignProfileMutation, CreateProfileMutation, MutationEnvelope, ResolvedMembershipRole,
};
use crate::d1_identity_queries::{D1IdentityQueryRepository, ProfileProjection};
use application_ports::CommandExecutionEvidence;
use application_ports::profiles::{
    ProfileApplicationPort, ProfileAssignmentApplicationPort, ProfileAssignmentPortError,
    ProfileAssignmentPortErrorClass, ProfileAssignmentWrite, ProfileCreateWrite, ProfilePortError,
    ProfilePortErrorClass, ProfileReadModel, ProfileReplayDecision, ProfileReplayReceipt,
    ProfileStatus,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, ProfileId, TenantScope,
};
use worker::Error;
use worker::d1::D1Database;

pub struct D1ProfileApplicationRepository {
    governed: D1GovernedCommandRepository,
    idempotency: D1IdempotencyRepository,
    queries: D1IdentityQueryRepository,
}

impl D1ProfileApplicationRepository {
    #[must_use]
    pub const fn new(
        governed_database: D1Database,
        idempotency_database: D1Database,
        query_database: D1Database,
    ) -> Self {
        Self {
            governed: D1GovernedCommandRepository::new(governed_database),
            idempotency: D1IdempotencyRepository::new(idempotency_database),
            queries: D1IdentityQueryRepository::new(query_database),
        }
    }
}

impl ProfileApplicationPort for D1ProfileApplicationRepository {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ProfileReplayDecision, ProfilePortError> {
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

    async fn create_profile(
        &self,
        actor: &ActorContext,
        write: &ProfileCreateWrite,
    ) -> Result<(), ProfilePortError> {
        let evidence = write.evidence();
        let mutation = CreateProfileMutation {
            profile_id: write.profile().profile_id(),
            envelope: mutation_envelope(evidence, write.event_payload_json()),
        };
        self.governed
            .create_profile(actor, mutation)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn find_visible_profile(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileReadModel>, ProfilePortError> {
        self.queries
            .find_visible_profile(scope, actor_id, resolved_role(role), profile_id)
            .await
            .map_err(map_dependency_error)?
            .map(profile_read_model)
            .transpose()
    }
}

impl ProfileAssignmentApplicationPort for D1ProfileApplicationRepository {
    async fn decide_assignment_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ProfileReplayDecision, ProfileAssignmentPortError> {
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
            .map_err(map_assignment_dependency_error)
    }

    async fn assign_profile(
        &self,
        actor: &ActorContext,
        write: &ProfileAssignmentWrite,
    ) -> Result<(), ProfileAssignmentPortError> {
        let evidence = write.evidence();
        let mutation = AssignProfileMutation {
            assignment_id: write.assignment_id(),
            profile_id: write.profile_id(),
            client_id: write.client_id(),
            expected_profile_version: write.expected_profile_version(),
            reason: write.reason(),
            envelope: mutation_envelope(evidence, write.event_payload_json()),
        };
        self.governed
            .assign_profile(actor, mutation)
            .await
            .map(|_| ())
            .map_err(map_assignment_write_error)
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

fn map_replay_decision(decision: IdempotencyDecision) -> ProfileReplayDecision {
    match decision {
        IdempotencyDecision::Miss => ProfileReplayDecision::Miss,
        IdempotencyDecision::Replay(receipt) => {
            ProfileReplayDecision::Replay(ProfileReplayReceipt::new(
                receipt.result_code(),
                receipt.result_reference().map(str::to_owned),
            ))
        }
        IdempotencyDecision::Conflict => ProfileReplayDecision::Conflict,
    }
}

const fn resolved_role(role: MembershipRole) -> ResolvedMembershipRole {
    match role {
        MembershipRole::TenantOwner => ResolvedMembershipRole::TenantOwner,
        MembershipRole::Member => ResolvedMembershipRole::Member,
    }
}

fn profile_read_model(projection: ProfileProjection) -> Result<ProfileReadModel, ProfilePortError> {
    let status = match projection.status() {
        "DRAFT" => ProfileStatus::Draft,
        "QUARANTINED" => ProfileStatus::Quarantined,
        "READY" => ProfileStatus::Ready,
        "IN_USE" => ProfileStatus::InUse,
        "DIRTY_LOCAL" => ProfileStatus::DirtyLocal,
        "SYNCING" => ProfileStatus::Syncing,
        "SUSPENDED" => ProfileStatus::Suspended,
        "DELETING" => ProfileStatus::Deleting,
        "DELETED" => ProfileStatus::Deleted,
        _ => return Err(integrity_failure()),
    };
    let version = AggregateVersion::new(projection.version()).map_err(|_| integrity_failure())?;
    Ok(ProfileReadModel::new(
        projection.profile_id().clone(),
        status,
        version,
        projection.linked_client_id().cloned(),
    ))
}

fn map_write_error(error: Error) -> ProfilePortError {
    ProfilePortError::new(classify_write_failure(&error.to_string()))
}

fn map_dependency_error(_error: Error) -> ProfilePortError {
    ProfilePortError::new(ProfilePortErrorClass::DependencyUnavailable)
}

const fn integrity_failure() -> ProfilePortError {
    ProfilePortError::new(ProfilePortErrorClass::IntegrityFailure)
}

fn classify_write_failure(message: &str) -> ProfilePortErrorClass {
    if message.contains("UNIQUE constraint failed") {
        return ProfilePortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
    {
        return ProfilePortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return ProfilePortErrorClass::InternalFailure;
    }
    ProfilePortErrorClass::DependencyUnavailable
}

fn map_assignment_write_error(error: Error) -> ProfileAssignmentPortError {
    ProfileAssignmentPortError::new(classify_assignment_write_failure(&error.to_string()))
}

fn map_assignment_dependency_error(_error: Error) -> ProfileAssignmentPortError {
    ProfileAssignmentPortError::new(ProfileAssignmentPortErrorClass::DependencyUnavailable)
}

fn classify_assignment_write_failure(message: &str) -> ProfileAssignmentPortErrorClass {
    if message.contains("owner_required")
        || message.contains("profile_missing")
        || message.contains("client_missing")
        || message.contains("client_not_active")
        || message.contains("target_missing")
    {
        return ProfileAssignmentPortErrorClass::NotFound;
    }
    if message.contains("state_mismatch")
        || message.contains("version_mismatch")
        || message.contains("tenant_version_mismatch")
    {
        return ProfileAssignmentPortErrorClass::VersionConflict;
    }
    if message.contains("time_regression")
        || message.contains("invalid_transition")
        || message.contains("grant_missing")
    {
        return ProfileAssignmentPortErrorClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return ProfileAssignmentPortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
    {
        return ProfileAssignmentPortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return ProfileAssignmentPortErrorClass::InternalFailure;
    }
    ProfileAssignmentPortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::{classify_assignment_write_failure, classify_write_failure};
    use application_ports::profiles::{ProfileAssignmentPortErrorClass, ProfilePortErrorClass};

    #[test]
    fn profile_write_failures_keep_public_classes_stable() {
        assert_eq!(
            classify_write_failure(
                "UNIQUE constraint failed: profile_create_commands.tenant_id, profile_create_commands.command_id"
            ),
            ProfilePortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("CHECK constraint failed: browser_profiles"),
            ProfilePortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("value exceeds SQLite INTEGER"),
            ProfilePortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_write_failure("network request failed"),
            ProfilePortErrorClass::DependencyUnavailable
        );
    }

    #[test]
    fn assignment_write_failures_keep_legacy_public_classes() {
        assert_eq!(
            classify_assignment_write_failure("profile_assignment_profile_missing"),
            ProfileAssignmentPortErrorClass::NotFound
        );
        assert_eq!(
            classify_assignment_write_failure("profile_assignment_client_not_active"),
            ProfileAssignmentPortErrorClass::NotFound
        );
        assert_eq!(
            classify_assignment_write_failure("profile_assignment_profile_version_mismatch"),
            ProfileAssignmentPortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_assignment_write_failure(
                "UNIQUE constraint failed: profile_assignment_commands.tenant_id"
            ),
            ProfileAssignmentPortErrorClass::Conflict
        );
        assert_eq!(
            classify_assignment_write_failure("profile_assignment_not_governed"),
            ProfileAssignmentPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_assignment_write_failure("aggregate version overflow"),
            ProfileAssignmentPortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_assignment_write_failure("network request failed"),
            ProfileAssignmentPortErrorClass::DependencyUnavailable
        );
    }
}
