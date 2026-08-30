use crate::generation_reopen::{
    SignedGenerationObjectGetPort, SignedGenerationObjectGetResponse,
    VerifiedGenerationObjectDownloader,
};
use crate::local_profile::{GenerationWorkspace, LocalGenerationState, MaterializationRoot};
use crate::operator_flow::{
    BrowserLaunchPreflightPort, DeviceAuthenticationPort, ProfileBridgeOperator,
    RuntimeBundleSelectionPort,
};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use crate::shipping_control_plane::{
    ControlPlaneCoordinator, ControlPlaneEnrollment, MachineHttpMethod, MachineHttpPort,
    MachineHttpResponse,
};
use crate::shipping_generation_save::{
    SignedGenerationObjectPutPort, SignedGenerationUploadCapability,
};
use crate::{FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl};
use bridge_domain::{BridgePortError, ClaimUri};
use control_plane_contract::coordinator_api::{
    CoordinatorCommandDto, CoordinatorCommandRequestDto, CoordinatorOutcomeDto,
    CoordinatorProjectionDto, CoordinatorReleaseDispositionDto, CoordinatorResponseDto,
    CoordinatorStatusDto,
};
use control_plane_contract::generation_key_api::{
    BridgeGenerationSealingMaterialRequest, BridgeGenerationSealingMaterialResponse,
    GENERATION_SEALING_CHUNK_BYTES,
};
use control_plane_contract::generation_reopen_api::{
    BridgeGenerationDownloadCapabilityRequest, BridgeGenerationDownloadCapabilityResponse,
    BridgeGenerationOpeningMaterialRequest, BridgeGenerationOpeningMaterialResponse,
    GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
};
use control_plane_contract::profile_generation_api::{
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
use control_plane_contract::profile_launch_api::{
    BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH, BridgeProfileLaunchRedemptionProjection,
    BridgeProfileLaunchRedemptionRequest,
};
use encrypted_generation_domain::{
    canonical_generation_object_key, inspect_generation_metadata_prelude,
};
use profile_platform_primitives::{
    CorrelationId, DeviceId, GenerationId, ProfileId, TenantId, UnixMillis,
};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

const TENANT: &str = "tenant_p3_reopen_e2e";
const PROFILE: &str = "profile_p3_reopen_e2e";
const ACTOR: &str = "actor_p3_reopen_e2e";
const DEVICE: &str = "device_p3_reopen_e2e";
const BASE_GENERATION: &str = "generation_p3_reopen_base";
const ROOT_KEY_ID: &str = "profile-generation-root-v1-7";
const SIGNED_UPLOAD_URL: &str = "https://example.invalid/p3-e2e-upload?signature=redacted";
const SIGNED_DOWNLOAD_URL: &str = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/profile-generations/p3-e2e.bpgc?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=access%2F20260830%2Fauto%2Fs3%2Faws4_request&X-Amz-Date=20260830T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Clone)]
struct BackendMachineHttp {
    state: Rc<RefCell<BackendState>>,
}

impl MachineHttpPort for BackendMachineHttp {
    type Error = BridgePortError;

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        _correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        let mut state = self.state.borrow_mut();
        state.events.push(format!("http:{path}"));
        if path == BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH {
            if method != MachineHttpMethod::PostJson {
                return Err(BridgePortError::InvalidResponse);
            }
            let request = serde_json::from_slice::<BridgeProfileLaunchRedemptionRequest>(
                body.ok_or(BridgePortError::InvalidResponse)?,
            )
            .map_err(|_| BridgePortError::InvalidResponse)?;
            if request.claim_code().len() < 24 {
                return Err(BridgePortError::InvalidResponse);
            }
            state.launch_count = state.launch_count.saturating_add(1);
            state.pending_launch_intent =
                Some(format!("launch_p3_reopen_{:04}", state.launch_count));
            let active_generation = state.active_generation.as_str().to_owned();
            let projection = BridgeProfileLaunchRedemptionProjection {
                tenant_id: TENANT.to_owned(),
                actor_id: ACTOR.to_owned(),
                profile_id: PROFILE.to_owned(),
                generation_id: active_generation.clone(),
                device_id: DEVICE.to_owned(),
                launch_intent_id: state
                    .pending_launch_intent
                    .clone()
                    .ok_or(BridgePortError::InvalidResponse)?,
            };
            state.events.push(format!("redeem:{active_generation}"));
            return json_response(&projection);
        }

