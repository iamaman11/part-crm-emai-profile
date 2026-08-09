use application_ports::ProfileCoordinatorPort;
use bridge_domain::{BridgePortError, ClaimUri, EnrollmentClaim};
use browser_execution_domain::{
    BrowserIdentityManifest, MaterializationBinding, NetworkClass, NetworkIdentityObservation,
    NetworkIdentityPolicy,
};
use profile_bridge::browser_execution::persist_materialization_binding;
use profile_bridge::browser_preflight::{
    BoundBrowserLaunchPreflight, BrowserRuntimeObservation, BrowserRuntimeObservationPort,
};
use profile_bridge::local_profile::{
    GenerationWorkspace, LocalGenerationState, MaterializationRoot,
};
use profile_bridge::operator_flow::{
    DeviceAuthenticationPort, EnrollmentPort, OperatorEnrollment, ProfileBridgeOperator,
    RuntimeBundleSelectionPort,
};
use profile_bridge::runtime_bundle::ApprovedRuntimeBundle;
use profile_bridge::{FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, GenerationId, ProfileId,
    SessionId, TenantId, TenantScope, UnixMillis,
};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use session_domain::ProfileLease;
use std::env;
use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

const SYNTHETIC_NOW: UnixMillis = UnixMillis::new(10);
const SYNTHETIC_CLOSE_AT: UnixMillis = UnixMillis::new(20);
const SYNTHETIC_CLAIM_EXPIRY: UnixMillis = UnixMillis::new(1_000);
const SYNTHETIC_RUNTIME_VERSION: &str = "0.1.0";

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("profile-bridge-synthetic: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run<I>(arguments: I) -> Result<(), SyntheticOperatorError>
where
    I: IntoIterator<Item = String>,
{
    let arguments = SyntheticArguments::parse(arguments)?;
    let claim = ClaimUri::parse(&arguments.claim_uri)
        .map_err(|_| SyntheticOperatorError::InvalidClaimUri)?;
    let root = MaterializationRoot::open_or_create(arguments.materialization_root)
        .map_err(|_| SyntheticOperatorError::MaterializationRoot)?;
    let fixture = SyntheticFixture::new(&claim)?;
    let workspace = root
        .create_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )
        .map_err(|_| SyntheticOperatorError::MaterializationGeneration)?;

    let runtime_bundles = SyntheticRuntimeBundles::new()?;
    let browser_preflight = synthetic_browser_preflight(&workspace, &fixture, &runtime_bundles)?;
    let mut operator = ProfileBridgeOperator::new(
        FakeDeviceIdentity::new(fixture.device_id.clone()),
        FakeDeviceKeyStore::default(),
        SyntheticDeviceAuthentication,
        SyntheticEnrollment::new(&claim, &fixture)?,
        SyntheticCoordinator::new(&fixture)?,
        runtime_bundles,
        browser_preflight,
        FakeProcessControl::default(),
        FakeCamouhost::default(),
    );
    operator
        .open(&claim, &root, SYNTHETIC_NOW)
        .map_err(|_| SyntheticOperatorError::OperatorOpen)?;
    let terminal = operator
        .close(SYNTHETIC_CLOSE_AT)
        .map_err(|_| SyntheticOperatorError::OperatorClose)?;
    if terminal.local_state() != LocalGenerationState::DirtyLocal
        || terminal.cleanup_failures().any()
        || operator.cleanup_blocked()
    {
        return Err(SyntheticOperatorError::UnexpectedTerminalState);
    }
    println!("synthetic-operator-complete state=DIRTY_LOCAL");
    Ok(())
}

fn synthetic_browser_preflight(
    workspace: &GenerationWorkspace,
    fixture: &SyntheticFixture,
    runtime_bundles: &SyntheticRuntimeBundles,
) -> Result<BoundBrowserLaunchPreflight<SyntheticRuntimeObservation>, SyntheticOperatorError> {
    let manifest = runtime_bundles.bundle.manifest();
    let browser_identity = BrowserIdentityManifest::new(
        1,
        manifest.runtime_version(),
        manifest.inventory_sha256().as_str(),
        "synthetic-camoufox-v1",
        "b".repeat(64),
    )
    .map_err(|_| SyntheticOperatorError::BrowserPreflightFixture)?;
    let binding = MaterializationBinding::new(
        fixture.actor.tenant_scope().tenant_id().clone(),
        fixture.profile_id.clone(),
        fixture.generation_id.clone(),
        "c".repeat(64),
        workspace
            .inventory()
            .map_err(|_| SyntheticOperatorError::BrowserPreflightFixture)?
            .inventory_digest(),
        browser_identity,
    )
    .map_err(|_| SyntheticOperatorError::BrowserPreflightFixture)?;
    persist_materialization_binding(workspace, &binding)
        .map_err(|_| SyntheticOperatorError::BrowserPreflightFixture)?;
    let policy = NetworkIdentityPolicy::new(
        Some("PL".to_owned()),
        Some("Mazowieckie".to_owned()),
        Some("Europe/Warsaw".to_owned()),
        [NetworkClass::Mobile],
        [5617],
        Some("synthetic-route".to_owned()),
    )
    .map_err(|_| SyntheticOperatorError::BrowserPreflightFixture)?;
    let observation = NetworkIdentityObservation::new(
        "PL",
        "Mazowieckie",
        "Europe/Warsaw",
        NetworkClass::Mobile,
        5617,
        "synthetic-route",
    )
    .map_err(|_| SyntheticOperatorError::BrowserPreflightFixture)?;
    Ok(BoundBrowserLaunchPreflight::new(
        binding,
        policy,
        SyntheticRuntimeObservation { observation },
    ))
}

