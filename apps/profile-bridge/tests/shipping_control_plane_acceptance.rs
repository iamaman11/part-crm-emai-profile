#![forbid(unsafe_code)]

use application_ports::{ProfileCoordinatorPort, ProfileCoordinatorRuntimePort};
use control_plane_contract::coordinator_api::{
    CoordinatorCommandDto, CoordinatorCommandRequestDto, CoordinatorOutcomeDto,
    CoordinatorProjectionDto, CoordinatorResponseDto, CoordinatorStatusDto,
};
use control_plane_contract::profile_launch_api::{
    BRIDGE_PROFILE_COORDINATOR_PATH_TEMPLATE, BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH,
    BridgeProfileLaunchRedemptionProjection, BridgeProfileLaunchRedemptionRequest,
};
use profile_bridge::operator_flow::EnrollmentPort;
use profile_bridge::shipping_control_plane::{
    ControlPlaneCoordinator, ControlPlaneEnrollment, MachineHttpMethod, MachineHttpPort,
    MachineHttpResponse, ShippingControlPlaneError,
};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, LaunchIntentId, ProfileId, TenantId, TenantScope,
    UnixMillis,
};
use std::sync::{Arc, Mutex};

const TENANT: &str = "tenant_01JP2SHIPPING";
const ACTOR: &str = "actor_01JP2SHIPPING";
const PROFILE: &str = "profile_01JP2SHIPPING";
const GENERATION: &str = "generation_01JP2SHIPPING";
const DEVICE: &str = "device_01JP2SHIPPING";
const LAUNCH_INTENT: &str = "launch_01JP2SHIPPING";
const CLAIM: &str = "claim_01JP2SHIPPING_0123456789";
const FENCE: &str = "fence_01JP2SHIPPING";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailureMode {
    None,
    WrongHeartbeatFence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ObservedRequest {
    method: MachineHttpMethod,
    path: String,
    body: Option<Vec<u8>>,
}

#[derive(Clone)]
struct CanonicalMachineHttp {
    observed: Arc<Mutex<Vec<ObservedRequest>>>,
    failure_mode: FailureMode,
}

impl CanonicalMachineHttp {
    fn new(failure_mode: FailureMode) -> Self {
        Self {
            observed: Arc::new(Mutex::new(Vec::new())),
            failure_mode,
        }
    }

    fn observations(&self) -> Arc<Mutex<Vec<ObservedRequest>>> {
        Arc::clone(&self.observed)
    }
}

impl MachineHttpPort for CanonicalMachineHttp {
    type Error = ();

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        assert!(!path.contains(CLAIM));
        assert!(!correlation_id.as_str().contains(CLAIM));
        self.observed.lock().map_err(|_| ())?.push(ObservedRequest {
            method,
            path: path.to_owned(),
            body: body.map(ToOwned::to_owned),
        });

        if path == BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH {
            assert_eq!(method, MachineHttpMethod::PostJson);
            let request = serde_json::from_slice::<BridgeProfileLaunchRedemptionRequest>(
                body.ok_or(())?,
            )
            .map_err(|_| ())?;
            assert_eq!(request.claim_code(), CLAIM);
            let response = BridgeProfileLaunchRedemptionProjection {
                tenant_id: TENANT.to_owned(),
                actor_id: ACTOR.to_owned(),
                profile_id: PROFILE.to_owned(),
                generation_id: GENERATION.to_owned(),
                device_id: DEVICE.to_owned(),
                launch_intent_id: LAUNCH_INTENT.to_owned(),
            };
            return Ok(json_response(200, &response)?);
        }

        let expected_path = BRIDGE_PROFILE_COORDINATOR_PATH_TEMPLATE
            .replace("{tenantId}", TENANT)
            .replace("{profileId}", PROFILE);
        assert_eq!(path, expected_path);

        match method {
            MachineHttpMethod::Get => {
                assert!(body.is_none());
                Ok(json_response(200, &snapshot_response())?)
            }
            MachineHttpMethod::PostJson => {
                let request = serde_json::from_slice::<CoordinatorCommandRequestDto>(
                    body.ok_or(())?,
                )
                .map_err(|_| ())?;
                Ok(json_response(200, &command_response(request, self.failure_mode)?)?)
            }
        }
    }
}

