mod initialization;

use crate::access_session::problem;
use crate::composition::{
    device_execution_preconditions, device_job_authorization,
    generation_download_capability_signer, generation_object_verifier, generation_root_keyring,
    generation_upload_capability_signer, profile_generation_successor_commit,
    profile_generation_successor_observer,
};
use application_ports::device_jobs::{DeviceJobAuthorizationPort, DeviceJobCapability};
use application_ports::generation_objects::{
    ActiveGenerationObjectReferencePort, GenerationObjectDescriptor,
    GenerationObjectDescriptorReadPort, GenerationObjectDescriptorVerifyPort,
    VerifiedPredecessorObjectReferencePort,
};
use application_ports::generations::GenerationPortErrorClass;
use application_ports::profile_generation_successor::{
    ProfileGenerationCommitWitness, ProfileGenerationSuccessorCommitErrorClass,
    ProfileGenerationSuccessorCommitOutcome, ProfileGenerationSuccessorCommitPort,
    ProfileGenerationSuccessorCommitRequest, ProfileGenerationSuccessorObservationPort,
    ProfileGenerationSuccessorRetryDecisionPort, ProfileGenerationSuccessorVersionPort,
    ProfileGenerationWriterAuthorityPort, ProfileGenerationWriterAuthorityRequest,
};
use application_ports::{
    DeviceExecutionBlocker, DeviceExecutionPreconditionPort, DeviceExecutionReadiness,
    DeviceJobPortErrorClass,
};
use cloudflare_adapters::r2_generation_download_capability::{
    R2GenerationDownloadCapabilityError, R2GenerationDownloadSigningTime,
};
use cloudflare_adapters::r2_generation_upload_capability::{
    R2GenerationUploadCapabilityError, R2GenerationUploadSigningTime,
};
use control_plane_contract::generation_key_api::{
    BridgeGenerationSealingMaterialRequest, BridgeGenerationSealingMaterialResponse,
    GENERATION_SEALING_CHUNK_BYTES,
};
use control_plane_contract::generation_reopen_api::{
    BridgeGenerationDownloadCapabilityRequest, BridgeGenerationDownloadCapabilityResponse,
    BridgeGenerationOpeningMaterialRequest, BridgeGenerationOpeningMaterialResponse,
    GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
};
use control_plane_contract::profile_generation_api::{
    BridgeGenerationLocalUnsyncedRetryDecisionRequest, BridgeGenerationSuccessorCommitOutcomeDto,
    BridgeGenerationSuccessorCommitResponse, BridgeGenerationSuccessorObserveRequest,
    BridgeGenerationSuccessorObserveResponse, BridgeGenerationSuccessorRetryDecisionRequest,
    BridgeGenerationSuccessorRetryDecisionResponse, BridgeGenerationSuccessorRetryVerdict,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
use device_domain::DeviceJobTarget;
use encrypted_generation_domain::{
    InspectedGenerationMetadataPrelude, MAX_GENERATION_METADATA_PRELUDE_BYTES,
    canonical_generation_object_key, inspect_generation_metadata_prelude,
    MAX_STREAM_CONTAINER_BYTES,
};
use profile_platform_primitives::{
    ActorContext, DeviceId, FencingToken, GenerationId, ProfileId, SessionId, UnixMillis,
};
use worker::{Date, Env, Method, Request, Response, Result};

const UPLOAD_CAPABILITY_EXPIRES_SECONDS: u32 = 300;
type UploadSigningTimeResult =
    core::result::Result<R2GenerationUploadSigningTime, R2GenerationUploadCapabilityError>;
type DownloadSigningTimeResult =
    core::result::Result<R2GenerationDownloadSigningTime, R2GenerationDownloadCapabilityError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BridgeGenerationMachineOperation {
    Initialize,
    SealingMaterial,
    UploadCapability,
    Commit,
    Observe,
    RetryDecision,
    LocalUnsyncedRetryDecision,
    DownloadCapability,
    OpeningMaterial,
    PredecessorDownloadCapability,
    PredecessorOpeningMaterial,
}

#[must_use]
pub(crate) fn operation(path: &str, method: Method) -> Option<BridgeGenerationMachineOperation> {
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
            "generation-initialization",
        ] => Some(BridgeGenerationMachineOperation::Initialize),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "sealing-material",
        ] => Some(BridgeGenerationMachineOperation::SealingMaterial),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "upload-capability",
        ] => Some(BridgeGenerationMachineOperation::UploadCapability),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "commit",
        ] => Some(BridgeGenerationMachineOperation::Commit),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "observe",
        ] => Some(BridgeGenerationMachineOperation::Observe),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "retry-decision",
        ] => Some(BridgeGenerationMachineOperation::RetryDecision),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-successor",
            "local-unsynced",
            "retry-decision",
        ] => Some(BridgeGenerationMachineOperation::LocalUnsyncedRetryDecision),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-reopen",
            "download-capability",
        ] => Some(BridgeGenerationMachineOperation::DownloadCapability),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-reopen",
            "opening-material",
        ] => Some(BridgeGenerationMachineOperation::OpeningMaterial),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-working-candidate",
            "download-capability",
        ] => Some(BridgeGenerationMachineOperation::PredecessorDownloadCapability),
        [
            "bridge",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generation-working-candidate",
            "opening-material",
        ] => Some(BridgeGenerationMachineOperation::PredecessorOpeningMaterial),
        _ => None,
    }
}

