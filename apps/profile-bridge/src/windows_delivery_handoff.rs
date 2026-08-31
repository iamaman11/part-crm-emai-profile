#![forbid(unsafe_code)]

use crate::local_profile::DeliveryActivationGuard;
use crate::windows_delivery::{DeliveryIdentity, DeliveryState, DeliveryStateError};
use crate::windows_delivery_recovery::VerifiedDeliveryRecoveryTarget;
use crate::windows_delivery_staging::{
    DeliveryStagingError, DeliveryStagingRoot, StagedDelivery, reopen_staged_delivery,
};
use crate::windows_delivery_store::{
    DeliveryHandoffEvidence, DeliveryHandoffKind, DeliveryHandoffOutcome, DeliveryStateStore,
    DeliveryStateStoreError,
};
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Child;
#[cfg(any(windows, test))]
use std::process::Command;

const PROFILE_BRIDGE_DIRECTORY: &str = "profile-bridge";
const PROFILE_BRIDGE_EXECUTABLE: &str = "profile-bridge.exe";
const RELEASES_DIRECTORY: &str = "releases";
const STATE_DIRECTORY: &str = "state";
pub const HANDOFF_ACTIVATE_ARGUMENT: &str = "--delivery-activate-staged";
pub const HANDOFF_ARRIVAL_ARGUMENT: &str = "--delivery-handoff-arrived";
#[cfg(any(windows, test))]
const PARENT_PID_ENV: &str = "PART_CRM_DELIVERY_HANDOFF_PARENT_PID";
#[cfg(any(windows, test))]
const CURRENT_EXECUTABLE_ENV: &str = "PART_CRM_DELIVERY_HANDOFF_CURRENT";
#[cfg(any(windows, test))]
const TARGET_EXECUTABLE_ENV: &str = "PART_CRM_DELIVERY_HANDOFF_TARGET";
#[cfg(any(windows, test))]
const POWERSHELL_HANDOFF: &str = "$ErrorActionPreference='Stop'; $pidToWait=[uint32]$env:PART_CRM_DELIVERY_HANDOFF_PARENT_PID; $expectedCurrent=$env:PART_CRM_DELIVERY_HANDOFF_CURRENT; if ([string]::IsNullOrWhiteSpace($expectedCurrent)) { exit 20 }; try { $parent=Get-Process -Id $pidToWait -ErrorAction Stop } catch { exit 21 }; try { $actualCurrent=[IO.Path]::GetFullPath($parent.Path); $expectedCurrent=[IO.Path]::GetFullPath($expectedCurrent) } catch { exit 22 }; if ($actualCurrent -ine $expectedCurrent) { exit 23 }; $parent.WaitForExit(); $target=$env:PART_CRM_DELIVERY_HANDOFF_TARGET; if ([string]::IsNullOrWhiteSpace($target)) { exit 24 }; $child=Start-Process -FilePath $target -ArgumentList '--delivery-handoff-arrived' -PassThru; if ($null -eq $child) { exit 25 }";

/// Exact filesystem/process identity already selected by the Windows delivery owner.
///
/// This value never selects a candidate. It can only be built from a stage that has already passed
/// the canonical staging verifier, or from R8's exact verified recovery target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDeliveryProcessTarget {
    identity: DeliveryIdentity,
    release_root: PathBuf,
    profile_bridge_executable: PathBuf,
}

impl VerifiedDeliveryProcessTarget {
    pub fn from_staged(staged: &StagedDelivery) -> Result<Self, DeliveryHandoffError> {
        let release_root =
            fs::canonicalize(staged.path()).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
        let executable = staged.profile_bridge_root().join(PROFILE_BRIDGE_EXECUTABLE);
        let executable = canonical_regular_executable(&executable)?;
        if executable.parent().and_then(Path::parent) != Some(release_root.as_path()) {
            return Err(DeliveryHandoffError::InvalidTarget);
        }
        Ok(Self {
            identity: staged.identity().clone(),
            release_root,
            profile_bridge_executable: executable,
        })
    }

