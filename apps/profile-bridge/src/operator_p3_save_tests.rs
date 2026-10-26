use crate::browser_execution::persist_browser_identity;
use crate::dirty_close::DirtyCloseLocalOutcome;
use crate::dirty_generation::{GenerationSealingMaterial, GenerationSealingMaterialPort};
use crate::local_profile::{
    BridgeWorkspaceLock, GenerationWorkspace, LocalGenerationState, LocalProfileError,
    MaterializationRoot,
};
use crate::operator_flow::{
    BrowserLaunchPreflightPort, DeviceAuthenticationPort, EnrollmentPort,
    GenerationSaveFailureReason, OperatorEnrollment, OperatorFailureStage, OperatorFlowError,
    ProfileBridgeOperator, RuntimeBundleSelectionPort,
};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort, MachineHttpResponse};
use crate::shipping_generation_save::{
    SignedGenerationObjectPutPort, SignedGenerationObjectStreamPutPort,
    SignedGenerationUploadCapability,
};
use crate::shipping_save_attempt::{
    SaveAttemptEvidenceError, discover_unresolved_exact_save_attempt,
    discover_unresolved_streaming_exact_save_attempt, has_unresolved_save_attempt_for_profile,
    inspect_exact_save_attempt, persist_local_unsynced_intent,
};
use crate::test_support::browser_identity_fixture;
use crate::{FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl};
use application_ports::ProfileCoordinatorPort;
use bridge_domain::{BridgePortError, ClaimCode, ClaimUri, EnrollmentClaim};
use control_plane_contract::profile_generation_api::{
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, GenerationId, LaunchIntentId,
    ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use session_domain::ProfileLease;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
struct FakeDeviceAuthentication;

impl DeviceAuthenticationPort for FakeDeviceAuthentication {
    type Error = BridgePortError;

    fn authenticate(
        &mut self,
        _device_id: &DeviceId,
        _key_handle: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct FakeEnrollment {
    claim: EnrollmentClaim,
    result: OperatorEnrollment,
}

impl EnrollmentPort for FakeEnrollment {
    type Error = BridgePortError;

    fn redeem_claim(
        &mut self,
        claim: &ClaimUri,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<OperatorEnrollment, Self::Error> {
        self.claim
            .redeem(claim.claim_code(), device_id, now)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(self.result.clone())
    }
}

#[derive(Clone, Debug)]
struct FakeCoordinator {
    lease: ProfileLease,
    expected_launch_intent_id: LaunchIntentId,
    claimed: u64,
    closed: u64,
    sealing_requests: u64,
    close_fail: bool,
}

impl ProfileCoordinatorPort for FakeCoordinator {
    type Error = BridgePortError;

    fn claim_launch_intent(
        &mut self,
        _actor: &ActorContext,
        _profile_id: &ProfileId,
        _device_id: &DeviceId,
        launch_intent_id: &LaunchIntentId,
    ) -> Result<ProfileLease, Self::Error> {
        self.claimed += 1;
        if launch_intent_id != &self.expected_launch_intent_id {
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
        self.closed += 1;
        if self.close_fail {
            Err(BridgePortError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl GenerationSealingMaterialPort for FakeCoordinator {
    type Error = BridgePortError;

    fn material_for(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        _generation_id: &GenerationId,
        _plaintext_digest: [u8; 32],
    ) -> Result<GenerationSealingMaterial, Self::Error> {
        self.sealing_requests += 1;
        if tenant_id != self.lease.tenant_id()
            || profile_id != self.lease.profile_id()
            || base_generation_id.as_str().is_empty()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(GenerationSealingMaterial::new(
            GenerationDek::new(
                KeyId::parse("profile-generation-root-v1-7")
                    .map_err(|_| BridgePortError::InvalidResponse)?,
                [7; 32],
            ),
            NoncePrefix::new([8; 16]),
            4096,
        ))
    }
}

#[derive(Clone, Debug)]
struct FakeRuntimeBundles {
    bundle: ApprovedRuntimeBundle,
}

impl RuntimeBundleSelectionPort for FakeRuntimeBundles {
    type Error = BridgePortError;

    fn select_bundle(&mut self) -> Result<ApprovedRuntimeBundle, Self::Error> {
        Ok(self.bundle.clone())
    }
}

#[derive(Clone, Debug)]
struct FakeBrowserPreflight;

impl BrowserLaunchPreflightPort for FakeBrowserPreflight {
    type Error = BridgePortError;

    fn evaluate_before_launch(
        &mut self,
        _workspace: &GenerationWorkspace,
        _device_id: &DeviceId,
        _workspace_epoch: u64,
        _runtime_bundle: &ApprovedRuntimeBundle,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

type TestOperator = ProfileBridgeOperator<
    FakeDeviceIdentity,
    FakeDeviceKeyStore,
    FakeDeviceAuthentication,
    FakeEnrollment,
    FakeCoordinator,
    FakeRuntimeBundles,
    FakeBrowserPreflight,
    FakeProcessControl,
    FakeCamouhost,
>;

type CommitHook =
    Rc<dyn Fn(&BridgeProfileGenerationSuccessorRequest) -> Result<(), BridgePortError>>;

#[derive(Clone, Default)]
struct TransportTrace {
    paths: Rc<RefCell<Vec<String>>>,
    generation_ids: Rc<RefCell<Vec<String>>>,
}

struct Transport {
    responses: VecDeque<Result<MachineHttpResponse, BridgePortError>>,
    trace: TransportTrace,
    commit_hook: Option<CommitHook>,
}

impl Transport {
    fn new(
        responses: impl IntoIterator<Item = Result<MachineHttpResponse, BridgePortError>>,
        trace: TransportTrace,
    ) -> Self {
        Self {
            responses: responses.into_iter().collect(),
            trace,
            commit_hook: None,
        }
    }

    fn with_commit_hook(mut self, hook: CommitHook) -> Self {
        self.commit_hook = Some(hook);
        self
    }
}

impl MachineHttpPort for Transport {
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
        self.trace.paths.borrow_mut().push(path.to_owned());
        self.trace
            .generation_ids
            .borrow_mut()
            .push(request.generation_id().to_owned());
        if path.ends_with("/commit")
            && let Some(hook) = self.commit_hook.as_ref()
        {
            hook(&request)?;
        }
        self.responses
            .pop_front()
            .ok_or(BridgePortError::Unavailable)?
    }
}

struct Put {
    fail: bool,
    calls: Rc<Cell<u64>>,
}

impl SignedGenerationObjectPutPort for Put {
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
        self.calls.set(self.calls.get().saturating_add(1));
        if self.fail {
            Err(BridgePortError::Unavailable)
        } else {
            Ok(())
        }
    }
}

/// The installed streaming path uses a typed retry-decision JSON request
/// before its ordinary signed PUT; fake responses never constitute D1 proof.
struct StreamTransport {
    responses: VecDeque<MachineHttpResponse>,
    paths: Rc<RefCell<Vec<String>>>,
}

impl MachineHttpPort for StreamTransport {
    type Error = BridgePortError;

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        _correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        if method != MachineHttpMethod::PostJson || body.is_none_or(<[u8]>::is_empty) {
            return Err(BridgePortError::InvalidResponse);
        }
        self.paths.borrow_mut().push(path.to_owned());
        self.responses
            .pop_front()
            .ok_or(BridgePortError::Unavailable)
    }
}

impl SignedGenerationObjectStreamPutPort for Put {
    fn put_verified_stream(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        file: &mut crate::generation_object_spool::VerifiedGenerationObjectFile,
    ) -> Result<(), Self::Error> {
        if capability
            .url()
            .map_err(|_| BridgePortError::InvalidResponse)?
            .is_empty()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        file.recheck_and_rewind()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        self.calls.set(self.calls.get().saturating_add(1));
        if self.fail {
            return Err(BridgePortError::Unavailable);
        }
        let expected = file.expected_bytes();
        let copied = std::io::copy(file.reader(), &mut std::io::sink())
            .map_err(|_| BridgePortError::Unavailable)?;
        if copied != expected {
            return Err(BridgePortError::InvalidResponse);
        }
        file.recheck_and_rewind()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(())
    }
}

struct Fixture {
    root_path: PathBuf,
    root: MaterializationRoot,
    claim_uri: ClaimUri,
    actor: ActorContext,
    profile_id: ProfileId,
    generation_id: GenerationId,
    device_id: DeviceId,
    launch_intent_id: LaunchIntentId,
    lease: ProfileLease,
}

impl Fixture {
    fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root_path = std::env::temp_dir().join(format!(
            "profile-bridge-p3-save-{label}-{}-{counter}",
            std::process::id()
        ));
        let root = MaterializationRoot::open_or_create(root_path.clone())?;
        let tenant_id = TenantId::parse(format!("tenant_p3_save_{counter}"))?;
        let profile_id = ProfileId::parse(format!("profile_p3_save_{counter}"))?;
        let generation_id = GenerationId::parse(format!("generation_p3_save_base_{counter}"))?;
        let device_id = DeviceId::parse(format!("device_p3_save_{counter}"))?;
        let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
        fs::write(workspace.path().join("prefs.js"), b"base")?;
        // This synthetic fixture must contain the same generation-owned identity that the
        // shipping first-open owner supplies before a real save/reopen. A fake preflight
        // alone is not evidence that a committed successor can be launched again.
        let bundle = approved_bundle()?;
        let identity = browser_identity_fixture(
            bundle.manifest().runtime_version(),
            bundle.manifest().inventory_sha256().as_str(),
            "profile-stability-v1-probe-p3-save",
            "f".repeat(64),
        )?;
        persist_browser_identity(&workspace, &identity)?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            ActorId::parse(format!("actor_p3_save_{counter}"))?,
            CorrelationId::parse(format!("corr_p3_save_{counter}"))?,
        );
        let lease = ProfileLease::issue(
            tenant_id,
            profile_id.clone(),
            SessionId::parse(format!("session_p3_save_{counter}"))?,
            device_id.clone(),
            counter.max(1),
            FencingToken::parse(format!("fence_p3_save_{counter}"))?,
        )?;
        let launch_intent_id = LaunchIntentId::parse(format!("launch_p3_save_{counter}"))?;
        let claim_uri = ClaimUri::parse(&format!(
            "profilebridge://claim/claim_p3_save_{counter:024}"
        ))?;
        let _ = ClaimCode::parse(format!("claim_p3_save_{counter:024}"))?;
        Ok(Self {
            root_path,
            root,
            claim_uri,
            actor,
            profile_id,
            generation_id,
            device_id,
            launch_intent_id,
            lease,
        })
    }

    fn enrollment(&self) -> Result<FakeEnrollment, Box<dyn std::error::Error>> {
        Ok(FakeEnrollment {
            claim: EnrollmentClaim::issue(
                self.claim_uri.claim_code().clone(),
                UnixMillis::new(1),
                UnixMillis::new(1_000),
            )?,
            result: OperatorEnrollment::new(
                self.actor.clone(),
                self.profile_id.clone(),
                self.generation_id.clone(),
                self.launch_intent_id.clone(),
            ),
        })
    }

    fn coordinator(&self) -> FakeCoordinator {
        FakeCoordinator {
            lease: self.lease.clone(),
            expected_launch_intent_id: self.launch_intent_id.clone(),
            claimed: 0,
            closed: 0,
            sealing_requests: 0,
            close_fail: false,
        }
    }

    fn operator(&self) -> Result<TestOperator, Box<dyn std::error::Error>> {
        self.operator_with_coordinator(self.coordinator())
    }

    fn operator_with_close_failure(&self) -> Result<TestOperator, Box<dyn std::error::Error>> {
        let mut coordinator = self.coordinator();
        coordinator.close_fail = true;
        self.operator_with_coordinator(coordinator)
    }

    fn operator_with_coordinator(
        &self,
        coordinator: FakeCoordinator,
    ) -> Result<TestOperator, Box<dyn std::error::Error>> {
        Ok(ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(self.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication,
            self.enrollment()?,
            coordinator,
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
            },
            FakeBrowserPreflight,
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        ))
    }

    fn open_mutate_and_close(
        &self,
        operator: &mut TestOperator,
    ) -> Result<(), Box<dyn std::error::Error>> {
        operator.open(&self.claim_uri, &self.root, UnixMillis::new(10))?;
        let workspace = self.root.open_generation(
            self.actor.tenant_scope().tenant_id(),
            &self.profile_id,
            &self.generation_id,
        )?;
        fs::write(workspace.path().join("prefs.js"), b"mutated-after-launch")?;
        operator.close(UnixMillis::new(20))?;
        Ok(())
    }

    fn base_workspace(&self) -> Result<GenerationWorkspace, Box<dyn std::error::Error>> {
        Ok(self.root.open_generation(
            self.actor.tenant_scope().tenant_id(),
            &self.profile_id,
            &self.generation_id,
        )?)
    }

    fn assert_base_lock_held(&self) -> Result<(), Box<dyn std::error::Error>> {
        let workspace = self.base_workspace()?;
        assert!(matches!(
            BridgeWorkspaceLock::acquire(&workspace, &self.device_id, self.lease.epoch()),
            Err(LocalProfileError::LockBusy)
        ));
        Ok(())
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

fn upload_required_response() -> Result<MachineHttpResponse, serde_json::Error> {
    let response = BridgeGenerationUploadCapabilityResponse::upload_required(
        "https://example.invalid/generation?signature=redacted",
        &[("x-test".to_owned(), "secret".to_owned())],
        300,
    );
    Ok(MachineHttpResponse::new(
        200,
        serde_json::to_vec(&response)?,
    ))
}

fn verified_response() -> Result<MachineHttpResponse, serde_json::Error> {
    Ok(MachineHttpResponse::new(
        200,
        serde_json::to_vec(&BridgeGenerationUploadCapabilityResponse::verified())?,
    ))
}

fn commit_response(
    outcome: BridgeGenerationSuccessorCommitOutcomeDto,
) -> Result<MachineHttpResponse, serde_json::Error> {
    Ok(MachineHttpResponse::new(
        200,
        serde_json::to_vec(&BridgeGenerationSuccessorCommitResponse { outcome })?,
    ))
}

#[test]
fn installed_streaming_close_commits_original_n_plus_one_and_retires_checkpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("stream-complete")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    persist_local_unsynced_intent(&fixture.root, &fixture.base_workspace()?, &fixture.lease)?;
    let requests = Rc::new(RefCell::new(Vec::new()));
    let puts = Rc::new(Cell::new(0));
    let completion = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::from([
                MachineHttpResponse::new(200, br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec()),
                upload_required_response()?,
                verified_response()?,
                commit_response(BridgeGenerationSuccessorCommitOutcomeDto::Activated)?,
            ]),
            paths: Rc::clone(&requests),
        },
        &mut Put {
            fail: false,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(30),
    )?;
    assert!(completion.is_saved());
    assert_eq!(puts.get(), 1);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert_eq!(operator.coordinator().closed, 1);
    assert_eq!(requests.borrow().len(), 4);
    assert!(requests.borrow()[0].ends_with("/generation-successor/retry-decision"));
    assert!(requests.borrow()[1].ends_with("/generation-successor/upload-capability"));
    assert!(requests.borrow()[2].ends_with("/generation-successor/upload-capability"));
    assert!(requests.borrow()[3].ends_with("/generation-successor/commit"));
    assert_eq!(
        fs::read(fixture.base_workspace()?.path().join("prefs.js"))?,
        b"mutated-after-launch"
    );
    let saved = fixture.root.open_generation(
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        completion.committed().generation_id(),
    )?;
    assert_eq!(
        fs::read(saved.path().join("prefs.js"))?,
        b"mutated-after-launch"
    );
    assert!(!has_unresolved_save_attempt_for_profile(
        &fixture.root,
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
    )?);
    assert!(!operator.has_pending_dirty_close());
    Ok(())
}

#[test]
fn installed_streaming_close_rejects_d1_block_without_put_or_native_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("stream-blocked")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    persist_local_unsynced_intent(&fixture.root, &fixture.base_workspace()?, &fixture.lease)?;
    let requests = Rc::new(RefCell::new(Vec::new()));
    let puts = Rc::new(Cell::new(0));
    let blocked = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"BLOCKED"}"#.to_vec(),
            )]),
            paths: Rc::clone(&requests),
        },
        &mut Put {
            fail: false,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        blocked,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::ControlRejected,
        })
    ));
    assert_eq!(requests.borrow().len(), 1);
    assert_eq!(puts.get(), 0);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert!(operator.has_pending_dirty_close());
    assert!(has_unresolved_save_attempt_for_profile(
        &fixture.root,
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
    )?);
    assert_eq!(
        fs::read(fixture.base_workspace()?.path().join("prefs.js"))?,
        b"mutated-after-launch"
    );
    fixture.assert_base_lock_held()?;
    Ok(())
}

