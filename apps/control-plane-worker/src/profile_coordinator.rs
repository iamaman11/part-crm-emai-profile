use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use cloudflare_adapters::d1_identity_acl::ResolvedMembershipRole;
use cloudflare_adapters::d1_identity_queries::D1IdentityQueryRepository;
use cloudflare_adapters::d1_profile_coordinator::{
    CoordinatorProjectionMutation, CoordinatorProjectionOutcome, D1ProfileCoordinatorRepository,
};
use cloudflare_adapters::profile_coordinator::{
    CoordinatorAdapterError, CoordinatorProjection, StoredCoordinatorCommand,
    StoredCoordinatorDocument, StoredCoordinatorEnvelope, StoredReleaseDisposition, outcome_name,
};
use control_plane_contract::{D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING};
use profile_platform_primitives::{OutboxEventId, ProfileId, TenantId, TenantScope, UnixMillis};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::{CoordinatorConfig, CoordinatorOutcome, coordinator_object_name};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::web_sys::WorkerGlobalScope;
use worker::{
    Date, DateInit, DurableObject, Env, Error, Headers, Method, Request, RequestInit, Response,
    Result, ScheduledTime, State, durable_object,
};

const STORAGE_KEY: &str = "profile-coordinator-v1";
const IDLE_TIMEOUT_MS: u64 = 30_000;
const HARD_TIMEOUT_MS: u64 = 900_000;
const DRAIN_TIMEOUT_MS: u64 = 60_000;
const MIN_INTENT_TTL_MS: u64 = 1_000;
const MAX_INTENT_TTL_MS: u64 = 300_000;

pub async fn dispatch(
    request: &mut Request,
    env: &Env,
    tenant_value: &str,
    profile_value: &str,
) -> Result<Response> {
    let tenant_id = match TenantId::parse(tenant_value.to_owned()) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(&correlation_hint(request)),
    };
    let profile_id = match ProfileId::parse(profile_value.to_owned()) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(&correlation_hint(request)),
    };
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_value)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let visible_profile = D1IdentityQueryRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .find_visible_profile(
            actor.actor().tenant_scope(),
            actor.actor().actor_id(),
            actor.role(),
            &profile_id,
        )
        .await?;
    let Some(profile) = visible_profile else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    if !profile_is_coordinatable(profile.status()) {
        return problem(
            actor.actor().correlation_id().as_str(),
            409,
            "conflict",
            "Conflict",
        );
    }

    let namespace = env.durable_object(PROFILE_COORDINATOR_BINDING)?;
    let object_id = namespace.id_from_name(&coordinator_object_name(&profile_id))?;
    let stub = object_id.get_stub()?;

    let response = match request.method() {
        Method::Get => {
            let internal = internal_request(
                "/snapshot",
                &CoordinatorSnapshotRequest {
                    tenant_id: tenant_id.as_str(),
                    profile_id: profile_id.as_str(),
                },
            )?;
            stub.fetch_with_request(internal).await?
        }
        Method::Post => {
            let body = match request.json::<CoordinatorCommandRequest>().await {
                Ok(value) => value,
                Err(_) => return invalid_request(request),
            };
            if matches!(body.command, CoordinatorApiCommand::MarkRecovered)
                && actor.role() != ResolvedMembershipRole::TenantOwner
            {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            }
            let now_ms = Date::now().as_millis();
            let command = match body
                .command
                .into_stored(actor.actor().actor_id().as_str(), now_ms)
            {
                Ok(value) => value,
                Err(_) => return invalid_request(request),
            };
            let internal = internal_request(
                "/command",
                &CoordinatorInternalCommandRequest {
                    tenant_id: tenant_id.as_str(),
                    profile_id: profile_id.as_str(),
                    envelope: StoredCoordinatorEnvelope::new(
                        body.idempotency_key,
                        body.sequence,
                        body.expected_version,
                        command,
                    ),
                },
            )?;
            stub.fetch_with_request(internal).await?
        }
        _ => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };

    project_and_respond(response, env, actor.actor().tenant_scope(), &profile_id).await
}

fn profile_is_coordinatable(status: &str) -> bool {
    matches!(status, "READY" | "IN_USE" | "DIRTY_LOCAL" | "SYNCING")
}

