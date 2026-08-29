use crate::operator_flow::{EnrollmentPort, OperatorEnrollment};
use application_ports::{ProfileCoordinatorPort, ProfileCoordinatorRuntimePort};
use bridge_domain::ClaimUri;
use control_plane_contract::coordinator_api::{
    CoordinatorCommandDto, CoordinatorCommandRequestDto, CoordinatorOutcomeDto,
    CoordinatorReleaseDispositionDto, CoordinatorResponseDto, CoordinatorStatusDto,
};
use control_plane_contract::profile_launch_api::{
    BRIDGE_PROFILE_COORDINATOR_PATH_TEMPLATE, BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH,
    BridgeProfileLaunchRedemptionProjection, BridgeProfileLaunchRedemptionRequest,
};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, GenerationId, IdempotencyKey,
    LaunchIntentId, ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
};
use session_domain::ProfileLease;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineHttpMethod {
    Get,
    PostJson,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineHttpResponse {
    status: u16,
    body: Vec<u8>,
}

impl MachineHttpResponse {
    #[must_use]
    pub const fn new(status: u16, body: Vec<u8>) -> Self {
        Self { status, body }
    }

    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

/// Narrow machine-authenticated HTTP effect used by the shipping Bridge adapters.
///
/// Route and payload semantics stay owned by `control-plane-contract`; concrete Windows TLS and
/// certificate-store behavior belongs to the outer native adapter.
pub trait MachineHttpPort {
    type Error;

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShippingControlPlaneError {
    Transport,
    HttpStatus,
    InvalidResponse,
    Clock,
}

impl core::fmt::Display for ShippingControlPlaneError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Transport => "machine-authenticated control-plane transport failed",
            Self::HttpStatus => "machine-authenticated control-plane request was rejected",
            Self::InvalidResponse => "machine-authenticated control-plane response was invalid",
            Self::Clock => "machine control-plane request identity could not be created",
        })
    }
}

impl std::error::Error for ShippingControlPlaneError {}

pub struct ControlPlaneEnrollment<T> {
    transport: T,
}

