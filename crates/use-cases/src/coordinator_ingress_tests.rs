use super::*;
use application_ports::ClockPort;
use application_ports::coordinator_ingress::{
    CoordinatorIngressApplicationPort, CoordinatorIngressPortError, CoordinatorProfileAccess,
    CoordinatorProjectionSnapshot, CoordinatorRuntimeOutcome, CoordinatorRuntimeResult,
};
use profile_platform_primitives::{
    ActorId, CorrelationId, OutboxEventId, TenantId, TenantScope,
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

struct FakeClock {
    now: UnixMillis,
}

impl ClockPort for FakeClock {
    fn now(&self) -> UnixMillis {
        self.now
    }
}

struct FakePort {
    profile: RefCell<Option<CoordinatorProfileAccess>>,
    snapshot_calls: Cell<u32>,
    execute_calls: Cell<u32>,
    project_calls: Cell<u32>,
    fencing_calls: Cell<u32>,
    outbox_calls: Cell<u32>,
    runtime_error: Cell<Option<application_ports::coordinator_ingress::CoordinatorIngressPortErrorClass>>,
}

impl FakePort {
    fn new(profile: Option<CoordinatorProfileAccess>) -> Self {
        Self {
            profile: RefCell::new(profile),
            snapshot_calls: Cell::new(0),
            execute_calls: Cell::new(0),
            project_calls: Cell::new(0),
            fencing_calls: Cell::new(0),
            outbox_calls: Cell::new(0),
            runtime_error: Cell::new(None),
        }
    }

    fn runtime_result(&self) -> Result<CoordinatorRuntimeResult, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JCOORDINGRESS")?;
        let profile_id = ProfileId::parse("profile_01JCOORDINGRESS")?;
        Ok(CoordinatorRuntimeResult::new(
            CoordinatorRuntimeOutcome::Snapshot,
            AggregateVersion::INITIAL,
            0,
            true,
            None,
            None,
            CoordinatorProjectionSnapshot::new(
                tenant_id,
                profile_id,
                "idle",
                AggregateVersion::INITIAL,
                0,
                0,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        ))
    }

    fn maybe_runtime_error(&self) -> Result<(), CoordinatorIngressPortError> {
        match self.runtime_error.get() {
            Some(class) => Err(CoordinatorIngressPortError::new(class)),
            None => Ok(()),
        }
    }
}

impl CoordinatorIngressApplicationPort for FakePort {
    async fn find_visible_profile(
        &self,
        _actor: &ActorContext,
        _role: MembershipRole,
        _profile_id: &ProfileId,
    ) -> Result<Option<CoordinatorProfileAccess>, CoordinatorIngressPortError> {
        Ok(self.profile.borrow().clone())
    }

    fn new_fencing_token(&self) -> Result<FencingToken, CoordinatorIngressPortError> {
        self.fencing_calls.set(self.fencing_calls.get() + 1);
        FencingToken::parse("fence_01JCOORDINGRESS")
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))
    }

    fn new_outbox_event_id(&self) -> Result<OutboxEventId, CoordinatorIngressPortError> {
        self.outbox_calls.set(self.outbox_calls.get() + 1);
        OutboxEventId::parse("outbox_01JCOORDINGRESS")
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))
    }

    async fn snapshot(
        &self,
        _scope: &TenantScope,
        _profile_id: &ProfileId,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError> {
        self.snapshot_calls.set(self.snapshot_calls.get() + 1);
        self.maybe_runtime_error()?;
        self.runtime_result()
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))
    }

    async fn execute(
        &self,
        _scope: &TenantScope,
        _profile_id: &ProfileId,
        _envelope: &CoordinatorCommandEnvelope,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError> {
        self.execute_calls.set(self.execute_calls.get() + 1);
        self.maybe_runtime_error()?;
        self.runtime_result()
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))
    }

    async fn project(
        &self,
        _scope: &TenantScope,
        _profile_id: &ProfileId,
        _result: &CoordinatorRuntimeResult,
        _outbox_event_id: &OutboxEventId,
        _projected_at: UnixMillis,
    ) -> Result<(), CoordinatorIngressPortError> {
        self.project_calls.set(self.project_calls.get() + 1);
        Ok(())
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_01JCOORDINGRESS")?),
        ActorId::parse("actor_01JCOORDINGRESS")?,
        CorrelationId::parse("corr_01JCOORDINGRESS")?,
    ))
}

fn profile_id() -> Result<ProfileId, Box<dyn std::error::Error>> {
    Ok(ProfileId::parse("profile_01JCOORDINGRESS")?)
}

fn clock() -> FakeClock {
    FakeClock {
        now: UnixMillis::new(10_000),
    }
}

fn envelope_input(command: CoordinatorCommandInput) -> Result<ExecuteCoordinatorCommand, Box<dyn std::error::Error>> {
    Ok(ExecuteCoordinatorCommand::new(
        IdempotencyKey::parse("idem_01JCOORDINGRESS")?,
        1,
        AggregateVersion::INITIAL,
        command,
    ))
}

