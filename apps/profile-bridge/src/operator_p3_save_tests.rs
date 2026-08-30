use crate::dirty_close::DirtyCloseLocalOutcome;
use crate::dirty_generation::{GenerationSealingMaterial, GenerationSealingMaterialPort};
use crate::local_profile::{
    BridgeWorkspaceLock, GenerationWorkspace, LocalGenerationState, LocalProfileError,
    MaterializationRoot,
};
use crate::operator_flow::{
    BrowserLaunchPreflightPort, DeviceAuthenticationPort, EnrollmentPort, OperatorEnrollment,
    OperatorFailureStage, OperatorFlowError, ProfileBridgeOperator, RuntimeBundleSelectionPort,
};
use crate::runtime_bundle::ApprovedRuntimeBundle;
use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort, MachineHttpResponse};
use crate::shipping_generation_save::{
    SignedGenerationObjectPutPort, SignedGenerationUploadCapability,
};
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

    fn select_bundle(
        &mut self,
        _actor: &ActorContext,
        _profile_id: &ProfileId,
        _generation_id: &GenerationId,
    ) -> Result<ApprovedRuntimeBundle, Self::Error> {
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

type CommitHook = Rc<
    dyn Fn(&BridgeProfileGenerationSuccessorRequest) -> Result<(), BridgePortError>,
>;

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
        if path.ends_with("/commit") {
            if let Some(hook) = self.commit_hook.as_ref() {
                hook(&request)?;
            }
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
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::GenerationSave
        ))
    ));
    assert!(operator.has_pending_dirty_close());
    assert_eq!(
        operator.pending_dirty_local_state(),
        Some(LocalGenerationState::DirtyLocal)
    );
    assert_eq!(operator.coordinator().closed, 0);
    assert_eq!(put_calls.get(), 1);
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
    fs::write(
        stale_candidate.path().join("retry-residue"),
        b"stale-precommit-candidate",
    )?;

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
    assert!(matches!(
        completion.local().local_outcome(),
        DirtyCloseLocalOutcome::CandidateAccepted(record)
            if record.generation_id() == &candidate
    ));
    assert!(!operator.has_pending_dirty_close());
    assert_eq!(operator.coordinator().closed, 1);
    assert_eq!(put_calls.get(), 2);
    assert!(
        trace
            .generation_ids
            .borrow()
            .iter()
            .all(|generation_id| generation_id == candidate.as_str())
    );
    let rebuilt_candidate = fixture.root.open_generation(
        fixture.actor.tenant_scope().tenant_id(),
        &fixture.profile_id,
        &candidate,
    )?;
    assert!(!rebuilt_candidate.path().join("retry-residue").exists());

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
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::GenerationSave
        ))
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
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::GenerationSave
        ))
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
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::GenerationSave
        ))
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
fn canonical_postcommit_candidate_mutation_requires_rematerialization_without_fallback()
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
        DirtyCloseLocalOutcome::RematerializeRequired(generation_id)
            if generation_id == completion.committed().generation_id()
    ));
    assert!(completion.local().workspace_lock_released());
    assert!(completion.local().coordinator_lease_released());
    assert!(!operator.has_pending_dirty_close());
    assert!(!operator.cleanup_blocked());
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
    assert!(matches!(
        fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            completion.committed().generation_id(),
        ),
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound))
    ));
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
fn canonical_postcommit_writer_owned_candidate_blocks_rematerialization_fail_closed()
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
        DirtyCloseLocalOutcome::RematerializationBlocked {
            generation_id,
            error: LocalProfileError::LockBusy,
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
    let lock = candidate_lock
        .borrow_mut()
        .take()
        .ok_or("candidate lock was not retained")?;
    lock.release()?;
    Ok(())
}
