mod profile_launch_coordinator;

use self::profile_launch_coordinator::ensure_bridge_launch_intent;
use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::bridge_machine::resolve_bridge_machine;
use crate::composition::{
    authenticated_device, device_execution_preconditions, device_job_authorization,
};
use cloudflare_adapters::d1_active_membership::D1ActiveMembership;
use control_plane_contract::D1_CATALOG_BINDING;
use control_plane_contract::profile_launch_api::{
    BridgeProfileLaunchRedemptionProjection, BridgeProfileLaunchRedemptionRequest,
    ProfileLaunchIntentProjectionV2, ProfileLaunchProjection,
};
use profile_platform_primitives::{CorrelationId, DeviceId, ProfileId, UnixMillis};
use use_cases::profile_launch::authorize_profile_launch;
use use_cases::profile_launch_authority::issue_profile_launch_authority;
use use_cases::profile_launch_redemption::{
    ProfileLaunchRedemptionInput, consume_validated_profile_launch_redemption,
    validate_profile_launch_redemption,
};
use use_cases::{ApplicationError, ProblemCode};
use worker::{Date, Env, Request, Response, Result};

use super::profile_launch_composition::{launch_authority, launch_context};

pub(super) async fn launch(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    if request.path().starts_with("/bridge/") {
        return redeem_from_bridge(request, env).await;
    }
    issue_for_operator(request, env, tenant_id, profile_id).await
}

async fn issue_for_operator(
    request: &mut Request,
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
    let application = crate::composition::profile_generation_application(env)?;
    let context = match use_cases::generation_initialization::authorize_initial_generation_access(
        actor,
        membership_role(&resolved),
        &profile_id,
        &application,
    )
    .await
    {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.correlation_id().as_str()),
    };
    if !matches!(
        context.status,
        application_ports::profiles::ProfileStatus::Draft
            | application_ports::profiles::ProfileStatus::Ready
    ) {
        return problem(
            actor.correlation_id().as_str(),
            409,
            "invalid_state",
            "Profile is not ready to open",
        );
    }
    let mut response = Response::from_json(&ProfileLaunchIntentProjectionV2 {
        schema_version: 2,
        launch_uri: format!(
            "profilebridge://open/v1/{}/{}/{}/{}",
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            profile_id.as_str(),
            context.version.value()
        ),
    })?;
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    Ok(response)
}

