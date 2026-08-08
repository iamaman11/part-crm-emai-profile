use application_ports::ClockPort;
use application_ports::coordinator_ingress::{
    CoordinatorIngressApplicationPort, CoordinatorIngressPortError, CoordinatorIngressPortErrorClass,
    CoordinatorProfileAccess, CoordinatorProjectionSnapshot, CoordinatorRuntimeOutcome,
    CoordinatorRuntimeResult,
};
use cloudflare_adapters::d1_identity_queries::D1IdentityQueryRepository;
use cloudflare_adapters::d1_profile_coordinator::{
    CoordinatorProjectionMutation, CoordinatorProjectionOutcome, D1ProfileCoordinatorRepository,
};
use cloudflare_adapters::profile_coordinator::{
    StoredCoordinatorCommand, StoredCoordinatorEnvelope,
};
use control_plane_contract::{D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING};
use identity_access_domain::MembershipRole;
use js_sys::Reflect;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, FencingToken, LaunchIntentId, OutboxEventId,
    ProfileId, SessionId, TenantScope, UnixMillis,
};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::{
    CoordinatorCommand, CoordinatorCommandEnvelope, ReleaseDisposition, coordinator_object_name,
};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::WorkerGlobalScope;
use worker::{Date, Env, Fetch, Headers, Method, Request, RequestInit, Result as WorkerResult};

pub struct WorkerCoordinatorClock;

impl ClockPort for WorkerCoordinatorClock {
    fn now(&self) -> UnixMillis {
        UnixMillis::new(Date::now().as_millis())
    }
}

pub struct WorkerCoordinatorIngressApplication {
    env: Env,
}

impl WorkerCoordinatorIngressApplication {
    #[must_use]
    pub fn new(env: &Env) -> Self {
        Self { env: env.clone() }
    }

    async fn runtime_request(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        envelope: Option<&CoordinatorCommandEnvelope>,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError> {
        let namespace = self
            .env
            .durable_object(PROFILE_COORDINATOR_BINDING)
            .map_err(map_worker_dependency)?;
        let object_id = namespace
            .id_from_name(&coordinator_object_name(profile_id))
            .map_err(map_worker_dependency)?;
        let stub = object_id.get_stub().map_err(map_worker_dependency)?;

        let request = match envelope {
            None => internal_request(
                "https://coordinator.internal/snapshot",
                &CoordinatorSnapshotRequest {
                    tenant_id: scope.tenant_id().as_str(),
                    profile_id: profile_id.as_str(),
                },
            )?,
            Some(envelope) => internal_request(
                "https://coordinator.internal/command",
                &CoordinatorInternalCommandRequest {
                    tenant_id: scope.tenant_id().as_str(),
                    profile_id: profile_id.as_str(),
                    envelope: StoredCoordinatorEnvelope::new(
                        envelope.idempotency_key().as_str().to_owned(),
                        envelope.sequence(),
                        envelope.expected_version().value(),
                        stored_command(envelope.command()),
                    ),
                },
            )?,
        };
        let mut response = stub.fetch_with_request(request).await.map_err(map_worker_dependency)?;
        if response.status_code() != 200 {
            return Err(CoordinatorIngressPortError::new(
                if response.status_code() == 404 {
                    CoordinatorIngressPortErrorClass::NotFound
                } else if response.status_code() == 409 {
                    CoordinatorIngressPortErrorClass::Conflict
                } else {
                    CoordinatorIngressPortErrorClass::DependencyUnavailable
                },
            ));
        }
        let response = response
            .json::<CoordinatorObjectResponse>()
            .await
            .map_err(map_worker_dependency)?;
        runtime_result(response)
    }
}

impl CoordinatorIngressApplicationPort for WorkerCoordinatorIngressApplication {
    async fn find_visible_profile(
        &self,
        actor: &ActorContext,
        role: MembershipRole,
        profile_id: &ProfileId,
    ) -> Result<Option<CoordinatorProfileAccess>, CoordinatorIngressPortError> {
        D1IdentityQueryRepository::new(
            self.env
                .d1(D1_CATALOG_BINDING)
                .map_err(map_worker_dependency)?,
        )
        .find_visible_profile(actor, role, profile_id)
        .await
        .map(|row| {
            row.map(|visible| {
                CoordinatorProfileAccess::new(
                    visible.status,
                    visible.active_generation_id.is_some(),
                )
            })
        })
        .map_err(map_worker_dependency)
    }

