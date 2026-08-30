use crate::dirty_generation::{GenerationSealingMaterial, GenerationSealingMaterialPort};
use crate::generation_reopen::{
    GenerationDownloadCapability, GenerationObjectDownloadPort, GenerationReopenControlPort,
    SignedGenerationObjectGetPort, SignedGenerationObjectGetResponse,
    VerifiedGenerationObjectDownloader,
};
use crate::local_profile::{GenerationWorkspace, MaterializationRoot};
use crate::operator_flow::{
    BrowserLaunchPreflightPort, DeviceAuthenticationPort, ProfileBridgeOperator,
    RuntimeBundleSelectionPort,
};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use crate::shipping_control_plane::{
    ControlPlaneEnrollment, MachineHttpMethod, MachineHttpPort, MachineHttpResponse,
};
use crate::shipping_generation_save::{
    SignedGenerationObjectPutPort, SignedGenerationUploadCapability,
};
use crate::{FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl, ProcessAction};
use application_ports::ProfileCoordinatorPort;
use bridge_domain::{BridgePortError, ClaimUri};
use control_plane_contract::profile_generation_api::{
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
use encrypted_generation_domain::{
    GenerationDek, KeyId, NoncePrefix, canonical_generation_object_key,
    inspect_generation_metadata_prelude,
};
use profile_platform_primitives::{
    ActorContext, CorrelationId, DeviceId, FencingToken, GenerationId, LaunchIntentId, ProfileId,
    SessionId, TenantId, UnixMillis,
};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use session_domain::ProfileLease;
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
const MUTATED_PREFS: &[u8] = b"p3-e2e-mutated-authoritative-successor";
const KEY_BYTES: [u8; 32] = [0x27; 32];

#[derive(Clone)]
struct BackendState {
    tenant_id: TenantId,
    profile_id: ProfileId,
    device_id: DeviceId,
    active_generation: Rc<RefCell<GenerationId>>,
    redemptions: Rc<Cell<u64>>,
}

impl BackendState {
    fn active_generation(&self) -> GenerationId {
        self.active_generation.borrow().clone()
    }

    fn activate(&self, generation_id: GenerationId) {
        *self.active_generation.borrow_mut() = generation_id;
    }
}

#[derive(Clone)]
struct EnrollmentHttp {
    backend: BackendState,
    initial_launch_intent: LaunchIntentId,
    reopen_launch_intent: LaunchIntentId,
}

impl MachineHttpPort for EnrollmentHttp {
    type Error = BridgePortError;

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        _correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        if method != MachineHttpMethod::PostJson
            || !path.ends_with("/bridge/v1/profile-launch/redeem")
            || body.is_none()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let redemption = self.backend.redemptions.get();
        let launch_intent = match redemption {
            0 => &self.initial_launch_intent,
            1 => &self.reopen_launch_intent,
            _ => return Err(BridgePortError::InvalidResponse),
        };
        self.backend.redemptions.set(redemption + 1);
        let response = serde_json::json!({
            "tenantId": self.backend.tenant_id.as_str(),
            "actorId": "actor_p3_authoritative_e2e",
            "profileId": self.backend.profile_id.as_str(),
            "generationId": self.backend.active_generation().as_str(),
            "deviceId": self.backend.device_id.as_str(),
            "launchIntentId": launch_intent.as_str(),
        });
        Ok(MachineHttpResponse::new(
            200,
            serde_json::to_vec(&response).map_err(|_| BridgePortError::InvalidResponse)?,
        ))
    }
}

#[derive(Clone, Debug)]
struct Authentication;

impl DeviceAuthenticationPort for Authentication {
    type Error = BridgePortError;

    fn authenticate(
        &mut self,
        _device_id: &DeviceId,
        _key_handle: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone)]
struct RuntimeBundles {
    backend: BackendState,
    bundle: ApprovedRuntimeBundle,
}

impl RuntimeBundleSelectionPort for RuntimeBundles {
    type Error = BridgePortError;

