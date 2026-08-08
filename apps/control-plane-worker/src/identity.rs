use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
    verify_request_identity,
};
use crate::command_evidence;
use crate::composition::{identity_ceremony_application, identity_governance_application};
use application_ports::identity_ceremonies::VerifiedIdentitySnapshot;
use control_plane_contract::RouteClass;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, IdentityId, InvitationId, UnixMillis,
};
use serde::{Deserialize, Serialize};
use use_cases::identity_ceremonies::{
    ExecuteInvitationAcceptCommand, ExecuteOwnerBootstrapCommand, IdentityCeremonyOutcome,
    execute_invitation_accept, execute_owner_bootstrap,
};
use use_cases::identity_governance::{
    ExecuteInvitationCreateCommand, ExecuteMembershipStatusCommand, ExecuteOwnerTransferCommand,
    IdentityGovernanceOperationError, IdentityMutationOutcome, authorize_identity_governance,
    execute_invitation_create, execute_membership_status, execute_owner_transfer,
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
        RouteClass::OwnerBootstrapApi => bootstrap_owner(request, env, tenant_id).await,
        RouteClass::OwnerTransferApi => transfer_owner(request, env, tenant_id).await,
        RouteClass::InvitationCollectionApi => create_invitation(request, env, tenant_id).await,
        RouteClass::InvitationAcceptApi => {
            let invitation_id = segments.get(5).copied().unwrap_or_default();
            accept_invitation(request, env, tenant_id, invitation_id).await
        }
        RouteClass::MembershipStatusApi => {
            let actor_id = segments.get(5).copied().unwrap_or_default();
            update_membership_status(request, env, tenant_id, actor_id).await
        }
        _ => neutral_not_found(&correlation_hint(request)),
    }
}

