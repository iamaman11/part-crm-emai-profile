use crate::d1_identity_acl::ResolvedMembershipRole;
use crate::d1_identity_queries::D1IdentityQueryRepository;
use crate::d1_profile_coordinator::{
    CoordinatorProjectionMutation, CoordinatorProjectionOutcome, D1ProfileCoordinatorRepository,
};
use crate::device_generation_commit_runtime::{
    DEVICE_GENERATION_COMMIT_PATH, DeviceGenerationCommitInternalErrorClass,
    DeviceGenerationCommitInternalErrorResponse, DeviceGenerationCommitInternalOutcome,
    DeviceGenerationCommitInternalRequest, DeviceGenerationCommitInternalResponse,
};
use crate::profile_coordinator::{
    CoordinatorProjection, StoredCoordinatorCommand, StoredCoordinatorEnvelope,
    StoredReleaseDisposition,
};
use application_ports::ClockPort;
use application_ports::coordinator_ingress::{
    CoordinatorIngressApplicationPort, CoordinatorIngressPortError,
    CoordinatorIngressPortErrorClass, CoordinatorProfileAccess, CoordinatorProjectionSnapshot,
    CoordinatorRuntimeOutcome, CoordinatorRuntimeResult,
};
use application_ports::device_generation_commit::{
    DeviceGenerationCommitError, DeviceGenerationCommitErrorClass, DeviceGenerationCommitOutcome,
    DeviceGenerationCommitPort, DeviceGenerationCommitRequest,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, FencingToken, LaunchIntentId, OutboxEventId,
    ProfileId, SessionId, TenantScope, UnixMillis,
};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::{
    CoordinatorCommand, CoordinatorCommandEnvelope, CoordinatorStatus, ReleaseDisposition,
    coordinator_object_name,
};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::web_sys::WorkerGlobalScope;
use worker::{Date, Env, Fetch, Headers, Method, Request, RequestInit};

pub struct CloudflareCoordinatorClock;

impl ClockPort for CloudflareCoordinatorClock {
    fn now(&self) -> UnixMillis {
        UnixMillis::new(Date::now().as_millis())
    }
}

pub struct CloudflareCoordinatorIngressApplication<'a> {
    env: &'a Env,
    d1_binding: &'a str,
    coordinator_binding: &'a str,
}

impl<'a> CloudflareCoordinatorIngressApplication<'a> {
    #[must_use]
    pub const fn new(env: &'a Env, d1_binding: &'a str, coordinator_binding: &'a str) -> Self {
        Self {
            env,
            d1_binding,
            coordinator_binding,
        }
    }

