use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::{browser_mailbox_execution_application, mailbox_binding_application};
use application_ports::mailboxes::MailboxProvider;
use control_plane_contract::RouteClass;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, ProfileId, SecretHandle,
};
use serde::{Deserialize, Serialize};
use use_cases::browser_execution::{
    BindBrowserMailboxExecutionCommand, BrowserMailboxExecutionBindingOutcome,
    execute_bind_browser_mailbox_execution,
};
use use_cases::mailboxes::{
    ExecuteCreateMailboxBindingCommand, ExecuteRevokeMailboxBindingCommand, MailboxBindingDetails,
    MailboxBindingMutationOutcome, MailboxBindingOperationError, authorize_mailbox_binding,
    execute_create_mailbox_binding, execute_revoke_mailbox_binding, get_mailbox_binding,
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
        RouteClass::MailboxBrowserExecutionBindApi => {
            let Some(binding_id) = parse_binding_id(&segments) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            bind_browser_execution(request, env, actor.actor(), role, binding_id).await
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
    if !valid_digest(&body.request_digest) {
        return invalid_request(actor.correlation_id().as_str());
    }
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
    if !valid_digest(&body.request_digest) {
        return invalid_request(actor.correlation_id().as_str());
    }
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

async fn bind_browser_execution(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: MailboxBindingId,
) -> Result<Response> {
    let body = match request.json::<BindBrowserMailboxExecutionRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let profile_id = match ProfileId::parse(body.profile_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if !valid_digest(&body.request_digest) {
        return invalid_request(actor.correlation_id().as_str());
    }
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = browser_mailbox_execution_application(env)?;
    match execute_bind_browser_mailbox_execution(
        actor,
        role,
        &application,
        BindBrowserMailboxExecutionCommand::new(binding_id, profile_id, evidence),
    )
    .await
    {
        Ok(outcome) => browser_execution_receipt(&outcome),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn operation_failure(
    correlation_id: &str,
    error: MailboxBindingOperationError,
) -> Result<Response> {
    match error {
        MailboxBindingOperationError::NotFound => neutral_not_found(correlation_id),
        MailboxBindingOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
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

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
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
struct BrowserExecutionBindingReceipt<'a> {
    binding_id: &'a str,
    profile_id: &'a str,
    replayed: bool,
}

fn browser_execution_receipt(outcome: &BrowserMailboxExecutionBindingOutcome) -> Result<Response> {
    Response::from_json(&BrowserExecutionBindingReceipt {
        binding_id: outcome.binding_id().as_str(),
        profile_id: outcome.profile_id().as_str(),
        replayed: outcome.replayed(),
    })
    .map(|response| response.with_status(201))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxBindingResponse<'a> {
    binding_id: &'a str,
    provider: &'static str,
    status: &'static str,
    version: u64,
}

impl<'a> From<&'a MailboxBindingDetails> for MailboxBindingResponse<'a> {
    fn from(binding: &'a MailboxBindingDetails) -> Self {
        Self {
            binding_id: binding.binding_id().as_str(),
            provider: binding.provider().storage_value(),
            status: binding.status().storage_value(),
            version: binding.version().value(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateMailboxBindingRequest {
    binding_id: String,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BindBrowserMailboxExecutionRequest {
    profile_id: String,
    request_digest: String,
}

#[cfg(test)]
mod tests {
    use super::{
        BindBrowserMailboxExecutionRequest, CreateMailboxBindingRequest, MailboxBindingResponse,
        MutationReceipt, valid_digest,
    };

    #[test]
    fn binding_transport_rejects_sensitive_unknown_fields_and_keeps_legacy_shape()
    -> Result<(), Box<dyn std::error::Error>> {
        let digest = "a".repeat(64);
        let valid = format!(
            r#"{{"bindingId":"mailbox_01JTRANSPORT","provider":"IMAP","secretHandle":"secret_01JTRANSPORT","requestDigest":"{digest}"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxBindingRequest>(&valid).is_ok());
        assert!(
            serde_json::from_str::<CreateMailboxBindingRequest>(
                &format!(
                    r#"{{"bindingId":"mailbox_01JTRANSPORT","provider":"IMAP","secretHandle":"secret_01JTRANSPORT","requestDigest":"{digest}","password":"forbidden"}}"#
                )
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<CreateMailboxBindingRequest>(
                &format!(
                    r#"{{"bindingId":"mailbox_01JTRANSPORT","provider":"IMAP","secretHandle":"secret_01JTRANSPORT","requestDigest":"{digest}","messageBody":"forbidden"}}"#
                )
            )
            .is_err()
        );

        assert!(valid_digest(&digest));
        assert!(!valid_digest(&"A".repeat(64)));
        assert!(!valid_digest(&"a".repeat(63)));

        let mutation = serde_json::to_value(MutationReceipt {
            result_code: "created",
            resource_id: "mailbox_01JTRANSPORT",
            aggregate_version: 1,
        })?;
        assert!(mutation.get("resultCode").is_some());
        assert!(mutation.get("resourceId").is_some());
        assert!(mutation.get("aggregateVersion").is_some());

        let response = serde_json::to_value(MailboxBindingResponse {
            binding_id: "mailbox_01JTRANSPORT",
            provider: "IMAP",
            status: "ACTIVE",
            version: 1,
        })?;
        assert!(response.get("bindingId").is_some());
        assert!(response.get("mailboxBindingId").is_none());
        assert!(response.get("provider").is_some());
        assert!(response.get("status").is_some());
        assert!(response.get("version").is_some());
        assert!(response.get("secretHandle").is_none());
        Ok(())
    }

    #[test]
    fn browser_execution_binding_transport_is_metadata_only_and_strict()
    -> Result<(), Box<dyn std::error::Error>> {
        let digest = "b".repeat(64);
        let valid = format!(
            r#"{{"profileId":"profile_01JTRANSPORT","requestDigest":"{digest}"}}"#
        );
        assert!(serde_json::from_str::<BindBrowserMailboxExecutionRequest>(&valid).is_ok());
        for forbidden in ["deviceId", "generationId", "query", "messageBody", "secretHandle"] {
            let invalid = format!(
                r#"{{"profileId":"profile_01JTRANSPORT","requestDigest":"{digest}","{forbidden}":"forbidden"}}"#
            );
            assert!(serde_json::from_str::<BindBrowserMailboxExecutionRequest>(&invalid).is_err());
        }
        Ok(())
    }
}
