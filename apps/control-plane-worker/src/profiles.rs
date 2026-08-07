use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::profile_application;
use application_ports::profiles::ProfileStatus;
use control_plane_contract::RouteClass;
use profile_platform_primitives::{ClientId, ProfileId};
use serde::{Deserialize, Serialize};
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
        Ok(profile) => Response::from_json(&ProfileResponse::from(&profile)),
        Err(ProfileOperationError::NotFound) => {
            neutral_not_found(actor.actor().correlation_id().as_str())
        }
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
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

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MutationReceipt<'a> {
    result_code: &'a str,
    resource_id: &'a str,
    aggregate_version: u64,
}

fn mutation_receipt(outcome: &ProfileMutationOutcome) -> Result<Response> {
    let status = if outcome.replayed() { 200 } else { 201 };
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.resource_id(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileResponse<'a> {
    profile_id: &'a str,
    status: &'static str,
    version: u64,
    linked_client_id: Option<&'a str>,
}

impl<'a> From<&'a ProfileDetails> for ProfileResponse<'a> {
    fn from(profile: &'a ProfileDetails) -> Self {
        Self {
            profile_id: profile.profile_id().as_str(),
            status: match profile.status() {
                ProfileStatus::Draft => "DRAFT",
                ProfileStatus::Quarantined => "QUARANTINED",
                ProfileStatus::Ready => "READY",
                ProfileStatus::InUse => "IN_USE",
                ProfileStatus::DirtyLocal => "DIRTY_LOCAL",
                ProfileStatus::Syncing => "SYNCING",
                ProfileStatus::Suspended => "SUSPENDED",
                ProfileStatus::Deleting => "DELETING",
                ProfileStatus::Deleted => "DELETED",
            },
            version: profile.version().value(),
            linked_client_id: profile.linked_client_id().map(ClientId::as_str),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileCreateRequest {
    profile_id: String,
    request_digest: String,
}

#[cfg(test)]
mod tests {
    use super::{MutationReceipt, ProfileResponse};

    #[test]
    fn transport_models_keep_camel_case_contract_field_names()
    -> Result<(), Box<dyn std::error::Error>> {
        let mutation = serde_json::to_value(MutationReceipt {
            result_code: "created",
            resource_id: "profile_01JTRANSPORT",
            aggregate_version: 1,
        })?;
        assert!(mutation.get("resultCode").is_some());
        assert!(mutation.get("resourceId").is_some());
        assert!(mutation.get("aggregateVersion").is_some());

        let response = serde_json::to_value(ProfileResponse {
            profile_id: "profile_01JTRANSPORT",
            status: "DRAFT",
            version: 1,
            linked_client_id: Some("client_01JTRANSPORT"),
        })?;
        assert!(response.get("profileId").is_some());
        assert!(response.get("linkedClientId").is_some());
        Ok(())
    }
}
