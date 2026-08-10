use application_ports::device_generation_commit::{
    CoordinatorGenerationCommitWitness, DeviceGenerationCommitError, DeviceGenerationCommitOutcome,
    DeviceGenerationCommitPort, DeviceGenerationCommitRequest,
};
use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceExecutionPreconditionPort, DeviceExecutionReadiness,
    DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobInsertOutcome, DeviceJobPortError,
    DeviceJobRepositoryPort, DeviceJobWriteOutcome,
};
use application_ports::generation_objects::{
    GenerationObjectDescriptor, GenerationObjectDescriptorVerifyPort,
};
use application_ports::generations::GenerationPortError;
use device_domain::{DeviceJob, DeviceJobId, DeviceJobTarget};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, CorrelationId, DeviceId, FencingToken, GenerationId,
    ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use use_cases_devices::{
    DeviceGenerationCommitOperationError, DeviceGenerationCommitServices,
    execute_commit_dirty_generation,
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

#[derive(Clone)]
struct Identity {
    device_id: DeviceId,
}

impl AuthenticatedDevicePort for Identity {
    async fn authenticated_device_id(
        &self,
        _actor: &ActorContext,
    ) -> Result<DeviceId, DeviceJobPortError> {
        Ok(self.device_id.clone())
    }
}

struct Authorization;

impl DeviceJobAuthorizationPort for Authorization {
    async fn is_device_job_authorized(
        &self,
        _actor: &ActorContext,
        _target: &DeviceJobTarget,
        capability: DeviceJobCapability,
    ) -> Result<bool, DeviceJobPortError> {
        Ok(capability == DeviceJobCapability::Complete)
    }
}

struct Preconditions;

impl DeviceExecutionPreconditionPort for Preconditions {
    async fn evaluate_device_execution(
        &self,
        _actor: &ActorContext,
        _target: &DeviceJobTarget,
    ) -> Result<DeviceExecutionReadiness, DeviceJobPortError> {
        Ok(DeviceExecutionReadiness::Ready)
    }
}

#[derive(Clone)]
struct Repository {
    job: DeviceJob,
}

impl DeviceJobRepositoryPort for Repository {
    async fn insert_device_job(
        &self,
        _tenant_id: &TenantId,
        _job: &DeviceJob,
    ) -> Result<DeviceJobInsertOutcome, DeviceJobPortError> {
        unreachable!("generation commit never inserts device jobs")
    }

    async fn load_device_job(
        &self,
        _tenant_id: &TenantId,
        job_id: &DeviceJobId,
    ) -> Result<Option<DeviceJob>, DeviceJobPortError> {
        Ok((self.job.job_id() == job_id).then(|| self.job.clone()))
    }

    async fn compare_and_swap_device_job(
        &self,
        _tenant_id: &TenantId,
        _expected_version: AggregateVersion,
        _job: &DeviceJob,
    ) -> Result<DeviceJobWriteOutcome, DeviceJobPortError> {
        unreachable!("generation commit does not mutate the job through the query repository")
    }
}

struct Evidence {
    verifier_calls: Rc<Cell<u32>>,
    verifier_observed: Rc<RefCell<Option<GenerationObjectDescriptor>>>,
    commit_calls: Rc<Cell<u32>>,
    commit_observed: Rc<RefCell<Option<DeviceGenerationCommitRequest>>>,
}

impl Evidence {
    fn new() -> Self {
        Self {
            verifier_calls: Rc::new(Cell::new(0)),
            verifier_observed: Rc::new(RefCell::new(None)),
            commit_calls: Rc::new(Cell::new(0)),
            commit_observed: Rc::new(RefCell::new(None)),
        }
    }
}

struct Verifier {
    result: bool,
    calls: Rc<Cell<u32>>,
    observed: Rc<RefCell<Option<GenerationObjectDescriptor>>>,
}

impl GenerationObjectDescriptorVerifyPort for Verifier {
    async fn verify_generation_object_descriptor_exact(
        &self,
        _scope: &TenantScope,
        descriptor: &GenerationObjectDescriptor,
    ) -> Result<bool, GenerationPortError> {
        self.calls.set(self.calls.get() + 1);
        self.observed.replace(Some(descriptor.clone()));
        Ok(self.result)
    }
}

struct Commit {
    calls: Rc<Cell<u32>>,
    observed: Rc<RefCell<Option<DeviceGenerationCommitRequest>>>,
}

impl DeviceGenerationCommitPort for Commit {
    async fn commit_device_generation(
        &self,
        _actor: &ActorContext,
        request: &DeviceGenerationCommitRequest,
    ) -> Result<DeviceGenerationCommitOutcome, DeviceGenerationCommitError> {
        self.calls.set(self.calls.get() + 1);
        self.observed.replace(Some(request.clone()));
        Ok(DeviceGenerationCommitOutcome::Activated)
    }
}

struct Fixture {
    actor: ActorContext,
    device_id: DeviceId,
    job: DeviceJob,
    request: DeviceGenerationCommitRequest,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_commit_device_01")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            ActorId::parse("actor_commit_device_01")?,
            CorrelationId::parse("corr_commit_device_01")?,
        );
        let device_id = DeviceId::parse("device_commit_device_01")?;
        let profile_id = ProfileId::parse("profile_commit_device_01")?;
        let base_generation_id = GenerationId::parse("generation_commit_base_01")?;
        let candidate_generation_id = GenerationId::parse("generation_commit_candidate_01")?;
        let target = DeviceJobTarget::new(
            tenant_id,
            device_id.clone(),
            profile_id.clone(),
            base_generation_id.clone(),
        );
        let job_id = DeviceJobId::parse("devjob_commit_device_01")?;
        let claim_id = device_domain::DeviceClaimId::parse("devclaim_commit_device_01")?;
        let mut job = DeviceJob::issue(job_id.clone(), target, 3, UnixMillis::new(10))?;
        let claim = job.claim(claim_id.clone(), UnixMillis::new(20), UnixMillis::new(100))?;
        let object_key = format!(
            "tenants/{}/profiles/{}/generations/{}.bpgc",
            actor.tenant_scope().tenant_id().as_str(),
            profile_id.as_str(),
            candidate_generation_id.as_str()
        );
        let descriptor = GenerationObjectDescriptor::new(
            profile_id.clone(),
            candidate_generation_id,
            object_key,
            "a".repeat(64),
            "b".repeat(64),
            4096,
        );
        let request = DeviceGenerationCommitRequest::new(
            job_id,
            claim_id,
            job.version(),
            claim.fence(),
            device_id.clone(),
            profile_id,
            base_generation_id,
            descriptor,
            AggregateVersion::new(7)?,
            CoordinatorGenerationCommitWitness::new(
                SessionId::parse("session_commit_device_01")?,
                FencingToken::parse("fencing_commit_device_01")?,
                5,
                11,
                17,
            ),
            UnixMillis::new(30),
        );
        Ok(Self {
            actor,
            device_id,
            job,
            request,
        })
    }

    fn request_with_fence(&self, claim_fence: u64) -> DeviceGenerationCommitRequest {
        DeviceGenerationCommitRequest::new(
            self.request.job_id().clone(),
            self.request.claim_id().clone(),
            self.request.expected_job_version(),
            claim_fence,
            self.request.device_id().clone(),
            self.request.profile_id().clone(),
            self.request.base_generation_id().clone(),
            self.request.object().clone(),
            self.request.expected_profile_version(),
            self.request.coordinator().clone(),
            self.request.observed_at(),
        )
    }
}