#[test]
fn one_profile_completes_one_hundred_sequential_streaming_saves_without_n_plus_two()
-> Result<(), Box<dyn std::error::Error>> {
    // Same physical profile, 100 distinct browser-close sessions and 100
    // generations. Each session closes natively before its sole N+1 is sealed.
    // The D1/R2 transport is deliberately synthetic; this is composed Bridge
    // save-owner regression, NOT installed B7 or live-cloud acceptance.
    let mut fixture = Fixture::new("stream-100-sequential")?;
    let tenant = fixture.actor.tenant_scope().tenant_id().clone();
    let profile = fixture.profile_id.clone();
    let mut seen = std::collections::HashSet::new();
    seen.insert(fixture.generation_id.clone());
    for sequence in 0_u64..100 {
        let mut operator = fixture.operator()?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        let workspace = fixture.base_workspace()?;
        let native_content = format!("native-firefox-save-{sequence:03}");
        fs::write(workspace.path().join("prefs.js"), native_content.as_bytes())?;
        operator.close(UnixMillis::new(20))?;
        persist_local_unsynced_intent(&fixture.root, &workspace, &fixture.lease)?;

        let requests = Rc::new(RefCell::new(Vec::new()));
        let puts = Rc::new(Cell::new(0));
        let completed = operator.save_retained_successor_streaming(
            &fixture.root,
            StreamTransport {
                responses: VecDeque::from([
                    MachineHttpResponse::new(
                        200,
                        br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                    ),
                    upload_required_response()?,
                    verified_response()?,
                    commit_response(BridgeGenerationSuccessorCommitOutcomeDto::Activated)?,
                ]),
                paths: Rc::clone(&requests),
            },
            &mut Put {
                fail: false,
                calls: Rc::clone(&puts),
            },
            UnixMillis::new(30),
        )?;
        assert!(completed.is_saved());
        assert_eq!(puts.get(), 1, "one immutable PUT per completed generation");
        assert_eq!(requests.borrow().len(), 4);
        assert_eq!(operator.coordinator().sealing_requests, 1);
        assert_eq!(operator.coordinator().closed, 1);
        assert!(!operator.has_pending_dirty_close());
        assert!(!has_unresolved_save_attempt_for_profile(
            &fixture.root,
            &tenant,
            &profile,
        )?);
        let successor = completed.committed().generation_id().clone();
        assert!(seen.insert(successor.clone()), "no repeated N+1 or silent N+2");
        let saved = fixture.root.open_generation(&tenant, &profile, &successor)?;
        assert_eq!(fs::read(saved.path().join("prefs.js"))?, native_content.as_bytes());
        assert_eq!(fs::read(workspace.path().join("prefs.js"))?, native_content.as_bytes());

        fixture.generation_id = successor;
        let epoch = fixture.lease.epoch() + 1;
        fixture.lease = ProfileLease::issue(
            tenant.clone(),
            profile.clone(),
            SessionId::parse(format!("session_p3_seq_{sequence:03}"))?,
            fixture.device_id.clone(),
            epoch,
            FencingToken::parse(format!("fence_p3_seq_{sequence:03}"))?,
        )?;
        fixture.launch_intent_id = LaunchIntentId::parse(format!("launch_p3_seq_{sequence:03}"))?;
        fixture.claim_uri =
            ClaimUri::parse(&format!("profilebridge://claim/claim_p3_seq_{sequence:024}"))?;
    }
    assert_eq!(seen.len(), 101);
    assert!(fixture.root.open_generation(&tenant, &profile, &fixture.generation_id).is_ok());
    Ok(())
}