    fn select_bundle(
        &mut self,
        _actor: &ActorContext,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<ApprovedRuntimeBundle, Self::Error> {
        if profile_id != &self.backend.profile_id
            || generation_id != &self.backend.active_generation()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(self.bundle.clone())
    }
}

struct RecordingPreflight {
    expected_prefs: Option<Vec<u8>>,
    calls: Rc<Cell<u64>>,
}

impl BrowserLaunchPreflightPort for RecordingPreflight {
    type Error = BridgePortError;

    fn evaluate_before_launch(
        &mut self,
        workspace: &GenerationWorkspace,
        _device_id: &DeviceId,
        _workspace_epoch: u64,
        _runtime_bundle: &ApprovedRuntimeBundle,
    ) -> Result<(), Self::Error> {
        if let Some(expected) = &self.expected_prefs {
            let observed = fs::read(workspace.path().join("prefs.js"))
                .map_err(|_| BridgePortError::Unavailable)?;
            if &observed != expected {
                return Err(BridgePortError::InvalidResponse);
            }
        }
        self.calls.set(self.calls.get() + 1);
        Ok(())
    }
}

#[derive(Clone)]
struct SaveCoordinator {
    backend: BackendState,
    lease: ProfileLease,
    expected_launch_intent: LaunchIntentId,
    closed: Rc<Cell<u64>>,
}

impl ProfileCoordinatorPort for SaveCoordinator {
    type Error = BridgePortError;

    fn claim_launch_intent(
        &mut self,
        _actor: &ActorContext,
        profile_id: &ProfileId,
        device_id: &DeviceId,
        launch_intent_id: &LaunchIntentId,
    ) -> Result<ProfileLease, Self::Error> {
        if profile_id != &self.backend.profile_id
            || device_id != &self.backend.device_id
            || launch_intent_id != &self.expected_launch_intent
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(self.lease.clone())
    }

    fn close_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        if lease.session_id() != self.lease.session_id()
            || lease.epoch() != self.lease.epoch()
            || lease.fencing_token() != self.lease.fencing_token()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.closed.set(self.closed.get() + 1);
        Ok(())
    }
}

impl GenerationSealingMaterialPort for SaveCoordinator {
    type Error = BridgePortError;

    fn material_for(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        generation_id: &GenerationId,
        _plaintext_digest: [u8; 32],
    ) -> Result<GenerationSealingMaterial, Self::Error> {
        if tenant_id != &self.backend.tenant_id
            || profile_id != &self.backend.profile_id
            || base_generation_id != &self.backend.active_generation()
            || generation_id == base_generation_id
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(GenerationSealingMaterial::new(
            GenerationDek::new(
                KeyId::parse("profile-generation-root-v1-7")
                    .map_err(|_| BridgePortError::InvalidResponse)?,
                KEY_BYTES,
            ),
            NoncePrefix::new([0x71; 16]),
            4096,
        ))
    }
}

impl GenerationReopenControlPort for SaveCoordinator {
    type Error = BridgePortError;

    fn download_capability(
        &mut self,
        _tenant_id: &TenantId,
        _profile_id: &ProfileId,
    ) -> Result<GenerationDownloadCapability, Self::Error> {
        Err(BridgePortError::InvalidResponse)
    }

    fn opening_material(
        &mut self,
        _tenant_id: &TenantId,
        _profile_id: &ProfileId,
        _metadata_prelude: &[u8],
    ) -> Result<GenerationDek, Self::Error> {
        Err(BridgePortError::InvalidResponse)
    }
}

#[derive(Clone)]
struct ReopenAuthority {
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: [u8; 32],
    container_digest: [u8; 32],
    container_bytes: u64,
    key_id: KeyId,
}

#[derive(Clone)]
struct ReopenCoordinator {
    backend: BackendState,
    lease: ProfileLease,
    expected_launch_intent: LaunchIntentId,
    authority: ReopenAuthority,
    download_calls: Rc<Cell<u64>>,
    opening_calls: Rc<Cell<u64>>,
    closed: Rc<Cell<u64>>,
}

