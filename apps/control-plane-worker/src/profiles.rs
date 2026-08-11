use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::profile_application;
use application_ports::profiles::ProfileStatus;
use control_plane_contract::RouteClass;
use control_plane_contract::profile_generation_api::{
    ProfileAssignmentRequest, ProfileCreateRequest, ProfileGrantRequest, ProfileProjectionDto,
    ProfileStatusDto,
};
use control_plane_contract::public_api::MutationReceipt;
use profile_platform_primitives::{ActorId, AggregateVersion, AssignmentId, ClientId, ProfileId};
use use_cases::profile_assignments::{
    ExecuteAssignProfileCommand, ProfileAssignmentOperationError, ProfileAssignmentOutcome,
    authorize_profile_assignment, execute_assign_profile, next_profile_assignment_version,
};
use use_cases::profile_grants::{
    ExecuteProfileGrantCommand, ProfileGrantAction, ProfileGrantOperationError,
    ProfileGrantOutcome, authorize_profile_grant, execute_profile_grant,
};
use use_cases::profiles::{
    ExecuteCreateProfileCommand, ProfileDetails, ProfileMutationOutcome, ProfileOperationError,
    authorize_profile_create, execute_create_profile, get_visible_profile,
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

    match route {
        RouteClass::ProfileCollectionApi => create_profile(request, env, tenant_id).await,
        RouteClass::ProfileResourceApi => {
            let profile_id = segments.get(5).copied().unwrap_or_default();
            get_profile(request, env, tenant_id, profile_id).await
        }
        RouteClass::ProfileAssignmentApi => {
            let profile_id = segments.get(5).copied().unwrap_or_default();
            assign_profile(request, env, tenant_id, profile_id).await
        }
        RouteClass::ProfileGrantApi => {
            let profile_id = segments.get(5).copied().unwrap_or_default();
            let actor_id = segments.get(7).copied().unwrap_or_default();
            update_profile_grant(request, env, tenant_id, profile_id, actor_id).await
        }
        _ => neutral_not_found(&correlation_hint(request)),
    }
}