        if path.ends_with("/coordinator") {
            return match method {
                MachineHttpMethod::Get if body.is_none() => state.snapshot_response(),
                MachineHttpMethod::PostJson => {
                    state.coordinator_command(body.ok_or(BridgePortError::InvalidResponse)?)
                }
                _ => Err(BridgePortError::InvalidResponse),
            };
        }

        if method != MachineHttpMethod::PostJson {
            return Err(BridgePortError::InvalidResponse);
        }
        let body = body.ok_or(BridgePortError::InvalidResponse)?;
        if path.ends_with("/generation-successor/sealing-material") {
            return state.sealing_material(body);
        }
        if path.ends_with("/generation-successor/upload-capability") {
            return state.upload_capability(body);
        }
        if path.ends_with("/generation-successor/commit") {
            return state.commit_successor(body);
        }
        if path.ends_with("/generation-reopen/download-capability") {
            return state.download_capability(body);
        }
        if path.ends_with("/generation-reopen/opening-material") {
            return state.opening_material(body);
        }
        Err(BridgePortError::InvalidResponse)
    }
}

struct BackendState {
    active_generation: GenerationId,
    coordinator_status: CoordinatorStatusDto,
    coordinator_version: u64,
    coordinator_sequence: u64,
    epoch: u64,
    live_session_id: Option<String>,
    live_fencing_token: Option<String>,
    pending_launch_intent: Option<String>,
    launch_count: u64,
    pending_successor: Option<BridgeProfileGenerationSuccessorRequest>,
    committed_successor: Option<BridgeProfileGenerationSuccessorRequest>,
    object: Option<Vec<u8>>,
    events: Vec<String>,
}

