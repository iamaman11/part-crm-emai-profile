use crate::access_session::problem;
use crate::composition::{
    device_execution_preconditions, device_job_authorization, generation_object_verifier,
    generation_upload_capability_signer, profile_generation_successor_commit,
};
use application_ports::DeviceJobPortErrorClass;
use application_ports::device_jobs::{DeviceJobAuthorizationPort, DeviceJobCapability};
use application_ports::generation_objects::{
    GenerationObjectDescriptor, GenerationObjectDescriptorVerifyPort,
};
use application_ports::generations::GenerationPortErrorClass;
use application_ports::profile_generation_successor::{
    ProfileGenerationCommitWitness, ProfileGenerationSuccessorCommitErrorClass,
    ProfileGenerationSuccessorCommitOutcome, ProfileGenerationSuccessorCommitPort,
    ProfileGenerationSuccessorCommitRequest, ProfileGenerationSuccessorVersionPort,
    ProfileGenerationWriterAuthorityPort, ProfileGenerationWriterAuthorityRequest,
};
use cloudflare_adapters::r2_generation_upload_capability::{
    R2GenerationUploadCapabilityError, R2GenerationUploadSigningTime,
};
use control_plane_contract::profile_generation_api::{
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
use device_domain::DeviceJobTarget;
use profile_platform_primitives::{
    ActorContext, DeviceId, FencingToken, GenerationId, ProfileId, SessionId, UnixMillis,
};
use worker::{Date, Env, Method, Request, Response, Result};

const CAPABILITY_EXPIRES_SECONDS: u32 = 300;
type SigningTimeResult =
    core::result::Result<R2GenerationUploadSigningTime, R2GenerationUploadCapabilityError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BridgeSuccessorOperation {
    UploadCapability,
    Commit,
}

#[must_use]
pub(crate) fn operation(path: &str, method: Method) -> Option<BridgeSuccessorOperation> {
    if method != Method::Post {
        return None;
    }
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    match segments.as_slice() {
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "upload-capability",
        ] => Some(BridgeSuccessorOperation::UploadCapability),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "commit",
        ] => Some(BridgeSuccessorOperation::Commit),
        _ => None,
    }
}

