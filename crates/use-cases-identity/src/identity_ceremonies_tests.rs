use super::*;
use application_ports::identity_ceremonies::{
    ActiveIdentityBinding, BootstrapOwnerWrite, IdentityCeremonyApplicationPort,
    InvitationAcceptWrite, TenantIdentityBoundary,
};
use application_ports::identity_governance::{
    IdentityGovernancePortError, IdentityGovernancePortErrorClass, IdentityReplayDecision,
    IdentityReplayReceipt,
};
use profile_platform_primitives::{
    AuditEventId, IdempotencyKey, OutboxEventId, TenantId, UnixMillis,
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
    binding: RefCell<Option<ActiveIdentityBinding>>,
    boundary: Cell<TenantIdentityBoundary>,
    replay: RefCell<Vec<IdentityReplayDecision>>,
    replay_calls: Cell<u32>,
    bootstrap_calls: Cell<u32>,
    accept_calls: Cell<u32>,
    write_error: Cell<Option<IdentityGovernancePortErrorClass>>,
}

impl FakePort {
    fn new() -> Self {
        Self {
            binding: RefCell::new(None),
            boundary: Cell::new(TenantIdentityBoundary::new(0, 0)),
            replay: RefCell::new(Vec::new()),
            replay_calls: Cell::new(0),
            bootstrap_calls: Cell::new(0),
            accept_calls: Cell::new(0),
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

impl IdentityCeremonyApplicationPort for FakePort {
    async fn find_active_identity_binding(
        &self,
        _scope: &TenantScope,
        _identity: &VerifiedIdentitySnapshot,
        _correlation_id: &CorrelationId,
    ) -> Result<Option<ActiveIdentityBinding>, IdentityGovernancePortError> {
        Ok(self.binding.borrow().clone())
    }

    async fn tenant_identity_boundary(
        &self,
        _scope: &TenantScope,
    ) -> Result<TenantIdentityBoundary, IdentityGovernancePortError> {
        Ok(self.boundary.get())
    }

    async fn decide_ceremony_replay(
        &self,
        _scope: &TenantScope,
        _actor_id: &ActorId,
        _command_name: &str,
        _evidence: &CommandExecutionEvidence,
    ) -> Result<IdentityReplayDecision, IdentityGovernancePortError> {
        self.replay_calls.set(self.replay_calls.get() + 1);
        Ok(if self.replay.borrow().is_empty() {
            IdentityReplayDecision::Miss
        } else {
            self.replay.borrow_mut().remove(0)
        })
    }

    async fn bootstrap_owner(
        &self,
        _context: &VerifiedIdentityCeremonyContext,
        _write: &BootstrapOwnerWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.bootstrap_calls.set(self.bootstrap_calls.get() + 1);
        self.write_result()
    }

    async fn accept_invitation(
        &self,
        _context: &VerifiedIdentityCeremonyContext,
        _write: &InvitationAcceptWrite,
    ) -> Result<(), IdentityGovernancePortError> {
        self.accept_calls.set(self.accept_calls.get() + 1);
        self.write_result()
    }
}

fn scope() -> Result<TenantScope, Box<dyn std::error::Error>> {
    Ok(TenantScope::new(TenantId::parse("tenant_01JCEREMONY")?))
}

fn correlation_id() -> Result<CorrelationId, Box<dyn std::error::Error>> {
    Ok(CorrelationId::parse("corr_01JCEREMONY")?)
}

fn identity() -> VerifiedIdentitySnapshot {
    VerifiedIdentitySnapshot::new("subject-01JCEREMONY", Some("contact-hint".to_owned()))
}

fn actor_id() -> Result<ActorId, Box<dyn std::error::Error>> {
    Ok(ActorId::parse("actor_01JCEREMONY")?)
}

fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
    Ok(CommandExecutionEvidence::new(
        IdempotencyKey::parse("idem_01JCEREMONY")?,
        "request-digest-01JCEREMONY",
        AuditEventId::parse("audit_01JCEREMONY")?,
        OutboxEventId::parse("outbox_01JCEREMONY")?,
        UnixMillis::new(10),
        UnixMillis::new(100),
    ))
}

fn bootstrap_command() -> Result<ExecuteOwnerBootstrapCommand, Box<dyn std::error::Error>> {
    Ok(ExecuteOwnerBootstrapCommand::new(
        actor_id()?,
        IdentityId::parse("identity_01JCEREMONY")?,
        "Ceremony Tenant",
        evidence()?,
    ))
}

fn accept_command() -> Result<ExecuteInvitationAcceptCommand, Box<dyn std::error::Error>> {
    Ok(ExecuteInvitationAcceptCommand::new(
        actor_id()?,
        IdentityId::parse("identity_01JCEREMONY")?,
        InvitationId::parse("invitation_01JCEREMONY")?,
        evidence()?,
    ))
}

#[test]
fn bootstrap_existing_identity_actor_mismatch_fails_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new();
    *port.binding.borrow_mut() = Some(ActiveIdentityBinding::new(
        ActorId::parse("actor_01JOTHER")?,
        MembershipRole::TenantOwner,
    ));
    assert_eq!(
        block_on(execute_owner_bootstrap(
            scope()?,
            correlation_id()?,
            identity(),
            &port,
            bootstrap_command()?,
        )),
        Err(IdentityGovernanceOperationError::NotFound)
    );
    assert_eq!(port.replay_calls.get(), 0);
    assert_eq!(port.bootstrap_calls.get(), 0);
    Ok(())
}

#[test]
fn existing_owner_exact_bootstrap_replay_skips_write() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new();
    *port.binding.borrow_mut() = Some(ActiveIdentityBinding::new(
        actor_id()?,
        MembershipRole::TenantOwner,
    ));
    port.replay
        .borrow_mut()
        .push(IdentityReplayDecision::Replay(IdentityReplayReceipt::new(
            "bootstrapped",
            None,
        )));
    let outcome = block_on(execute_owner_bootstrap(
        scope()?,
        correlation_id()?,
        identity(),
        &port,
        bootstrap_command()?,
    ))?;
    assert!(outcome.replayed());
    assert_eq!(outcome.result_code(), "bootstrapped");
    assert_eq!(port.bootstrap_calls.get(), 0);
    Ok(())
}

