use application_ports::ProfileCoordinatorPort;
use bridge_domain::{
    BridgePortError, CamouhostMessage, CamouhostPort, ClaimUri, EnrollmentClaim,
};
use profile_bridge::local_profile::{LocalGenerationState, MaterializationRoot};
use profile_bridge::operator_flow::{
    DeviceAuthenticationPort, EnrollmentPort, OperatorEnrollment, OperatorFailureStage,
    OperatorFlowError, ProfileBridgeOperator, RuntimeBundleSelectionPort,
};
use profile_bridge::runtime_bundle::ApprovedRuntimeBundle;
use profile_bridge::{
    FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl, ProcessAction,
};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, GenerationId, ProfileId,
    SessionId, TenantId, TenantScope, UnixMillis,
};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use session_domain::ProfileLease;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
struct AllowAuthentication;

impl DeviceAuthenticationPort for AllowAuthentication {
    type Error = BridgePortError;

    fn authenticate(&mut self, _device_id: &DeviceId, _key_handle: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct TestEnrollment {
    claim: EnrollmentClaim,
    result: OperatorEnrollment,
}

impl EnrollmentPort for TestEnrollment {
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
struct TestCoordinator {
    lease: ProfileLease,
    acquire_fail: bool,
    close_fail: bool,
    acquired: u64,
    closed: u64,
}

impl ProfileCoordinatorPort for TestCoordinator {
    type Error = BridgePortError;

    fn acquire_lease(
        &mut self,
        _actor: &ActorContext,
        _profile_id: &ProfileId,
        _device_id: &DeviceId,
    ) -> Result<ProfileLease, Self::Error> {
        self.acquired += 1;
        if self.acquire_fail {
            Err(BridgePortError::Unavailable)
        } else {
            Ok(self.lease.clone())
        }
    }

    fn close_lease(&mut self, _lease: &ProfileLease) -> Result<(), Self::Error> {
        self.closed += 1;
        if self.close_fail {
            Err(BridgePortError::Unavailable)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
struct TestRuntimeBundles {
    bundle: ApprovedRuntimeBundle,
}

impl RuntimeBundleSelectionPort for TestRuntimeBundles {
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

#[derive(Default)]
struct RejectHelloCamouhost;

impl CamouhostPort for RejectHelloCamouhost {
    fn exchange(
        &mut self,
        _message: &CamouhostMessage,
    ) -> Result<CamouhostMessage, BridgePortError> {
        Err(BridgePortError::InvalidResponse)
    }
}

struct Fixture {
    root_path: PathBuf,
    root: MaterializationRoot,
    claim_uri: ClaimUri,
    other_claim_uri: ClaimUri,
    actor: ActorContext,
    profile_id: ProfileId,
    generation_id: GenerationId,
    device_id: DeviceId,
    lease: ProfileLease,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root_path = std::env::temp_dir().join(format!(
            "profile-bridge-acceptance-{}-{counter}",
            std::process::id()
        ));
        if root_path.exists() {
            fs::remove_dir_all(&root_path)?;
        }
        let root = MaterializationRoot::open_or_create(root_path.clone())?;
        let tenant_id = TenantId::parse(format!("tenant_01JACCEPT{counter}"))?;
        let profile_id = ProfileId::parse(format!("profile_01JACCEPT{counter}"))?;
        let generation_id = GenerationId::parse(format!("generation_01JACCEPT{counter}"))?;
        let device_id = DeviceId::parse(format!("device_01JACCEPT{counter}"))?;
        root.create_generation(&tenant_id, &profile_id, &generation_id)?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            ActorId::parse(format!("actor_01JACCEPT{counter}"))?,
            CorrelationId::parse(format!("corr_01JACCEPT{counter}"))?,
        );
        let lease = ProfileLease::issue(
            tenant_id,
            profile_id.clone(),
            SessionId::parse(format!("session_01JACCEPT{counter}"))?,
            device_id.clone(),
            counter.max(1),
            FencingToken::parse(format!("fence_01JACCEPT{counter}"))?,
        )?;
        let claim_uri = ClaimUri::parse(&format!(
            "profilebridge://claim/claim_01JACCEPT{counter:024}"
        ))?;
        let other_claim_uri = ClaimUri::parse(&format!(
            "profilebridge://claim/claim_01JOTHER{counter:025}"
        ))?;
        Ok(Self {
            root_path,
            root,
            claim_uri,
            other_claim_uri,
            actor,
            profile_id,
            generation_id,
            device_id,
            lease,
        })
    }

    fn enrollment(&self) -> Result<TestEnrollment, Box<dyn std::error::Error>> {
        Ok(TestEnrollment {
            claim: EnrollmentClaim::issue(
                self.claim_uri.claim_code().clone(),
                UnixMillis::new(1),
                UnixMillis::new(1_000),
            )?,
            result: OperatorEnrollment::new(
                self.actor.clone(),
                self.profile_id.clone(),
                self.generation_id.clone(),
            ),
        })
    }

    fn coordinator(&self) -> TestCoordinator {
        TestCoordinator {
            lease: self.lease.clone(),
            acquire_fail: false,
            close_fail: false,
            acquired: 0,
            closed: 0,
        }
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

fn operator<H: CamouhostPort>(
    fixture: &Fixture,
    coordinator: TestCoordinator,
    camouhost: H,
) -> Result<
    ProfileBridgeOperator<
        FakeDeviceIdentity,
        FakeDeviceKeyStore,
        AllowAuthentication,
        TestEnrollment,
        TestCoordinator,
        TestRuntimeBundles,
        FakeProcessControl,
        H,
    >,
    Box<dyn std::error::Error>,
> {
    Ok(ProfileBridgeOperator::new(
        FakeDeviceIdentity::new(fixture.device_id.clone()),
        FakeDeviceKeyStore::default(),
        AllowAuthentication,
        fixture.enrollment()?,
        coordinator,
        TestRuntimeBundles {
            bundle: approved_bundle()?,
        },
        FakeProcessControl::default(),
        camouhost,
    ))
}

#[test]
fn busy_invalid_and_replayed_claims_fail_before_second_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut operator = operator(&fixture, fixture.coordinator(), FakeCamouhost::default())?;

    assert_eq!(
        operator.open(&fixture.other_claim_uri, &fixture.root, UnixMillis::new(10)),
        Err(OperatorFlowError::Stage(OperatorFailureStage::Enrollment))
    );
    assert_eq!(operator.coordinator().acquired, 0);
    assert!(operator.process().actions().is_empty());

    operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(11))?;
    assert_eq!(
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(12)),
        Err(OperatorFlowError::Busy)
    );
    operator.close(UnixMillis::new(20))?;
    assert_eq!(
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(21)),
        Err(OperatorFlowError::Stage(OperatorFailureStage::Enrollment))
    );
    assert_eq!(operator.coordinator().acquired, 1);
    Ok(())
}

#[test]
fn coordinator_failure_starts_no_runtime_process() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut coordinator = fixture.coordinator();
    coordinator.acquire_fail = true;
    let mut operator = operator(&fixture, coordinator, FakeCamouhost::default())?;

    assert_eq!(
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
        Err(OperatorFlowError::Stage(
            OperatorFailureStage::CoordinatorAcquire
        ))
    );
    assert_eq!(operator.coordinator().acquired, 1);
    assert_eq!(operator.coordinator().closed, 0);
    assert!(operator.process().actions().is_empty());
    Ok(())
}

#[test]
fn launch_protocol_failure_marks_recovery_and_releases_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut operator = operator(&fixture, fixture.coordinator(), RejectHelloCamouhost)?;

    let error = operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10));
    assert!(matches!(
        error,
        Err(OperatorFlowError::Runtime {
            stage: OperatorFailureStage::RuntimeLaunch,
            ..
        })
    ));
    assert_eq!(
        operator.last_terminal().map(|record| record.local_state()),
        Some(LocalGenerationState::RecoveryRequired)
    );
    assert_eq!(operator.coordinator().closed, 1);
    assert_eq!(
        operator.process().actions(),
        [
            ProcessAction::Spawn(fixture.lease.session_id().clone()),
            ProcessAction::ForceTerminate(fixture.lease.session_id().clone()),
        ]
    );
    Ok(())
}

#[test]
fn unresolved_cleanup_is_observable_and_blocks_future_sessions()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let wrong_lease = ProfileLease::issue(
        fixture.actor.tenant_scope().tenant_id().clone(),
        fixture.profile_id.clone(),
        SessionId::parse("session_01JACCEPT_WRONG")?,
        DeviceId::parse("device_01JACCEPT_WRONG")?,
        999,
        FencingToken::parse("fence_01JACCEPT_WRONG")?,
    )?;
    let mut coordinator = fixture.coordinator();
    coordinator.lease = wrong_lease;
    coordinator.close_fail = true;
    let mut operator = operator(&fixture, coordinator, FakeCamouhost::default())?;

    assert!(matches!(
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
        Err(OperatorFlowError::Terminal {
            stage: OperatorFailureStage::LeaseValidation,
            cleanup,
            ..
        }) if cleanup.coordinator_lease()
    ));
    assert!(operator.cleanup_blocked());
    assert_eq!(
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(11)),
        Err(OperatorFlowError::CleanupRequired)
    );
    assert!(operator.process().actions().is_empty());
    Ok(())
}