pub(crate) async fn dispatch_authorized(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    operation: BridgeSuccessorOperation,
) -> Result<Response> {
    let body = match request
        .json::<BridgeProfileGenerationSuccessorRequest>()
        .await
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let base_generation_id = match GenerationId::parse(body.base_generation_id().to_owned()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let generation_id = match GenerationId::parse(body.generation_id().to_owned()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if generation_id == base_generation_id
        || body.container_bytes() == 0
        || body.coordinator_epoch() == 0
    {
        return invalid_request(actor.correlation_id().as_str());
    }
    let session_id = match SessionId::parse(body.coordinator_session_id().to_owned()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let fencing_token = match FencingToken::parse(body.coordinator_fencing_token().to_owned()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if !descriptor_shape_is_canonical(
        actor,
        profile_id,
        &generation_id,
        body.object_key(),
        body.metadata_digest(),
        body.container_digest(),
    ) {
        return invalid_request(actor.correlation_id().as_str());
    }

    let descriptor = GenerationObjectDescriptor::new(
        profile_id.clone(),
        generation_id.clone(),
        body.object_key(),
        body.metadata_digest(),
        body.container_digest(),
        body.container_bytes(),
    );
    let target = DeviceJobTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        device_id.clone(),
        profile_id.clone(),
        base_generation_id.clone(),
    );

    match device_authorized(env, actor, &target).await {
        Ok(true) => {}
        Ok(false) | Err(DeviceJobPortErrorClass::AuthenticationFailed) => {
            return forbidden(actor.correlation_id().as_str());
        }
        Err(DeviceJobPortErrorClass::IntegrityFailure) => {
            return integrity_failure(actor.correlation_id().as_str());
        }
        Err(DeviceJobPortErrorClass::DependencyUnavailable) => {
            return dependency(actor.correlation_id().as_str());
        }
    }

    let preconditions = device_execution_preconditions(env)?;
    let expected_profile_version = match preconditions
        .load_successor_expected_profile_version(
            actor,
            profile_id,
            &base_generation_id,
            &generation_id,
        )
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return version_conflict(actor.correlation_id().as_str()),
        Err(error) => {
            return successor_failure(actor.correlation_id().as_str(), error.class());
        }
    };

    let verifier = generation_object_verifier(env)?;
    match operation {
        BridgeSuccessorOperation::UploadCapability => {
            let successor = profile_generation_successor_commit(env);
            if let Err(error) = successor
                .prove_profile_generation_writer_authority(
                    actor,
                    &ProfileGenerationWriterAuthorityRequest::new(
                        device_id.clone(),
                        profile_id.clone(),
                        session_id,
                        fencing_token,
                        body.coordinator_epoch(),
                    ),
                )
                .await
            {
                return successor_failure(actor.correlation_id().as_str(), error.class());
            }

            match verifier
                .verify_generation_object_descriptor_exact(actor.tenant_scope(), &descriptor)
                .await
            {
                Ok(true) => {
                    return machine_json(&BridgeGenerationUploadCapabilityResponse::verified());
                }
                Ok(false) => {}
                Err(error) => {
                    return object_verify_failure(actor.correlation_id().as_str(), error.class());
                }
            }

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
            machine_json(&BridgeGenerationUploadCapabilityResponse::upload_required(
                capability.url(),
                capability.headers(),
                capability.expires_seconds(),
            ))
        }
        BridgeSuccessorOperation::Commit => {
            match verifier
                .verify_generation_object_descriptor_exact(actor.tenant_scope(), &descriptor)
                .await
            {
                Ok(true) => {}
                Ok(false) => return verification_conflict(actor.correlation_id().as_str()),
                Err(error) => {
                    return object_verify_failure(actor.correlation_id().as_str(), error.class());
                }
            }

            // Authorization is re-read after exact object verification so revocation racing a save
            // cannot rely on the earlier upload-capability decision.
            match device_authorized(env, actor, &target).await {
                Ok(true) => {}
                Ok(false) | Err(DeviceJobPortErrorClass::AuthenticationFailed) => {
                    return forbidden(actor.correlation_id().as_str());
                }
                Err(DeviceJobPortErrorClass::IntegrityFailure) => {
                    return integrity_failure(actor.correlation_id().as_str());
                }
                Err(DeviceJobPortErrorClass::DependencyUnavailable) => {
                    return dependency(actor.correlation_id().as_str());
                }
            }

            let successor = profile_generation_successor_commit(env);
            let authority = match successor
                .prove_profile_generation_writer_authority(
                    actor,
                    &ProfileGenerationWriterAuthorityRequest::new(
                        device_id.clone(),
                        profile_id.clone(),
                        session_id.clone(),
                        fencing_token.clone(),
                        body.coordinator_epoch(),
                    ),
                )
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    return successor_failure(actor.correlation_id().as_str(), error.class());
                }
            };
            let commit = ProfileGenerationSuccessorCommitRequest::new(
                device_id.clone(),
                profile_id.clone(),
                base_generation_id,
                descriptor,
                expected_profile_version,
                ProfileGenerationCommitWitness::new(
                    session_id,
                    fencing_token,
                    body.coordinator_epoch(),
                    authority.coordinator_version(),
                    authority.coordinator_sequence(),
                ),
                server_now(),
            );
            let outcome = match successor
                .commit_profile_generation_successor(actor, &commit)
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    return successor_failure(actor.correlation_id().as_str(), error.class());
                }
            };
            machine_json(&BridgeGenerationSuccessorCommitResponse {
                outcome: match outcome {
                    ProfileGenerationSuccessorCommitOutcome::Activated => {
                        BridgeGenerationSuccessorCommitOutcomeDto::Activated
                    }
                    ProfileGenerationSuccessorCommitOutcome::AlreadyActive => {
                        BridgeGenerationSuccessorCommitOutcomeDto::AlreadyActive
                    }
                },
            })
        }
    }
}

