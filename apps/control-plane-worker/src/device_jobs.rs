use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::composition::{
    authenticated_device, device_execution_preconditions, device_job_authorization,
    device_job_repository,
};
use application_ports::{
    AuthenticatedDevicePort, DeviceJobPortError, DeviceJobPortErrorClass,
};
use control_plane_contract::RouteClass;
use device_domain::{
    DeviceClaimId, DeviceJob, DeviceJobError, DeviceJobId, DeviceJobStatus, DeviceJobTarget,
};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, GenerationId, ProfileId, UnixMillis,
};
use serde::{Deserialize, Serialize};
use use_cases_devices::{
    ApplyDeviceJobOutcomeCommand, ClaimDeviceJobCommand, DeviceJobOperationError, DeviceJobOutcome,
    DeviceJobQueryError, HeartbeatDeviceJobCommand, ListClaimableDeviceJobsRequest,
    execute_apply_device_job_outcome, execute_claim_device_job, execute_heartbeat_device_job,
    execute_list_claimable_device_jobs,
};
use worker::{Date, Env, Request, Response, Result};

const CLAIMABLE_PAGE_SIZE: u16 = 20;
const DEVICE_CLAIM_LEASE_MS: u64 = 30_000;
const MAX_DEVICE_RETRY_DELAY_MS: u64 = 300_000;

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let job_id = segments
        .get(5)
        .and_then(|value| DeviceJobId::parse((*value).to_owned()).ok());

    let Some(resolved) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let actor = resolved.actor();
    if route != RouteClass::DeviceJobClaimableApi && job_id.is_none() {
        return neutral_not_found(actor.correlation_id().as_str());
    }

    let trusted_device = authenticated_device(env)?;
    let device_id = match trusted_device.authenticated_device_id(actor).await {
        Ok(value) => value,
        Err(error) => return identity_failure(actor.correlation_id().as_str(), error.class()),
    };
    let bound_device = ResolvedAuthenticatedDevice::new(device_id);

    match route {
        RouteClass::DeviceJobClaimableApi => list_claimable(env, actor, &bound_device).await,
        RouteClass::DeviceJobClaimApi => {
            claim(
                request,
                env,
                actor,
                &bound_device,
                job_id.expect("validated device job route id"),
            )
            .await
        }
        RouteClass::DeviceJobHeartbeatApi => {
            heartbeat(
                request,
                env,
                actor,
                &bound_device,
                job_id.expect("validated device job route id"),
            )
            .await
        }
        RouteClass::DeviceJobOutcomeApi => {
            apply_outcome(
                request,
                env,
                actor,
                &bound_device,
                job_id.expect("validated device job route id"),
            )
            .await
        }
        _ => neutral_not_found(actor.correlation_id().as_str()),
    }
}

async fn list_claimable(
    env: &Env,
    actor: &ActorContext,
    device_identity: &ResolvedAuthenticatedDevice,
) -> Result<Response> {
    let now = server_now();
    let authorization = device_job_authorization(env)?;
    let preconditions = device_execution_preconditions(env)?;
    let repository = device_job_repository(env)?;
    let request = match ListClaimableDeviceJobsRequest::new(CLAIMABLE_PAGE_SIZE, now) {
        Ok(value) => value,
        Err(error) => return query_failure(actor.correlation_id().as_str(), error),
    };
    match execute_list_claimable_device_jobs(
        actor,
        device_identity,
        &authorization,
        &preconditions,
        &repository,
        request,
    )
    .await
    {
        Ok(jobs) => Response::from_json(&ClaimableDeviceJobsResponse {
            jobs: jobs.iter().map(DeviceJobResponse::from).collect(),
        }),
        Err(error) => query_failure(actor.correlation_id().as_str(), error),
    }
}