async fn project_and_respond(
    mut response: Response,
    env: &Env,
    scope: &TenantScope,
    profile_id: &ProfileId,
) -> Result<Response> {
    if response.status_code() >= 400 {
        return Ok(response);
    }
    let payload = response.json::<CoordinatorObjectResponse>().await?;
    let outcome = projection_outcome(&payload.outcome)?;
    let outbox_event_id = generate_outbox_event_id().map_err(request_error)?;
    D1ProfileCoordinatorRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .project(
            scope,
            CoordinatorProjectionMutation {
                profile_id,
                projection: &payload.projection,
                outcome,
                outbox_event_id: &outbox_event_id,
                projected_at: UnixMillis::new(Date::now().as_millis()),
            },
        )
        .await?;
    Response::from_json(&payload)
}

fn projection_outcome(value: &str) -> Result<CoordinatorProjectionOutcome> {
    Ok(match value {
        "snapshot" => CoordinatorProjectionOutcome::Snapshot,
        "launch_intent_issued" => CoordinatorProjectionOutcome::LaunchIntentIssued,
        "lease_claimed" => CoordinatorProjectionOutcome::LeaseClaimed,
        "heartbeat_accepted" => CoordinatorProjectionOutcome::HeartbeatAccepted,
        "released" => CoordinatorProjectionOutcome::Released,
        "drain_started" => CoordinatorProjectionOutcome::DrainStarted,
        "timed_out" => CoordinatorProjectionOutcome::TimedOut,
        "launch_intent_expired" => CoordinatorProjectionOutcome::LaunchIntentExpired,
        "recovered" => CoordinatorProjectionOutcome::Recovered,
        "no_change" => CoordinatorProjectionOutcome::NoChange,
        _ => {
            return Err(Error::RustError(
                "unknown coordinator projection outcome".to_owned(),
            ));
        }
    })
}

#[derive(Deserialize)]
struct CoordinatorCommandRequest {
    idempotency_key: String,
    sequence: u64,
    expected_version: u64,
    command: CoordinatorApiCommand,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CoordinatorApiCommand {
    IssueLaunchIntent {
        launch_intent_id: String,
        device_id: String,
        expires_in_ms: u64,
    },
    Claim {
        launch_intent_id: String,
        device_id: String,
        session_id: String,
    },
    Heartbeat {
        session_id: String,
        epoch: u64,
        fencing_token: String,
    },
    Release {
        session_id: String,
        epoch: u64,
        fencing_token: String,
        disposition: StoredReleaseDisposition,
    },
    BeginDrain,
    MarkRecovered,
}

impl CoordinatorApiCommand {
    fn into_stored(
        self,
        actor_id: &str,
        now_ms: u64,
    ) -> Result<StoredCoordinatorCommand, CoordinatorRequestError> {
        Ok(match self {
            Self::IssueLaunchIntent {
                launch_intent_id,
                device_id,
                expires_in_ms,
            } => {
                if !(MIN_INTENT_TTL_MS..=MAX_INTENT_TTL_MS).contains(&expires_in_ms) {
                    return Err(CoordinatorRequestError::InvalidIntentTtl);
                }
                let expires_at_ms = now_ms
                    .checked_add(expires_in_ms)
                    .ok_or(CoordinatorRequestError::TimeOverflow)?;
                StoredCoordinatorCommand::IssueLaunchIntent {
                    launch_intent_id,
                    actor_id: actor_id.to_owned(),
                    device_id,
                    now_ms,
                    expires_at_ms,
                }
            }
            Self::Claim {
                launch_intent_id,
                device_id,
                session_id,
            } => StoredCoordinatorCommand::Claim {
                launch_intent_id,
                actor_id: actor_id.to_owned(),
                device_id,
                session_id,
                fencing_token: generate_fencing_token()?,
                now_ms,
            },
            Self::Heartbeat {
                session_id,
                epoch,
                fencing_token,
            } => StoredCoordinatorCommand::Heartbeat {
                session_id,
                epoch,
                fencing_token,
                now_ms,
            },
            Self::Release {
                session_id,
                epoch,
                fencing_token,
                disposition,
            } => StoredCoordinatorCommand::Release {
                session_id,
                epoch,
                fencing_token,
                disposition,
                now_ms,
            },
            Self::BeginDrain => StoredCoordinatorCommand::BeginDrain { now_ms },
            Self::MarkRecovered => StoredCoordinatorCommand::MarkRecovered { now_ms },
        })
    }
}

#[derive(Debug)]
enum CoordinatorRequestError {
    InvalidIntentTtl,
    TimeOverflow,
    CryptoUnavailable,
    InvalidGeneratedId,
}

fn generate_fencing_token() -> Result<String, CoordinatorRequestError> {
    Ok(format!("fence_{}", random_uuid()?))
}

fn generate_outbox_event_id() -> Result<OutboxEventId, CoordinatorRequestError> {
    OutboxEventId::parse(format!("outbox_{}", random_uuid()?))
        .map_err(|_| CoordinatorRequestError::InvalidGeneratedId)
}

fn random_uuid() -> Result<String, CoordinatorRequestError> {
    let global: WorkerGlobalScope = worker::js_sys::global().unchecked_into();
    let crypto = global
        .crypto()
        .map_err(|_| CoordinatorRequestError::CryptoUnavailable)?;
    Ok(crypto.random_uuid())
}

#[derive(Serialize)]
struct CoordinatorSnapshotRequest<'a> {
    tenant_id: &'a str,
    profile_id: &'a str,
}