    async fn runtime_request(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        envelope: Option<&CoordinatorCommandEnvelope>,
    ) -> Result<CoordinatorRuntimeResult, CoordinatorIngressPortError> {
        let namespace = self
            .env
            .durable_object(self.coordinator_binding)
            .map_err(map_worker_dependency)?;
        let object_id = namespace
            .id_from_name(&coordinator_object_name(profile_id))
            .map_err(map_worker_dependency)?;
        let stub = object_id.get_stub().map_err(map_worker_dependency)?;

        let request = match envelope {
            None => internal_request(
                "/snapshot",
                &CoordinatorSnapshotRequest {
                    tenant_id: scope.tenant_id().as_str(),
                    profile_id: profile_id.as_str(),
                },
            )?,
            Some(envelope) => internal_request(
                "/command",
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
        let mut response = stub
            .fetch_with_request(request)
            .await
            .map_err(map_worker_dependency)?;
        if response.status_code() != 200 {
            let class = match response.status_code() {
                400 => CoordinatorIngressPortErrorClass::InvalidRequest,
                404 => CoordinatorIngressPortErrorClass::NotFound,
                409 => CoordinatorIngressPortErrorClass::Conflict,
                _ => CoordinatorIngressPortErrorClass::DependencyUnavailable,
            };
            return Err(CoordinatorIngressPortError::new(class));
        }
        let response = response
            .json::<CoordinatorObjectResponse>()
            .await
            .map_err(map_worker_dependency)?;
        runtime_result(response)
    }
}

pub struct CloudflareDeviceGenerationCommitPort<'a> {
    env: &'a Env,
    coordinator_binding: &'a str,
}

impl<'a> CloudflareDeviceGenerationCommitPort<'a> {
    #[must_use]
    pub const fn new(env: &'a Env, coordinator_binding: &'a str) -> Self {
        Self {
            env,
            coordinator_binding,
        }
    }
}

impl DeviceGenerationCommitPort for CloudflareDeviceGenerationCommitPort<'_> {
    async fn commit_device_generation(
        &self,
        actor: &ActorContext,
        request: &DeviceGenerationCommitRequest,
    ) -> Result<DeviceGenerationCommitOutcome, DeviceGenerationCommitError> {
        let namespace = self
            .env
            .durable_object(self.coordinator_binding)
            .map_err(|_| generation_commit_dependency())?;
        let object_id = namespace
            .id_from_name(&coordinator_object_name(request.profile_id()))
            .map_err(|_| generation_commit_dependency())?;
        let stub = object_id
            .get_stub()
            .map_err(|_| generation_commit_dependency())?;
        let internal = DeviceGenerationCommitInternalRequest::from_domain(actor, request);
        let request = generation_commit_internal_request(&internal)?;
        let mut response = stub
            .fetch_with_request(request)
            .await
            .map_err(|_| generation_commit_dependency())?;

        if response.status_code() == 200 {
            let body = response
                .json::<DeviceGenerationCommitInternalResponse>()
                .await
                .map_err(|_| generation_commit_integrity())?;
            return Ok(match body.outcome {
                DeviceGenerationCommitInternalOutcome::Activated => {
                    DeviceGenerationCommitOutcome::Activated
                }
                DeviceGenerationCommitInternalOutcome::AlreadyActive => {
                    DeviceGenerationCommitOutcome::AlreadyActive
                }
            });
        }

        let status = response.status_code();
        let body = response
            .json::<DeviceGenerationCommitInternalErrorResponse>()
            .await
            .map_err(|_| generation_commit_dependency())?;
        Err(match (status, body.class) {
            (409, DeviceGenerationCommitInternalErrorClass::StaleAuthority) => {
                generation_commit_stale_authority()
            }
            (409, DeviceGenerationCommitInternalErrorClass::VersionConflict) => {
                generation_commit_version_conflict()
            }
            (400 | 500, DeviceGenerationCommitInternalErrorClass::IntegrityFailure) => {
                generation_commit_integrity()
            }
            (503, DeviceGenerationCommitInternalErrorClass::DependencyUnavailable) => {
                generation_commit_dependency()
            }
            _ => generation_commit_dependency(),
        })
    }
}

impl CoordinatorIngressApplicationPort for CloudflareCoordinatorIngressApplication<'_> {
    async fn find_visible_profile(
        &self,
        actor: &ActorContext,
        role: MembershipRole,
        profile_id: &ProfileId,
    ) -> Result<Option<CoordinatorProfileAccess>, CoordinatorIngressPortError> {
        let role = match role {
            MembershipRole::TenantOwner => ResolvedMembershipRole::TenantOwner,
            MembershipRole::Member => ResolvedMembershipRole::Member,
        };
        D1IdentityQueryRepository::new(
            self.env
                .d1(self.d1_binding)
                .map_err(map_worker_dependency)?,
        )
        .find_visible_profile(actor.tenant_scope(), actor.actor_id(), role, profile_id)
        .await
        .map(|row| {
            row.map(|visible| {
                CoordinatorProfileAccess::new(
                    visible.status(),
                    visible.active_generation_id().is_some(),
                )
            })
        })
        .map_err(map_worker_dependency)
    }

    fn new_fencing_token(&self) -> Result<FencingToken, CoordinatorIngressPortError> {
        FencingToken::parse(format!("fence_{}", random_uuid()?)).map_err(|_| {
            CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure)
        })
    }

    fn new_outbox_event_id(&self) -> Result<OutboxEventId, CoordinatorIngressPortError> {
        OutboxEventId::parse(format!("outbox_{}", random_uuid()?)).map_err(|_| {
            CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure)
        })
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
        self.runtime_request(scope, profile_id, Some(envelope))
            .await
    }

    async fn project(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        result: &CoordinatorRuntimeResult,
        outbox_event_id: &OutboxEventId,
        projected_at: UnixMillis,
    ) -> Result<(), CoordinatorIngressPortError> {
        let projection = coordinator_projection(result.projection());
        D1ProfileCoordinatorRepository::new(
            self.env
                .d1(self.d1_binding)
                .map_err(map_worker_dependency)?,
        )
        .project(
            scope,
            CoordinatorProjectionMutation {
                profile_id,
                projection: &projection,
                outcome: projection_outcome(result.outcome()),
                outbox_event_id,
                projected_at,
            },
        )
        .await
        .map(|_| ())
        .map_err(map_worker_dependency)
    }
}