    pub fn from_recovery(
        target: &VerifiedDeliveryRecoveryTarget,
    ) -> Result<Self, DeliveryHandoffError> {
        let release_root = fs::canonicalize(target.release_root())
            .map_err(|_| DeliveryHandoffError::InvalidTarget)?;
        let executable = canonical_regular_executable(target.profile_bridge_executable())?;
        if executable.parent().and_then(Path::parent) != Some(release_root.as_path()) {
            return Err(DeliveryHandoffError::InvalidTarget);
        }
        Ok(Self {
            identity: target.identity().clone(),
            release_root,
            profile_bridge_executable: executable,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> &DeliveryIdentity {
        &self.identity
    }

    #[must_use]
    pub fn release_root(&self) -> &Path {
        &self.release_root
    }

    #[must_use]
    pub fn profile_bridge_executable(&self) -> &Path {
        &self.profile_bridge_executable
    }
}

/// One mechanical same-binary transfer. The request owns no release/trust/lifecycle policy and
/// carries no claim credential. The child helper binds the exact old PID to the exact old executable,
/// waits for that process to disappear, then starts the already-verified target in bounded arrival
/// mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OneShotDeliveryHandoff {
    current_process_id: u32,
    current_executable: PathBuf,
    target: VerifiedDeliveryProcessTarget,
}

impl OneShotDeliveryHandoff {
    pub fn new(
        current_process_id: u32,
        current_executable: impl AsRef<Path>,
        target: VerifiedDeliveryProcessTarget,
    ) -> Result<Self, DeliveryHandoffError> {
        if current_process_id == 0 {
            return Err(DeliveryHandoffError::InvalidCurrentProcess);
        }
        let current_executable = canonical_regular_executable(current_executable.as_ref())?;
        if current_executable.file_name().and_then(OsStr::to_str) != Some(PROFILE_BRIDGE_EXECUTABLE)
            || current_executable == target.profile_bridge_executable
        {
            return Err(DeliveryHandoffError::InvalidCurrentProcess);
        }
        Ok(Self {
            current_process_id,
            current_executable,
            target,
        })
    }

    #[must_use]
    pub const fn current_process_id(&self) -> u32 {
        self.current_process_id
    }

    #[must_use]
    pub fn current_executable(&self) -> &Path {
        &self.current_executable
    }

    #[must_use]
    pub const fn target(&self) -> &VerifiedDeliveryProcessTarget {
        &self.target
    }

    #[cfg(windows)]
    pub fn schedule(self) -> Result<Child, DeliveryHandoffError> {
        let mut command = self.windows_helper_command();
        command
            .spawn()
            .map_err(|_| DeliveryHandoffError::HelperLaunchFailed)
    }

    #[cfg(any(windows, test))]
    fn windows_helper_command(&self) -> Command {
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                POWERSHELL_HANDOFF,
            ])
            .env(PARENT_PID_ENV, self.current_process_id.to_string())
            .env(CURRENT_EXECUTABLE_ENV, &self.current_executable)
            .env(
                TARGET_EXECUTABLE_ENV,
                &self.target.profile_bridge_executable,
            );
        command
    }
}

/// The R9 effect-shell composition. It never chooses a release: canonical delivery state and exact
/// staged/recovery targets are inputs. Handoff evidence is stored in the existing hash-chained state
/// journal, and activation itself happens only when the exact successor process arrives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryHandoffCoordinator {
    staging_root: DeliveryStagingRoot,
    state_root: PathBuf,
}

impl DeliveryHandoffCoordinator {
    #[must_use]
    pub fn new(staging_root: DeliveryStagingRoot, state_root: impl Into<PathBuf>) -> Self {
        Self {
            staging_root,
            state_root: state_root.into(),
        }
    }