#[test]
fn one_hundred_isolated_profiles_complete_one_streaming_save_each_without_cross_scope()
-> Result<(), Box<dyn std::error::Error>> {
    // Cross-profile load exercise: 100 independent profiles / 100 ordinary
    // controlled-close saves. This is NOT 100 successive generations of one
    // profile or independent cloud WORKING/LKG acceptance.
    let mut fixtures = Vec::with_capacity(100);
    let mut scopes = std::collections::HashSet::new();
    for _ in 0..100 {
        let fixture = Fixture::new("stream-100-profiles")?;
        let tenant = fixture.actor.tenant_scope().tenant_id();
        let scope = (
            tenant.as_str().to_owned(),
            fixture.profile_id.as_str().to_owned(),
        );
        assert!(scopes.insert(scope));
        let mut operator = fixture.operator()?;
        fixture.open_mutate_and_close(&mut operator)?;
        persist_local_unsynced_intent(&fixture.root, &fixture.base_workspace()?, &fixture.lease)?;
        let paths = Rc::new(RefCell::new(Vec::new()));
        let puts = Rc::new(Cell::new(0));
        let completion = operator.save_retained_successor_streaming(
            &fixture.root,
            StreamTransport {
                responses: VecDeque::from([
                    MachineHttpResponse::new(
                        200,
                        br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                    ),
                    upload_required_response()?,
                    verified_response()?,
                    commit_response(BridgeGenerationSuccessorCommitOutcomeDto::Activated)?,
                ]),
                paths: Rc::clone(&paths),
            },
            &mut Put {
                fail: false,
                calls: Rc::clone(&puts),
            },
            UnixMillis::new(30),
        )?;
        assert!(completion.is_saved());
        assert_eq!(operator.coordinator().sealing_requests, 1);
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(puts.get(), 1);
        assert_eq!(paths.borrow().len(), 4);
        let saved_generation = completion.committed().generation_id().clone();
        let saved = fixture
            .root
            .open_generation(tenant, &fixture.profile_id, &saved_generation)?;
        assert_eq!(
            fs::read(saved.path().join("prefs.js"))?,
            b"mutated-after-launch",
        );
        assert!(!has_unresolved_save_attempt_for_profile(
            &fixture.root,
            tenant,
            &fixture.profile_id,
        )?);
        assert!(!operator.has_pending_dirty_close());
        fixtures.push((fixture, saved_generation));
    }
    assert_eq!(scopes.len(), 100);
    // Retain all roots throughout the loop: earlier generations cannot be
    // mutated by later independent saves or polluted with another scope's BPAT.
    for (fixture, generation) in &fixtures {
        let workspace = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            generation,
        )?;
        assert_eq!(
            fs::read(workspace.path().join("prefs.js"))?,
            b"mutated-after-launch",
        );
        assert!(!has_unresolved_save_attempt_for_profile(
            &fixture.root,
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
        )?);
    }
    Ok(())
}