fn coordinator_projection(value: &CoordinatorProjectionSnapshot) -> CoordinatorProjection {
    CoordinatorProjection {
        tenant_id: value.tenant_id().as_str().to_owned(),
        profile_id: value.profile_id().as_str().to_owned(),
        status: coordinator_status_name(value.status()).to_owned(),
        version: value.version().value(),
        sequence: value.sequence(),
        next_epoch: value.next_epoch(),
        active_session_id: value
            .active_session_id()
            .map(|item| item.as_str().to_owned()),
        active_device_id: value
            .active_device_id()
            .map(|item| item.as_str().to_owned()),
        active_epoch: value.active_epoch(),
        idle_expires_at_ms: value.idle_expires_at().map(UnixMillis::value),
        hard_expires_at_ms: value.hard_expires_at().map(UnixMillis::value),
        drain_deadline_ms: value.drain_deadline().map(UnixMillis::value),
        pending_launch_intent_id: value
            .pending_launch_intent_id()
            .map(|item| item.as_str().to_owned()),
        pending_intent_expires_at_ms: value.pending_intent_expires_at().map(UnixMillis::value),
    }
}

const fn coordinator_status_name(value: CoordinatorStatus) -> &'static str {
    match value {
        CoordinatorStatus::Idle => "idle",
        CoordinatorStatus::Active => "active",
        CoordinatorStatus::Draining => "draining",
        CoordinatorStatus::Dirty => "dirty",
        CoordinatorStatus::Uncertain => "uncertain",
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
                ReleaseDisposition::Clean => StoredReleaseDisposition::Clean,
                ReleaseDisposition::Dirty => StoredReleaseDisposition::Dirty,
                ReleaseDisposition::Uncertain => StoredReleaseDisposition::Uncertain,
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
        AggregateVersion::new(response.version).map_err(|_| integrity_failure())?,
        response.sequence,
        response.replayed,
        response
            .fencing_token
            .map(FencingToken::parse)
            .transpose()
            .map_err(|_| integrity_failure())?,
        response.epoch,
        CoordinatorProjectionSnapshot::new(
            profile_platform_primitives::TenantId::parse(projection.tenant_id)
                .map_err(|_| integrity_failure())?,
            ProfileId::parse(projection.profile_id).map_err(|_| integrity_failure())?,
            runtime_status(&projection.status)?,
            AggregateVersion::new(projection.version).map_err(|_| integrity_failure())?,
            projection.sequence,
            projection.next_epoch,
            projection
                .active_session_id
                .map(SessionId::parse)
                .transpose()
                .map_err(|_| integrity_failure())?,
            projection
                .active_device_id
                .map(DeviceId::parse)
                .transpose()
                .map_err(|_| integrity_failure())?,
            projection.active_epoch,
            projection.idle_expires_at_ms.map(UnixMillis::new),
            projection.hard_expires_at_ms.map(UnixMillis::new),
            projection.drain_deadline_ms.map(UnixMillis::new),
            projection
                .pending_launch_intent_id
                .map(LaunchIntentId::parse)
                .transpose()
                .map_err(|_| integrity_failure())?,
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
        _ => Err(integrity_failure()),
    }
}