    fn new_fencing_token(&self) -> Result<FencingToken, CoordinatorIngressPortError> {
        FencingToken::parse(format!("fence_{}", random_uuid()?))
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))
    }

    fn new_outbox_event_id(&self) -> Result<OutboxEventId, CoordinatorIngressPortError> {
        OutboxEventId::parse(format!("outbox_{}", random_uuid()?))
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))
    }

    async fn snapshot(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError> {
        self.runtime_request(scope, profile_id, None).await
    }

    async fn execute(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        envelope: &CoordinatorCommandEnvelope,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError> {
        self.runtime_request(scope, profile_id, Some(envelope)).await
    }

    async fn project(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        result: &CoordinatorRuntimeResult,
        outbox_event_id: &OutboxEventId,
        projected_at: UnixMillis,
    ) -> Result<(), CoordinatorIngressPortError> {
        let projection = result.projection();
        if projection.tenant_id() != scope.tenant_id() || projection.profile_id() != profile_id {
            return Err(CoordinatorIngressPortError::new(
                CoordinatorIngressPortErrorClass::IntegrityFailure,
            ));
        }
        let actor = ActorContext::new(
            scope.clone(),
            profile_platform_primitives::ActorId::parse("actor_coordinator_projection")
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))?,
            profile_platform_primitives::CorrelationId::parse("corr_coordinator_projection")
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))?,
        );
        D1ProfileCoordinatorRepository::new(
            self.env
                .d1(D1_CATALOG_BINDING)
                .map_err(map_worker_dependency)?,
        )
        .persist_projection(
            &actor,
            CoordinatorProjectionMutation {
                profile_id,
                outcome: projection_outcome(result.outcome()),
                coordinator_version: result.version(),
                coordinator_sequence: result.sequence(),
                status: projection.status(),
                next_epoch: projection.next_epoch(),
                active_session_id: projection.active_session_id(),
                active_device_id: projection.active_device_id(),
                active_epoch: projection.active_epoch(),
                idle_expires_at: projection.idle_expires_at(),
                hard_expires_at: projection.hard_expires_at(),
                drain_deadline: projection.drain_deadline(),
                pending_launch_intent_id: projection.pending_launch_intent_id(),
                pending_intent_expires_at: projection.pending_intent_expires_at(),
                outbox_event_id,
                projected_at,
            },
        )
        .await
        .map(|_| ())
        .map_err(map_worker_dependency)
    }
}

fn stored_command(command: &CoordinatorCommand) -> StoredCoordinatorCommand {
    match command {
        CoordinatorCommand::IssueLaunchIntent {
            launch_intent_id,
            actor_id,
            device_id,
            now,
            expires_at,
        } => StoredCoordinatorCommand::IssueLaunchIntent {
            launch_intent_id: launch_intent_id.as_str().to_owned(),
            actor_id: actor_id.as_str().to_owned(),
            device_id: device_id.as_str().to_owned(),
            now_ms: now.value(),
            expires_at_ms: expires_at.value(),
        },
        CoordinatorCommand::Claim {
            launch_intent_id,
            actor_id,
            device_id,
            session_id,
            fencing_token,
            now,
        } => StoredCoordinatorCommand::Claim {
            launch_intent_id: launch_intent_id.as_str().to_owned(),
            actor_id: actor_id.as_str().to_owned(),
            device_id: device_id.as_str().to_owned(),
            session_id: session_id.as_str().to_owned(),
            fencing_token: fencing_token.as_str().to_owned(),
            now_ms: now.value(),
        },
        CoordinatorCommand::Heartbeat {
            session_id,
            epoch,
            fencing_token,
            now,
        } => StoredCoordinatorCommand::Heartbeat {
            session_id: session_id.as_str().to_owned(),
            epoch: *epoch,
            fencing_token: fencing_token.as_str().to_owned(),
            now_ms: now.value(),
        },
        CoordinatorCommand::Release {
            session_id,
            epoch,
            fencing_token,
            disposition,
            now,
        } => StoredCoordinatorCommand::Release {
            session_id: session_id.as_str().to_owned(),
            epoch: *epoch,
            fencing_token: fencing_token.as_str().to_owned(),
            disposition: match disposition {
                ReleaseDisposition::Clean => "clean".to_owned(),
                ReleaseDisposition::DirtyLocal => "dirty_local".to_owned(),
            },
            now_ms: now.value(),
        },
        CoordinatorCommand::BeginDrain { now } => StoredCoordinatorCommand::BeginDrain {
            now_ms: now.value(),
        },
        CoordinatorCommand::Tick { now } => StoredCoordinatorCommand::Tick {
            now_ms: now.value(),
        },
        CoordinatorCommand::MarkRecovered { now } => StoredCoordinatorCommand::MarkRecovered {
            now_ms: now.value(),
        },
    }
}

fn runtime_result(
    response: CoordinatorObjectResponse,
) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError> {
    let projection = response.projection;
    Ok(CoordinatorRuntimeResult::new(
        runtime_outcome(&response.outcome)?,
        AggregateVersion::new(response.version)
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
        response.sequence,
        response.replayed,
        response
            .fencing_token
            .map(FencingToken::parse)
            .transpose()
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
        response.epoch,
        CoordinatorProjectionSnapshot::new(
            profile_platform_primitives::TenantId::parse(projection.tenant_id)
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
            ProfileId::parse(projection.profile_id)
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
            projection.status,
            AggregateVersion::new(projection.version)
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
            projection.sequence,
            projection.next_epoch,
            projection.active_session_id.map(SessionId::parse).transpose()
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
            projection.active_device_id.map(DeviceId::parse).transpose()
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
            projection.active_epoch,
            projection.idle_expires_at_ms.map(UnixMillis::new),
            projection.hard_expires_at_ms.map(UnixMillis::new),
            projection.drain_deadline_ms.map(UnixMillis::new),
            projection.pending_launch_intent_id.map(LaunchIntentId::parse).transpose()
                .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure))?,
            projection.pending_intent_expires_at_ms.map(UnixMillis::new),
        ),
    ))
}