#[test]
fn occupied_tenant_boundary_stops_bootstrap_before_write() -> Result<(), Box<dyn std::error::Error>>
{
    let port = FakePort::new();
    port.boundary.set(TenantIdentityBoundary::new(1, 1));
    assert_eq!(
        block_on(execute_owner_bootstrap(
            scope()?,
            correlation_id()?,
            identity(),
            &port,
            bootstrap_command()?,
        )),
        Err(IdentityGovernanceOperationError::Conflict)
    );
    assert_eq!(port.replay_calls.get(), 0);
    assert_eq!(port.bootstrap_calls.get(), 0);
    Ok(())
}

#[test]
fn invitation_accept_existing_actor_mismatch_fails_closed() -> Result<(), Box<dyn std::error::Error>>
{
    let port = FakePort::new();
    *port.binding.borrow_mut() = Some(ActiveIdentityBinding::new(
        ActorId::parse("actor_01JOTHER")?,
        MembershipRole::Member,
    ));
    assert_eq!(
        block_on(execute_invitation_accept(
            scope()?,
            correlation_id()?,
            identity(),
            &port,
            accept_command()?,
        )),
        Err(IdentityGovernanceOperationError::NotFound)
    );
    assert_eq!(port.accept_calls.get(), 0);
    Ok(())
}

#[test]
fn invitation_accept_conflict_rechecks_exact_replay() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new();
    port.write_error
        .set(Some(IdentityGovernancePortErrorClass::Conflict));
    port.replay
        .borrow_mut()
        .push(IdentityReplayDecision::Replay(IdentityReplayReceipt::new(
            "accepted", None,
        )));
    let outcome = block_on(execute_invitation_accept(
        scope()?,
        correlation_id()?,
        identity(),
        &port,
        accept_command()?,
    ))?;
    assert!(outcome.replayed());
    assert_eq!(port.accept_calls.get(), 1);
    assert_eq!(port.replay_calls.get(), 1);
    Ok(())
}

#[test]
fn fresh_invitation_accept_writes_once() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new();
    let outcome = block_on(execute_invitation_accept(
        scope()?,
        correlation_id()?,
        identity(),
        &port,
        accept_command()?,
    ))?;
    assert!(!outcome.replayed());
    assert_eq!(outcome.result_code(), "accepted");
    assert_eq!(outcome.aggregate_version(), AggregateVersion::INITIAL);
    assert_eq!(port.accept_calls.get(), 1);
    Ok(())
}
