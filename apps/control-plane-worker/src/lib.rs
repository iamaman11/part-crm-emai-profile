#![forbid(unsafe_code)]

mod access_session;

use access_session::{correlation_hint, neutral_not_found, session_response};
use cloudflare_adapters::d1_catalog::D1CatalogRepository;
use cloudflare_adapters::d1_identity_acl::D1IdentityAclRepository;
use control_plane_contract::{
    D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING, R2_PROFILES_BINDING, RouteClass,
    STATIC_ASSETS_BINDING, VERIFICATION_QUEUE_BINDING, classify_route,
};
use worker::{
    Context, DurableObject, Env, Request, Response, Result, State, durable_object, event,
};

#[event(fetch, respond_with_errors)]
pub async fn main(request: Request, env: Env, _context: Context) -> Result<Response> {
    let route = classify_route(request.method().as_ref(), &request.path());
    match route {
        RouteClass::HealthApi => Response::ok("control-plane-ready"),
        RouteClass::BindingProbeApi => binding_probe(&env),
        RouteClass::BridgeDeniedByDefault => Response::error("Not Found", 404),
        RouteClass::StaticAssets => {
            env.assets(STATIC_ASSETS_BINDING)?
                .fetch_request(request)
                .await
        }
        RouteClass::AuthenticatedSessionApi => session_response(&request, &env).await,
        RouteClass::OwnerBootstrapApi
        | RouteClass::OwnerTransferApi
        | RouteClass::InvitationCollectionApi
        | RouteClass::InvitationAcceptApi
        | RouteClass::MembershipStatusApi
        | RouteClass::ClientCollectionApi
        | RouteClass::ClientResourceApi
        | RouteClass::ClientGrantApi
        | RouteClass::ProfileCollectionApi
        | RouteClass::ProfileResourceApi
        | RouteClass::ProfileAssignmentApi
        | RouteClass::ProfileGrantApi => authenticated_api_boundary(&request, &env),
    }
}

fn authenticated_api_boundary(request: &Request, env: &Env) -> Result<Response> {
    let catalog = env.d1(D1_CATALOG_BINDING)?;
    let _identity_acl_repository = D1IdentityAclRepository::new(catalog);
    neutral_not_found(&correlation_hint(request))
}

fn binding_probe(env: &Env) -> Result<Response> {
    let catalog = env.d1(D1_CATALOG_BINDING)?;
    let _catalog_repository = D1CatalogRepository::new(catalog);
    let identity_catalog = env.d1(D1_CATALOG_BINDING)?;
    let _identity_acl_repository = D1IdentityAclRepository::new(identity_catalog);
    let _objects = env.bucket(R2_PROFILES_BINDING)?;
    let _verification = env.queue(VERIFICATION_QUEUE_BINDING)?;
    let coordinator = env.durable_object(PROFILE_COORDINATOR_BINDING)?;
    let _stub = coordinator.id_from_name("profile-foundation")?.get_stub()?;

    Response::ok("bindings-ready")
}

#[durable_object(fetch)]
pub struct ProfileCoordinator {
    _state: State,
    _env: Env,
}

impl DurableObject for ProfileCoordinator {
    fn new(state: State, env: Env) -> Self {
        Self {
            _state: state,
            _env: env,
        }
    }

    async fn fetch(&self, _request: Request) -> Result<Response> {
        Response::ok("profile-coordinator-ready")
    }
}
