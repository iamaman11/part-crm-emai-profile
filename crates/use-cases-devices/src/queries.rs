use application_ports::device_jobs::{
    AuthenticatedDevicePort, DeviceExecutionPreconditionPort, DeviceExecutionReadiness,
    DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobPortError, DeviceJobPortErrorClass,
    DeviceJobQueryPort,
};
use device_domain::DeviceJob;
use profile_platform_primitives::{ActorContext, UnixMillis};

pub const MAX_CLAIMABLE_DEVICE_JOBS: u16 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListClaimableDeviceJobsRequest {
    limit: u16,
    now: UnixMillis,
}

impl ListClaimableDeviceJobsRequest {
    pub fn new(limit: u16, now: UnixMillis) -> Result<Self, DeviceJobQueryError> {
        if limit == 0 || limit > MAX_CLAIMABLE_DEVICE_JOBS {
            return Err(DeviceJobQueryError::InvalidRequest);
        }
        Ok(Self { limit, now })
    }

    #[must_use]
    pub const fn limit(self) -> u16 {
        self.limit
    }

    #[must_use]
    pub const fn now(self) -> UnixMillis {
        self.now
    }
}

pub async fn execute_list_claimable_device_jobs<D, A, P, Q>(
    actor: &ActorContext,
    device_identity: &D,
    authorization: &A,
    preconditions: &P,
    query: &Q,
    request: ListClaimableDeviceJobsRequest,
) -> Result<Vec<DeviceJob>, DeviceJobQueryError>
where
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
    Q: DeviceJobQueryPort,
{
    let device_id = device_identity
        .authenticated_device_id(actor)
        .await
        .map_err(map_port_error)?;
    let candidates = query
        .list_claimable_device_jobs(actor, &device_id, request.now(), request.limit())
        .await
        .map_err(map_port_error)?;
    if candidates.len() > usize::from(request.limit()) {
        return Err(DeviceJobQueryError::IntegrityFailure);
    }

    let mut visible = Vec::with_capacity(candidates.len());
    for job in candidates {
        let target = job.target();
        if target.tenant_id() != actor.tenant_scope().tenant_id()
            || target.device_id() != &device_id
        {
            return Err(DeviceJobQueryError::IntegrityFailure);
        }
        let authorized = authorization
            .is_device_job_authorized(actor, target, DeviceJobCapability::Claim)
            .await
            .map_err(map_port_error)?;
        if !authorized {
            continue;
        }
        match preconditions
            .evaluate_device_execution(actor, target)
            .await
            .map_err(map_port_error)?
        {
            DeviceExecutionReadiness::Ready => visible.push(job),
            DeviceExecutionReadiness::Blocked(_) => {}
        }
    }
    Ok(visible)
}

fn map_port_error(error: DeviceJobPortError) -> DeviceJobQueryError {
    match error.class() {
        DeviceJobPortErrorClass::IntegrityFailure => DeviceJobQueryError::IntegrityFailure,
        DeviceJobPortErrorClass::DependencyUnavailable => {
            DeviceJobQueryError::DependencyUnavailable
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobQueryError {
    InvalidRequest,
    IntegrityFailure,
    DependencyUnavailable,
}

impl core::fmt::Display for DeviceJobQueryError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "claimable device-job query is outside the bounded contract",
            Self::IntegrityFailure => "claimable device-job query failed integrity validation",
            Self::DependencyUnavailable => "claimable device-job query dependency is unavailable",
        })
    }
}

impl std::error::Error for DeviceJobQueryError {}