    pub fn from_current_executable(
        current_executable: impl AsRef<Path>,
    ) -> Result<Self, DeliveryHandoffError> {
        let current_executable = canonical_regular_executable(current_executable.as_ref())?;
        let profile_bridge_root = current_executable
            .parent()
            .ok_or(DeliveryHandoffError::InvalidInstalledLayout)?;
        if profile_bridge_root.file_name().and_then(OsStr::to_str) != Some(PROFILE_BRIDGE_DIRECTORY) {
            return Err(DeliveryHandoffError::InvalidInstalledLayout);
        }
        let release_root = profile_bridge_root
            .parent()
            .ok_or(DeliveryHandoffError::InvalidInstalledLayout)?;
        let releases_root = release_root
            .parent()
            .ok_or(DeliveryHandoffError::InvalidInstalledLayout)?;
        if releases_root.file_name().and_then(OsStr::to_str) != Some(RELEASES_DIRECTORY) {
            return Err(DeliveryHandoffError::InvalidInstalledLayout);
        }
        let delivery_root = releases_root
            .parent()
            .ok_or(DeliveryHandoffError::InvalidInstalledLayout)?;
        let releases_metadata = fs::symlink_metadata(releases_root)
            .map_err(|_| DeliveryHandoffError::InvalidInstalledLayout)?;
        if releases_metadata.file_type().is_symlink() || !releases_metadata.is_dir() {
            return Err(DeliveryHandoffError::InvalidInstalledLayout);
        }
        let staging_root = DeliveryStagingRoot::open_or_create(releases_root)
            .map_err(|_| DeliveryHandoffError::InvalidInstalledLayout)?;
        Ok(Self::new(staging_root, delivery_root.join(STATE_DIRECTORY)))
    }

    pub fn has_started_handoff(&self) -> Result<bool, DeliveryHandoffError> {
        let store = self.open_store()?;
        Ok(store
            .handoff()
            .is_some_and(|evidence| evidence.outcome() == DeliveryHandoffOutcome::Started))
    }

    pub fn prepare_activation(
        &self,
        _guard: &DeliveryActivationGuard,
        current_process_id: u32,
        current_executable: impl AsRef<Path>,
    ) -> Result<OneShotDeliveryHandoff, DeliveryHandoffError> {
        let mut store = self.open_store()?;
        if store
            .handoff()
            .is_some_and(|evidence| evidence.outcome() == DeliveryHandoffOutcome::Started)
        {
            return Err(DeliveryHandoffError::HandoffAlreadyStarted);
        }
        let target_identity = store
            .state()
            .staged()
            .cloned()
            .ok_or(DeliveryHandoffError::NoStagedTarget)?;
        self.verify_current_active_source(store.state(), current_executable.as_ref())?;
        let target = self.target_for_identity(&target_identity)?;
        let handoff =
            OneShotDeliveryHandoff::new(current_process_id, current_executable.as_ref(), target)?;
        let evidence = DeliveryHandoffEvidence::activation_started(store.state(), &target_identity)
            .map_err(|_| DeliveryHandoffError::StateMismatch)?;
        let state = store.state().clone();
        self.persist_handoff_exact(&mut store, &state, evidence)?;
        Ok(handoff)
    }

    pub fn prepare_recovery(
        &self,
        _guard: &DeliveryActivationGuard,
        source_candidate: &DeliveryIdentity,
        source_attempt: u64,
        recovery_target: &VerifiedDeliveryRecoveryTarget,
        current_process_id: u32,
        current_executable: impl AsRef<Path>,
    ) -> Result<OneShotDeliveryHandoff, DeliveryHandoffError> {
        let mut store = self.open_store()?;
        if store
            .handoff()
            .is_some_and(|evidence| evidence.outcome() == DeliveryHandoffOutcome::Started)
        {
            return Err(DeliveryHandoffError::HandoffAlreadyStarted);
        }
        self.verify_current_identity(source_candidate, current_executable.as_ref())?;
        let target = VerifiedDeliveryProcessTarget::from_recovery(recovery_target)?;
        let reopened_target = self.target_for_identity(target.identity())?;
        if reopened_target != target {
            return Err(DeliveryHandoffError::InvalidTarget);
        }
        let handoff =
            OneShotDeliveryHandoff::new(current_process_id, current_executable.as_ref(), target)?;
        let evidence = DeliveryHandoffEvidence::recovery_started(
            store.state(),
            source_candidate,
            source_attempt,
            handoff.target().identity(),
        )
        .map_err(|_| DeliveryHandoffError::StateMismatch)?;
        let state = store.state().clone();
        self.persist_handoff_exact(&mut store, &state, evidence)?;
        Ok(handoff)
    }