impl BackendState {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            active_generation: GenerationId::parse(BASE_GENERATION)?,
            coordinator_status: CoordinatorStatusDto::Idle,
            coordinator_version: 1,
            coordinator_sequence: 0,
            epoch: 0,
            live_session_id: None,
            live_fencing_token: None,
            pending_launch_intent: None,
            launch_count: 0,
            pending_successor: None,
            committed_successor: None,
            object: None,
            events: Vec::new(),
        })
    }

    fn projection(&self) -> CoordinatorProjectionDto {
        let active = self.coordinator_status == CoordinatorStatusDto::Active;
        CoordinatorProjectionDto {
            tenant_id: TENANT.to_owned(),
            profile_id: PROFILE.to_owned(),
            status: self.coordinator_status,
            version: self.coordinator_version,
            sequence: self.coordinator_sequence,
            next_epoch: self.epoch.saturating_add(1),
            active_session_id: active.then(|| self.live_session_id.clone()).flatten(),
            active_device_id: active.then(|| DEVICE.to_owned()),
            active_epoch: active.then_some(self.epoch),
            idle_expires_at_ms: active.then_some(30_000),
            hard_expires_at_ms: active.then_some(900_000),
            drain_deadline_ms: None,
            pending_launch_intent_id: None,
            pending_intent_expires_at_ms: None,
        }
    }

    fn snapshot_response(&mut self) -> Result<MachineHttpResponse, BridgePortError> {
        if self.coordinator_status != CoordinatorStatusDto::Idle
            || self.live_session_id.is_some()
            || self.live_fencing_token.is_some()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.events.push("coordinator:snapshot:idle".to_owned());
        json_response(&CoordinatorResponseDto {
            outcome: CoordinatorOutcomeDto::Snapshot,
            version: self.coordinator_version,
            sequence: self.coordinator_sequence,
            replayed: false,
            fencing_token: None,
            epoch: None,
            projection: self.projection(),
        })
    }

    fn coordinator_command(&mut self, body: &[u8]) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<CoordinatorCommandRequestDto>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if request.expected_version != self.coordinator_version
            || request.sequence != self.coordinator_sequence.saturating_add(1)
        {
            return Err(BridgePortError::InvalidResponse);
        }
        match request.command {
            CoordinatorCommandDto::Claim {
                launch_intent_id,
                device_id,
                session_id,
            } => {
                if self.coordinator_status != CoordinatorStatusDto::Idle
                    || self.pending_launch_intent.as_deref() != Some(launch_intent_id.as_str())
                    || device_id != DEVICE
                    || self.live_session_id.is_some()
                {
                    return Err(BridgePortError::InvalidResponse);
                }
                self.coordinator_version = self.coordinator_version.saturating_add(1);
                self.coordinator_sequence = request.sequence;
                self.epoch = self.epoch.saturating_add(1);
                let fencing_token = format!("fence_p3_reopen_{:04}", self.epoch);
                self.live_session_id = Some(session_id.clone());
                self.live_fencing_token = Some(fencing_token.clone());
                self.pending_launch_intent = None;
                self.coordinator_status = CoordinatorStatusDto::Active;
                self.events
                    .push(format!("coordinator:claim:{}", self.epoch));
                json_response(&CoordinatorResponseDto {
                    outcome: CoordinatorOutcomeDto::LeaseClaimed,
                    version: self.coordinator_version,
                    sequence: self.coordinator_sequence,
                    replayed: false,
                    fencing_token: Some(fencing_token),
                    epoch: Some(self.epoch),
                    projection: self.projection(),
                })
            }
            CoordinatorCommandDto::Release {
                session_id,
                epoch,
                fencing_token,
                disposition,
            } => {
                self.check_witness(&session_id, &fencing_token, epoch)?;
                if disposition == CoordinatorReleaseDispositionDto::Clean
                    && self.committed_successor.is_none()
                {
                    return Err(BridgePortError::InvalidResponse);
                }
                self.coordinator_version = self.coordinator_version.saturating_add(1);
                self.coordinator_sequence = request.sequence;
                self.coordinator_status = match disposition {
                    CoordinatorReleaseDispositionDto::Clean => CoordinatorStatusDto::Idle,
                    CoordinatorReleaseDispositionDto::Dirty => CoordinatorStatusDto::Dirty,
                    CoordinatorReleaseDispositionDto::Uncertain => CoordinatorStatusDto::Uncertain,
                };
                self.live_session_id = None;
                self.live_fencing_token = None;
                self.events
                    .push(format!("coordinator:release:{disposition:?}"));
                json_response(&CoordinatorResponseDto {
                    outcome: CoordinatorOutcomeDto::Released,
                    version: self.coordinator_version,
                    sequence: self.coordinator_sequence,
                    replayed: false,
                    fencing_token: None,
                    epoch: None,
                    projection: self.projection(),
                })
            }
            CoordinatorCommandDto::Heartbeat {
                session_id,
                epoch,
                fencing_token,
            } => {
                self.check_witness(&session_id, &fencing_token, epoch)?;
                self.coordinator_version = self.coordinator_version.saturating_add(1);
                self.coordinator_sequence = request.sequence;
                self.events.push("coordinator:heartbeat".to_owned());
                json_response(&CoordinatorResponseDto {
                    outcome: CoordinatorOutcomeDto::HeartbeatAccepted,
                    version: self.coordinator_version,
                    sequence: self.coordinator_sequence,
                    replayed: false,
                    fencing_token: Some(fencing_token),
                    epoch: Some(epoch),
                    projection: self.projection(),
                })
            }
            _ => Err(BridgePortError::InvalidResponse),
        }
    }

    fn sealing_material(&mut self, body: &[u8]) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeGenerationSealingMaterialRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.check_witness(
            request.coordinator_session_id(),
            request.coordinator_fencing_token(),
            request.coordinator_epoch(),
        )?;
        if request.base_generation_id() != self.active_generation.as_str()
            || request.generation_id() == request.base_generation_id()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.events
            .push(format!("seal:{}", request.generation_id()));
        json_response(&BridgeGenerationSealingMaterialResponse::new(
            ROOT_KEY_ID,
            lower_hex(&[7; 32]),
            lower_hex(&[8; 16]),
            GENERATION_SEALING_CHUNK_BYTES,
        ))
    }

    fn upload_capability(&mut self, body: &[u8]) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeProfileGenerationSuccessorRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.validate_successor_request(&request)?;
        if let Some(existing) = self.pending_successor.as_ref()
            && existing != &request
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.pending_successor = Some(request.clone());
        let verified = self.object.as_ref().is_some_and(|object| {
            u64::try_from(object.len()).ok() == Some(request.container_bytes())
                && lower_hex(&Sha256::digest(object)) == request.container_digest()
        });
        if verified {
            self.events
                .push(format!("upload:verified:{}", request.generation_id()));
            json_response(&BridgeGenerationUploadCapabilityResponse::verified())
        } else {
            self.events
                .push(format!("upload:required:{}", request.generation_id()));
            json_response(&BridgeGenerationUploadCapabilityResponse::upload_required(
                SIGNED_UPLOAD_URL,
                &[("x-profile-generation".to_owned(), "p3-e2e".to_owned())],
                300,
            ))
        }
    }

    fn commit_successor(&mut self, body: &[u8]) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeProfileGenerationSuccessorRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.validate_successor_request(&request)?;
        if self.pending_successor.as_ref() != Some(&request) {
            return Err(BridgePortError::InvalidResponse);
        }
        let object = self
            .object
            .as_ref()
            .ok_or(BridgePortError::InvalidResponse)?;
        if u64::try_from(object.len()).ok() != Some(request.container_bytes())
            || lower_hex(&Sha256::digest(object)) != request.container_digest()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.active_generation = GenerationId::parse(request.generation_id().to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.committed_successor = Some(request.clone());
        self.events
            .push(format!("commit:{}", request.generation_id()));
        json_response(&BridgeGenerationSuccessorCommitResponse {
            outcome: BridgeGenerationSuccessorCommitOutcomeDto::Activated,
        })
    }

    fn download_capability(&mut self, body: &[u8]) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeGenerationDownloadCapabilityRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.check_witness(
            request.coordinator_session_id(),
            request.coordinator_fencing_token(),
            request.coordinator_epoch(),
        )?;
        let committed = self
            .committed_successor
            .clone()
            .ok_or(BridgePortError::InvalidResponse)?;
        if committed.generation_id() != self.active_generation.as_str() {
            return Err(BridgePortError::InvalidResponse);
        }
        self.events.push(format!(
            "download-capability:{}",
            self.active_generation.as_str()
        ));
        json_response(&BridgeGenerationDownloadCapabilityResponse::new(
            committed.generation_id(),
            committed.object_key(),
            committed.metadata_digest(),
            committed.container_digest(),
            committed.container_bytes(),
            SIGNED_DOWNLOAD_URL,
            GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
        ))
    }

    fn opening_material(&mut self, body: &[u8]) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeGenerationOpeningMaterialRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.check_witness(
            request.coordinator_session_id(),
            request.coordinator_fencing_token(),
            request.coordinator_epoch(),
        )?;
        let object = self
            .object
            .as_ref()
            .ok_or(BridgePortError::InvalidResponse)?;
        let inspected = inspect_generation_metadata_prelude(object)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let prelude = object
            .get(..inspected.prelude_bytes())
            .ok_or(BridgePortError::InvalidResponse)?;
        if request.metadata_prelude_hex() != lower_hex(prelude)
            || inspected.metadata().generation_id() != &self.active_generation
            || inspected.metadata().object_key()
                != canonical_generation_object_key(
                    &TenantId::parse(TENANT.to_owned())
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                    &ProfileId::parse(PROFILE.to_owned())
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                    &self.active_generation,
                )
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.events.push(format!(
            "opening-material:{}",
            self.active_generation.as_str()
        ));
        json_response(&BridgeGenerationOpeningMaterialResponse::new(
            ROOT_KEY_ID,
            lower_hex(&[7; 32]),
        ))
    }

    fn validate_successor_request(
        &self,
        request: &BridgeProfileGenerationSuccessorRequest,
    ) -> Result<(), BridgePortError> {
        self.check_witness(
            request.coordinator_session_id(),
            request.coordinator_fencing_token(),
            request.coordinator_epoch(),
        )?;
        if request.base_generation_id() != self.active_generation.as_str()
            || request.generation_id() == request.base_generation_id()
            || request.object_key()
                != canonical_generation_object_key(
                    &TenantId::parse(TENANT.to_owned())
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                    &ProfileId::parse(PROFILE.to_owned())
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                    &GenerationId::parse(request.generation_id().to_owned())
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                )
            || request.container_bytes() == 0
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }

    fn check_witness(
        &self,
        session_id: &str,
        fencing_token: &str,
        epoch: u64,
    ) -> Result<(), BridgePortError> {
        if self.coordinator_status != CoordinatorStatusDto::Active
            || self.live_session_id.as_deref() != Some(session_id)
            || self.live_fencing_token.as_deref() != Some(fencing_token)
            || self.epoch != epoch
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }
}