async fn bootstrap_owner(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(verified) = verify_request_identity(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let correlation_id = verified.correlation_id().as_str();
    let body = match request.json::<OwnerBootstrapRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let actor_id = match ActorId::parse(body.actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let identity_id = match IdentityId::parse(body.identity_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let evidence_actor = ActorContext::new(
        verified.scope().clone(),
        actor_id.clone(),
        verified.correlation_id().clone(),
    );
    let evidence = match command_evidence::from_request(request, &evidence_actor, body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let snapshot = VerifiedIdentitySnapshot::new(
        verified.identity().subject(),
        verified.identity().contact_hint().map(str::to_owned),
    );
    let application = identity_ceremony_application(env, verified.identity().clone())?;
    match execute_owner_bootstrap(
        verified.scope().clone(),
        verified.correlation_id().clone(),
        snapshot,
        &application,
        ExecuteOwnerBootstrapCommand::new(
            actor_id,
            identity_id,
            body.tenant_display_name,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => ceremony_receipt(&outcome, if outcome.replayed() { 200 } else { 201 }),
        Err(error) => governance_failure(correlation_id, error),
    }
}

async fn transfer_owner(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_identity_governance(role) {
        return governance_failure(actor.actor().correlation_id().as_str(), error);
    }
    let body = match request.json::<OwnerTransferRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let next_owner_actor_id = match ActorId::parse(body.next_owner_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let current_owner_version = match AggregateVersion::new(body.current_owner_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let next_owner_version = match AggregateVersion::new(body.next_owner_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = identity_governance_application(env)?;
    match execute_owner_transfer(
        actor.actor(),
        role,
        &application,
        ExecuteOwnerTransferCommand::new(
            next_owner_actor_id,
            current_owner_version,
            next_owner_version,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 200),
        Err(error) => governance_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn create_invitation(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_identity_governance(role) {
        return governance_failure(actor.actor().correlation_id().as_str(), error);
    }
    let body = match request.json::<InvitationCreateRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let invitation_id = match InvitationId::parse(body.invitation_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_tenant_version = match AggregateVersion::new(body.expected_tenant_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = identity_governance_application(env)?;
    match execute_invitation_create(
        actor.actor(),
        role,
        &application,
        ExecuteInvitationCreateCommand::new(
            invitation_id,
            body.invited_contact_hmac,
            UnixMillis::new(body.expires_at_ms),
            expected_tenant_version,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, if outcome.replayed() { 200 } else { 201 }),
        Err(error) => governance_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn accept_invitation(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    invitation_id: &str,
) -> Result<Response> {
    let Some(verified) = verify_request_identity(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let correlation_id = verified.correlation_id().as_str();
    let body = match request.json::<InvitationAcceptRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let invitation_id = match InvitationId::parse(invitation_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let identity_id = match IdentityId::parse(body.identity_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let actor_id = match ActorId::parse(body.actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let evidence_actor = ActorContext::new(
        verified.scope().clone(),
        actor_id.clone(),
        verified.correlation_id().clone(),
    );
    let evidence = match command_evidence::from_request(request, &evidence_actor, body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let snapshot = VerifiedIdentitySnapshot::new(
        verified.identity().subject(),
        verified.identity().contact_hint().map(str::to_owned),
    );
    let application = identity_ceremony_application(env, verified.identity().clone())?;
    match execute_invitation_accept(
        verified.scope().clone(),
        verified.correlation_id().clone(),
        snapshot,
        &application,
        ExecuteInvitationAcceptCommand::new(actor_id, identity_id, invitation_id, evidence),
    )
    .await
    {
        Ok(outcome) => ceremony_receipt(&outcome, 200),
        Err(error) => governance_failure(correlation_id, error),
    }
}

async fn update_membership_status(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_identity_governance(role) {
        return governance_failure(actor.actor().correlation_id().as_str(), error);
    }
    let body = match request.json::<MembershipStatusRequest>().await {
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
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let application = identity_governance_application(env)?;
    match execute_membership_status(
        actor.actor(),
        role,
        &application,
        ExecuteMembershipStatusCommand::new(
            target_actor_id,
            expected_version,
            body.status,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 200),
        Err(error) => governance_failure(actor.actor().correlation_id().as_str(), error),
    }
}

fn governance_failure(
    correlation_id: &str,
    error: IdentityGovernanceOperationError,
) -> Result<Response> {
    match error {
        IdentityGovernanceOperationError::InvalidRequest => invalid_request(correlation_id),
        IdentityGovernanceOperationError::NotFound => neutral_not_found(correlation_id),
        IdentityGovernanceOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        IdentityGovernanceOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        IdentityGovernanceOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        IdentityGovernanceOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        IdentityGovernanceOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        IdentityGovernanceOperationError::DependencyUnavailable => problem(
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

fn mutation_receipt(outcome: &IdentityMutationOutcome, status: u16) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.resource_id(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

fn ceremony_receipt(outcome: &IdentityCeremonyOutcome, status: u16) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.resource_id(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OwnerBootstrapRequest {
    actor_id: String,
    identity_id: String,
    tenant_display_name: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OwnerTransferRequest {
    next_owner_actor_id: String,
    current_owner_version: u64,
    next_owner_version: u64,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvitationCreateRequest {
    invitation_id: String,
    invited_contact_hmac: String,
    expires_at_ms: u64,
    expected_tenant_version: u64,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvitationAcceptRequest {
    identity_id: String,
    actor_id: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MembershipStatusRequest {
    status: String,
    expected_version: u64,
    request_digest: String,
}

#[cfg(test)]
mod tests {
    use super::{InvitationCreateRequest, MembershipStatusRequest, MutationReceipt};

    #[test]
    fn transport_models_keep_legacy_camel_case_and_unknown_field_tolerance()
    -> Result<(), Box<dyn std::error::Error>> {
        let receipt = serde_json::to_value(MutationReceipt {
            result_code: "updated",
            resource_id: "actor_01JIDENTITYTRANSPORT",
            aggregate_version: 2,
        })?;
        assert!(receipt.get("resultCode").is_some());
        assert!(receipt.get("resourceId").is_some());
        assert!(receipt.get("aggregateVersion").is_some());

        let invitation = r#"{
            "invitationId":"invitation_01JIDENTITYTRANSPORT",
            "invitedContactHmac":"contact-hmac",
            "expiresAtMs":100,
            "expectedTenantVersion":1,
            "requestDigest":"request-digest-01JIDENTITYTRANSPORT",
            "legacyIgnoredField":"still-tolerated"
        }"#;
        assert!(serde_json::from_str::<InvitationCreateRequest>(invitation).is_ok());

        let membership = r#"{
            "status":"SUSPENDED",
            "expectedVersion":1,
            "requestDigest":"request-digest-01JIDENTITYTRANSPORT",
            "legacyIgnoredField":"still-tolerated"
        }"#;
        assert!(serde_json::from_str::<MembershipStatusRequest>(membership).is_ok());
        Ok(())
    }
}
