use super::{Membership, MembershipRole, MembershipStatus};
use core::fmt;
use profile_platform_primitives::{ActorContext, ActorId, CorrelationId, TenantScope};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TenantBoundarySummary {
    membership_count: u64,
    active_owner_count: u64,
}

impl TenantBoundarySummary {
    #[must_use]
    pub const fn new(membership_count: u64, active_owner_count: u64) -> Self {
        Self {
            membership_count,
            active_owner_count,
        }
    }

    #[must_use]
    pub const fn membership_count(self) -> u64 {
        self.membership_count
    }

    #[must_use]
    pub const fn active_owner_count(self) -> u64 {
        self.active_owner_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerBootstrapDecision {
    Create(Membership),
    Existing(Membership),
}

pub fn decide_owner_bootstrap(
    scope: &TenantScope,
    requested_actor_id: ActorId,
    boundary: TenantBoundarySummary,
    existing_membership: Option<&Membership>,
) -> Result<OwnerBootstrapDecision, MembershipLifecycleError> {
    if let Some(existing) = existing_membership {
        if existing.tenant_id() == scope.tenant_id()
            && existing.actor_id() == &requested_actor_id
            && existing.role() == MembershipRole::TenantOwner
            && existing.status() == MembershipStatus::Active
        {
            return Ok(OwnerBootstrapDecision::Existing(existing.clone()));
        }
        return Err(MembershipLifecycleError::BoundaryNotEmpty);
    }

    if boundary.membership_count() != 0 || boundary.active_owner_count() != 0 {
        return Err(MembershipLifecycleError::BoundaryNotEmpty);
    }

    Ok(OwnerBootstrapDecision::Create(Membership::new(
        scope.tenant_id().clone(),
        requested_actor_id,
        MembershipRole::TenantOwner,
        MembershipStatus::Active,
    )))
}

pub fn resolve_actor_context(
    scope: TenantScope,
    membership: &Membership,
    correlation_id: CorrelationId,
) -> Result<ActorContext, MembershipLifecycleError> {
    if membership.tenant_id() != scope.tenant_id() {
        return Err(MembershipLifecycleError::TenantMismatch);
    }
    if membership.status() != MembershipStatus::Active {
        return Err(MembershipLifecycleError::MembershipInactive);
    }

    Ok(ActorContext::new(
        scope,
        membership.actor_id().clone(),
        correlation_id,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerTransferDecision {
    previous_owner: Membership,
    next_owner: Membership,
}

impl OwnerTransferDecision {
    #[must_use]
    pub const fn previous_owner(&self) -> &Membership {
        &self.previous_owner
    }

    #[must_use]
    pub const fn next_owner(&self) -> &Membership {
        &self.next_owner
    }
}

pub fn decide_owner_transfer(
    actor: &ActorContext,
    current_owner: &Membership,
    next_owner: &Membership,
    boundary: TenantBoundarySummary,
) -> Result<OwnerTransferDecision, MembershipLifecycleError> {
    if actor.tenant_scope().tenant_id() != current_owner.tenant_id()
        || actor.tenant_scope().tenant_id() != next_owner.tenant_id()
    {
        return Err(MembershipLifecycleError::TenantMismatch);
    }
    if actor.actor_id() != current_owner.actor_id()
        || current_owner.role() != MembershipRole::TenantOwner
        || current_owner.status() != MembershipStatus::Active
    {
        return Err(MembershipLifecycleError::OwnerRequired);
    }
    if next_owner.actor_id() == current_owner.actor_id()
        || next_owner.role() != MembershipRole::Member
        || next_owner.status() != MembershipStatus::Active
    {
        return Err(MembershipLifecycleError::InvalidSuccessor);
    }
    if boundary.active_owner_count() != 1 {
        return Err(MembershipLifecycleError::OwnerInvariantViolation);
    }

    Ok(OwnerTransferDecision {
        previous_owner: Membership::new(
            current_owner.tenant_id().clone(),
            current_owner.actor_id().clone(),
            MembershipRole::Member,
            MembershipStatus::Active,
        ),
        next_owner: Membership::new(
            next_owner.tenant_id().clone(),
            next_owner.actor_id().clone(),
            MembershipRole::TenantOwner,
            MembershipStatus::Active,
        ),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipCommand {
    Activate,
    Suspend,
    Revoke,
}

pub fn decide_membership_status_change(
    actor: &ActorContext,
    actor_membership: &Membership,
    target: &Membership,
    boundary: TenantBoundarySummary,
    command: MembershipCommand,
) -> Result<Membership, MembershipLifecycleError> {
    require_active_owner(actor, actor_membership)?;
    if actor.tenant_scope().tenant_id() != target.tenant_id() {
        return Err(MembershipLifecycleError::TenantMismatch);
    }

    let next_status = match command {
        MembershipCommand::Activate => MembershipStatus::Active,
        MembershipCommand::Suspend => MembershipStatus::Suspended,
        MembershipCommand::Revoke => MembershipStatus::Revoked,
    };

    if target.role() == MembershipRole::TenantOwner
        && target.status() == MembershipStatus::Active
        && next_status != MembershipStatus::Active
        && boundary.active_owner_count() <= 1
    {
        return Err(MembershipLifecycleError::LastActiveOwner);
    }
    if target.status() == MembershipStatus::Revoked && command != MembershipCommand::Revoke {
        return Err(MembershipLifecycleError::InvalidTransition);
    }

    Ok(Membership::new(
        target.tenant_id().clone(),
        target.actor_id().clone(),
        target.role(),
        next_status,
    ))
}

pub fn require_active_owner(
    actor: &ActorContext,
    membership: &Membership,
) -> Result<(), MembershipLifecycleError> {
    if actor.tenant_scope().tenant_id() != membership.tenant_id() {
        return Err(MembershipLifecycleError::TenantMismatch);
    }
    if actor.actor_id() != membership.actor_id()
        || membership.role() != MembershipRole::TenantOwner
        || membership.status() != MembershipStatus::Active
    {
        return Err(MembershipLifecycleError::OwnerRequired);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipLifecycleError {
    TenantMismatch,
    MembershipInactive,
    BoundaryNotEmpty,
    OwnerRequired,
    InvalidSuccessor,
    OwnerInvariantViolation,
    LastActiveOwner,
    InvalidTransition,
}

impl fmt::Display for MembershipLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TenantMismatch => "membership tenant mismatch",
            Self::MembershipInactive => "membership is not active",
            Self::BoundaryNotEmpty => "tenant boundary is not empty",
            Self::OwnerRequired => "active tenant owner is required",
            Self::InvalidSuccessor => "owner successor must be a different active member",
            Self::OwnerInvariantViolation => "tenant owner invariant is violated",
            Self::LastActiveOwner => "last active owner cannot be suspended or revoked",
            Self::InvalidTransition => "membership status transition is invalid",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for MembershipLifecycleError {}

#[cfg(test)]
mod tests {
    use super::{
        MembershipCommand, MembershipLifecycleError, OwnerBootstrapDecision, TenantBoundarySummary,
        decide_membership_status_change, decide_owner_bootstrap, decide_owner_transfer,
        resolve_actor_context,
    };
    use crate::{Membership, MembershipRole, MembershipStatus};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, TenantId, TenantScope,
    };

    struct Fixture {
        scope: TenantScope,
        owner: Membership,
        member: Membership,
        actor: ActorContext,
    }

    fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JLIFECYCLE")?;
        let owner_id = ActorId::parse("actor_01JOWNER")?;
        let member_id = ActorId::parse("actor_01JMEMBER")?;
        let scope = TenantScope::new(tenant_id.clone());
        let owner = Membership::new(
            tenant_id.clone(),
            owner_id.clone(),
            MembershipRole::TenantOwner,
            MembershipStatus::Active,
        );
        let member = Membership::new(
            tenant_id,
            member_id,
            MembershipRole::Member,
            MembershipStatus::Active,
        );
        let actor = ActorContext::new(
            scope.clone(),
            owner_id,
            CorrelationId::parse("corr_01JLIFECYCLE")?,
        );
        Ok(Fixture {
            scope,
            owner,
            member,
            actor,
        })
    }

    #[test]
    fn bootstrap_is_empty_boundary_only_and_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let decision = decide_owner_bootstrap(
            &fixture.scope,
            fixture.owner.actor_id().clone(),
            TenantBoundarySummary::new(0, 0),
            None,
        )?;
        assert!(matches!(decision, OwnerBootstrapDecision::Create(_)));

        let replay = decide_owner_bootstrap(
            &fixture.scope,
            fixture.owner.actor_id().clone(),
            TenantBoundarySummary::new(1, 1),
            Some(&fixture.owner),
        )?;
        assert!(matches!(replay, OwnerBootstrapDecision::Existing(_)));
        assert_eq!(
            decide_owner_bootstrap(
                &fixture.scope,
                fixture.member.actor_id().clone(),
                TenantBoundarySummary::new(1, 1),
                None,
            ),
            Err(MembershipLifecycleError::BoundaryNotEmpty)
        );
        Ok(())
    }

    #[test]
    fn actor_resolution_denies_suspended_and_revoked_members()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let correlation = CorrelationId::parse("corr_02JLIFECYCLE")?;
        let active = resolve_actor_context(fixture.scope.clone(), &fixture.member, correlation)?;
        assert_eq!(active.actor_id(), fixture.member.actor_id());

        for status in [MembershipStatus::Suspended, MembershipStatus::Revoked] {
            let inactive = Membership::new(
                fixture.scope.tenant_id().clone(),
                fixture.member.actor_id().clone(),
                MembershipRole::Member,
                status,
            );
            assert_eq!(
                resolve_actor_context(
                    fixture.scope.clone(),
                    &inactive,
                    CorrelationId::parse("corr_03JLIFECYCLE")?,
                ),
                Err(MembershipLifecycleError::MembershipInactive)
            );
        }
        Ok(())
    }

    #[test]
    fn transfer_is_atomic_decision_and_last_owner_cannot_be_removed()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let transfer = decide_owner_transfer(
            &fixture.actor,
            &fixture.owner,
            &fixture.member,
            TenantBoundarySummary::new(2, 1),
        )?;
        assert_eq!(transfer.previous_owner().role(), MembershipRole::Member);
        assert_eq!(transfer.next_owner().role(), MembershipRole::TenantOwner);

        assert_eq!(
            decide_membership_status_change(
                &fixture.actor,
                &fixture.owner,
                &fixture.owner,
                TenantBoundarySummary::new(2, 1),
                MembershipCommand::Revoke,
            ),
            Err(MembershipLifecycleError::LastActiveOwner)
        );
        Ok(())
    }
}