    pub fn resume_started(
        &self,
        _guard: &DeliveryActivationGuard,
        current_process_id: u32,
        current_executable: impl AsRef<Path>,
    ) -> Result<Option<OneShotDeliveryHandoff>, DeliveryHandoffError> {
        let store = self.open_store()?;
        let Some(evidence) = store.handoff().cloned() else {
            return Ok(None);
        };
        if evidence.outcome() != DeliveryHandoffOutcome::Started {
            return Ok(None);
        }
        match evidence.kind() {
            DeliveryHandoffKind::Activation => {
                self.verify_current_active_source(store.state(), current_executable.as_ref())?;
            }
            DeliveryHandoffKind::Recovery => {
                self.verify_current_identity(
                    evidence.source_candidate(),
                    current_executable.as_ref(),
                )?;
            }
        }
        let target = self.target_for_identity(evidence.target())?;
        OneShotDeliveryHandoff::new(current_process_id, current_executable, target).map(Some)
    }

    pub fn complete_arrival(
        &self,
        guard: &DeliveryActivationGuard,
        current_executable: impl AsRef<Path>,
    ) -> Result<(), DeliveryHandoffError> {
        let mut store = self.open_store()?;
        let evidence = store
            .handoff()
            .cloned()
            .ok_or(DeliveryHandoffError::NoStartedHandoff)?;
        if evidence.outcome() != DeliveryHandoffOutcome::Started {
            return Err(DeliveryHandoffError::NoStartedHandoff);
        }
        let target = self.target_for_identity(evidence.target())?;
        let current = canonical_regular_executable(current_executable.as_ref())?;
        if current != target.profile_bridge_executable {
            return Err(DeliveryHandoffError::CurrentExecutableMismatch);
        }

        let mut next = store.state().clone();
        match evidence.kind() {
            DeliveryHandoffKind::Activation => next
                .activate_staged(guard.confirms_quiescence())
                .map_err(DeliveryHandoffError::State)?,
            DeliveryHandoffKind::Recovery => {}
        }
        let arrived = evidence
            .arrived(&next)
            .map_err(|_| DeliveryHandoffError::StateMismatch)?;
        self.persist_handoff_exact(&mut store, &next, arrived)
    }

    fn open_store(&self) -> Result<DeliveryStateStore, DeliveryHandoffError> {
        DeliveryStateStore::open(&self.state_root).map_err(DeliveryHandoffError::Store)
    }

    fn target_for_identity(
        &self,
        identity: &DeliveryIdentity,
    ) -> Result<VerifiedDeliveryProcessTarget, DeliveryHandoffError> {
        let staged = reopen_staged_delivery(&self.staging_root, identity)
            .map_err(DeliveryHandoffError::Staging)?;
        VerifiedDeliveryProcessTarget::from_staged(&staged)
    }

    fn verify_current_active_source(
        &self,
        state: &DeliveryState,
        current_executable: &Path,
    ) -> Result<(), DeliveryHandoffError> {
        let active = state.active().ok_or(DeliveryHandoffError::NoActiveSource)?;
        if !state.active_health_confirmed() {
            return Err(DeliveryHandoffError::StateMismatch);
        }
        self.verify_current_identity(active, current_executable)
    }

    fn verify_current_identity(
        &self,
        identity: &DeliveryIdentity,
        current_executable: &Path,
    ) -> Result<(), DeliveryHandoffError> {
        let expected = self.target_for_identity(identity)?;
        let current = canonical_regular_executable(current_executable)?;
        if current != expected.profile_bridge_executable {
            return Err(DeliveryHandoffError::CurrentExecutableMismatch);
        }
        Ok(())
    }

