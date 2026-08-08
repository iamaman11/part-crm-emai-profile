#![forbid(unsafe_code)]

mod access_session;
mod api;
mod clients;
mod command_evidence;
mod composition;
mod mailbox_bindings;
mod mailbox_jobs;
mod mutation_failure;
mod profile_coordinator;
mod profile_generations;
mod profiles;
mod request_evidence;

pub use profile_coordinator::ProfileCoordinator;

use access_session::session_response;
use cloudflare_adapters::d1_catalog::D1CatalogRepository;
use cloudflare_adapters::d1_idempotency::D1IdempotencyRepository;
use cloudflare_adapters::d1_identity_acl::D1IdentityAclRepository;
use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use control_plane_contract::{
    D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING, R2_PROFILES_BINDING, RouteClass,
    STATIC_ASSETS_BINDING, VERIFICATION_QUEUE_BINDING, classify_route,
};
use profile_platform_primitives::ProfileId;
use session_domain::coordinator::coordinator_object_name;
use worker::{Context, Env, Request, Response, Result, event};

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
        RouteClass::ClientCollectionApi | RouteClass::ClientResourceApi => {
            clients::dispatch(route, &mut request, &env).await
        }
        RouteClass::ProfileCollectionApi
        | RouteClass::ProfileResourceApi
        | RouteClass::ProfileAssignmentApi => profiles::dispatch(route, &mut request, &env).await,
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
        RouteClass::OwnerBootstrapApi
        | RouteClass::OwnerTransferApi
        | RouteClass::InvitationCollectionApi
        | RouteClass::InvitationAcceptApi
        | RouteClass::MembershipStatusApi
        | RouteClass::ClientGrantApi
        | RouteClass::ProfileGrantApi => api::dispatch(route, &mut request, &env).await,
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
    profile_coordinator::dispatch(request, env, tenant_id, profile_id).await
}

fn binding_probe(env: &Env) -> Result<Response> {
    let catalog = env.d1(D1_CATALOG_BINDING)?;
    let _catalog_repository = D1CatalogRepository::new(catalog);
    let identity_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _identity_acl_repository = D1IdentityAclRepository::new(identity_catalog);
    let mailbox_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _mailbox_repository = D1MailboxRepository::new(mailbox_catalog);
    let idempotency_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _idempotency_repository = D1IdempotencyRepository::new(idempotency_catalog);
    let _objects = env.bucket(R2_PROFILES_BINDING)?;
    let _verification = env.queue(VERIFICATION_QUEUE_BINDING)?;
    let coordinator = env.durable_object(PROFILE_COORDINATOR_BINDING)?;
    let probe_profile = ProfileId::parse("profile_foundation")
        .map_err(|error| worker::Error::RustError(error.to_string()))?;
    let _stub = coordinator
        .id_from_name(&coordinator_object_name(&probe_profile))?
        .get_stub()?;

    Response::ok("bindings-ready")
}
