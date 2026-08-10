use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::composition::{
    authenticated_device, coordinator_ingress_application, device_execution_preconditions,
    device_generation_commit, device_generation_replay_probe, device_job_authorization,
    device_job_repository, generation_object_verifier,
};
use application_ports::coordinator_ingress::{
    CoordinatorIngressApplicationPort, CoordinatorIngressPortErrorClass,
};
use application_ports::device_generation_commit::{
    CoordinatorGenerationCommitWitness, DeviceGenerationCommitErrorClass,
    DeviceGenerationCommitOutcome, DeviceGenerationCommitRequest, DeviceGenerationProfileVersionPort,
    DeviceGenerationReplayProbe, DeviceGenerationReplayProbeOutcome, DeviceGenerationReplayProbePort,
};
use application_ports::device_jobs::{
    DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobRepositoryPort,
};
use application_ports::generation_objects::{
    GenerationObjectDescriptor, GenerationObjectDescriptorVerifyPort,
};
use application_ports::generations::GenerationPortErrorClass;
use application_ports::{AuthenticatedDevicePort, DeviceJobPortErrorClass};
use device_domain::{DeviceClaimId, DeviceJobId, DeviceJobStatus};
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
    let profile_id = match ProfileId::parse(body.profile_id.clone()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let base_generation_id = match GenerationId::parse(body.base_generation_id.clone()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let claim_id = match DeviceClaimId::parse(body.claim_id.clone()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let session_id = match SessionId::parse(body.coordinator_session_id.clone()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if body.fence == 0 || body.coordinator_epoch == 0 {
        return invalid_request(actor.correlation_id().as_str());
    }
    let now = server_now();

    let trusted_identity = authenticated_device(env)?;
    let device_id = match trusted_identity.authenticated_device_id(actor).await {
        Ok(value) => value,
        Err(error) => {
            return device_port_failure(actor.correlation_id().as_str(), error.class());
        }
    };

    let repository = device_job_repository(env)?;
    let job = match repository
        .load_device_job(actor.tenant_scope().tenant_id(), &job_id)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return neutral_not_found(actor.correlation_id().as_str()),
        Err(error) => {
            return device_port_failure(actor.correlation_id().as_str(), error.class());
        }
    };
    if job.target().device_id() != &device_id
        || job.target().profile_id() != &profile_id
        || job.target().generation_id() != &base_generation_id
    {
        return neutral_not_found(actor.correlation_id().as_str());
    }

    let authorization = device_job_authorization(env)?;
    match authorization
        .is_device_job_authorized(actor, job.target(), DeviceJobCapability::Complete)
        .await
    {
        Ok(true) => {}
        Ok(false) => return forbidden(actor.correlation_id().as_str()),
        Err(error) => {
            return device_port_failure(actor.correlation_id().as_str(), error.class());
        }
    }

    if job.status() == DeviceJobStatus::Succeeded {
        let probe = match body.replay_probe(
            job_id,
            claim_id,
            device_id,
            profile_id,
            base_generation_id,
            session_id,
        ) {
            Ok(value) => value,
            Err(()) => return invalid_request(actor.correlation_id().as_str()),
        };
        let replay = device_generation_replay_probe(env)?;
        match replay.probe_committed_generation(actor, &probe).await {
            Ok(DeviceGenerationReplayProbeOutcome::ExactCommitted) => {}
            Ok(DeviceGenerationReplayProbeOutcome::Missing)
            | Ok(DeviceGenerationReplayProbeOutcome::Conflict) => {
                return version_conflict(actor.correlation_id().as_str());
            }
            Err(error) => {
                return generation_commit_port_failure(
                    actor.correlation_id().as_str(),
                    error.class(),
                );
            }
        }
        let verifier = generation_object_verifier(env)?;
        match verifier
            .verify_generation_object_descriptor_exact(actor.tenant_scope(), probe.object())
            .await
        {
            Ok(true) => {
                return Response::from_json(&DeviceGenerationCommitResponse::from(
                    DeviceGenerationCommitOutcome::AlreadyActive,
                ));
            }
            Ok(false) => return integrity_failure(actor.correlation_id().as_str()),
            Err(error) => {
                return replay_object_failure(actor.correlation_id().as_str(), error.class());
            }
        }
    }

    if job.status() != DeviceJobStatus::Running {
        return stale_authority(actor.correlation_id().as_str());
    }
    let Some(claim) = job.active_claim() else {
        return stale_authority(actor.correlation_id().as_str());
    };
    if claim.claim_id() != &claim_id
        || claim.fence() != body.fence
        || job.last_fence() != body.fence
        || claim.target() != job.target()
        || claim.is_expired(now)
    {
        return stale_authority(actor.correlation_id().as_str());
    }
    let expected_job_version = job.version();

    let preconditions = device_execution_preconditions(env)?;
    let expected_profile_version = match preconditions
        .load_active_profile_version(actor, &profile_id, &base_generation_id)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return version_conflict(actor.correlation_id().as_str()),
        Err(error) => {
            return generation_commit_port_failure(actor.correlation_id().as_str(), error.class());
        }
    };

    let coordinator = coordinator_ingress_application(env);
    let snapshot = match coordinator
        .snapshot(actor.tenant_scope(), &profile_id)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            return coordinator_snapshot_failure(actor.correlation_id().as_str(), error.class());
        }
    };
    let projection = snapshot.projection();
    if projection.active_session_id() != Some(&session_id)
        || projection.active_device_id() != Some(&device_id)
        || projection.active_epoch() != Some(body.coordinator_epoch)
    {
        return stale_authority(actor.correlation_id().as_str());
    }

    let request = match body.into_domain(
        job_id,
        claim_id,
        device_id,
        profile_id,
        base_generation_id,
        now,
        expected_job_version,
        expected_profile_version,
        snapshot.version().value(),
        snapshot.sequence(),
        session_id,
    ) {
        Ok(value) => value,
        Err(()) => return invalid_request(actor.correlation_id().as_str()),
    };

    let identity = ResolvedAuthenticatedDevice::new(request.device_id().clone());
    let preconditions = device_execution_preconditions(env)?;
    let verifier = generation_object_verifier(env)?;
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceGenerationCommitBody {
    profile_id: String,
    base_generation_id: String,
    claim_id: String,
    fence: u64,
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
}

