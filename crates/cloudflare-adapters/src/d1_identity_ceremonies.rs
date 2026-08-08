use crate::access_identity::VerifiedExternalIdentity;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::{
    BootstrapOwnerMutation, D1IdentityAclRepository, ResolvedMembershipRole,
    VerifiedBootstrapContext,
};
use crate::d1_identity_failure::{map_identity_dependency_error, map_identity_write_error};
use crate::d1_identity_governance::{map_replay_decision, mutation_envelope};
use crate::d1_invitation_acceptance::{
    AcceptInvitationMutation, D1InvitationAcceptanceRepository,
};
use application_ports::CommandExecutionEvidence;
use application_ports::identity_ceremonies::{
    ActiveIdentityBinding, BootstrapOwnerWrite, IdentityCeremonyApplicationPort,
    InvitationAcceptWrite, TenantIdentityBoundary, VerifiedIdentityCeremonyContext,
    VerifiedIdentitySnapshot,
};
use application_ports::identity_governance::{
    IdentityGovernancePortError, IdentityGovernancePortErrorClass, IdentityReplayDecision,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorId, CorrelationId, TenantScope};
use worker::d1::D1Database;

pub struct D1IdentityCeremonyApplicationRepository {
    identity: D1IdentityAclRepository,
    idempotency: D1IdempotencyRepository,
    invitation_acceptance: D1InvitationAcceptanceRepository,
    verified_identity: VerifiedExternalIdentity,
}

impl D1IdentityCeremonyApplicationRepository {
    #[must_use]
    pub const fn new(
        identity_database: D1Database,
        idempotency_database: D1Database,
        invitation_acceptance_database: D1Database,
        verified_identity: VerifiedExternalIdentity,
    ) -> Self {
        Self {
            identity: D1IdentityAclRepository::new(identity_database),
            idempotency: D1IdempotencyRepository::new(idempotency_database),
            invitation_acceptance: D1InvitationAcceptanceRepository::new(
                invitation_acceptance_database,
            ),
            verified_identity,
        }
    }

    fn snapshot_matches(&self, snapshot: &VerifiedIdentitySnapshot) -> bool {
        snapshot_matches(&self.verified_identity, snapshot)
    }
}

impl IdentityCeremonyApplicationPort for D1IdentityCeremonyApplicationRepository {
    async fn find_active_identity_binding(
        &self,
        scope: &TenantScope,
        identity: &VerifiedIdentitySnapshot,
        correlation_id: &CorrelationId,
    ) -> Result<Option<ActiveIdentityBinding>, IdentityGovernancePortError> {
        if !self.snapshot_matches(identity) {
            return Err(IdentityGovernancePortError::new(
                IdentityGovernancePortErrorClass::NotFound,
            ));
        }
        self.identity
            .resolve_active_actor(
                scope.clone(),
                &self.verified_identity,
                correlation_id.clone(),
            )
            .await
            .map(|resolved| {
                resolved.map(|actor| {
                    let role = match actor.role() {
                        ResolvedMembershipRole::TenantOwner => MembershipRole::TenantOwner,
                        ResolvedMembershipRole::Member => MembershipRole::Member,
                    };
                    ActiveIdentityBinding::new(actor.actor().actor_id().clone(), role)
                })
            })
            .map_err(map_identity_dependency_error)
    }

    async fn tenant_identity_boundary(
        &self,
        scope: &TenantScope,
    ) -> Result<TenantIdentityBoundary, IdentityGovernancePortError> {
        self.identity
            .tenant_boundary(scope)
            .await
            .map(|boundary| {
                TenantIdentityBoundary::new(
                    boundary.membership_count,
                    boundary.active_owner_count,
                )
            })
            .map_err(map_identity_dependency_error)
    }

    async fn decide_ceremony_replay(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IdentityReplayDecision, IdentityGovernancePortError> {
        self.idempotency
            .decide(
                scope,
                actor_id,
                evidence.idempotency_key(),
                command_name,
                evidence.request_digest(),
                evidence.now(),
            )
            .await
            .map(map_replay_decision)
            .map_err(map_identity_dependency_error)
    }

    async fn bootstrap_owner(
        &self,
        context: &VerifiedIdentityCeremonyContext,
        write: &BootstrapOwnerWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        if !self.snapshot_matches(context.identity()) {
            return Err(IdentityGovernancePortError::new(
                IdentityGovernancePortErrorClass::NotFound,
            ));
        }
        let bootstrap_context = VerifiedBootstrapContext::from_verified_identity(
            context.scope().clone(),
            context.actor_id().clone(),
            context.correlation_id().clone(),
            &self.verified_identity,
        );
        self.identity
            .bootstrap_owner(
                &bootstrap_context,
                BootstrapOwnerMutation {
                    tenant_display_name: write.tenant_display_name(),
                    identity_id: write.identity_id(),
                    envelope: mutation_envelope(write.evidence(), write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_identity_write_error)
    }

    async fn accept_invitation(
        &self,
        context: &VerifiedIdentityCeremonyContext,
        write: &InvitationAcceptWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        if !self.snapshot_matches(context.identity()) {
            return Err(IdentityGovernancePortError::new(
                IdentityGovernancePortErrorClass::NotFound,
            ));
        }
        let bootstrap_context = VerifiedBootstrapContext::from_verified_identity(
            context.scope().clone(),
            context.actor_id().clone(),
            context.correlation_id().clone(),
            &self.verified_identity,
        );
        self.invitation_acceptance
            .accept(
                &bootstrap_context,
                &self.verified_identity,
                context.correlation_id(),
                AcceptInvitationMutation {
                    invitation_id: write.invitation_id(),
                    identity_id: write.identity_id(),
                    envelope: mutation_envelope(write.evidence(), write.event_payload_json()),
                },
            )
            .await
            .map(|_| ())
            .map_err(map_identity_write_error)
    }
}

fn snapshot_matches(
    verified_identity: &VerifiedExternalIdentity,
    snapshot: &VerifiedIdentitySnapshot,
) -> bool {
    snapshot.subject() == verified_identity.subject()
        && snapshot.contact_hint() == verified_identity.contact_hint()
}

#[cfg(test)]
mod tests {
    use super::snapshot_matches;
    use crate::access_identity::DeterministicFakeIdentityAdapter;
    use application_ports::identity_ceremonies::VerifiedIdentitySnapshot;

    #[test]
    fn provider_verified_snapshot_must_match_exactly() -> Result<(), Box<dyn std::error::Error>> {
        let verified = DeterministicFakeIdentityAdapter::new(
            "subject-01JIDENTITYCEREMONY",
            Some("contact-hint".to_owned()),
        )?
        .verify();
        assert!(snapshot_matches(
            &verified,
            &VerifiedIdentitySnapshot::new(
                "subject-01JIDENTITYCEREMONY",
                Some("contact-hint".to_owned()),
            )
        ));
        assert!(!snapshot_matches(
            &verified,
            &VerifiedIdentitySnapshot::new(
                "subject-01JOTHER",
                Some("contact-hint".to_owned()),
            )
        ));
        Ok(())
    }
}