async fn create_profile(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_profile_create(role) {
        return operation_failure(actor.actor().correlation_id().as_str(), error);
    }
    let body = match request.json::<ProfileCreateRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let profile_id = match ProfileId::parse(body.profile_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = profile_application(env)?;
    match execute_create_profile(
        actor.actor(),
        role,
        &application,
        ExecuteCreateProfileCommand::new(profile_id, evidence),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn get_profile(
    request: &Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let application = profile_application(env)?;
    match get_visible_profile(
        actor.actor(),
        membership_role(&actor),
        &application,
        &profile_id,
    )
    .await
    {
        Ok(profile) => Response::from_json(&profile_projection(&profile)),
        Err(ProfileOperationError::NotFound) => {
            neutral_not_found(actor.actor().correlation_id().as_str())
        }
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn assign_profile(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_profile_assignment(role) {
        return assignment_failure(actor.actor().correlation_id().as_str(), error);
    }

    let body = match request.json::<ProfileAssignmentRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let assignment_id = match AssignmentId::parse(body.assignment_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let client_id = match ClientId::parse(body.client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_profile_version = match AggregateVersion::new(body.expected_profile_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    if let Err(error) = next_profile_assignment_version(expected_profile_version) {
        return assignment_failure(actor.actor().correlation_id().as_str(), error);
    }
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = profile_application(env)?;
    match execute_assign_profile(
        actor.actor(),
        role,
        &application,
        ExecuteAssignProfileCommand::new(
            assignment_id,
            profile_id,
            client_id,
            expected_profile_version,
            body.reason,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => assignment_receipt(&outcome),
        Err(error) => assignment_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn update_profile_grant(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_profile_grant(role) {
        return profile_grant_failure(actor.actor().correlation_id().as_str(), error);
    }

    let body = match request.json::<ProfileGrantRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let target_actor_id = match ActorId::parse(target_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_profile_version = match AggregateVersion::new(body.expected_profile_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let action = if request.method().as_ref() == "DELETE" {
        ProfileGrantAction::Revoke
    } else {
        ProfileGrantAction::Grant
    };
    let application = profile_application(env)?;
    match execute_profile_grant(
        actor.actor(),
        role,
        &application,
        action,
        ExecuteProfileGrantCommand::new(
            target_actor_id,
            profile_id,
            expected_profile_version,
            body.role,
            body.reason,
            evidence,
        ),
    )
    .await
    {
        Ok(_) if action == ProfileGrantAction::Revoke => no_content(),
        Ok(outcome) => profile_grant_receipt(&outcome),
        Err(error) => profile_grant_failure(actor.actor().correlation_id().as_str(), error),
    }
}

fn operation_failure(correlation_id: &str, error: ProfileOperationError) -> Result<Response> {
    match error {
        ProfileOperationError::NotFound => neutral_not_found(correlation_id),
        ProfileOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ProfileOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        ProfileOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        ProfileOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn assignment_failure(
    correlation_id: &str,
    error: ProfileAssignmentOperationError,
) -> Result<Response> {
    match error {
        ProfileAssignmentOperationError::NotFound => neutral_not_found(correlation_id),
        ProfileAssignmentOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        ProfileAssignmentOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        ProfileAssignmentOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        ProfileAssignmentOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        ProfileAssignmentOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        ProfileAssignmentOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn profile_grant_failure(
    correlation_id: &str,
    error: ProfileGrantOperationError,
) -> Result<Response> {
    match error {
        ProfileGrantOperationError::InvalidRequest => invalid_request(correlation_id),
        ProfileGrantOperationError::NotFound => neutral_not_found(correlation_id),
        ProfileGrantOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        ProfileGrantOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        ProfileGrantOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        ProfileGrantOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        ProfileGrantOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        ProfileGrantOperationError::DependencyUnavailable => problem(
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

fn no_content() -> Result<Response> {
    Ok(Response::empty()?.with_status(204))
}

fn mutation_receipt(outcome: &ProfileMutationOutcome) -> Result<Response> {
    let status = if outcome.replayed() { 200 } else { 201 };
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

fn assignment_receipt(outcome: &ProfileAssignmentOutcome) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(200))
}

fn profile_grant_receipt(outcome: &ProfileGrantOutcome) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(200))
}

fn profile_projection(profile: &ProfileDetails) -> ProfileProjectionDto {
    ProfileProjectionDto {
        profile_id: profile.profile_id().as_str().to_owned(),
        status: profile_status(profile.status()),
        version: profile.version().value(),
        linked_client_id: profile
            .linked_client_id()
            .map(|value| value.as_str().to_owned()),
    }
}

const fn profile_status(status: ProfileStatus) -> ProfileStatusDto {
    match status {
        ProfileStatus::Draft => ProfileStatusDto::Draft,
        ProfileStatus::Quarantined => ProfileStatusDto::Quarantined,
        ProfileStatus::Ready => ProfileStatusDto::Ready,
        ProfileStatus::InUse => ProfileStatusDto::InUse,
        ProfileStatus::DirtyLocal => ProfileStatusDto::DirtyLocal,
        ProfileStatus::Syncing => ProfileStatusDto::Syncing,
        ProfileStatus::Suspended => ProfileStatusDto::Suspended,
        ProfileStatus::Deleting => ProfileStatusDto::Deleting,
        ProfileStatus::Deleted => ProfileStatusDto::Deleted,
    }
}

#[cfg(test)]
mod tests {
    use super::profile_status;
    use application_ports::profiles::ProfileStatus;
    use control_plane_contract::profile_generation_api::ProfileStatusDto;

    #[test]
    fn domain_status_mapping_covers_every_public_profile_status() {
        for (domain, wire) in [
            (ProfileStatus::Draft, ProfileStatusDto::Draft),
            (ProfileStatus::Quarantined, ProfileStatusDto::Quarantined),
            (ProfileStatus::Ready, ProfileStatusDto::Ready),
            (ProfileStatus::InUse, ProfileStatusDto::InUse),
            (ProfileStatus::DirtyLocal, ProfileStatusDto::DirtyLocal),
            (ProfileStatus::Syncing, ProfileStatusDto::Syncing),
            (ProfileStatus::Suspended, ProfileStatusDto::Suspended),
            (ProfileStatus::Deleting, ProfileStatusDto::Deleting),
            (ProfileStatus::Deleted, ProfileStatusDto::Deleted),
        ] {
            assert_eq!(profile_status(domain), wire);
        }
    }
}
