use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
    verify_request_identity,
};
use crate::mutation_failure::{MutationFailureClass, classify_mutation_failure, mutation_failure};
use crate::request_evidence::{audit_event_id, outbox_event_id};
use cloudflare_adapters::d1_governed_commands::D1GovernedCommandRepository;
use cloudflare_adapters::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use cloudflare_adapters::d1_identity_acl::{
    AssignProfileMutation, BootstrapOwnerMutation, ClientGrantMutation, ClientGrantValue,
    CreateInvitationMutation, CreateProfileMutation, D1IdentityAclRepository,
    MembershipStatusMutation, MembershipStatusValue, MutationEnvelope as IdentityEnvelope,
    OwnerTransferMutation, ProfileGrantMutation, ProfileGrantValue, ResolvedActor,
    ResolvedMembershipRole, VerifiedBootstrapContext,
};
use cloudflare_adapters::d1_identity_queries::{D1IdentityQueryRepository, ProfileProjection};
use cloudflare_adapters::d1_invitation_acceptance::{
    AcceptInvitationMutation, D1InvitationAcceptanceRepository,
};
use control_plane_contract::{D1_CATALOG_BINDING, RouteClass};
use profile_platform_primitives::{
    ActorId, AggregateVersion, AssignmentId, AuditEventId, ClientId, IdempotencyKey, IdentityId,
    InvitationId, OutboxEventId, ProfileId, TenantScope, UnixMillis,
};
use serde::{Deserialize, Serialize};
use worker::{Date, Env, Error, Request, Response, Result};

const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const IDEMPOTENCY_TTL_MS: u64 = 86_400_000;

const OWNER_BOOTSTRAP_COMMAND: &str = "tenant.owner_bootstrap";
const OWNER_TRANSFER_COMMAND: &str = "membership.owner_transfer";
const INVITATION_CREATE_COMMAND: &str = "invitation.create";
const INVITATION_ACCEPT_COMMAND: &str = "invitation.accept";
const PROFILE_CREATE_COMMAND: &str = "profile.create";
const PROFILE_ASSIGN_COMMAND: &str = "profile.assign_client";
const PROFILE_GRANT_COMMAND: &str = "profile.grant";
const PROFILE_GRANT_REVOKE_COMMAND: &str = "profile.grant_revoke";
const CLIENT_GRANT_COMMAND: &str = "client.grant";
const CLIENT_GRANT_REVOKE_COMMAND: &str = "client.grant_revoke";

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
        RouteClass::ClientGrantApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            let actor_id = segments.get(7).copied().unwrap_or_default();
            update_client_grant(request, env, tenant_id, client_id, actor_id).await
        }
        RouteClass::ProfileCollectionApi => create_profile(request, env, tenant_id).await,
        RouteClass::ProfileResourceApi => {
            let profile_id = segments.get(5).copied().unwrap_or_default();
            get_profile(request, env, tenant_id, profile_id).await
        }
        RouteClass::ProfileAssignmentApi => {
            let profile_id = segments.get(5).copied().unwrap_or_default();
            assign_profile(request, env, tenant_id, profile_id).await
        }
        RouteClass::ProfileGrantApi => {
            let profile_id = segments.get(5).copied().unwrap_or_default();
            let actor_id = segments.get(7).copied().unwrap_or_default();
            update_profile_grant(request, env, tenant_id, profile_id, actor_id).await
        }
        _ => neutral_not_found(&correlation_hint(request)),
    }
}