/// Both browser launch and automatic first preparation use the same authorization/claim owner.
pub(crate) async fn issue_for_device(
    env: &Env,
    actor: &profile_platform_primitives::ActorContext,
    role: identity_access_domain::MembershipRole,
    profile_id: &ProfileId,
    selected_device: &DeviceId,
    evidence: &application_ports::CommandExecutionEvidence,
) -> Result<core::result::Result<ProfileLaunchProjection, ApplicationError>> {
    let context = launch_context(env)?;
    let device = authenticated_device(env)?.select_device(selected_device.clone());
    let authorization = device_job_authorization(env)?;
    if authorization
        .admit_profile_device(
            actor,
            profile_id,
            selected_device,
            UnixMillis::new(Date::now().as_millis()),
        )
        .await
        .is_err()
    {
        return Ok(Err(ApplicationError::new(
            ProblemCode::DependencyUnavailable,
        )));
    }
    let preconditions = device_execution_preconditions(env)?;
    let target = match authorize_profile_launch(
        actor,
        role,
        profile_id,
        &context,
        &device,
        &authorization,
        &preconditions,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    let authority = launch_authority(env)?;
    let issued = match issue_profile_launch_authority(actor, &target, evidence, &authority).await {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    Ok(Ok(ProfileLaunchProjection {
        launch_uri: format!("profilebridge://claim/{}", issued.claim_code()),
        expires_at_ms: issued.expires_at().value(),
    }))
}

async fn redeem_from_bridge(request: &mut Request, env: &Env) -> Result<Response> {
    let correlation_value = correlation_hint(request);
    let correlation_id = match CorrelationId::parse(correlation_value.clone()) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(&correlation_value),
    };

    let Some(machine_binding) = resolve_bridge_machine(request, env, &correlation_id).await? else {
        return neutral_not_found(correlation_id.as_str());
    };
    let body = match request.json::<BridgeProfileLaunchRedemptionRequest>().await {
        Ok(value) => value,
        Err(_) => return neutral_not_found(correlation_id.as_str()),
    };

    let memberships = D1ActiveMembership::new(env.d1(D1_CATALOG_BINDING)?);
    let authority = launch_authority(env)?;
    let context = launch_context(env)?;
    let device = authenticated_device(env)?.select_device(machine_binding.device_id().clone());
    let authorization = device_job_authorization(env)?;
    let preconditions = device_execution_preconditions(env)?;
    let now = UnixMillis::new(Date::now().as_millis());
    let validated = match validate_profile_launch_redemption(
        ProfileLaunchRedemptionInput::new(
            &correlation_id,
            body.claim_code(),
            &machine_binding,
            now,
        ),
        &memberships,
        &authority,
        &context,
        &device,
        &authorization,
        &preconditions,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return application_failure(correlation_id.as_str(), error),
    };

    let launch_intent_id = match ensure_bridge_launch_intent(
        env,
        validated.actor(),
        validated.role(),
        validated.binding().profile_id(),
        validated.binding().device_id(),
        body.claim_code(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return application_failure(correlation_id.as_str(), error),
    };

    let revalidated_at = UnixMillis::new(Date::now().as_millis());
    let revalidated = match validate_profile_launch_redemption(
        ProfileLaunchRedemptionInput::new(
            &correlation_id,
            body.claim_code(),
            &machine_binding,
            revalidated_at,
        ),
        &memberships,
        &authority,
        &context,
        &device,
        &authorization,
        &preconditions,
    )
    .await
    {
        Ok(value) if value.binding() == validated.binding() => value,
        Ok(_) => return neutral_not_found(correlation_id.as_str()),
        Err(error) => return application_failure(correlation_id.as_str(), error),
    };

    let redeemed = match consume_validated_profile_launch_redemption(
        &revalidated,
        body.claim_code(),
        revalidated_at,
        &authority,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return application_failure(correlation_id.as_str(), error),
    };

    let mut response = Response::from_json(&BridgeProfileLaunchRedemptionProjection {
        tenant_id: redeemed.tenant_id().as_str().to_owned(),
        actor_id: redeemed.actor_id().as_str().to_owned(),
        profile_id: redeemed.profile_id().as_str().to_owned(),
        generation_id: redeemed.generation_id().as_str().to_owned(),
        device_id: redeemed.device_id().as_str().to_owned(),
        launch_intent_id: launch_intent_id.as_str().to_owned(),
    })?;
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    Ok(response)
}

pub(crate) fn application_failure(
    correlation_id: &str,
    error: ApplicationError,
) -> Result<Response> {
    match error.code() {
        ProblemCode::NotFound | ProblemCode::Forbidden => neutral_not_found(correlation_id),
        ProblemCode::InvalidRequest => {
            problem(correlation_id, 400, "invalid_request", "Invalid Request")
        }
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

#[cfg(test)]
mod tests {
    use super::ProfileLaunchIntentProjectionV2;

    #[test]
    fn browser_intent_contains_no_device_generation_or_launch_credential()
    -> Result<(), Box<dyn std::error::Error>> {
        let value = serde_json::to_value(ProfileLaunchIntentProjectionV2 {
            schema_version: 2,
            launch_uri:
                "profilebridge://open/v1/tenant_01JLAUNCH/actor_01JLAUNCH/profile_01JLAUNCH/1"
                    .to_owned(),
        })?;
        assert_eq!(value["schemaVersion"], 2);
        assert!(value.get("deviceId").is_none());
        assert!(value.get("generationId").is_none());
        assert!(value.get("claimCode").is_none());
        assert!(value.get("expiresAtMs").is_none());
        assert!(
            !value["launchUri"]
                .as_str()
                .unwrap_or_default()
                .contains("claim")
        );
        Ok(())
    }
}