    fn persist_handoff_exact(
        &self,
        store: &mut DeliveryStateStore,
        state: &DeliveryState,
        evidence: DeliveryHandoffEvidence,
    ) -> Result<(), DeliveryHandoffError> {
        let persistence_error = store.persist_handoff(state, evidence.clone()).err();
        let reopened = DeliveryStateStore::open(&self.state_root).map_err(|_| {
            persistence_error.map_or(
                DeliveryHandoffError::DurableCommit,
                DeliveryHandoffError::Store,
            )
        })?;
        if reopened.state() != state || reopened.handoff() != Some(&evidence) {
            return Err(persistence_error.map_or(
                DeliveryHandoffError::DurableCommit,
                DeliveryHandoffError::Store,
            ));
        }
        Ok(())
    }
}

fn canonical_regular_executable(path: &Path) -> Result<PathBuf, DeliveryHandoffError> {
    if !path.is_absolute() {
        return Err(DeliveryHandoffError::InvalidTarget);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DeliveryHandoffError::InvalidTarget);
    }
    let canonical = fs::canonicalize(path).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
    let canonical_metadata =
        fs::symlink_metadata(&canonical).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
    if canonical_metadata.file_type().is_symlink() || !canonical_metadata.is_file() {
        return Err(DeliveryHandoffError::InvalidTarget);
    }
    Ok(canonical)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryHandoffError {
    InvalidTarget,
    InvalidCurrentProcess,
    InvalidInstalledLayout,
    CurrentExecutableMismatch,
    NoActiveSource,
    NoStagedTarget,
    NoStartedHandoff,
    HandoffAlreadyStarted,
    HelperLaunchFailed,
    StateMismatch,
    DurableCommit,
    Staging(DeliveryStagingError),
    Store(DeliveryStateStoreError),
    State(DeliveryStateError),
}

impl fmt::Display for DeliveryHandoffError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTarget => "Windows delivery handoff target is invalid",
            Self::InvalidCurrentProcess => "Windows delivery handoff current process is invalid",
            Self::InvalidInstalledLayout => "Windows delivery handoff installed layout is invalid",
            Self::CurrentExecutableMismatch => {
                "Windows delivery handoff current executable identity is inconsistent"
            }
            Self::NoActiveSource => "Windows delivery handoff has no active source release",
            Self::NoStagedTarget => "Windows delivery handoff has no staged target release",
            Self::NoStartedHandoff => "Windows delivery handoff has no started transfer",
            Self::HandoffAlreadyStarted => "Windows delivery handoff transfer is already started",
            Self::HelperLaunchFailed => "Windows delivery one-shot helper could not be started",
            Self::StateMismatch => "Windows delivery handoff state identity is inconsistent",
            Self::DurableCommit => "Windows delivery handoff evidence did not commit durably",
            Self::Staging(_) => "Windows delivery handoff staged release is invalid",
            Self::Store(_) => "Windows delivery handoff state store is unavailable",
            Self::State(_) => "Windows delivery handoff state transition was rejected",
        })
    }
}

