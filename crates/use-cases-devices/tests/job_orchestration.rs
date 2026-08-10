use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceExecutionBlocker, DeviceExecutionPreconditionPort,
    DeviceExecutionReadiness, DeviceJobAuthorizationPort, DeviceJobCapability,
    DeviceJobInsertOutcome, DeviceJobPortError, DeviceJobRepositoryPort, DeviceJobWriteOutcome,
};
use device_domain::{DeviceClaimId, DeviceJob, DeviceJobId, DeviceJobTarget};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, CorrelationId, DeviceId, GenerationId, ProfileId,
    TenantId, TenantScope, UnixMillis,
};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::task::{Context, Poll, Waker};
use use_cases_devices::{
    ApplyDeviceJobOutcomeCommand, ClaimDeviceJobCommand, DeviceJobOperationError, DeviceJobOutcome,
    IssueDeviceJobCommand, execute_apply_device_job_outcome, execute_claim_device_job,
    execute_issue_device_job,
};

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

struct FakeAuthenticatedDevice {
    device_id: DeviceId,
    calls: Cell<u32>,
}

impl AuthenticatedDevicePort for FakeAuthenticatedDevice {
    async fn authenticated_device_id(
        &self,
        _actor: &ActorContext,
    ) -> Result<DeviceId, DeviceJobPortError> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.device_id.clone())
    }
}

struct FakeAuthorization {
    allowed: bool,
    calls: Cell<u32>,
}

impl DeviceJobAuthorizationPort for FakeAuthorization {
    async fn is_device_job_authorized(
        &self,
        _actor: &ActorContext,
        _target: &DeviceJobTarget,
        _capability: DeviceJobCapability,
    ) -> Result<bool, DeviceJobPortError> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.allowed)
    }
}

struct FakePreconditions {
    readiness: DeviceExecutionReadiness,
    calls: Cell<u32>,
}

impl DeviceExecutionPreconditionPort for FakePreconditions {
    async fn evaluate_device_execution(
        &self,
        _actor: &ActorContext,
        _target: &DeviceJobTarget,
    ) -> Result<DeviceExecutionReadiness, DeviceJobPortError> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.readiness)
    }
}

struct FakeRepository {
    job: RefCell<Option<DeviceJob>>,
    inserts: Cell<u32>,
    loads: Cell<u32>,
    writes: Cell<u32>,
}

impl FakeRepository {
    fn empty() -> Self {
        Self {
            job: RefCell::new(None),
            inserts: Cell::new(0),
            loads: Cell::new(0),
            writes: Cell::new(0),
        }
    }

    fn with_job(job: DeviceJob) -> Self {
        Self {
            job: RefCell::new(Some(job)),
            inserts: Cell::new(0),
            loads: Cell::new(0),
            writes: Cell::new(0),
        }
    }
}

impl DeviceJobRepositoryPort for FakeRepository {
    async fn insert_device_job(
        &self,
        _tenant_id: &TenantId,
        job: &DeviceJob,
    ) -> Result<DeviceJobInsertOutcome, DeviceJobPortError> {
        self.inserts.set(self.inserts.get() + 1);
        let mut slot = self.job.borrow_mut();
        if slot.is_some() {
            Ok(DeviceJobInsertOutcome::Conflict)
        } else {
            *slot = Some(job.clone());
            Ok(DeviceJobInsertOutcome::Inserted)
        }
    }

    async fn load_device_job(
        &self,
        _tenant_id: &TenantId,
        job_id: &DeviceJobId,
    ) -> Result<Option<DeviceJob>, DeviceJobPortError> {
        self.loads.set(self.loads.get() + 1);
        Ok(self
            .job
            .borrow()
            .as_ref()
            .filter(|job| job.job_id() == job_id)
            .cloned())
    }

    async fn compare_and_swap_device_job(
        &self,
        _tenant_id: &TenantId,
        expected_version: AggregateVersion,
        job: &DeviceJob,
    ) -> Result<DeviceJobWriteOutcome, DeviceJobPortError> {
        self.writes.set(self.writes.get() + 1);
        let mut slot = self.job.borrow_mut();
        let Some(current) = slot.as_ref() else {
            return Ok(DeviceJobWriteOutcome::VersionConflict);
        };
        if current.version() != expected_version {
            return Ok(DeviceJobWriteOutcome::VersionConflict);
        }
        *slot = Some(job.clone());
        Ok(DeviceJobWriteOutcome::Applied)
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_01JDEVICE")?),
        ActorId::parse("actor_01JDEVICE")?,
        CorrelationId::parse("corr_01JDEVICE")?,
    ))
}

fn target() -> Result<DeviceJobTarget, Box<dyn std::error::Error>> {
    Ok(DeviceJobTarget::new(
        TenantId::parse("tenant_01JDEVICE")?,
        DeviceId::parse("device_01JDEVICE")?,
        ProfileId::parse("profile_01JDEVICE")?,
        GenerationId::parse("generation_01JDEVICE")?,
    ))
}

fn authenticated_device() -> Result<FakeAuthenticatedDevice, Box<dyn std::error::Error>> {
    Ok(FakeAuthenticatedDevice {
        device_id: DeviceId::parse("device_01JDEVICE")?,
        calls: Cell::new(0),
    })
}

fn initial_job() -> Result<DeviceJob, Box<dyn std::error::Error>> {
    Ok(DeviceJob::issue(
        DeviceJobId::parse("devjob_01JDEVICE")?,
        target()?,
        3,
        UnixMillis::new(100),
    )?)
}