pub(crate) async fn dispatch_authorized(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    operation: BridgeGenerationMachineOperation,
    role: identity_access_domain::MembershipRole,
) -> Result<Response> {
    if operation == BridgeGenerationMachineOperation::Initialize {
        return initialization::dispatch(request, env, profile_id, actor, device_id, role).await;
    }
    if operation == BridgeGenerationMachineOperation::SealingMaterial {
        return dispatch_sealing_material(request, env, profile_id, actor, device_id).await;
    }
    if matches!(
        operation,
        BridgeGenerationMachineOperation::DownloadCapability
            | BridgeGenerationMachineOperation::PredecessorDownloadCapability
    ) {
        return dispatch_download_capability(
            request,
            env,
            profile_id,
            actor,
            device_id,
            operation == BridgeGenerationMachineOperation::PredecessorDownloadCapability,
        )
        .await;
    }
    if matches!(
        operation,
        BridgeGenerationMachineOperation::OpeningMaterial
            | BridgeGenerationMachineOperation::PredecessorOpeningMaterial
    ) {
        return dispatch_opening_material(
            request,
            env,
            profile_id,
            actor,
            device_id,
            operation == BridgeGenerationMachineOperation::PredecessorOpeningMaterial,
        )
        .await;
    }
    if operation == BridgeGenerationMachineOperation::Observe {
        return dispatch_successor_observation(request, env, profile_id, actor, device_id).await;
    }
    if matches!(
        operation,
        BridgeGenerationMachineOperation::RetryDecision
            | BridgeGenerationMachineOperation::LocalUnsyncedRetryDecision
    ) {
        return dispatch_successor_retry_decision(
            request,
            env,
            profile_id,
            actor,
            device_id,
            operation == BridgeGenerationMachineOperation::LocalUnsyncedRetryDecision,
        )
        .await;
    }

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
        || !valid_stream_container_bytes(body.container_bytes())
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
        BridgeGenerationMachineOperation::Initialize
        | BridgeGenerationMachineOperation::SealingMaterial
        | BridgeGenerationMachineOperation::DownloadCapability
        | BridgeGenerationMachineOperation::OpeningMaterial
        | BridgeGenerationMachineOperation::PredecessorDownloadCapability
        | BridgeGenerationMachineOperation::PredecessorOpeningMaterial
        | BridgeGenerationMachineOperation::Observe
        | BridgeGenerationMachineOperation::RetryDecision
        | BridgeGenerationMachineOperation::LocalUnsyncedRetryDecision => {
            unreachable!("handled before successor DTO")
        }
        BridgeGenerationMachineOperation::UploadCapability => {
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
            let signing_time = match server_upload_signing_time() {
                Ok(value) => value,
                Err(_) => return integrity_failure(actor.correlation_id().as_str()),
            };
            let capability = match signer.sign_put(
                actor.tenant_scope(),
                &descriptor,
                &signing_time,
                UPLOAD_CAPABILITY_EXPIRES_SECONDS,
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
        BridgeGenerationMachineOperation::Commit => {
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

/// Observe already-applied D1 commit without replaying the original
/// (potentially expired) coordinator lease/fence or issuing any cloud write.
/// The caller must prove a *fresh* machine session and current Profile Operate
/// ACL through the outer ingress, and must still retain current device/base
/// authorization. "false" is not permission to re-upload or recreate.
async fn dispatch_successor_observation(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
) -> Result<Response> {
    let body = match request
        .json::<BridgeGenerationSuccessorObserveRequest>()
        .await
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let Ok(base) = GenerationId::parse(body.base_generation_id) else {
        return invalid_request(actor.correlation_id().as_str());
    };
    let Ok(successor) = GenerationId::parse(body.generation_id) else {
        return invalid_request(actor.correlation_id().as_str());
    };
    if base == successor
        || body.container_bytes == 0
        || !descriptor_shape_is_canonical(
            actor,
            profile_id,
            &successor,
            &body.object_key,
            &body.metadata_digest,
            &body.container_digest,
        )
    {
        return invalid_request(actor.correlation_id().as_str());
    }
    let target = DeviceJobTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        device_id.clone(),
        profile_id.clone(),
        base.clone(),
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
    let object = GenerationObjectDescriptor::new(
        profile_id.clone(),
        successor,
        body.object_key,
        body.metadata_digest,
        body.container_digest,
        body.container_bytes,
    );
    let observer = profile_generation_successor_observer(env)?;
    let active_exact = match observer
        .exact_successor_is_committed_active(actor, device_id, profile_id, &base, &object)
        .await
    {
        Ok(value) => value,
        Err(error) => return successor_failure(actor.correlation_id().as_str(), error.class()),
    };
    let mut response =
        Response::from_json(&BridgeGenerationSuccessorObserveResponse { active_exact })?;
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    Ok(response)
}

/// Fresh coordinator + current ACL precede a single D1 nonapplication read.
/// No R2 PUT, commit, signed URL, or old-fence replay is performed here.
async fn dispatch_successor_retry_decision(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    local_unsynced: bool,
) -> Result<Response> {
    // Two STRICT DTOs share the same current ACL, exclusive fresh coordinator
    // witness and the same D1 lifecycle owner; no write is authorized here.
    let (base_raw, attempt, session_raw, fence_raw, epoch) = if local_unsynced {
        let body = match request
            .json::<BridgeGenerationLocalUnsyncedRetryDecisionRequest>()
            .await
        {
            Ok(value) => value,
            Err(_) => return invalid_request(actor.correlation_id().as_str()),
        };
        (
            body.base_generation_id,
            None,
            body.coordinator_session_id,
            body.coordinator_fencing_token,
            body.coordinator_epoch,
        )
    } else {
        let body = match request
            .json::<BridgeGenerationSuccessorRetryDecisionRequest>()
            .await
        {
            Ok(value) => value,
            Err(_) => return invalid_request(actor.correlation_id().as_str()),
        };
        (
            body.attempt.base_generation_id.clone(),
            Some(body.attempt),
            body.coordinator_session_id,
            body.coordinator_fencing_token,
            body.coordinator_epoch,
        )
    };
    let Ok(base) = GenerationId::parse(base_raw) else {
        return invalid_request(actor.correlation_id().as_str());
    };
    let Some((session, fencing_token)) = reopen_witness(&session_raw, &fence_raw, epoch) else {
        return invalid_request(actor.correlation_id().as_str());
    };
    let object = if let Some(attempt) = attempt {
        let Ok(candidate) = GenerationId::parse(attempt.generation_id) else {
            return invalid_request(actor.correlation_id().as_str());
        };
        if base == candidate
            || attempt.container_bytes == 0
            || !descriptor_shape_is_canonical(
                actor,
                profile_id,
                &candidate,
                &attempt.object_key,
                &attempt.metadata_digest,
                &attempt.container_digest,
            )
        {
            return invalid_request(actor.correlation_id().as_str());
        }
        Some(GenerationObjectDescriptor::new(
            profile_id.clone(),
            candidate,
            attempt.object_key,
            attempt.metadata_digest,
            attempt.container_digest,
            attempt.container_bytes,
        ))
    } else {
        None
    };
    let target = DeviceJobTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        device_id.clone(),
        profile_id.clone(),
        base.clone(),
    );
    let preconditions = device_execution_preconditions(env)?;
    match preconditions
        .evaluate_device_execution(actor, &target)
        .await
    {
        Ok(DeviceExecutionReadiness::Ready) => {}
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::DeviceUnauthorized)) => {
            return forbidden(actor.correlation_id().as_str());
        }
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::GenerationInactive)) => {
            return version_conflict(actor.correlation_id().as_str());
        }
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::CertificationIncomplete)) => {
            return verification_conflict(actor.correlation_id().as_str());
        }
        Err(error) => match error.class() {
            DeviceJobPortErrorClass::AuthenticationFailed => {
                return forbidden(actor.correlation_id().as_str());
            }
            DeviceJobPortErrorClass::IntegrityFailure => {
                return integrity_failure(actor.correlation_id().as_str());
            }
            DeviceJobPortErrorClass::DependencyUnavailable => {
                return dependency(actor.correlation_id().as_str());
            }
        },
    }
    let authority = profile_generation_successor_commit(env);
    if let Err(error) = authority
        .prove_profile_generation_writer_authority(
            actor,
            &ProfileGenerationWriterAuthorityRequest::new(
                device_id.clone(),
                profile_id.clone(),
                session,
                fencing_token,
                epoch,
            ),
        )
        .await
    {
        return successor_failure(actor.correlation_id().as_str(), error.class());
    }
    let decision = profile_generation_successor_observer(env)?;
    let verdict = match object.as_ref() {
        Some(object) => {
            decision
                .exact_unapplied_successor_is_retryable(actor, profile_id, &base, object)
                .await
        }
        None => {
            decision
                .unsealed_local_unsynced_is_retryable(actor, profile_id, &base)
                .await
        }
    };
    let allowed = match verdict {
        Ok(value) => value,
        Err(error) => return successor_failure(actor.correlation_id().as_str(), error.class()),
    };
    let mut response = Response::from_json(&BridgeGenerationSuccessorRetryDecisionResponse {
        verdict: if allowed {
            BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable
        } else {
            BridgeGenerationSuccessorRetryVerdict::Blocked
        },
    })?;
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    Ok(response)
}