#[cfg(test)]
mod tests {
    use super::{
        DeviceJobQueryError, ListClaimableDeviceJobsRequest, MAX_CLAIMABLE_DEVICE_JOBS,
        execute_list_claimable_device_jobs,
    };
    use application_ports::device_jobs::{
        AuthenticatedDevicePort, DeviceExecutionPreconditionPort, DeviceExecutionReadiness,
        DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobPortError, DeviceJobQueryPort,
    };
    use device_domain::{DeviceJob, DeviceJobId, DeviceJobTarget};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, DeviceId, GenerationId, ProfileId, TenantId,
        TenantScope, UnixMillis,
    };
    use std::cell::Cell;
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

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JQUERYDEVICE")?),
            ActorId::parse("actor_01JQUERYDEVICE")?,
            CorrelationId::parse("corr_01JQUERYDEVICE")?,
        ))
    }

    fn target(device_id: DeviceId) -> Result<DeviceJobTarget, Box<dyn std::error::Error>> {
        Ok(DeviceJobTarget::new(
            TenantId::parse("tenant_01JQUERYDEVICE")?,
            device_id,
            ProfileId::parse("profile_01JQUERYDEVICE")?,
            GenerationId::parse("generation_01JQUERYDEVICE")?,
        ))
    }

    fn job(device_id: DeviceId) -> Result<DeviceJob, Box<dyn std::error::Error>> {
        Ok(DeviceJob::issue(
            DeviceJobId::parse("devjob_01JQUERYDEVICE")?,
            target(device_id)?,
            3,
            UnixMillis::new(100),
        )?)
    }

    struct FixedDevice {
        device_id: DeviceId,
        calls: Cell<u32>,
    }

    impl AuthenticatedDevicePort for FixedDevice {
        async fn authenticated_device_id(
            &self,
            _actor: &ActorContext,
        ) -> Result<DeviceId, DeviceJobPortError> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.device_id.clone())
        }
    }

    struct FixedAuthorization {
        allowed: bool,
        calls: Cell<u32>,
    }

    impl DeviceJobAuthorizationPort for FixedAuthorization {
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

    struct FixedPreconditions {
        readiness: DeviceExecutionReadiness,
        calls: Cell<u32>,
    }

    impl DeviceExecutionPreconditionPort for FixedPreconditions {
        async fn evaluate_device_execution(
            &self,
            _actor: &ActorContext,
            _target: &DeviceJobTarget,
        ) -> Result<DeviceExecutionReadiness, DeviceJobPortError> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.readiness)
        }
    }

    struct FixedQuery {
        rows: Vec<DeviceJob>,
        calls: Cell<u32>,
    }

    impl DeviceJobQueryPort for FixedQuery {
        async fn list_claimable_device_jobs(
            &self,
            _actor: &ActorContext,
            _device_id: &DeviceId,
            _now: UnixMillis,
            _limit: u16,
        ) -> Result<Vec<DeviceJob>, DeviceJobPortError> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.rows.clone())
        }
    }

    #[test]
    fn query_bounds_are_strict() {
        assert_eq!(
            ListClaimableDeviceJobsRequest::new(0, UnixMillis::new(1)),
            Err(DeviceJobQueryError::InvalidRequest)
        );
        assert_eq!(
            ListClaimableDeviceJobsRequest::new(MAX_CLAIMABLE_DEVICE_JOBS + 1, UnixMillis::new(1)),
            Err(DeviceJobQueryError::InvalidRequest)
        );
    }

    #[test]
    fn authenticated_device_precedes_query_and_unauthorized_rows_are_not_projected()
    -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let device_id = DeviceId::parse("device_01JQUERYDEVICE")?;
        let device = FixedDevice {
            device_id: device_id.clone(),
            calls: Cell::new(0),
        };
        let authorization = FixedAuthorization {
            allowed: false,
            calls: Cell::new(0),
        };
        let preconditions = FixedPreconditions {
            readiness: DeviceExecutionReadiness::Ready,
            calls: Cell::new(0),
        };
        let query = FixedQuery {
            rows: vec![job(device_id)?],
            calls: Cell::new(0),
        };
        let result = block_on(execute_list_claimable_device_jobs(
            &actor,
            &device,
            &authorization,
            &preconditions,
            &query,
            ListClaimableDeviceJobsRequest::new(10, UnixMillis::new(150))?,
        ))?;
        assert!(result.is_empty());
        assert_eq!(device.calls.get(), 1);
        assert_eq!(query.calls.get(), 1);
        assert_eq!(authorization.calls.get(), 1);
        assert_eq!(preconditions.calls.get(), 0);
        Ok(())
    }

    #[test]
    fn foreign_device_row_is_integrity_failure_before_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let authenticated = DeviceId::parse("device_01JQUERYDEVICE")?;
        let device = FixedDevice {
            device_id: authenticated,
            calls: Cell::new(0),
        };
        let authorization = FixedAuthorization {
            allowed: true,
            calls: Cell::new(0),
        };
        let preconditions = FixedPreconditions {
            readiness: DeviceExecutionReadiness::Ready,
            calls: Cell::new(0),
        };
        let query = FixedQuery {
            rows: vec![job(DeviceId::parse("device_02JQUERYDEVICE")?)?],
            calls: Cell::new(0),
        };
        assert_eq!(
            block_on(execute_list_claimable_device_jobs(
                &actor,
                &device,
                &authorization,
                &preconditions,
                &query,
                ListClaimableDeviceJobsRequest::new(10, UnixMillis::new(150))?,
            )),
            Err(DeviceJobQueryError::IntegrityFailure)
        );
        assert_eq!(authorization.calls.get(), 0);
        assert_eq!(preconditions.calls.get(), 0);
        Ok(())
    }
}
