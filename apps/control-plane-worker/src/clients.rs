use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::client_application;
use client_domain::{ClientKind, ClientStatus};
use control_plane_contract::RouteClass;
use profile_platform_primitives::{ActorId, AggregateVersion, ClientId};
use serde::{Deserialize, Serialize};
use use_cases::client_grants::{
    ClientGrantAction, ClientGrantOperationError, ClientGrantOutcome, ExecuteClientGrantCommand,
    authorize_client_grant, execute_client_grant,
};
use use_cases::clients::{
    ClientDetails, ClientMutationOutcome, ClientOperationError, ExecuteCreateClientCommand,
    authorize_client_create, execute_create_client, get_visible_client,
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
        RouteClass::ClientCollectionApi => create_client(request, env, tenant_id).await,
        RouteClass::ClientResourceApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            get_client(request, env, tenant_id, client_id).await
        }
        RouteClass::ClientGrantApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            let actor_id = segments.get(7).copied().unwrap_or_default();
            update_client_grant(request, env, tenant_id, client_id, actor_id).await
        }
        _ => neutral_not_found(&correlation_hint(request)),
    }
}

async fn create_client(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_client_create(role) {
        return operation_failure(actor.actor().correlation_id().as_str(), error);
    }
    let body = match request.json::<ClientCreateRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let client_id = match ClientId::parse(body.client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let kind = match body.kind.as_str() {
        "PERSON" => ClientKind::Person,
        "ORGANIZATION" => ClientKind::Organization,
        _ => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = client_application(env)?;
    match execute_create_client(
        actor.actor(),
        role,
        &application,
        ExecuteCreateClientCommand::new(client_id, kind, body.display_name, evidence),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn get_client(
    request: &Request,
    env: &Env,
    tenant_id: &str,
    client_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let application = client_application(env)?;
    match get_visible_client(
        actor.actor(),
        membership_role(&actor),
        &application,
        &client_id,
    )
    .await
    {
        Ok(client) => Response::from_json(&ClientResponse::from(&client)),
        Err(ClientOperationError::NotFound) => {
            neutral_not_found(actor.actor().correlation_id().as_str())
        }
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn update_client_grant(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    client_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_client_grant(role) {
        return client_grant_failure(actor.actor().correlation_id().as_str(), error);
    }

    let body = match request.json::<ClientGrantRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let target_actor_id = match ActorId::parse(target_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_client_version = match AggregateVersion::new(body.expected_client_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let action = if request.method().as_ref() == "DELETE" {
        ClientGrantAction::Revoke
    } else {
        ClientGrantAction::Grant
    };
    let application = client_application(env)?;
    match execute_client_grant(
        actor.actor(),
        role,
        &application,
        action,
        ExecuteClientGrantCommand::new(
            target_actor_id,
            client_id,
            expected_client_version,
            body.role,
            body.reason,
            evidence,
        ),
    )
    .await
    {
        Ok(_) if action == ClientGrantAction::Revoke => no_content(),
        Ok(outcome) => client_grant_receipt(&outcome),
        Err(error) => client_grant_failure(actor.actor().correlation_id().as_str(), error),
    }
}

fn operation_failure(correlation_id: &str, error: ClientOperationError) -> Result<Response> {
    match error {
        ClientOperationError::NotFound => neutral_not_found(correlation_id),
        ClientOperationError::InvalidRequest => {
            problem(correlation_id, 400, "invalid_request", "Invalid Request")
        }
        ClientOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ClientOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        ClientOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        ClientOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn client_grant_failure(
    correlation_id: &str,
    error: ClientGrantOperationError,
) -> Result<Response> {
    match error {
        ClientGrantOperationError::InvalidRequest => invalid_request(correlation_id),
        ClientGrantOperationError::NotFound => neutral_not_found(correlation_id),
        ClientGrantOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        ClientGrantOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        ClientGrantOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ClientGrantOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        ClientGrantOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        ClientGrantOperationError::DependencyUnavailable => problem(
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
    Response::empty().map(|response| response.with_status(204))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MutationReceipt<'a> {
    result_code: &'a str,
    resource_id: &'a str,
    aggregate_version: u64,
}

fn mutation_receipt(outcome: &ClientMutationOutcome) -> Result<Response> {
    let status = if outcome.replayed() { 200 } else { 201 };
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.resource_id(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

fn client_grant_receipt(outcome: &ClientGrantOutcome) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.resource_id(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(200))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientResponse<'a> {
    client_id: &'a str,
    kind: &'static str,
    display_name: &'a str,
    status: &'static str,
    version: u64,
}

impl<'a> From<&'a ClientDetails> for ClientResponse<'a> {
    fn from(client: &'a ClientDetails) -> Self {
        Self {
            client_id: client.client_id().as_str(),
            kind: match client.kind() {
                ClientKind::Person => "PERSON",
                ClientKind::Organization => "ORGANIZATION",
            },
            display_name: client.display_name(),
            status: match client.status() {
                ClientStatus::Active => "ACTIVE",
                ClientStatus::Archived => "ARCHIVED",
                ClientStatus::Merged => "MERGED",
            },
            version: client.version().value(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientCreateRequest {
    client_id: String,
    kind: String,
    display_name: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientGrantRequest {
    role: String,
    reason: String,
    expected_client_version: u64,
    request_digest: String,
}

#[cfg(test)]
mod tests {
    use super::{ClientGrantRequest, ClientResponse, MutationReceipt};

    #[test]
    fn transport_models_keep_camel_case_contract_field_names()
    -> Result<(), Box<dyn std::error::Error>> {
        let mutation = serde_json::to_value(MutationReceipt {
            result_code: "created",
            resource_id: "client_01JTRANSPORT",
            aggregate_version: 1,
        })?;
        assert!(mutation.get("resultCode").is_some());
        assert!(mutation.get("resourceId").is_some());
        assert!(mutation.get("aggregateVersion").is_some());

        let response = serde_json::to_value(ClientResponse {
            client_id: "client_01JTRANSPORT",
            kind: "PERSON",
            display_name: "Client",
            status: "ACTIVE",
            version: 1,
        })?;
        assert!(response.get("clientId").is_some());
        assert!(response.get("displayName").is_some());
        Ok(())
    }

    #[test]
    fn client_grant_request_preserves_legacy_unknown_field_tolerance() {
        let payload = r#"{
            "role":"CLIENT_VIEWER",
            "reason":"legacy-compatible",
            "expectedClientVersion":1,
            "requestDigest":"request-digest-01JCLIENTTRANSPORT",
            "legacyIgnoredField":"still-tolerated"
        }"#;
        assert!(serde_json::from_str::<ClientGrantRequest>(payload).is_ok());
    }
}