struct SyntheticRuntimeObservation {
    observation: NetworkIdentityObservation,
}

impl BrowserRuntimeObservationPort for SyntheticRuntimeObservation {
    type Error = BridgePortError;

    fn observe(
        &mut self,
        _workspace: &GenerationWorkspace,
        _device_id: &DeviceId,
    ) -> Result<BrowserRuntimeObservation, Self::Error> {
        Ok(BrowserRuntimeObservation::new(
            self.observation.clone(),
            false,
        ))
    }
}

struct SyntheticArguments {
    claim_uri: String,
    materialization_root: PathBuf,
}

impl SyntheticArguments {
    fn parse<I>(arguments: I) -> Result<Self, SyntheticOperatorError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut arguments = arguments.into_iter();
        let claim_uri = arguments
            .next()
            .ok_or(SyntheticOperatorError::MissingClaimUri)?;
        let materialization_root = PathBuf::from(
            arguments
                .next()
                .ok_or(SyntheticOperatorError::MissingMaterializationRoot)?,
        );
        if arguments.next().is_some() {
            return Err(SyntheticOperatorError::ExtraArguments);
        }
        if !materialization_root.is_absolute() {
            return Err(SyntheticOperatorError::MaterializationRootMustBeAbsolute);
        }
        Ok(Self {
            claim_uri,
            materialization_root,
        })
    }
}

#[derive(Clone)]
struct SyntheticFixture {
    actor: ActorContext,
    profile_id: ProfileId,
    generation_id: GenerationId,
    device_id: DeviceId,
}

impl SyntheticFixture {
    fn new(_claim: &ClaimUri) -> Result<Self, SyntheticOperatorError> {
        let tenant_id = TenantId::parse("tenant_01JSYNTHETICOPERATOR")
            .map_err(|_| SyntheticOperatorError::FixtureIdentity)?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id),
            ActorId::parse("actor_01JSYNTHETICOPERATOR")
                .map_err(|_| SyntheticOperatorError::FixtureIdentity)?,
            CorrelationId::parse("corr_01JSYNTHETICOPERATOR")
                .map_err(|_| SyntheticOperatorError::FixtureIdentity)?,
        );
        Ok(Self {
            actor,
            profile_id: ProfileId::parse("profile_01JSYNTHETICOPERATOR")
                .map_err(|_| SyntheticOperatorError::FixtureIdentity)?,
            generation_id: GenerationId::parse("generation_01JSYNTHETICOPERATOR")
                .map_err(|_| SyntheticOperatorError::FixtureIdentity)?,
            device_id: DeviceId::parse("device_01JSYNTHETICOPERATOR")
                .map_err(|_| SyntheticOperatorError::FixtureIdentity)?,
        })
    }
}

struct SyntheticDeviceAuthentication;

impl DeviceAuthenticationPort for SyntheticDeviceAuthentication {
    type Error = BridgePortError;

    fn authenticate(&mut self, device_id: &DeviceId, key_handle: &str) -> Result<(), Self::Error> {
        let expected = format!("fake_key_handle_{}", device_id.as_str());
        if key_handle == expected {
            Ok(())
        } else {
            Err(BridgePortError::InvalidResponse)
        }
    }
}

struct SyntheticEnrollment {
    claim: EnrollmentClaim,
    enrollment: OperatorEnrollment,
}

impl SyntheticEnrollment {
    fn new(claim: &ClaimUri, fixture: &SyntheticFixture) -> Result<Self, SyntheticOperatorError> {
        Ok(Self {
            claim: EnrollmentClaim::issue(
                claim.claim_code().clone(),
                UnixMillis::new(1),
                SYNTHETIC_CLAIM_EXPIRY,
            )
            .map_err(|_| SyntheticOperatorError::EnrollmentFixture)?,
            enrollment: OperatorEnrollment::new(
                fixture.actor.clone(),
                fixture.profile_id.clone(),
                fixture.generation_id.clone(),
            ),
        })
    }
}

impl EnrollmentPort for SyntheticEnrollment {
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
        Ok(self.enrollment.clone())
    }
}

struct SyntheticCoordinator {
    lease: ProfileLease,
    active: bool,
}