fn runtime_outcome(value: &str) -> Result<CoordinatorRuntimeOutcome, CoordinatorIngressPortError> {
    match value {
        "snapshot" => Ok(CoordinatorRuntimeOutcome::Snapshot),
        "launch_intent_issued" => Ok(CoordinatorRuntimeOutcome::LaunchIntentIssued),
        "lease_claimed" => Ok(CoordinatorRuntimeOutcome::LeaseClaimed),
        "heartbeat_accepted" => Ok(CoordinatorRuntimeOutcome::HeartbeatAccepted),
        "released" => Ok(CoordinatorRuntimeOutcome::Released),
        "drain_started" => Ok(CoordinatorRuntimeOutcome::DrainStarted),
        "timed_out" => Ok(CoordinatorRuntimeOutcome::TimedOut),
        "launch_intent_expired" => Ok(CoordinatorRuntimeOutcome::LaunchIntentExpired),
        "recovered" => Ok(CoordinatorRuntimeOutcome::Recovered),
        "no_change" => Ok(CoordinatorRuntimeOutcome::NoChange),
        _ => Err(CoordinatorIngressPortError::new(
            CoordinatorIngressPortErrorClass::IntegrityFailure,
        )),
    }
}

const fn projection_outcome(value: CoordinatorRuntimeOutcome) -> CoordinatorProjectionOutcome {
    match value {
        CoordinatorRuntimeOutcome::Snapshot => CoordinatorProjectionOutcome::Snapshot,
        CoordinatorRuntimeOutcome::LaunchIntentIssued => CoordinatorProjectionOutcome::LaunchIntentIssued,
        CoordinatorRuntimeOutcome::LeaseClaimed => CoordinatorProjectionOutcome::LeaseClaimed,
        CoordinatorRuntimeOutcome::HeartbeatAccepted => CoordinatorProjectionOutcome::HeartbeatAccepted,
        CoordinatorRuntimeOutcome::Released => CoordinatorProjectionOutcome::Released,
        CoordinatorRuntimeOutcome::DrainStarted => CoordinatorProjectionOutcome::DrainStarted,
        CoordinatorRuntimeOutcome::TimedOut => CoordinatorProjectionOutcome::TimedOut,
        CoordinatorRuntimeOutcome::LaunchIntentExpired => CoordinatorProjectionOutcome::LaunchIntentExpired,
        CoordinatorRuntimeOutcome::Recovered => CoordinatorProjectionOutcome::Recovered,
        CoordinatorRuntimeOutcome::NoChange => CoordinatorProjectionOutcome::NoChange,
    }
}

fn map_worker_dependency(_error: worker::Error) -> CoordinatorIngressPortError {
    CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::DependencyUnavailable)
}

fn random_uuid() -> Result<String, CoordinatorIngressPortError> {
    let global = js_sys::global();
    let scope: WorkerGlobalScope = global
        .dyn_into()
        .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))?;
    Reflect::apply(
        &Reflect::get(scope.crypto().as_ref(), &JsValue::from_str("randomUUID"))
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))?,
        scope.crypto().as_ref(),
        &js_sys::Array::new(),
    )
    .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))?
    .as_string()
    .ok_or_else(|| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))
}

fn internal_request<T: Serialize>(
    url: &str,
    body: &T,
) -> Result<Request, CoordinatorIngressPortError> {
    let body = serde_json::to_string(body)
        .map_err(|_| CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure))?;
    let headers = Headers::new();
    headers
        .set("content-type", "application/json")
        .map_err(map_worker_dependency)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.into()));
    Request::new_with_init(url, &init).map_err(map_worker_dependency)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoordinatorSnapshotRequest<'a> {
    tenant_id: &'a str,
    profile_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoordinatorInternalCommandRequest<'a> {
    tenant_id: &'a str,
    profile_id: &'a str,
    envelope: StoredCoordinatorEnvelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoordinatorObjectResponse {
    outcome: String,
    version: u64,
    sequence: u64,
    replayed: bool,
    fencing_token: Option<String>,
    epoch: Option<u64>,
    projection: CoordinatorProjectionResponse,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoordinatorProjectionResponse {
    tenant_id: String,
    profile_id: String,
    status: String,
    version: u64,
    sequence: u64,
    next_epoch: u64,
    active_session_id: Option<String>,
    active_device_id: Option<String>,
    active_epoch: Option<u64>,
    idle_expires_at_ms: Option<u64>,
    hard_expires_at_ms: Option<u64>,
    drain_deadline_ms: Option<u64>,
    pending_launch_intent_id: Option<String>,
    pending_intent_expires_at_ms: Option<u64>,
}