fn json_response<T: serde::Serialize>(status: u16, value: &T) -> Result<MachineHttpResponse, ()> {
    serde_json::to_vec(value)
        .map(|body| MachineHttpResponse::new(status, body))
        .map_err(|_| ())
}

fn projection(
    status: CoordinatorStatusDto,
    version: u64,
    sequence: u64,
    active_session_id: Option<String>,
    active_device_id: Option<String>,
    active_epoch: Option<u64>,
    idle_expires_at_ms: Option<u64>,
    hard_expires_at_ms: Option<u64>,
) -> CoordinatorProjectionDto {
    CoordinatorProjectionDto {
        tenant_id: TENANT.to_owned(),
        profile_id: PROFILE.to_owned(),
        status,
        version,
        sequence,
        next_epoch: 2,
        active_session_id,
        active_device_id,
        active_epoch,
        idle_expires_at_ms,
        hard_expires_at_ms,
        drain_deadline_ms: None,
        pending_launch_intent_id: None,
        pending_intent_expires_at_ms: None,
    }
}

fn snapshot_response() -> CoordinatorResponseDto {
    CoordinatorResponseDto {
        outcome: CoordinatorOutcomeDto::Snapshot,
        version: 1,
        sequence: 0,
        replayed: false,
        fencing_token: None,
        epoch: None,
        projection: projection(
            CoordinatorStatusDto::Idle,
            1,
            0,
            None,
            None,
            None,
            None,
            None,
        ),
    }
}

fn command_response(
    request: CoordinatorCommandRequestDto,
    failure_mode: FailureMode,
) -> Result<CoordinatorResponseDto, ()> {
    match request.command {
        CoordinatorCommandDto::Claim {
            launch_intent_id,
            device_id,
            session_id,
        } => {
            assert_eq!(request.expected_version, 1);
            assert_eq!(request.sequence, 1);
            assert_eq!(launch_intent_id, LAUNCH_INTENT);
            assert_eq!(device_id, DEVICE);
            Ok(CoordinatorResponseDto {
                outcome: CoordinatorOutcomeDto::LeaseClaimed,
                version: 2,
                sequence: 1,
                replayed: false,
                fencing_token: Some(FENCE.to_owned()),
                epoch: Some(1),
                projection: projection(
                    CoordinatorStatusDto::Active,
                    2,
                    1,
                    Some(session_id),
                    Some(DEVICE.to_owned()),
                    Some(1),
                    Some(30_000),
                    Some(900_000),
                ),
            })
        }
        CoordinatorCommandDto::Heartbeat {
            session_id,
            epoch,
            fencing_token,
        } => {
            assert_eq!(request.expected_version, 2);
            assert_eq!(request.sequence, 2);
            assert_eq!(epoch, 1);
            assert_eq!(fencing_token, FENCE);
            Ok(CoordinatorResponseDto {
                outcome: CoordinatorOutcomeDto::HeartbeatAccepted,
                version: 3,
                sequence: 2,
                replayed: false,
                fencing_token: Some(match failure_mode {
                    FailureMode::None => FENCE.to_owned(),
                    FailureMode::WrongHeartbeatFence => "fence_wrong".to_owned(),
                }),
                epoch: Some(1),
                projection: projection(
                    CoordinatorStatusDto::Active,
                    3,
                    2,
                    Some(session_id),
                    Some(DEVICE.to_owned()),
                    Some(1),
                    Some(60_000),
                    Some(900_000),
                ),
            })
        }
        CoordinatorCommandDto::Release {
            session_id,
            epoch,
            fencing_token,
            ..
        } => {
            assert_eq!(request.expected_version, 3);
            assert_eq!(request.sequence, 3);
            assert!(!session_id.is_empty());
            assert_eq!(epoch, 1);
            assert_eq!(fencing_token, FENCE);
            Ok(CoordinatorResponseDto {
                outcome: CoordinatorOutcomeDto::Released,
                version: 4,
                sequence: 3,
                replayed: false,
                fencing_token: None,
                epoch: None,
                projection: projection(
                    CoordinatorStatusDto::Uncertain,
                    4,
                    3,
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            })
        }
        _ => Err(()),
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse(TENANT)?),
        ActorId::parse(ACTOR)?,
        CorrelationId::parse("corr_01JP2SHIPPING")?,
    ))
}

