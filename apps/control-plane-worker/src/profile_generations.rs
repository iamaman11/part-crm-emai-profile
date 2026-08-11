use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::profile_generation_application;
use application_ports::generations::{GenerationReadModel, GenerationStatus};
use control_plane_contract::RouteClass;
use control_plane_contract::profile_generation_api::{
    GenerationProjectionDto, GenerationStatusDto, ProfileGenerationVersionRequest,
    QuarantineGenerationRequest, RegisterGenerationRequest, VerifyGenerationRequest,
};
use control_plane_contract::public_api::MutationReceipt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, GenerationId, ProfileId};
use use_cases::generations::{
    GenerationMutationOutcome, GenerationOperationError, ProfileGenerationVersionCommand,
    QuarantineGenerationCommand, RegisterGenerationCommand, VerifyGenerationCommand,
    authorize_generation_mutation, execute_activate_generation, execute_deactivate_generation,
    execute_quarantine_generation, execute_register_generation, execute_verify_generation,
    get_visible_generation, next_generation_version, validate_generation_registration,
    validate_generation_verification_reference,
};
use worker::{Env, Request, Response, Result};

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let profile_id = segments
        .get(5)
        .and_then(|value| ProfileId::parse((*value).to_owned()).ok());
    let generation_id = segments
        .get(7)
        .and_then(|value| GenerationId::parse((*value).to_owned()).ok());

    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);

    match route {
        RouteClass::ProfileGenerationCollectionApi => {
            if let Err(error) = authorize_generation_mutation(role) {
                return operation_failure(actor.actor().correlation_id().as_str(), error);
            }
            let Some(profile_id) = profile_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            register_generation(request, env, actor.actor(), role, profile_id).await
        }
        RouteClass::ProfileGenerationResourceApi => {
            let (Some(profile_id), Some(generation_id)) = (profile_id, generation_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            get_generation(env, actor.actor(), role, &profile_id, &generation_id).await
        }
        RouteClass::ProfileGenerationVerifyApi => {
            if let Err(error) = authorize_generation_mutation(role) {
                return operation_failure(actor.actor().correlation_id().as_str(), error);
            }
            let (Some(profile_id), Some(generation_id)) = (profile_id, generation_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            verify_generation(request, env, actor.actor(), role, profile_id, generation_id).await
        }
        RouteClass::ProfileGenerationActivateApi => {
            if let Err(error) = authorize_generation_mutation(role) {
                return operation_failure(actor.actor().correlation_id().as_str(), error);
            }
            let (Some(profile_id), Some(generation_id)) = (profile_id, generation_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            change_profile_generation(
                request,
                env,
                actor.actor(),
                role,
                profile_id,
                generation_id,
                true,
            )
            .await
        }
        RouteClass::ProfileGenerationDeactivateApi => {
            if let Err(error) = authorize_generation_mutation(role) {
                return operation_failure(actor.actor().correlation_id().as_str(), error);
            }
            let (Some(profile_id), Some(generation_id)) = (profile_id, generation_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            change_profile_generation(
                request,
                env,
                actor.actor(),
                role,
                profile_id,
                generation_id,
                false,
            )
            .await
        }
        RouteClass::ProfileGenerationQuarantineApi => {
            if let Err(error) = authorize_generation_mutation(role) {
                return operation_failure(actor.actor().correlation_id().as_str(), error);
            }
            let (Some(profile_id), Some(generation_id)) = (profile_id, generation_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            quarantine_generation(request, env, actor.actor(), role, profile_id, generation_id)
                .await
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

async fn register_generation(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: ProfileId,
) -> Result<Response> {
    let body = match request.json::<RegisterGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let generation_id = match GenerationId::parse(body.generation_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if let Err(error) = validate_generation_registration(
        &body.object_key,
        &body.metadata_digest,
        &body.container_digest,
    ) {
        return operation_failure(actor.correlation_id().as_str(), error);
    }
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = profile_generation_application(env)?;
    match execute_register_generation(
        actor,
        role,
        &application,
        RegisterGenerationCommand {
            profile_id,
            generation_id,
            object_key: body.object_key,
            metadata_digest: body.metadata_digest,
            container_digest: body.container_digest,
            evidence,
        },
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 201),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn get_generation(
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<Response> {
    let application = profile_generation_application(env)?;
    match get_visible_generation(actor, role, &application, profile_id, generation_id).await {
        Ok(generation) => Response::from_json(&generation_projection(&generation)),
        Err(GenerationOperationError::NotFound) => {
            neutral_not_found(actor.correlation_id().as_str())
        }
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn verify_generation(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: ProfileId,
    generation_id: GenerationId,
) -> Result<Response> {
    let body = match request.json::<VerifyGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_generation_version = match AggregateVersion::new(body.expected_generation_version)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if let Err(error) = validate_generation_verification_reference(&body.verification_reference) {
        return operation_failure(actor.correlation_id().as_str(), error);
    }
    if let Err(error) = next_generation_version(expected_generation_version) {
        return operation_failure(actor.correlation_id().as_str(), error);
    }
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = profile_generation_application(env)?;
    match execute_verify_generation(
        actor,
        role,
        &application,
        VerifyGenerationCommand {
            profile_id,
            generation_id,
            expected_generation_version,
            verification_reference: body.verification_reference,
            evidence,
        },
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 200),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn change_profile_generation(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: ProfileId,
    generation_id: GenerationId,
    activate: bool,
) -> Result<Response> {
    let body = match request.json::<ProfileGenerationVersionRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_profile_version = match AggregateVersion::new(body.expected_profile_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if let Err(error) = next_generation_version(expected_profile_version) {
        return operation_failure(actor.correlation_id().as_str(), error);
    }
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = profile_generation_application(env)?;
    let command = ProfileGenerationVersionCommand {
        profile_id,
        generation_id,
        expected_profile_version,
        evidence,
    };
    let result = if activate {
        execute_activate_generation(actor, role, &application, command).await
    } else {
        execute_deactivate_generation(actor, role, &application, command).await
    };
    match result {
        Ok(outcome) => mutation_receipt(&outcome, 200),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn quarantine_generation(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: ProfileId,
    generation_id: GenerationId,
) -> Result<Response> {
    let body = match request.json::<QuarantineGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_generation_version = match AggregateVersion::new(body.expected_generation_version)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if let Err(error) = next_generation_version(expected_generation_version) {
        return operation_failure(actor.correlation_id().as_str(), error);
    }
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = profile_generation_application(env)?;
    match execute_quarantine_generation(
        actor,
        role,
        &application,
        QuarantineGenerationCommand {
            profile_id,
            generation_id,
            expected_generation_version,
            evidence,
        },
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 200),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn operation_failure(correlation_id: &str, error: GenerationOperationError) -> Result<Response> {
    match error {
        GenerationOperationError::InvalidRequest => {
            problem(correlation_id, 400, "invalid_request", "Invalid Request")
        }
        GenerationOperationError::NotFound => neutral_not_found(correlation_id),
        GenerationOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        GenerationOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        GenerationOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        GenerationOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        GenerationOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        GenerationOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn mutation_receipt(outcome: &GenerationMutationOutcome, status: u16) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

fn generation_projection(generation: &GenerationReadModel) -> GenerationProjectionDto {
    GenerationProjectionDto {
        generation_id: generation.generation_id().as_str().to_owned(),
        metadata_digest: generation.metadata_digest().to_owned(),
        container_digest: generation.container_digest().to_owned(),
        status: generation_status(generation.status()),
        version: generation.version().value(),
        verification_reference: generation.verification_reference().map(str::to_owned),
    }
}

const fn generation_status(status: GenerationStatus) -> GenerationStatusDto {
    match status {
        GenerationStatus::Registered => GenerationStatusDto::Registered,
        GenerationStatus::Verified => GenerationStatusDto::Verified,
        GenerationStatus::Quarantined => GenerationStatusDto::Quarantined,
    }
}

#[cfg(test)]
mod tests {
    use super::generation_status;
    use application_ports::generations::GenerationStatus;
    use control_plane_contract::profile_generation_api::GenerationStatusDto;

    #[test]
    fn domain_status_mapping_covers_every_public_generation_status() {
        for (domain, wire) in [
            (
                GenerationStatus::Registered,
                GenerationStatusDto::Registered,
            ),
            (GenerationStatus::Verified, GenerationStatusDto::Verified),
            (
                GenerationStatus::Quarantined,
                GenerationStatusDto::Quarantined,
            ),
        ] {
            assert_eq!(generation_status(domain), wire);
        }
    }
}
