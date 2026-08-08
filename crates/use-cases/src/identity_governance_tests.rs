use super::*;
use profile_platform_primitives::{
    AuditEventId, CorrelationId, IdempotencyKey, OutboxEventId, TenantId, TenantScope,
};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::task::{Context, Poll, Waker};

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::hint::spin_loop(),
        }
    }
}

struct FakePort {
    replay: RefCell<Vec<IdentityReplayDecision>>,
    commands: RefCell<Vec<String>>,
    replay_calls: Cell<u32>,
    transfer_calls: Cell<u32>,
    invitation_calls: Cell<u32>,
    membership_calls: Cell<u32>,
    write_error: Cell<Option<IdentityGovernancePortErrorClass>>,
}

impl FakePort {
    fn new(replay: Vec<IdentityReplayDecision>) -> Self {
        Self {
            replay: RefCell::new(replay),
            commands: RefCell::new(Vec::new()),
            replay_calls: Cell::new(0),
            transfer_calls: Cell::new(0),
            invitation_calls: Cell::new(0),
            membership_calls: Cell::new(0),
            write_error: Cell::new(None),
        }
    }

    fn write_result(&self) -> Result<(), IdentityGovernancePortError> {
        match self.write_error.get() {
            Some(class) => Err(IdentityGovernancePortError::new(class)),
            None => Ok(()),
        }
    }
}

impl ActiveOwnerGovernanceApplicationPort for FakePort {
    async fn decide_identity_replay(
        &self,
        _actor: &ActorContext,
        command_name: &str,
        _evidence: &CommandExecutionEvidence,
    ) -> Result<IdentityReplayDecision, IdentityGovernancePortError> {
        self.replay_calls.set(self.replay_calls.get() + 1);
        self.commands.borrow_mut().push(command_name.to_owned());
        Ok(if self.replay.borrow().is_empty() {
            IdentityReplayDecision::Miss
        } else {
            self.replay.borrow_mut().remove(0)
        })
    }

    async fn transfer_owner(
        &self,
        _actor: &ActorContext,
        _write: &OwnerTransferWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.transfer_calls.set(self.transfer_calls.get() + 1);
        self.write_result()
    }

    async fn create_invitation(
        &self,
        _actor: &ActorContext,
        _write: &InvitationCreateWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.invitation_calls.set(self.invitation_calls.get() + 1);
        self.write_result()
    }

    async fn update_membership_status(
        &self,
        _actor: &ActorContext,
        _write: &MembershipStatusWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.membership_calls.set(self.membership_calls.get() + 1);
        self.write_result()
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_01JIDENTITYGOV")?),
        ActorId::parse("actor_01JIDENTITYOWNER")?,
        CorrelationId::parse("corr_01JIDENTITYGOV")?,
    ))
}

fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
    Ok(CommandExecutionEvidence::new(
        IdempotencyKey::parse("idem_01JIDENTITYGOV")?,
        "request-digest-01JIDENTITYGOV",
        AuditEventId::parse("audit_01JIDENTITYGOV")?,
        OutboxEventId::parse("outbox_01JIDENTITYGOV")?,
        UnixMillis::new(10),
        UnixMillis::new(100),
    ))
}

fn transfer_command(
    next_owner_version: AggregateVersion,
) -> Result<ExecuteOwnerTransferCommand, Box<dyn std::error::Error>> {
    Ok(ExecuteOwnerTransferCommand::new(
        ActorId::parse("actor_01JIDENTITYNEXT")?,
        AggregateVersion::INITIAL,
        next_owner_version,
        evidence()?,
    ))
}

fn membership_command(
    status: &str,
) -> Result<ExecuteMembershipStatusCommand, Box<dyn std::error::Error>> {
    Ok(ExecuteMembershipStatusCommand::new(
        ActorId::parse("actor_01JIDENTITYMEMBER")?,
        AggregateVersion::INITIAL,
        status,
        evidence()?,
    ))
}