fn runtime_status(value: &str) -> Result<CoordinatorStatus, CoordinatorIngressPortError> {
    match value {
        "idle" => Ok(CoordinatorStatus::Idle),
        "active" => Ok(CoordinatorStatus::Active),
        "draining" => Ok(CoordinatorStatus::Draining),
        "dirty" => Ok(CoordinatorStatus::Dirty),
        "uncertain" => Ok(CoordinatorStatus::Uncertain),
        _ => Err(integrity_failure()),
    }
}

const fn projection_outcome(value: CoordinatorRuntimeOutcome) -> CoordinatorProjectionOutcome {
    match value {
        CoordinatorRuntimeOutcome::Snapshot => CoordinatorProjectionOutcome::Snapshot,
        CoordinatorRuntimeOutcome::LaunchIntentIssued => {
            CoordinatorProjectionOutcome::LaunchIntentIssued
        }
        CoordinatorRuntimeOutcome::LeaseClaimed => CoordinatorProjectionOutcome::LeaseClaimed,
        CoordinatorRuntimeOutcome::HeartbeatAccepted => {
            CoordinatorProjectionOutcome::HeartbeatAccepted
        }
        CoordinatorRuntimeOutcome::Released => CoordinatorProjectionOutcome::Released,
        CoordinatorRuntimeOutcome::DrainStarted => CoordinatorProjectionOutcome::DrainStarted,
        CoordinatorRuntimeOutcome::TimedOut => CoordinatorProjectionOutcome::TimedOut,
        CoordinatorRuntimeOutcome::LaunchIntentExpired => {
            CoordinatorProjectionOutcome::LaunchIntentExpired
        }
        CoordinatorRuntimeOutcome::Recovered => CoordinatorProjectionOutcome::Recovered,
        CoordinatorRuntimeOutcome::NoChange => CoordinatorProjectionOutcome::NoChange,
    }
}

fn random_uuid() -> Result<String, CoordinatorIngressPortError> {
    let global: WorkerGlobalScope = worker::js_sys::global().unchecked_into();
    let crypto = global.crypto().map_err(|_| internal_failure())?;
    Ok(crypto.random_uuid())
}

fn internal_request<T: Serialize>(
    path: &str,
    body: &T,
) -> Result<Request, CoordinatorIngressPortError> {
    let payload = serde_json::to_string(body).map_err(|_| internal_failure())?;
    let headers = Headers::new();
    headers
        .set("content-type", "application/json")
        .map_err(map_worker_dependency)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&payload)));
    Request::new_with_init(
        &format!("https://profile-coordinator.internal{path}"),
        &init,
    )
    .map_err(map_worker_dependency)
}

fn generation_commit_internal_request(
    body: &DeviceGenerationCommitInternalRequest,
) -> Result<Request, DeviceGenerationCommitError> {
    let payload = serde_json::to_string(body).map_err(|_| generation_commit_integrity())?;
    let headers = Headers::new();
    headers
        .set("content-type", "application/json")
        .map_err(|_| generation_commit_dependency())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&payload)));
    Request::new_with_init(
        &format!("https://profile-coordinator.internal{DEVICE_GENERATION_COMMIT_PATH}"),
        &init,
    )
    .map_err(|_| generation_commit_dependency())
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

#[derive(Deserialize)]
struct CoordinatorObjectResponse {
    outcome: String,
    version: u64,
    sequence: u64,
    replayed: bool,
    fencing_token: Option<String>,
    epoch: Option<u64>,
    projection: CoordinatorProjection,
}

fn map_worker_dependency(_error: worker::Error) -> CoordinatorIngressPortError {
    CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::DependencyUnavailable)
}

const fn integrity_failure() -> CoordinatorIngressPortError {
    CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::IntegrityFailure)
}

const fn internal_failure() -> CoordinatorIngressPortError {
    CoordinatorIngressPortError::new(CoordinatorIngressPortErrorClass::InternalFailure)
}

const fn generation_commit_stale_authority() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::StaleAuthority)
}

const fn generation_commit_version_conflict() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::VersionConflict)
}

const fn generation_commit_integrity() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::IntegrityFailure)
}

const fn generation_commit_dependency() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::DependencyUnavailable)
}
