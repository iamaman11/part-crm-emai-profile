use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::composition::{
    authenticated_device, coordinator_ingress_application, device_execution_preconditions,
    device_job_authorization, device_job_repository, generation_upload_capability_signer,
};
use application_ports::coordinator_ingress::{
    CoordinatorIngressApplicationPort, CoordinatorIngressPortErrorClass,
};
use application_ports::device_generation_commit::{
    DeviceGenerationCommitErrorClass, DeviceGenerationProfileVersionPort,
};
use application_ports::device_jobs::{
    DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobRepositoryPort,
};
use application_ports::generation_objects::GenerationObjectDescriptor;
use application_ports::{AuthenticatedDevicePort, DeviceJobPortErrorClass};
use cloudflare_adapters::r2_generation_upload_capability::{
    R2GenerationUploadCapabilityError, R2GenerationUploadSigningTime,
};
use device_domain::{DeviceClaimId, DeviceJobId, DeviceJobStatus};
use profile_platform_primitives::{GenerationId, ProfileId, SessionId, UnixMillis};
use serde::{Deserialize, Serialize};
use worker::{Date, Env, Request, Response, Result};

const CAPABILITY_EXPIRES_SECONDS: u32 = 300;
type SigningTimeResult =
    core::result::Result<R2GenerationUploadSigningTime, R2GenerationUploadCapabilityError>;

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
    let body = match request.json::<DeviceGenerationUploadCapabilityBody>().await {
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
    if body.fence == 0 || body.coordinator_epoch == 0 || body.container_bytes == 0 {
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

    let preconditions = device_execution_preconditions(env)?;
    match preconditions
        .load_active_profile_version(actor, &profile_id, &base_generation_id)
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => return version_conflict(actor.correlation_id().as_str()),
        Err(error) => {
            return generation_precondition_failure(actor.correlation_id().as_str(), error.class());
        }
    }

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
    if snapshot.sequence().checked_add(1) != Some(snapshot.version().value()) {
        return integrity_failure(actor.correlation_id().as_str());
    }

    let descriptor = match body.into_descriptor(profile_id, &base_generation_id) {
        Ok(value) => value,
        Err(()) => return invalid_request(actor.correlation_id().as_str()),
    };
    let signer = match generation_upload_capability_signer(env) {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    let signing_time = match server_signing_time() {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    let capability = match signer.sign_put(
        actor.tenant_scope(),
        &descriptor,
        &signing_time,
        CAPABILITY_EXPIRES_SECONDS,
    ) {
        Ok(value) => value,
        Err(error) => return signing_failure(actor.correlation_id().as_str(), error),
    };

    Response::from_json(&DeviceGenerationUploadCapabilityResponse {
        method: "PUT",
        url: capability.url(),
        headers: capability
            .headers()
            .iter()
            .map(|(name, value)| DeviceGenerationUploadHeader {
                name: name.as_str(),
                value: value.as_str(),
            })
            .collect(),
        expires_seconds: capability.expires_seconds(),
    })
}

fn server_now() -> UnixMillis {
    UnixMillis::new(Date::now().as_millis())
}

fn server_signing_time() -> SigningTimeResult {
    let now: worker::js_sys::Date = Date::now().into();
    R2GenerationUploadSigningTime::parse(format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.get_utc_full_year(),
        now.get_utc_month() + 1,
        now.get_utc_date(),
        now.get_utc_hours(),
        now.get_utc_minutes(),
        now.get_utc_seconds(),
    ))
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceGenerationUploadCapabilityBody {
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
    coordinator_epoch: u64,
}

impl DeviceGenerationUploadCapabilityBody {
    fn into_descriptor(
        self,
        profile_id: ProfileId,
        base_generation_id: &GenerationId,
    ) -> core::result::Result<GenerationObjectDescriptor, ()> {
        let generation_id = GenerationId::parse(self.generation_id).map_err(|_| ())?;
        if &generation_id == base_generation_id {
            return Err(());
        }
        Ok(GenerationObjectDescriptor::new(
            profile_id,
            generation_id,
            self.object_key,
            self.metadata_digest,
            self.container_digest,
            self.container_bytes,
        ))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceGenerationUploadCapabilityResponse<'a> {
    method: &'static str,
    url: &'a str,
    headers: Vec<DeviceGenerationUploadHeader<'a>>,
    expires_seconds: u32,
}

#[derive(Serialize)]
struct DeviceGenerationUploadHeader<'a> {
    name: &'a str,
    value: &'a str,
}

fn device_port_failure(correlation_id: &str, class: DeviceJobPortErrorClass) -> Result<Response> {
    match class {
        DeviceJobPortErrorClass::AuthenticationFailed => forbidden(correlation_id),
        DeviceJobPortErrorClass::IntegrityFailure => integrity_failure(correlation_id),
        DeviceJobPortErrorClass::DependencyUnavailable => dependency(correlation_id),
    }
}

fn generation_precondition_failure(
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

fn signing_failure(
    correlation_id: &str,
    error: R2GenerationUploadCapabilityError,
) -> Result<Response> {
    match error {
        R2GenerationUploadCapabilityError::InvalidDescriptor
        | R2GenerationUploadCapabilityError::InvalidDigest => invalid_request(correlation_id),
        R2GenerationUploadCapabilityError::InvalidAccountId
        | R2GenerationUploadCapabilityError::InvalidBucketName
        | R2GenerationUploadCapabilityError::InvalidCredentials
        | R2GenerationUploadCapabilityError::InvalidSigningTime
        | R2GenerationUploadCapabilityError::InvalidExpiry => integrity_failure(correlation_id),
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
    use super::DeviceGenerationUploadCapabilityBody;

    #[test]
    fn transport_is_metadata_only_and_rejects_client_authority_fields() {
        let base = r#"{
            "profileId":"profile_upload_route_01",
            "baseGenerationId":"generation_upload_route_base_01",
            "claimId":"devclaim_upload_route_01",
            "fence":1,
            "generationId":"generation_upload_route_candidate_01",
            "objectKey":"tenants/tenant_upload_route_01/profiles/profile_upload_route_01/generations/generation_upload_route_candidate_01.bpgc",
            "metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "containerBytes":4096,
            "coordinatorSessionId":"session_upload_route_01",
            "coordinatorEpoch":4
        }"#;
        assert!(serde_json::from_str::<DeviceGenerationUploadCapabilityBody>(base).is_ok());
        for forbidden in [
            "tenantId",
            "deviceId",
            "observedAtMs",
            "clientClockMs",
            "expectedJobVersion",
            "expectedProfileVersion",
            "coordinatorVersion",
            "coordinatorSequence",
            "coordinatorFencingToken",
            "ciphertext",
            "container",
            "uploadBytes",
        ] {
            let tampered = base.replacen('}', &format!(r#", "{forbidden}": 1}}"#), 1);
            assert!(
                serde_json::from_str::<DeviceGenerationUploadCapabilityBody>(&tampered).is_err(),
                "forbidden upload capability field unexpectedly accepted: {forbidden}"
            );
        }
    }
}