struct BackendPut {
    state: Rc<RefCell<BackendState>>,
}

impl SignedGenerationObjectPutPort for BackendPut {
    type Error = BridgePortError;

    fn put_exact(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        container: &[u8],
    ) -> Result<(), Self::Error> {
        if capability
            .url()
            .map_err(|_| BridgePortError::InvalidResponse)?
            != SIGNED_UPLOAD_URL
            || capability.expires_seconds() == 0
            || container.is_empty()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let mut state = self.state.borrow_mut();
        let pending = state
            .pending_successor
            .as_ref()
            .ok_or(BridgePortError::InvalidResponse)?;
        if u64::try_from(container.len()).ok() != Some(pending.container_bytes())
            || lower_hex(&Sha256::digest(container)) != pending.container_digest()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let pending_generation = pending.generation_id().to_owned();
        state.object = Some(container.to_vec());
        state.events.push(format!("put:{pending_generation}"));
        Ok(())
    }
}

struct BackendGet {
    state: Rc<RefCell<BackendState>>,
}

impl SignedGenerationObjectGetPort for BackendGet {
    type Error = BridgePortError;

    fn get_exact(
        &mut self,
        signed_url: &str,
        max_bytes: usize,
    ) -> Result<SignedGenerationObjectGetResponse, Self::Error> {
        if signed_url != SIGNED_DOWNLOAD_URL {
            return Err(BridgePortError::InvalidResponse);
        }
        let mut state = self.state.borrow_mut();
        let object = state
            .object
            .clone()
            .ok_or(BridgePortError::InvalidResponse)?;
        if object.len() != max_bytes {
            return Err(BridgePortError::InvalidResponse);
        }
        let active = state.active_generation.as_str().to_owned();
        state.events.push(format!("get:{active}"));
        Ok(SignedGenerationObjectGetResponse::new(200, object))
    }
}

