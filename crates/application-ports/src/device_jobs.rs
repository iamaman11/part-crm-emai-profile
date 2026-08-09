use core::{fmt, future::Future};
use device_domain::{DeviceJob, DeviceJobId, DeviceJobTarget};
use profile_platform_primitives::{ActorContext, AggregateVersion, DeviceId, TenantId, UnixMillis};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobCapability {
    Issue,
    Claim,
    Heartbeat,
    Complete,
    Recover,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceExecutionBlocker {
    DeviceUnauthorized,
    GenerationInactive,
    CertificationIncomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceExecutionReadiness {
    Ready,
    Blocked(DeviceExecutionBlocker),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobInsertOutcome {
    Inserted,
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobWriteOutcome {
    Applied,
    VersionConflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobPortErrorClass {
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceJobPortError {
    class: DeviceJobPortErrorClass,
}

impl DeviceJobPortError {
    #[must_use]
    pub const fn new(class: DeviceJobPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> DeviceJobPortErrorClass {
        self.class
    }
}

impl fmt::Display for DeviceJobPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            DeviceJobPortErrorClass::IntegrityFailure => "device job port integrity failure",
            DeviceJobPortErrorClass::DependencyUnavailable => "device job dependency unavailable",
        })
    }
}

impl std::error::Error for DeviceJobPortError {}

pub trait AuthenticatedDevicePort {
    fn authenticated_device_id(
        &self,
        actor: &ActorContext,
    ) -> impl Future<Output = Result<DeviceId, DeviceJobPortError>>;
}

pub trait DeviceJobAuthorizationPort {
    fn is_device_job_authorized(
        &self,
        actor: &ActorContext,
        target: &DeviceJobTarget,
        capability: DeviceJobCapability,
    ) -> impl Future<Output = Result<bool, DeviceJobPortError>>;
}

pub trait DeviceExecutionPreconditionPort {
    fn evaluate_device_execution(
        &self,
        actor: &ActorContext,
        target: &DeviceJobTarget,
    ) -> impl Future<Output = Result<DeviceExecutionReadiness, DeviceJobPortError>>;
}

pub trait DeviceJobQueryPort {
    fn list_claimable_device_jobs(
        &self,
        actor: &ActorContext,
        device_id: &DeviceId,
        now: UnixMillis,
        limit: u16,
    ) -> impl Future<Output = Result<Vec<DeviceJob>, DeviceJobPortError>>;
}

pub trait DeviceJobRepositoryPort {
    fn insert_device_job(
        &self,
        tenant_id: &TenantId,
        job: &DeviceJob,
    ) -> impl Future<Output = Result<DeviceJobInsertOutcome, DeviceJobPortError>>;

    fn load_device_job(
        &self,
        tenant_id: &TenantId,
        job_id: &DeviceJobId,
    ) -> impl Future<Output = Result<Option<DeviceJob>, DeviceJobPortError>>;

    fn compare_and_swap_device_job(
        &self,
        tenant_id: &TenantId,
        expected_version: AggregateVersion,
        job: &DeviceJob,
    ) -> impl Future<Output = Result<DeviceJobWriteOutcome, DeviceJobPortError>>;
}