impl ProfileCoordinatorPort for ReopenCoordinator {
    type Error = BridgePortError;

    fn claim_launch_intent(
        &mut self,
        _actor: &ActorContext,
        profile_id: &ProfileId,
        device_id: &DeviceId,
        launch_intent_id: &LaunchIntentId,
    ) -> Result<ProfileLease, Self::Error> {
        if profile_id != &self.backend.profile_id
            || device_id != &self.backend.device_id
            || launch_intent_id != &self.expected_launch_intent
            || self.backend.active_generation() != self.authority.generation_id
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(self.lease.clone())
    }

    fn close_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        if lease.session_id() != self.lease.session_id()
            || lease.epoch() != self.lease.epoch()
            || lease.fencing_token() != self.lease.fencing_token()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.closed.set(self.closed.get() + 1);
        Ok(())
    }
}

impl GenerationReopenControlPort for ReopenCoordinator {
    type Error = BridgePortError;

    fn download_capability(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
    ) -> Result<GenerationDownloadCapability, Self::Error> {
        if tenant_id != &self.backend.tenant_id
            || profile_id != &self.backend.profile_id
            || self.backend.active_generation() != self.authority.generation_id
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.download_calls.set(self.download_calls.get() + 1);
        Ok(GenerationDownloadCapability::new(
            self.authority.generation_id.clone(),
            self.authority.object_key.clone(),
            self.authority.metadata_digest,
            self.authority.container_digest,
            self.authority.container_bytes,
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/object?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=test&X-Amz-Date=20260830T000000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=test".to_owned(),
            300,
        ))
    }

    fn opening_material(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        metadata_prelude: &[u8],
    ) -> Result<GenerationDek, Self::Error> {
        let inspected = inspect_generation_metadata_prelude(metadata_prelude)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let metadata = inspected.metadata();
        if tenant_id != &self.backend.tenant_id
            || profile_id != &self.backend.profile_id
            || metadata.tenant_id() != tenant_id
            || metadata.profile_id() != profile_id
            || metadata.generation_id() != &self.backend.active_generation()
            || metadata.generation_id() != &self.authority.generation_id
            || metadata.object_key() != self.authority.object_key
            || inspected.metadata_digest().bytes() != self.authority.metadata_digest
            || metadata.key_id() != &self.authority.key_id
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.opening_calls.set(self.opening_calls.get() + 1);
        Ok(GenerationDek::new(self.authority.key_id.clone(), KEY_BYTES))
    }
}

struct SaveTransport {
    backend: BackendState,
    phase: u8,
    candidate: Option<GenerationId>,
}

impl MachineHttpPort for SaveTransport {
    type Error = BridgePortError;

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        _correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        if method != MachineHttpMethod::PostJson {
            return Err(BridgePortError::InvalidResponse);
        }
        let request = serde_json::from_slice::<BridgeProfileGenerationSuccessorRequest>(
            body.ok_or(BridgePortError::InvalidResponse)?,
        )
        .map_err(|_| BridgePortError::InvalidResponse)?;
        let base = GenerationId::parse(request.base_generation_id().to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let candidate = GenerationId::parse(request.generation_id().to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if base != self.backend.active_generation() {
            return Err(BridgePortError::InvalidResponse);
        }

        if path.ends_with("/commit") {
            if self.phase != 2 || self.candidate.as_ref() != Some(&candidate) {
                return Err(BridgePortError::InvalidResponse);
            }
            self.backend.activate(candidate);
            self.phase = 3;
            return Ok(MachineHttpResponse::new(
                200,
                serde_json::to_vec(&BridgeGenerationSuccessorCommitResponse {
                    outcome: BridgeGenerationSuccessorCommitOutcomeDto::Activated,
                })
                .map_err(|_| BridgePortError::InvalidResponse)?,
            ));
        }

        match self.phase {
            0 => {
                self.candidate = Some(candidate);
                self.phase = 1;
                Ok(MachineHttpResponse::new(
                    200,
                    serde_json::to_vec(&BridgeGenerationUploadCapabilityResponse::upload_required(
                        "https://example.invalid/generation?signature=redacted",
                        &[("x-test".to_owned(), "secret".to_owned())],
                        300,
                    ))
                    .map_err(|_| BridgePortError::InvalidResponse)?,
                ))
            }
            1 if self.candidate.as_ref() == Some(&candidate) => {
                self.phase = 2;
                Ok(MachineHttpResponse::new(
                    200,
                    serde_json::to_vec(&BridgeGenerationUploadCapabilityResponse::verified())
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                ))
            }
            _ => Err(BridgePortError::InvalidResponse),
        }
    }
}

struct CapturingPut {
    container: Rc<RefCell<Option<Vec<u8>>>>,
    calls: Rc<Cell<u64>>,
}

impl SignedGenerationObjectPutPort for CapturingPut {
    type Error = BridgePortError;

