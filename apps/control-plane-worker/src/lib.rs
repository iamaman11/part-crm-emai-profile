#![forbid(unsafe_code)]

use control_plane_contract::{
    D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING, R2_PROFILES_BINDING,
    STATIC_ASSETS_BINDING, RouteClass, VERIFICATION_QUEUE_BINDING, classify_route,
};
use worker::{Context, DurableObject, Env, Request, Response, Result, State, durable_object, event};

#[event(fetch, respond_with_errors)]
pub async fn main(request: Request, env: Env, _context: Context) -> Result<Response> {
    match classify_route(&request.path()) {
        RouteClass::ApiHealth => Response::ok("control-plane-ready"),
        RouteClass::ApiBindingProbe => binding_probe(&env),
        RouteClass::BridgeDeniedByDefault => Response::error("Not Found", 404),
        RouteClass::BrowserAsset => {
            env.assets(STATIC_ASSETS_BINDING)?
                .fetch_request(request)
                .await
        }
    }
}

fn binding_probe(env: &Env) -> Result<Response> {
    let _catalog = env.d1(D1_CATALOG_BINDING)?;
    let _objects = env.bucket(R2_PROFILES_BINDING)?;
    let _verification = env.queue(VERIFICATION_QUEUE_BINDING)?;
    let coordinator = env.durable_object(PROFILE_COORDINATOR_BINDING)?;
    let _stub = coordinator
        .id_from_name("profile-foundation")?
        .get_stub()?;

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