impl SyntheticCoordinator {
    fn new(fixture: &SyntheticFixture) -> Result<Self, SyntheticOperatorError> {
        let lease = ProfileLease::issue(
            fixture.actor.tenant_scope().tenant_id().clone(),
            fixture.profile_id.clone(),
            SessionId::parse("session_01JSYNTHETICOPERATOR")
                .map_err(|_| SyntheticOperatorError::FixtureIdentity)?,
            fixture.device_id.clone(),
            1,
            FencingToken::parse("fence_01JSYNTHETICOPERATOR")
                .map_err(|_| SyntheticOperatorError::FixtureIdentity)?,
        )
        .map_err(|_| SyntheticOperatorError::CoordinatorFixture)?;
        Ok(Self {
            lease,
            active: false,
        })
    }
}

impl ProfileCoordinatorPort for SyntheticCoordinator {
    type Error = BridgePortError;

    fn acquire_lease(
        &mut self,
        _actor: &ActorContext,
        _profile_id: &ProfileId,
        _device_id: &DeviceId,
    ) -> Result<ProfileLease, Self::Error> {
        if self.active {
            return Err(BridgePortError::Unavailable);
        }
        self.active = true;
        Ok(self.lease.clone())
    }

    fn close_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error> {
        if !self.active || lease.session_id() != self.lease.session_id() {
            return Err(BridgePortError::InvalidResponse);
        }
        self.active = false;
        Ok(())
    }
}

struct SyntheticRuntimeBundles {
    bundle: ApprovedRuntimeBundle,
}

impl SyntheticRuntimeBundles {
    fn new() -> Result<Self, SyntheticOperatorError> {
        let calculated = digest('a')?;
        let entrypoint = BundleRelativePath::parse("camouhost/main.py")
            .map_err(|_| SyntheticOperatorError::RuntimeBundleFixture)?;
        let manifest = RuntimeManifest::new(
            SYNTHETIC_RUNTIME_VERSION,
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            calculated.clone(),
        )
        .map_err(|_| SyntheticOperatorError::RuntimeBundleFixture)?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(entrypoint, 10, digest('b')?)])
            .map_err(|_| SyntheticOperatorError::RuntimeBundleFixture)?;
        let bundle = ApprovedRuntimeBundle::validate(manifest, inventory, &calculated)
            .map_err(|_| SyntheticOperatorError::RuntimeBundleFixture)?;
        Ok(Self { bundle })
    }
}

impl RuntimeBundleSelectionPort for SyntheticRuntimeBundles {
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

fn digest(character: char) -> Result<Sha256Digest, SyntheticOperatorError> {
    Sha256Digest::parse(character.to_string().repeat(64))
        .map_err(|_| SyntheticOperatorError::RuntimeBundleFixture)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntheticOperatorError {
    MissingClaimUri,
    MissingMaterializationRoot,
    ExtraArguments,
    InvalidClaimUri,
    MaterializationRootMustBeAbsolute,
    MaterializationRoot,
    MaterializationGeneration,
    BrowserPreflightFixture,
    FixtureIdentity,
    EnrollmentFixture,
    CoordinatorFixture,
    RuntimeBundleFixture,
    OperatorOpen,
    OperatorClose,
    UnexpectedTerminalState,
}

impl fmt::Display for SyntheticOperatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingClaimUri => "missing claim URI",
            Self::MissingMaterializationRoot => "missing absolute materialization root",
            Self::ExtraArguments => "unexpected extra arguments",
            Self::InvalidClaimUri => "invalid claim URI",
            Self::MaterializationRootMustBeAbsolute => "materialization root must be absolute",
            Self::MaterializationRoot => "could not prepare materialization root",
            Self::MaterializationGeneration => "could not create synthetic generation",
            Self::BrowserPreflightFixture => "synthetic browser preflight fixture is invalid",
            Self::FixtureIdentity => "synthetic identity fixture is invalid",
            Self::EnrollmentFixture => "synthetic enrollment fixture is invalid",
            Self::CoordinatorFixture => "synthetic coordinator fixture is invalid",
            Self::RuntimeBundleFixture => "synthetic runtime bundle fixture is invalid",
            Self::OperatorOpen => "synthetic operator open failed",
            Self::OperatorClose => "synthetic operator close failed",
            Self::UnexpectedTerminalState => "synthetic operator ended in an unexpected state",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SyntheticOperatorError {}

#[cfg(test)]
mod tests {
    use super::{SyntheticArguments, SyntheticOperatorError};

    #[test]
    fn arguments_require_exact_claim_and_absolute_root() {
        assert_eq!(
            SyntheticArguments::parse(Vec::<String>::new()).err(),
            Some(SyntheticOperatorError::MissingClaimUri)
        );
        assert_eq!(
            SyntheticArguments::parse(["profilebridge://claim/claim_0123456789abcdef".to_owned()])
                .err(),
            Some(SyntheticOperatorError::MissingMaterializationRoot)
        );
        assert_eq!(
            SyntheticArguments::parse([
                "profilebridge://claim/claim_0123456789abcdef".to_owned(),
                "relative-root".to_owned(),
            ])
            .err(),
            Some(SyntheticOperatorError::MaterializationRootMustBeAbsolute)
        );
    }
}
