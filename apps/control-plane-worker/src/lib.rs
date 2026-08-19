#![forbid(unsafe_code)]

mod access_session;
mod capability_gate;
mod client_mail_query;
mod client_mail_send;
mod clients;
mod command_evidence;
mod composition;
mod device_generation_commit;
mod device_generation_upload_capability;
mod device_jobs;
mod identity;
mod integration_events;
mod mailbox_bindings;
mod mailbox_client_association_composition;
mod mailbox_client_associations;
mod mailbox_gmail_oauth;
mod mailbox_jobs;
mod mailbox_microsoft_graph_oauth;
mod mailbox_queue_evidence;
mod mailbox_scheduling;
mod mailbox_standards_onboarding;
mod notifications;
mod operator_queries;
mod profile_coordinator;
mod profile_coordinator_ingress;
mod profile_generations;
mod profiles;
mod realtime_contract;
mod realtime_fanout;
mod realtime_notifications;
mod request_evidence;

pub use profile_coordinator::ProfileCoordinator;
pub use realtime_notifications::NotificationHub;

use access_session::session_response;
use capability_gate::ActivationUnit;
use cloudflare_adapters::control_plane_queue::ControlPlaneQueueMessage;
use cloudflare_adapters::d1_catalog::D1CatalogRepository;
use cloudflare_adapters::d1_idempotency::D1IdempotencyRepository;
use cloudflare_adapters::d1_identity_acl::D1IdentityAclRepository;
use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use cloudflare_adapters::d1_notification_operations::D1NotificationOperationsRepository;
use control_plane_contract::{
    D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING, R2_PROFILES_BINDING, RouteClass,
    STATIC_ASSETS_BINDING, classify_route,
};
use profile_platform_primitives::{ActorId, ProfileId, TenantId};
use session_domain::coordinator::coordinator_object_name;
use worker::{
    Context, Env, MessageBatch, MessageExt, Method, Request, Response, Result, ScheduleContext,
    ScheduledEvent, event,
};

#[event(fetch, respond_with_errors)]
pub async fn main(mut request: Request, env: Env, _context: Context) -> Result<Response> {
    let path = request.path();
    let route = classify_route(request.method().as_ref(), &path);

    if !matches!(
        route,
        RouteClass::HealthApi
            | RouteClass::DynamicRouteNotFound
            | RouteClass::BridgeDeniedByDefault
            | RouteClass::StaticAssets
    ) {
        match capability_gate::route_enabled(&env, route, &path) {
            Ok(true) => {}
            Ok(false) => return Response::error("Not Found", 404),
            Err(_) => return Response::error("Capability Profile Unavailable", 503),
        }
    }

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
        RouteClass::ClientMailSearchApi | RouteClass::ClientMailMessageApi => {
            client_mail_query::dispatch(route, &mut request, &env).await
        }
        RouteClass::ClientMailSendApi => client_mail_send::dispatch(&mut request, &env).await,
        RouteClass::ProfileCollectionApi if request.method() == Method::Get => {
            operator_queries::dispatch(route, &request, &env).await
        }
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
        RouteClass::MailboxBindingCollectionApi if request.method() == Method::Get => {
            operator_queries::dispatch(route, &request, &env).await
        }
        RouteClass::MailboxBindingResourceApi
            if mailbox_standards_onboarding::is_request(&path) =>
        {
            mailbox_standards_onboarding::handle(request, &env, route).await
        }
        RouteClass::MailboxBindingResourceApi
            if mailbox_gmail_oauth::is_gmail_oauth_path(&path) =>
        {
            mailbox_gmail_oauth::dispatch(&mut request, &env).await
        }
        RouteClass::MailboxBindingResourceApi
            if mailbox_microsoft_graph_oauth::is_microsoft_graph_oauth_path(&path) =>
        {
            mailbox_microsoft_graph_oauth::dispatch(&mut request, &env).await
        }
        RouteClass::MailboxBindingResourceApi
            if mailbox_client_associations::is_client_association_path(&path) =>
        {
            mailbox_client_associations::dispatch(&mut request, &env).await
        }
        RouteClass::MailboxBindingCollectionApi
        | RouteClass::MailboxBindingResourceApi
        | RouteClass::MailboxBindingRevokeApi
        | RouteClass::MailboxBrowserExecutionBindApi => {
            mailbox_bindings::dispatch(route, &mut request, &env).await
        }
        RouteClass::MailboxJobCollectionApi
        | RouteClass::MailboxJobResourceApi
        | RouteClass::MailboxJobRunApi => mailbox_jobs::dispatch(route, &mut request, &env).await,
        RouteClass::DeviceJobClaimableApi
        | RouteClass::DeviceJobClaimApi
        | RouteClass::DeviceJobHeartbeatApi
        | RouteClass::DeviceJobOutcomeApi => device_jobs::dispatch(route, &mut request, &env).await,
        RouteClass::DeviceGenerationUploadCapabilityApi => {
            device_generation_upload_capability::dispatch(&mut request, &env).await
        }
        RouteClass::DeviceGenerationCommitApi => {
            device_generation_commit::dispatch(&mut request, &env).await
        }
        RouteClass::NotificationEventCollectionApi
        | RouteClass::NotificationEventAckApi
        | RouteClass::NotificationReplayCollectionApi
        | RouteClass::NotificationOperationsApi => {
            notifications::dispatch(route, &mut request, &env).await
        }
        RouteClass::MembershipCollectionApi => {
            operator_queries::dispatch(route, &request, &env).await
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
                if capability_gate::unit_enabled(&env, ActivationUnit::Notifications)? {
                    integration_events::consume_one(&message, event, &env).await?;
                } else {
                    message.retry();
                }
            }
            ControlPlaneQueueMessage::MailboxJob(job) => {
                if capability_gate::unit_enabled(&env, ActivationUnit::MailboxJobs)? {
                    mailbox_scheduling::consume_one(&message, job, &env).await?;
                } else {
                    message.retry();
                }
            }
        }
    }
    Ok(())
}

