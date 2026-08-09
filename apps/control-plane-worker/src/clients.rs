use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::{
    client_application, client_contact_protection, client_merge_application,
    client_persistence_application, client_registry_projection,
};
use application_ports::client_registry::{
    ClientRegistryActivityProjection as DomainActivityProjection,
    ClientRegistryAssignmentProjection as DomainAssignmentProjection,
    ClientRegistryContactProjection as DomainContactProjection,
    ClientRegistryHistoryProjection as DomainHistoryProjection, ClientRegistryListItem,
};
use client_domain::{AssignmentStatus, ClientKind, ClientStatus, ContactKind, ContactStatus};
use control_plane_contract::RouteClass;
use control_plane_contract::client_registry_api::{
    ClientActivityProjection, ClientArchiveRequest, ClientAssignmentProjection,
    ClientContactArchiveRequest, ClientContactProjection, ClientContactUpsertRequest,
    ClientHistoryProjection, ClientListProjection, ClientMergeRequest, ClientUpdateRequest,
};
use control_plane_contract::public_api::{
    ClientCreateRequest, ClientGrantRequest, ClientProjection, MutationReceipt,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorId, AggregateVersion, ClientId, ContactPointId};
use use_cases_clients::client_grants::{
    ClientGrantAction, ClientGrantOperationError, ClientGrantOutcome, ExecuteClientGrantCommand,
    authorize_client_grant, execute_client_grant,
};
use use_cases_clients::clients::{
    ClientDetails, ClientMutationOutcome, ClientOperationError, ExecuteCreateClientCommand,
    authorize_client_create, execute_create_client, get_visible_client,
};
use use_cases_clients::contacts::{
    ArchiveContactCommand, ContactApplicationError, ContactMutationOutcome,
    PrepareProtectedContactCommand, TransientContactValue, authorize_contact_mutation,
    execute_archive_contact, execute_upsert_contact,
};
use use_cases_clients::lifecycle::{
    ArchiveClientCommand, ClientLifecycleError, ClientLifecycleOutcome, UpdateClientCommand,
    authorize_client_lifecycle, execute_archive_client, execute_update_client,
};
use use_cases_clients::merge::{
    ClientMergeApplicationError, ClientMergeOutcome, MergeClientCommand, execute_merge_client,
};
use use_cases_clients::registry::{
    ClientRegistryQueryError, get_visible_client_history, list_visible_clients,
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
        RouteClass::ClientCollectionApi if request.method().as_ref() == "GET" => {
            list_clients(request, env, tenant_id).await
        }
        RouteClass::ClientCollectionApi => create_client(request, env, tenant_id).await,
        RouteClass::ClientResourceApi if request.method().as_ref() == "PATCH" => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            update_client(request, env, tenant_id, client_id).await
        }
        RouteClass::ClientResourceApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            get_client(request, env, tenant_id, client_id).await
        }
        RouteClass::ClientArchiveApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            archive_client(request, env, tenant_id, client_id).await
        }
        RouteClass::ClientContactApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            let contact_point_id = segments.get(7).copied().unwrap_or_default();
            update_client_contact(request, env, tenant_id, client_id, contact_point_id).await
        }
        RouteClass::ClientMergeApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            merge_client(request, env, tenant_id, client_id).await
        }
        RouteClass::ClientHistoryApi => {
            let client_id = segments.get(5).copied().unwrap_or_default();
            get_client_history(request, env, tenant_id, client_id).await
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
    let kind = match parse_client_kind(&body.kind) {
        Some(value) => value,
        None => return invalid_request(actor.actor().correlation_id().as_str()),
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

async fn list_clients(request: &Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let projection = client_registry_projection(env)?;
    match list_visible_clients(actor.actor(), membership_role(&actor), &projection).await {
        Ok(clients) => Response::from_json(&ClientListProjection {
            clients: clients.iter().map(client_list_projection).collect(),
        }),
        Err(error) => registry_failure(actor.actor().correlation_id().as_str(), error),
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
        Ok(client) => Response::from_json(&client_projection(&client)),
        Err(ClientOperationError::NotFound) => {
            neutral_not_found(actor.actor().correlation_id().as_str())
        }
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn update_client(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    client_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_client_lifecycle(role) {
        return lifecycle_failure(actor.actor().correlation_id().as_str(), error);
    }
    let body = match request.json::<ClientUpdateRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let expected_version = match AggregateVersion::new(body.expected_client_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let repository = client_persistence_application(env)?;
    match execute_update_client(
        actor.actor(),
        role,
        &repository,
        UpdateClientCommand::new(client_id, expected_version, body.display_name, evidence),
    )
    .await
    {
        Ok(outcome) => lifecycle_receipt(&outcome),
        Err(error) => lifecycle_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn archive_client(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    client_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_client_lifecycle(role) {
        return lifecycle_failure(actor.actor().correlation_id().as_str(), error);
    }
    let body = match request.json::<ClientArchiveRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let expected_version = match AggregateVersion::new(body.expected_client_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let repository = client_persistence_application(env)?;
    match execute_archive_client(
        actor.actor(),
        role,
        &repository,
        ArchiveClientCommand::new(client_id, expected_version, evidence),
    )
    .await
    {
        Ok(outcome) => lifecycle_receipt(&outcome),
        Err(error) => lifecycle_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn update_client_contact(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    client_id: &str,
    contact_point_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if let Err(error) = authorize_contact_mutation(role) {
        return contact_failure(actor.actor().correlation_id().as_str(), error);
    }
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let contact_point_id = match ContactPointId::parse(contact_point_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let repository = client_persistence_application(env)?;

    if request.method().as_ref() == "DELETE" {
        let body = match request.json::<ClientContactArchiveRequest>().await {
            Ok(value) => value,
            Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
        };
        let expected_version = match AggregateVersion::new(body.expected_client_version) {
            Ok(value) => value,
            Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
        };
        let next_version = match expected_version.next() {
            Ok(value) => value,
            Err(_) => return internal_failure(actor.actor().correlation_id().as_str()),
        };
        let kind = match parse_contact_kind(&body.kind) {
            Some(value) => value,
            None => return invalid_request(actor.actor().correlation_id().as_str()),
        };
        let evidence =
            match command_evidence::from_request(request, actor.actor(), body.request_digest) {
                Ok(value) => value,
                Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
        return match execute_archive_contact(
            actor.actor(),
            role,
            &repository,
            ArchiveContactCommand::new(
                client_id,
                contact_point_id.clone(),
                expected_version,
                kind,
                evidence,
            ),
        )
        .await
        {
            Ok(outcome) => contact_receipt(&contact_point_id, next_version, &outcome),
            Err(error) => contact_failure(actor.actor().correlation_id().as_str(), error),
        };
    }

    let body = match request.json::<ClientContactUpsertRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_version = match AggregateVersion::new(body.expected_client_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let next_version = match expected_version.next() {
        Ok(value) => value,
        Err(_) => return internal_failure(actor.actor().correlation_id().as_str()),
    };
    let kind = match parse_contact_kind(&body.kind) {
        Some(value) => value,
        None => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let protector = client_contact_protection(env)?;
    match execute_upsert_contact(
        actor.actor(),
        role,
        &protector,
        &repository,
        PrepareProtectedContactCommand::new(
            client_id,
            contact_point_id.clone(),
            expected_version,
            kind,
            TransientContactValue::new(body.value),
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => contact_receipt(&contact_point_id, next_version, &outcome),
        Err(error) => contact_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn merge_client(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    source_client_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);
    if role != MembershipRole::TenantOwner {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    }
    let body = match request.json::<ClientMergeRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let source_client_id = match ClientId::parse(source_client_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let target_client_id = match ClientId::parse(body.target_client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_source_version = match AggregateVersion::new(body.expected_source_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let expected_target_version = match AggregateVersion::new(body.expected_target_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), body.request_digest)
    {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let repository = client_merge_application(env)?;
    match execute_merge_client(
        actor.actor(),
        role,
        &repository,
        MergeClientCommand::new(
            source_client_id,
            target_client_id,
            expected_source_version,
            expected_target_version,
            body.reason,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => merge_receipt(&outcome),
        Err(error) => merge_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn get_client_history(
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
    let projection = client_registry_projection(env)?;
    match get_visible_client_history(
        actor.actor(),
        membership_role(&actor),
        &projection,
        &client_id,
    )
    .await
    {
        Ok(history) => Response::from_json(&history_projection(&history)),
        Err(error) => registry_failure(actor.actor().correlation_id().as_str(), error),
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
        ClientOperationError::InvalidRequest => invalid_request(correlation_id),
        ClientOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ClientOperationError::IntegrityFailure => integrity_failure(correlation_id),
        ClientOperationError::InternalFailure => internal_failure(correlation_id),
        ClientOperationError::DependencyUnavailable => dependency_unavailable(correlation_id),
    }
}

fn lifecycle_failure(correlation_id: &str, error: ClientLifecycleError) -> Result<Response> {
    match error {
        ClientLifecycleError::NotFound => neutral_not_found(correlation_id),
        ClientLifecycleError::InvalidRequest => invalid_request(correlation_id),
        ClientLifecycleError::VersionConflict => version_conflict(correlation_id),
        ClientLifecycleError::InvalidState => invalid_state(correlation_id),
        ClientLifecycleError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ClientLifecycleError::IntegrityFailure => integrity_failure(correlation_id),
        ClientLifecycleError::InternalFailure => internal_failure(correlation_id),
        ClientLifecycleError::DependencyUnavailable => dependency_unavailable(correlation_id),
    }
}

fn contact_failure(correlation_id: &str, error: ContactApplicationError) -> Result<Response> {
    match error {
        ContactApplicationError::NotFound => neutral_not_found(correlation_id),
        ContactApplicationError::InvalidRequest => invalid_request(correlation_id),
        ContactApplicationError::VersionConflict => version_conflict(correlation_id),
        ContactApplicationError::InvalidState => invalid_state(correlation_id),
        ContactApplicationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ContactApplicationError::KeyUnavailable
        | ContactApplicationError::DependencyUnavailable => dependency_unavailable(correlation_id),
        ContactApplicationError::IntegrityFailure => integrity_failure(correlation_id),
        ContactApplicationError::InternalFailure => internal_failure(correlation_id),
    }
}

fn merge_failure(correlation_id: &str, error: ClientMergeApplicationError) -> Result<Response> {
    match error {
        ClientMergeApplicationError::NotFound => neutral_not_found(correlation_id),
        ClientMergeApplicationError::InvalidRequest => invalid_request(correlation_id),
        ClientMergeApplicationError::VersionConflict => version_conflict(correlation_id),
        ClientMergeApplicationError::InvalidState => invalid_state(correlation_id),
        ClientMergeApplicationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ClientMergeApplicationError::IntegrityFailure => integrity_failure(correlation_id),
        ClientMergeApplicationError::InternalFailure => internal_failure(correlation_id),
        ClientMergeApplicationError::DependencyUnavailable => dependency_unavailable(correlation_id),
    }
}

fn registry_failure(correlation_id: &str, error: ClientRegistryQueryError) -> Result<Response> {
    match error {
        ClientRegistryQueryError::NotFound => neutral_not_found(correlation_id),
        ClientRegistryQueryError::IntegrityFailure => integrity_failure(correlation_id),
        ClientRegistryQueryError::DependencyUnavailable => dependency_unavailable(correlation_id),
    }
}

fn client_grant_failure(
    correlation_id: &str,
    error: ClientGrantOperationError,
) -> Result<Response> {
    match error {
        ClientGrantOperationError::InvalidRequest => invalid_request(correlation_id),
        ClientGrantOperationError::NotFound => neutral_not_found(correlation_id),
        ClientGrantOperationError::VersionConflict => version_conflict(correlation_id),
        ClientGrantOperationError::InvalidState => invalid_state(correlation_id),
        ClientGrantOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        ClientGrantOperationError::IntegrityFailure => integrity_failure(correlation_id),
        ClientGrantOperationError::InternalFailure => internal_failure(correlation_id),
        ClientGrantOperationError::DependencyUnavailable => dependency_unavailable(correlation_id),
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn version_conflict(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 409, "version_conflict", "Version Conflict")
}

fn invalid_state(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 409, "invalid_state", "Invalid State")
}

fn integrity_failure(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        500,
        "integrity_failure",
        "Integrity Failure",
    )
}

fn internal_failure(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 500, "internal_failure", "Internal Failure")
}

fn dependency_unavailable(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        503,
        "dependency_unavailable",
        "Dependency Unavailable",
    )
}

fn no_content() -> Result<Response> {
    Response::empty().map(|response| response.with_status(204))
}

fn mutation_receipt(outcome: &ClientMutationOutcome) -> Result<Response> {
    let status = if outcome.replayed() { 200 } else { 201 };
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

fn lifecycle_receipt(outcome: &ClientLifecycleOutcome) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
}

fn contact_receipt(
    contact_point_id: &ContactPointId,
    next_version: AggregateVersion,
    outcome: &ContactMutationOutcome,
) -> Result<Response> {
    let (result_code, resource_id, aggregate_version) = match outcome {
        ContactMutationOutcome::Applied {
            contact_point_id,
            client_version,
            ..
        } => (
            "applied".to_owned(),
            contact_point_id.as_str().to_owned(),
            *client_version,
        ),
        ContactMutationOutcome::Replayed(receipt) => (
            receipt.result_code().to_owned(),
            receipt
                .result_reference()
                .unwrap_or(contact_point_id.as_str())
                .to_owned(),
            next_version,
        ),
    };
    Response::from_json(&MutationReceipt {
        result_code,
        resource_id,
        aggregate_version: aggregate_version.value(),
    })
}

fn merge_receipt(outcome: &ClientMergeOutcome) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.target_client_id().as_str().to_owned(),
        aggregate_version: outcome.source_version().value(),
    })
}

fn client_grant_receipt(outcome: &ClientGrantOutcome) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
}

fn client_projection(client: &ClientDetails) -> ClientProjection {
    ClientProjection {
        client_id: client.client_id().as_str().to_owned(),
        kind: client_kind_code(client.kind()).to_owned(),
        display_name: client.display_name().to_owned(),
        status: client_status_code(client.status()).to_owned(),
        version: client.version().value(),
    }
}

fn client_list_projection(client: &ClientRegistryListItem) -> ClientProjection {
    ClientProjection {
        client_id: client.client_id().as_str().to_owned(),
        kind: client_kind_code(client.kind()).to_owned(),
        display_name: client.display_name().to_owned(),
        status: client_status_code(client.status()).to_owned(),
        version: client.version().value(),
    }
}

fn history_projection(history: &DomainHistoryProjection) -> ClientHistoryProjection {
    ClientHistoryProjection {
        contacts: history.contacts().iter().map(contact_projection).collect(),
        assignments: history
            .assignments()
            .iter()
            .map(assignment_projection)
            .collect(),
        activity: history.activity().iter().map(activity_projection).collect(),
    }
}

fn contact_projection(contact: &DomainContactProjection) -> ClientContactProjection {
    ClientContactProjection {
        contact_point_id: contact.contact_point_id().as_str().to_owned(),
        kind: contact.kind().stable_code().to_owned(),
        status: contact.status().stable_code().to_owned(),
    }
}

fn assignment_projection(assignment: &DomainAssignmentProjection) -> ClientAssignmentProjection {
    ClientAssignmentProjection {
        assignment_id: assignment.assignment_id().as_str().to_owned(),
        profile_id: assignment.profile_id().as_str().to_owned(),
        status: match assignment.status() {
            AssignmentStatus::Active => "ACTIVE",
            AssignmentStatus::Closed => "CLOSED",
        }
        .to_owned(),
        assigned_at_ms: assignment.assigned_at().value(),
        closed_at_ms: assignment.closed_at().map(|value| value.value()),
        reason: assignment.reason().to_owned(),
    }
}

fn activity_projection(activity: &DomainActivityProjection) -> ClientActivityProjection {
    ClientActivityProjection {
        audit_event_id: activity.audit_event_id().as_str().to_owned(),
        action: activity.action().to_owned(),
        resource_type: activity.resource_type().to_owned(),
        resource_id: activity.resource_id().to_owned(),
        result_code: activity.result_code().to_owned(),
        occurred_at_ms: activity.occurred_at().value(),
    }
}

const fn parse_client_kind(value: &str) -> Option<ClientKind> {
    match value.as_bytes() {
        b"PERSON" => Some(ClientKind::Person),
        b"ORGANIZATION" => Some(ClientKind::Organization),
        _ => None,
    }
}

const fn parse_contact_kind(value: &str) -> Option<ContactKind> {
    match value.as_bytes() {
        b"EMAIL" => Some(ContactKind::Email),
        b"PHONE" => Some(ContactKind::Phone),
        b"URL" => Some(ContactKind::Url),
        _ => None,
    }
}

const fn client_kind_code(kind: ClientKind) -> &'static str {
    match kind {
        ClientKind::Person => "PERSON",
        ClientKind::Organization => "ORGANIZATION",
    }
}

const fn client_status_code(status: ClientStatus) -> &'static str {
    match status {
        ClientStatus::Active => "ACTIVE",
        ClientStatus::Archived => "ARCHIVED",
        ClientStatus::Merged => "MERGED",
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientGrantRequest, ClientProjection, MutationReceipt, parse_contact_kind};
    use client_domain::ContactKind;

    #[test]
    fn transport_models_keep_camel_case_contract_field_names()
    -> Result<(), Box<dyn std::error::Error>> {
        let mutation = serde_json::to_value(MutationReceipt {
            result_code: "created".to_owned(),
            resource_id: "client_01JTRANSPORT".to_owned(),
            aggregate_version: 1,
        })?;
        assert!(mutation.get("resultCode").is_some());
        assert!(mutation.get("resourceId").is_some());
        assert!(mutation.get("aggregateVersion").is_some());

        let response = serde_json::to_value(ClientProjection {
            client_id: "client_01JTRANSPORT".to_owned(),
            kind: "PERSON".to_owned(),
            display_name: "Client".to_owned(),
            status: "ACTIVE".to_owned(),
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

    #[test]
    fn contact_kind_parser_is_bounded_to_contract_vocabulary() {
        assert_eq!(parse_contact_kind("EMAIL"), Some(ContactKind::Email));
        assert_eq!(parse_contact_kind("PHONE"), Some(ContactKind::Phone));
        assert_eq!(parse_contact_kind("URL"), Some(ContactKind::Url));
        assert_eq!(parse_contact_kind("email"), None);
    }
}