async fn dispatch_download_capability(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    historical: bool,
) -> Result<Response> {
    let body = match request
        .json::<BridgeGenerationDownloadCapabilityRequest>()
        .await
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let (session_id, fencing_token) = match reopen_witness(
        body.coordinator_session_id(),
        body.coordinator_fencing_token(),
        body.coordinator_epoch(),
    ) {
        Some(value) => value,
        None => return invalid_request(actor.correlation_id().as_str()),
    };

    let selected = if historical {
        historical_predecessor_reopen_descriptor(
            env,
            profile_id,
            actor,
            device_id,
            session_id,
            fencing_token,
            body.coordinator_epoch(),
        )
        .await
    } else {
        authoritative_reopen_descriptor(
            env,
            profile_id,
            actor,
            device_id,
            session_id,
            fencing_token,
            body.coordinator_epoch(),
        )
        .await
    };
    let (reference, descriptor) = match selected {
        Ok(value) => value,
        Err(response) => return response,
    };
    let signer = match generation_download_capability_signer(env) {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    let signing_time = match server_download_signing_time() {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    let capability = match signer.sign_get(
        actor.tenant_scope(),
        &descriptor,
        &signing_time,
        GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
    ) {
        Ok(value) => value,
        Err(error) => {
            return download_signing_failure(actor.correlation_id().as_str(), error);
        }
    };
    if descriptor.generation_id() != reference.generation_id() {
        return integrity_failure(actor.correlation_id().as_str());
    }
    machine_json(&BridgeGenerationDownloadCapabilityResponse::new(
        descriptor.generation_id().as_str(),
        descriptor.object_key(),
        descriptor.metadata_digest(),
        descriptor.container_digest(),
        descriptor.container_bytes(),
        capability.url(),
        capability.expires_seconds(),
    ))
}

async fn dispatch_opening_material(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    historical: bool,
) -> Result<Response> {
    let body = match request
        .json::<BridgeGenerationOpeningMaterialRequest>()
        .await
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let (session_id, fencing_token) = match reopen_witness(
        body.coordinator_session_id(),
        body.coordinator_fencing_token(),
        body.coordinator_epoch(),
    ) {
        Some(value) => value,
        None => return invalid_request(actor.correlation_id().as_str()),
    };
    let prelude = match decode_bounded_lower_hex(
        body.metadata_prelude_hex(),
        MAX_GENERATION_METADATA_PRELUDE_BYTES,
    ) {
        Some(value) => value,
        None => return invalid_request(actor.correlation_id().as_str()),
    };
    let inspected = match inspect_generation_metadata_prelude(&prelude) {
        Ok(value) if value.prelude_bytes() == prelude.len() => value,
        Ok(_) | Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };

    let selected = if historical {
        historical_predecessor_reopen_descriptor(
            env,
            profile_id,
            actor,
            device_id,
            session_id,
            fencing_token,
            body.coordinator_epoch(),
        )
        .await
    } else {
        authoritative_reopen_descriptor(
            env,
            profile_id,
            actor,
            device_id,
            session_id,
            fencing_token,
            body.coordinator_epoch(),
        )
        .await
    };
    let (reference, descriptor) = match selected {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !opening_metadata_matches_descriptor(actor, profile_id, &descriptor, &inspected)
        || descriptor.generation_id() != reference.generation_id()
    {
        return integrity_failure(actor.correlation_id().as_str());
    }

    let metadata = inspected.metadata();
    let keyring = match generation_root_keyring(env) {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    let material = match keyring.derive_for_key_id(
        metadata.key_id(),
        actor.tenant_scope().tenant_id(),
        profile_id,
        descriptor.generation_id(),
        metadata.plaintext_digest().bytes(),
    ) {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    if material.key_id() != metadata.key_id() || material.nonce_prefix() != metadata.nonce_prefix()
    {
        return integrity_failure(actor.correlation_id().as_str());
    }
    let dek_secret = material.copy_dek_secret();
    machine_json(&BridgeGenerationOpeningMaterialResponse::new(
        material.key_id().as_str(),
        lower_hex(&dek_secret[..]),
    ))
}

async fn authoritative_reopen_descriptor(
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    session_id: SessionId,
    fencing_token: FencingToken,
    coordinator_epoch: u64,
) -> core::result::Result<
    (
        application_ports::generation_objects::GenerationObjectCatalogReference,
        GenerationObjectDescriptor,
    ),
    Result<Response>,
> {
    let preconditions = match device_execution_preconditions(env) {
        Ok(value) => value,
        Err(error) => return Err(Err(error)),
    };
    let reference = match preconditions
        .load_active_verified_generation_object(actor.tenant_scope(), profile_id)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return Err(verification_conflict(actor.correlation_id().as_str())),
        Err(error) => {
            return Err(object_verify_failure(
                actor.correlation_id().as_str(),
                error.class(),
            ));
        }
    };
    let target = DeviceJobTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        device_id.clone(),
        profile_id.clone(),
        reference.generation_id().clone(),
    );
    match preconditions
        .evaluate_device_execution(actor, &target)
        .await
    {
        Ok(DeviceExecutionReadiness::Ready) => {}
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::DeviceUnauthorized)) => {
            return Err(forbidden(actor.correlation_id().as_str()));
        }
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::GenerationInactive)) => {
            return Err(version_conflict(actor.correlation_id().as_str()));
        }
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::CertificationIncomplete)) => {
            return Err(verification_conflict(actor.correlation_id().as_str()));
        }
        Err(error) => match error.class() {
            DeviceJobPortErrorClass::AuthenticationFailed => {
                return Err(forbidden(actor.correlation_id().as_str()));
            }
            DeviceJobPortErrorClass::IntegrityFailure => {
                return Err(integrity_failure(actor.correlation_id().as_str()));
            }
            DeviceJobPortErrorClass::DependencyUnavailable => {
                return Err(dependency(actor.correlation_id().as_str()));
            }
        },
    }

    let successor = profile_generation_successor_commit(env);
    if let Err(error) = successor
        .prove_profile_generation_writer_authority(
            actor,
            &ProfileGenerationWriterAuthorityRequest::new(
                device_id.clone(),
                profile_id.clone(),
                session_id,
                fencing_token,
                coordinator_epoch,
            ),
        )
        .await
    {
        return Err(successor_failure(
            actor.correlation_id().as_str(),
            error.class(),
        ));
    }

    let objects = match generation_object_verifier(env) {
        Ok(value) => value,
        Err(error) => return Err(Err(error)),
    };
    let descriptor = match objects
        .load_generation_object_descriptor_exact(actor.tenant_scope(), &reference)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return Err(verification_conflict(actor.correlation_id().as_str())),
        Err(error) => {
            return Err(object_verify_failure(
                actor.correlation_id().as_str(),
                error.class(),
            ));
        }
    };
    Ok((reference, descriptor))
}