#[test]
fn unauthorized_issue_never_touches_repository() -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let authorization = FakeAuthorization {
        allowed: false,
        calls: Cell::new(0),
    };
    let repository = FakeRepository::empty();
    let result = block_on(execute_issue_device_job(
        &actor,
        &authorization,
        &repository,
        IssueDeviceJobCommand::new(
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            3,
            UnixMillis::new(100),
        ),
    ));
    assert_eq!(result, Err(DeviceJobOperationError::Forbidden));
    assert_eq!(authorization.calls.get(), 1);
    assert_eq!(repository.inserts.get(), 0);
    assert_eq!(repository.loads.get(), 0);
    Ok(())
}

#[test]
fn foreign_authenticated_device_cannot_claim_target() -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let device_identity = FakeAuthenticatedDevice {
        device_id: DeviceId::parse("device_02JDEVICE")?,
        calls: Cell::new(0),
    };
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let preconditions = FakePreconditions {
        readiness: DeviceExecutionReadiness::Ready,
        calls: Cell::new(0),
    };
    let repository = FakeRepository::with_job(initial_job()?);
    let result = block_on(execute_claim_device_job(
        &actor,
        &device_identity,
        &authorization,
        &preconditions,
        &repository,
        ClaimDeviceJobCommand::new(
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            AggregateVersion::INITIAL,
            DeviceClaimId::parse("devclaim_01JDEVICE")?,
            UnixMillis::new(110),
            UnixMillis::new(200),
        ),
    ));
    assert_eq!(result, Err(DeviceJobOperationError::Forbidden));
    assert_eq!(device_identity.calls.get(), 1);
    assert_eq!(authorization.calls.get(), 0);
    assert_eq!(preconditions.calls.get(), 0);
    assert_eq!(repository.loads.get(), 0);
    assert_eq!(repository.writes.get(), 0);
    Ok(())
}

#[test]
fn blocked_claim_never_loads_or_mutates_job() -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let device_identity = authenticated_device()?;
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let preconditions = FakePreconditions {
        readiness: DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::GenerationInactive),
        calls: Cell::new(0),
    };
    let repository = FakeRepository::with_job(initial_job()?);
    let result = block_on(execute_claim_device_job(
        &actor,
        &device_identity,
        &authorization,
        &preconditions,
        &repository,
        ClaimDeviceJobCommand::new(
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            AggregateVersion::INITIAL,
            DeviceClaimId::parse("devclaim_01JDEVICE")?,
            UnixMillis::new(110),
            UnixMillis::new(200),
        ),
    ));
    assert_eq!(
        result,
        Err(DeviceJobOperationError::PreconditionFailed(
            DeviceExecutionBlocker::GenerationInactive
        ))
    );
    assert_eq!(device_identity.calls.get(), 1);
    assert_eq!(authorization.calls.get(), 1);
    assert_eq!(preconditions.calls.get(), 1);
    assert_eq!(repository.loads.get(), 0);
    assert_eq!(repository.writes.get(), 0);
    Ok(())
}

#[test]
fn successful_claim_is_cas_persisted_after_checks() -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let device_identity = authenticated_device()?;
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let preconditions = FakePreconditions {
        readiness: DeviceExecutionReadiness::Ready,
        calls: Cell::new(0),
    };
    let repository = FakeRepository::with_job(initial_job()?);
    let job = block_on(execute_claim_device_job(
        &actor,
        &device_identity,
        &authorization,
        &preconditions,
        &repository,
        ClaimDeviceJobCommand::new(
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            AggregateVersion::INITIAL,
            DeviceClaimId::parse("devclaim_01JDEVICE")?,
            UnixMillis::new(110),
            UnixMillis::new(200),
        ),
    ))?;
    assert_eq!(job.last_fence(), 1);
    assert_eq!(device_identity.calls.get(), 1);
    assert_eq!(repository.loads.get(), 1);
    assert_eq!(repository.writes.get(), 1);
    Ok(())
}

#[test]
fn completion_rechecks_preconditions_before_accepting_result()
-> Result<(), Box<dyn std::error::Error>> {
    let actor = actor()?;
    let device_identity = authenticated_device()?;
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let ready = FakePreconditions {
        readiness: DeviceExecutionReadiness::Ready,
        calls: Cell::new(0),
    };
    let repository = FakeRepository::with_job(initial_job()?);
    let running = block_on(execute_claim_device_job(
        &actor,
        &device_identity,
        &authorization,
        &ready,
        &repository,
        ClaimDeviceJobCommand::new(
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            AggregateVersion::INITIAL,
            DeviceClaimId::parse("devclaim_01JDEVICE")?,
            UnixMillis::new(110),
            UnixMillis::new(200),
        ),
    ))?;
    let blocked = FakePreconditions {
        readiness: DeviceExecutionReadiness::Blocked(
            DeviceExecutionBlocker::CertificationIncomplete,
        ),
        calls: Cell::new(0),
    };
    let writes_before = repository.writes.get();
    let result = block_on(execute_apply_device_job_outcome(
        &actor,
        &device_identity,
        &authorization,
        &blocked,
        &repository,
        ApplyDeviceJobOutcomeCommand::new(
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            running.version(),
            DeviceClaimId::parse("devclaim_01JDEVICE")?,
            running.last_fence(),
            UnixMillis::new(120),
            DeviceJobOutcome::Succeeded,
        ),
    ));
    assert_eq!(
        result,
        Err(DeviceJobOperationError::PreconditionFailed(
            DeviceExecutionBlocker::CertificationIncomplete
        ))
    );
    assert_eq!(repository.writes.get(), writes_before);
    Ok(())
}
