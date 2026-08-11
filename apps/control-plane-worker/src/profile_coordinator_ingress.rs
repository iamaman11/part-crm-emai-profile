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
use control_plane_contract::coordinator_api::{
    CoordinatorCommandDto, CoordinatorCommandRequestDto, CoordinatorOutcomeDto,
    CoordinatorProjectionDto, CoordinatorReleaseDispositionDto, CoordinatorResponseDto,
    CoordinatorStatusDto,
};
use control_plane_contract::{D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    AggregateVersion, DeviceId, FencingToken, IdempotencyKey, LaunchIntentId, ProfileId, SessionId,
    TenantId,
};
use session_domain::coordinator::{CoordinatorStatus, ReleaseDisposition};
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
            let body = match request.json::<CoordinatorCommandRequestDto>().await {
                Ok(value) => value,
                Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
            let command = match into_application(body) {
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
    Response::from_json(&coordinator_response(&result))
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

fn into_application(value: CoordinatorCommandRequestDto) -> Result<ExecuteCoordinatorCommand, ()> {
    Ok(ExecuteCoordinatorCommand::new(
        IdempotencyKey::parse(value.idempotency_key).map_err(|_| ())?,
        value.sequence,
        AggregateVersion::new(value.expected_version).map_err(|_| ())?,
        command_into_application(value.command)?,
    ))
}

fn command_into_application(value: CoordinatorCommandDto) -> Result<CoordinatorCommandInput, ()> {
    Ok(match value {
        CoordinatorCommandDto::IssueLaunchIntent {
            launch_intent_id,
            device_id,
            expires_in_ms,
        } => CoordinatorCommandInput::IssueLaunchIntent {
            launch_intent_id: LaunchIntentId::parse(launch_intent_id).map_err(|_| ())?,
            device_id: DeviceId::parse(device_id).map_err(|_| ())?,
            expires_in_ms,
        },
        CoordinatorCommandDto::Claim {
            launch_intent_id,
            device_id,
            session_id,
        } => CoordinatorCommandInput::Claim {
            launch_intent_id: LaunchIntentId::parse(launch_intent_id).map_err(|_| ())?,
            device_id: DeviceId::parse(device_id).map_err(|_| ())?,
            session_id: SessionId::parse(session_id).map_err(|_| ())?,
        },
        CoordinatorCommandDto::Heartbeat {
            session_id,
            epoch,
            fencing_token,
        } => CoordinatorCommandInput::Heartbeat {
            session_id: SessionId::parse(session_id).map_err(|_| ())?,
            epoch,
            fencing_token: FencingToken::parse(fencing_token).map_err(|_| ())?,
        },
        CoordinatorCommandDto::Release {
            session_id,
            epoch,
            fencing_token,
            disposition,
        } => CoordinatorCommandInput::Release {
            session_id: SessionId::parse(session_id).map_err(|_| ())?,
            epoch,
            fencing_token: FencingToken::parse(fencing_token).map_err(|_| ())?,
            disposition: release_disposition(disposition),
        },
        CoordinatorCommandDto::BeginDrain => CoordinatorCommandInput::BeginDrain,
        CoordinatorCommandDto::MarkRecovered => CoordinatorCommandInput::MarkRecovered,
    })
}

const fn release_disposition(value: CoordinatorReleaseDispositionDto) -> ReleaseDisposition {
    match value {
        CoordinatorReleaseDispositionDto::Clean => ReleaseDisposition::Clean,
        CoordinatorReleaseDispositionDto::Dirty => ReleaseDisposition::Dirty,
        CoordinatorReleaseDispositionDto::Uncertain => ReleaseDisposition::Uncertain,
    }
}

fn coordinator_response(value: &CoordinatorRuntimeResult) -> CoordinatorResponseDto {
    CoordinatorResponseDto {
        outcome: coordinator_outcome(value.outcome()),
        version: value.version().value(),
        sequence: value.sequence(),
        replayed: value.replayed(),
        fencing_token: value.fencing_token().map(|token| token.as_str().to_owned()),
        epoch: value.epoch(),
        projection: coordinator_projection(value.projection()),
    }
}

fn coordinator_projection(value: &CoordinatorProjectionSnapshot) -> CoordinatorProjectionDto {
    CoordinatorProjectionDto {
        tenant_id: value.tenant_id().as_str().to_owned(),
        profile_id: value.profile_id().as_str().to_owned(),
        status: coordinator_status(value.status()),
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

const fn coordinator_status(value: CoordinatorStatus) -> CoordinatorStatusDto {
    match value {
        CoordinatorStatus::Idle => CoordinatorStatusDto::Idle,
        CoordinatorStatus::Active => CoordinatorStatusDto::Active,
        CoordinatorStatus::Draining => CoordinatorStatusDto::Draining,
        CoordinatorStatus::Dirty => CoordinatorStatusDto::Dirty,
        CoordinatorStatus::Uncertain => CoordinatorStatusDto::Uncertain,
    }
}

const fn coordinator_outcome(value: CoordinatorRuntimeOutcome) -> CoordinatorOutcomeDto {
    match value {
        CoordinatorRuntimeOutcome::Snapshot => CoordinatorOutcomeDto::Snapshot,
        CoordinatorRuntimeOutcome::LaunchIntentIssued => CoordinatorOutcomeDto::LaunchIntentIssued,
        CoordinatorRuntimeOutcome::LeaseClaimed => CoordinatorOutcomeDto::LeaseClaimed,
        CoordinatorRuntimeOutcome::HeartbeatAccepted => CoordinatorOutcomeDto::HeartbeatAccepted,
        CoordinatorRuntimeOutcome::Released => CoordinatorOutcomeDto::Released,
        CoordinatorRuntimeOutcome::DrainStarted => CoordinatorOutcomeDto::DrainStarted,
        CoordinatorRuntimeOutcome::TimedOut => CoordinatorOutcomeDto::TimedOut,
        CoordinatorRuntimeOutcome::LaunchIntentExpired => CoordinatorOutcomeDto::LaunchIntentExpired,
        CoordinatorRuntimeOutcome::Recovered => CoordinatorOutcomeDto::Recovered,
        CoordinatorRuntimeOutcome::NoChange => CoordinatorOutcomeDto::NoChange,
    }
}

#[cfg(test)]
mod tests {
    use super::{coordinator_outcome, coordinator_status, into_application, release_disposition};
    use application_ports::coordinator_ingress::CoordinatorRuntimeOutcome;
    use control_plane_contract::coordinator_api::{
        CoordinatorCommandRequestDto, CoordinatorOutcomeDto, CoordinatorReleaseDispositionDto,
        CoordinatorStatusDto,
    };
    use session_domain::coordinator::{CoordinatorStatus, ReleaseDisposition};

    #[test]
    fn canonical_request_preserves_unknown_field_tolerance_before_application_validation() {
        let body = r#"{
            "idempotency_key":"invalid-until-application-layer",
            "sequence":0,
            "expected_version":0,
            "extra":"accepted",
            "command":{"type":"begin_drain","extra_command":"accepted"}
        }"#;
        let decoded = serde_json::from_str::<CoordinatorCommandRequestDto>(body)
            .expect("wire DTO remains tolerant and defers application validation");
        assert!(into_application(decoded).is_err());
    }

    #[test]
    fn domain_and_application_enums_map_exhaustively_to_public_wire_enums() {
        assert_eq!(coordinator_status(CoordinatorStatus::Idle), CoordinatorStatusDto::Idle);
        assert_eq!(
            coordinator_status(CoordinatorStatus::Uncertain),
            CoordinatorStatusDto::Uncertain
        );
        assert_eq!(
            coordinator_outcome(CoordinatorRuntimeOutcome::LaunchIntentExpired),
            CoordinatorOutcomeDto::LaunchIntentExpired
        );
        assert_eq!(
            release_disposition(CoordinatorReleaseDispositionDto::Dirty),
            ReleaseDisposition::Dirty
        );
    }
}