struct DeviceAuthentication;

impl DeviceAuthenticationPort for DeviceAuthentication {
    type Error = BridgePortError;

    fn authenticate(&mut self, device_id: &DeviceId, key_handle: &str) -> Result<(), Self::Error> {
        if device_id.as_str() == DEVICE && !key_handle.is_empty() {
            Ok(())
        } else {
            Err(BridgePortError::InvalidResponse)
        }
    }
}

struct RecordingBundles {
    bundle: ApprovedRuntimeBundle,
    generations: Rc<RefCell<Vec<String>>>,
}

impl RuntimeBundleSelectionPort for RecordingBundles {
    type Error = BridgePortError;

    fn select_bundle(
        &mut self,
        _actor: &profile_platform_primitives::ActorContext,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<ApprovedRuntimeBundle, Self::Error> {
        if profile_id.as_str() != PROFILE {
            return Err(BridgePortError::InvalidResponse);
        }
        self.generations
            .borrow_mut()
            .push(generation_id.as_str().to_owned());
        Ok(self.bundle.clone())
    }
}

struct RecordingPreflight {
    observations: Rc<RefCell<Vec<(String, Vec<u8>)>>>,
}

impl BrowserLaunchPreflightPort for RecordingPreflight {
    type Error = BridgePortError;

