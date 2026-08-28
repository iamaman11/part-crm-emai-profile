use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::{
    authenticated_device, device_execution_preconditions, device_job_authorization,
};
use control_plane_contract::profile_launch_api::ProfileLaunchProjection;
use profile_platform_primitives::ProfileId;
use serde::Serialize;
use use_cases::profile_launch::authorize_profile_launch;
use use_cases::profile_launch_authority::issue_profile_launch_authority;
use use_cases::{ApplicationError, ProblemCode};
use worker::{Env, Request, Response, Result};

use super::profile_launch_composition::{launch_authority, launch_context};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileLaunchCommandEvidence<'a> {
    profile_id: &'a str,
}

pub(super) async fn launch(
    request: &Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    let Some(resolved) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let actor = resolved.actor();
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(
        request,
        actor,
        &ProfileLaunchCommandEvidence {
            profile_id: profile_id.as_str(),
        },
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };

    let context = launch_context(env)?;
    let device = authenticated_device(env)?;
    let authorization = device_job_authorization(env)?;
    let preconditions = device_execution_preconditions(env)?;
    let target = match authorize_profile_launch(
        actor,
        membership_role(&resolved),
        &profile_id,
        &context,
        &device,
        &authorization,
        &preconditions,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return application_failure(actor.correlation_id().as_str(), error),
    };

    let authority = launch_authority(env)?;
    let issued = match issue_profile_launch_authority(actor, &target, &evidence, &authority).await {
        Ok(value) => value,
        Err(error) => return application_failure(actor.correlation_id().as_str(), error),
    };
    let mut response = Response::from_json(&ProfileLaunchProjection {
        launch_uri: format!("profilebridge://claim/{}", issued.claim_code()),
        expires_at_ms: issued.expires_at().value(),
    })?;
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    Ok(response)
}

fn application_failure(correlation_id: &str, error: ApplicationError) -> Result<Response> {
    match error.code() {
        ProblemCode::NotFound | ProblemCode::Forbidden => neutral_not_found(correlation_id),
        ProblemCode::InvalidRequest => invalid_request(correlation_id),
        ProblemCode::InvalidState => problem(correlation_id, 409, "invalid_state", "Invalid State"),
        ProblemCode::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        ProblemCode::LeaseConflict => {
            problem(correlation_id, 409, "lease_conflict", "Lease Conflict")
        }
        ProblemCode::ReplayRejected => {
            problem(correlation_id, 409, "replay_rejected", "Replay Rejected")
        }
        ProblemCode::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
        ProblemCode::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        ProblemCode::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

#[cfg(test)]
mod tests {
    use super::ProfileLaunchCommandEvidence;

    #[test]
    fn launch_command_evidence_contains_no_device_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let value = serde_json::to_value(ProfileLaunchCommandEvidence {
            profile_id: "profile_01JLAUNCH",
        })?;
        assert_eq!(value["profileId"], "profile_01JLAUNCH");
        assert!(value.get("deviceId").is_none());
        assert!(value.get("generationId").is_none());
        Ok(())
    }
}
