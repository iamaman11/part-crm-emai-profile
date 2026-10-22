use super::*;
use application_ports::generation_initialization::InitialGenerationWrite;
use control_plane_contract::generation_initialization_api::{
    InitialGenerationCommitted, InitialGenerationPrepared, InitialGenerationRequest,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::AggregateVersion;
use use_cases::generation_initialization::{
    authorize_initial_generation, authorize_initial_generation_access,
    execute_initialize_generation, validate_initial_generation_object,
};
use use_cases::generations::GenerationOperationError;

pub(super) async fn dispatch(
    request: &mut Request,
    env: &Env,
    profile_id: &ProfileId,
    actor: &ActorContext,
    device_id: &DeviceId,
    role: MembershipRole,
) -> Result<Response> {
    let application = crate::composition::profile_generation_application(env)?;
    let context =
        match authorize_initial_generation_access(actor, role, profile_id, &application).await {
            Ok(value) => value,
            Err(error) => return failure(actor, error),
        };
    let body = match request.json::<InitialGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let generation_id = match GenerationId::parse(body.generation_id()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected = match AggregateVersion::new(body.expected_profile_version()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if matches!(body, InitialGenerationRequest::Prepare { .. })
        && context.status == application_ports::profiles::ProfileStatus::Ready
    {
        let evidence = crate::command_evidence::from_profile_initialization(
            request,
            actor,
            &generation_id,
            &body,
            true,
        )?;
        return match crate::profiles::profile_launch::issue_for_device(
            env, actor, role, profile_id, device_id, &evidence,
        )
        .await?
        {
            Ok(launch) => machine_json(&InitialGenerationPrepared::Launch { launch }),
            Err(_) => failure(actor, GenerationOperationError::InvalidState),
        };
    }
    // A commit replay is evaluated by the application before requiring DRAFT again.
    if !matches!(body, InitialGenerationRequest::Commit { .. }) {
        if let Err(error) =
            authorize_initial_generation(actor, role, profile_id, expected, &application).await
        {
            return failure(actor, error);
        }
    }
    match &body {
        InitialGenerationRequest::Prepare { .. } => {
            machine_json(&InitialGenerationPrepared::Initialize {
                generation_id: generation_id.as_str().to_owned(),
                expected_profile_version: expected.value(),
            })
        }
        InitialGenerationRequest::SealingMaterial {
            plaintext_digest, ..
        } => {
            let Some(plaintext) = decode_lower_sha256(plaintext_digest) else {
                return invalid_request(actor.correlation_id().as_str());
            };
            let keyring = match generation_root_keyring(env) {
                Ok(value) => value,
                Err(_) => return integrity_failure(actor.correlation_id().as_str()),
            };
            let material = match keyring.derive_active(
                actor.tenant_scope().tenant_id(),
                profile_id,
                &generation_id,
                plaintext,
            ) {
                Ok(value) => value,
                Err(_) => return integrity_failure(actor.correlation_id().as_str()),
            };
            let secret = material.copy_dek_secret();
            machine_json(&BridgeGenerationSealingMaterialResponse::new(
                material.key_id().as_str(),
                lower_hex(&secret[..]),
                lower_hex(&material.nonce_prefix().bytes()),
                GENERATION_SEALING_CHUNK_BYTES,
            ))
        }
        InitialGenerationRequest::UploadCapability {
            metadata_digest,
            container_digest,
            container_bytes,
            ..
        }
        | InitialGenerationRequest::Commit {
            metadata_digest,
            container_digest,
            container_bytes,
            ..
        } => {
            let descriptor = GenerationObjectDescriptor::new(
                profile_id.clone(),
                generation_id.clone(),
                canonical_generation_object_key(
                    actor.tenant_scope().tenant_id(),
                    profile_id,
                    &generation_id,
                ),
                metadata_digest,
                container_digest,
                *container_bytes,
            );
            if let Err(error) = validate_initial_generation_object(actor, &descriptor) {
                return failure(actor, error);
            }
            let verifier = generation_object_verifier(env)?;
            if matches!(body, InitialGenerationRequest::Commit { .. }) {
                let evidence = crate::command_evidence::from_profile_initialization(
                    request,
                    actor,
                    &generation_id,
                    &body,
                    false,
                )?;
                let outcome = match execute_initialize_generation(
                    actor,
                    role,
                    &application,
                    &verifier,
                    InitialGenerationWrite {
                        device_id: device_id.clone(),
                        object: descriptor,
                        expected_profile_version: expected,
                        evidence,
                    },
                )
                .await
                {
                    Ok(value) => value,
                    Err(error) => return failure(actor, error),
                };
                let launch_evidence = crate::command_evidence::from_profile_initialization(
                    request,
                    actor,
                    &generation_id,
                    &body,
                    true,
                )?;
                let launch = match crate::profiles::profile_launch::issue_for_device(
                    env,
                    actor,
                    role,
                    profile_id,
                    device_id,
                    &launch_evidence,
                )
                .await?
                {
                    Ok(value) => value,
                    Err(_error) => {
                        return crate::access_session::problem(
                            actor.correlation_id().as_str(),
                            409,
                            "launch_unavailable",
                            "Profile prepared; refresh the application to open it",
                        );
                    }
                };
                return machine_json(&InitialGenerationCommitted {
                    launch,
                    generation_id: outcome.generation_id.as_str().to_owned(),
                    profile_version: outcome.profile_version.value(),
                    replayed: outcome.replayed,
                });
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
            let time = match server_upload_signing_time() {
                Ok(value) => value,
                Err(_) => return integrity_failure(actor.correlation_id().as_str()),
            };
            let capability = match signer.sign_put(
                actor.tenant_scope(),
                &descriptor,
                &time,
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
    }
}

fn failure(actor: &ActorContext, error: GenerationOperationError) -> Result<Response> {
    use GenerationOperationError as E;
    let (status, code, title) = match error {
        E::NotFound => (404, "not_found", "Resource unavailable"),
        E::InvalidRequest => (
            400,
            "invalid_request",
            "Initial generation request is invalid",
        ),
        E::VersionConflict | E::Conflict => (
            409,
            "version_conflict",
            "Profile changed; refresh the application",
        ),
        E::InvalidState => (
            409,
            "invalid_state",
            "Profile is not awaiting an initial generation",
        ),
        E::IntegrityFailure => (
            409,
            "integrity_failure",
            "Initial generation verification failed",
        ),
        E::InternalFailure => (
            500,
            "internal_failure",
            "Initial generation operation failed",
        ),
        E::DependencyUnavailable => (
            503,
            "dependency_unavailable",
            "Initial generation dependency unavailable",
        ),
    };
    problem(actor.correlation_id().as_str(), status, code, title)
}