#[test]
fn streaming_retry_after_failed_put_preserves_exact_n_plus_one()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("stream-put-retry")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    persist_local_unsynced_intent(&fixture.root, &fixture.base_workspace()?, &fixture.lease)?;
    let trace = Rc::new(RefCell::new(Vec::new()));
    let puts = Rc::new(Cell::new(0));
    let first = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::from([
                MachineHttpResponse::new(200, br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec()),
                upload_required_response()?,
            ]),
            paths: Rc::clone(&trace),
        },
        &mut Put {
            fail: true,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        first,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::UploadOutcomeUncertain,
        })
    ));
    let tenant = fixture.actor.tenant_scope().tenant_id();
    let original = discover_unresolved_streaming_exact_save_attempt(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?
    .ok_or("streaming BPAT must survive failed signed PUT")?;
    let successor = original.successor_id().to_owned();
    let digest = original.container_digest().to_owned();
    drop(original);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert_eq!(puts.get(), 1);
    assert!(operator.has_pending_dirty_close());
    fixture.assert_base_lock_held()?;

    let finished = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::from([
                MachineHttpResponse::new(200, br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec()),
                upload_required_response()?,
                verified_response()?,
                commit_response(BridgeGenerationSuccessorCommitOutcomeDto::Activated)?,
            ]),
            paths: Rc::clone(&trace),
        },
        &mut Put {
            fail: false,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(40),
    )?;
    assert!(finished.is_saved());
    assert_eq!(finished.committed().generation_id().as_str(), successor);
    assert_eq!(finished.committed().container_digest(), digest);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert_eq!(puts.get(), 2);
    assert_eq!(operator.coordinator().closed, 1);
    assert!(!operator.has_pending_dirty_close());
    assert!(!has_unresolved_save_attempt_for_profile(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?);
    Ok(())
}

#[test]
fn streaming_lost_commit_response_never_invents_another_successor_or_blind_put()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("stream-commit-unknown")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    persist_local_unsynced_intent(&fixture.root, &fixture.base_workspace()?, &fixture.lease)?;
    let paths = Rc::new(RefCell::new(Vec::new()));
    let puts = Rc::new(Cell::new(0));
    // R2 accepted and the server independently verified the exact object,
    // but the commit POST response was lost. It may already be ACTIVE_EXACT.
    let first = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::from([
                MachineHttpResponse::new(200, br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec()),
                upload_required_response()?,
                verified_response()?,
            ]),
            paths: Rc::clone(&paths),
        },
        &mut Put {
            fail: false,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        first,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::ControlTransport,
        })
    ));
    assert_eq!(puts.get(), 1);
    assert_eq!(paths.borrow().len(), 4);
    let tenant = fixture.actor.tenant_scope().tenant_id();
    let checkpoint = discover_unresolved_streaming_exact_save_attempt(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?
    .ok_or("original streamed attempt is missing after unknown commit")?;
    let successor = checkpoint.successor_id().to_owned();
    let digest = checkpoint.container_digest().to_owned();
    drop(checkpoint);
    assert!(operator.has_pending_dirty_close());
    fixture.assert_base_lock_held()?;
    assert_eq!(operator.coordinator().sealing_requests, 1);

    // A blocked fresh server nonapplication decision never licenses another
    // signed PUT, new seal, local retirement or claim that N+1 was unsaved.
    let second_paths = Rc::new(RefCell::new(Vec::new()));
    let second = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"BLOCKED"}"#.to_vec(),
            )]),
            paths: Rc::clone(&second_paths),
        },
        &mut Put {
            fail: false,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(40),
    );
    assert!(matches!(
        second,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::ControlRejected,
        })
    ));
    assert_eq!(second_paths.borrow().len(), 1);
    assert_eq!(puts.get(), 1);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    let original = discover_unresolved_streaming_exact_save_attempt(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?
    .ok_or("original N+1 must survive a blocked lost-commit retry")?;
    assert_eq!(original.successor_id(), successor);
    assert_eq!(original.container_digest(), digest);
    assert!(operator.has_pending_dirty_close());
    fixture.assert_base_lock_held()?;
    assert_eq!(
        fs::read(fixture.base_workspace()?.path().join("prefs.js"))?,
        b"mutated-after-launch"
    );
    Ok(())
}