async fn bootstrap_owner(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(verified) = verify_request_identity(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<OwnerBootstrapRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let actor_id = match ActorId::parse(body.actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let identity_id = match IdentityId::parse(body.identity_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let envelope = match EnvelopeOwned::from_request(
        request,
        verified.scope(),
        &actor_id,
        body.request_digest,
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let repository = D1IdentityAclRepository::new(env.d1(D1_CATALOG_BINDING)?);

    let existing = match repository
        .resolve_active_actor(
            verified.scope().clone(),
            verified.identity(),
            verified.correlation_id().clone(),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return mutation_failure(request, error),
    };
    if let Some(existing) = existing {
        if existing.role() != ResolvedMembershipRole::TenantOwner
            || existing.actor().actor_id() != &actor_id
        {
            return neutral_not_found(verified.correlation_id().as_str());
        }
        if let Some(response) = replay_response(
            request,
            env,
            verified.scope(),
            &actor_id,
            verified.correlation_id().as_str(),
            OWNER_BOOTSTRAP_COMMAND,
            &envelope,
            verified.scope().tenant_id().as_str(),
            1,
            200,
        )
        .await?
        {
            return Ok(response);
        }
        return conflict(request);
    }

    let boundary = match repository.tenant_boundary(verified.scope()).await {
        Ok(value) => value,
        Err(error) => return mutation_failure(request, error),
    };
    if boundary.membership_count != 0 || boundary.active_owner_count != 0 {
        return conflict(request);
    }
    let context = VerifiedBootstrapContext::from_verified_identity(
        verified.scope().clone(),
        actor_id.clone(),
        verified.correlation_id().clone(),
        verified.identity(),
    );
    let mutation = BootstrapOwnerMutation {
        tenant_display_name: &body.tenant_display_name,
        identity_id: &identity_id,
        envelope: envelope.identity(),
    };
    match repository.bootstrap_owner(&context, mutation).await {
        Ok(_) => mutation_receipt(
            "bootstrapped",
            verified.scope().tenant_id().as_str(),
            1,
            201,
        ),
        Err(error) => {
            mutation_failure_or_replay(
                request,
                env,
                verified.scope(),
                &actor_id,
                verified.correlation_id().as_str(),
                OWNER_BOOTSTRAP_COMMAND,
                &envelope,
                verified.scope().tenant_id().as_str(),
                1,
                200,
                error,
            )
            .await
        }
    }
}

async fn transfer_owner(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = active_owner(request, env, tenant_id).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<OwnerTransferRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let next_owner_actor_id = match ActorId::parse(body.next_owner_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let current_owner_version = match AggregateVersion::new(body.current_owner_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let next_owner_version = match AggregateVersion::new(body.next_owner_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_aggregate_version(next_owner_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_actor(request, &actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_for_actor(
        request,
        env,
        &actor,
        OWNER_TRANSFER_COMMAND,
        &envelope,
        next_owner_actor_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = OwnerTransferMutation {
        next_owner_actor_id: &next_owner_actor_id,
        current_owner_version,
        next_owner_version,
        envelope: envelope.identity(),
    };
    let result = D1GovernedCommandRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .transfer_owner(actor.actor(), mutation)
        .await;
    match result {
        Ok(_) => mutation_receipt(
            "transferred",
            next_owner_actor_id.as_str(),
            response_version,
            200,
        ),
        Err(error) => {
            mutation_failure_or_replay_for_actor(
                request,
                env,
                &actor,
                OWNER_TRANSFER_COMMAND,
                &envelope,
                next_owner_actor_id.as_str(),
                response_version,
                200,
                error,
            )
            .await
        }
    }
}

async fn create_invitation(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = active_owner(request, env, tenant_id).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<InvitationCreateRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let invitation_id = match InvitationId::parse(body.invitation_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_tenant_version = match AggregateVersion::new(body.expected_tenant_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_aggregate_version(expected_tenant_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_actor(request, &actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_for_actor(
        request,
        env,
        &actor,
        INVITATION_CREATE_COMMAND,
        &envelope,
        invitation_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = CreateInvitationMutation {
        invitation_id: &invitation_id,
        invited_contact_hmac: &body.invited_contact_hmac,
        expires_at: UnixMillis::new(body.expires_at_ms),
        tenant_expected_version: expected_tenant_version,
        envelope: envelope.identity(),
    };
    let result = D1GovernedCommandRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .create_invitation(actor.actor(), mutation)
        .await;
    match result {
        Ok(_) => mutation_receipt("created", invitation_id.as_str(), response_version, 201),
        Err(error) => {
            mutation_failure_or_replay_for_actor(
                request,
                env,
                &actor,
                INVITATION_CREATE_COMMAND,
                &envelope,
                invitation_id.as_str(),
                response_version,
                200,
                error,
            )
            .await
        }
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
    let body = match request.json::<InvitationAcceptRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let invitation_id = match InvitationId::parse(invitation_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let identity_id = match IdentityId::parse(body.identity_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let actor_id = match ActorId::parse(body.actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let envelope = match EnvelopeOwned::from_request(
        request,
        verified.scope(),
        &actor_id,
        body.request_digest,
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let identity_repository = D1IdentityAclRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let existing = match identity_repository
        .resolve_active_actor(
            verified.scope().clone(),
            verified.identity(),
            verified.correlation_id().clone(),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return mutation_failure(request, error),
    };
    if let Some(existing) = existing {
        if existing.actor().actor_id() != &actor_id {
            return neutral_not_found(verified.correlation_id().as_str());
        }
        if let Some(response) = replay_response(
            request,
            env,
            verified.scope(),
            &actor_id,
            verified.correlation_id().as_str(),
            INVITATION_ACCEPT_COMMAND,
            &envelope,
            actor_id.as_str(),
            1,
            200,
        )
        .await?
        {
            return Ok(response);
        }
        return conflict(request);
    }
    let context = VerifiedBootstrapContext::from_verified_identity(
        verified.scope().clone(),
        actor_id.clone(),
        verified.correlation_id().clone(),
        verified.identity(),
    );
    let mutation = AcceptInvitationMutation {
        invitation_id: &invitation_id,
        identity_id: &identity_id,
        envelope: envelope.identity(),
    };
    let result = D1InvitationAcceptanceRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .accept(
            &context,
            verified.identity(),
            verified.correlation_id(),
            mutation,
        )
        .await;
    match result {
        Ok(_) => mutation_receipt("accepted", actor_id.as_str(), 1, 200),
        Err(error) => {
            mutation_failure_or_replay(
                request,
                env,
                verified.scope(),
                &actor_id,
                verified.correlation_id().as_str(),
                INVITATION_ACCEPT_COMMAND,
                &envelope,
                actor_id.as_str(),
                1,
                200,
                error,
            )
            .await
        }
    }
}

async fn update_membership_status(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = active_owner(request, env, tenant_id).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<MembershipStatusRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let target_actor_id = match ActorId::parse(target_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_version = match AggregateVersion::new(body.expected_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_aggregate_version(expected_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let (next_status, command_name) = match body.status.as_str() {
        "ACTIVE" => (MembershipStatusValue::Active, "membership.activate"),
        "SUSPENDED" => (MembershipStatusValue::Suspended, "membership.suspend"),
        "REVOKED" => (MembershipStatusValue::Revoked, "membership.revoke"),
        _ => return invalid_request(request),
    };
    let envelope = match EnvelopeOwned::from_actor(request, &actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_for_actor(
        request,
        env,
        &actor,
        command_name,
        &envelope,
        target_actor_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = MembershipStatusMutation {
        target_actor_id: &target_actor_id,
        expected_version,
        next_status,
        envelope: envelope.identity(),
    };
    let result = D1GovernedCommandRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .update_membership_status(actor.actor(), mutation)
        .await;
    match result {
        Ok(_) => mutation_receipt("updated", target_actor_id.as_str(), response_version, 200),
        Err(error) => {
            mutation_failure_or_replay_for_actor(
                request,
                env,
                &actor,
                command_name,
                &envelope,
                target_actor_id.as_str(),
                response_version,
                200,
                error,
            )
            .await
        }
    }
}

async fn create_profile(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = active_owner(request, env, tenant_id).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<ProfileCreateRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let profile_id = match ProfileId::parse(body.profile_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let envelope = match EnvelopeOwned::from_actor(request, &actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_for_actor(
        request,
        env,
        &actor,
        PROFILE_CREATE_COMMAND,
        &envelope,
        profile_id.as_str(),
        1,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = CreateProfileMutation {
        profile_id: &profile_id,
        envelope: envelope.identity(),
    };
    let result = D1GovernedCommandRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .create_profile(actor.actor(), mutation)
        .await;
    match result {
        Ok(_) => mutation_receipt("created", profile_id.as_str(), 1, 201),
        Err(error) => {
            mutation_failure_or_replay_for_actor(
                request,
                env,
                &actor,
                PROFILE_CREATE_COMMAND,
                &envelope,
                profile_id.as_str(),
                1,
                200,
                error,
            )
            .await
        }
    }
}

async fn get_profile(
    request: &Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let Some(profile) = D1IdentityQueryRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .find_visible_profile(
            actor.actor().tenant_scope(),
            actor.actor().actor_id(),
            actor.role(),
            &profile_id,
        )
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    Response::from_json(&ProfileResponse::from(&profile))
}

async fn assign_profile(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    let Some(actor) = active_owner(request, env, tenant_id).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<AssignmentRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let assignment_id = match AssignmentId::parse(body.assignment_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let client_id = match ClientId::parse(body.client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_profile_version = match AggregateVersion::new(body.expected_profile_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_aggregate_version(expected_profile_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_actor(request, &actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_for_actor(
        request,
        env,
        &actor,
        PROFILE_ASSIGN_COMMAND,
        &envelope,
        assignment_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = AssignProfileMutation {
        assignment_id: &assignment_id,
        profile_id: &profile_id,
        client_id: &client_id,
        expected_profile_version,
        reason: &body.reason,
        envelope: envelope.identity(),
    };
    let result = D1GovernedCommandRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .assign_profile(actor.actor(), mutation)
        .await;
    match result {
        Ok(_) => mutation_receipt("assigned", assignment_id.as_str(), response_version, 200),
        Err(error) => {
            mutation_failure_or_replay_for_actor(
                request,
                env,
                &actor,
                PROFILE_ASSIGN_COMMAND,
                &envelope,
                assignment_id.as_str(),
                response_version,
                200,
                error,
            )
            .await
        }
    }
}

async fn update_profile_grant(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = active_owner(request, env, tenant_id).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<ProfileGrantRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let target_actor_id = match ActorId::parse(target_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_profile_version = match AggregateVersion::new(body.expected_profile_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_aggregate_version(expected_profile_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let role = match body.role.as_str() {
        "PROFILE_VIEWER" => ProfileGrantValue::Viewer,
        "PROFILE_OPERATOR" => ProfileGrantValue::Operator,
        _ => return invalid_request(request),
    };
    let revoke = request.method().as_ref() == "DELETE";
    let command_name = if revoke {
        PROFILE_GRANT_REVOKE_COMMAND
    } else {
        PROFILE_GRANT_COMMAND
    };
    let replay_status = if revoke { 204 } else { 200 };
    let envelope = match EnvelopeOwned::from_actor(request, &actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_for_actor(
        request,
        env,
        &actor,
        command_name,
        &envelope,
        profile_id.as_str(),
        response_version,
        replay_status,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = ProfileGrantMutation {
        target_actor_id: &target_actor_id,
        profile_id: &profile_id,
        expected_profile_version,
        role,
        reason: &body.reason,
        envelope: envelope.identity(),
    };
    let repository = D1GovernedCommandRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let result = if revoke {
        repository
            .revoke_profile_grant(actor.actor(), mutation)
            .await
    } else {
        repository.grant_profile(actor.actor(), mutation).await
    };
    match result {
        Ok(_) if revoke => no_content(),
        Ok(_) => mutation_receipt("granted", profile_id.as_str(), response_version, 200),
        Err(error) => {
            mutation_failure_or_replay_for_actor(
                request,
                env,
                &actor,
                command_name,
                &envelope,
                profile_id.as_str(),
                response_version,
                replay_status,
                error,
            )
            .await
        }
    }
}

async fn update_client_grant(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    client_id: &str,
    target_actor_id: &str,
) -> Result<Response> {
    let Some(actor) = active_owner(request, env, tenant_id).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<ClientGrantRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let target_actor_id = match ActorId::parse(target_actor_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_client_version = match AggregateVersion::new(body.expected_client_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_aggregate_version(expected_client_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let role = match body.role.as_str() {
        "CLIENT_VIEWER" => ClientGrantValue::Viewer,
        "CLIENT_EDITOR" => ClientGrantValue::Editor,
        _ => return invalid_request(request),
    };
    let revoke = request.method().as_ref() == "DELETE";
    let command_name = if revoke {
        CLIENT_GRANT_REVOKE_COMMAND
    } else {
        CLIENT_GRANT_COMMAND
    };
    let replay_status = if revoke { 204 } else { 200 };
    let envelope = match EnvelopeOwned::from_actor(request, &actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_for_actor(
        request,
        env,
        &actor,
        command_name,
        &envelope,
        client_id.as_str(),
        response_version,
        replay_status,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = ClientGrantMutation {
        target_actor_id: &target_actor_id,
        client_id: &client_id,
        expected_client_version,
        role,
        reason: &body.reason,
        envelope: envelope.identity(),
    };
    let repository = D1GovernedCommandRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let result = if revoke {
        repository
            .revoke_client_grant(actor.actor(), mutation)
            .await
    } else {
        repository.grant_client(actor.actor(), mutation).await
    };
    match result {
        Ok(_) if revoke => no_content(),
        Ok(_) => mutation_receipt("granted", client_id.as_str(), response_version, 200),
        Err(error) => {
            mutation_failure_or_replay_for_actor(
                request,
                env,
                &actor,
                command_name,
                &envelope,
                client_id.as_str(),
                response_version,
                replay_status,
                error,
            )
            .await
        }
    }
}

async fn active_owner(
    request: &Request,
    env: &Env,
    tenant_id: &str,
) -> Result<Option<ResolvedActor>> {
    let actor = resolve_active_request_actor(request, env, Some(tenant_id)).await?;
    Ok(actor.filter(|resolved| resolved.role() == ResolvedMembershipRole::TenantOwner))
}

#[allow(clippy::too_many_arguments)]
async fn replay_response(
    request: &Request,
    env: &Env,
    scope: &TenantScope,
    actor_id: &ActorId,
    correlation_id: &str,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    status: u16,
) -> Result<Option<Response>> {
    let decision = match D1IdempotencyRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .decide(
            scope,
            actor_id,
            &envelope.idempotency_key,
            command_name,
            &envelope.request_digest,
            envelope.now,
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return mutation_failure(request, error).map(Some),
    };
    match decision {
        IdempotencyDecision::Miss => Ok(None),
        IdempotencyDecision::Replay(_) if status == 204 => no_content().map(Some),
        IdempotencyDecision::Replay(receipt) => mutation_receipt(
            receipt.result_code(),
            receipt.result_reference().unwrap_or(resource_id),
            aggregate_version,
            status,
        )
        .map(Some),
        IdempotencyDecision::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict").map(Some)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn replay_for_actor(
    request: &Request,
    env: &Env,
    actor: &ResolvedActor,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    status: u16,
) -> Result<Option<Response>> {
    replay_response(
        request,
        env,
        actor.actor().tenant_scope(),
        actor.actor().actor_id(),
        actor.actor().correlation_id().as_str(),
        command_name,
        envelope,
        resource_id,
        aggregate_version,
        status,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn mutation_failure_or_replay(
    request: &Request,
    env: &Env,
    scope: &TenantScope,
    actor_id: &ActorId,
    correlation_id: &str,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    replay_status: u16,
    error: Error,
) -> Result<Response> {
    if classify_mutation_failure(&error.to_string()) == MutationFailureClass::Conflict {
        if let Some(response) = replay_response(
            request,
            env,
            scope,
            actor_id,
            correlation_id,
            command_name,
            envelope,
            resource_id,
            aggregate_version,
            replay_status,
        )
        .await?
        {
            return Ok(response);
        }
    }
    mutation_failure(request, error)
}

#[allow(clippy::too_many_arguments)]
async fn mutation_failure_or_replay_for_actor(
    request: &Request,
    env: &Env,
    actor: &ResolvedActor,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    replay_status: u16,
    error: Error,
) -> Result<Response> {
    mutation_failure_or_replay(
        request,
        env,
        actor.actor().tenant_scope(),
        actor.actor().actor_id(),
        actor.actor().correlation_id().as_str(),
        command_name,
        envelope,
        resource_id,
        aggregate_version,
        replay_status,
        error,
    )
    .await
}

fn next_aggregate_version(version: AggregateVersion) -> Option<u64> {
    version.next().ok().map(AggregateVersion::value)
}

fn invalid_request(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        400,
        "invalid_request",
        "Invalid Request",
    )
}

fn conflict(request: &Request) -> Result<Response> {
    problem(&correlation_hint(request), 409, "conflict", "Conflict")
}

fn internal_failure(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        500,
        "internal_failure",
        "Internal Failure",
    )
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

fn mutation_receipt(
    result_code: &str,
    resource_id: &str,
    aggregate_version: u64,
    status: u16,
) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code,
        resource_id,
        aggregate_version,
    })
    .map(|response| response.with_status(status))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileResponse<'a> {
    profile_id: &'a str,
    status: &'a str,
    version: u64,
    linked_client_id: Option<&'a str>,
}

impl<'a> From<&'a ProfileProjection> for ProfileResponse<'a> {
    fn from(profile: &'a ProfileProjection) -> Self {
        Self {
            profile_id: profile.profile_id().as_str(),
            status: profile.status(),
            version: profile.version(),
            linked_client_id: profile.linked_client_id().map(ClientId::as_str),
        }
    }
}

struct EnvelopeOwned {
    idempotency_key: IdempotencyKey,
    request_digest: String,
    audit_event_id: AuditEventId,
    outbox_event_id: OutboxEventId,
    now: UnixMillis,
    expires_at: UnixMillis,
    payload_json: String,
}

impl EnvelopeOwned {
    fn from_actor(
        request: &Request,
        actor: &ResolvedActor,
        request_digest: String,
    ) -> Result<Self> {
        Self::from_request(
            request,
            actor.actor().tenant_scope(),
            actor.actor().actor_id(),
            request_digest,
        )
    }

    fn from_request(
        request: &Request,
        scope: &TenantScope,
        actor_id: &ActorId,
        request_digest: String,
    ) -> Result<Self> {
        if !(16..=256).contains(&request_digest.len()) {
            return Err(Error::RustError(
                "request digest length is invalid".to_owned(),
            ));
        }
        let key = request
            .headers()
            .get(IDEMPOTENCY_HEADER)?
            .ok_or_else(|| Error::RustError("idempotency key missing".to_owned()))?;
        let idempotency_key =
            IdempotencyKey::parse(key).map_err(|error| Error::RustError(error.to_string()))?;
        let audit_event_id = audit_event_id(scope.tenant_id(), actor_id, &idempotency_key)?;
        let outbox_event_id = outbox_event_id(scope.tenant_id(), actor_id, &idempotency_key)?;
        let now = Date::now().as_millis();
        let expires_at = now
            .checked_add(IDEMPOTENCY_TTL_MS)
            .ok_or_else(|| Error::RustError("idempotency expiry overflow".to_owned()))?;
        Ok(Self {
            idempotency_key,
            request_digest,
            audit_event_id,
            outbox_event_id,
            now: UnixMillis::new(now),
            expires_at: UnixMillis::new(expires_at),
            payload_json: "{}".to_owned(),
        })
    }

    fn identity(&self) -> IdentityEnvelope<'_> {
        IdentityEnvelope {
            idempotency_key: &self.idempotency_key,
            request_digest: &self.request_digest,
            audit_event_id: &self.audit_event_id,
            outbox_event_id: &self.outbox_event_id,
            payload_json: &self.payload_json,
            now: self.now,
            idempotency_expires_at: self.expires_at,
        }
    }
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileCreateRequest {
    profile_id: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentRequest {
    assignment_id: String,
    client_id: String,
    reason: String,
    expected_profile_version: u64,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileGrantRequest {
    role: String,
    reason: String,
    expected_profile_version: u64,
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
    use super::{
        CLIENT_GRANT_COMMAND, CLIENT_GRANT_REVOKE_COMMAND, PROFILE_GRANT_COMMAND,
        PROFILE_GRANT_REVOKE_COMMAND, next_aggregate_version,
    };
    use profile_platform_primitives::AggregateVersion;

    #[test]
    fn aggregate_response_versions_never_saturate() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(next_aggregate_version(AggregateVersion::INITIAL), Some(2));
        assert_eq!(
            next_aggregate_version(AggregateVersion::new(u64::MAX)?),
            None
        );
        Ok(())
    }

    #[test]
    fn grant_and_revoke_commands_are_distinct_idempotency_domains() {
        assert_ne!(PROFILE_GRANT_COMMAND, PROFILE_GRANT_REVOKE_COMMAND);
        assert_ne!(CLIENT_GRANT_COMMAND, CLIENT_GRANT_REVOKE_COMMAND);
    }
}