    fn evaluate_before_launch(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        _workspace_epoch: u64,
        _runtime_bundle: &ApprovedRuntimeBundle,
    ) -> Result<(), Self::Error> {
        if device_id.as_str() != DEVICE {
            return Err(BridgePortError::InvalidResponse);
        }
        let generation = workspace
            .path()
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(BridgePortError::InvalidResponse)?
            .to_owned();
        let prefs = fs::read(workspace.path().join("prefs.js"))
            .map_err(|_| BridgePortError::Unavailable)?;
        self.observations.borrow_mut().push((generation, prefs));
        Ok(())
    }
}

struct CleanupRoot(PathBuf);

impl Drop for CleanupRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn json_response<T: Serialize>(value: &T) -> Result<MachineHttpResponse, BridgePortError> {
    serde_json::to_vec(value)
        .map(|body| MachineHttpResponse::new(200, body))
        .map_err(|_| BridgePortError::InvalidResponse)
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn digest(character: char) -> Result<Sha256Digest, Box<dyn std::error::Error>> {
    Ok(Sha256Digest::parse(character.to_string().repeat(64))?)
}

fn approved_bundle() -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
    let calculated = digest('a')?;
    let entrypoint = BundleRelativePath::parse("camouhost/main.py")?;
    let manifest = RuntimeManifest::new(
        "0.1.0",
        "3.12",
        RuntimePlatform::WindowsX86_64,
        entrypoint.clone(),
        calculated.clone(),
    )?;
    let inventory = RuntimeInventory::new([InventoryEntry::new(entrypoint, 10, digest('b')?)])?;
    Ok(ApprovedRuntimeBundle::validate(
        manifest,
        inventory,
        &calculated,
    )?)
}

fn event_index(events: &[String], prefix: &str) -> Result<usize, Box<dyn std::error::Error>> {
    events
        .iter()
        .position(|event| event.starts_with(prefix))
        .ok_or_else(|| format!("missing event {prefix}").into())
}

#[test]
fn canonical_save_then_local_loss_reopens_server_selected_successor()
-> Result<(), Box<dyn std::error::Error>> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root_path = std::env::temp_dir().join(format!(
        "profile-bridge-p3-reopen-e2e-{}-{counter}",
        std::process::id()
    ));
    let _cleanup = CleanupRoot(root_path.clone());
    let root = MaterializationRoot::open_or_create(root_path)?;
    let tenant_id = TenantId::parse(TENANT)?;
    let profile_id = ProfileId::parse(PROFILE)?;
    let base_generation = GenerationId::parse(BASE_GENERATION)?;
    let device_id = DeviceId::parse(DEVICE)?;
    let base_workspace = root.create_generation(&tenant_id, &profile_id, &base_generation)?;
    fs::write(base_workspace.path().join("prefs.js"), b"base")?;

