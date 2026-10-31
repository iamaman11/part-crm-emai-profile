use crate::browser_execution::{load_materialization_binding, persist_materialization_binding};
use crate::generation_object_spool::VerifiedGenerationObjectFile;
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
    SignedGenerationObjectPutPort, SignedGenerationObjectStreamPutPort,
    SignedGenerationUploadCapability,
};
use crate::test_support::browser_identity_fixture;
use crate::{FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl};
use bridge_domain::{BridgePortError, ClaimUri};
use browser_execution_domain::MaterializationBinding;
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
    BridgeGenerationWorkingProofRequest, GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
};
use control_plane_contract::profile_generation_api::{
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationSuccessorRetryDecisionRequest, BridgeGenerationSuccessorRetryDecisionResponse,
    BridgeGenerationSuccessorRetryVerdict, BridgeGenerationUploadCapabilityResponse,
    BridgeProfileGenerationSuccessorRequest,
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
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

type PreflightObservation = (String, Vec<u8>);
type PreflightObservations = Rc<RefCell<Vec<PreflightObservation>>>;

const TENANT: &str = "tenant_p3_reopen_e2e";
const PROFILE: &str = "profile_p3_reopen_e2e";
const ACTOR: &str = "actor_p3_reopen_e2e";
const DEVICE: &str = "device_p3_reopen_e2e";
const BASE_GENERATION: &str = "generation_p3_reopen_base";
const ROOT_KEY_ID: &str = "profile-generation-root-v1-7";
const ROTATED_ROOT_KEY_ID: &str = "profile-generation-root-v1-8";
const SIGNED_UPLOAD_URL: &str = "https://example.invalid/p3-e2e-upload?signature=redacted";
// Synthetic signer keeps the read-only historical object distinct from CURRENT.
const HISTORICAL_SIGNED_DOWNLOAD_URL: &str = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/profile-generations/p3-e2e-old.bpgc?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=access%2F20260830%2Fauto%2Fs3%2Faws4_request&X-Amz-Date=20260830T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
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
        if path.ends_with("/generation-successor/retry-decision") {
            return state.retry_decision(body);
        }
        if path.ends_with("/generation-successor/sealing-material") {
            return state.sealing_material(body);
        }
        if path.ends_with("/generation-successor/upload-capability") {
            return state.upload_capability(body);
        }
        if path.ends_with("/generation-successor/commit") {
            return state.commit_successor(body);
        }
        if path.ends_with("/generation-working-candidate/download-capability") {
            return state.historical_download_capability(body);
        }
        if path.ends_with("/generation-working-candidate/opening-material") {
            return state.historical_opening_material(body);
        }
        if path.ends_with("/generation-working-candidate/certify") {
            return state.historical_working_proof(body);
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
    // Fixture-only immutable object history. The committed object stays
    // intact while a different successor's PUT/commit is unresolved.
    pending_object: Option<Vec<u8>>,
    object: Option<Vec<u8>>,
    retained_object: Option<(BridgeProfileGenerationSuccessorRequest, Vec<u8>)>,
    // Fixture-only retained old-root outage. Production uses the existing
    // authenticated keyring; a missing historical key must never silently
    // fall back to the newest root or trigger a new browser writer.
    historical_root_available: bool,
    historical_working_proof: Option<String>,
    /// Inject a stale D1/CURRENT read during the second (post-browser) check.
    /// The first read and real cold launch must have already succeeded.
    historical_recheck_rejected: bool,
    historical_capability_reads: u32,
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
            pending_object: None,
            object: None,
            retained_object: None,
            historical_root_available: true,
            historical_working_proof: None,
            historical_recheck_rejected: false,
            historical_capability_reads: 0,
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
        // The second real-browser Save exercises key rotation while the
        // previous encrypted generation retains its historical opening key.
        let (key_id, key_bytes, nonce_bytes) = if self.committed_successor.is_some() {
            (ROTATED_ROOT_KEY_ID, [9_u8; 32], [10_u8; 16])
        } else {
            (ROOT_KEY_ID, [7_u8; 32], [8_u8; 16])
        };
        json_response(&BridgeGenerationSealingMaterialResponse::new(
            key_id,
            lower_hex(&key_bytes),
            lower_hex(&nonce_bytes),
            GENERATION_SEALING_CHUNK_BYTES,
        ))
    }

    fn retry_decision(&mut self, body: &[u8]) -> Result<MachineHttpResponse, BridgePortError> {
        let request =
            serde_json::from_slice::<BridgeGenerationSuccessorRetryDecisionRequest>(body)
                .map_err(|_| BridgePortError::InvalidResponse)?;
        self.check_witness(
            &request.coordinator_session_id,
            &request.coordinator_fencing_token,
            request.coordinator_epoch,
        )?;
        let attempt = request.attempt;
        let tenant = TenantId::parse(TENANT.to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let profile = ProfileId::parse(PROFILE.to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let generation = GenerationId::parse(attempt.generation_id.clone())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if attempt.base_generation_id != self.active_generation.as_str()
            || attempt.generation_id == attempt.base_generation_id
            || attempt.object_key != canonical_generation_object_key(&tenant, &profile, &generation)
            || attempt.container_bytes == 0
            || attempt.metadata_digest.len() != 64
            || attempt.container_digest.len() != 64
            || self.pending_successor.is_some()
            || self.pending_object.is_some()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        // The fixture represents an independently verified D1 nonapplication
        // observation. It never grants a stale fence, a signed PUT or a second
        // successor. In production the same verdict comes from the D1 owner.
        self.events
            .push(format!("retry:nonapplied:{}", attempt.generation_id));
        json_response(&BridgeGenerationSuccessorRetryDecisionResponse {
            verdict: BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable,
        })
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
        let verified = self.pending_object.as_ref().is_some_and(|object| {
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
            .pending_object
            .as_ref()
            .ok_or(BridgePortError::InvalidResponse)?;
        if u64::try_from(object.len()).ok() != Some(request.container_bytes())
            || lower_hex(&Sha256::digest(object)) != request.container_digest()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let successor = GenerationId::parse(request.generation_id().to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if let (Some(previous), Some(previous_object)) =
            (self.committed_successor.take(), self.object.take())
        {
            self.retained_object = Some((previous, previous_object));
        }
        self.object = self.pending_object.take();
        self.pending_successor = None;
        self.active_generation = successor;
        self.committed_successor = Some(request.clone());
        self.events
            .push(format!("commit:{}", request.generation_id()));
        json_response(&BridgeGenerationSuccessorCommitResponse {
            outcome: BridgeGenerationSuccessorCommitOutcomeDto::Activated,
        })
    }

    /// Preserve the genuine CURRENT pointer. The fixture server, not the
    /// request body, selects the journal-linked immediate VERIFIED predecessor.
    /// The same current coordinator cursor is required for both historical
    /// requests, matching the production worker's read-only admission.
    fn historical_download_capability(
        &mut self,
        body: &[u8],
    ) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeGenerationDownloadCapabilityRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.check_witness(
            request.coordinator_session_id(),
            request.coordinator_fencing_token(),
            request.coordinator_epoch(),
        )?;
        self.historical_capability_reads = self.historical_capability_reads.saturating_add(1);
        if self.historical_recheck_rejected && self.historical_capability_reads >= 2 {
            return Err(BridgePortError::InvalidResponse);
        }
        let (previous, bytes) = self.retained_object
            .as_ref()
            .ok_or(BridgePortError::InvalidResponse)?;
        if previous.generation_id() == self.active_generation.as_str() || self.object.is_none() {
            return Err(BridgePortError::InvalidResponse);
        }
        let metadata = inspect_generation_metadata_prelude(bytes)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if metadata.metadata().generation_id().as_str() != previous.generation_id() {
            return Err(BridgePortError::InvalidResponse);
        }
        self.events.push(format!("historical-download-capability:{}", previous.generation_id()));
        json_response(&BridgeGenerationDownloadCapabilityResponse::new(
            previous.generation_id(),
            previous.object_key(),
            previous.metadata_digest(),
            previous.container_digest(),
            previous.container_bytes(),
            HISTORICAL_SIGNED_DOWNLOAD_URL,
            GENERATION_DOWNLOAD_CAPABILITY_MAX_EXPIRES_SECONDS,
        ))
    }

    fn historical_opening_material(
        &mut self,
        body: &[u8],
    ) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeGenerationOpeningMaterialRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.check_witness(
            request.coordinator_session_id(),
            request.coordinator_fencing_token(),
            request.coordinator_epoch(),
        )?;
        let (previous, bytes) = self.retained_object
            .as_ref()
            .ok_or(BridgePortError::InvalidResponse)?;
        if previous.generation_id() == self.active_generation.as_str() || self.object.is_none() {
            return Err(BridgePortError::InvalidResponse);
        }
        let inspected = inspect_generation_metadata_prelude(bytes)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let prelude = bytes.get(..inspected.prelude_bytes())
            .ok_or(BridgePortError::InvalidResponse)?;
        if request.metadata_prelude_hex() != lower_hex(prelude)
            || inspected.metadata().generation_id().as_str() != previous.generation_id()
            || inspected.metadata().object_key() != previous.object_key()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let key_bytes = match inspected.metadata().key_id().as_str() {
            ROOT_KEY_ID if self.historical_root_available => [7_u8; 32],
            ROOT_KEY_ID => return Err(BridgePortError::InvalidResponse),
            ROTATED_ROOT_KEY_ID => [9_u8; 32],
            _ => return Err(BridgePortError::InvalidResponse),
        };
        self.events.push(format!("historical-opening-material:{}", previous.generation_id()));
        json_response(&BridgeGenerationOpeningMaterialResponse::new(
            inspected.metadata().key_id().as_str(),
            lower_hex(&key_bytes),
        ))
    }

    fn historical_working_proof(
        &mut self,
        body: &[u8],
    ) -> Result<MachineHttpResponse, BridgePortError> {
        let request = serde_json::from_slice::<BridgeGenerationWorkingProofRequest>(body)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.check_witness(
            request.coordinator_session_id(),
            request.coordinator_fencing_token(),
            request.coordinator_epoch(),
        )?;
        if !self.historical_root_available
            || request.current_generation_id() != self.active_generation.as_str()
            || request.current_generation_id() == request.predecessor_generation_id()
            || request.runtime_inventory_sha256().len() != 64
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let (previous, bytes) = self.retained_object
            .as_ref()
            .ok_or(BridgePortError::InvalidResponse)?;
        let inspected = inspect_generation_metadata_prelude(bytes)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let prelude = bytes.get(..inspected.prelude_bytes())
            .ok_or(BridgePortError::InvalidResponse)?;
        if previous.generation_id() != request.predecessor_generation_id()
            || request.metadata_prelude_hex() != lower_hex(prelude)
            || inspected.metadata().key_id().as_str() != ROOT_KEY_ID
            || self.object.is_none()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let previous_generation = previous.generation_id().to_owned();
        self.historical_working_proof = Some(previous_generation.clone());
        self.events.push(format!("historical-working-proof:{previous_generation}"));
        json_response(&serde_json::json!({"confirmed": true}))
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
        // Use only the key version authenticated in the old immutable
        // object's metadata. Never substitute the currently active root.
        let opening_key = match inspected.metadata().key_id().as_str() {
            ROOT_KEY_ID if self.historical_root_available => [7_u8; 32],
            ROOT_KEY_ID => return Err(BridgePortError::InvalidResponse),
            ROTATED_ROOT_KEY_ID => [9_u8; 32],
            _ => return Err(BridgePortError::InvalidResponse),
        };
        json_response(&BridgeGenerationOpeningMaterialResponse::new(
            inspected.metadata().key_id().as_str(),
            lower_hex(&opening_key),
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
        state.pending_object = Some(container.to_vec());
        state.events.push(format!("put:{pending_generation}"));
        Ok(())
    }
}

impl SignedGenerationObjectStreamPutPort for BackendPut {
    fn put_verified_stream(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        object: &mut VerifiedGenerationObjectFile,
    ) -> Result<(), Self::Error> {
        object
            .recheck_and_rewind()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        // Test-only in-memory R2 substitute. Shipping's Windows adapter reads
        // the same VerifiedGenerationObjectFile directly into curl stdin.
        let mut captured = Vec::new();
        object
            .reader()
            .read_to_end(&mut captured)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if u64::try_from(captured.len()).ok() != Some(object.expected_bytes()) {
            return Err(BridgePortError::InvalidResponse);
        }
        object
            .recheck_and_rewind()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.put_exact(capability, &captured)
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
        let mut state = self.state.borrow_mut();
        let (object, selected, historical) = if signed_url == HISTORICAL_SIGNED_DOWNLOAD_URL {
            let (previous, bytes) = state.retained_object
                .as_ref()
                .ok_or(BridgePortError::InvalidResponse)?;
            if previous.generation_id() == state.active_generation.as_str() {
                return Err(BridgePortError::InvalidResponse);
            }
            (bytes.clone(), previous.generation_id().to_owned(), true)
        } else if signed_url == SIGNED_DOWNLOAD_URL {
            (
                state.object.clone().ok_or(BridgePortError::InvalidResponse)?,
                state.active_generation.as_str().to_owned(),
                false,
            )
        } else {
            return Err(BridgePortError::InvalidResponse);
        };
        if object.len() != max_bytes {
            return Err(BridgePortError::InvalidResponse);
        }
        if historical {
            state.events.push(format!("historical-get:{selected}"));
        } else {
            state.events.push(format!("get:{selected}"));
        }
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

struct TestRuntimeBundle {
    bundle: ApprovedRuntimeBundle,
}

impl RuntimeBundleSelectionPort for TestRuntimeBundle {
    type Error = BridgePortError;

    fn select_bundle(&mut self) -> Result<ApprovedRuntimeBundle, Self::Error> {
        Ok(self.bundle.clone())
    }
}

struct RecordingPreflight {
    observations: PreflightObservations,
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
        let generation = workspace.generation_id().as_str().to_owned();
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
    run_canonical_saved_successor_reopen(true)
}

#[test]
fn canonical_save_then_local_reuse_skips_cloud_download()
-> Result<(), Box<dyn std::error::Error>> {
    run_canonical_saved_successor_reopen(false)
}

/// Hosted Windows operator-composition regression, NOT a live Cloudflare DO/R2
/// or a pinned Camoufox Playwright result. Its backend is an in-memory provider
/// fixture; the filesystem/lock, streamed encrypted Save, signed GET and actual
/// shipping Operator ownership/materialization are the production owners.
///
/// Models the exact order missing from the original physical B7 fixture:
/// confirmed N+1, a fresh writer, a crash with new browser WAL bytes, a
/// separately-authorized recovery claim, Save N+2, and cold download N+2.
/// The preceding coordinator-domain test proves the atomic same-device
/// Uncertain recovery; this fixture does NOT grant authority itself.
#[cfg(windows)]
#[test]
fn committed_then_crashed_writer_recovers_wal_and_cold_reopens_sole_successor()
-> Result<(), Box<dyn std::error::Error>> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root_path = std::env::temp_dir().join(format!(
        "profile-bridge-p3-crash-streamed-{}-{counter}",
        std::process::id()
    ));
    let _cleanup = CleanupRoot(root_path.clone());
    let root = MaterializationRoot::open_or_create(root_path)?;
    let tenant = TenantId::parse(TENANT)?;
    let profile = ProfileId::parse(PROFILE)?;
    let device = DeviceId::parse(DEVICE)?;
    let base_id = GenerationId::parse(BASE_GENERATION)?;
    let base = root.create_generation(&tenant, &profile, &base_id)?;
    fs::write(base.path().join("prefs.js"), b"first-committed-source")?;
    let config = b"{}\n";
    fs::write(base.path().join("camoufox-config.json"), config)?;
    let bundle = approved_bundle()?;
    let identity = browser_identity_fixture(
        bundle.manifest().runtime_version(),
        bundle.manifest().inventory_sha256().as_str(),
        "profile-stability-v1-probe-p3-reopen-e2e",
        lower_hex(&Sha256::digest(config)),
    )?;
    let binding = MaterializationBinding::new(
        tenant.clone(),
        profile.clone(),
        base_id.clone(),
        "c".repeat(64),
        base.materialization_inventory_digest()?,
        identity,
    )?;
    persist_materialization_binding(&base, &binding)?;

    let state = Rc::new(RefCell::new(BackendState::new()?));
    let transport = BackendMachineHttp { state: Rc::clone(&state) };
    let preflights: PreflightObservations = Rc::new(RefCell::new(Vec::new()));
    let make_operator = || -> Result<_, Box<dyn std::error::Error>> {
        Ok(ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(device.clone()),
            FakeDeviceKeyStore::default(),
            DeviceAuthentication,
            ControlPlaneEnrollment::new(transport.clone()),
            ControlPlaneCoordinator::new(transport.clone()),
            TestRuntimeBundle { bundle: approved_bundle()? },
            RecordingPreflight { observations: Rc::clone(&preflights) },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        ))
    };
    let mut downloader = VerifiedGenerationObjectDownloader::new(BackendGet {
        state: Rc::clone(&state),
    });
    let mut uploader = BackendPut { state: Rc::clone(&state) };
    let mut operator = make_operator()?;
    let first_claim = ClaimUri::parse(
        "profilebridge://claim/claim_p3_crash_first_000001_0123456789",
    )?;
    operator.open_authoritative(&first_claim, &root, &mut downloader, UnixMillis::new(10))?;
    fs::write(base.path().join("prefs.js"), b"confirmed-committed-generation")?;
    operator.close_with_durable_local_intent(&root, UnixMillis::new(20))?;
    let first = operator.save_retained_successor_streaming(
        &root,
        transport.clone(),
        &mut uploader,
        UnixMillis::new(30),
    )?;
    assert!(first.is_saved());
    let committed_id = first.committed().generation_id().clone();
    assert_ne!(committed_id, base_id);
    assert_eq!(state.borrow().coordinator_status, CoordinatorStatusDto::Idle);

    // This fresh Open uses the exact cloud-committed CURRENT generation.
    let second_claim = ClaimUri::parse(
        "profilebridge://claim/claim_p3_crash_second_000002_0123456789",
    )?;
    operator.open_authoritative(&second_claim, &root, &mut downloader, UnixMillis::new(40))?;
    let committed = root.open_generation(&tenant, &profile, &committed_id)?;
    assert_eq!(fs::read(committed.path().join("prefs.js"))?, b"confirmed-committed-generation");
    let user_data = committed.path().join("user_data");
    fs::create_dir_all(&user_data)?;
    let cookie_db = user_data.join("cookies.sqlite");
    let wal = user_data.join("cookies.sqlite-wal");
    let cookie_bytes = b"confirmed-cookie-db-before-crash";
    let wal_bytes = b"new-unpublished-cookie-wal-after-crash";
    fs::write(&cookie_db, cookie_bytes)?;
    fs::write(&wal, wal_bytes)?;
    let marker = committed.path().join(".profile-platform.lock");
    let before_epoch = state.borrow().epoch;
    assert_eq!(
        fs::read_to_string(&marker)?,
        format!("profile-platform-bridge-lock-v1\n{}\n{before_epoch}\n", device.as_str()),
    );

    // Drop without Close/Save models process death: the OS admission handle
    // disappears but the durable Bridge lock and dirty Firefox bytes survive.
    drop(operator);
    assert!(marker.exists());
    assert_eq!(fs::read(&wal)?, wal_bytes);
    {
        let mut server = state.borrow_mut();
        assert_eq!(server.coordinator_status, CoordinatorStatusDto::Active);
        server.coordinator_status = CoordinatorStatusDto::Uncertain;
        server.coordinator_version += 1;
        server.coordinator_sequence += 1;
        server.live_session_id = None;
        server.live_fencing_token = None;
        // The real frontend/DO atomic RecoverAndIssueLaunchIntent is proven
        // separately in coordinator.rs. Model its *positive authorized result*
        // here: a new one-time claim can be redeemed; this fake transport is
        // NOT a simulation of DO provenance verification.
        server.coordinator_status = CoordinatorStatusDto::Idle;
        server.coordinator_version += 1;
        server.coordinator_sequence += 1;
        server.events.push("fixture:authorized-uncertain-recovery".to_owned());
    }

    let mut recovered = make_operator()?;
    let recovered_claim = ClaimUri::parse(
        "profilebridge://claim/claim_p3_crash_recovered_000003_0123456789",
    )?;
    recovered.open_authoritative(
        &recovered_claim, &root, &mut downloader, UnixMillis::new(50),
    )?;
    assert_eq!(recovered.active_local_state(), Some(LocalGenerationState::InUse));
    let recovered_epoch = state.borrow().epoch;
    assert_eq!(recovered_epoch, before_epoch + 1);
    assert_eq!(
        fs::read_to_string(&marker)?,
        format!("profile-platform-bridge-lock-v1\n{}\n{recovered_epoch}\n", device.as_str()),
    );
    assert_eq!(fs::read(&cookie_db)?, cookie_bytes);
    assert_eq!(fs::read(&wal)?, wal_bytes);
    assert_eq!(
        state.borrow().events.iter().filter(|e| e.starts_with("commit:")).count(),
        1,
        "a fresh recovered Open must not duplicate the previous commit",
    );

    // Explicit next controlled Close/Save publishes the new WAL exactly once.
    recovered.close_with_durable_local_intent(&root, UnixMillis::new(60))?;
    let second = recovered.save_retained_successor_streaming(
        &root,
        transport.clone(),
        &mut uploader,
        UnixMillis::new(70),
    )?;
    assert!(second.is_saved());
    let next_id = second.committed().generation_id().clone();
    assert_ne!(next_id, committed_id);
    let next = root.open_generation(&tenant, &profile, &next_id)?;
    assert_eq!(fs::read(next.path().join("user_data/cookies.sqlite-wal"))?, wal_bytes);
    assert_eq!(fs::read(next.path().join("user_data/cookies.sqlite"))?, cookie_bytes);

    // Only a disposable test workspace is removed. A genuinely independent
    // signed GET + decrypted materialization must restore the NEW saved WAL.
    fs::remove_dir_all(next.path())?;
    let mut cold = make_operator()?;
    let cold_claim = ClaimUri::parse(
        "profilebridge://claim/claim_p3_crash_cold_000004_0123456789",
    )?;
    cold.open_authoritative(&cold_claim, &root, &mut downloader, UnixMillis::new(80))?;
    let restored = root.open_generation(&tenant, &profile, &next_id)?;
    assert_eq!(fs::read(restored.path().join("user_data/cookies.sqlite-wal"))?, wal_bytes);
    assert_eq!(fs::read(restored.path().join("user_data/cookies.sqlite"))?, cookie_bytes);
    {
        let backend = state.borrow();
        let events = &backend.events;
        assert_eq!(events.iter().filter(|e| e.starts_with("commit:")).count(), 2);
        assert_eq!(events.iter().filter(|e| e.starts_with("put:")).count(), 2);
        assert_eq!(events.iter().filter(|e| e.starts_with("get:")).count(), 1);
        assert_eq!(
            events
                .iter()
                .filter(|e| e.as_str() == "fixture:authorized-uncertain-recovery")
                .count(),
            1,
        );
    }
    cold.abort(UnixMillis::new(90))?;
    Ok(())
}

#[cfg(windows)]
/// K3 source/fixture proof, not a live-provider R2 retention or installed B7 verdict.
/// Exercise the SAME streamed Save owner 100 times with one real filesystem root,
/// a single coordinator and an in-memory machine/R2 fixture. Every reopen must
/// select the previous committed generation and reuse verified native local bytes.
#[test]
fn one_hundred_sequential_streamed_saves_keep_current_local_profile_reopenable()
-> Result<(), Box<dyn std::error::Error>> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root_path = std::env::temp_dir().join(format!(
        "profile-bridge-p3-100-streamed-{}-{counter}",
        std::process::id()
    ));
    let _cleanup = CleanupRoot(root_path.clone());
    let root = MaterializationRoot::open_or_create(root_path)?;
    let tenant = TenantId::parse(TENANT)?;
    let profile = ProfileId::parse(PROFILE)?;
    let device = DeviceId::parse(DEVICE)?;
    let mut current = GenerationId::parse(BASE_GENERATION)?;
    let base = root.create_generation(&tenant, &profile, &current)?;
    fs::write(base.path().join("prefs.js"), b"initial")?;
    let config = b"{}\n";
    fs::write(base.path().join("camoufox-config.json"), config)?;
    let bundle = approved_bundle()?;
    let identity = browser_identity_fixture(
        bundle.manifest().runtime_version(),
        bundle.manifest().inventory_sha256().as_str(),
        "profile-stability-v1-probe-p3-reopen-e2e",
        lower_hex(&Sha256::digest(config)),
    )?;
    let binding = MaterializationBinding::new(
        tenant.clone(),
        profile.clone(),
        current.clone(),
        "c".repeat(64),
        base.materialization_inventory_digest()?,
        identity,
    )?;
    persist_materialization_binding(&base, &binding)?;

    let state = Rc::new(RefCell::new(BackendState::new()?));
    let transport = BackendMachineHttp { state: Rc::clone(&state) };
    let preflights: PreflightObservations = Rc::new(RefCell::new(Vec::new()));
    let mut operator = ProfileBridgeOperator::new(
        FakeDeviceIdentity::new(device),
        FakeDeviceKeyStore::default(),
        DeviceAuthentication,
        ControlPlaneEnrollment::new(transport.clone()),
        ControlPlaneCoordinator::new(transport.clone()),
        TestRuntimeBundle { bundle },
        RecordingPreflight { observations: Rc::clone(&preflights) },
        FakeProcessControl::default(),
        FakeCamouhost::default(),
    );
    let mut downloader = VerifiedGenerationObjectDownloader::new(BackendGet {
        state: Rc::clone(&state),
    });
    let mut put = BackendPut { state: Rc::clone(&state) };
    let mut seen = std::collections::HashSet::new();
    seen.insert(current.clone());
    let mut expected = b"initial".to_vec();

    for cycle in 0..100_u64 {
        let claim = ClaimUri::parse(&format!(
            "profilebridge://claim/claim_p3_100_streamed_{cycle:04}_0123456789abcdef"
        ))?;
        let started = 10 + cycle * 100;
        operator.open_authoritative(&claim, &root, &mut downloader, UnixMillis::new(started))?;
        assert_eq!(operator.active_local_state(), Some(LocalGenerationState::InUse));
        let workspace = root.open_generation(&tenant, &profile, &current)?;
        assert_eq!(fs::read(workspace.path().join("prefs.js"))?, expected);
        let updated = format!("verified-sequential-save-{cycle:03}").into_bytes();
        fs::write(workspace.path().join("prefs.js"), &updated)?;
        operator.close_with_durable_local_intent(&root, UnixMillis::new(started + 10))?;
        let completion = operator.save_retained_successor_streaming(
            &root,
            transport.clone(),
            &mut put,
            UnixMillis::new(started + 20),
        )?;
        assert!(completion.is_saved(), "save cycle {cycle} reported incomplete");
        let next = completion.committed().generation_id().clone();
        assert_ne!(current, next);
        assert!(seen.insert(next.clone()), "duplicate successor on cycle {cycle}");
        let accepted = root.open_generation(&tenant, &profile, &next)?;
        let ready = load_materialization_binding(&accepted, &tenant, &profile, &next)?;
        assert_eq!(ready.source_container_sha256(), completion.committed().container_digest());
        assert_eq!(ready.materialized_inventory_digest(), accepted.materialization_inventory_digest()?);
        assert_eq!(fs::read(accepted.path().join("prefs.js"))?, updated);
        {
            let backend = state.borrow();
            assert_eq!(backend.active_generation, next);
            assert_eq!(backend.coordinator_status, CoordinatorStatusDto::Idle);
            assert!(backend.pending_object.is_none());
            assert!(backend.pending_successor.is_none());
        }
        current = next;
        expected = updated;
    }
    let backend = state.borrow();
    let events = &backend.events;
    assert_eq!(events.iter().filter(|e| e.starts_with("commit:")).count(), 100);
    assert_eq!(events.iter().filter(|e| e.starts_with("put:")).count(), 100);
    assert_eq!(events.iter().filter(|e| e.as_str() == "coordinator:release:Clean").count(), 100);
    // Every next Open used the current local candidate, not an unnecessary
    // signed GET or a fallback to an older generation. This does NOT prove
    // live R2 object deletion or that a durable independent LKG exists.
    assert!(!events.iter().any(|e| {
        e.starts_with("download-capability:")
            || e.starts_with("get:")
            || e.starts_with("opening-material:")
    }));
    let observations = preflights.borrow();
    assert_eq!(observations.len(), 100);
    assert_eq!(observations.last().map(|o| o.1.as_slice()), Some(b"verified-sequential-save-098".as_slice()));
    Ok(())
}

fn run_canonical_saved_successor_reopen(
    remove_local_successor: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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
    let config_bytes = b"{}\n";
    fs::write(
        base_workspace.path().join("camoufox-config.json"),
        config_bytes,
    )?;
    let bundle = approved_bundle()?;
    let browser_identity = browser_identity_fixture(
        bundle.manifest().runtime_version(),
        bundle.manifest().inventory_sha256().as_str(),
        "profile-stability-v1-probe-p3-reopen-e2e",
        lower_hex(&Sha256::digest(config_bytes)),
    )?;
    let binding = MaterializationBinding::new(
        tenant_id.clone(),
        profile_id.clone(),
        base_generation.clone(),
        "c".repeat(64),
        base_workspace.materialization_inventory_digest()?,
        browser_identity,
    )?;
    persist_materialization_binding(&base_workspace, &binding)?;

    let state = Rc::new(RefCell::new(BackendState::new()?));
    let transport = BackendMachineHttp {
        state: Rc::clone(&state),
    };
    let preflight: PreflightObservations = Rc::new(RefCell::new(Vec::new()));
    let enrollment = ControlPlaneEnrollment::new(transport.clone());
    let coordinator = ControlPlaneCoordinator::new(transport.clone());
    let mut operator = ProfileBridgeOperator::new(
        FakeDeviceIdentity::new(device_id.clone()),
        FakeDeviceKeyStore::default(),
        DeviceAuthentication,
        enrollment,
        coordinator,
        TestRuntimeBundle { bundle },
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
    // Installed Bridge522 regression: the real next-launch readiness owner must accept
    // the saved candidate before any fake rematerialization can supply a binding.
    let accepted_workspace = root.open_generation(&tenant_id, &profile_id, &successor)?;
    let ready = load_materialization_binding(
        &accepted_workspace, &tenant_id, &profile_id, &successor,
    )?;
    assert_eq!(
        ready.source_container_sha256(),
        completion.committed().container_digest()
    );
    assert_eq!(
        ready.materialized_inventory_digest(),
        accepted_workspace.materialization_inventory_digest()?
    );
    {
        let backend = state.borrow();
        assert_eq!(backend.active_generation, successor);
        assert_eq!(backend.coordinator_status, CoordinatorStatusDto::Idle);
        assert!(backend.committed_successor.is_some());
    }

    if remove_local_successor {
        // Test fixture owns this disposable workspace. Production recovery never
        // deletes an unresolved candidate to manufacture a cold-reopen test.
        let disposable_candidate = root.open_generation(&tenant_id, &profile_id, &successor)?;
        fs::remove_dir_all(disposable_candidate.path())?;
        assert!(
            root.open_generation(&tenant_id, &profile_id, &successor)
                .is_err()
        );
    }

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
    assert!(commit < clean_release);
    assert!(clean_release < second_redeem);
    if remove_local_successor {
        let capability = event_index(&events, "download-capability:")?;
        let get = event_index(&events, "get:")?;
        let opening = event_index(&events, "opening-material:")?;
        assert!(second_redeem < capability);
        assert!(capability < get);
        assert!(get < opening);
    } else {
        // Normal same-device reopen must use the exact committed local candidate and
        // never consult the signed-download capability, immutable object GET, or DEK.
        assert!(!events.iter().any(|event| {
            event.starts_with("download-capability:")
                || event.starts_with("get:")
                || event.starts_with("opening-material:")
        }));
    }
    assert!(events.iter().all(|event| !event.contains("device-job")));

    operator.abort(UnixMillis::new(50))?;
    Ok(())
}
