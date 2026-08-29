use crate::profile_generation_successor_runtime::{
    PROFILE_GENERATION_WRITER_AUTHORITY_PATH, ProfileGenerationSuccessorInternalErrorClass,
    ProfileGenerationSuccessorInternalErrorResponse, ProfileGenerationWriterAuthorityInternalRequest,
    ProfileGenerationWriterAuthorityInternalResponse,
};
use application_ports::profile_generation_successor::{
    ProfileGenerationSuccessorCommitError, ProfileGenerationSuccessorCommitErrorClass,
    ProfileGenerationWriterAuthority, ProfileGenerationWriterAuthorityPort,
    ProfileGenerationWriterAuthorityRequest,
};
use profile_platform_primitives::ActorContext;
use serde::Serialize;
use session_domain::coordinator::coordinator_object_name;
use worker::wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, Request, RequestInit};

pub struct CloudflareProfileGenerationWriterAuthorityPort<'a> {
    env: &'a Env,
    coordinator_binding: &'a str,
}

impl<'a> CloudflareProfileGenerationWriterAuthorityPort<'a> {
    #[must_use]
    pub const fn new(env: &'a Env, coordinator_binding: &'a str) -> Self {
        Self {
            env,
            coordinator_binding,
        }
    }
}

impl ProfileGenerationWriterAuthorityPort for CloudflareProfileGenerationWriterAuthorityPort<'_> {
    async fn prove_profile_generation_writer_authority(
        &self,
        actor: &ActorContext,
        request: &ProfileGenerationWriterAuthorityRequest,
    ) -> Result<ProfileGenerationWriterAuthority, ProfileGenerationSuccessorCommitError> {
        let namespace = self
            .env
            .durable_object(self.coordinator_binding)
            .map_err(|_| dependency_failure())?;
        let object_id = namespace
            .id_from_name(&coordinator_object_name(request.profile_id()))
            .map_err(|_| dependency_failure())?;
        let stub = object_id.get_stub().map_err(|_| dependency_failure())?;
        let internal = ProfileGenerationWriterAuthorityInternalRequest::from_domain(actor, request);
        let request = internal_request(PROFILE_GENERATION_WRITER_AUTHORITY_PATH, &internal)?;
        let mut response = stub
            .fetch_with_request(request)
            .await
            .map_err(|_| dependency_failure())?;

        if response.status_code() == 200 {
            let body = response
                .json::<ProfileGenerationWriterAuthorityInternalResponse>()
                .await
                .map_err(|_| integrity_failure())?;
            if body.coordinator_version == 0 || body.coordinator_sequence == 0 {
                return Err(integrity_failure());
            }
            return Ok(ProfileGenerationWriterAuthority::new(
                body.coordinator_version,
                body.coordinator_sequence,
            ));
        }

        let status = response.status_code();
        let body = response
            .json::<ProfileGenerationSuccessorInternalErrorResponse>()
            .await
            .map_err(|_| dependency_failure())?;
        Err(match (status, body.class) {
            (409, ProfileGenerationSuccessorInternalErrorClass::StaleAuthority) => {
                stale_authority()
            }
            (409, ProfileGenerationSuccessorInternalErrorClass::VersionConflict) => {
                version_conflict()
            }
            (400 | 500, ProfileGenerationSuccessorInternalErrorClass::IntegrityFailure) => {
                integrity_failure()
            }
            (503, ProfileGenerationSuccessorInternalErrorClass::DependencyUnavailable) => {
                dependency_failure()
            }
            _ => dependency_failure(),
        })
    }
}

fn internal_request<T: Serialize>(
    path: &str,
    body: &T,
) -> Result<Request, ProfileGenerationSuccessorCommitError> {
    let payload = serde_json::to_string(body).map_err(|_| integrity_failure())?;
    let headers = Headers::new();
    headers
        .set("content-type", "application/json")
        .map_err(|_| dependency_failure())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&payload)));
    Request::new_with_init(&format!("https://profile-coordinator.internal{path}"), &init)
        .map_err(|_| dependency_failure())
}

const fn stale_authority() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::StaleAuthority,
    )
}

const fn version_conflict() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::VersionConflict,
    )
}

const fn integrity_failure() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::IntegrityFailure,
    )
}

const fn dependency_failure() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::DependencyUnavailable,
    )
}
