use crate::dirty_generation::{GenerationSealingMaterial, GenerationSealingMaterialPort};
use crate::generation_reopen::{GenerationDownloadCapability, GenerationReopenControlPort};
use crate::operator_flow::{EnrollmentPort, OperatorEnrollment};
use application_ports::{ProfileCoordinatorPort, ProfileCoordinatorRuntimePort};
use bridge_domain::ClaimUri;
use control_plane_contract::coordinator_api::{
    CoordinatorCommandDto, CoordinatorCommandRequestDto, CoordinatorOutcomeDto,
    CoordinatorReleaseDispositionDto, CoordinatorResponseDto, CoordinatorStatusDto,
};
use control_plane_contract::generation_key_api::{
    BRIDGE_GENERATION_SEALING_MATERIAL_PATH_TEMPLATE, BridgeGenerationSealingMaterialRequest,
    BridgeGenerationSealingMaterialResponse, GENERATION_SEALING_CHUNK_BYTES,
};
use control_plane_contract::generation_reopen_api::{
    BRIDGE_PROFILE_GENERATION_DOWNLOAD_CAPABILITY_PATH_TEMPLATE,
    BRIDGE_PROFILE_GENERATION_OPENING_MATERIAL_PATH_TEMPLATE,
    BridgeGenerationDownloadCapabilityRequest, BridgeGenerationDownloadCapabilityResponse,
    BridgeGenerationOpeningMaterialRequest, BridgeGenerationOpeningMaterialResponse,
    GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
};
use control_plane_contract::profile_launch_api::{
    BRIDGE_PROFILE_COORDINATOR_PATH_TEMPLATE, BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH,
    BridgeProfileLaunchRedemptionProjection, BridgeProfileLaunchRedemptionRequest,
};
use encrypted_generation_domain::{
    GenerationDek, GenerationRootKeyVersion, KeyId, MAX_GENERATION_CONTAINER_BYTES,
    MAX_GENERATION_METADATA_PRELUDE_BYTES, NoncePrefix, canonical_generation_object_key,
    inspect_generation_metadata_prelude,
};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, GenerationId, IdempotencyKey,
    LaunchIntentId, ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
};
use session_domain::ProfileLease;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const MAX_SEALING_MATERIAL_RESPONSE_BYTES: usize = 1_024;
const MAX_REOPEN_DOWNLOAD_CAPABILITY_RESPONSE_BYTES: usize = 8_192;
const MAX_REOPEN_OPENING_MATERIAL_RESPONSE_BYTES: usize = 1_024;
const MAX_SIGNED_GENERATION_DOWNLOAD_URL_BYTES: usize = 4_096;

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

    fn exact_reopen_cursor(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
    ) -> Result<CoordinatorCursor, ShippingControlPlaneError> {
        self.cursor
            .as_ref()
            .filter(|cursor| {
                cursor.tenant_id == *tenant_id
                    && cursor.profile_id == *profile_id
                    && cursor.epoch != 0
            })
            .cloned()
            .ok_or(ShippingControlPlaneError::InvalidResponse)
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

impl<T> GenerationSealingMaterialPort for ControlPlaneCoordinator<T>
where
    T: MachineHttpPort,
{
    type Error = ShippingControlPlaneError;

    fn material_for(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        generation_id: &GenerationId,
        plaintext_digest: [u8; 32],
    ) -> Result<GenerationSealingMaterial, Self::Error> {
        if base_generation_id == generation_id {
            return Err(Self::Error::InvalidResponse);
        }
        let cursor = self
            .cursor
            .as_ref()
            .filter(|cursor| cursor.tenant_id == *tenant_id && cursor.profile_id == *profile_id)
            .cloned()
            .ok_or(Self::Error::InvalidResponse)?;
        let request = BridgeGenerationSealingMaterialRequest::new(
            base_generation_id.as_str(),
            generation_id.as_str(),
            encode_lower_hex(&plaintext_digest),
            cursor.session_id.as_str(),
            cursor.fencing_token.as_str(),
            cursor.epoch,
        );
        let mut body = serde_json::to_vec(&request).map_err(|_| Self::Error::InvalidResponse)?;
        let correlation_id = next_correlation_id()?;
        let response = self
            .transport
            .request(
                MachineHttpMethod::PostJson,
                &generation_sealing_material_path(tenant_id, profile_id),
                &correlation_id,
                Some(&body),
            )
            .map_err(|_| Self::Error::Transport);
        body.fill(0);
        let response = decode_sealing_material_response(response?)?;
        let (key_id, dek_hex, nonce_prefix_hex, chunk_size) = response.into_parts();
        if chunk_size != GENERATION_SEALING_CHUNK_BYTES {
            return Err(Self::Error::InvalidResponse);
        }
        let key_id = KeyId::parse(key_id).map_err(|_| Self::Error::InvalidResponse)?;
        GenerationRootKeyVersion::from_key_id(&key_id).map_err(|_| Self::Error::InvalidResponse)?;
        let dek = decode_secret_dek(dek_hex.as_str()).ok_or(Self::Error::InvalidResponse)?;
        let nonce_prefix =
            decode_lower_hex::<16>(&nonce_prefix_hex).ok_or(Self::Error::InvalidResponse)?;
        Ok(GenerationSealingMaterial::new(
            GenerationDek::new(key_id, dek.0),
            NoncePrefix::new(nonce_prefix),
            chunk_size,
        ))
    }
}

impl<T> GenerationReopenControlPort for ControlPlaneCoordinator<T>
where
    T: MachineHttpPort,
{
    type Error = ShippingControlPlaneError;

    fn download_capability(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
    ) -> Result<GenerationDownloadCapability, Self::Error> {
        let cursor = self.exact_reopen_cursor(tenant_id, profile_id)?;
        let request = BridgeGenerationDownloadCapabilityRequest::new(
            cursor.session_id.as_str(),
            cursor.fencing_token.as_str(),
            cursor.epoch,
        );
        let mut body = serde_json::to_vec(&request).map_err(|_| Self::Error::InvalidResponse)?;
        let correlation_id = next_correlation_id()?;
        let response = self
            .transport
            .request(
                MachineHttpMethod::PostJson,
                &generation_download_capability_path(tenant_id, profile_id),
                &correlation_id,
                Some(&body),
            )
            .map_err(|_| Self::Error::Transport);
        body.fill(0);
        let mut response = decode_reopen_download_capability_response(response?)?;

        if response.method() != "GET"
            || response.container_bytes() == 0
            || response.container_bytes()
                > u64::try_from(MAX_GENERATION_CONTAINER_BYTES)
                    .map_err(|_| Self::Error::InvalidResponse)?
            || response.expires_seconds() == 0
            || response.expires_seconds() > GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS
        {
            return Err(Self::Error::InvalidResponse);
        }
        let generation_id = GenerationId::parse(response.generation_id().to_owned())
            .map_err(|_| Self::Error::InvalidResponse)?;
        let canonical_key = canonical_generation_object_key(tenant_id, profile_id, &generation_id);
        if response.object_key() != canonical_key || !valid_signed_r2_get_url(response.url()) {
            return Err(Self::Error::InvalidResponse);
        }
        let metadata_digest =
            decode_lower_hex::<32>(response.metadata_digest()).ok_or(Self::Error::InvalidResponse)?;
        let container_digest =
            decode_lower_hex::<32>(response.container_digest()).ok_or(Self::Error::InvalidResponse)?;
        let container_bytes = response.container_bytes();
        let expires_seconds = response.expires_seconds();
        let signed_url = response.take_url();

        Ok(GenerationDownloadCapability::new(
            generation_id,
            canonical_key,
            metadata_digest,
            container_digest,
            container_bytes,
            signed_url,
            expires_seconds,
        ))
    }

    fn opening_material(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        metadata_prelude: &[u8],
    ) -> Result<GenerationDek, Self::Error> {
        let cursor = self.exact_reopen_cursor(tenant_id, profile_id)?;
        if metadata_prelude.is_empty()
            || metadata_prelude.len() > MAX_GENERATION_METADATA_PRELUDE_BYTES
        {
            return Err(Self::Error::InvalidResponse);
        }
        let inspected = inspect_generation_metadata_prelude(metadata_prelude)
            .map_err(|_| Self::Error::InvalidResponse)?;
        if inspected.prelude_bytes() != metadata_prelude.len()
            || inspected.metadata().tenant_id() != tenant_id
            || inspected.metadata().profile_id() != profile_id
            || inspected.metadata().object_key()
                != canonical_generation_object_key(
                    tenant_id,
                    profile_id,
                    inspected.metadata().generation_id(),
                )
        {
            return Err(Self::Error::InvalidResponse);
        }

        let request = BridgeGenerationOpeningMaterialRequest::new(
            encode_lower_hex(metadata_prelude),
            cursor.session_id.as_str(),
            cursor.fencing_token.as_str(),
            cursor.epoch,
        );
        let mut body = serde_json::to_vec(&request).map_err(|_| Self::Error::InvalidResponse)?;
        let correlation_id = next_correlation_id()?;
        let response = self
            .transport
            .request(
                MachineHttpMethod::PostJson,
                &generation_opening_material_path(tenant_id, profile_id),
                &correlation_id,
                Some(&body),
            )
            .map_err(|_| Self::Error::Transport);
        body.fill(0);
        let response = decode_reopen_opening_material_response(response?)?;
        let (key_id, dek_hex) = response.into_parts();
        let key_id = KeyId::parse(key_id).map_err(|_| Self::Error::InvalidResponse)?;
        GenerationRootKeyVersion::from_key_id(&key_id).map_err(|_| Self::Error::InvalidResponse)?;
        if &key_id != inspected.metadata().key_id() {
            return Err(Self::Error::InvalidResponse);
        }
        let dek = decode_secret_dek(dek_hex.as_str()).ok_or(Self::Error::InvalidResponse)?;
        Ok(GenerationDek::new(key_id, dek.0))
    }
}

struct SecretDek([u8; 32]);

impl Drop for SecretDek {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

fn decode_sealing_material_response(
    mut response: MachineHttpResponse,
) -> Result<BridgeGenerationSealingMaterialResponse, ShippingControlPlaneError> {
    if !(200..=299).contains(&response.status()) {
        response.body.fill(0);
        return Err(ShippingControlPlaneError::HttpStatus);
    }
    if response.body.len() > MAX_SEALING_MATERIAL_RESPONSE_BYTES {
        response.body.fill(0);
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let decoded = serde_json::from_slice::<BridgeGenerationSealingMaterialResponse>(&response.body);
    response.body.fill(0);
    decoded.map_err(|_| ShippingControlPlaneError::InvalidResponse)
}

fn decode_reopen_download_capability_response(
    mut response: MachineHttpResponse,
) -> Result<BridgeGenerationDownloadCapabilityResponse, ShippingControlPlaneError> {
    if !(200..=299).contains(&response.status()) {
        response.body.fill(0);
        return Err(ShippingControlPlaneError::HttpStatus);
    }
    if response.body.is_empty() || response.body.len() > MAX_REOPEN_DOWNLOAD_CAPABILITY_RESPONSE_BYTES
    {
        response.body.fill(0);
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let decoded =
        serde_json::from_slice::<BridgeGenerationDownloadCapabilityResponse>(&response.body);
    response.body.fill(0);
    decoded.map_err(|_| ShippingControlPlaneError::InvalidResponse)
}

fn decode_reopen_opening_material_response(
    mut response: MachineHttpResponse,
) -> Result<BridgeGenerationOpeningMaterialResponse, ShippingControlPlaneError> {
    if !(200..=299).contains(&response.status()) {
        response.body.fill(0);
        return Err(ShippingControlPlaneError::HttpStatus);
    }
    if response.body.is_empty() || response.body.len() > MAX_REOPEN_OPENING_MATERIAL_RESPONSE_BYTES {
        response.body.fill(0);
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let decoded = serde_json::from_slice::<BridgeGenerationOpeningMaterialResponse>(&response.body);
    response.body.fill(0);
    decoded.map_err(|_| ShippingControlPlaneError::InvalidResponse)
}

fn decode_secret_dek(value: &str) -> Option<SecretDek> {
    decode_lower_hex::<32>(value).map(SecretDek)
}

fn decode_lower_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N.saturating_mul(2) {
        return None;
    }
    let bytes = value.as_bytes();
    let mut decoded = [0_u8; N];
    for (index, output) in decoded.iter_mut().enumerate() {
        let high = lower_hex_nibble(bytes[index * 2])?;
        let low = lower_hex_nibble(bytes[index * 2 + 1])?;
        *output = (high << 4) | low;
    }
    Some(decoded)
}

const fn lower_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn valid_signed_r2_get_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_SIGNED_GENERATION_DOWNLOAD_URL_BYTES
        || value.bytes().any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || value.contains('#')
    {
        return false;
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, path_and_query)) = rest.split_once('/') else {
        return false;
    };
    const R2_SUFFIX: &str = ".r2.cloudflarestorage.com";
    let Some(account_id) = authority.strip_suffix(R2_SUFFIX) else {
        return false;
    };
    if account_id.len() != 32
        || !account_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || authority.contains('@')
        || authority.contains(':')
    {
        return false;
    }
    let Some((path, query)) = path_and_query.split_once('?') else {
        return false;
    };
    if path.is_empty() || query.is_empty() {
        return false;
    }
    let required = [
        "X-Amz-Algorithm=AWS4-HMAC-SHA256",
        "X-Amz-Credential=",
        "X-Amz-Date=",
        "X-Amz-Expires=",
        "X-Amz-SignedHeaders=host",
        "X-Amz-Signature=",
    ];
    required.iter().all(|needle| query.contains(needle))
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

fn generation_sealing_material_path(tenant_id: &TenantId, profile_id: &ProfileId) -> String {
    BRIDGE_GENERATION_SEALING_MATERIAL_PATH_TEMPLATE
        .replace("{tenantId}", tenant_id.as_str())
        .replace("{profileId}", profile_id.as_str())
}

fn generation_download_capability_path(tenant_id: &TenantId, profile_id: &ProfileId) -> String {
    BRIDGE_PROFILE_GENERATION_DOWNLOAD_CAPABILITY_PATH_TEMPLATE
        .replace("{tenantId}", tenant_id.as_str())
        .replace("{profileId}", profile_id.as_str())
}

fn generation_opening_material_path(tenant_id: &TenantId, profile_id: &ProfileId) -> String {
    BRIDGE_PROFILE_GENERATION_OPENING_MATERIAL_PATH_TEMPLATE
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
        ControlPlaneCoordinator, ControlPlaneEnrollment, ControlPlaneLeaseTiming,
        CoordinatorCursor, MAX_SEALING_MATERIAL_RESPONSE_BYTES, MachineHttpMethod, MachineHttpPort,
        MachineHttpResponse, ShippingControlPlaneError, lease_timing, valid_signed_r2_get_url,
    };
    use crate::dirty_generation::GenerationSealingMaterialPort;
    use crate::generation_reopen::GenerationReopenControlPort;
    use crate::operator_flow::EnrollmentPort;
    use bridge_domain::ClaimUri;
    use control_plane_contract::generation_key_api::{
        BRIDGE_GENERATION_SEALING_MATERIAL_PATH_TEMPLATE, BridgeGenerationSealingMaterialRequest,
        BridgeGenerationSealingMaterialResponse, GENERATION_SEALING_CHUNK_BYTES,
    };
    use control_plane_contract::generation_reopen_api::{
        BRIDGE_PROFILE_GENERATION_DOWNLOAD_CAPABILITY_PATH_TEMPLATE,
        BridgeGenerationDownloadCapabilityRequest, BridgeGenerationDownloadCapabilityResponse,
        GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
    };
    use encrypted_generation_domain::canonical_generation_object_key;
    use profile_platform_primitives::{
        CorrelationId, DeviceId, FencingToken, GenerationId, ProfileId, SessionId, TenantId,
        UnixMillis,
    };
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

    #[derive(Default)]
    struct SealingMachineHttp {
        responses: VecDeque<MachineHttpResponse>,
        requests: Vec<(MachineHttpMethod, String, Vec<u8>)>,
    }

    impl MachineHttpPort for SealingMachineHttp {
        type Error = ();

        fn request(
            &mut self,
            method: MachineHttpMethod,
            path: &str,
            _correlation_id: &CorrelationId,
            body: Option<&[u8]>,
        ) -> Result<MachineHttpResponse, Self::Error> {
            self.requests.push((
                method,
                path.to_owned(),
                body.map_or_else(Vec::new, <[u8]>::to_vec),
            ));
            self.responses.pop_front().ok_or(())
        }
    }

    struct SealingFixture {
        coordinator: ControlPlaneCoordinator<SealingMachineHttp>,
        tenant_id: TenantId,
        profile_id: ProfileId,
        base_generation_id: GenerationId,
        candidate_generation_id: GenerationId,
    }

    impl SealingFixture {
        fn new(response: MachineHttpResponse) -> Result<Self, Box<dyn std::error::Error>> {
            let tenant_id = TenantId::parse("tenant_sealing_adapter_01")?;
            let profile_id = ProfileId::parse("profile_sealing_adapter_01")?;
            let base_generation_id = GenerationId::parse("generation_sealing_base_01")?;
            let candidate_generation_id = GenerationId::parse("generation_sealing_next_01")?;
            let cursor = CoordinatorCursor {
                tenant_id: tenant_id.clone(),
                profile_id: profile_id.clone(),
                device_id: DeviceId::parse("device_sealing_adapter_01")?,
                session_id: SessionId::parse("session_sealing_adapter_01")?,
                epoch: 7,
                fencing_token: FencingToken::parse("fence_sealing_adapter_01")?,
                version: 11,
                sequence: 13,
                timing: ControlPlaneLeaseTiming {
                    idle_expires_at_ms: 30_000,
                    hard_expires_at_ms: 900_000,
                },
            };
            Ok(Self {
                coordinator: ControlPlaneCoordinator {
                    transport: SealingMachineHttp {
                        responses: VecDeque::from([response]),
                        requests: Vec::new(),
                    },
                    cursor: Some(cursor),
                },
                tenant_id,
                profile_id,
                base_generation_id,
                candidate_generation_id,
            })
        }

        fn request_material(&mut self) -> Result<(), ShippingControlPlaneError> {
            self.coordinator
                .material_for(
                    &self.tenant_id,
                    &self.profile_id,
                    &self.base_generation_id,
                    &self.candidate_generation_id,
                    [0xab; 32],
                )
                .map(|_| ())
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

    #[test]
    fn sealing_material_uses_live_cursor_authority_and_canonical_route()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = BridgeGenerationSealingMaterialResponse::new(
            "profile-generation-root-v1-2",
            "ab".repeat(32),
            "cd".repeat(16),
            GENERATION_SEALING_CHUNK_BYTES,
        );
        let mut fixture = SealingFixture::new(MachineHttpResponse::new(
            200,
            serde_json::to_vec(&response)?,
        ))?;
        fixture.request_material()?;

        assert_eq!(fixture.coordinator.transport.requests.len(), 1);
        let (method, path, body) = &fixture.coordinator.transport.requests[0];
        assert_eq!(*method, MachineHttpMethod::PostJson);
        let expected_path = BRIDGE_GENERATION_SEALING_MATERIAL_PATH_TEMPLATE
            .replace("{tenantId}", fixture.tenant_id.as_str())
            .replace("{profileId}", fixture.profile_id.as_str());
        assert_eq!(path, &expected_path);
        let request = serde_json::from_slice::<BridgeGenerationSealingMaterialRequest>(body)?;
        assert_eq!(
            request.base_generation_id(),
            fixture.base_generation_id.as_str()
        );
        assert_eq!(
            request.generation_id(),
            fixture.candidate_generation_id.as_str()
        );
        assert_eq!(request.plaintext_digest(), "ab".repeat(32));
        assert_eq!(
            request.coordinator_session_id(),
            "session_sealing_adapter_01"
        );
        assert_eq!(
            request.coordinator_fencing_token(),
            "fence_sealing_adapter_01"
        );
        assert_eq!(request.coordinator_epoch(), 7);
        let body_text = String::from_utf8_lossy(body);
        assert!(!body_text.contains("device_sealing_adapter_01"));
        assert!(!body_text.contains("rootKeyVersion"));
        assert!(!body_text.contains("coordinatorVersion"));
        assert!(!body_text.contains("coordinatorSequence"));
        Ok(())
    }

    #[test]
    fn sealing_material_requires_live_exact_cursor_before_transport()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_sealing_no_cursor_01")?;
        let profile_id = ProfileId::parse("profile_sealing_no_cursor_01")?;
        let base_generation_id = GenerationId::parse("generation_sealing_no_cursor_base_01")?;
        let generation_id = GenerationId::parse("generation_sealing_no_cursor_next_01")?;
        let mut coordinator = ControlPlaneCoordinator::new(SealingMachineHttp::default());
        assert_eq!(
            coordinator
                .material_for(
                    &tenant_id,
                    &profile_id,
                    &base_generation_id,
                    &generation_id,
                    [0; 32],
                )
                .map(|_| ()),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        assert!(coordinator.transport.requests.is_empty());
        Ok(())
    }

    #[test]
    fn sealing_material_rejects_noncanonical_or_oversized_secret_response()
    -> Result<(), Box<dyn std::error::Error>> {
        let malformed = [
            serde_json::to_vec(&BridgeGenerationSealingMaterialResponse::new(
                "profile-generation-root-v1-02",
                "ab".repeat(32),
                "cd".repeat(16),
                GENERATION_SEALING_CHUNK_BYTES,
            ))?,
            serde_json::to_vec(&BridgeGenerationSealingMaterialResponse::new(
                "profile-generation-root-v1-2",
                "AB".repeat(32),
                "cd".repeat(16),
                GENERATION_SEALING_CHUNK_BYTES,
            ))?,
            serde_json::to_vec(&BridgeGenerationSealingMaterialResponse::new(
                "profile-generation-root-v1-2",
                "ab".repeat(32),
                "CD".repeat(16),
                GENERATION_SEALING_CHUNK_BYTES,
            ))?,
            serde_json::to_vec(&BridgeGenerationSealingMaterialResponse::new(
                "profile-generation-root-v1-2",
                "ab".repeat(32),
                "cd".repeat(16),
                1,
            ))?,
            br#"{"keyId":"profile-generation-root-v1-2","dekHex":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","noncePrefixHex":"cccccccccccccccccccccccccccccccc","chunkSize":65536,"unexpected":true}"#.to_vec(),
            vec![b'x'; MAX_SEALING_MATERIAL_RESPONSE_BYTES + 1],
        ];
        for body in malformed {
            let mut fixture = SealingFixture::new(MachineHttpResponse::new(200, body))?;
            assert_eq!(
                fixture.request_material(),
                Err(ShippingControlPlaneError::InvalidResponse)
            );
        }
        Ok(())
    }

    #[test]
    fn sealing_material_rejects_non_success_without_reclassification()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = SealingFixture::new(MachineHttpResponse::new(403, b"denied".to_vec()))?;
        assert_eq!(
            fixture.request_material(),
            Err(ShippingControlPlaneError::HttpStatus)
        );
        Ok(())
    }

    #[test]
    fn reopen_download_uses_live_cursor_and_accepts_only_canonical_descriptor()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_sealing_adapter_01")?;
        let profile_id = ProfileId::parse("profile_sealing_adapter_01")?;
        let generation_id = GenerationId::parse("generation_sealing_next_01")?;
        let object_key = canonical_generation_object_key(&tenant_id, &profile_id, &generation_id);
        let signed_url = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/profile-generations/tenants/tenant_sealing_adapter_01/profiles/profile_sealing_adapter_01/generations/generation_sealing_next_01.bpgc?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=access%2F20260830%2Fauto%2Fs3%2Faws4_request&X-Amz-Date=20260830T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let response = BridgeGenerationDownloadCapabilityResponse::new(
            generation_id.as_str(),
            &object_key,
            "aa".repeat(32),
            "bb".repeat(32),
            4096,
            signed_url,
            GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
        );
        let mut fixture = SealingFixture::new(MachineHttpResponse::new(
            200,
            serde_json::to_vec(&response)?,
        ))?;
        let capability = fixture
            .coordinator
            .download_capability(&fixture.tenant_id, &fixture.profile_id)?;
        assert_eq!(capability.generation_id(), &fixture.candidate_generation_id);
        assert_eq!(capability.object_key(), object_key);
        assert_eq!(capability.container_bytes(), 4096);
        assert_eq!(capability.metadata_digest(), [0xaa; 32]);
        assert_eq!(capability.container_digest(), [0xbb; 32]);
        assert_eq!(capability.signed_url(), Some(signed_url));

        let (method, path, body) = &fixture.coordinator.transport.requests[0];
        assert_eq!(*method, MachineHttpMethod::PostJson);
        let expected_path = BRIDGE_PROFILE_GENERATION_DOWNLOAD_CAPABILITY_PATH_TEMPLATE
            .replace("{tenantId}", fixture.tenant_id.as_str())
            .replace("{profileId}", fixture.profile_id.as_str());
        assert_eq!(path, &expected_path);
        let request = serde_json::from_slice::<BridgeGenerationDownloadCapabilityRequest>(body)?;
        assert_eq!(request.coordinator_session_id(), "session_sealing_adapter_01");
        assert_eq!(request.coordinator_fencing_token(), "fence_sealing_adapter_01");
        assert_eq!(request.coordinator_epoch(), 7);
        let body_text = String::from_utf8_lossy(body);
        for forbidden in ["generationId", "objectKey", "metadataDigest", "containerDigest"] {
            assert!(!body_text.contains(forbidden));
        }
        Ok(())
    }

    #[test]
    fn reopen_requires_exact_cursor_before_transport() -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_reopen_no_cursor_01")?;
        let profile_id = ProfileId::parse("profile_reopen_no_cursor_01")?;
        let mut coordinator = ControlPlaneCoordinator::new(SealingMachineHttp::default());
        assert!(matches!(
            coordinator.download_capability(&tenant_id, &profile_id),
            Err(ShippingControlPlaneError::InvalidResponse)
        ));
        assert!(coordinator.transport.requests.is_empty());
        Ok(())
    }

    #[test]
    fn signed_r2_get_url_is_fail_closed() {
        let good = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/object?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=x&X-Amz-Date=20260830T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=abc";
        assert!(valid_signed_r2_get_url(good));
        for bad in [
            good.replacen("https://", "http://", 1),
            good.replacen(".r2.cloudflarestorage.com", ".example.com", 1),
            format!("{good}#fragment"),
            good.replacen("X-Amz-Signature=abc", "signature=abc", 1),
            good.replacen("0123456789abcdef0123456789abcdef", "ABCDEF0123456789ABCDEF0123456789", 1),
        ] {
            assert!(!valid_signed_r2_get_url(&bad));
        }
    }
}