impl DeviceGenerationCommitBody {
    #[allow(clippy::too_many_arguments)]
    fn replay_probe(
        &self,
        job_id: DeviceJobId,
        claim_id: DeviceClaimId,
        device_id: DeviceId,
        profile_id: ProfileId,
        base_generation_id: GenerationId,
        coordinator_session_id: SessionId,
    ) -> core::result::Result<DeviceGenerationReplayProbe, ()> {
        let generation_id = GenerationId::parse(self.generation_id.clone()).map_err(|_| ())?;
        Ok(DeviceGenerationReplayProbe::new(
            job_id,
            claim_id,
            self.fence,
            device_id,
            profile_id.clone(),
            base_generation_id,
            GenerationObjectDescriptor::new(
                profile_id,
                generation_id,
                self.object_key.clone(),
                self.metadata_digest.clone(),
                self.container_digest.clone(),
                self.container_bytes,
            ),
            coordinator_session_id,
            FencingToken::parse(self.coordinator_fencing_token.clone()).map_err(|_| ())?,
            self.coordinator_epoch,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn into_domain(
        self,
        job_id: DeviceJobId,
        claim_id: DeviceClaimId,
        device_id: DeviceId,
        profile_id: ProfileId,
        base_generation_id: GenerationId,
        observed_at: UnixMillis,
        expected_job_version: AggregateVersion,
        expected_profile_version: AggregateVersion,
        coordinator_version: u64,
        coordinator_sequence: u64,
        coordinator_session_id: SessionId,
    ) -> core::result::Result<DeviceGenerationCommitRequest, ()> {
        let generation_id = GenerationId::parse(self.generation_id).map_err(|_| ())?;
        Ok(DeviceGenerationCommitRequest::new(
            job_id,
            claim_id,
            expected_job_version,
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
            expected_profile_version,
            CoordinatorGenerationCommitWitness::new(
                coordinator_session_id,
                FencingToken::parse(self.coordinator_fencing_token).map_err(|_| ())?,
                self.coordinator_epoch,
                coordinator_version,
                coordinator_sequence,
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

fn device_port_failure(correlation_id: &str, class: DeviceJobPortErrorClass) -> Result<Response> {
    match class {
        DeviceJobPortErrorClass::AuthenticationFailed => forbidden(correlation_id),
        DeviceJobPortErrorClass::IntegrityFailure => integrity_failure(correlation_id),
        DeviceJobPortErrorClass::DependencyUnavailable => dependency(correlation_id),
    }
}

fn generation_commit_port_failure(
    correlation_id: &str,
    class: DeviceGenerationCommitErrorClass,
) -> Result<Response> {
    match class {
        DeviceGenerationCommitErrorClass::StaleAuthority => stale_authority(correlation_id),
        DeviceGenerationCommitErrorClass::VersionConflict => version_conflict(correlation_id),
        DeviceGenerationCommitErrorClass::IntegrityFailure => integrity_failure(correlation_id),
        DeviceGenerationCommitErrorClass::DependencyUnavailable => dependency(correlation_id),
    }
}

fn replay_object_failure(correlation_id: &str, class: GenerationPortErrorClass) -> Result<Response> {
    match class {
        GenerationPortErrorClass::DependencyUnavailable | GenerationPortErrorClass::InternalFailure => {
            dependency(correlation_id)
        }
        GenerationPortErrorClass::NotFound
        | GenerationPortErrorClass::VersionConflict
        | GenerationPortErrorClass::InvalidState
        | GenerationPortErrorClass::Conflict
        | GenerationPortErrorClass::IntegrityFailure => integrity_failure(correlation_id),
    }
}

fn coordinator_snapshot_failure(
    correlation_id: &str,
    class: CoordinatorIngressPortErrorClass,
) -> Result<Response> {
    match class {
        CoordinatorIngressPortErrorClass::NotFound => neutral_not_found(correlation_id),
        CoordinatorIngressPortErrorClass::Conflict => stale_authority(correlation_id),
        CoordinatorIngressPortErrorClass::InvalidRequest
        | CoordinatorIngressPortErrorClass::IntegrityFailure
        | CoordinatorIngressPortErrorClass::InternalFailure => integrity_failure(correlation_id),
        CoordinatorIngressPortErrorClass::DependencyUnavailable => dependency(correlation_id),
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
        DeviceGenerationCommitOperationError::VersionConflict => version_conflict(correlation_id),
        DeviceGenerationCommitOperationError::StaleClaim => stale_authority(correlation_id),
        DeviceGenerationCommitOperationError::PreconditionFailed(_) => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        DeviceGenerationCommitOperationError::ObjectVerificationFailed => problem(
            correlation_id,
            409,
            "integrity_failure",
            "Integrity Failure",
        ),
        DeviceGenerationCommitOperationError::IntegrityFailure => integrity_failure(correlation_id),
        DeviceGenerationCommitOperationError::DependencyUnavailable => dependency(correlation_id),
        DeviceGenerationCommitOperationError::Commit(class) => {
            generation_commit_port_failure(correlation_id, class)
        }
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn forbidden(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 403, "forbidden", "Forbidden")
}

fn stale_authority(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 409, "lease_conflict", "Lease Conflict")
}

fn version_conflict(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 409, "version_conflict", "Version Conflict")
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
            "claimId":"devclaim_commit_route_01",
            "fence":1,
            "generationId":"generation_commit_route_candidate_01",
            "objectKey":"tenants/tenant_commit_route_01/profiles/profile_commit_route_01/generations/generation_commit_route_candidate_01.bpgc",
            "metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "containerBytes":4096,
            "coordinatorSessionId":"session_commit_route_01",
            "coordinatorFencingToken":"fence_commit_route_01",
            "coordinatorEpoch":4
        }"#;
        assert!(serde_json::from_str::<DeviceGenerationCommitBody>(base).is_ok());
        for forbidden in [
            "deviceId",
            "tenantId",
            "observedAtMs",
            "executedAtMs",
            "expectedJobVersion",
            "expectedProfileVersion",
            "coordinatorVersion",
            "coordinatorSequence",
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