fn execute(
    fixture: &Fixture,
    identity: &Identity,
    verifier_result: bool,
    evidence: &Evidence,
    request: &DeviceGenerationCommitRequest,
) -> Result<DeviceGenerationCommitOutcome, DeviceGenerationCommitOperationError> {
    let repository = Repository {
        job: fixture.job.clone(),
    };
    let verifier = Verifier {
        result: verifier_result,
        calls: Rc::clone(&evidence.verifier_calls),
        observed: Rc::clone(&evidence.verifier_observed),
    };
    let commit = Commit {
        calls: Rc::clone(&evidence.commit_calls),
        observed: Rc::clone(&evidence.commit_observed),
    };
    let services = DeviceGenerationCommitServices::new(
        identity,
        &Authorization,
        &Preconditions,
        &repository,
        &verifier,
        &commit,
    );
    block_on(execute_commit_dirty_generation(
        &fixture.actor,
        &services,
        request,
    ))
}

#[test]
fn exact_verified_request_reaches_commit_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let evidence = Evidence::new();

    let outcome = execute(
        &fixture,
        &Identity {
            device_id: fixture.device_id.clone(),
        },
        true,
        &evidence,
        &fixture.request,
    )?;

    assert_eq!(outcome, DeviceGenerationCommitOutcome::Activated);
    assert_eq!(evidence.verifier_calls.get(), 1);
    assert_eq!(evidence.commit_calls.get(), 1);
    assert_eq!(
        evidence.verifier_observed.borrow().as_ref(),
        Some(fixture.request.object())
    );
    assert_eq!(
        evidence.commit_observed.borrow().as_ref(),
        Some(&fixture.request)
    );
    Ok(())
}

#[test]
fn stale_claim_fence_stops_before_object_verification_or_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let evidence = Evidence::new();
    let request = fixture.request_with_fence(fixture.job.last_fence() + 1);

    let result = execute(
        &fixture,
        &Identity {
            device_id: fixture.device_id.clone(),
        },
        true,
        &evidence,
        &request,
    );

    assert_eq!(
        result,
        Err(DeviceGenerationCommitOperationError::StaleClaim)
    );
    assert_eq!(evidence.verifier_calls.get(), 0);
    assert_eq!(evidence.commit_calls.get(), 0);
    Ok(())
}

#[test]
fn mismatched_authenticated_device_stops_before_object_verification_or_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let evidence = Evidence::new();

    let result = execute(
        &fixture,
        &Identity {
            device_id: DeviceId::parse("device_other_device_01")?,
        },
        true,
        &evidence,
        &fixture.request,
    );

    assert_eq!(result, Err(DeviceGenerationCommitOperationError::Forbidden));
    assert_eq!(evidence.verifier_calls.get(), 0);
    assert_eq!(evidence.commit_calls.get(), 0);
    Ok(())
}

#[test]
fn failed_exact_object_verification_never_calls_catalog_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let evidence = Evidence::new();

    let result = execute(
        &fixture,
        &Identity {
            device_id: fixture.device_id.clone(),
        },
        false,
        &evidence,
        &fixture.request,
    );

    assert_eq!(
        result,
        Err(DeviceGenerationCommitOperationError::ObjectVerificationFailed)
    );
    assert_eq!(evidence.verifier_calls.get(), 1);
    assert_eq!(evidence.commit_calls.get(), 0);
    Ok(())
}
