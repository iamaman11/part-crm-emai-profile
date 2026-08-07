use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::mailbox_binding_application;
use application_ports::mailboxes::{MailboxBindingStatus, MailboxProvider};
use control_plane_contract::RouteClass;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, SecretHandle,
};
use serde::{Deserialize, Serialize};
use use_cases::mailboxes::{
    ExecuteCreateMailboxBindingCommand, ExecuteRevokeMailboxBindingCommand,
    MailboxBindingDetails, MailboxBindingMutationOutcome, MailboxBindingOperationError,
    authorize_mailbox_binding, execute_create_mailbox_binding, execute_revoke_mailbox_binding,
    get_mailbox_binding,
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

    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_mailbox_binding(role) {
        return operation_failure(actor.actor().correlation_id().as_str(), error);
    }

    match route {
        RouteClass::MailboxBindingCollectionApi => {
            create_binding(request, env, actor.actor(), role).await
        }
        RouteClass::MailboxBindingResourceApi => {
            let Some(binding_id) = parse_binding_id(&segments) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            get_binding(env, actor.actor(), role, &binding_id).await
        }
        RouteClass::MailboxBindingRevokeApi => {
            let Some(binding_id) = parse_binding_id(&segments) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            revoke_binding(request, env, actor.actor(), role, binding_id).await
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

fn parse_binding_id(segments: &[&str]) -> Option<MailboxBindingId> {
    segments
        .get(5)
        .and_then(|value| MailboxBindingId::parse((*value).to_owned()).ok())
}

async fn create_binding(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
) -> Result<Response> {
    let body = match request.json::<CreateMailboxBindingRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let binding_id = match MailboxBindingId::parse(body.binding_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let secret_handle = match SecretHandle::parse(body.secret_handle) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let provider = match MailboxProvider::parse_storage(&body.provider) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = mailbox_binding_application(env)?;
    match execute_create_mailbox_binding(
        actor,
        role,
        &application,
        ExecuteCreateMailboxBindingCommand::new(binding_id, provider, secret_handle, evidence),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 201),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn get_binding(
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: &MailboxBindingId,
) -> Result<Response> {
    let application = mailbox_binding_application(env)?;
    match get_mailbox_binding(actor, role, &application, binding_id).await {
        Ok(binding) => Response::from_json(&MailboxBindingResponse::from(&binding)),
        Err(MailboxBindingOperationError::NotFound) => {
            neutral_not_found(actor.correlation_id().as_str())
        }
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn revoke_binding(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: MailboxBindingId,
) -> Result<Response> {
    let body = match request.json::<RevokeMailboxBindingRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_version = match AggregateVersion::new(body.expected_binding_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = mailbox_binding_application(env)?;
    match execute_revoke_mailbox_binding(
        actor,
        role,
        &application,
        ExecuteRevokeMailboxBindingCommand::new(binding_id, expected_version, evidence),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 200),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn operation_failure(
    correlation_id: &str,
    error: MailboxBindingOperationError,
) -> Result<Response> {
    match error {
        MailboxBindingOperationError::NotFound => neutral_not_found(correlation_id),
        MailboxBindingOperationError::VersionConflict => problem(
            correlation_id,
            409,
            "version_conflict",
            "Version Conflict",
        ),
        MailboxBindingOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        MailboxBindingOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        MailboxBindingOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        MailboxBindingOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        MailboxBindingOperationError::DependencyUnavailable => problem(
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

fn mutation_receipt(outcome: &MailboxBindingMutationOutcome, status: u16) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.resource_id(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxBindingResponse<'a> {
    mailbox_binding_id: &'a str,
    provider: &'static str,
    status: &'static str,
    version: u64,
}

impl<'a> From<&'a MailboxBindingDetails> for MailboxBindingResponse<'a> {
    fn from(binding: &'a MailboxBindingDetails) -> Self {
        Self {
            mailbox_binding_id: binding.binding_id().as_str(),
            provider: binding.provider().storage_value(),
            status: match binding.status() {
                MailboxBindingStatus::Active => "ACTIVE",
                MailboxBindingStatus::Revoked => "REVOKED",
            },
            version: binding.version().value(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateMailboxBindingRequest {
    mailbox_binding_id: String,
    provider: String,
    secret_handle: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevokeMailboxBindingRequest {
    expected_binding_version: u64,
    request_digest: String,
}

#[cfg(test)]
mod tests {
    use super::{CreateMailboxBindingRequest, MailboxBindingResponse, MutationReceipt};

    #[test]
    fn binding_transport_rejects_sensitive_unknown_fields_and_keeps_camel_case()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(
            serde_json::from_str::<CreateMailboxBindingRequest>(
                r#"{"mailboxBindingId":"mailbox_01JTRANSPORT","provider":"IMAP","secretHandle":"secret_01JTRANSPORT","requestDigest":"digest_01JTRANSPORT","password":"forbidden"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<CreateMailboxBindingRequest>(
                r#"{"mailboxBindingId":"mailbox_01JTRANSPORT","provider":"IMAP","secretHandle":"secret_01JTRANSPORT","requestDigest":"digest_01JTRANSPORT","messageBody":"forbidden"}"#
            )
            .is_err()
        );

        let mutation = serde_json::to_value(MutationReceipt {
            result_code: "created",
            resource_id: "mailbox_01JTRANSPORT",
            aggregate_version: 1,
        })?;
        assert!(mutation.get("resultCode").is_some());
        assert!(mutation.get("resourceId").is_some());
        assert!(mutation.get("aggregateVersion").is_some());

        let response = serde_json::to_value(MailboxBindingResponse {
            mailbox_binding_id: "mailbox_01JTRANSPORT",
            provider: "IMAP",
            status: "ACTIVE",
            version: 1,
        })?;
        assert!(response.get("mailboxBindingId").is_some());
        assert!(response.get("provider").is_some());
        assert!(response.get("status").is_some());
        assert!(response.get("version").is_some());
        assert!(response.get("secretHandle").is_none());
        Ok(())
    }
}
