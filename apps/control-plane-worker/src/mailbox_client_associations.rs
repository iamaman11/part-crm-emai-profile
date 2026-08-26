use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::mailbox_client_association_composition::mailbox_client_association_application;
use control_plane_contract::mailbox_client_association_api::{
    ChangeMailboxClientAssociationRequestDto, MailboxClientAssociationMutationReceiptDto,
    MailboxClientAssociationProjectionDto,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};
use use_cases_mailboxes::client_association_queries::{
    MailboxClientAssociationDetails, get_mailbox_client_association,
};
use use_cases_mailboxes::client_associations::{
    ExecuteMailboxClientAssociationCommand, MailboxClientAssociationOperationError,
    MailboxClientAssociationOutcome, execute_mailbox_client_association,
};
use worker::{Env, Method, Request, Response, Result};

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
        .json::<ChangeMailboxClientAssociationRequestDto>()
        .await
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let client_id = match body.client_id.as_ref() {
        Some(value) => match ClientId::parse(value.clone()) {
            Ok(value) => Some(value),
            Err(_) => return invalid_request(actor.correlation_id().as_str()),
        },
        None => None,
    };
    let expected_version =
        application_ports::mailbox_client_associations::MailboxClientAssociationVersion::new(
            body.expected_relationship_version,
        );
    let evidence = match command_evidence::from_request(request, actor, &body) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
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

fn projection(
    details: &MailboxClientAssociationDetails,
    role: MembershipRole,
) -> MailboxClientAssociationProjectionDto {
    MailboxClientAssociationProjectionDto {
        binding_id: details.binding_id().as_str().to_owned(),
        client_id: details.client_id().map(|value| value.as_str().to_owned()),
        relationship_version: details.relationship_version().value(),
        mailbox_executable: details.mailbox_executable(),
        can_manage: role == MembershipRole::TenantOwner,
    }
}

fn receipt(
    outcome: &MailboxClientAssociationOutcome,
) -> MailboxClientAssociationMutationReceiptDto {
    MailboxClientAssociationMutationReceiptDto {
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

#[cfg(test)]
mod tests {
    use super::is_client_association_path;
    use control_plane_contract::mailbox_client_association_api::ChangeMailboxClientAssociationRequestDto;

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
    fn change_wire_requires_explicit_client_id_and_rejects_legacy_digest_and_unknown_fields() {
        let missing = r#"{"expectedRelationshipVersion":0}"#;
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(missing).is_err());
        let unbind = r#"{"clientId":null,"expectedRelationshipVersion":0}"#;
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(unbind).is_ok());
        let legacy =
            r#"{"clientId":null,"expectedRelationshipVersion":0,"requestDigest":"legacy"}"#;
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(legacy).is_err());
        for forbidden in ["secretHandle", "password", "providerToken", "profileId"] {
            let invalid = format!(
                r#"{{"clientId":null,"expectedRelationshipVersion":0,"{forbidden}":"forbidden"}}"#
            );
            assert!(
                serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(&invalid).is_err()
            );
        }
    }
}
