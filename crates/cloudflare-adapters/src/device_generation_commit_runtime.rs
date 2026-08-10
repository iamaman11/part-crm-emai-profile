use application_ports::device_generation_commit::{
    CoordinatorGenerationCommitWitness, DeviceGenerationCommitError,
    DeviceGenerationCommitErrorClass, DeviceGenerationCommitOutcome, DeviceGenerationCommitPort,
    DeviceGenerationCommitRequest,
};
use application_ports::generation_objects::GenerationObjectDescriptor;
use device_domain::{DeviceClaimId, DeviceJobId};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, CorrelationId, DeviceId, FencingToken, GenerationId,
    ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::coordinator_object_name;
use sha2::{Digest, Sha256};
use worker::wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, Request, RequestInit};

pub const DEVICE_GENERATION_COMMIT_PATH: &str = "/generation-commit";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceGenerationCommitInternalRequest {
    tenant_id: String,
    actor_id: String,
    correlation_id: String,
    job_id: String,
    claim_id: String,
    expected_job_version: u64,
    claim_fence: u64,
    device_id: String,
    profile_id: String,
    base_generation_id: String,
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

impl DeviceGenerationCommitInternalRequest {
    #[must_use]
    pub fn from_domain(actor: &ActorContext, request: &DeviceGenerationCommitRequest) -> Self {
        let object = request.object();
        Self {
            tenant_id: actor.tenant_scope().tenant_id().as_str().to_owned(),
            actor_id: actor.actor_id().as_str().to_owned(),
            correlation_id: actor.correlation_id().as_str().to_owned(),
            job_id: request.job_id().as_str().to_owned(),
            claim_id: request.claim_id().as_str().to_owned(),
            expected_job_version: request.expected_job_version().value(),
            claim_fence: request.claim_fence(),
            device_id: request.device_id().as_str().to_owned(),
            profile_id: request.profile_id().as_str().to_owned(),
            base_generation_id: request.base_generation_id().as_str().to_owned(),
            generation_id: object.generation_id().as_str().to_owned(),
            object_key: object.object_key().to_owned(),
            metadata_digest: object.metadata_digest().to_owned(),
            container_digest: object.container_digest().to_owned(),
            container_bytes: object.container_bytes(),
            expected_profile_version: request.expected_profile_version().value(),
            coordinator_session_id: request.coordinator().session_id().as_str().to_owned(),
            coordinator_fencing_token: request.coordinator().fencing_token().as_str().to_owned(),
            coordinator_epoch: request.coordinator().epoch(),
            coordinator_version: request.coordinator().coordinator_version(),
            coordinator_sequence: request.coordinator().coordinator_sequence(),
        }
    }

    pub fn into_domain(
        self,
        observed_at: UnixMillis,
    ) -> Result<(ActorContext, DeviceGenerationCommitRequest), DeviceGenerationCommitError> {
        let tenant_id = TenantId::parse(self.tenant_id).map_err(|_| integrity_failure())?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id),
            ActorId::parse(self.actor_id).map_err(|_| integrity_failure())?,
            CorrelationId::parse(self.correlation_id).map_err(|_| integrity_failure())?,
        );
        let profile_id = ProfileId::parse(self.profile_id).map_err(|_| integrity_failure())?;
        let request = DeviceGenerationCommitRequest::new(
            DeviceJobId::parse(self.job_id).map_err(|_| integrity_failure())?,
            DeviceClaimId::parse(self.claim_id).map_err(|_| integrity_failure())?,
            AggregateVersion::new(self.expected_job_version).map_err(|_| integrity_failure())?,
            self.claim_fence,
            DeviceId::parse(self.device_id).map_err(|_| integrity_failure())?,
            profile_id.clone(),
            GenerationId::parse(self.base_generation_id).map_err(|_| integrity_failure())?,
            GenerationObjectDescriptor::new(
                profile_id,
                GenerationId::parse(self.generation_id).map_err(|_| integrity_failure())?,
                self.object_key,
                self.metadata_digest,
                self.container_digest,
                self.container_bytes,
            ),
            AggregateVersion::new(self.expected_profile_version)
                .map_err(|_| integrity_failure())?,
            CoordinatorGenerationCommitWitness::new(
                SessionId::parse(self.coordinator_session_id).map_err(|_| integrity_failure())?,
                FencingToken::parse(self.coordinator_fencing_token)
                    .map_err(|_| integrity_failure())?,
                self.coordinator_epoch,
                self.coordinator_version,
                self.coordinator_sequence,
            ),
            observed_at,
        );
        Ok((actor, request))
    }

    #[must_use]
    pub fn authority_digest(&self) -> String {
        let mut hasher = Sha256::new();
        for value in [
            self.tenant_id.as_bytes(),
            self.actor_id.as_bytes(),
            self.job_id.as_bytes(),
            self.claim_id.as_bytes(),
            self.device_id.as_bytes(),
            self.profile_id.as_bytes(),
            self.base_generation_id.as_bytes(),
            self.generation_id.as_bytes(),
            self.object_key.as_bytes(),
            self.metadata_digest.as_bytes(),
            self.container_digest.as_bytes(),
            self.coordinator_session_id.as_bytes(),
            self.coordinator_fencing_token.as_bytes(),
        ] {
            hash_field(&mut hasher, value);
        }
        for value in [
            self.expected_job_version,
            self.claim_fence,
            self.container_bytes,
            self.expected_profile_version,
            self.coordinator_epoch,
            self.coordinator_version,
            self.coordinator_sequence,
        ] {
            hash_field(&mut hasher, &value.to_be_bytes());
        }
        hex_digest(hasher.finalize().into())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceGenerationCommitInternalOutcome {
    Activated,
    AlreadyActive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceGenerationCommitInternalErrorClass {
    StaleAuthority,
    VersionConflict,
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceGenerationCommitInternalResponse {
    pub outcome: DeviceGenerationCommitInternalOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceGenerationCommitInternalErrorResponse {
    pub class: DeviceGenerationCommitInternalErrorClass,
}

pub struct CloudflareDeviceGenerationCommitPort<'a> {
    env: &'a Env,
    coordinator_binding: &'a str,
}

impl<'a> CloudflareDeviceGenerationCommitPort<'a> {
    #[must_use]
    pub const fn new(env: &'a Env, coordinator_binding: &'a str) -> Self {
        Self {
            env,
            coordinator_binding,
        }
    }
}

impl DeviceGenerationCommitPort for CloudflareDeviceGenerationCommitPort<'_> {
    async fn commit_device_generation(
        &self,
        actor: &ActorContext,
        request: &DeviceGenerationCommitRequest,
    ) -> Result<DeviceGenerationCommitOutcome, DeviceGenerationCommitError> {
        let namespace = self
            .env
            .durable_object(self.coordinator_binding)
            .map_err(|_| dependency_failure())?;
        let object_id = namespace
            .id_from_name(&coordinator_object_name(request.profile_id()))
            .map_err(|_| dependency_failure())?;
        let stub = object_id.get_stub().map_err(|_| dependency_failure())?;
        let internal = DeviceGenerationCommitInternalRequest::from_domain(actor, request);
        let request = internal_request(&internal)?;
        let mut response = stub
            .fetch_with_request(request)
            .await
            .map_err(|_| dependency_failure())?;

        if response.status_code() == 200 {
            return response
                .json::<DeviceGenerationCommitInternalResponse>()
                .await
                .map_err(|_| integrity_failure())
                .map(|body| match body.outcome {
                    DeviceGenerationCommitInternalOutcome::Activated => {
                        DeviceGenerationCommitOutcome::Activated
                    }
                    DeviceGenerationCommitInternalOutcome::AlreadyActive => {
                        DeviceGenerationCommitOutcome::AlreadyActive
                    }
                });
        }

        let status = response.status_code();
        let class = response
            .json::<DeviceGenerationCommitInternalErrorResponse>()
            .await
            .ok()
            .map(|body| body.class);
        Err(match (status, class) {
            (409, Some(DeviceGenerationCommitInternalErrorClass::StaleAuthority)) => {
                stale_authority()
            }
            (409, Some(DeviceGenerationCommitInternalErrorClass::VersionConflict)) => {
                version_conflict()
            }
            (400 | 500, Some(DeviceGenerationCommitInternalErrorClass::IntegrityFailure)) => {
                integrity_failure()
            }
            (503, Some(DeviceGenerationCommitInternalErrorClass::DependencyUnavailable)) => {
                dependency_failure()
            }
            _ => dependency_failure(),
        })
    }
}

fn internal_request(
    body: &DeviceGenerationCommitInternalRequest,
) -> Result<Request, DeviceGenerationCommitError> {
    let payload = serde_json::to_string(body).map_err(|_| integrity_failure())?;
    let headers = Headers::new();
    headers
        .set("content-type", "application/json")
        .map_err(|_| dependency_failure())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&payload)));
    Request::new_with_init(
        &format!("https://profile-coordinator.internal{DEVICE_GENERATION_COMMIT_PATH}"),
        &init,
    )
    .map_err(|_| dependency_failure())
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