#[test]
fn streaming_retry_rejects_changed_native_bytes_without_network_or_n_plus_two()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("stream-mutated-retry")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    persist_local_unsynced_intent(&fixture.root, &fixture.base_workspace()?, &fixture.lease)?;
    let puts = Rc::new(Cell::new(0));
    let first = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::from([
                MachineHttpResponse::new(200, br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec()),
                upload_required_response()?,
            ]),
            paths: Rc::new(RefCell::new(Vec::new())),
        },
        &mut Put {
            fail: true,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(30),
    );
    assert!(first.is_err());
    let tenant = fixture.actor.tenant_scope().tenant_id();
    let original = discover_unresolved_streaming_exact_save_attempt(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?
    .ok_or("original N+1 missing after uncertain PUT")?;
    let candidate = fixture.root.open_generation(
        tenant,
        &fixture.profile_id,
        &GenerationId::parse(original.successor_id())?,
    )?;
    let originally_sealed = original.successor_id().to_owned();
    drop(original);
    fs::write(
        candidate.path().join("prefs.js"),
        b"local unknown changed after failed PUT",
    )?;
    let observed = Rc::new(RefCell::new(Vec::new()));
    let second = operator.save_retained_successor_streaming(
        &fixture.root,
        StreamTransport {
            responses: VecDeque::new(),
            paths: Rc::clone(&observed),
        },
        &mut Put {
            fail: false,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(40),
    );
    assert!(second.is_err());
    assert!(
        observed.borrow().is_empty(),
        "changed native files must block before D1/R2"
    );
    assert_eq!(puts.get(), 1);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert!(operator.has_pending_dirty_close());
    fixture.assert_base_lock_held()?;
    assert_eq!(
        fs::read(candidate.path().join("prefs.js"))?,
        b"local unknown changed after failed PUT"
    );
    assert!(has_unresolved_save_attempt_for_profile(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?);
    assert!(!originally_sealed.is_empty());
    Ok(())
}

#[test]
fn canonical_operator_save_retries_same_successor_after_upload_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("retry")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    assert_eq!(
        operator.pending_dirty_local_state(),
        Some(LocalGenerationState::DirtyLocal)
    );
    assert_eq!(operator.coordinator().closed, 0);
    fixture.assert_base_lock_held()?;

    let trace = TransportTrace::default();
    let put_calls = Rc::new(Cell::new(0));
    let first = operator.save_retained_successor(
        &fixture.root,
        Transport::new([Ok(upload_required_response()?)], trace.clone()),
        &mut Put {
            fail: true,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        first,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::UploadOutcomeUncertain,
        })
    ));
    assert!(operator.has_pending_dirty_close());
    assert_eq!(
        operator.pending_dirty_local_state(),
        Some(LocalGenerationState::DirtyLocal)
    );
    assert_eq!(operator.coordinator().closed, 0);
    assert_eq!(put_calls.get(), 1);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert_eq!(trace.paths.borrow().len(), 1);
    assert!(!trace.paths.borrow()[0].ends_with("/commit"));
    fixture.assert_base_lock_held()?;

    let candidate = GenerationId::parse(
        trace
            .generation_ids
            .borrow()
            .first()
            .ok_or("missing first candidate")?
            .clone(),
    )?;
    let stale_candidate = fixture.root.open_generation(
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        &candidate,
    )?;
    let original_inventory = stale_candidate.inventory()?;
    let original_path = stale_candidate.path().to_path_buf();

    let completion = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(upload_required_response()?),
                Ok(verified_response()?),
                Ok(commit_response(
                    BridgeGenerationSuccessorCommitOutcomeDto::Activated,
                )?),
            ],
            trace.clone(),
        ),
        &mut Put {
            fail: false,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(40),
    )?;
    assert!(completion.is_saved());
    // Full commit + local readiness + releases retire only the exact
    // ciphertext checkpoint, preventing growth on repeated ordinary saves.
    assert!(
        inspect_exact_save_attempt(
            &fixture.root,
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &candidate,
        )?
        .is_none()
    );
    assert!(matches!(
        completion.local().local_outcome(),
        DirtyCloseLocalOutcome::CandidateAccepted(record)
            if record.generation_id() == &candidate
    ));
    assert!(!operator.has_pending_dirty_close());
    assert_eq!(operator.coordinator().closed, 1);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert_eq!(put_calls.get(), 2);
    assert!(
        trace
            .generation_ids
            .borrow()
            .iter()
            .all(|generation_id| generation_id == candidate.as_str())
    );
    let retained_candidate = fixture.root.open_generation(
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        &candidate,
    )?;
    assert_eq!(retained_candidate.path(), original_path);
    assert_eq!(retained_candidate.inventory()?, original_inventory);

    let base_workspace = fixture.base_workspace()?;
    let released =
        BridgeWorkspaceLock::acquire(&base_workspace, &fixture.device_id, fixture.lease.epoch())?;
    released.release()?;

    let requests_before_duplicate = trace.paths.borrow().len();
    let puts_before_duplicate = put_calls.get();
    let duplicate = operator.save_retained_successor(
        &fixture.root,
        Transport::new([], trace.clone()),
        &mut Put {
            fail: false,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(41),
    );
    assert!(matches!(
        duplicate,
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::GenerationSave
        ))
    ));
    assert_eq!(trace.paths.borrow().len(), requests_before_duplicate);
    assert_eq!(put_calls.get(), puts_before_duplicate);
    Ok(())
}

#[test]
fn canonical_save_checkpoint_survives_owner_process_loss_without_authorizing_publish()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("checkpoint-restart")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let put_calls = Rc::new(Cell::new(0));
    let first = operator.save_retained_successor(
        &fixture.root,
        Transport::new([Ok(upload_required_response()?)], trace.clone()),
        &mut Put {
            fail: true,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        first,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::UploadOutcomeUncertain,
        })
    ));
    let candidate = GenerationId::parse(
        trace
            .generation_ids
            .borrow()
            .first()
            .ok_or("missing attempted generation")?
            .clone(),
    )?;
    let before = inspect_exact_save_attempt(
        &fixture.root,
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        &candidate,
    )?
    .ok_or("missing durable save attempt")?;
    let expected_hash = before.container_digest().to_owned();
    let expected_ciphertext = before.ciphertext().to_vec();
    assert!(!expected_ciphertext.is_empty());
    assert!(before.matches_original_lease(&fixture.lease));
    drop(operator);

    // Fresh root handle, no RetainedDirtyClose or in-memory sealed generation.
    // Inspection is local/read-only and must not perform HTTP PUT or commit.
    let reopened_root = MaterializationRoot::open_or_create(&fixture.root_path)?;
    let recovered = inspect_exact_save_attempt(
        &reopened_root,
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        &candidate,
    )?
    .ok_or("the exact attempt did not survive process loss")?;
    assert_eq!(recovered.ciphertext(), expected_ciphertext);
    assert_eq!(recovered.container_digest(), expected_hash);
    // After total process loss, discover the successor without an in-memory
    // lease or a caller-supplied generation ID; do not send any HTTP effect.
    let discovered = discover_unresolved_exact_save_attempt(
        &reopened_root,
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
    )?
    .ok_or("crash-surviving exact successor not discovered")?;
    assert_eq!(discovered.successor_id(), candidate.as_str());
    assert_eq!(discovered.container_digest(), expected_hash);
    assert_eq!(discovered.ciphertext(), expected_ciphertext);
    assert!(discovered.matches_original_lease(&fixture.lease));
    assert_eq!(put_calls.get(), 1);
    assert_eq!(trace.paths.borrow().len(), 1);
    assert!(
        reopened_root
            .open_generation(
                fixture.actor.tenant_scope().tenant_id(),
                &fixture.profile_id,
                &candidate,
            )
            .is_ok()
    );
    Ok(())
}