impl<T> ControlPlaneEnrollment<T> {
    #[must_use]
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<T> EnrollmentPort for ControlPlaneEnrollment<T>
where
    T: MachineHttpPort,
{
    type Error = ShippingControlPlaneError;

    fn redeem_claim(
        &mut self,
        claim: &ClaimUri,
        device_id: &DeviceId,
        _now: UnixMillis,
    ) -> Result<OperatorEnrollment, Self::Error> {
        let correlation_id = next_correlation_id()?;
        let request = BridgeProfileLaunchRedemptionRequest::new(
            claim
                .claim_code()
                .expose_for_transport()
                .as_str()
                .to_owned(),
        );
        let mut body = serde_json::to_vec(&request).map_err(|_| Self::Error::InvalidResponse)?;
        let response = self
            .transport
            .request(
                MachineHttpMethod::PostJson,
                BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH,
                &correlation_id,
                Some(&body),
            )
            .map_err(|_| Self::Error::Transport);
        body.fill(0);
        let response = accepted_response(response?)?;
        let projection =
            serde_json::from_slice::<BridgeProfileLaunchRedemptionProjection>(response.body())
                .map_err(|_| Self::Error::InvalidResponse)?;

        let tenant_id =
            TenantId::parse(projection.tenant_id).map_err(|_| Self::Error::InvalidResponse)?;
        let actor_id =
            ActorId::parse(projection.actor_id).map_err(|_| Self::Error::InvalidResponse)?;
        let profile_id =
            ProfileId::parse(projection.profile_id).map_err(|_| Self::Error::InvalidResponse)?;
        let generation_id = GenerationId::parse(projection.generation_id)
            .map_err(|_| Self::Error::InvalidResponse)?;
        let returned_device =
            DeviceId::parse(projection.device_id).map_err(|_| Self::Error::InvalidResponse)?;
        let launch_intent_id = LaunchIntentId::parse(projection.launch_intent_id)
            .map_err(|_| Self::Error::InvalidResponse)?;
        if &returned_device != device_id {
            return Err(Self::Error::InvalidResponse);
        }

        Ok(OperatorEnrollment::new(
            ActorContext::new(TenantScope::new(tenant_id), actor_id, correlation_id),
            profile_id,
            generation_id,
            launch_intent_id,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlPlaneLeaseTiming {
    idle_expires_at_ms: u64,
    hard_expires_at_ms: u64,
}

impl ControlPlaneLeaseTiming {
    #[must_use]
    pub const fn idle_expires_at_ms(self) -> u64 {
        self.idle_expires_at_ms
    }

    #[must_use]
    pub const fn hard_expires_at_ms(self) -> u64 {
        self.hard_expires_at_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CoordinatorCursor {
    tenant_id: TenantId,
    profile_id: ProfileId,
    device_id: DeviceId,
    session_id: SessionId,
    epoch: u64,
    fencing_token: FencingToken,
    version: u64,
    sequence: u64,
    timing: ControlPlaneLeaseTiming,
}

pub struct ControlPlaneCoordinator<T> {
    transport: T,
    cursor: Option<CoordinatorCursor>,
}

impl<T> ControlPlaneCoordinator<T> {
    #[must_use]
    pub const fn new(transport: T) -> Self {
        Self {
            transport,
            cursor: None,
        }
    }

    pub fn runtime_timing(&self) -> Result<ControlPlaneLeaseTiming, ShippingControlPlaneError> {
        self.cursor
            .as_ref()
            .map(|cursor| cursor.timing)
            .ok_or(ShippingControlPlaneError::InvalidResponse)
    }
}

impl<T> ControlPlaneCoordinator<T>
where
    T: MachineHttpPort,
{
    fn snapshot(
        &mut self,
        actor: &ActorContext,
        profile_id: &ProfileId,
    ) -> Result<CoordinatorResponseDto, ShippingControlPlaneError> {
        let response = self
            .transport
            .request(
                MachineHttpMethod::Get,
                &coordinator_path(actor.tenant_scope().tenant_id(), profile_id),
                actor.correlation_id(),
                None,
            )
            .map_err(|_| ShippingControlPlaneError::Transport)?;
        decode_coordinator_response(response)
    }

    fn command(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        request: &CoordinatorCommandRequestDto,
    ) -> Result<CoordinatorResponseDto, ShippingControlPlaneError> {
        let correlation_id = next_correlation_id()?;
        let mut body =
            serde_json::to_vec(request).map_err(|_| ShippingControlPlaneError::InvalidResponse)?;
        let response = self
            .transport
            .request(
                MachineHttpMethod::PostJson,
                &coordinator_path(tenant_id, profile_id),
                &correlation_id,
                Some(&body),
            )
            .map_err(|_| ShippingControlPlaneError::Transport);
        body.fill(0);
        decode_coordinator_response(response?)
    }
}

impl<T> ProfileCoordinatorPort for ControlPlaneCoordinator<T>
where
    T: MachineHttpPort,
{
    type Error = ShippingControlPlaneError;

    fn claim_launch_intent(
        &mut self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        device_id: &DeviceId,
        launch_intent_id: &LaunchIntentId,
    ) -> Result<ProfileLease, Self::Error> {
        if self.cursor.is_some() {
            return Err(Self::Error::InvalidResponse);
        }
        let snapshot = self.snapshot(actor, profile_id)?;
        validate_snapshot(&snapshot, actor.tenant_scope().tenant_id(), profile_id)?;
        let session_id = next_session_id()?;
        let sequence = snapshot
            .sequence
            .checked_add(1)
            .ok_or(Self::Error::InvalidResponse)?;
        let request = CoordinatorCommandRequestDto {
            idempotency_key: next_idempotency_key()?.as_str().to_owned(),
            sequence,
            expected_version: snapshot.version,
            command: CoordinatorCommandDto::Claim {
                launch_intent_id: launch_intent_id.as_str().to_owned(),
                device_id: device_id.as_str().to_owned(),
                session_id: session_id.as_str().to_owned(),
            },
        };
        let response = self.command(actor.tenant_scope().tenant_id(), profile_id, &request)?;
        let (epoch, timing) = validate_active_projection(
            &response,
            CoordinatorOutcomeDto::LeaseClaimed,
            actor.tenant_scope().tenant_id(),
            profile_id,
            device_id,
            &session_id,
            sequence,
        )?;
        if response.epoch != Some(epoch) {
            return Err(Self::Error::InvalidResponse);
        }
        let fencing_token = response
            .fencing_token
            .as_ref()
            .ok_or(Self::Error::InvalidResponse)
            .and_then(|value| {
                FencingToken::parse(value.clone()).map_err(|_| Self::Error::InvalidResponse)
            })?;
        let lease = ProfileLease::issue(
            actor.tenant_scope().tenant_id().clone(),
            profile_id.clone(),
            session_id.clone(),
            device_id.clone(),
            epoch,
            fencing_token.clone(),
        )
        .map_err(|_| Self::Error::InvalidResponse)?;
        self.cursor = Some(CoordinatorCursor {
            tenant_id: actor.tenant_scope().tenant_id().clone(),
            profile_id: profile_id.clone(),
            device_id: device_id.clone(),
            session_id,
            epoch,
            fencing_token,
            version: response.version,
            sequence: response.sequence,
            timing,
        });
        Ok(lease)
    }

    fn close_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        let cursor = self
            .cursor
            .as_ref()
            .filter(|value| cursor_matches_lease(value, lease))
            .cloned()
            .ok_or(Self::Error::InvalidResponse)?;
        let sequence = cursor
            .sequence
            .checked_add(1)
            .ok_or(Self::Error::InvalidResponse)?;
        let request = CoordinatorCommandRequestDto {
            idempotency_key: next_idempotency_key()?.as_str().to_owned(),
            sequence,
            expected_version: cursor.version,
            command: CoordinatorCommandDto::Release {
                session_id: cursor.session_id.as_str().to_owned(),
                epoch: cursor.epoch,
                fencing_token: cursor.fencing_token.as_str().to_owned(),
                disposition: CoordinatorReleaseDispositionDto::Uncertain,
            },
        };
        let response = self.command(&cursor.tenant_id, &cursor.profile_id, &request)?;
        validate_released_response(&response, &cursor, sequence)?;
        self.cursor = None;
        Ok(())
    }
}

impl<T> ProfileCoordinatorRuntimePort for ControlPlaneCoordinator<T>
where
    T: MachineHttpPort,
{
    fn heartbeat_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        let cursor = self
            .cursor
            .as_ref()
            .filter(|value| cursor_matches_lease(value, lease))
            .cloned()
            .ok_or(Self::Error::InvalidResponse)?;
        let sequence = cursor
            .sequence
            .checked_add(1)
            .ok_or(Self::Error::InvalidResponse)?;
        let request = CoordinatorCommandRequestDto {
            idempotency_key: next_idempotency_key()?.as_str().to_owned(),
            sequence,
            expected_version: cursor.version,
            command: CoordinatorCommandDto::Heartbeat {
                session_id: cursor.session_id.as_str().to_owned(),
                epoch: cursor.epoch,
                fencing_token: cursor.fencing_token.as_str().to_owned(),
            },
        };
        let response = self.command(&cursor.tenant_id, &cursor.profile_id, &request)?;
        let (epoch, timing) = validate_active_projection(
            &response,
            CoordinatorOutcomeDto::HeartbeatAccepted,
            &cursor.tenant_id,
            &cursor.profile_id,
            &cursor.device_id,
            &cursor.session_id,
            sequence,
        )?;
        if epoch != cursor.epoch
            || response.epoch.is_some_and(|value| value != cursor.epoch)
            || response
                .fencing_token
                .as_deref()
                .is_some_and(|value| value != cursor.fencing_token.as_str())
        {
            return Err(Self::Error::InvalidResponse);
        }
        let current = self.cursor.as_mut().ok_or(Self::Error::InvalidResponse)?;
        current.version = response.version;
        current.sequence = response.sequence;
        current.timing = timing;
        Ok(())
    }
}

fn accepted_response(
    response: MachineHttpResponse,
) -> Result<MachineHttpResponse, ShippingControlPlaneError> {
    if (200..=299).contains(&response.status()) {
        Ok(response)
    } else {
        Err(ShippingControlPlaneError::HttpStatus)
    }
}

fn decode_coordinator_response(
    response: MachineHttpResponse,
) -> Result<CoordinatorResponseDto, ShippingControlPlaneError> {
    let response = accepted_response(response)?;
    serde_json::from_slice(response.body()).map_err(|_| ShippingControlPlaneError::InvalidResponse)
}

fn validate_snapshot(
    response: &CoordinatorResponseDto,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
) -> Result<(), ShippingControlPlaneError> {
    if response.outcome != CoordinatorOutcomeDto::Snapshot
        || response.replayed
        || response.version == 0
        || response.projection.tenant_id != tenant_id.as_str()
        || response.projection.profile_id != profile_id.as_str()
        || response.projection.version != response.version
        || response.projection.sequence != response.sequence
        || response.projection.status != CoordinatorStatusDto::Idle
        || response.projection.active_session_id.is_some()
        || response.projection.active_device_id.is_some()
        || response.projection.active_epoch.is_some()
        || response.fencing_token.is_some()
        || response.epoch.is_some()
    {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_active_projection(
    response: &CoordinatorResponseDto,
    expected_outcome: CoordinatorOutcomeDto,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    device_id: &DeviceId,
    session_id: &SessionId,
    expected_sequence: u64,
) -> Result<(u64, ControlPlaneLeaseTiming), ShippingControlPlaneError> {
    if response.outcome != expected_outcome
        || response.replayed
        || response.sequence != expected_sequence
        || response.version == 0
        || response.projection.tenant_id != tenant_id.as_str()
        || response.projection.profile_id != profile_id.as_str()
        || response.projection.version != response.version
        || response.projection.sequence != response.sequence
        || response.projection.status != CoordinatorStatusDto::Active
        || response.projection.active_session_id.as_deref() != Some(session_id.as_str())
        || response.projection.active_device_id.as_deref() != Some(device_id.as_str())
    {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let epoch = response
        .projection
        .active_epoch
        .ok_or(ShippingControlPlaneError::InvalidResponse)?;
    if epoch == 0 {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let timing = lease_timing(
        response.projection.idle_expires_at_ms,
        response.projection.hard_expires_at_ms,
    )?;
    Ok((epoch, timing))
}

fn lease_timing(
    idle_expires_at_ms: Option<u64>,
    hard_expires_at_ms: Option<u64>,
) -> Result<ControlPlaneLeaseTiming, ShippingControlPlaneError> {
    let idle_expires_at_ms =
        idle_expires_at_ms.ok_or(ShippingControlPlaneError::InvalidResponse)?;
    let hard_expires_at_ms =
        hard_expires_at_ms.ok_or(ShippingControlPlaneError::InvalidResponse)?;
    if idle_expires_at_ms == 0 || hard_expires_at_ms == 0 || idle_expires_at_ms > hard_expires_at_ms
    {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    Ok(ControlPlaneLeaseTiming {
        idle_expires_at_ms,
        hard_expires_at_ms,
    })
}

fn validate_released_response(
    response: &CoordinatorResponseDto,
    cursor: &CoordinatorCursor,
    expected_sequence: u64,
) -> Result<(), ShippingControlPlaneError> {
    if response.outcome != CoordinatorOutcomeDto::Released
        || response.replayed
        || response.sequence != expected_sequence
        || response.version == 0
        || response.projection.tenant_id != cursor.tenant_id.as_str()
        || response.projection.profile_id != cursor.profile_id.as_str()
        || response.projection.version != response.version
        || response.projection.sequence != response.sequence
        || response.projection.status != CoordinatorStatusDto::Uncertain
        || response.projection.active_session_id.is_some()
        || response.projection.active_device_id.is_some()
        || response.projection.active_epoch.is_some()
        || response.fencing_token.is_some()
        || response.epoch.is_some()
    {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    Ok(())
}

fn cursor_matches_lease(cursor: &CoordinatorCursor, lease: &ProfileLease) -> bool {
    cursor.tenant_id == *lease.tenant_id()
        && cursor.profile_id == *lease.profile_id()
        && cursor.device_id == *lease.device_id()
        && cursor.session_id == *lease.session_id()
        && cursor.epoch == lease.epoch()
        && cursor.fencing_token == *lease.fencing_token()
}

fn coordinator_path(tenant_id: &TenantId, profile_id: &ProfileId) -> String {
    BRIDGE_PROFILE_COORDINATOR_PATH_TEMPLATE
        .replace("{tenantId}", tenant_id.as_str())
        .replace("{profileId}", profile_id.as_str())
}

fn unique_value(prefix: &str) -> Result<String, ShippingControlPlaneError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ShippingControlPlaneError::Clock)?
        .as_millis();
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!(
        "{prefix}_{millis}_{}_{}",
        std::process::id(),
        sequence
    ))
}

fn next_correlation_id() -> Result<CorrelationId, ShippingControlPlaneError> {
    CorrelationId::parse(unique_value("corr")?).map_err(|_| ShippingControlPlaneError::Clock)
}

fn next_session_id() -> Result<SessionId, ShippingControlPlaneError> {
    SessionId::parse(unique_value("session")?).map_err(|_| ShippingControlPlaneError::Clock)
}

fn next_idempotency_key() -> Result<IdempotencyKey, ShippingControlPlaneError> {
    IdempotencyKey::parse(unique_value("idem")?).map_err(|_| ShippingControlPlaneError::Clock)
}

#[cfg(test)]
mod tests {
    use super::{
        ControlPlaneEnrollment, ControlPlaneLeaseTiming, MachineHttpMethod, MachineHttpPort,
        MachineHttpResponse, ShippingControlPlaneError, lease_timing,
    };
    use crate::operator_flow::EnrollmentPort;
    use bridge_domain::ClaimUri;
    use profile_platform_primitives::{CorrelationId, DeviceId, UnixMillis};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeMachineHttp {
        responses: VecDeque<MachineHttpResponse>,
    }

    impl MachineHttpPort for FakeMachineHttp {
        type Error = ();

        fn request(
            &mut self,
            method: MachineHttpMethod,
            _path: &str,
            _correlation_id: &CorrelationId,
            body: Option<&[u8]>,
        ) -> Result<MachineHttpResponse, Self::Error> {
            assert_eq!(method, MachineHttpMethod::PostJson);
            assert!(body.is_some_and(|value| {
                String::from_utf8_lossy(value).contains("claim_01JBRIDGE_FEASIBILITY")
            }));
            self.responses.pop_front().ok_or(())
        }
    }

    #[test]
    fn active_lease_timing_is_server_owned_and_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            lease_timing(Some(30_000), Some(900_000))?,
            ControlPlaneLeaseTiming {
                idle_expires_at_ms: 30_000,
                hard_expires_at_ms: 900_000,
            }
        );
        assert_eq!(
            lease_timing(None, Some(900_000)),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        assert_eq!(
            lease_timing(Some(30_000), None),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        assert_eq!(
            lease_timing(Some(0), Some(900_000)),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        assert_eq!(
            lease_timing(Some(900_001), Some(900_000)),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        Ok(())
    }

    #[test]
    fn enrollment_uses_canonical_projection_and_rechecks_local_device()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = MachineHttpResponse::new(
            200,
            br#"{"tenantId":"tenant_01JBRIDGE","actorId":"actor_01JBRIDGE","profileId":"profile_01JBRIDGE","generationId":"generation_01JBRIDGE","deviceId":"device_01JBRIDGE","launchIntentId":"launch_01JBRIDGE"}"#.to_vec(),
        );
        let mut enrollment = ControlPlaneEnrollment::new(FakeMachineHttp {
            responses: VecDeque::from([response]),
        });
        let result = enrollment.redeem_claim(
            &ClaimUri::parse("profilebridge://claim/claim_01JBRIDGE_FEASIBILITY")?,
            &DeviceId::parse("device_01JBRIDGE")?,
            UnixMillis::new(1),
        )?;
        assert_eq!(result.profile_id().as_str(), "profile_01JBRIDGE");
        assert_eq!(result.generation_id().as_str(), "generation_01JBRIDGE");
        assert_eq!(result.launch_intent_id().as_str(), "launch_01JBRIDGE");
        Ok(())
    }

    #[test]
    fn enrollment_rejects_wrong_machine_binding() -> Result<(), Box<dyn std::error::Error>> {
        let response = MachineHttpResponse::new(
            200,
            br#"{"tenantId":"tenant_01JBRIDGE","actorId":"actor_01JBRIDGE","profileId":"profile_01JBRIDGE","generationId":"generation_01JBRIDGE","deviceId":"device_02JBRIDGE","launchIntentId":"launch_01JBRIDGE"}"#.to_vec(),
        );
        let mut enrollment = ControlPlaneEnrollment::new(FakeMachineHttp {
            responses: VecDeque::from([response]),
        });
        let error = enrollment.redeem_claim(
            &ClaimUri::parse("profilebridge://claim/claim_01JBRIDGE_FEASIBILITY")?,
            &DeviceId::parse("device_01JBRIDGE")?,
            UnixMillis::new(1),
        );
        assert_eq!(error, Err(ShippingControlPlaneError::InvalidResponse));
        Ok(())
    }
}