#[test]
fn non_owner_stops_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
    assert_eq!(
        block_on(execute_owner_transfer(
            &actor()?,
            MembershipRole::Member,
            &port,
            transfer_command(AggregateVersion::INITIAL)?,
        )),
        Err(IdentityGovernanceOperationError::NotFound)
    );
    assert_eq!(port.replay_calls.get(), 0);
    assert_eq!(port.transfer_calls.get(), 0);
    Ok(())
}

#[test]
fn overflow_stops_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
    assert_eq!(
        block_on(execute_owner_transfer(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            transfer_command(AggregateVersion::new(u64::MAX)?)?,
        )),
        Err(IdentityGovernanceOperationError::InternalFailure)
    );
    assert_eq!(port.replay_calls.get(), 0);
    assert_eq!(port.transfer_calls.get(), 0);
    Ok(())
}

#[test]
fn invalid_status_stops_before_replay() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
    assert_eq!(
        block_on(execute_membership_status(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            membership_command("PAUSED")?,
        )),
        Err(IdentityGovernanceOperationError::InvalidRequest)
    );
    assert_eq!(port.replay_calls.get(), 0);
    assert_eq!(port.membership_calls.get(), 0);
    Ok(())
}

#[test]
fn membership_domains_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("ACTIVE", "membership.activate"),
        ("SUSPENDED", "membership.suspend"),
        ("REVOKED", "membership.revoke"),
    ];
    for (status, expected_command) in cases {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        block_on(execute_membership_status(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            membership_command(status)?,
        ))?;
        assert_eq!(port.commands.borrow().as_slice(), [expected_command]);
        assert_eq!(port.membership_calls.get(), 1);
    }
    Ok(())
}

#[test]
fn exact_replay_skips_owner_transfer_write() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(vec![IdentityReplayDecision::Replay(
        IdentityReplayReceipt::new("transferred", Some("actor_existing".to_owned())),
    )]);
    let outcome = block_on(execute_owner_transfer(
        &actor()?,
        MembershipRole::TenantOwner,
        &port,
        transfer_command(AggregateVersion::INITIAL)?,
    ))?;
    assert!(outcome.replayed());
    assert_eq!(outcome.result_code(), "transferred");
    assert_eq!(outcome.resource_id(), "actor_existing");
    assert_eq!(outcome.aggregate_version(), AggregateVersion::new(2)?);
    assert_eq!(port.transfer_calls.get(), 0);
    Ok(())
}

#[test]
fn conflict_rechecks_exact_replay_once() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(vec![
        IdentityReplayDecision::Miss,
        IdentityReplayDecision::Replay(IdentityReplayReceipt::new("created", None)),
    ]);
    port.write_error
        .set(Some(IdentityGovernancePortErrorClass::Conflict));
    let outcome = block_on(execute_invitation_create(
        &actor()?,
        MembershipRole::TenantOwner,
        &port,
        ExecuteInvitationCreateCommand::new(
            InvitationId::parse("invitation_01JIDENTITYGOV")?,
            "contact-hmac",
            UnixMillis::new(90),
            AggregateVersion::INITIAL,
            evidence()?,
        ),
    ))?;
    assert!(outcome.replayed());
    assert_eq!(port.replay_calls.get(), 2);
    assert_eq!(port.invitation_calls.get(), 1);
    Ok(())
}

#[test]
fn non_conflict_failure_never_rechecks_replay() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(vec![
        IdentityReplayDecision::Miss,
        IdentityReplayDecision::Replay(IdentityReplayReceipt::new("created", None)),
    ]);
    port.write_error
        .set(Some(IdentityGovernancePortErrorClass::VersionConflict));
    let result = block_on(execute_invitation_create(
        &actor()?,
        MembershipRole::TenantOwner,
        &port,
        ExecuteInvitationCreateCommand::new(
            InvitationId::parse("invitation_01JIDENTITYGOV")?,
            "contact-hmac",
            UnixMillis::new(90),
            AggregateVersion::INITIAL,
            evidence()?,
        ),
    ));
    assert_eq!(
        result,
        Err(IdentityGovernanceOperationError::VersionConflict)
    );
    assert_eq!(port.replay_calls.get(), 1);
    assert_eq!(port.invitation_calls.get(), 1);
    Ok(())
}