#[test]
fn crashed_save_checkpoint_blocks_fresh_launch_only_for_affected_profile()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("crashed-save-fresh-open")?;
    let mut owner = fixture.operator()?;
    fixture.open_mutate_and_close(&mut owner)?;
    let trace = TransportTrace::default();
    let puts = Rc::new(Cell::new(0));
    let failed = owner.save_retained_successor(
        &fixture.root,
        Transport::new([Ok(upload_required_response()?)], trace.clone()),
        &mut Put {
            fail: true,
            calls: Rc::clone(&puts),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        failed,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::UploadOutcomeUncertain,
        })
    ));
    let tenant = fixture.actor.tenant_scope().tenant_id();
    assert!(has_unresolved_save_attempt_for_profile(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?);
    assert!(!has_unresolved_save_attempt_for_profile(
        &fixture.root,
        tenant,
        &ProfileId::parse("profile_unrelated_crash_guard")?,
    )?);
    drop(owner);

    // Even after losing the exact in-process lease and prepared data, a new
    // operator must not open the stale predecessor as a Clean browser.
    let mut fresh = fixture.operator()?;
    let result = fresh.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(40));
    assert!(matches!(
        result,
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::GenerationSave
        ))
    ));
    assert_eq!(fresh.coordinator().claimed, 1);
    assert_eq!(fresh.coordinator().closed, 1);
    assert_eq!(fresh.active_local_state(), None);
    assert_eq!(puts.get(), 1);
    assert_eq!(trace.paths.borrow().len(), 1);
    assert!(has_unresolved_save_attempt_for_profile(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?);
    Ok(())
}

#[test]
fn torn_scoped_checkpoint_filename_still_blocks_stale_browser_reopen()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("torn-checkpoint-name")?;
    let mut owner = fixture.operator()?;
    fixture.open_mutate_and_close(&mut owner)?;
    let first = owner.save_retained_successor(
        &fixture.root,
        Transport::new([Ok(upload_required_response()?)], TransportTrace::default()),
        &mut Put {
            fail: true,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    );
    assert!(first.is_err());
    let checkpoint = fs::read_dir(fixture.root.path())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(".profile-save-attempt-v1-") && name.ends_with(".bin")
                })
        })
        .ok_or("missing attempted encrypted checkpoint")?;
    let malformed_name = checkpoint.with_extension("bin.torn");
    fs::rename(&checkpoint, &malformed_name)?;
    drop(owner);
    assert!(has_unresolved_save_attempt_for_profile(
        &fixture.root,
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
    )?);

    let mut fresh = fixture.operator()?;
    assert!(matches!(
        fresh.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(40)),
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::GenerationSave
        ))
    ));
    assert!(malformed_name.exists());
    assert!(matches!(
        discover_unresolved_exact_save_attempt(
            &fixture.root,
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
        ),
        Err(SaveAttemptEvidenceError::Invalid)
    ));
    assert_eq!(fresh.coordinator().closed, 1);
    Ok(())
}

#[test]
fn profile_discovery_rejects_pending_sibling_without_erasing_exact_checkpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("checkpoint-pending-sibling")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let transport = TransportTrace::default();
    let failed = operator.save_retained_successor(
        &fixture.root,
        Transport::new([Ok(upload_required_response()?)], transport.clone()),
        &mut Put {
            fail: true,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    );
    assert!(failed.is_err());
    let tenant = fixture.actor.tenant_scope().tenant_id();
    let one = discover_unresolved_exact_save_attempt(&fixture.root, tenant, &fixture.profile_id)?
        .ok_or("expected valid checkpoint before introducing competing evidence")?;
    let candidate = GenerationId::parse(one.successor_id().to_owned())?;
    let canonical = fs::read_dir(fixture.root.path())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(".profile-save-attempt-v1-") && name.ends_with(".bin")
                })
        })
        .ok_or("missing scoped checkpoint")?;
    // Torn in-flight stage, even alongside a valid canonical record, cannot
    // be silently ignored, replaced, removed or mistaken for fresh authority.
    let pending = canonical.with_extension("bin.pending-unknown");
    fs::write(&pending, b"torn-stage")?;
    assert!(matches!(
        discover_unresolved_exact_save_attempt(&fixture.root, tenant, &fixture.profile_id),
        Err(SaveAttemptEvidenceError::Invalid)
    ));
    assert!(has_unresolved_save_attempt_for_profile(
        &fixture.root,
        tenant,
        &fixture.profile_id,
    )?);
    assert!(pending.exists());
    assert!(
        inspect_exact_save_attempt(&fixture.root, tenant, &fixture.profile_id, &candidate)?
            .is_some()
    );
    assert_eq!(transport.paths.borrow().len(), 1);
    Ok(())
}

#[test]
fn canonical_save_refuses_tampered_checkpoint_before_any_retry_network_effect()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("checkpoint-tampered")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let put_calls = Rc::new(Cell::new(0));
    let first = operator.save_retained_successor(
        &fixture.root,
        Transport::new([Ok(upload_required_response()?)], trace.clone()),
        &mut Put {
            fail: true,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        first,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::UploadOutcomeUncertain,
        })
    ));
    let candidate = GenerationId::parse(
        trace
            .generation_ids
            .borrow()
            .first()
            .ok_or("missing attempted generation")?
            .clone(),
    )?;
    let checkpoint = fs::read_dir(fixture.root.path())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(".profile-save-attempt-v1-") && name.ends_with(".bin")
                })
        })
        .ok_or("checkpoint not published")?;
    let mut original = fs::read(&checkpoint)?;
    let final_byte = original.last_mut().ok_or("empty checkpoint")?;
    *final_byte ^= 1;
    fs::write(&checkpoint, &original)?;

    assert!(matches!(
        inspect_exact_save_attempt(
            &fixture.root,
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &candidate,
        ),
        Err(SaveAttemptEvidenceError::Invalid)
    ));
    let calls_before = trace.paths.borrow().len();
    let retry = operator.save_retained_successor(
        &fixture.root,
        Transport::new([], trace.clone()),
        &mut Put {
            fail: false,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(40),
    );
    assert!(matches!(
        retry,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::CheckpointIntegrity,
        })
    ));
    assert_eq!(trace.paths.borrow().len(), calls_before);
    assert_eq!(put_calls.get(), 1);
    assert_eq!(operator.coordinator().closed, 0);
    assert!(operator.has_pending_dirty_close());
    fixture.assert_base_lock_held()?;
    assert!(checkpoint.exists());
    Ok(())
}