#[test]
fn missing_or_non_live_profile_stops_before_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let missing = FakePort::new(None);
    assert_eq!(
        block_on(execute_coordinator_ingress(
            &actor()?,
            MembershipRole::Member,
            &profile_id()?,
            &missing,
            &clock(),
            CoordinatorIngressRequest::Snapshot,
        )),
        Err(CoordinatorIngressOperationError::NotFound)
    );
    assert_eq!(missing.snapshot_calls.get(), 0);

    let draft = FakePort::new(Some(CoordinatorProfileAccess::new("DRAFT", true)));
    assert_eq!(
        block_on(execute_coordinator_ingress(
            &actor()?,
            MembershipRole::Member,
            &profile_id()?,
            &draft,
            &clock(),
            CoordinatorIngressRequest::Snapshot,
        )),
        Err(CoordinatorIngressOperationError::Conflict)
    );
    assert_eq!(draft.snapshot_calls.get(), 0);
    Ok(())
}

#[test]
fn snapshot_runtime_result_is_projected_after_success() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(Some(CoordinatorProfileAccess::new("READY", true)));
    block_on(execute_coordinator_ingress(
        &actor()?,
        MembershipRole::Member,
        &profile_id()?,
        &port,
        &clock(),
        CoordinatorIngressRequest::Snapshot,
    ))?;
    assert_eq!(port.snapshot_calls.get(), 1);
    assert_eq!(port.outbox_calls.get(), 1);
    assert_eq!(port.project_calls.get(), 1);
    Ok(())
}

#[test]
fn owner_only_recovery_stops_non_owner_before_execute() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(Some(CoordinatorProfileAccess::new("DIRTY_LOCAL", true)));
    assert_eq!(
        block_on(execute_coordinator_ingress(
            &actor()?,
            MembershipRole::Member,
            &profile_id()?,
            &port,
            &clock(),
            CoordinatorIngressRequest::Command(envelope_input(CoordinatorCommandInput::MarkRecovered)?),
        )),
        Err(CoordinatorIngressOperationError::NotFound)
    );
    assert_eq!(port.execute_calls.get(), 0);
    assert_eq!(port.outbox_calls.get(), 0);
    assert_eq!(port.project_calls.get(), 0);
    Ok(())
}

#[test]
fn invalid_launch_ttl_stops_before_execute_and_projection() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(Some(CoordinatorProfileAccess::new("READY", true)));
    let request = CoordinatorIngressRequest::Command(envelope_input(
        CoordinatorCommandInput::IssueLaunchIntent {
            launch_intent_id: LaunchIntentId::parse("launch_01JCOORDINGRESS")?,
            device_id: DeviceId::parse("device_01JCOORDINGRESS")?,
            expires_in_ms: 999,
        },
    )?);
    assert_eq!(
        block_on(execute_coordinator_ingress(
            &actor()?,
            MembershipRole::TenantOwner,
            &profile_id()?,
            &port,
            &clock(),
            request,
        )),
        Err(CoordinatorIngressOperationError::InvalidRequest)
    );
    assert_eq!(port.execute_calls.get(), 0);
    assert_eq!(port.project_calls.get(), 0);
    Ok(())
}

#[test]
fn claim_generates_fencing_token_once_and_projects() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(Some(CoordinatorProfileAccess::new("READY", true)));
    let request = CoordinatorIngressRequest::Command(envelope_input(CoordinatorCommandInput::Claim {
        launch_intent_id: LaunchIntentId::parse("launch_01JCOORDINGRESS")?,
        device_id: DeviceId::parse("device_01JCOORDINGRESS")?,
        session_id: SessionId::parse("session_01JCOORDINGRESS")?,
    })?);
    block_on(execute_coordinator_ingress(
        &actor()?,
        MembershipRole::Member,
        &profile_id()?,
        &port,
        &clock(),
        request,
    ))?;
    assert_eq!(port.fencing_calls.get(), 1);
    assert_eq!(port.execute_calls.get(), 1);
    assert_eq!(port.project_calls.get(), 1);
    Ok(())
}

#[test]
fn runtime_failure_never_projects() -> Result<(), Box<dyn std::error::Error>> {
    let port = FakePort::new(Some(CoordinatorProfileAccess::new("READY", true)));
    port.runtime_error
        .set(Some(CoordinatorIngressPortErrorClass::Conflict));
    assert_eq!(
        block_on(execute_coordinator_ingress(
            &actor()?,
            MembershipRole::Member,
            &profile_id()?,
            &port,
            &clock(),
            CoordinatorIngressRequest::Snapshot,
        )),
        Err(CoordinatorIngressOperationError::Conflict)
    );
    assert_eq!(port.project_calls.get(), 0);
    Ok(())
}