/// The device only requests a read-only historical candidate, never selects an
/// ID/key. The active-profile authorization is checked against CURRENT before
/// D1 supplies exactly its journal-linked verified predecessor. Neither request
/// is a WORKING admission or permission to prune any immutable R2 object.
async fn historical_predecessor_reopen_descriptor(
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    session_id: SessionId,
    fencing_token: FencingToken,
    coordinator_epoch: u64,
) -> core::result::Result<
    (
        application_ports::generation_objects::GenerationObjectCatalogReference,
        GenerationObjectDescriptor,
    ),
    Result<Response>,
> {
    let (current, _) = authoritative_reopen_descriptor(
        env,
        profile_id,
        actor,
        device_id,
        session_id,
        fencing_token,
        coordinator_epoch,
    )
    .await?;
    let observer = match profile_generation_successor_observer(env) {
        Ok(value) => value,
        Err(error) => return Err(Err(error)),
    };
    let predecessor = match observer
        .load_verified_predecessor_of_active(
            actor.tenant_scope(),
            profile_id,
            current.generation_id(),
        )
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return Err(verification_conflict(actor.correlation_id().as_str())),
        Err(error) => {
            return Err(object_verify_failure(
                actor.correlation_id().as_str(),
                error.class(),
            ));
        }
    };
    if predecessor.generation_id() == current.generation_id() {
        return Err(integrity_failure(actor.correlation_id().as_str()));
    }
    let objects = match generation_object_verifier(env) {
        Ok(value) => value,
        Err(error) => return Err(Err(error)),
    };
    let descriptor = match objects
        .load_generation_object_descriptor_exact(actor.tenant_scope(), &predecessor)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return Err(verification_conflict(actor.correlation_id().as_str())),
        Err(error) => {
            return Err(object_verify_failure(
                actor.correlation_id().as_str(),
                error.class(),
            ));
        }
    };
    if descriptor.generation_id() != predecessor.generation_id() {
        return Err(integrity_failure(actor.correlation_id().as_str()));
    }
    // A concurrent pointer change never makes this stale historical grant
    // silently valid. A later successor transaction must re-authorize.
    let active = match device_execution_preconditions(env) {
        Ok(value) => value,
        Err(error) => return Err(Err(error)),
    };
    let fresh_current = match active
        .load_active_verified_generation_object(actor.tenant_scope(), profile_id)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return Err(verification_conflict(actor.correlation_id().as_str())),
        Err(error) => {
            return Err(object_verify_failure(
                actor.correlation_id().as_str(),
                error.class(),
            ));
        }
    };
    if fresh_current != current {
        return Err(version_conflict(actor.correlation_id().as_str()));
    }
    Ok((predecessor, descriptor))
}