    let state = Rc::new(RefCell::new(BackendState::new()?));
    let transport = BackendMachineHttp {
        state: Rc::clone(&state),
    };
    let runtime_generations = Rc::new(RefCell::new(Vec::new()));
    let preflight = Rc::new(RefCell::new(Vec::new()));
    let enrollment = ControlPlaneEnrollment::new(transport.clone());
    let coordinator = ControlPlaneCoordinator::new(transport.clone());
    let mut operator = ProfileBridgeOperator::new(
        FakeDeviceIdentity::new(device_id.clone()),
        FakeDeviceKeyStore::default(),
        DeviceAuthentication,
        enrollment,
        coordinator,
        RecordingBundles {
            bundle: approved_bundle()?,
            generations: Rc::clone(&runtime_generations),
        },
        RecordingPreflight {
            observations: Rc::clone(&preflight),
        },
        FakeProcessControl::default(),
        FakeCamouhost::default(),
    );
    let mut downloader = VerifiedGenerationObjectDownloader::new(BackendGet {
        state: Rc::clone(&state),
    });

    let first_claim = ClaimUri::parse("profilebridge://claim/claim_p3_reopen_e2e_first_000001")?;
    operator.open_authoritative(&first_claim, &root, &mut downloader, UnixMillis::new(10))?;
    assert_eq!(
        operator.active_local_state(),
        Some(LocalGenerationState::InUse)
    );
    let first_workspace = root.open_generation(&tenant_id, &profile_id, &base_generation)?;
    fs::write(
        first_workspace.path().join("prefs.js"),
        b"mutated-after-launch",
    )?;
    operator.close(UnixMillis::new(20))?;
    assert_eq!(
        operator.pending_dirty_local_state(),
        Some(LocalGenerationState::DirtyLocal)
    );

    let mut put = BackendPut {
        state: Rc::clone(&state),
    };
    let completion = operator.save_retained_successor(
        &root,
        transport.clone(),
        &mut put,
        UnixMillis::new(30),
    )?;
    assert!(completion.is_saved());
    let successor = completion.committed().generation_id().clone();
    assert_ne!(successor, base_generation);
    {
        let backend = state.borrow();
        assert_eq!(backend.active_generation, successor);
        assert_eq!(backend.coordinator_status, CoordinatorStatusDto::Idle);
        assert!(backend.committed_successor.is_some());
    }

    root.reject_generation_for_rematerialization(&tenant_id, &profile_id, &successor)?;
    assert!(
        root.open_generation(&tenant_id, &profile_id, &successor)
            .is_err()
    );

    let second_claim = ClaimUri::parse("profilebridge://claim/claim_p3_reopen_e2e_second_000002")?;
    operator.open_authoritative(&second_claim, &root, &mut downloader, UnixMillis::new(40))?;
    assert_eq!(
        operator.active_local_state(),
        Some(LocalGenerationState::InUse)
    );
    let reopened = root.open_generation(&tenant_id, &profile_id, &successor)?;
    assert_eq!(
        fs::read(reopened.path().join("prefs.js"))?,
        b"mutated-after-launch"
    );

    assert_eq!(
        runtime_generations.borrow().as_slice(),
        [BASE_GENERATION, successor.as_str()]
    );
    let observations = preflight.borrow();
    assert_eq!(observations.len(), 2);
    assert_eq!(observations[0].0, BASE_GENERATION);
    assert_eq!(observations[0].1, b"base");
    assert_eq!(observations[1].0, successor.as_str());
    assert_eq!(observations[1].1, b"mutated-after-launch");
    drop(observations);

    let events = state.borrow().events.clone();
    let commit = event_index(&events, "commit:")?;
    let clean_release = event_index(&events, "coordinator:release:Clean")?;
    let second_redeem = events
        .iter()
        .enumerate()
        .filter(|(_, event)| event.starts_with("redeem:"))
        .nth(1)
        .map(|(index, _)| index)
        .ok_or("missing second redemption")?;
    let capability = event_index(&events, "download-capability:")?;
    let get = event_index(&events, "get:")?;
    let opening = event_index(&events, "opening-material:")?;
    assert!(commit < clean_release);
    assert!(clean_release < second_redeem);
    assert!(second_redeem < capability);
    assert!(capability < get);
    assert!(get < opening);
    assert!(events.iter().all(|event| !event.contains("device-job")));

    operator.abort(UnixMillis::new(50))?;
    Ok(())
}