#[test]
fn canonical_operator_retry_refuses_changed_candidate_and_preserves_unknown_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("retry-mutated")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let put_calls = Rc::new(Cell::new(0));
    let first = operator.save_retained_successor(
        &fixture.root,
        Transport::new([Ok(upload_required_response()?)], trace.clone()),
        &mut Put {
            fail: true,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        first,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::UploadOutcomeUncertain,
        })
    ));
    let candidate = GenerationId::parse(
        trace
            .generation_ids
            .borrow()
            .first()
            .ok_or("missing candidate")?
            .clone(),
    )?;
    let workspace = fixture.root.open_generation(
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        &candidate,
    )?;
    let unknown = workspace.path().join("user_data");
    fs::create_dir(&unknown)?;
    fs::write(
        unknown.join("cookies.sqlite-wal"),
        b"unaccounted-local-writes",
    )?;
    let observed_requests = trace.paths.borrow().len();

    let retry = operator.save_retained_successor(
        &fixture.root,
        Transport::new([], trace.clone()),
        &mut Put {
            fail: false,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(40),
    );
    assert!(matches!(
        retry,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::NativeSourceChanged,
        })
    ));
    assert_eq!(trace.paths.borrow().len(), observed_requests);
    assert_eq!(put_calls.get(), 1);
    assert_eq!(
        fs::read(unknown.join("cookies.sqlite-wal"))?,
        b"unaccounted-local-writes"
    );
    assert!(operator.has_pending_dirty_close());
    assert_eq!(operator.coordinator().closed, 0);
    fixture.assert_base_lock_held()?;
    Ok(())
}

#[test]
fn canonical_operator_lost_commit_response_reuses_verified_original_without_second_put()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("commit-lost-then-reconciled")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let put_calls = Rc::new(Cell::new(0));
    let first = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(upload_required_response()?),
                Ok(verified_response()?),
                Err(BridgePortError::Unavailable),
            ],
            trace.clone(),
        ),
        &mut Put {
            fail: false,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        first,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::ControlTransport,
        })
    ));
    assert_eq!(put_calls.get(), 1);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    let candidate = trace
        .generation_ids
        .borrow()
        .first()
        .ok_or("missing candidate")?
        .clone();
    let completion = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(verified_response()?),
                Ok(commit_response(
                    BridgeGenerationSuccessorCommitOutcomeDto::AlreadyActive,
                )?),
            ],
            trace.clone(),
        ),
        &mut Put {
            fail: false,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(40),
    )?;
    assert!(completion.is_saved());
    assert_eq!(put_calls.get(), 1);
    assert_eq!(operator.coordinator().sealing_requests, 1);
    assert!(
        trace
            .generation_ids
            .borrow()
            .iter()
            .all(|id| id == &candidate)
    );
    assert_eq!(completion.committed().generation_id().as_str(), candidate);
    assert!(!operator.has_pending_dirty_close());
    assert_eq!(operator.coordinator().closed, 1);
    Ok(())
}

#[test]
fn canonical_operator_never_verified_cannot_commit_or_release()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("never-verified")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let put_calls = Rc::new(Cell::new(0));
    let result = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(upload_required_response()?),
                Ok(upload_required_response()?),
            ],
            trace.clone(),
        ),
        &mut Put {
            fail: false,
            calls: put_calls,
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        result,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::VerificationUnproven,
        })
    ));
    assert!(operator.has_pending_dirty_close());
    assert_eq!(operator.coordinator().closed, 0);
    assert!(
        trace
            .paths
            .borrow()
            .iter()
            .all(|path| !path.ends_with("/commit"))
    );
    fixture.assert_base_lock_held()?;
    Ok(())
}

#[test]
fn canonical_operator_commit_transport_failure_retains_precommit_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("commit-transport")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let result = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(upload_required_response()?),
                Ok(verified_response()?),
                Err(BridgePortError::Unavailable),
            ],
            trace.clone(),
        ),
        &mut Put {
            fail: false,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        result,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::ControlTransport,
        })
    ));
    assert!(operator.has_pending_dirty_close());
    assert_eq!(operator.coordinator().closed, 0);
    assert_eq!(
        trace
            .paths
            .borrow()
            .iter()
            .filter(|path| path.ends_with("/commit"))
            .count(),
        1
    );
    fixture.assert_base_lock_held()?;
    Ok(())
}

#[test]
fn canonical_operator_malformed_commit_response_retains_precommit_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("malformed-commit")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let result = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(upload_required_response()?),
                Ok(verified_response()?),
                Ok(MachineHttpResponse::new(200, b"{}".to_vec())),
            ],
            trace,
        ),
        &mut Put {
            fail: false,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    );
    assert!(matches!(
        result,
        Err(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::ControlInvalidResponse,
        })
    ));
    assert!(operator.has_pending_dirty_close());
    assert_eq!(operator.coordinator().closed, 0);
    fixture.assert_base_lock_held()?;
    Ok(())
}

#[test]
fn canonical_operator_already_active_replay_is_terminal_same_successor()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("already-active")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let trace = TransportTrace::default();
    let put_calls = Rc::new(Cell::new(0));
    let completion = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(verified_response()?),
                Ok(commit_response(
                    BridgeGenerationSuccessorCommitOutcomeDto::AlreadyActive,
                )?),
            ],
            trace.clone(),
        ),
        &mut Put {
            fail: false,
            calls: Rc::clone(&put_calls),
        },
        UnixMillis::new(30),
    )?;
    assert!(completion.is_saved());
    assert_eq!(put_calls.get(), 0);
    assert_eq!(operator.coordinator().closed, 1);
    assert!(!operator.has_pending_dirty_close());
    let generation_ids = trace.generation_ids.borrow();
    assert_eq!(generation_ids.len(), 2);
    assert_eq!(generation_ids[0], generation_ids[1]);
    assert_eq!(
        completion.committed().generation_id().as_str(),
        generation_ids[0]
    );
    Ok(())
}