impl std::error::Error for DeliveryHandoffError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_profile::{DeliveryActivationGuard, MaterializationRoot};
    use crate::windows_delivery::{
        DetachedSignatureEnvelope, DetachedSignatureVerifier, TrustedSigner, TrustedSignerSet,
        TrustedSignerStatus, WindowsDeliveryCompatibility, WindowsDeliveryComponent,
        WindowsDeliveryComponents, WindowsDeliveryEvidence, WindowsDeliveryManifest,
        verify_delivery_candidate,
    };
    use crate::windows_delivery_recovery::{
        DeliveryRecoveryCoordinator, DeliveryRecoveryDisposition, DeliveryRecoveryReason,
    };
    use crate::windows_delivery_staging::{
        DeliveryArchiveEntry, DeliveryArchiveReader, DeliveryComponentKind, stage_verified_delivery,
    };
    use bridge_domain::CAMOUHOST_IPC_VERSION;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::io::{self, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);
    const CERTIFICATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-handoff-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct AcceptVerifier;

    impl DetachedSignatureVerifier for AcceptVerifier {
        type Error = ();

        fn verify_cms(
            &mut self,
            _manifest_bytes: &[u8],
            _cms_der: &[u8],
            _expected_certificate_sha256: &str,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct MemoryArchiveReader {
        files: HashMap<DeliveryComponentKind, Vec<(String, Vec<u8>)>>,
    }

    impl MemoryArchiveReader {
        fn for_release(label: &str) -> Self {
            let mut files = HashMap::new();
            files.insert(
                DeliveryComponentKind::ProfileBridge,
                vec![(
                    PROFILE_BRIDGE_EXECUTABLE.to_owned(),
                    format!("profile-bridge-{label}").into_bytes(),
                )],
            );
            files.insert(
                DeliveryComponentKind::RuntimeBundle,
                vec![(
                    "runtime.txt".to_owned(),
                    format!("runtime-{label}").into_bytes(),
                )],
            );
            Self { files }
        }
    }

    impl DeliveryArchiveReader for MemoryArchiveReader {
        type Error = ();

        fn entries(
            &mut self,
            component: DeliveryComponentKind,
            _artifact_path: &Path,
        ) -> Result<Vec<DeliveryArchiveEntry>, Self::Error> {
            Ok(self.files[&component]
                .iter()
                .map(|(path, bytes)| {
                    DeliveryArchiveEntry::regular_file(path, bytes.len() as u64, sha256_hex(bytes))
                })
                .collect())
        }

        fn copy_regular_file(
            &mut self,
            component: DeliveryComponentKind,
            _artifact_path: &Path,
            entry_index: usize,
            writer: &mut dyn Write,
        ) -> Result<(), Self::Error> {
            writer
                .write_all(&self.files[&component][entry_index].1)
                .map_err(|_| ())
        }
    }

    struct Fixture {
        _directory: TestDirectory,
        staging: DeliveryStagingRoot,
        state_root: PathBuf,
        materialization: MaterializationRoot,
        first: DeliveryIdentity,
        second: DeliveryIdentity,
        first_executable: PathBuf,
        second_executable: PathBuf,
    }

    fn fixture(label: &str, second_started: bool) -> Result<Fixture, Box<dyn std::error::Error>> {
        let directory = TestDirectory::create(label)?;
        let releases = directory.0.join(RELEASES_DIRECTORY);
        fs::create_dir(&releases)?;
        let staging = DeliveryStagingRoot::open_or_create(&releases)?;
        let state_root = directory.0.join(STATE_DIRECTORY);
        let mut store = DeliveryStateStore::initialize(&state_root)?;
        let materialization = MaterializationRoot::open_or_create(directory.0.join("profiles"))?;

        let (first_candidate, first_bridge, first_runtime) = candidate(&directory.0, 1, '1')?;
        let (second_candidate, second_bridge, second_runtime) = candidate(&directory.0, 2, '2')?;
        let mut first_reader = MemoryArchiveReader::for_release("first");
        let first_stage = stage_verified_delivery(
            &staging,
            &first_candidate,
            &first_bridge,
            &first_runtime,
            &mut first_reader,
        )?;
        let mut second_reader = MemoryArchiveReader::for_release("second");
        let second_stage = stage_verified_delivery(
            &staging,
            &second_candidate,
            &second_bridge,
            &second_runtime,
            &mut second_reader,
        )?;
        let first = first_candidate.identity();
        let second = second_candidate.identity();
        let first_executable = fs::canonicalize(
            first_stage
                .profile_bridge_root()
                .join(PROFILE_BRIDGE_EXECUTABLE),
        )?;
        let second_executable = fs::canonicalize(
            second_stage
                .profile_bridge_root()
                .join(PROFILE_BRIDGE_EXECUTABLE),
        )?;

        let mut state = DeliveryState::default();
        state.stage(&first_candidate)?;
        state.activate_staged(true)?;
        state.start_health_attempt(&first, 1)?;
        state.confirm_health()?;
        state.stage(&second_candidate)?;
        if second_started {
            state.activate_staged(true)?;
            state.start_health_attempt(&second, 2)?;
        }
        store.persist(&state)?;

        Ok(Fixture {
            _directory: directory,
            staging,
            state_root,
            materialization,
            first,
            second,
            first_executable,
            second_executable,
        })
    }

    fn trust() -> Result<TrustedSignerSet, Box<dyn std::error::Error>> {
        Ok(TrustedSignerSet::new([TrustedSigner::new(
            "test-active",
            CERTIFICATE,
            TrustedSignerStatus::Active,
        )?])?)
    }

    fn candidate(
        root: &Path,
        sequence: u64,
        digit: char,
    ) -> Result<
        (
            crate::windows_delivery::VerifiedDeliveryCandidate,
            PathBuf,
            PathBuf,
        ),
        Box<dyn std::error::Error>,
    > {
        let bridge_bytes = format!("bridge-artifact-{sequence}").into_bytes();
        let runtime_bytes = format!("runtime-artifact-{sequence}").into_bytes();
        let bridge_path = root.join(format!("bridge-{sequence}.bin"));
        let runtime_path = root.join(format!("runtime-{sequence}.bin"));
        fs::write(&bridge_path, &bridge_bytes)?;
        fs::write(&runtime_path, &runtime_bytes)?;
        let manifest = WindowsDeliveryManifest {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY".to_owned(),
            release_set_id: format!("release-set-v3-sha256-{}", digit.to_string().repeat(64)),
            sequence,
            source_commit_sha: digit.to_string().repeat(40),
            components: WindowsDeliveryComponents {
                profile_bridge: WindowsDeliveryComponent {
                    release_id: format!(
                        "profile-bridge-v2-sha256-{}",
                        digit.to_string().repeat(64)
                    ),
                    artifact_sha256: sha256_hex(&bridge_bytes),
                    artifact_size_bytes: bridge_bytes.len() as u64,
                    component_manifest_sha256: "a".repeat(64),
                },
                runtime_bundle: WindowsDeliveryComponent {
                    release_id: format!(
                        "runtime-bundle-v2-sha256-{}",
                        digit.to_string().repeat(64)
                    ),
                    artifact_sha256: sha256_hex(&runtime_bytes),
                    artifact_size_bytes: runtime_bytes.len() as u64,
                    component_manifest_sha256: "b".repeat(64),
                },
            },
            evidence: WindowsDeliveryEvidence {
                sbom_sha256: "c".repeat(64),
                provenance_sha256: "d".repeat(64),
            },
            compatibility: WindowsDeliveryCompatibility {
                profile_bridge_protocol_version: 1,
                camouhost_ipc_version: CAMOUHOST_IPC_VERSION,
                runtime_bundle_version: "2.0.0".to_owned(),
            },
        };
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        let signature = serde_json::to_vec(&DetachedSignatureEnvelope {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS".to_owned(),
            key_id: "test-active".to_owned(),
            cms_der_hex: "00".to_owned(),
        })?;
        let mut verifier = AcceptVerifier;
        let candidate =
            verify_delivery_candidate(&manifest_bytes, &signature, &trust()?, None, &mut verifier)?;
        Ok((candidate, bridge_path, runtime_path))
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut encoded, "{byte:02x}");
        }
        encoded
    }

    #[test]
    fn one_shot_plan_binds_exact_old_pid_executable_and_target_without_claim()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture("plan", false)?;
        let target = DeliveryHandoffCoordinator::new(fixture.staging, &fixture.state_root)
            .target_for_identity(&fixture.second)?;
        let handoff = OneShotDeliveryHandoff::new(42, &fixture.first_executable, target)?;
        let command = handoff.windows_helper_command();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let envs = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(command.get_program(), "powershell.exe");
        assert!(args.iter().any(|arg| arg == POWERSHELL_HANDOFF));
        assert!(
            args.iter()
                .all(|arg| !arg.contains("profilebridge://claim"))
        );
        assert!(envs.iter().any(|(key, value)| {
            key == CURRENT_EXECUTABLE_ENV
                && value.as_deref() == Some(fixture.first_executable.to_string_lossy().as_ref())
        }));
        assert_eq!(handoff.current_process_id(), 42);
        assert_eq!(handoff.target().identity(), &fixture.second);
        Ok(())
    }

    #[test]
    fn activation_stays_on_old_healthy_release_until_exact_successor_arrives()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture("activation", false)?;
        let coordinator =
            DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        let handoff = coordinator.prepare_activation(&guard, 42, &fixture.first_executable)?;
        assert_eq!(handoff.target().identity(), &fixture.second);
        let after_start = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(after_start.state().active(), Some(&fixture.first));
        assert!(after_start.state().active_health_confirmed());
        assert_eq!(after_start.state().staged(), Some(&fixture.second));
        assert_eq!(
            after_start.handoff().map(DeliveryHandoffEvidence::outcome),
            Some(DeliveryHandoffOutcome::Started)
        );
        drop(guard);

        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        coordinator.complete_arrival(&guard, &fixture.second_executable)?;
        let arrived = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(arrived.state().active(), Some(&fixture.second));
        assert!(!arrived.state().active_health_confirmed());
        assert_eq!(arrived.state().activation_generation(), 2);
        assert_eq!(
            arrived.handoff().map(DeliveryHandoffEvidence::outcome),
            Some(DeliveryHandoffOutcome::Arrived)
        );
        Ok(())
    }

    #[test]
    fn interrupted_started_activation_resumes_same_exact_target_without_state_switch()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture("resume", false)?;
        let coordinator =
            DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        coordinator.prepare_activation(&guard, 42, &fixture.first_executable)?;
        let resumed = coordinator
            .resume_started(&guard, 43, &fixture.first_executable)?
            .ok_or("expected resumable handoff")?;
        assert_eq!(resumed.target().identity(), &fixture.second);
        let state = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(state.state().active(), Some(&fixture.first));
        assert_eq!(state.state().staged(), Some(&fixture.second));
        Ok(())
    }

    #[test]
    fn recovery_handoff_consumes_only_r8_verified_lkg_target()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture("recovery", true)?;
        let recovery =
            DeliveryRecoveryCoordinator::new(fixture.staging.clone(), &fixture.state_root);
        let disposition = recovery.recover(
            &fixture.second,
            2,
            DeliveryRecoveryReason::InterruptedActivation,
        )?;
        let DeliveryRecoveryDisposition::Handoff(recovery_target) = disposition else {
            return Err("expected exact LKG recovery target".into());
        };
        let coordinator =
            DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        let handoff = coordinator.prepare_recovery(
            &guard,
            &fixture.second,
            2,
            &recovery_target,
            44,
            &fixture.second_executable,
        )?;
        assert_eq!(handoff.target().identity(), &fixture.first);
        drop(guard);

        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        coordinator.complete_arrival(&guard, &fixture.first_executable)?;
        let arrived = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(arrived.state().active(), Some(&fixture.first));
        assert!(arrived.state().active_health_confirmed());
        assert_eq!(
            arrived.handoff().map(DeliveryHandoffEvidence::kind),
            Some(DeliveryHandoffKind::Recovery)
        );
        assert_eq!(
            arrived.handoff().map(DeliveryHandoffEvidence::outcome),
            Some(DeliveryHandoffOutcome::Arrived)
        );
        Ok(())
    }

    #[test]
    fn target_or_current_executable_substitution_fails_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture("reject", false)?;
        let coordinator =
            DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        assert_eq!(
            coordinator.prepare_activation(&guard, 42, &fixture.second_executable),
            Err(DeliveryHandoffError::CurrentExecutableMismatch)
        );
        assert_eq!(
            coordinator.complete_arrival(&guard, &fixture.first_executable),
            Err(DeliveryHandoffError::NoStartedHandoff)
        );
        Ok(())
    }
}