async fn device_authorized(
    env: &Env,
    actor: &ActorContext,
    target: &DeviceJobTarget,
) -> core::result::Result<bool, DeviceJobPortErrorClass> {
    let authorization = device_job_authorization(env)
        .map_err(|_| DeviceJobPortErrorClass::DependencyUnavailable)?;
    authorization
        .is_device_job_authorized(actor, target, DeviceJobCapability::Complete)
        .await
        .map_err(|error| error.class())
}

fn descriptor_shape_is_canonical(
    actor: &ActorContext,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    object_key: &str,
    metadata_digest: &str,
    container_digest: &str,
) -> bool {
    object_key
        == format!(
            "tenants/{}/profiles/{}/generations/{}.bpgc",
            actor.tenant_scope().tenant_id().as_str(),
            profile_id.as_str(),
            generation_id.as_str(),
        )
        && lower_sha256(metadata_digest)
        && lower_sha256(container_digest)
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn machine_json<T: serde::Serialize>(value: &T) -> Result<Response> {
    let mut response = Response::from_json(value)?;
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    Ok(response)
}

fn successor_failure(
    correlation_id: &str,
    class: ProfileGenerationSuccessorCommitErrorClass,
) -> Result<Response> {
    match class {
        ProfileGenerationSuccessorCommitErrorClass::StaleAuthority => {
            stale_authority(correlation_id)
        }
        ProfileGenerationSuccessorCommitErrorClass::VersionConflict => {
            version_conflict(correlation_id)
        }
        ProfileGenerationSuccessorCommitErrorClass::IntegrityFailure => {
            integrity_failure(correlation_id)
        }
        ProfileGenerationSuccessorCommitErrorClass::DependencyUnavailable => {
            dependency(correlation_id)
        }
    }
}

fn object_verify_failure(
    correlation_id: &str,
    class: GenerationPortErrorClass,
) -> Result<Response> {
    match class {
        GenerationPortErrorClass::DependencyUnavailable => dependency(correlation_id),
        GenerationPortErrorClass::NotFound
        | GenerationPortErrorClass::VersionConflict
        | GenerationPortErrorClass::InvalidState
        | GenerationPortErrorClass::Conflict
        | GenerationPortErrorClass::IntegrityFailure
        | GenerationPortErrorClass::InternalFailure => integrity_failure(correlation_id),
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

fn verification_conflict(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        409,
        "generation_not_verified",
        "Generation Not Verified",
    )
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
    use super::{BridgeSuccessorOperation, descriptor_shape_is_canonical, operation};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, GenerationId, ProfileId, TenantId, TenantScope,
    };
    use worker::Method;

    #[test]
    fn only_exact_successor_posts_are_recognized() {
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/upload-capability",
                Method::Post,
            ),
            Some(BridgeSuccessorOperation::UploadCapability)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/commit",
                Method::Post,
            ),
            Some(BridgeSuccessorOperation::Commit)
        );
        for (method, path) in [
            (
                Method::Get,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/commit",
            ),
            (
                Method::Post,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/verify",
            ),
            (
                Method::Post,
                "/bridge/v2/tenants/tenant_01/profiles/profile_01/generation-successor/commit",
            ),
        ] {
            assert_eq!(operation(path, method), None);
        }
    }

    #[test]
    fn descriptor_transport_admission_is_exact_and_lowercase()
    -> Result<(), Box<dyn std::error::Error>> {
        let actor = ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_successor_ingress_01")?),
            ActorId::parse("actor_successor_ingress_01")?,
            CorrelationId::parse("corr_successor_ingress_01")?,
        );
        let profile_id = ProfileId::parse("profile_successor_ingress_01")?;
        let generation_id = GenerationId::parse("generation_successor_ingress_01")?;
        let key = format!(
            "tenants/{}/profiles/{}/generations/{}.bpgc",
            actor.tenant_scope().tenant_id().as_str(),
            profile_id.as_str(),
            generation_id.as_str(),
        );
        assert!(descriptor_shape_is_canonical(
            &actor,
            &profile_id,
            &generation_id,
            &key,
            &"a".repeat(64),
            &"b".repeat(64),
        ));
        assert!(!descriptor_shape_is_canonical(
            &actor,
            &profile_id,
            &generation_id,
            &key,
            &"A".repeat(64),
            &"b".repeat(64),
        ));
        Ok(())
    }
}