#[event(scheduled)]
pub async fn control_plane_schedule(_event: ScheduledEvent, env: Env, _context: ScheduleContext) {
    match capability_gate::unit_enabled(&env, ActivationUnit::Notifications) {
        Ok(true) => {
            if integration_events::dispatch_pending(&env).await.is_err() {
                worker::console_error!("notification scheduled operation failed");
            }
        }
        Ok(false) => {}
        Err(_) => worker::console_error!("notification capability profile unavailable"),
    }
    match capability_gate::unit_enabled(&env, ActivationUnit::MailboxJobs) {
        Ok(true) => {
            if mailbox_scheduling::dispatch_pending(&env).await.is_err() {
                worker::console_error!("mailbox scheduled operation failed");
            }
        }
        Ok(false) => {}
        Err(_) => worker::console_error!("mailbox capability profile unavailable"),
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
    let _generation_upload_signer = composition::generation_upload_capability_signer(env)?;
    let _integration_events_queue =
        env.queue(integration_events::INTEGRATION_EVENTS_QUEUE_BINDING)?;
    if capability_gate::unit_enabled(env, ActivationUnit::MailboxJobs)? {
        let _mailbox_jobs_queue = env.queue(mailbox_scheduling::MAILBOX_JOBS_QUEUE_BINDING)?;
    }
    if capability_gate::unit_enabled(env, ActivationUnit::MailboxAdmin)? {
        let _mailbox_secret_resolver = env.service("MAILBOX_SECRET_RESOLVER")?;
    }
    let coordinator = env.durable_object(PROFILE_COORDINATOR_BINDING)?;
    let coordinator_id = coordinator.id_from_name(&coordinator_object_name(
        &ProfileId::parse("profile_binding_probe")
            .map_err(|error| worker::Error::RustError(error.to_string()))?,
    ))?;
    let _coordinator_stub = coordinator_id.get_stub()?;
    let notification_hubs = env.durable_object(realtime_notifications::NOTIFICATION_HUB_BINDING)?;
    let notification_tenant = TenantId::parse("tenant_binding_probe")
        .map_err(|error| worker::Error::RustError(error.to_string()))?;
    let notification_actor = ActorId::parse("actor_binding_probe")
        .map_err(|error| worker::Error::RustError(error.to_string()))?;
    let notification_hub_id =
        notification_hubs.id_from_name(&realtime_notifications::notification_hub_object_name(
            &notification_tenant,
            &notification_actor,
        ))?;
    let _notification_hub_stub = notification_hub_id.get_stub()?;
    let _ = profile_objects;
    Response::ok("bindings-ready")
}
