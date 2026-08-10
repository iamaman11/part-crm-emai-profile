use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::composition::{
    authenticated_device, device_execution_preconditions, device_generation_commit,
    device_job_authorization, device_job_repository,
};
use application_ports::AuthenticatedDevicePort;
use application_ports::device_generation_commit::{
    CoordinatorGenerationCommitWitness, DeviceGenerationCommitErrorClass,
    DeviceGenerationCommitOutcome, DeviceGenerationCommitRequest,
};
use application_ports::generation_objects::GenerationObjectDescriptor;
use cloudflare_adapters::r2_generation_objects::R2GenerationObjects;
use control_plane_contract::R2_PROFILES_BINDING;
use device_domain::{DeviceClaimId, DeviceJobId};
use profile_platform_primitives::{
    AggregateVersion, DeviceId, FencingToken, GenerationId, ProfileId, SessionId, UnixMillis,
};
use serde::{Deserialize, Serialize};
use use_cases_devices::{
    DeviceGenerationCommitOperationError, DeviceGenerationCommitServices,
    execute_commit_dirty_generation,
};
use worker::{Date, Env, Request, Response, Result};

pub async fn dispatch(request: &mut Request, env: &Env) -> Result<Response> {
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
    let Some(job_id) = job_id else {
        return neutral_not_found(actor.correlation_id().as_str());
    };
    let body = match request.json::<DeviceGenerationCommitBody>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };

    let trusted_identity = authenticated_device(env)?;
    let device_id = match trusted_identity.authenticated_device_id(actor).await {
        Ok(value) => value,
        Err(_) => return forbidden(actor.correlation_id().as_str()),
    };
    let request = match body.into_domain(job_id, device_id, server_now()) {
        Ok(value) => value,
        Err(()) => return invalid_request(actor.correlation_id().as_str()),
    };

    let identity = ResolvedAuthenticatedDevice::new(request.device_id().clone());
    let authorization = device_job_authorization(env)?;
    let preconditions = device_execution_preconditions(env)?;
    let repository = device_job_repository(env)?;
    let verifier = R2GenerationObjects::new(env.bucket(R2_PROFILES_BINDING)?);
    let commit = device_generation_commit(env);
    let services = DeviceGenerationCommitServices::new(
        &identity,
        &authorization,
        &preconditions,
        &repository,
        &verifier,
        &commit,
    );

    match execute_commit_dirty_generation(actor, &services, &request).await {
        Ok(outcome) => Response::from_json(&DeviceGenerationCommitResponse::from(outcome)),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn server_now() -> UnixMillis {
    UnixMillis::new(Date::now().as_millis())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceGenerationCommitBody {
    profile_id: String,
    base_generation_id: String,
    expected_job_version: u64,
    claim_id: String,
    fence: u64,
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
    expected_profile_version: u64,
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
    coordinator_version: u64,
    coordinator_sequence: u64,
}

impl DeviceGenerationCommitBody {
    fn into_domain(
        self,
        job_id: DeviceJobId,
        device_id: DeviceId,
        observed_at: UnixMillis,
    ) -> core::result::Result<DeviceGenerationCommitRequest, ()> {
        let profile_id = ProfileId::parse(self.profile_id).map_err(|_| ())?;
        let base_generation_id = GenerationId::parse(self.base_generation_id).map_err(|_| ())?;
        let generation_id = GenerationId::parse(self.generation_id).map_err(|_| ())?;
        Ok(DeviceGenerationCommitRequest::new(
            job_id,
            DeviceClaimId::parse(self.claim_id).map_err(|_| ())?,
            AggregateVersion::new(self.expected_job_version).map_err(|_| ())?,
            self.fence,
            device_id,
            profile_id.clone(),
            base_generation_id,
            GenerationObjectDescriptor::new(
                profile_id,
                generation_id,
                self.object_key,
                self.metadata_digest,
                self.container_digest,
                self.container_bytes,
            ),
            AggregateVersion::new(self.expected_profile_version).map_err(|_| ())?,
            CoordinatorGenerationCommitWitness::new(
                SessionId::parse(self.coordinator_session_id).map_err(|_| ())?,
                FencingToken::parse(self.coordinator_fencing_token).map_err(|_| ())?,
                self.coordinator_epoch,
                self.coordinator_version,
                self.coordinator_sequence,
            ),
            observed_at,
        ))
    }
}

#[derive(Clone)]
struct ResolvedAuthenticatedDevice {
    device_id: DeviceId,
}

impl ResolvedAuthenticatedDevice {
    const fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }
}

impl AuthenticatedDevicePort for ResolvedAuthenticatedDevice {
    async fn authenticated_device_id(
        &self,
        _actor: &profile_platform_primitives::ActorContext,
    ) -> core::result::Result<DeviceId, application_ports::DeviceJobPortError> {
        Ok(self.device_id.clone())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum DeviceGenerationCommitResponseValue {
    Activated,
    AlreadyActive,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceGenerationCommitResponse {
    outcome: DeviceGenerationCommitResponseValue,
}

impl From<DeviceGenerationCommitOutcome> for DeviceGenerationCommitResponse {
    fn from(outcome: DeviceGenerationCommitOutcome) -> Self {
        Self {
            outcome: match outcome {
                DeviceGenerationCommitOutcome::Activated => {
                    DeviceGenerationCommitResponseValue::Activated
                }
                DeviceGenerationCommitOutcome::AlreadyActive => {
                    DeviceGenerationCommitResponseValue::AlreadyActive
                }
            },
        }
    }
}

fn operation_failure(
    correlation_id: &str,
    error: DeviceGenerationCommitOperationError,
) -> Result<Response> {
    match error {
        DeviceGenerationCommitOperationError::InvalidRequest => invalid_request(correlation_id),
        DeviceGenerationCommitOperationError::Forbidden => forbidden(correlation_id),
        DeviceGenerationCommitOperationError::NotFound => neutral_not_found(correlation_id),
        DeviceGenerationCommitOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        DeviceGenerationCommitOperationError::StaleClaim => {
            problem(correlation_id, 409, "lease_conflict", "Lease Conflict")
        }
        DeviceGenerationCommitOperationError::PreconditionFailed(_) => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        DeviceGenerationCommitOperationError::ObjectVerificationFailed => {
            problem(correlation_id, 409, "integrity_failure", "Integrity Failure")
        }
        DeviceGenerationCommitOperationError::IntegrityFailure => integrity_failure(correlation_id),
        DeviceGenerationCommitOperationError::DependencyUnavailable => dependency(correlation_id),
        DeviceGenerationCommitOperationError::Commit(class) => {
            commit_failure(correlation_id, class)
        }
    }
}

fn commit_failure(correlation_id: &str, class: DeviceGenerationCommitErrorClass) -> Result<Response> {
    match class {
        DeviceGenerationCommitErrorClass::StaleAuthority => {
            problem(correlation_id, 409, "lease_conflict", "Lease Conflict")
        }
        DeviceGenerationCommitErrorClass::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        DeviceGenerationCommitErrorClass::IntegrityFailure => integrity_failure(correlation_id),
        DeviceGenerationCommitErrorClass::DependencyUnavailable => dependency(correlation_id),
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn forbidden(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 403, "forbidden", "Forbidden")
}

fn integrity_failure(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        500,
        "integrity_failure",
        "Integrity Failure",
    )
}

fn dependency(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        503,
        "dependency_unavailable",
        "Dependency Unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::DeviceGenerationCommitBody;

    #[test]
    fn transport_is_metadata_only_and_rejects_client_authority_fields() {
        let base = r#"{
            "profileId":"profile_commit_route_01",
            "baseGenerationId":"generation_commit_route_base_01",
            "expectedJobVersion":2,
            "claimId":"devclaim_commit_route_01",
            "fence":1,
            "generationId":"generation_commit_route_candidate_01",
            "objectKey":"tenants/tenant_commit_route_01/profiles/profile_commit_route_01/generations/generation_commit_route_candidate_01.bpgc",
            "metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "containerBytes":4096,
            "expectedProfileVersion":3,
            "coordinatorSessionId":"session_commit_route_01",
            "coordinatorFencingToken":"fence_commit_route_01",
            "coordinatorEpoch":4,
            "coordinatorVersion":9,
            "coordinatorSequence":8
        }"#;
        assert!(serde_json::from_str::<DeviceGenerationCommitBody>(base).is_ok());
        for forbidden in [
            "deviceId",
            "tenantId",
            "observedAtMs",
            "executedAtMs",
            "ciphertext",
            "container",
        ] {
            let tampered = base.replacen('}', &format!(r#", "{forbidden}": 1}}"#), 1);
            assert!(
                serde_json::from_str::<DeviceGenerationCommitBody>(&tampered).is_err(),
                "forbidden generation commit field unexpectedly accepted: {forbidden}"
            );
        }
    }
}
