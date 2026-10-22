use super::{application_authority, path_device_id, path_tenant_id};
use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use control_plane_contract::device_application_api::{
    DeviceRegistrationProjection, DeviceRegistrationStatus,
};
use worker::{Env, Request, Response, Result};

/// An authenticated, actor-scoped, read-only projection of the existing device authority.
/// A browser callback merely supplies the resource locator; it never supplies a readiness verdict.
pub(super) async fn get_registration(request: &Request, env: &Env) -> Result<Response> {
    let Some(tenant_id) = path_tenant_id(request) else {
        return neutral_not_found(&correlation_hint(request));
    };
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id.as_str())).await?
    else {
        return neutral_not_found(&correlation_hint(request));
    };
    let correlation = actor.actor().correlation_id().as_str();
    let Some(device_id) = path_device_id(request) else {
        return neutral_not_found(correlation);
    };
    let authority = match application_authority(env) {
        Ok(value) => value,
        Err(_) => return registration_unavailable(correlation),
    };
    let device = match authority
        .resolve_owned_active_device(actor.actor(), &device_id)
        .await
    {
        Ok(Some(device)) => device,
        Ok(None) => return neutral_not_found(correlation),
        Err(_) => return registration_unavailable(correlation),
    };
    let mut response = Response::from_json(&DeviceRegistrationProjection {
        tenant_id: tenant_id.as_str().to_owned(),
        actor_id: device.actor_id().as_str().to_owned(),
        device_id: device.device_id().as_str().to_owned(),
        binding_version: device.binding_version().value(),
        status: DeviceRegistrationStatus::Registered,
    })?;
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}

fn registration_unavailable(correlation: &str) -> Result<Response> {
    // The canonical browser Problem uses internal_failure for unavailable infrastructure; the
    // response stays HTTP503 and preserves correlation. No raw provider error enters the response.
    problem(
        correlation,
        503,
        "internal_failure",
        "Device registration unavailable",
    )
}