    fn put_exact(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        container: &[u8],
    ) -> Result<(), Self::Error> {
        if capability
            .url()
            .map_err(|_| BridgePortError::InvalidResponse)?
            .is_empty()
            || container.is_empty()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.calls.set(self.calls.get() + 1);
        *self.container.borrow_mut() = Some(container.to_vec());
        Ok(())
    }
}

struct NeverDownloader {
    calls: Rc<Cell<u64>>,
}

impl GenerationObjectDownloadPort for NeverDownloader {
    type Error = BridgePortError;

    fn download_generation_object(
        &mut self,
        _capability: &GenerationDownloadCapability,
    ) -> Result<Vec<u8>, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Err(BridgePortError::InvalidResponse)
    }
}

struct ExactGet {
    container: Vec<u8>,
    calls: Rc<Cell<u64>>,
    observed_max: Rc<Cell<usize>>,
}

impl SignedGenerationObjectGetPort for ExactGet {
    type Error = BridgePortError;

    fn get_exact(
        &mut self,
        signed_url: &str,
        max_bytes: usize,
    ) -> Result<SignedGenerationObjectGetResponse, Self::Error> {
        if !signed_url.starts_with("https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/")
            || max_bytes != self.container.len()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        self.calls.set(self.calls.get() + 1);
        self.observed_max.set(max_bytes);
        Ok(SignedGenerationObjectGetResponse::new(
            200,
            self.container.clone(),
        ))
    }
}

