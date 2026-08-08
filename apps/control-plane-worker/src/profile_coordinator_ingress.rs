use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use application_ports::coordinator_ingress::{
    CoordinatorProjectionSnapshot, CoordinatorRuntimeOutcome, CoordinatorRuntimeResult,
};
use cloudflare_adapters::coordinator_ingress::{
    CloudflareCoordinatorClock, CloudflareCoordinatorIngressApplication,
};
use cloudflare_adapters::d1_identity_acl::ResolvedMembershipRole;
use control_plane_contract::{D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    AggregateVersion, DeviceId, FencingToken, IdempotencyKey, LaunchIntentId, ProfileId, SessionId,
    TenantId,
};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::ReleaseDisposition;
use use_cases::coordinator_ingress::{
    CoordinatorCommandInput, CoordinatorIngressOperationError, CoordinatorIngressRequest,
    ExecuteCoordinatorCommand, execute_prepared_coordinator_ingress, prepare_coordinator_ingress,
};
use worker::{Env, Error, Method, Request, Response, Result};

pub async fn dispatch(
    request: &mut Request,
    env: &Env,
    tenant_value: &str,
    profile_value: &str,
) -> Result<Response> {
    if TenantId::parse(tenant_value.to_owned()).is_err() {
        return neutral_not_found(&correlation_hint(request));
    }
    let profile_id = match ProfileId::parse(profile_value.to_owned()) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(&correlation_hint(request)),
    };
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_value)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let role = membership_role(actor.role());
    let application = CloudflareCoordinatorIngressApplication::new(
        env,
        D1_CATALOG_BINDING,
        PROFILE_COORDINATOR_BINDING,
    );
    let access =
        match prepare_coordinator_ingress(actor.actor(), role, &profile_id, &application).await {
            Ok(value) => value,
            Err(error) => return operation_error(error, actor.actor().correlation_id().as_str()),
        };

    let command = match request.method() {
        Method::Get => CoordinatorIngressRequest::Snapshot,
        Method::Post => {
            let body = match request.json::<CoordinatorCommandRequest>().await {
                Ok(value) => value,
                Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
            let command = match body.into_application() {
                Ok(value) => value,
                Err(()) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
            CoordinatorIngressRequest::Command(command)
        }
        _ => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };

    let clock = CloudflareCoordinatorClock;
    let result = match execute_prepared_coordinator_ingress(
        actor.actor(),
        role,
        &access,
        &application,
        &clock,
        command,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return operation_error(error, actor.actor().correlation_id().as_str()),
    };
    Response::from_json(&CoordinatorApiResponse::from_runtime(&result))
}

fn membership_role(role: ResolvedMembershipRole) -> MembershipRole {
    match role {
        ResolvedMembershipRole::TenantOwner => MembershipRole::TenantOwner,
        ResolvedMembershipRole::Member => MembershipRole::Member,
    }
}

fn operation_error(
    error: CoordinatorIngressOperationError,
    correlation_id: &str,
) -> Result<Response> {
    match error {
        CoordinatorIngressOperationError::InvalidRequest => invalid_request(correlation_id),
        CoordinatorIngressOperationError::NotFound => neutral_not_found(correlation_id),
        CoordinatorIngressOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        CoordinatorIngressOperationError::IntegrityFailure
        | CoordinatorIngressOperationError::InternalFailure
        | CoordinatorIngressOperationError::DependencyUnavailable => {
            Err(Error::RustError(error.to_string()))
        }
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

#[derive(Deserialize)]
struct CoordinatorCommandRequest {
    idempotency_key: String,
    sequence: u64,
    expected_version: u64,
    command: CoordinatorApiCommand,
}

impl CoordinatorCommandRequest {
    fn into_application(self) -> Result<ExecuteCoordinatorCommand, ()> {
        Ok(ExecuteCoordinatorCommand::new(
            IdempotencyKey::parse(self.idempotency_key).map_err(|_| ())?,
            self.sequence,
            AggregateVersion::new(self.expected_version).map_err(|_| ())?,
            self.command.into_application()?,
        ))
    }
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
        disposition: CoordinatorApiReleaseDisposition,
    },
    BeginDrain,
    MarkRecovered,
}

impl CoordinatorApiCommand {
    fn into_application(self) -> Result<CoordinatorCommandInput, ()> {
        Ok(match self {
            Self::IssueLaunchIntent {
                launch_intent_id,
                device_id,
                expires_in_ms,
            } => CoordinatorCommandInput::IssueLaunchIntent {
                launch_intent_id: LaunchIntentId::parse(launch_intent_id).map_err(|_| ())?,
                device_id: DeviceId::parse(device_id).map_err(|_| ())?,
                expires_in_ms,
            },
            Self::Claim {
                launch_intent_id,
                device_id,
                session_id,
            } => CoordinatorCommandInput::Claim {
                launch_intent_id: LaunchIntentId::parse(launch_intent_id).map_err(|_| ())?,
                device_id: DeviceId::parse(device_id).map_err(|_| ())?,
                session_id: SessionId::parse(session_id).map_err(|_| ())?,
            },
            Self::Heartbeat {
                session_id,
                epoch,
                fencing_token,
            } => CoordinatorCommandInput::Heartbeat {
                session_id: SessionId::parse(session_id).map_err(|_| ())?,
                epoch,
                fencing_token: FencingToken::parse(fencing_token).map_err(|_| ())?,
            },
            Self::Release {
                session_id,
                epoch,
                fencing_token,
                disposition,
            } => CoordinatorCommandInput::Release {
                session_id: SessionId::parse(session_id).map_err(|_| ())?,
                epoch,
                fencing_token: FencingToken::parse(fencing_token).map_err(|_| ())?,
                disposition: disposition.into(),
            },
            Self::BeginDrain => CoordinatorCommandInput::BeginDrain,
            Self::MarkRecovered => CoordinatorCommandInput::MarkRecovered,
        })
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CoordinatorApiReleaseDisposition {
    Clean,
    Dirty,
    Uncertain,
}

impl From<CoordinatorApiReleaseDisposition> for ReleaseDisposition {
    fn from(value: CoordinatorApiReleaseDisposition) -> Self {
        match value {
            CoordinatorApiReleaseDisposition::Clean => Self::Clean,
            CoordinatorApiReleaseDisposition::Dirty => Self::Dirty,
            CoordinatorApiReleaseDisposition::Uncertain => Self::Uncertain,
        }
    }
}

#[derive(Serialize)]
struct CoordinatorApiResponse {
    outcome: &'static str,
    version: u64,
    sequence: u64,
    replayed: bool,
    fencing_token: Option<String>,
    epoch: Option<u64>,
    projection: CoordinatorApiProjection,
}

impl CoordinatorApiResponse {
    fn from_runtime(value: &CoordinatorRuntimeResult) -> Self {
        Self {
            outcome: outcome_name(value.outcome()),
            version: value.version().value(),
            sequence: value.sequence(),
            replayed: value.replayed(),
            fencing_token: value.fencing_token().map(|token| token.as_str().to_owned()),
            epoch: value.epoch(),
            projection: CoordinatorApiProjection::from_snapshot(value.projection()),
        }
    }
}

#[derive(Serialize)]
struct CoordinatorApiProjection {
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

impl CoordinatorApiProjection {
    fn from_snapshot(value: &CoordinatorProjectionSnapshot) -> Self {
        Self {
            tenant_id: value.tenant_id().as_str().to_owned(),
            profile_id: value.profile_id().as_str().to_owned(),
            status: value.status().to_owned(),
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
            idle_expires_at_ms: value.idle_expires_at().map(|item| item.value()),
            hard_expires_at_ms: value.hard_expires_at().map(|item| item.value()),
            drain_deadline_ms: value.drain_deadline().map(|item| item.value()),
            pending_launch_intent_id: value
                .pending_launch_intent_id()
                .map(|item| item.as_str().to_owned()),
            pending_intent_expires_at_ms: value
                .pending_intent_expires_at()
                .map(|item| item.value()),
        }
    }
}

const fn outcome_name(value: CoordinatorRuntimeOutcome) -> &'static str {
    match value {
        CoordinatorRuntimeOutcome::Snapshot => "snapshot",
        CoordinatorRuntimeOutcome::LaunchIntentIssued => "launch_intent_issued",
        CoordinatorRuntimeOutcome::LeaseClaimed => "lease_claimed",
        CoordinatorRuntimeOutcome::HeartbeatAccepted => "heartbeat_accepted",
        CoordinatorRuntimeOutcome::Released => "released",
        CoordinatorRuntimeOutcome::DrainStarted => "drain_started",
        CoordinatorRuntimeOutcome::TimedOut => "timed_out",
        CoordinatorRuntimeOutcome::LaunchIntentExpired => "launch_intent_expired",
        CoordinatorRuntimeOutcome::Recovered => "recovered",
        CoordinatorRuntimeOutcome::NoChange => "no_change",
    }
}
