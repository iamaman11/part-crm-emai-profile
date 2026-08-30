use crate::access_session::problem;
use crate::composition::{
    device_execution_preconditions, device_job_authorization,
    generation_download_capability_signer, generation_object_verifier, generation_root_keyring,
    generation_upload_capability_signer, profile_generation_successor_commit,
};
use application_ports::device_jobs::{DeviceJobAuthorizationPort, DeviceJobCapability};
use application_ports::generation_objects::{
    ActiveGenerationObjectReferencePort, GenerationObjectDescriptor,
    GenerationObjectDescriptorReadPort, GenerationObjectDescriptorVerifyPort,
};
use application_ports::generations::GenerationPortErrorClass;
use application_ports::profile_generation_successor::{
    ProfileGenerationCommitWitness, ProfileGenerationSuccessorCommitErrorClass,
    ProfileGenerationSuccessorCommitOutcome, ProfileGenerationSuccessorCommitPort,
    ProfileGenerationSuccessorCommitRequest, ProfileGenerationSuccessorVersionPort,
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
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
use device_domain::DeviceJobTarget;
use encrypted_generation_domain::{
    InspectedGenerationMetadataPrelude, MAX_GENERATION_METADATA_PRELUDE_BYTES,
    canonical_generation_object_key, inspect_generation_metadata_prelude,
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
    SealingMaterial,
    UploadCapability,
    Commit,
    DownloadCapability,
    OpeningMaterial,
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
) -> Result<Response> {
    if operation == BridgeGenerationMachineOperation::SealingMaterial {
        return dispatch_sealing_material(request, env, profile_id, actor, device_id).await;
    }
    if operation == BridgeGenerationMachineOperation::DownloadCapability {
        return dispatch_download_capability(request, env, profile_id, actor, device_id).await;
    }
    if operation == BridgeGenerationMachineOperation::OpeningMaterial {
        return dispatch_opening_material(request, env, profile_id, actor, device_id).await;
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
        BridgeGenerationMachineOperation::SealingMaterial
        | BridgeGenerationMachineOperation::DownloadCapability
        | BridgeGenerationMachineOperation::OpeningMaterial => {
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

async fn dispatch_download_capability(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
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

    let (reference, descriptor) = match authoritative_reopen_descriptor(
        env,
        profile_id,
        actor,
        device_id,
        session_id,
        fencing_token,
        body.coordinator_epoch(),
    )
    .await
    {
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

    let (reference, descriptor) = match authoritative_reopen_descriptor(
        env,
        profile_id,
        actor,
        device_id,
        session_id,
        fencing_token,
        body.coordinator_epoch(),
    )
    .await
    {
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
        descriptor_shape_is_canonical, operation,
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
