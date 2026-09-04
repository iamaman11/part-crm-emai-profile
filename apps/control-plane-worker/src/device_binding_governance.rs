use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::identity_governance_application;
use control_plane_contract::RouteClass;
use profile_platform_primitives::{
    ActorId, AggregateVersion, DeviceId, MachineCertificateFingerprint,
};
use serde::{Deserialize, Serialize};
use use_cases_devices::{
    DeviceBindingMutationOutcome, DeviceBindingOperationError, ExecuteDeviceBindCommand,
    ExecuteDeviceRevokeCommand, execute_device_bind, execute_device_revoke,
};
use use_cases_identity::identity_governance::authorize_identity_governance;
use worker::{Env, Request, Response, Result};

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let target_actor_id = segments.get(5).copied().unwrap_or_default();

    match route {
        RouteClass::DeviceBindingResourceApi => {
            bind_device(request, env, tenant_id, target_actor_id).await
        }
        RouteClass::DeviceBindingRevokeApi => {
            revoke_device(request, env, tenant_id, target_actor_id).await
        }
        _ => neutral_not_found(&correlation_hint(request)),
    }
}

async fn bind_device(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    if authorize_identity_governance(membership_role(&actor)).is_err() {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    }

    let body = match request.json::<DeviceBindingWriteRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let target_actor_id = match ActorId::parse(target_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let device_id = match DeviceId::parse(body.device_id.clone()) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    if !is_canonical_sha256_hex(&body.certificate_fingerprint) {
        return invalid_request(actor.actor().correlation_id().as_str());
    }
    let certificate_fingerprint =
        match MachineCertificateFingerprint::parse(body.certificate_fingerprint.clone()) {
            Ok(value) => value,
            Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
        };
    let expected_previous_version = match body
        .expected_previous_version
        .map(AggregateVersion::new)
        .transpose()
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), &body) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = identity_governance_application(env)?;
    match execute_device_bind(
        actor.actor(),
        &application,
        ExecuteDeviceBindCommand::new(
            target_actor_id,
            device_id,
            certificate_fingerprint,
            expected_previous_version,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome),
        Err(error) => device_binding_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn revoke_device(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    if authorize_identity_governance(membership_role(&actor)).is_err() {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    }

    let body = match request.json::<DeviceBindingRevokeRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let target_actor_id = match ActorId::parse(target_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_version = match AggregateVersion::new(body.expected_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), &body) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = identity_governance_application(env)?;
    match execute_device_revoke(
        actor.actor(),
        &application,
        ExecuteDeviceRevokeCommand::new(target_actor_id, expected_version, evidence),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome),
        Err(error) => device_binding_failure(actor.actor().correlation_id().as_str(), error),
    }
}

fn is_canonical_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn device_binding_failure(
    correlation_id: &str,
    error: DeviceBindingOperationError,
) -> Result<Response> {
    match error {
        DeviceBindingOperationError::InvalidRequest => invalid_request(correlation_id),
        DeviceBindingOperationError::NotFound => neutral_not_found(correlation_id),
        DeviceBindingOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        DeviceBindingOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        DeviceBindingOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        DeviceBindingOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        DeviceBindingOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        DeviceBindingOperationError::DependencyUnavailable => problem(
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

fn mutation_receipt(outcome: &DeviceBindingMutationOutcome) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.actor_id().as_str(),
        aggregate_version: outcome.binding_version().value(),
    })
    .map(|response| response.with_status(200))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceBindingWriteRequest {
    device_id: String,
    certificate_fingerprint: String,
    expected_previous_version: Option<u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceBindingRevokeRequest {
    expected_version: u64,
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceBindingRevokeRequest, DeviceBindingWriteRequest, MutationReceipt,
        is_canonical_sha256_hex,
    };

    #[test]
    fn transport_models_are_strict_and_contain_only_non_secret_binding_material()
    -> Result<(), Box<dyn std::error::Error>> {
        let initial: DeviceBindingWriteRequest = serde_json::from_str(
            r#"{
                "deviceId":"device_01JDEVICEBIND",
                "certificateFingerprint":"abababababababababababababababababababababababababababababababab"
            }"#,
        )?;
        assert_eq!(initial.expected_previous_version, None);

        let rebind: DeviceBindingWriteRequest = serde_json::from_str(
            r#"{
                "deviceId":"device_01JDEVICEBIND2",
                "certificateFingerprint":"cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                "expectedPreviousVersion":3
            }"#,
        )?;
        assert_eq!(rebind.expected_previous_version, Some(3));

        for forbidden in [
            "privateKey",
            "certificatePem",
            "certificateDer",
            "pfx",
            "pkcs12",
        ] {
            let payload = format!(
                r#"{{"deviceId":"device_01JDEVICEBIND","certificateFingerprint":"{}","{forbidden}":"secret"}}"#,
                "ab".repeat(32)
            );
            assert!(serde_json::from_str::<DeviceBindingWriteRequest>(&payload).is_err());
        }

        let revoke: DeviceBindingRevokeRequest = serde_json::from_str(r#"{"expectedVersion":4}"#)?;
        assert_eq!(revoke.expected_version, 4);
        assert!(
            serde_json::from_str::<DeviceBindingRevokeRequest>(
                r#"{"expectedVersion":4,"deviceId":"device_01JDEVICEBIND"}"#
            )
            .is_err()
        );

        let receipt = serde_json::to_value(MutationReceipt {
            result_code: "bound",
            resource_id: "actor_01JDEVICETARGET",
            aggregate_version: 1,
        })?;
        assert_eq!(receipt["resultCode"], "bound");
        assert_eq!(receipt["resourceId"], "actor_01JDEVICETARGET");
        assert_eq!(receipt["aggregateVersion"], 1);
        Ok(())
    }

    #[test]
    fn certificate_fingerprint_wire_form_is_exact_lowercase_sha256_hex() {
        assert!(is_canonical_sha256_hex(&"ab".repeat(32)));
        assert!(!is_canonical_sha256_hex(&"AB".repeat(32)));
        assert!(!is_canonical_sha256_hex(&"a".repeat(63)));
        assert!(!is_canonical_sha256_hex(&"g".repeat(64)));
    }
}
