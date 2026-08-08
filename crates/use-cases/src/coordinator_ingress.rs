use application_ports::coordinator_ingress::{
    CoordinatorIngressApplicationPort, CoordinatorIngressClockPort, CoordinatorIngressPortError,
    CoordinatorIngressPortErrorClass, CoordinatorProfileAccess, CoordinatorRuntimeResult,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, FencingToken, IdempotencyKey, LaunchIntentId,
    ProfileId, SessionId, UnixMillis,
};
use session_domain::coordinator::{
    CoordinatorCommand, CoordinatorCommandEnvelope, ReleaseDisposition,
};

const MIN_INTENT_TTL_MS: u64 = 1_000;
const MAX_INTENT_TTL_MS: u64 = 300_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinatorIngressOperationError {
    InvalidRequest,
    NotFound,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for CoordinatorIngressOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "coordinator request is invalid",
            Self::NotFound => "coordinator resource not found",
            Self::Conflict => "coordinator conflict",
            Self::IntegrityFailure => "coordinator integrity failure",
            Self::InternalFailure => "coordinator internal failure",
            Self::DependencyUnavailable => "coordinator dependency unavailable",
        })
    }
}

impl std::error::Error for CoordinatorIngressOperationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoordinatorIngressRequest {
    Snapshot,
    Command(ExecuteCoordinatorCommand),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteCoordinatorCommand {
    idempotency_key: IdempotencyKey,
    sequence: u64,
    expected_version: AggregateVersion,
    command: CoordinatorCommandInput,
}

impl ExecuteCoordinatorCommand {
    #[must_use]
    pub const fn new(
        idempotency_key: IdempotencyKey,
        sequence: u64,
        expected_version: AggregateVersion,
        command: CoordinatorCommandInput,
    ) -> Self {
        Self {
            idempotency_key,
            sequence,
            expected_version,
            command,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoordinatorCommandInput {
    IssueLaunchIntent {
        launch_intent_id: LaunchIntentId,
        device_id: DeviceId,
        expires_in_ms: u64,
    },
    Claim {
        launch_intent_id: LaunchIntentId,
        device_id: DeviceId,
        session_id: SessionId,
    },
    Heartbeat {
        session_id: SessionId,
        epoch: u64,
        fencing_token: FencingToken,
    },
    Release {
        session_id: SessionId,
        epoch: u64,
        fencing_token: FencingToken,
        disposition: ReleaseDisposition,
    },
    BeginDrain,
    MarkRecovered,
}

pub async fn execute_coordinator_ingress<
    P: CoordinatorIngressApplicationPort,
    C: CoordinatorIngressClockPort,
>(
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: &ProfileId,
    port: &P,
    clock: &C,
    request: CoordinatorIngressRequest,
) -> Result<CoordinatorRuntimeResult, CoordinatorIngressOperationError> {
    let profile = port
        .find_visible_profile(actor, role, profile_id)
        .await
        .map_err(map_port_error)?
        .ok_or(CoordinatorIngressOperationError::NotFound)?;
    require_coordinatable(&profile)?;

    let result = match request {
        CoordinatorIngressRequest::Snapshot => {
            port.snapshot(actor.tenant_scope(), profile_id)
                .await
                .map_err(map_port_error)?
        }
        CoordinatorIngressRequest::Command(command) => {
            if matches!(command.command, CoordinatorCommandInput::MarkRecovered)
                && role != MembershipRole::TenantOwner
            {
                return Err(CoordinatorIngressOperationError::NotFound);
            }
            let envelope = build_envelope(actor, port, clock, command)?;
            port.execute(actor.tenant_scope(), profile_id, &envelope)
                .await
                .map_err(map_port_error)?
        }
    };

    let outbox_event_id = port.new_outbox_event_id().map_err(map_port_error)?;
    port.project(
        actor.tenant_scope(),
        profile_id,
        &result,
        &outbox_event_id,
        clock.now(),
    )
    .await
    .map_err(map_port_error)?;
    Ok(result)
}

fn require_coordinatable(
    profile: &CoordinatorProfileAccess,
) -> Result<(), CoordinatorIngressOperationError> {
    if profile.has_active_generation()
        && matches!(profile.status(), "READY" | "IN_USE" | "DIRTY_LOCAL" | "SYNCING")
    {
        Ok(())
    } else {
        Err(CoordinatorIngressOperationError::Conflict)
    }
}

fn build_envelope<P: CoordinatorIngressApplicationPort, C: CoordinatorIngressClockPort>(
    actor: &ActorContext,
    port: &P,
    clock: &C,
    input: ExecuteCoordinatorCommand,
) -> Result<CoordinatorCommandEnvelope, CoordinatorIngressOperationError> {
    let now = clock.now();
    let command = match input.command {
        CoordinatorCommandInput::IssueLaunchIntent {
            launch_intent_id,
            device_id,
            expires_in_ms,
        } => {
            if !(MIN_INTENT_TTL_MS..=MAX_INTENT_TTL_MS).contains(&expires_in_ms) {
                return Err(CoordinatorIngressOperationError::InvalidRequest);
            }
            let expires_at = now
                .value()
                .checked_add(expires_in_ms)
                .map(UnixMillis::new)
                .ok_or(CoordinatorIngressOperationError::InvalidRequest)?;
            CoordinatorCommand::IssueLaunchIntent {
                launch_intent_id,
                actor_id: actor.actor_id().clone(),
                device_id,
                now,
                expires_at,
            }
        }
        CoordinatorCommandInput::Claim {
            launch_intent_id,
            device_id,
            session_id,
        } => CoordinatorCommand::Claim {
            launch_intent_id,
            actor_id: actor.actor_id().clone(),
            device_id,
            session_id,
            fencing_token: port.new_fencing_token().map_err(map_port_error)?,
            now,
        },
        CoordinatorCommandInput::Heartbeat {
            session_id,
            epoch,
            fencing_token,
        } => CoordinatorCommand::Heartbeat {
            session_id,
            epoch,
            fencing_token,
            now,
        },
        CoordinatorCommandInput::Release {
            session_id,
            epoch,
            fencing_token,
            disposition,
        } => CoordinatorCommand::Release {
            session_id,
            epoch,
            fencing_token,
            disposition,
            now,
        },
        CoordinatorCommandInput::BeginDrain => CoordinatorCommand::BeginDrain { now },
        CoordinatorCommandInput::MarkRecovered => CoordinatorCommand::MarkRecovered { now },
    };

    CoordinatorCommandEnvelope::new(
        input.idempotency_key,
        input.sequence,
        input.expected_version,
        command,
    )
    .map_err(|_| CoordinatorIngressOperationError::InvalidRequest)
}

fn map_port_error(error: CoordinatorIngressPortError) -> CoordinatorIngressOperationError {
    match error.class() {
        CoordinatorIngressPortErrorClass::NotFound => CoordinatorIngressOperationError::NotFound,
        CoordinatorIngressPortErrorClass::InvalidRequest => {
            CoordinatorIngressOperationError::InvalidRequest
        }
        CoordinatorIngressPortErrorClass::Conflict => CoordinatorIngressOperationError::Conflict,
        CoordinatorIngressPortErrorClass::IntegrityFailure => {
            CoordinatorIngressOperationError::IntegrityFailure
        }
        CoordinatorIngressPortErrorClass::InternalFailure => {
            CoordinatorIngressOperationError::InternalFailure
        }
        CoordinatorIngressPortErrorClass::DependencyUnavailable => {
            CoordinatorIngressOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
#[path = "coordinator_ingress_tests.rs"]
mod coordinator_ingress_tests;
