#![forbid(unsafe_code)]

mod access_session;
mod clients;
mod command_evidence;
mod composition;
mod identity;
mod integration_events;
mod mailbox_bindings;
mod mailbox_jobs;
mod mailbox_queue_evidence;
mod mailbox_scheduling;
mod mutation_failure;
mod notifications;
mod profile_coordinator;
mod profile_coordinator_ingress;
mod profile_generations;
mod profiles;
mod request_evidence;

pub use profile_coordinator::ProfileCoordinator;

use access_session::session_response;
use cloudflare_adapters::control_plane_queue::ControlPlaneQueueMessage;
use cloudflare_adapters::d1_catalog::D1CatalogRepository;
use cloudflare_adapters::d1_idempotency::D1IdempotencyRepository;
use cloudflare_adapters::d1_identity_acl::D1IdentityAclRepository;
use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use cloudflare_adapters::d1_notification_operations::D1NotificationOperationsRepository;
use control_plane_contract::{
    D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING, R2_PROFILES_BINDING, RouteClass,
    STATIC_ASSETS_BINDING, VERIFICATION_QUEUE_BINDING, classify_route,
};
use profile_platform_primitives::ProfileId;
use session_domain::coordinator::coordinator_object_name;
use worker::{
    Context, Env, MessageBatch, Request, Response, Result, ScheduleContext, ScheduledEvent, event,
};

#[event(fetch, respond_with_errors)]
pub async fn main(mut request: Request, env: Env, _context: Context) -> Result<Response> {
    let route = classify_route(request.method().as_ref(), &request.path());
    match route {
        RouteClass::HealthApi => Response::ok("control-plane-ready"),
        RouteClass::BindingProbeApi => binding_probe(&env),
        RouteClass::DynamicRouteNotFound | RouteClass::BridgeDeniedByDefault => {
            Response::error("Not Found", 404)
        }
        RouteClass::StaticAssets => {
            env.assets(STATIC_ASSETS_BINDING)?
                .fetch_request(request)
                .await
        }
        RouteClass::AuthenticatedSessionApi => session_response(&request, &env).await,
        RouteClass::ClientCollectionApi
        | RouteClass::ClientResourceApi
        | RouteClass::ClientArchiveApi
        | RouteClass::ClientContactApi
        | RouteClass::ClientMergeApi
        | RouteClass::ClientHistoryApi
        | RouteClass::ClientGrantApi => clients::dispatch(route, &mut request, &env).await,
        RouteClass::ProfileCollectionApi
        | RouteClass::ProfileResourceApi
        | RouteClass::ProfileAssignmentApi
        | RouteClass::ProfileGrantApi => profiles::dispatch(route, &mut request, &env).await,
        RouteClass::ProfileCoordinatorApi => dispatch_profile_coordinator(&mut request, &env).await,
        RouteClass::ProfileGenerationCollectionApi
        | RouteClass::ProfileGenerationResourceApi
        | RouteClass::ProfileGenerationVerifyApi
        | RouteClass::ProfileGenerationActivateApi
        | RouteClass::ProfileGenerationDeactivateApi
        | RouteClass::ProfileGenerationQuarantineApi => {
            profile_generations::dispatch(route, &mut request, &env).await
        }
        RouteClass::MailboxBindingCollectionApi
        | RouteClass::MailboxBindingResourceApi
        | RouteClass::MailboxBindingRevokeApi => {
            mailbox_bindings::dispatch(route, &mut request, &env).await
        }
        RouteClass::MailboxJobCollectionApi
        | RouteClass::MailboxJobResourceApi
        | RouteClass::MailboxJobRunApi => mailbox_jobs::dispatch(route, &mut request, &env).await,
        RouteClass::NotificationEventCollectionApi
        | RouteClass::NotificationEventAckApi
        | RouteClass::NotificationReplayCollectionApi
        | RouteClass::NotificationOperationsApi => {
            notifications::dispatch(route, &mut request, &env).await
        }
        RouteClass::OwnerBootstrapApi
        | RouteClass::OwnerTransferApi
        | RouteClass::InvitationCollectionApi
        | RouteClass::InvitationAcceptApi
        | RouteClass::MembershipStatusApi => identity::dispatch(route, &mut request, &env).await,
    }
}

#[event(queue)]
pub async fn control_plane_queue(
    message_batch: MessageBatch<ControlPlaneQueueMessage>,
    env: Env,
    _context: Context,
) -> Result<()> {
    for message in message_batch.messages()? {
        match message.body().clone() {
            ControlPlaneQueueMessage::IntegrationEvent(event) => {
                integration_events::consume_one(&message, event, &env).await?;
            }
            ControlPlaneQueueMessage::MailboxJob(job) => {
                mailbox_scheduling::consume_one(&message, job, &env).await?;
            }
        }
    }
    Ok(())
}

#[event(scheduled)]
pub async fn control_plane_schedule(_event: ScheduledEvent, env: Env, _context: ScheduleContext) {
    if integration_events::dispatch_pending(&env).await.is_err() {
        worker::console_error!("notification scheduled operation failed");
    }
    if mailbox_scheduling::dispatch_pending(&env).await.is_err() {
        worker::console_error!("mailbox scheduled operation failed");
    }
}

async fn dispatch_profile_coordinator(request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let profile_id = segments.get(5).copied().unwrap_or_default();
    profile_coordinator_ingress::dispatch(request, env, tenant_id, profile_id).await
}

fn binding_probe(env: &Env) -> Result<Response> {
    let catalog = env.d1(D1_CATALOG_BINDING)?;
    let _catalog_repository = D1CatalogRepository::new(catalog);
    let identity_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _identity_acl_repository = D1IdentityAclRepository::new(identity_catalog);
    let mailbox_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _mailbox_repository = D1MailboxRepository::new(mailbox_catalog);
    let notification_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _notification_operations_repository =
        D1NotificationOperationsRepository::new(notification_catalog);
    let idempotency_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _idempotency_repository = D1IdempotencyRepository::new(idempotency_catalog);
    let profile_objects = env.bucket(R2_PROFILES_BINDING)?;
    let _verification_queue = env.queue(VERIFICATION_QUEUE_BINDING)?;
    let _integration_events_queue = env.queue(integration_events::INTEGRATION_EVENTS_QUEUE_BINDING)?;
    let _mailbox_jobs_queue = env.queue(mailbox_scheduling::MAILBOX_JOBS_QUEUE_BINDING)?;
    let _mailbox_secret_resolver = env.service("MAILBOX_SECRET_RESOLVER")?;
    let coordinator = env.durable_object(PROFILE_COORDINATOR_BINDING)?;
    let coordinator_id = coordinator.id_from_name(&coordinator_object_name(
        &ProfileId::parse("profile_binding_probe")
            .map_err(|error| worker::Error::RustError(error.to_string()))?,
    ))?;
    let _coordinator_stub = coordinator_id.get_stub()?;
    let _ = profile_objects;
    Response::ok("bindings-ready")
}