async fn claim(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    device_identity: &ResolvedAuthenticatedDevice,
    job_id: DeviceJobId,
) -> Result<Response> {
    let body = match request.json::<ClaimDeviceJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let Some((target, expected_version)) = parse_target_and_version(
        actor,
        device_identity,
        &body.profile_id,
        &body.generation_id,
        body.expected_job_version,
    ) else {
        return invalid_request(actor.correlation_id().as_str());
    };
    let claim_id = match DeviceClaimId::parse(body.claim_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let now = server_now();
    let lease_expires_at = match checked_future(now, DEVICE_CLAIM_LEASE_MS) {
        Some(value) => value,
        None => return integrity_failure(actor.correlation_id().as_str()),
    };
    let authorization = device_job_authorization(env)?;
    let preconditions = device_execution_preconditions(env)?;
    let repository = device_job_repository(env)?;
    match execute_claim_device_job(
        actor,
        device_identity,
        &authorization,
        &preconditions,
        &repository,
        ClaimDeviceJobCommand::new(
            job_id,
            target,
            expected_version,
            claim_id,
            now,
            lease_expires_at,
        ),
    )
    .await
    {
        Ok(job) => Response::from_json(&DeviceJobResponse::from(&job)),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn heartbeat(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    device_identity: &ResolvedAuthenticatedDevice,
    job_id: DeviceJobId,
) -> Result<Response> {
    let body = match request.json::<HeartbeatDeviceJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let Some((target, expected_version)) = parse_target_and_version(
        actor,
        device_identity,
        &body.profile_id,
        &body.generation_id,
        body.expected_job_version,
    ) else {
        return invalid_request(actor.correlation_id().as_str());
    };
    let claim_id = match DeviceClaimId::parse(body.claim_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if body.fence == 0 {
        return invalid_request(actor.correlation_id().as_str());
    }
    let now = server_now();
    let lease_expires_at = match checked_future(now, DEVICE_CLAIM_LEASE_MS) {
        Some(value) => value,
        None => return integrity_failure(actor.correlation_id().as_str()),
    };
    let authorization = device_job_authorization(env)?;
    let repository = device_job_repository(env)?;
    match execute_heartbeat_device_job(
        actor,
        device_identity,
        &authorization,
        &repository,
        HeartbeatDeviceJobCommand::new(
            job_id,
            target,
            expected_version,
            claim_id,
            body.fence,
            now,
            lease_expires_at,
        ),
    )
    .await
    {
        Ok(job) => Response::from_json(&DeviceJobResponse::from(&job)),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn apply_outcome(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    device_identity: &ResolvedAuthenticatedDevice,
    job_id: DeviceJobId,
) -> Result<Response> {
    let body = match request.json::<ApplyDeviceJobOutcomeRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let Some((target, expected_version)) = parse_target_and_version(
        actor,
        device_identity,
        &body.profile_id,
        &body.generation_id,
        body.expected_job_version,
    ) else {
        return invalid_request(actor.correlation_id().as_str());
    };
    let claim_id = match DeviceClaimId::parse(body.claim_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if body.fence == 0 {
        return invalid_request(actor.correlation_id().as_str());
    }
    let now = server_now();
    let outcome = match body.outcome {
        DeviceJobOutcomeRequest::Succeeded if body.retry_delay_ms.is_none() => {
            DeviceJobOutcome::Succeeded
        }
        DeviceJobOutcomeRequest::AuthRequired if body.retry_delay_ms.is_none() => {
            DeviceJobOutcome::AuthRequired
        }
        DeviceJobOutcomeRequest::RecoveryRequired if body.retry_delay_ms.is_none() => {
            DeviceJobOutcome::RecoveryRequired
        }
        DeviceJobOutcomeRequest::Failed if body.retry_delay_ms.is_none() => DeviceJobOutcome::Failed,
        DeviceJobOutcomeRequest::RetryScheduled | DeviceJobOutcomeRequest::ProfileBusy => {
            let Some(delay) = body.retry_delay_ms else {
                return invalid_request(actor.correlation_id().as_str());
            };
            if delay == 0 || delay > MAX_DEVICE_RETRY_DELAY_MS {
                return invalid_request(actor.correlation_id().as_str());
            }
            let Some(retry_at) = checked_future(now, delay) else {
                return integrity_failure(actor.correlation_id().as_str());
            };
            if body.outcome == DeviceJobOutcomeRequest::RetryScheduled {
                DeviceJobOutcome::RetryScheduled { retry_at }
            } else {
                DeviceJobOutcome::ProfileBusy { retry_at }
            }
        }
        _ => return invalid_request(actor.correlation_id().as_str()),
    };
    let authorization = device_job_authorization(env)?;
    let preconditions = device_execution_preconditions(env)?;
    let repository = device_job_repository(env)?;
    match execute_apply_device_job_outcome(
        actor,
        device_identity,
        &authorization,
        &preconditions,
        &repository,
        ApplyDeviceJobOutcomeCommand::new(
            job_id,
            target,
            expected_version,
            claim_id,
            body.fence,
            now,
            outcome,
        ),
    )
    .await
    {
        Ok(job) => Response::from_json(&DeviceJobResponse::from(&job)),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn parse_target_and_version(
    actor: &ActorContext,
    device_identity: &ResolvedAuthenticatedDevice,
    profile_id: &str,
    generation_id: &str,
    expected_job_version: u64,
) -> Option<(DeviceJobTarget, AggregateVersion)> {
    let profile_id = ProfileId::parse(profile_id.to_owned()).ok()?;
    let generation_id = GenerationId::parse(generation_id.to_owned()).ok()?;
    let expected_version = AggregateVersion::new(expected_job_version).ok()?;
    Some((
        DeviceJobTarget::new(
            actor.tenant_scope().tenant_id().clone(),
            device_identity.device_id().clone(),
            profile_id,
            generation_id,
        ),
        expected_version,
    ))
}

fn server_now() -> UnixMillis {
    UnixMillis::new(Date::now().as_millis())
}

fn checked_future(now: UnixMillis, delta_ms: u64) -> Option<UnixMillis> {
    now.value().checked_add(delta_ms).map(UnixMillis::new)
}

fn identity_failure(correlation_id: &str, class: DeviceJobPortErrorClass) -> Result<Response> {
    match class {
        DeviceJobPortErrorClass::AuthenticationFailed => {
            problem(correlation_id, 403, "forbidden", "Forbidden")
        }
        DeviceJobPortErrorClass::IntegrityFailure => integrity_failure(correlation_id),
        DeviceJobPortErrorClass::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn query_failure(correlation_id: &str, error: DeviceJobQueryError) -> Result<Response> {
    match error {
        DeviceJobQueryError::InvalidRequest => invalid_request(correlation_id),
        DeviceJobQueryError::Forbidden => problem(correlation_id, 403, "forbidden", "Forbidden"),
        DeviceJobQueryError::IntegrityFailure => integrity_failure(correlation_id),
        DeviceJobQueryError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn operation_failure(correlation_id: &str, error: DeviceJobOperationError) -> Result<Response> {
    match error {
        DeviceJobOperationError::InvalidRequest => invalid_request(correlation_id),
        DeviceJobOperationError::Forbidden => problem(correlation_id, 403, "forbidden", "Forbidden"),
        DeviceJobOperationError::NotFound => neutral_not_found(correlation_id),
        DeviceJobOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        DeviceJobOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        DeviceJobOperationError::PreconditionFailed(_) => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        DeviceJobOperationError::Domain(error) => domain_failure(correlation_id, error),
        DeviceJobOperationError::IntegrityFailure => integrity_failure(correlation_id),
        DeviceJobOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn domain_failure(correlation_id: &str, error: DeviceJobError) -> Result<Response> {
    match error {
        DeviceJobError::InvalidSnapshot
        | DeviceJobError::AttemptOverflow
        | DeviceJobError::FenceOverflow
        | DeviceJobError::VersionOverflow => integrity_failure(correlation_id),
        DeviceJobError::InvalidMaxAttempts
        | DeviceJobError::InvalidLease
        | DeviceJobError::InvalidRetryAt => invalid_request(correlation_id),
        DeviceJobError::StaleClaim | DeviceJobError::LeaseExpired | DeviceJobError::LeaseStillActive => {
            problem(correlation_id, 409, "lease_conflict", "Lease Conflict")
        }
        DeviceJobError::InvalidState
        | DeviceJobError::NotDue
        | DeviceJobError::RecoveryRequired
        | DeviceJobError::ClaimAlreadyActive
        | DeviceJobError::MissingActiveClaim
        | DeviceJobError::AttemptsExhausted
        | DeviceJobError::TimeRegression => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn integrity_failure(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 500, "integrity_failure", "Integrity Failure")
}

#[derive(Clone)]
struct ResolvedAuthenticatedDevice {
    device_id: DeviceId,
}

impl ResolvedAuthenticatedDevice {
    const fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }

    const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

impl AuthenticatedDevicePort for ResolvedAuthenticatedDevice {
    async fn authenticated_device_id(
        &self,
        _actor: &ActorContext,
    ) -> core::result::Result<DeviceId, DeviceJobPortError> {
        Ok(self.device_id.clone())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClaimDeviceJobRequest {
    profile_id: String,
    generation_id: String,
    expected_job_version: u64,
    claim_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeartbeatDeviceJobRequest {
    profile_id: String,
    generation_id: String,
    expected_job_version: u64,
    claim_id: String,
    fence: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum DeviceJobOutcomeRequest {
    Succeeded,
    RetryScheduled,
    ProfileBusy,
    AuthRequired,
    RecoveryRequired,
    Failed,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyDeviceJobOutcomeRequest {
    profile_id: String,
    generation_id: String,
    expected_job_version: u64,
    claim_id: String,
    fence: u64,
    outcome: DeviceJobOutcomeRequest,
    retry_delay_ms: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaimableDeviceJobsResponse<'a> {
    jobs: Vec<DeviceJobResponse<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceClaimResponse<'a> {
    claim_id: &'a str,
    fence: u64,
    lease_expires_at_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceJobResponse<'a> {
    job_id: &'a str,
    profile_id: &'a str,
    generation_id: &'a str,
    status: &'static str,
    attempt: u32,
    max_attempts: u32,
    last_fence: u64,
    aggregate_version: u64,
    claim: Option<DeviceClaimResponse<'a>>,
    retry_at_ms: Option<u64>,
}

impl<'a> From<&'a DeviceJob> for DeviceJobResponse<'a> {
    fn from(job: &'a DeviceJob) -> Self {
        Self {
            job_id: job.job_id().as_str(),
            profile_id: job.target().profile_id().as_str(),
            generation_id: job.target().generation_id().as_str(),
            status: status_value(job.status()),
            attempt: job.attempt(),
            max_attempts: job.max_attempts(),
            last_fence: job.last_fence(),
            aggregate_version: job.version().value(),
            claim: job.active_claim().map(|claim| DeviceClaimResponse {
                claim_id: claim.claim_id().as_str(),
                fence: claim.fence(),
                lease_expires_at_ms: claim.lease_expires_at().value(),
            }),
            retry_at_ms: job.retry_at().map(UnixMillis::value),
        }
    }
}

const fn status_value(status: DeviceJobStatus) -> &'static str {
    match status {
        DeviceJobStatus::PendingDevice => "PENDING_DEVICE",
        DeviceJobStatus::ProfileBusy => "PROFILE_BUSY",
        DeviceJobStatus::Running => "RUNNING",
        DeviceJobStatus::RetryScheduled => "RETRY_SCHEDULED",
        DeviceJobStatus::AuthRequired => "AUTH_REQUIRED",
        DeviceJobStatus::RecoveryRequired => "RECOVERY_REQUIRED",
        DeviceJobStatus::Succeeded => "SUCCEEDED",
        DeviceJobStatus::Failed => "FAILED",
        DeviceJobStatus::Cancelled => "CANCELLED",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyDeviceJobOutcomeRequest, CLAIMABLE_PAGE_SIZE, ClaimDeviceJobRequest,
        DEVICE_CLAIM_LEASE_MS, HeartbeatDeviceJobRequest, MAX_DEVICE_RETRY_DELAY_MS,
        checked_future,
    };
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn transport_rejects_device_time_and_lease_substitution_fields() {
        let base = r#"{"profileId":"profile_01JDEVICE","generationId":"generation_01JDEVICE","expectedJobVersion":1,"claimId":"devclaim_01JDEVICE"}"#;
        assert!(serde_json::from_str::<ClaimDeviceJobRequest>(base).is_ok());
        for forbidden in [
            "deviceId",
            "observedAtMs",
            "leaseExpiresAtMs",
            "tenantId",
        ] {
            let tampered = base.replacen('}', &format!(r#","{forbidden}":1}}"#), 1);
            assert!(
                serde_json::from_str::<ClaimDeviceJobRequest>(&tampered).is_err(),
                "forbidden field unexpectedly accepted: {forbidden}"
            );
        }
    }

    #[test]
    fn heartbeat_and_outcome_are_strict_and_retry_is_relative_only() {
        let heartbeat = r#"{"profileId":"profile_01JDEVICE","generationId":"generation_01JDEVICE","expectedJobVersion":2,"claimId":"devclaim_01JDEVICE","fence":1}"#;
        assert!(serde_json::from_str::<HeartbeatDeviceJobRequest>(heartbeat).is_ok());
        let outcome = r#"{"profileId":"profile_01JDEVICE","generationId":"generation_01JDEVICE","expectedJobVersion":2,"claimId":"devclaim_01JDEVICE","fence":1,"outcome":"RETRY_SCHEDULED","retryDelayMs":1000}"#;
        assert!(serde_json::from_str::<ApplyDeviceJobOutcomeRequest>(outcome).is_ok());
        for forbidden in ["deviceId", "observedAtMs", "retryAtMs", "leaseExpiresAtMs"] {
            let tampered = outcome.replacen('}', &format!(r#","{forbidden}":1234}}"#), 1);
            assert!(
                serde_json::from_str::<ApplyDeviceJobOutcomeRequest>(&tampered).is_err(),
                "forbidden outcome field unexpectedly accepted: {forbidden}"
            );
        }
    }

    #[test]
    fn server_policy_bounds_are_positive_and_checked() {
        assert!(CLAIMABLE_PAGE_SIZE > 0);
        assert!(DEVICE_CLAIM_LEASE_MS > 0);
        assert!(MAX_DEVICE_RETRY_DELAY_MS >= DEVICE_CLAIM_LEASE_MS);
        assert_eq!(
            checked_future(UnixMillis::new(100), DEVICE_CLAIM_LEASE_MS),
            Some(UnixMillis::new(100 + DEVICE_CLAIM_LEASE_MS))
        );
        assert_eq!(checked_future(UnixMillis::new(u64::MAX), 1), None);
    }
}