fn hex_digest(bytes: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

const fn stale_authority() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::StaleAuthority)
}

const fn version_conflict() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::VersionConflict)
}

const fn integrity_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::IntegrityFailure)
}

const fn dependency_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::DeviceGenerationCommitInternalRequest;
    use application_ports::device_generation_commit::{
        CoordinatorGenerationCommitWitness, DeviceGenerationCommitRequest,
    };
    use application_ports::generation_objects::GenerationObjectDescriptor;
    use device_domain::{DeviceClaimId, DeviceJobId};
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, CorrelationId, DeviceId, FencingToken,
        GenerationId, ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
    };

    fn fixture() -> Result<(ActorContext, DeviceGenerationCommitRequest), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_runtime_commit_01")?;
        let profile_id = ProfileId::parse("profile_runtime_commit_01")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            ActorId::parse("actor_runtime_commit_01")?,
            CorrelationId::parse("corr_runtime_commit_01")?,
        );
        let request = DeviceGenerationCommitRequest::new(
            DeviceJobId::parse("devjob_runtime_commit_01")?,
            DeviceClaimId::parse("devclaim_runtime_commit_01")?,
            AggregateVersion::new(4)?,
            2,
            DeviceId::parse("device_runtime_commit_01")?,
            profile_id.clone(),
            GenerationId::parse("generation_runtime_base_01")?,
            GenerationObjectDescriptor::new(
                profile_id,
                GenerationId::parse("generation_runtime_candidate_01")?,
                format!(
                    "tenants/{}/profiles/{}/generations/{}.bpgc",
                    tenant_id.as_str(),
                    "profile_runtime_commit_01",
                    "generation_runtime_candidate_01"
                ),
                "a".repeat(64),
                "b".repeat(64),
                4096,
            ),
            AggregateVersion::new(7)?,
            CoordinatorGenerationCommitWitness::new(
                SessionId::parse("session_runtime_commit_01")?,
                FencingToken::parse("fence_runtime_commit_01")?,
                3,
                11,
                10,
            ),
            UnixMillis::new(100),
        );
        Ok((actor, request))
    }

    #[test]
    fn internal_shape_round_trips_without_client_clock_authority()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, request) = fixture()?;
        let internal = DeviceGenerationCommitInternalRequest::from_domain(&actor, &request);
        let digest = internal.authority_digest();
        let serialized = serde_json::to_string(&internal)?;
        assert!(!serialized.contains("observed_at"));
        assert!(!serialized.contains("observedAt"));
        let parsed: DeviceGenerationCommitInternalRequest = serde_json::from_str(&serialized)?;
        assert_eq!(parsed.authority_digest(), digest);
        let (round_actor, round_request) = parsed.into_domain(UnixMillis::new(200))?;
        assert_eq!(round_actor, actor);
        assert_eq!(round_request.observed_at(), UnixMillis::new(200));
        assert_eq!(round_request.object(), request.object());
        assert_eq!(round_request.coordinator(), request.coordinator());
        Ok(())
    }

    #[test]
    fn authority_digest_changes_with_fencing_token() -> Result<(), Box<dyn std::error::Error>> {
        let (actor, request) = fixture()?;
        let first = DeviceGenerationCommitInternalRequest::from_domain(&actor, &request);
        let mut serialized = serde_json::to_value(&first)?;
        serialized["coordinator_fencing_token"] = serde_json::Value::String(
            "fence_runtime_commit_other".to_owned(),
        );
        let second: DeviceGenerationCommitInternalRequest = serde_json::from_value(serialized)?;
        assert_ne!(first.authority_digest(), second.authority_digest());
        Ok(())
    }
}