fn reopen_witness(
    session_id: &str,
    fencing_token: &str,
    coordinator_epoch: u64,
) -> Option<(SessionId, FencingToken)> {
    if coordinator_epoch == 0 {
        return None;
    }
    Some((
        SessionId::parse(session_id.to_owned()).ok()?,
        FencingToken::parse(fencing_token.to_owned()).ok()?,
    ))
}

fn opening_metadata_matches_descriptor(
    actor: &ActorContext,
    profile_id: &ProfileId,
    descriptor: &GenerationObjectDescriptor,
    inspected: &InspectedGenerationMetadataPrelude,
) -> bool {
    let metadata = inspected.metadata();
    metadata.tenant_id() == actor.tenant_scope().tenant_id()
        && metadata.profile_id() == profile_id
        && metadata.generation_id() == descriptor.generation_id()
        && metadata.object_key() == descriptor.object_key()
        && lower_hex(&inspected.metadata_digest().bytes()) == descriptor.metadata_digest()
}

async fn dispatch_sealing_material(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
) -> Result<Response> {
    let body = match request
        .json::<BridgeGenerationSealingMaterialRequest>()
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
    if generation_id == base_generation_id || body.coordinator_epoch() == 0 {
        return invalid_request(actor.correlation_id().as_str());
    }
    let plaintext_digest = match decode_lower_sha256(body.plaintext_digest()) {
        Some(value) => value,
        None => return invalid_request(actor.correlation_id().as_str()),
    };
    let session_id = match SessionId::parse(body.coordinator_session_id().to_owned()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let fencing_token = match FencingToken::parse(body.coordinator_fencing_token().to_owned()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let target = DeviceJobTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        device_id.clone(),
        profile_id.clone(),
        base_generation_id,
    );

    let preconditions = device_execution_preconditions(env)?;
    match preconditions
        .evaluate_device_execution(actor, &target)
        .await
    {
        Ok(DeviceExecutionReadiness::Ready) => {}
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::DeviceUnauthorized)) => {
            return forbidden(actor.correlation_id().as_str());
        }
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::GenerationInactive)) => {
            return version_conflict(actor.correlation_id().as_str());
        }
        Ok(DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::CertificationIncomplete)) => {
            return verification_conflict(actor.correlation_id().as_str());
        }
        Err(error) => match error.class() {
            DeviceJobPortErrorClass::AuthenticationFailed => {
                return forbidden(actor.correlation_id().as_str());
            }
            DeviceJobPortErrorClass::IntegrityFailure => {
                return integrity_failure(actor.correlation_id().as_str());
            }
            DeviceJobPortErrorClass::DependencyUnavailable => {
                return dependency(actor.correlation_id().as_str());
            }
        },
    }

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

    let keyring = match generation_root_keyring(env) {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    let material = match keyring.derive_active(
        actor.tenant_scope().tenant_id(),
        profile_id,
        &generation_id,
        plaintext_digest,
    ) {
        Ok(value) => value,
        Err(_) => return integrity_failure(actor.correlation_id().as_str()),
    };
    let dek_secret = material.copy_dek_secret();
    let nonce_prefix = material.nonce_prefix().bytes();
    machine_json(&BridgeGenerationSealingMaterialResponse::new(
        material.key_id().as_str(),
        lower_hex(&dek_secret[..]),
        lower_hex(&nonce_prefix),
        GENERATION_SEALING_CHUNK_BYTES,
    ))
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

/// Exact ingress size envelope must match both streaming crypto and D1
/// journals BEFORE R2 signed PUT is issued. Rejecting only at commit would
/// leave a verified but forever uncommittable orphan in R2.
fn valid_stream_container_bytes(bytes: u64) -> bool {
    (1..=MAX_STREAM_CONTAINER_BYTES).contains(&bytes)
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
        == canonical_generation_object_key(
            actor.tenant_scope().tenant_id(),
            profile_id,
            generation_id,
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

fn decode_lower_sha256(value: &str) -> Option<[u8; 32]> {
    if !lower_sha256(value) {
        return None;
    }
    let bytes = value.as_bytes();
    let mut decoded = [0_u8; 32];
    for (index, output) in decoded.iter_mut().enumerate() {
        let high = lower_hex_nibble(bytes[index * 2])?;
        let low = lower_hex_nibble(bytes[index * 2 + 1])?;
        *output = (high << 4) | low;
    }
    Some(decoded)
}

fn decode_bounded_lower_hex(value: &str, max_bytes: usize) -> Option<Vec<u8>> {
    if value.is_empty() || value.len() % 2 != 0 || value.len() > max_bytes.checked_mul(2)? {
        return None;
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(value.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = lower_hex_nibble(pair[0])?;
        let low = lower_hex_nibble(pair[1])?;
        decoded.push((high << 4) | low);
    }
    Some(decoded)
}

const fn lower_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn server_now() -> UnixMillis {
    UnixMillis::new(Date::now().as_millis())
}

fn server_upload_signing_time() -> UploadSigningTimeResult {
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

fn server_download_signing_time() -> DownloadSigningTimeResult {
    let now: worker::js_sys::Date = Date::now().into();
    R2GenerationDownloadSigningTime::parse(format!(
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

fn download_signing_failure(
    correlation_id: &str,
    error: R2GenerationDownloadCapabilityError,
) -> Result<Response> {
    match error {
        R2GenerationDownloadCapabilityError::InvalidAccountId
        | R2GenerationDownloadCapabilityError::InvalidBucketName
        | R2GenerationDownloadCapabilityError::InvalidCredentials
        | R2GenerationDownloadCapabilityError::InvalidSigningTime
        | R2GenerationDownloadCapabilityError::InvalidExpiry
        | R2GenerationDownloadCapabilityError::InvalidDescriptor => {
            integrity_failure(correlation_id)
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
    use super::{
        BridgeGenerationMachineOperation, decode_bounded_lower_hex, decode_lower_sha256,
        descriptor_shape_is_canonical, operation, valid_stream_container_bytes,
    };
    use encrypted_generation_domain::canonical_generation_object_key;
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, GenerationId, ProfileId, TenantId, TenantScope,
    };
    use worker::Method;

    #[test]
    fn only_exact_generation_machine_posts_are_recognized() {
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/sealing-material",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::SealingMaterial)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/upload-capability",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::UploadCapability)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/commit",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::Commit)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/observe",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::Observe)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/retry-decision",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::RetryDecision)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/local-unsynced/retry-decision",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::LocalUnsyncedRetryDecision)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-reopen/download-capability",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::DownloadCapability)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-reopen/opening-material",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::OpeningMaterial)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-working-candidate/download-capability",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::PredecessorDownloadCapability)
        );
        assert_eq!(
            operation(
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-working-candidate/opening-material",
                Method::Post,
            ),
            Some(BridgeGenerationMachineOperation::PredecessorOpeningMaterial)
        );
        for (method, path) in [
            (
                Method::Get,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/commit",
            ),
            (
                Method::Get,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-reopen/download-capability",
            ),
            (
                Method::Get,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-reopen/opening-material",
            ),
            (
                Method::Get,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-working-candidate/download-capability",
            ),
            (
                Method::Get,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-working-candidate/opening-material",
            ),
            (
                Method::Post,
                "/bridge/v1/tenants/tenant_01/profiles/profile_01/generation-successor/verify",
            ),
            (
                Method::Post,
                "/bridge/v2/tenants/tenant_01/profiles/profile_01/generation-successor/commit",
            ),
            (
                Method::Post,
                "/bridge/v2/tenants/tenant_01/profiles/profile_01/generation-reopen/download-capability",
            ),
            (
                Method::Post,
                "/bridge/v2/tenants/tenant_01/profiles/profile_01/generation-reopen/opening-material",
            ),
        ] {
            assert_eq!(operation(path, method), None);
        }
    }

    #[test]
    fn plaintext_digest_transport_is_exact_lowercase_sha256() {
        assert_eq!(decode_lower_sha256(&"0f".repeat(32)), Some([0x0f; 32]));
        assert_eq!(decode_lower_sha256(&"0F".repeat(32)), None);
        assert_eq!(decode_lower_sha256(&"0f".repeat(31)), None);
        assert_eq!(decode_lower_sha256(&format!("{}g0", "0f".repeat(31))), None);
    }

    #[test]
    fn bounded_metadata_hex_rejects_uppercase_odd_and_oversized_input() {
        assert_eq!(decode_bounded_lower_hex("00ff", 2), Some(vec![0, 255]));
        assert_eq!(decode_bounded_lower_hex("00FF", 2), None);
        assert_eq!(decode_bounded_lower_hex("0", 2), None);
        assert_eq!(decode_bounded_lower_hex("000000", 2), None);
        assert_eq!(decode_bounded_lower_hex("", 2), None);
    }

    #[test]
    fn streaming_size_matches_crypto_and_d1_before_any_signed_put() {
        use encrypted_generation_domain::MAX_STREAM_CONTAINER_BYTES;
        assert!(!valid_stream_container_bytes(0));
        assert!(valid_stream_container_bytes(1));
        assert!(valid_stream_container_bytes(512 * 1024 * 1024 + 1024 * 1024));
        assert!(valid_stream_container_bytes(MAX_STREAM_CONTAINER_BYTES));
        assert!(!valid_stream_container_bytes(MAX_STREAM_CONTAINER_BYTES + 1));
        assert!(!valid_stream_container_bytes(u64::MAX));
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
        let key = canonical_generation_object_key(
            actor.tenant_scope().tenant_id(),
            &profile_id,
            &generation_id,
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