struct Fixture {
    root_path: PathBuf,
    root: MaterializationRoot,
    backend: BackendState,
    base_generation: GenerationId,
    initial_claim: ClaimUri,
    reopen_claim: ClaimUri,
    initial_launch_intent: LaunchIntentId,
    reopen_launch_intent: LaunchIntentId,
    initial_lease: ProfileLease,
    reopen_lease: ProfileLease,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root_path = std::env::temp_dir().join(format!(
            "profile-bridge-p3-reopen-e2e-{}-{counter}",
            std::process::id()
        ));
        let root = MaterializationRoot::open_or_create(root_path.clone())?;
        let tenant_id = TenantId::parse(format!("tenant_p3_reopen_{counter}"))?;
        let profile_id = ProfileId::parse(format!("profile_p3_reopen_{counter}"))?;
        let device_id = DeviceId::parse(format!("device_p3_reopen_{counter}"))?;
        let base_generation = GenerationId::parse(format!("generation_p3_reopen_base_{counter}"))?;
        let workspace = root.create_generation(&tenant_id, &profile_id, &base_generation)?;
        fs::write(workspace.path().join("prefs.js"), b"p3-e2e-base")?;
        let backend = BackendState {
            tenant_id: tenant_id.clone(),
            profile_id: profile_id.clone(),
            device_id: device_id.clone(),
            active_generation: Rc::new(RefCell::new(base_generation.clone())),
            redemptions: Rc::new(Cell::new(0)),
        };
        let initial_launch_intent =
            LaunchIntentId::parse(format!("launch_p3_reopen_initial_{counter}"))?;
        let reopen_launch_intent =
            LaunchIntentId::parse(format!("launch_p3_reopen_second_{counter}"))?;
        let initial_lease = ProfileLease::issue(
            tenant_id.clone(),
            profile_id.clone(),
            SessionId::parse(format!("session_p3_reopen_initial_{counter}"))?,
            device_id.clone(),
            counter.max(1),
            FencingToken::parse(format!("fence_p3_reopen_initial_{counter}"))?,
        )?;
        let reopen_lease = ProfileLease::issue(
            tenant_id,
            profile_id,
            SessionId::parse(format!("session_p3_reopen_second_{counter}"))?,
            device_id,
            counter.saturating_add(100),
            FencingToken::parse(format!("fence_p3_reopen_second_{counter}"))?,
        )?;
        Ok(Self {
            root_path,
            root,
            backend,
            base_generation,
            initial_claim: ClaimUri::parse(&format!(
                "profilebridge://claim/claim_p3_reopen_initial_{counter:024}"
            ))?,
            reopen_claim: ClaimUri::parse(&format!(
                "profilebridge://claim/claim_p3_reopen_second_{counter:024}"
            ))?,
            initial_launch_intent,
            reopen_launch_intent,
            initial_lease,
            reopen_lease,
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root_path);
    }
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

#[test]
fn canonical_save_then_authoritative_reopen_rematerializes_committed_successor_before_runtime()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let first_preflight_calls = Rc::new(Cell::new(0));
    let first_closed = Rc::new(Cell::new(0));
    let enrollment_http = EnrollmentHttp {
        backend: fixture.backend.clone(),
        initial_launch_intent: fixture.initial_launch_intent.clone(),
        reopen_launch_intent: fixture.reopen_launch_intent.clone(),
    };
    let mut first = ProfileBridgeOperator::new(
        FakeDeviceIdentity::new(fixture.backend.device_id.clone()),
        FakeDeviceKeyStore::default(),
        Authentication,
        ControlPlaneEnrollment::new(enrollment_http.clone()),
        SaveCoordinator {
            backend: fixture.backend.clone(),
            lease: fixture.initial_lease.clone(),
            expected_launch_intent: fixture.initial_launch_intent.clone(),
            closed: Rc::clone(&first_closed),
        },
        RuntimeBundles {
            backend: fixture.backend.clone(),
            bundle: approved_bundle()?,
        },
        RecordingPreflight {
            expected_prefs: None,
            calls: Rc::clone(&first_preflight_calls),
        },
        FakeProcessControl::default(),
        FakeCamouhost::default(),
    );
    let first_download_calls = Rc::new(Cell::new(0));
    let mut no_download = NeverDownloader {
        calls: Rc::clone(&first_download_calls),
    };
    first.open_authoritative(
        &fixture.initial_claim,
        &fixture.root,
        &mut no_download,
        UnixMillis::new(10),
    )?;
    assert_eq!(first_download_calls.get(), 0);
    assert_eq!(first_preflight_calls.get(), 1);
    assert_eq!(fixture.backend.active_generation(), fixture.base_generation);

    let base_workspace = fixture.root.open_generation(
        &fixture.backend.tenant_id,
        &fixture.backend.profile_id,
        &fixture.base_generation,
    )?;
    fs::write(base_workspace.path().join("prefs.js"), MUTATED_PREFS)?;
    first.close(UnixMillis::new(20))?;
    assert_eq!(fixture.backend.active_generation(), fixture.base_generation);

