use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use application_ports::NotificationReplayIntent;
use application_ports::{CursorAdvanceWriteOutcome, ReplayPreparationOutcome, ReplayReasonClass};
use cloudflare_adapters::d1_notification_operations::D1NotificationOperationsRepository;
use cloudflare_adapters::d1_notifications::D1NotificationRepository;
use control_plane_contract::RouteClass;
use control_plane_contract::public_api::{
    NotificationCatchUpAckRequest, NotificationCatchUpProjection, NotificationEventProjection,
    NotificationOperationsProjection, NotificationReplayReceipt, NotificationReplayRequest,
};
use profile_platform_primitives::{AuditEventId, OpaqueId, OutboxEventId, UnixMillis};
use use_cases_notifications::catch_up::{acknowledge_catch_up, load_catch_up};
use use_cases_notifications::error::NotificationOperationError;
use use_cases_notifications::operations::load_operations;
use use_cases_notifications::replay::prepare_replay;
use worker::{Date, Env, Request, Response, Result};

const DEFAULT_CATCH_UP_PAGE_SIZE: u32 = 100;

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();

    match route {
        RouteClass::NotificationEventCollectionApi => get_catch_up(request, env, tenant_id).await,
        RouteClass::NotificationEventAckApi => acknowledge(request, env, tenant_id).await,
        RouteClass::NotificationReplayCollectionApi => {
            prepare_operator_replay(request, env, tenant_id).await
        }
        RouteClass::NotificationOperationsApi => get_operations(request, env, tenant_id).await,
        _ => neutral_not_found(&correlation_hint(request)),
    }
}

async fn get_catch_up(request: &Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let operations = D1NotificationOperationsRepository::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    let cursors =
        D1NotificationRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);
    match load_catch_up(
        &operations,
        &cursors,
        &operations,
        actor.actor(),
        DEFAULT_CATCH_UP_PAGE_SIZE,
    )
    .await
    {
        Ok(batch) => Response::from_json(&NotificationCatchUpProjection {
            events: batch
                .events()
                .iter()
                .map(|event| NotificationEventProjection {
                    event_id: event.event_id().as_str().to_owned(),
                    aggregate_type: event.aggregate_type().to_owned(),
                    aggregate_id: event.aggregate_id().as_str().to_owned(),
                    event_type: event.event_type().to_owned(),
                    occurred_at_ms: event.occurred_at().value(),
                })
                .collect(),
        }),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn acknowledge(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<NotificationCatchUpAckRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let event_id = match OutboxEventId::parse(body.event_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let operations = D1NotificationOperationsRepository::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    let cursors =
        D1NotificationRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);
    let now = UnixMillis::new(Date::now().as_millis());
    match acknowledge_catch_up(
        &operations,
        &cursors,
        &operations,
        actor.actor(),
        &event_id,
        now,
    )
    .await
    {
        Ok(CursorAdvanceWriteOutcome::Advanced | CursorAdvanceWriteOutcome::Unchanged) => {
            no_content()
        }
        Ok(CursorAdvanceWriteOutcome::Stale) => problem(
            actor.actor().correlation_id().as_str(),
            409,
            "conflict",
            "Conflict",
        ),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn prepare_operator_replay(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let body = match request.json::<NotificationReplayRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let replay_id = match OpaqueId::parse(body.replay_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let consumer_id = match OpaqueId::parse(body.consumer_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let event_id = match OutboxEventId::parse(body.event_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let audit_event_id = match AuditEventId::parse(body.audit_event_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let reason_class = match body.reason_class.as_str() {
        "DEPENDENCY_RECOVERED" => ReplayReasonClass::DependencyRecovered,
        "OPERATOR_REMEDIATION" => ReplayReasonClass::OperatorRemediation,
        "INTEGRITY_REVALIDATED" => ReplayReasonClass::IntegrityRevalidated,
        _ => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let intent = NotificationReplayIntent::new(
        replay_id,
        consumer_id,
        event_id,
        audit_event_id,
        reason_class,
        UnixMillis::new(Date::now().as_millis()),
    );
    let operations = D1NotificationOperationsRepository::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    match prepare_replay(&operations, &operations, actor.actor(), &intent).await {
        Ok(outcome) => Response::from_json(&NotificationReplayReceipt {
            replay_id: intent.replay_id().as_str().to_owned(),
            result_code: match outcome {
                ReplayPreparationOutcome::Prepared => "prepared",
                ReplayPreparationOutcome::Duplicate => "duplicate",
            }
            .to_owned(),
        }),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn get_operations(request: &Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let operations = D1NotificationOperationsRepository::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    match load_operations(
        &operations,
        &operations,
        actor.actor(),
        UnixMillis::new(Date::now().as_millis()),
    )
    .await
    {
        Ok(snapshot) => Response::from_json(&NotificationOperationsProjection {
            ready_count: snapshot.ready_count(),
            retry_scheduled_count: snapshot.retry_scheduled_count(),
            delivered_count: snapshot.delivered_count(),
            dead_letter_count: snapshot.dead_letter_count(),
            pending_replay_count: snapshot.pending_replay_count(),
            max_attempt_count: snapshot.max_attempt_count(),
            oldest_open_age_ms: snapshot.oldest_open_age_ms(),
            catch_up_lag_count: snapshot.catch_up_lag_count(),
        }),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

fn operation_failure(correlation_id: &str, error: NotificationOperationError) -> Result<Response> {
    match error {
        NotificationOperationError::Forbidden => {
            problem(correlation_id, 403, "forbidden", "Forbidden")
        }
        NotificationOperationError::InvalidInput => invalid_request(correlation_id),
        NotificationOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        NotificationOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        NotificationOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
        NotificationOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn no_content() -> Result<Response> {
    Response::empty().map(|response| response.with_status(204))
}
