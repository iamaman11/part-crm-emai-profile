use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
    verify_access_assertion,
};
use crate::command_evidence;
use crate::composition::{
    authenticated_device, device_execution_preconditions, device_job_authorization,
};
use application_ports::DeviceJobPortErrorClass;
use cloudflare_adapters::d1_active_membership::D1ActiveMembership;
use cloudflare_adapters::d1_authenticated_device::D1AuthenticatedDevice;
use control_plane_contract::D1_CATALOG_BINDING;
use control_plane_contract::profile_launch_api::ProfileLaunchProjection;
use profile_platform_primitives::{CorrelationId, ProfileId, UnixMillis};
use serde::{Deserialize, Serialize};
use use_cases::profile_launch::authorize_profile_launch;
use use_cases::profile_launch_authority::issue_profile_launch_authority;
use use_cases::profile_launch_redemption::redeem_profile_launch_authority;
use use_cases::{ApplicationError, ProblemCode};
use worker::{Date, Env, Request, Response, Result};

use super::profile_launch_composition::{launch_authority, launch_context};

const BRIDGE_ACCESS_AUDIENCE_VAR: &str = "BRIDGE_ACCESS_AUDIENCE";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileLaunchCommandEvidence<'a> {
    profile_id: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BridgeProfileLaunchRedemptionRequest {
    claim_code: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeProfileLaunchRedemptionProjection {
    tenant_id: String,
    actor_id: String,
    profile_id: String,
    generation_id: String,
    device_id: String,
}

pub(super) async fn launch(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    // Route classification admits exactly one POST Bridge path as ProfileLaunchApi. Keeping the
    // authentication split inside the semantic launch owner prevents a second redemption owner.
    if request.path().starts_with("/bridge/") {
        return redeem_from_bridge(request, env).await;
    }
    issue_for_operator(request, env, tenant_id, profile_id).await
}

async fn issue_for_operator(
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

async fn redeem_from_bridge(request: &mut Request, env: &Env) -> Result<Response> {
    let correlation_value = correlation_hint(request);
    let correlation_id = match CorrelationId::parse(correlation_value.clone()) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(&correlation_value),
    };

    // Access is the perimeter proof for the dedicated Bridge audience; it is not the device
    // identity owner. The actual machine must additionally prove possession of its client
    // certificate at the TLS edge. Both checks happen before body parsing and before the launch
    // authority adapter exists, so failed machine authentication cannot probe claim existence.
    let Some(_access_identity) =
        verify_access_assertion(request, env, BRIDGE_ACCESS_AUDIENCE_VAR).await?
    else {
        return neutral_not_found(correlation_id.as_str());
    };
    let Some(machine_fingerprint) = verified_mtls_fingerprint(request) else {
        return neutral_not_found(correlation_id.as_str());
    };

    let machine_device = D1AuthenticatedDevice::new(env.d1(D1_CATALOG_BINDING)?);
    let machine_binding = match machine_device
        .resolve_machine_certificate_fingerprint(&machine_fingerprint)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return neutral_not_found(correlation_id.as_str()),
        Err(error) => return machine_identity_failure(correlation_id.as_str(), error.class()),
    };

    let body = match request.json::<BridgeProfileLaunchRedemptionRequest>().await {
        Ok(value) => value,
        Err(_) => return neutral_not_found(correlation_id.as_str()),
    };

    let memberships = D1ActiveMembership::new(env.d1(D1_CATALOG_BINDING)?);
    let authority = launch_authority(env)?;
    let context = launch_context(env)?;
    let device = authenticated_device(env)?;
    let authorization = device_job_authorization(env)?;
    let preconditions = device_execution_preconditions(env)?;
    let now = UnixMillis::new(Date::now().as_millis());
    let redeemed = match redeem_profile_launch_authority(
        &correlation_id,
        &body.claim_code,
        &machine_binding,
        now,
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

    let mut response = Response::from_json(&BridgeProfileLaunchRedemptionProjection {
        tenant_id: redeemed.tenant_id().as_str().to_owned(),
        actor_id: redeemed.actor_id().as_str().to_owned(),
        profile_id: redeemed.profile_id().as_str().to_owned(),
        generation_id: redeemed.generation_id().as_str().to_owned(),
        device_id: redeemed.device_id().as_str().to_owned(),
    })?;
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    Ok(response)
}

fn verified_mtls_fingerprint(request: &Request) -> Option<String> {
    let tls = request.cf()?.tls_client_auth()?;
    if tls.cert_presented() != "1" || tls.cert_verified() != "SUCCESS" {
        return None;
    }
    normalize_sha256_fingerprint(&tls.cert_fingerprint_sha256())
}

fn normalize_sha256_fingerprint(value: &str) -> Option<String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn machine_identity_failure(
    correlation_id: &str,
    class: DeviceJobPortErrorClass,
) -> Result<Response> {
    match class {
        DeviceJobPortErrorClass::AuthenticationFailed => neutral_not_found(correlation_id),
        DeviceJobPortErrorClass::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        DeviceJobPortErrorClass::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
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
    use super::{
        BRIDGE_ACCESS_AUDIENCE_VAR, ProfileLaunchCommandEvidence, normalize_sha256_fingerprint,
    };

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

    #[test]
    fn bridge_machine_auth_uses_a_dedicated_access_audience() {
        assert_eq!(BRIDGE_ACCESS_AUDIENCE_VAR, "BRIDGE_ACCESS_AUDIENCE");
        assert_ne!(BRIDGE_ACCESS_AUDIENCE_VAR, "ACCESS_AUDIENCE");
    }

    #[test]
    fn bridge_device_identity_accepts_only_exact_sha256_certificate_fingerprint() {
        let lower = "a1".repeat(32);
        let upper = lower.to_ascii_uppercase();
        assert_eq!(normalize_sha256_fingerprint(&lower), Some(lower.clone()));
        assert_eq!(normalize_sha256_fingerprint(&upper), Some(lower));
        assert_eq!(normalize_sha256_fingerprint(&"a".repeat(63)), None);
        assert_eq!(normalize_sha256_fingerprint(&"g".repeat(64)), None);
        assert_eq!(normalize_sha256_fingerprint(&format!("{}:", "a".repeat(63))), None);
    }
}