#[test]
fn canonical_shipping_machine_client_redeems_claim_claims_heartbeats_and_releases_exact_lease()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = CanonicalMachineHttp::new(FailureMode::None);
    let observations = transport.observations();
    let mut enrollment = ControlPlaneEnrollment::new(transport.clone());
    let mut coordinator = ControlPlaneCoordinator::new(transport);
    let device = DeviceId::parse(DEVICE)?;
    let profile = ProfileId::parse(PROFILE)?;
    let launch_intent = LaunchIntentId::parse(LAUNCH_INTENT)?;
    let claim = bridge_domain::ClaimUri::parse(&format!("profilebridge://claim/{CLAIM}"))?;

    let redeemed = enrollment.redeem_claim(&claim, &device, UnixMillis::new(1))?;
    assert_eq!(redeemed.actor().tenant_scope().tenant_id().as_str(), TENANT);
    assert_eq!(redeemed.profile_id().as_str(), PROFILE);
    assert_eq!(redeemed.generation_id().as_str(), GENERATION);
    assert_eq!(redeemed.launch_intent_id(), &launch_intent);

    let lease = coordinator.claim_launch_intent(&actor()?, &profile, &device, &launch_intent)?;
    assert_eq!(lease.device_id(), &device);
    assert_eq!(lease.epoch(), 1);
    assert_eq!(lease.fencing_token().as_str(), FENCE);
    assert_eq!(coordinator.runtime_timing()?.idle_expires_at_ms(), 30_000);
    coordinator.heartbeat_lease(&lease)?;
    assert_eq!(coordinator.runtime_timing()?.idle_expires_at_ms(), 60_000);
    coordinator.close_lease(&lease)?;

    let requests = observations.lock().map_err(|_| "observation lock poisoned")?;
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].path, BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH);
    assert_eq!(requests[0].method, MachineHttpMethod::PostJson);
    assert!(requests[0]
        .body
        .as_deref()
        .is_some_and(|body| String::from_utf8_lossy(body).contains(CLAIM)));
    assert!(requests.iter().skip(1).all(|request| {
        !request.path.contains(CLAIM)
            && request
                .body
                .as_deref()
                .is_none_or(|body| !String::from_utf8_lossy(body).contains(CLAIM))
    }));
    Ok(())
}

#[test]
fn changed_heartbeat_fence_is_rejected_without_accepting_new_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = CanonicalMachineHttp::new(FailureMode::WrongHeartbeatFence);
    let mut coordinator = ControlPlaneCoordinator::new(transport);
    let device = DeviceId::parse(DEVICE)?;
    let profile = ProfileId::parse(PROFILE)?;
    let launch_intent = LaunchIntentId::parse(LAUNCH_INTENT)?;
    let lease = coordinator.claim_launch_intent(&actor()?, &profile, &device, &launch_intent)?;

    assert_eq!(
        coordinator.heartbeat_lease(&lease),
        Err(ShippingControlPlaneError::InvalidResponse)
    );
    assert_eq!(coordinator.runtime_timing()?.idle_expires_at_ms(), 30_000);
    Ok(())
}
