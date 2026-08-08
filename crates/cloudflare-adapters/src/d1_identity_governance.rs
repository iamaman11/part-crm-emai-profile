use crate::d1_governed_commands::D1GovernedCommandRepository;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::{
    CreateInvitationMutation, MembershipStatusMutation, MembershipStatusValue, MutationEnvelope,
    OwnerTransferMutation,
};
use crate::d1_identity_failure::{map_identity_dependency_error, map_identity_write_error};
use application_ports::CommandExecutionEvidence;
use application_ports::identity_governance::{
    ActiveOwnerGovernanceApplicationPort, IdentityGovernancePortError, IdentityReplayDecision,
    IdentityReplayReceipt, InvitationCreateWrite, MembershipStatusTarget, MembershipStatusWrite,
    OwnerTransferWrite,
};
use profile_platform_primitives::ActorContext;
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
            .map_err(map_identity_dependency_error)
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
            .map_err(map_identity_write_error)
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
            .map_err(map_identity_write_error)
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
            .map_err(map_identity_write_error)
    }
}

pub(crate) fn mutation_envelope<'a>(
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

pub(crate) fn map_replay_decision(decision: IdempotencyDecision) -> IdentityReplayDecision {
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