#[test]
fn canonical_postcommit_candidate_mutation_preserves_bytes_for_owned_recovery()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("postcommit-candidate-mutation")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;

    let root = fixture.root.clone();
    let tenant_id = fixture.actor.tenant_scope().tenant_id().clone();
    let profile_id = fixture.profile_id.clone();
    let hook: CommitHook = Rc::new(move |request| {
        let generation_id = GenerationId::parse(request.generation_id().to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let candidate = root
            .open_generation(&tenant_id, &profile_id, &generation_id)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        fs::write(
            candidate.path().join("late-postcommit-mutation"),
            b"changed-after-server-commit",
        )
        .map_err(|_| BridgePortError::Unavailable)?;
        Ok(())
    });
    let trace = TransportTrace::default();
    let completion = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(verified_response()?),
                Ok(commit_response(
                    BridgeGenerationSuccessorCommitOutcomeDto::Activated,
                )?),
            ],
            trace,
        )
        .with_commit_hook(hook),
        &mut Put {
            fail: false,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    )?;

    assert!(!completion.is_saved());
    assert!(matches!(
        completion.local().local_outcome(),
        DirtyCloseLocalOutcome::MaterializationBindingBlocked {
            generation_id,
            reason: crate::browser_execution::BrowserLaunchBlocker::RecoveryRequired,
        } if generation_id == completion.committed().generation_id()
    ));
    assert!(completion.local().workspace_lock_released());
    assert!(completion.local().coordinator_lease_released());
    assert!(!operator.has_pending_dirty_close());
    assert!(operator.cleanup_blocked());
    assert_eq!(operator.coordinator().closed, 1);
    let terminal = operator.last_terminal().ok_or("missing terminal record")?;
    assert_eq!(
        terminal.generation_id(),
        completion.committed().generation_id()
    );
    assert_eq!(
        terminal.local_state(),
        LocalGenerationState::SupersededEvictable
    );
    assert!(!terminal.cleanup_failures().any());
    let retained = fixture.root.open_generation(
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        completion.committed().generation_id(),
    )?;
    assert_eq!(
        fs::read(retained.path().join("late-postcommit-mutation"))?,
        b"changed-after-server-commit"
    );
    // A committed-but-unknown local candidate must keep its encrypted
    // checkpoint: there is no proof this local browser state is usable.
    assert!(matches!(
        inspect_exact_save_attempt(
            &fixture.root,
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            completion.committed().generation_id(),
        ),
        Err(SaveAttemptEvidenceError::Changed)
    ));
    assert!(
        fs::read_dir(fixture.root.path())?
            .filter_map(Result::ok)
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".profile-save-attempt-v1-"))
    );
    assert!(
        crate::browser_execution::load_materialization_binding(
            &retained,
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            completion.committed().generation_id(),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn canonical_postcommit_workspace_release_failure_is_recovery_required_but_still_releases_coordinator()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("postcommit-workspace-release")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;
    let base_workspace = fixture.base_workspace()?;
    fs::write(
        base_workspace.path().join(".profile-platform.lock"),
        b"tampered-lock-ownership\n",
    )?;

    let completion = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(verified_response()?),
                Ok(commit_response(
                    BridgeGenerationSuccessorCommitOutcomeDto::Activated,
                )?),
            ],
            TransportTrace::default(),
        ),
        &mut Put {
            fail: false,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    )?;

    assert!(!completion.is_saved());
    assert!(matches!(
        completion.local().local_outcome(),
        DirtyCloseLocalOutcome::CandidateAccepted(record)
            if record.generation_id() == completion.committed().generation_id()
    ));
    assert!(!completion.local().workspace_lock_released());
    assert!(completion.local().coordinator_lease_released());
    assert_eq!(operator.coordinator().closed, 1);
    assert!(!operator.has_pending_dirty_close());
    assert!(operator.cleanup_blocked());
    let terminal = operator.last_terminal().ok_or("missing terminal record")?;
    assert_eq!(
        terminal.generation_id(),
        completion.committed().generation_id()
    );
    assert_eq!(
        terminal.local_state(),
        LocalGenerationState::SupersededEvictable
    );
    assert!(terminal.cleanup_failures().workspace_lock());
    assert!(!terminal.cleanup_failures().coordinator_lease());
    Ok(())
}

#[test]
fn canonical_postcommit_coordinator_release_failure_is_recovery_required_without_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("postcommit-coordinator-release")?;
    let mut operator = fixture.operator_with_close_failure()?;
    fixture.open_mutate_and_close(&mut operator)?;

    let completion = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(verified_response()?),
                Ok(commit_response(
                    BridgeGenerationSuccessorCommitOutcomeDto::Activated,
                )?),
            ],
            TransportTrace::default(),
        ),
        &mut Put {
            fail: false,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    )?;

    assert!(!completion.is_saved());
    assert!(matches!(
        completion.local().local_outcome(),
        DirtyCloseLocalOutcome::CandidateAccepted(record)
            if record.generation_id() == completion.committed().generation_id()
    ));
    assert!(completion.local().workspace_lock_released());
    assert!(!completion.local().coordinator_lease_released());
    assert_eq!(operator.coordinator().closed, 1);
    assert!(!operator.has_pending_dirty_close());
    assert!(operator.cleanup_blocked());
    let terminal = operator.last_terminal().ok_or("missing terminal record")?;
    assert_eq!(
        terminal.generation_id(),
        completion.committed().generation_id()
    );
    assert_eq!(
        terminal.local_state(),
        LocalGenerationState::SupersededEvictable
    );
    assert!(!terminal.cleanup_failures().workspace_lock());
    assert!(terminal.cleanup_failures().coordinator_lease());

    let base_workspace = fixture.base_workspace()?;
    let reacquired =
        BridgeWorkspaceLock::acquire(&base_workspace, &fixture.device_id, fixture.lease.epoch())?;
    reacquired.release()?;
    Ok(())
}

#[test]
fn canonical_postcommit_writer_owned_candidate_retained_and_blocks_readiness()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new("postcommit-rematerialization-blocked")?;
    let mut operator = fixture.operator()?;
    fixture.open_mutate_and_close(&mut operator)?;

    let root = fixture.root.clone();
    let tenant_id = fixture.actor.tenant_scope().tenant_id().clone();
    let profile_id = fixture.profile_id.clone();
    let device_id = fixture.device_id.clone();
    let epoch = fixture.lease.epoch();
    let candidate_lock = Rc::new(RefCell::new(None));
    let hook_lock = Rc::clone(&candidate_lock);
    let hook: CommitHook = Rc::new(move |request| {
        let generation_id = GenerationId::parse(request.generation_id().to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let candidate = root
            .open_generation(&tenant_id, &profile_id, &generation_id)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        fs::write(
            candidate.path().join("late-postcommit-mutation"),
            b"changed-after-server-commit",
        )
        .map_err(|_| BridgePortError::Unavailable)?;
        let lock = BridgeWorkspaceLock::acquire(&candidate, &device_id, epoch)
            .map_err(|_| BridgePortError::Unavailable)?;
        *hook_lock.borrow_mut() = Some(lock);
        Ok(())
    });

    let completion = operator.save_retained_successor(
        &fixture.root,
        Transport::new(
            [
                Ok(verified_response()?),
                Ok(commit_response(
                    BridgeGenerationSuccessorCommitOutcomeDto::Activated,
                )?),
            ],
            TransportTrace::default(),
        )
        .with_commit_hook(hook),
        &mut Put {
            fail: false,
            calls: Rc::new(Cell::new(0)),
        },
        UnixMillis::new(30),
    )?;

    assert!(!completion.is_saved());
    assert!(matches!(
        completion.local().local_outcome(),
        DirtyCloseLocalOutcome::MaterializationBindingBlocked {
            generation_id,
            reason: crate::browser_execution::BrowserLaunchBlocker::RecoveryRequired,
        } if generation_id == completion.committed().generation_id()
    ));
    assert!(completion.local().workspace_lock_released());
    assert!(completion.local().coordinator_lease_released());
    assert_eq!(operator.coordinator().closed, 1);
    assert!(!operator.has_pending_dirty_close());
    assert!(operator.cleanup_blocked());
    let terminal = operator.last_terminal().ok_or("missing terminal record")?;
    assert_eq!(
        terminal.generation_id(),
        completion.committed().generation_id()
    );
    assert_eq!(
        terminal.local_state(),
        LocalGenerationState::SupersededEvictable
    );
    assert!(!terminal.cleanup_failures().any());
    let retained = fixture.root.open_generation(
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        completion.committed().generation_id(),
    )?;
    assert_eq!(
        fs::read(retained.path().join("late-postcommit-mutation"))?,
        b"changed-after-server-commit"
    );
    let lock = candidate_lock
        .borrow_mut()
        .take()
        .ok_or("candidate lock was not retained")?;
    lock.release()?;
    Ok(())
}