#[derive(Serialize)]
struct CoordinatorInternalCommandRequest<'a> {
    tenant_id: &'a str,
    profile_id: &'a str,
    envelope: StoredCoordinatorEnvelope,
}

fn internal_request<T: Serialize>(path: &str, body: &T) -> Result<Request> {
    let payload = serde_json::to_string(body).map_err(json_error)?;
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&payload)));
    Request::new_with_init(
        &format!("https://profile-coordinator.internal{path}"),
        &init,
    )
}

#[durable_object]
pub struct ProfileCoordinator {
    state: State,
    _env: Env,
}

impl DurableObject for ProfileCoordinator {
    fn new(state: State, env: Env) -> Self {
        Self { state, _env: env }
    }

    async fn fetch(&self, mut request: Request) -> Result<Response> {
        match (request.method(), request.path().as_str()) {
            (Method::Post, "/snapshot") => self.snapshot(&mut request).await,
            (Method::Post, "/command") => self.command(&mut request).await,
            _ => Response::error("Not Found", 404),
        }
    }

    async fn alarm(&self) -> Result<Response> {
        let Some(mut document) = self
            .state
            .storage()
            .get::<StoredCoordinatorDocument>(STORAGE_KEY)
            .await?
        else {
            return Response::ok("no coordinator state");
        };
        let state = document.replay().map_err(adapter_error)?;
        let sequence = state
            .last_sequence()
            .checked_add(1)
            .ok_or_else(|| Error::RustError("coordinator sequence overflow".to_owned()))?;
        let envelope = StoredCoordinatorEnvelope::new(
            format!("alarm_{sequence:020}"),
            sequence,
            state.version().value(),
            StoredCoordinatorCommand::Tick {
                now_ms: Date::now().as_millis(),
            },
        );
        let applied = document.apply(envelope).map_err(adapter_error)?;
        if applied.appended() {
            self.state.storage().put(STORAGE_KEY, &document).await?;
        }
        schedule_alarm(&self.state, applied.next_alarm_at()).await?;
        Response::from_json(&CoordinatorObjectResponse::from_applied(&applied))
    }
}

impl ProfileCoordinator {
    async fn snapshot(&self, request: &mut Request) -> Result<Response> {
        let body = request.json::<CoordinatorSnapshotOwned>().await?;
        let tenant_id = TenantId::parse(body.tenant_id).map_err(identifier_error)?;
        let profile_id = ProfileId::parse(body.profile_id).map_err(identifier_error)?;
        let document = self.load_document(&tenant_id, &profile_id).await?;
        let projection = document.projection().map_err(adapter_error)?;
        Response::from_json(&CoordinatorObjectResponse::from_snapshot(projection))
    }

    async fn command(&self, request: &mut Request) -> Result<Response> {
        let body = request.json::<CoordinatorInternalCommandOwned>().await?;
        let tenant_id = TenantId::parse(body.tenant_id).map_err(identifier_error)?;
        let profile_id = ProfileId::parse(body.profile_id).map_err(identifier_error)?;
        let mut document = self.load_document(&tenant_id, &profile_id).await?;
        let applied = match document.apply(body.envelope) {
            Ok(value) => value,
            Err(error) => return coordinator_conflict(error),
        };
        if applied.appended() {
            self.state.storage().put(STORAGE_KEY, &document).await?;
        }
        schedule_alarm(&self.state, applied.next_alarm_at()).await?;
        Response::from_json(&CoordinatorObjectResponse::from_applied(&applied))
    }

