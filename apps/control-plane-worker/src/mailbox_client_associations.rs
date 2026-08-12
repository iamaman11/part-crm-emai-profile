use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::mailbox_client_association_composition::mailbox_client_association_application;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use use_cases_mailboxes::client_association_queries::{
    MailboxClientAssociationDetails, get_mailbox_client_association,
};
use use_cases_mailboxes::client_associations::{
    ExecuteMailboxClientAssociationCommand, MailboxClientAssociationOperationError,
    MailboxClientAssociationOutcome, execute_mailbox_client_association,
};
use worker::{Env, Method, Request, Response, Result};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChangeMailboxClientAssociationRequest {
    client_id: Value,
    expected_relationship_version: u64,
    request_digest: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxClientAssociationProjection {
    binding_id: String,
    client_id: Option<String>,
    relationship_version: u64,
    mailbox_executable: bool,
    can_manage: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxClientAssociationMutationReceipt {
    result_code: String,
    binding_id: String,
    client_id: Option<String>,
    relationship_version: u64,
    replayed: bool,
}

#[must_use]
pub fn is_client_association_path(path: &str) -> bool {
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    matches!(
        segments.as_slice(),
        [
            "api",
            "v1",
            "tenants",
            _,
            "mailboxes",
            _,
            "client-association"
        ]
    )
}

pub async fn dispatch(request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let binding_id = match segments
        .get(5)
        .and_then(|value| MailboxBindingId::parse((*value).to_owned()).ok())
    {
        Some(value) => value,
        None => return neutral_not_found(&correlation_hint(request)),
    };
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);

    match request.method() {
        Method::Get => get_association(env, actor.actor(), role, &binding_id).await,
        Method::Post => change_association(request, env, actor.actor(), role, binding_id).await,
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

async fn get_association(
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: &MailboxBindingId,
) -> Result<Response> {
    let application = mailbox_client_association_application(env)?;
    match get_mailbox_client_association(actor, role, &application, binding_id).await {
        Ok(details) => Response::from_json(&projection(&details, role)),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn change_association(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: MailboxBindingId,
) -> Result<Response> {
    let body = match request
        .json::<ChangeMailboxClientAssociationRequest>()
        .await
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if !valid_digest(&body.request_digest) {
        return invalid_request(actor.correlation_id().as_str());
    }
    let client_id = match parse_nullable_client_id(body.client_id) {
        Ok(value) => value,
        Err(()) => return invalid_request(actor.correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_version =
        application_ports::mailbox_client_associations::MailboxClientAssociationVersion::new(
            body.expected_relationship_version,
        );
    let command = match client_id {
        Some(client_id) => ExecuteMailboxClientAssociationCommand::associate(
            binding_id,
            client_id,
            expected_version,
            evidence,
        ),
        None => {
            ExecuteMailboxClientAssociationCommand::unbind(binding_id, expected_version, evidence)
        }
    };
    let application = mailbox_client_association_application(env)?;
    match execute_mailbox_client_association(actor, role, &application, command).await {
        Ok(outcome) => Response::from_json(&receipt(&outcome)),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn parse_nullable_client_id(value: Value) -> Result<Option<ClientId>, ()> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => ClientId::parse(value).map(Some).map_err(|_| ()),
        _ => Err(()),
    }
}

fn projection(
    details: &MailboxClientAssociationDetails,
    role: MembershipRole,
) -> MailboxClientAssociationProjection {
    MailboxClientAssociationProjection {
        binding_id: details.binding_id().as_str().to_owned(),
        client_id: details.client_id().map(|value| value.as_str().to_owned()),
        relationship_version: details.relationship_version().value(),
        mailbox_executable: details.mailbox_executable(),
        can_manage: role == MembershipRole::TenantOwner,
    }
}

fn receipt(outcome: &MailboxClientAssociationOutcome) -> MailboxClientAssociationMutationReceipt {
    MailboxClientAssociationMutationReceipt {
        result_code: outcome.result_code().to_owned(),
        binding_id: outcome.binding_id().as_str().to_owned(),
        client_id: outcome.client_id().map(|value| value.as_str().to_owned()),
        relationship_version: outcome.relationship_version().value(),
        replayed: outcome.replayed(),
    }
}

fn operation_failure(
    correlation_id: &str,
    error: MailboxClientAssociationOperationError,
) -> Result<Response> {
    match error {
        MailboxClientAssociationOperationError::NotFound => neutral_not_found(correlation_id),
        MailboxClientAssociationOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        MailboxClientAssociationOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        MailboxClientAssociationOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        MailboxClientAssociationOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        MailboxClientAssociationOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        MailboxClientAssociationOperationError::DependencyUnavailable => problem(
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

#[cfg(test)]
mod tests {
    use super::{
        ChangeMailboxClientAssociationRequest, is_client_association_path,
        parse_nullable_client_id, valid_digest,
    };
    use serde_json::{Value, json};

    #[test]
    fn association_route_is_exact_and_does_not_capture_sibling_mailbox_paths() {
        assert!(is_client_association_path(
            "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/client-association"
        ));
        for path in [
            "/api/v1/tenants/tenant_01/mailboxes/mailbox_01",
            "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs",
            "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/client-association/extra",
        ] {
            assert!(!is_client_association_path(path));
        }
    }

    #[test]
    fn change_wire_requires_explicit_client_id_and_rejects_sensitive_or_unknown_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let digest = "a".repeat(64);
        let missing = format!(r#"{{"expectedRelationshipVersion":0,"requestDigest":"{digest}"}}"#);
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequest>(&missing).is_err());
        let unbind = format!(
            r#"{{"clientId":null,"expectedRelationshipVersion":0,"requestDigest":"{digest}"}}"#
        );
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequest>(&unbind).is_ok());
        for forbidden in ["secretHandle", "password", "providerToken", "profileId"] {
            let invalid = format!(
                r#"{{"clientId":null,"expectedRelationshipVersion":0,"requestDigest":"{digest}","{forbidden}":"forbidden"}}"#
            );
            assert!(
                serde_json::from_str::<ChangeMailboxClientAssociationRequest>(&invalid).is_err()
            );
        }
        assert!(valid_digest(&digest));
        assert!(!valid_digest(&"A".repeat(64)));
        Ok(())
    }

    #[test]
    fn nullable_client_parser_accepts_only_null_or_opaque_client_id() {
        assert_eq!(parse_nullable_client_id(Value::Null), Ok(None));
        assert!(parse_nullable_client_id(json!("client_01JASSOCIATION")).is_ok());
        assert_eq!(parse_nullable_client_id(json!(42)), Err(()));
    }
}