    let captured = Rc::new(RefCell::new(None));
    let put_calls = Rc::new(Cell::new(0));
    let completion = first.save_retained_successor(
        &fixture.root,
        SaveTransport {
            backend: fixture.backend.clone(),
            phase: 0,
            candidate: None,
        },
        &mut CapturingPut {
            container: Rc::clone(&captured),
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(30),
    )?;
    assert!(completion.is_saved());
    assert_eq!(first_closed.get(), 1);
    assert_eq!(put_calls.get(), 1);
    let successor = completion.committed().generation_id().clone();
    assert_ne!(successor, fixture.base_generation);
    assert_eq!(fixture.backend.active_generation(), successor);

    let container = captured
        .borrow_mut()
        .take()
        .ok_or("successor container was not uploaded")?;
    let inspected = inspect_generation_metadata_prelude(&container)?;
    assert_eq!(inspected.metadata().generation_id(), &successor);
    assert_eq!(
        inspected.metadata().object_key(),
        canonical_generation_object_key(
            &fixture.backend.tenant_id,
            &fixture.backend.profile_id,
            &successor,
        )
    );
    let container_digest: [u8; 32] = Sha256::digest(&container).into();
    let authority = ReopenAuthority {
        generation_id: successor.clone(),
        object_key: inspected.metadata().object_key().to_owned(),
        metadata_digest: inspected.metadata_digest().bytes(),
        container_digest,
        container_bytes: u64::try_from(container.len())?,
        key_id: inspected.metadata().key_id().clone(),
    };

    let local_successor = fixture.root.open_generation(
        &fixture.backend.tenant_id,
        &fixture.backend.profile_id,
        &successor,
    )?;
    fs::remove_dir_all(local_successor.path())?;
    assert!(
        fixture
            .root
            .open_generation(
                &fixture.backend.tenant_id,
                &fixture.backend.profile_id,
                &successor,
            )
            .is_err()
    );

    let reopen_download_calls = Rc::new(Cell::new(0));
    let reopen_opening_calls = Rc::new(Cell::new(0));
    let reopen_closed = Rc::new(Cell::new(0));
    let second_preflight_calls = Rc::new(Cell::new(0));
    let mut second = ProfileBridgeOperator::new(
        FakeDeviceIdentity::new(fixture.backend.device_id.clone()),
        FakeDeviceKeyStore::default(),
        Authentication,
        ControlPlaneEnrollment::new(enrollment_http),
        ReopenCoordinator {
            backend: fixture.backend.clone(),
            lease: fixture.reopen_lease.clone(),
            expected_launch_intent: fixture.reopen_launch_intent.clone(),
            authority,
            download_calls: Rc::clone(&reopen_download_calls),
            opening_calls: Rc::clone(&reopen_opening_calls),
            closed: Rc::clone(&reopen_closed),
        },
        RuntimeBundles {
            backend: fixture.backend.clone(),
            bundle: approved_bundle()?,
        },
        RecordingPreflight {
            expected_prefs: Some(MUTATED_PREFS.to_vec()),
            calls: Rc::clone(&second_preflight_calls),
        },
        FakeProcessControl::default(),
        FakeCamouhost::default(),
    );
    let get_calls = Rc::new(Cell::new(0));
    let observed_max = Rc::new(Cell::new(0));
    let mut downloader = VerifiedGenerationObjectDownloader::new(ExactGet {
        container,
        calls: Rc::clone(&get_calls),
        observed_max: Rc::clone(&observed_max),
    });
    second.open_authoritative(
        &fixture.reopen_claim,
        &fixture.root,
        &mut downloader,
        UnixMillis::new(40),
    )?;

    assert_eq!(fixture.backend.active_generation(), successor);
    assert_eq!(reopen_download_calls.get(), 1);
    assert_eq!(reopen_opening_calls.get(), 1);
    assert_eq!(get_calls.get(), 1);
    assert!(observed_max.get() > 0);
    assert_eq!(second_preflight_calls.get(), 1);
    let rematerialized = fixture.root.open_generation(
        &fixture.backend.tenant_id,
        &fixture.backend.profile_id,
        &successor,
    )?;
    assert_eq!(fs::read(rematerialized.path().join("prefs.js"))?, MUTATED_PREFS);
    let active_session = second
        .active_session_id()
        .cloned()
        .ok_or("reopened runtime did not start")?;
    assert_eq!(
        second.process().actions(),
        [ProcessAction::Spawn(active_session.clone())]
    );

    let terminal = second.abort(UnixMillis::new(50))?;
    assert_eq!(terminal.generation_id(), &successor);
    assert_eq!(reopen_closed.get(), 1);
    Ok(())
}