    async fn load_document(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
    ) -> Result<StoredCoordinatorDocument> {
        if let Some(document) = self
            .state
            .storage()
            .get::<StoredCoordinatorDocument>(STORAGE_KEY)
            .await?
        {
            document
                .ensure_identity(tenant_id, profile_id)
                .map_err(adapter_error)?;
            return Ok(document);
        }
        Ok(StoredCoordinatorDocument::new(
            tenant_id,
            profile_id,
            CoordinatorConfig::new(IDLE_TIMEOUT_MS, HARD_TIMEOUT_MS, DRAIN_TIMEOUT_MS)
                .map_err(domain_error)?,
        ))
    }
}

#[derive(Deserialize)]
struct CoordinatorSnapshotOwned {
    tenant_id: String,
    profile_id: String,
}

#[derive(Deserialize)]
struct CoordinatorInternalCommandOwned {
    tenant_id: String,
    profile_id: String,
    envelope: StoredCoordinatorEnvelope,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CoordinatorObjectResponse {
    outcome: String,
    version: u64,
    sequence: u64,
    replayed: bool,
    fencing_token: Option<String>,
    epoch: Option<u64>,
    projection: CoordinatorProjection,
}

impl CoordinatorObjectResponse {
    fn from_snapshot(projection: CoordinatorProjection) -> Self {
        Self {
            outcome: "snapshot".to_owned(),
            version: projection.version,
            sequence: projection.sequence,
            replayed: true,
            fencing_token: None,
            epoch: projection.active_epoch,
            projection,
        }
    }

    fn from_applied(
        applied: &cloudflare_adapters::profile_coordinator::CoordinatorApplied,
    ) -> Self {
        let (fencing_token, epoch) = match applied.decision().outcome() {
            CoordinatorOutcome::LeaseClaimed { lease } => (
                Some(lease.fencing_token().as_str().to_owned()),
                Some(lease.epoch()),
            ),
            _ => (None, None),
        };
        Self {
            outcome: outcome_name(applied.decision().outcome()).to_owned(),
            version: applied.decision().version().value(),
            sequence: applied.decision().sequence(),
            replayed: !applied.appended(),
            fencing_token,
            epoch,
            projection: applied.projection().clone(),
        }
    }
}

async fn schedule_alarm(state: &State, deadline: Option<UnixMillis>) -> Result<()> {
    match deadline {
        Some(deadline) => {
            let date = Date::new(DateInit::Millis(deadline.value()));
            state
                .storage()
                .set_alarm(ScheduledTime::new(date.into()))
                .await
        }
        None => state.storage().delete_alarm().await,
    }
}

fn coordinator_conflict(error: CoordinatorAdapterError) -> Result<Response> {
    let code = match error {
        CoordinatorAdapterError::TenantMismatch | CoordinatorAdapterError::ProfileMismatch => 404,
        CoordinatorAdapterError::Identifier(_) | CoordinatorAdapterError::ZeroVersion(_) => 400,
        CoordinatorAdapterError::Domain(_) | CoordinatorAdapterError::JournalCapacityExceeded => {
            409
        }
    };
    Response::from_json(&CoordinatorErrorResponse {
        code: if code == 404 {
            "not_found"
        } else if code == 400 {
            "invalid_request"
        } else {
            "conflict"
        },
    })
    .map(|response| response.with_status(code))
}

#[derive(Serialize)]
struct CoordinatorErrorResponse {
    code: &'static str,
}

fn invalid_request(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        400,
        "invalid_request",
        "Invalid Request",
    )
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

fn adapter_error(error: CoordinatorAdapterError) -> Error {
    Error::RustError(error.to_string())
}

fn domain_error(error: session_domain::coordinator::CoordinatorError) -> Error {
    Error::RustError(error.to_string())
}

fn request_error(error: CoordinatorRequestError) -> Error {
    Error::RustError(format!("coordinator request error: {error:?}"))
}

fn json_error(error: serde_json::Error) -> Error {
    Error::RustError(error.to_string())
}
