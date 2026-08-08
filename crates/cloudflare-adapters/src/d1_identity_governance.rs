use crate::d1_governed_commands::D1GovernedCommandRepository;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::{
    CreateInvitationMutation, MembershipStatusMutation, MembershipStatusValue, MutationEnvelope,
    OwnerTransferMutation,
};
use application_ports::CommandExecutionEvidence;
use application_ports::identity_governance::{
    ActiveOwnerGovernanceApplicationPort, IdentityGovernancePortError,
    IdentityGovernancePortErrorClass, IdentityReplayDecision, IdentityReplayReceipt,
    InvitationCreateWrite, MembershipStatusTarget, MembershipStatusWrite, OwnerTransferWrite,
};
use profile_platform_primitives::ActorContext;
use worker::Error;
use worker::d1::D1Database;

pub struct D1IdentityGovernanceApplicationRepository {
    governed: D1GovernedCommandRepository,
    idempotency: D1IdempotencyRepository,
}

impl D1IdentityGovernanceApplicationRepository {
    #[must_use]
    pub const fn new(governed_database: D1Database, idempotency_database: D1Database) -> Self {
        Self {
            governed: D1GovernedCommandRepository::new(governed_database),
            idempotency: D1IdempotencyRepository::new(idempotency_database),
        }
    }
}

impl ActiveOwnerGovernanceApplicationPort for D1IdentityGovernanceApplicationRepository {
    async fn decide_identity_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IdentityReplayDecision, IdentityGovernancePortError> {
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

    async fn transfer_owner(
        &self,
        actor: &ActorContext,
        write: &OwnerTransferWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.governed
            .transfer_owner(
                actor,
                OwnerTransferMutation {
                    next_owner_actor_id: write.next_owner_actor_id(),
                    current_owner_version: write.current_owner_version(),
                    next_owner_version: write.next_owner_version(),
                    envelope: mutation_envelope(write.evidence(), write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn create_invitation(
        &self,
        actor: &ActorContext,
        write: &InvitationCreateWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.governed
            .create_invitation(
                actor,
                CreateInvitationMutation {
                    invitation_id: write.invitation_id(),
                    invited_contact_hmac: write.invited_contact_hmac(),
                    expires_at: write.expires_at(),
                    tenant_expected_version: write.tenant_expected_version(),
                    envelope: mutation_envelope(write.evidence(), write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn update_membership_status(
        &self,
        actor: &ActorContext,
        write: &MembershipStatusWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.governed
            .update_membership_status(
                actor,
                MembershipStatusMutation {
                    target_actor_id: write.target_actor_id(),
                    expected_version: write.expected_version(),
                    next_status: membership_status(write.next_status()),
                    envelope: mutation_envelope(write.evidence(), write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_write_error)
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

fn map_replay_decision(decision: IdempotencyDecision) -> IdentityReplayDecision {
    match decision {
        IdempotencyDecision::Miss => IdentityReplayDecision::Miss,
        IdempotencyDecision::Replay(receipt) => {
            IdentityReplayDecision::Replay(IdentityReplayReceipt::new(
                receipt.result_code(),
                receipt.result_reference().map(str::to_owned),
            ))
        }
        IdempotencyDecision::Conflict => IdentityReplayDecision::Conflict,
    }
}

const fn membership_status(status: MembershipStatusTarget) -> MembershipStatusValue {
    match status {
        MembershipStatusTarget::Active => MembershipStatusValue::Active,
        MembershipStatusTarget::Suspended => MembershipStatusValue::Suspended,
        MembershipStatusTarget::Revoked => MembershipStatusValue::Revoked,
    }
}

fn map_dependency_error(_error: Error) -> IdentityGovernancePortError {
    IdentityGovernancePortError::new(IdentityGovernancePortErrorClass::DependencyUnavailable)
}

fn map_write_error(error: Error) -> IdentityGovernancePortError {
    IdentityGovernancePortError::new(classify_write_failure(&error.to_string()))
}

fn classify_write_failure(message: &str) -> IdentityGovernancePortErrorClass {
    if message.contains("owner_required")
        || message.contains("target_missing")
        || message.contains("successor_mismatch")
    {
        return IdentityGovernancePortErrorClass::NotFound;
    }
    if message.contains("version_mismatch")
        || message.contains("current_owner_mismatch")
        || message.contains("tenant_version_mismatch")
    {
        return IdentityGovernancePortErrorClass::VersionConflict;
    }
    if message.contains("last_active_owner")
        || message.contains("invalid_transition")
        || message.contains("time_regression")
    {
        return IdentityGovernancePortErrorClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return IdentityGovernancePortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
    {
        return IdentityGovernancePortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return IdentityGovernancePortErrorClass::InternalFailure;
    }
    IdentityGovernancePortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::classify_write_failure;
    use application_ports::identity_governance::IdentityGovernancePortErrorClass;

    #[test]
    fn identity_governance_write_failures_keep_public_classes_stable() {
        assert_eq!(
            classify_write_failure("owner_transfer_successor_mismatch"),
            IdentityGovernancePortErrorClass::NotFound
        );
        assert_eq!(
            classify_write_failure("membership_status_target_missing"),
            IdentityGovernancePortErrorClass::NotFound
        );
        assert_eq!(
            classify_write_failure("owner_transfer_current_owner_mismatch"),
            IdentityGovernancePortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_write_failure("owner_transfer_successor_version_mismatch"),
            IdentityGovernancePortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_write_failure("invitation_create_tenant_version_mismatch"),
            IdentityGovernancePortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_write_failure("last_active_owner"),
            IdentityGovernancePortErrorClass::InvalidState
        );
        assert_eq!(
            classify_write_failure("UNIQUE constraint failed: membership_status_commands"),
            IdentityGovernancePortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("CHECK constraint failed: memberships"),
            IdentityGovernancePortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("aggregate version overflow"),
            IdentityGovernancePortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_write_failure("network request failed"),
            IdentityGovernancePortErrorClass::DependencyUnavailable
        );
    }
}
