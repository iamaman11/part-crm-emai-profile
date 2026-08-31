#![forbid(unsafe_code)]

use crate::windows_delivery::{
    DeliveryActivationOutcome, DeliveryFailureKind, DeliveryIdentity, DeliveryState,
    DeliveryStateError,
};
use crate::windows_delivery_staging::{
    DeliveryStagingRoot, StagedDelivery, reopen_staged_delivery,
};
use crate::windows_delivery_store::{
    DeliveryHandoffKind, DeliveryHandoffOutcome, DeliveryStateStore, DeliveryStateStoreError,
};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_BRIDGE_EXECUTABLE: &str = "profile-bridge.exe";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryRecoveryReason {
    HealthRejected,
    InterruptedActivation,
    InterruptedHandoff,
}

impl DeliveryRecoveryReason {
    const fn failure_kind(self) -> DeliveryFailureKind {
        match self {
            Self::HealthRejected => DeliveryFailureKind::HealthRejected,
            Self::InterruptedActivation => DeliveryFailureKind::InterruptedActivation,
            Self::InterruptedHandoff => DeliveryFailureKind::InterruptedHandoff,
        }
    }

    const fn expected_activation_outcome(self) -> DeliveryActivationOutcome {
        match self {
            Self::HealthRejected | Self::InterruptedActivation => {
                DeliveryActivationOutcome::HealthAttemptStarted
            }
            Self::InterruptedHandoff => DeliveryActivationOutcome::PendingHealth,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryRecoveryDisposition {
    Handoff(VerifiedDeliveryRecoveryTarget),
    RecoveryRequired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDeliveryRecoveryTarget {
    identity: DeliveryIdentity,
    release_root: PathBuf,
    profile_bridge_executable: PathBuf,
}

impl VerifiedDeliveryRecoveryTarget {
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

/// Filesystem/durable-state recovery transaction for one failed Windows delivery activation.
///
/// The canonical `DeliveryState` decides the transition. This effect shell verifies an exact staged
/// LKG before committing state that points to it. It deliberately does not spawn a process or carry a
/// claim credential; R9 consumes the returned exact executable target through the one-shot same-binary
/// handoff owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryRecoveryCoordinator {
    staging_root: DeliveryStagingRoot,
    state_root: PathBuf,
}

impl DeliveryRecoveryCoordinator {
    #[must_use]
    pub fn new(staging_root: DeliveryStagingRoot, state_root: impl Into<PathBuf>) -> Self {
        Self {
            staging_root,
            state_root: state_root.into(),
        }
    }

    pub fn recover(
        &self,
        expected_candidate: &DeliveryIdentity,
        expected_attempt: u64,
        reason: DeliveryRecoveryReason,
    ) -> Result<DeliveryRecoveryDisposition, DeliveryRecoveryError> {
        let mut store = DeliveryStateStore::open(&self.state_root)
            .map_err(DeliveryRecoveryError::StateStore)?;
        validate_exact_started_attempt(&store, expected_candidate, expected_attempt, reason)?;

        let mut next = store.state().clone();
        let transition = match reason {
            DeliveryRecoveryReason::HealthRejected => next.fail_health_and_rollback(),
            DeliveryRecoveryReason::InterruptedActivation => next.recover_interrupted_activation(),
            DeliveryRecoveryReason::InterruptedHandoff => next.recover_interrupted_handoff(),
        };
        let recovery_required = match transition {
            Ok(()) => false,
            Err(DeliveryStateError::RecoveryRequired) => true,
            Err(error) => return Err(DeliveryRecoveryError::State(error)),
        };
        next.validate_persisted()
            .map_err(DeliveryRecoveryError::State)?;

        let target = if recovery_required {
            if next.active().is_some() || next.active_health_confirmed() {
                return Err(DeliveryRecoveryError::StateMismatch);
            }
            None
        } else {
            let lkg = next
                .active()
                .cloned()
                .ok_or(DeliveryRecoveryError::StateMismatch)?;
            if !next.active_health_confirmed() || next.last_known_good().is_some() {
                return Err(DeliveryRecoveryError::StateMismatch);
            }
            Some(verify_recovery_target(&self.staging_root, &lkg)?)
        };

        verify_recovery_evidence(&next, expected_candidate, expected_attempt, reason)?;
        persist_exact(&self.state_root, &mut store, &next)?;

        match target {
            Some(target) => Ok(DeliveryRecoveryDisposition::Handoff(target)),
            None => Ok(DeliveryRecoveryDisposition::RecoveryRequired),
        }
    }
}

fn validate_exact_started_attempt(
    store: &DeliveryStateStore,
    candidate: &DeliveryIdentity,
    attempt: u64,
    reason: DeliveryRecoveryReason,
) -> Result<(), DeliveryRecoveryError> {
    let state = store.state();
    state
        .validate_persisted()
        .map_err(DeliveryRecoveryError::State)?;
    if attempt == 0
        || state.active() != Some(candidate)
        || state.active_health_confirmed()
        || state.activation_generation() != attempt
    {
        return Err(DeliveryRecoveryError::StateMismatch);
    }
    let activation = state
        .last_activation()
        .ok_or(DeliveryRecoveryError::StateMismatch)?;
    if activation.attempt != attempt
        || activation.candidate != *candidate
        || activation.outcome != reason.expected_activation_outcome()
    {
        return Err(DeliveryRecoveryError::StateMismatch);
    }
    if reason == DeliveryRecoveryReason::InterruptedHandoff {
        let handoff = store
            .handoff()
            .ok_or(DeliveryRecoveryError::StateMismatch)?;
        if handoff.kind() != DeliveryHandoffKind::Activation
            || handoff.outcome() != DeliveryHandoffOutcome::Started
            || handoff.source_candidate() != candidate
            || handoff.source_attempt() != attempt
            || handoff.target() != candidate
        {
            return Err(DeliveryRecoveryError::StateMismatch);
        }
    }
    Ok(())
}

fn verify_recovery_evidence(
    state: &DeliveryState,
    failed: &DeliveryIdentity,
    attempt: u64,
    reason: DeliveryRecoveryReason,
) -> Result<(), DeliveryRecoveryError> {
    let activation = state
        .last_activation()
        .ok_or(DeliveryRecoveryError::StateMismatch)?;
    let failure = state
        .last_failure()
        .ok_or(DeliveryRecoveryError::StateMismatch)?;
    if activation.attempt != attempt
        || activation.candidate != *failed
        || failure.candidate != *failed
        || failure.kind != reason.failure_kind()
    {
        return Err(DeliveryRecoveryError::StateMismatch);
    }
    let expected_outcome = if state.active().is_some() {
        DeliveryActivationOutcome::RolledBack
    } else {
        DeliveryActivationOutcome::RecoveryRequired
    };
    if activation.outcome != expected_outcome {
        return Err(DeliveryRecoveryError::StateMismatch);
    }
    Ok(())
}

fn verify_recovery_target(
    staging_root: &DeliveryStagingRoot,
    identity: &DeliveryIdentity,
) -> Result<VerifiedDeliveryRecoveryTarget, DeliveryRecoveryError> {
    let staged = reopen_staged_delivery(staging_root, identity)
        .map_err(|_| DeliveryRecoveryError::StagedRelease)?;
    verified_target_from_stage(staged)
}

fn verified_target_from_stage(
    staged: StagedDelivery,
) -> Result<VerifiedDeliveryRecoveryTarget, DeliveryRecoveryError> {
    let release_root =
        fs::canonicalize(staged.path()).map_err(|_| DeliveryRecoveryError::InvalidExecutable)?;
    let executable = staged.profile_bridge_root().join(PROFILE_BRIDGE_EXECUTABLE);
    let metadata =
        fs::symlink_metadata(&executable).map_err(|_| DeliveryRecoveryError::InvalidExecutable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DeliveryRecoveryError::InvalidExecutable);
    }
    let executable =
        fs::canonicalize(executable).map_err(|_| DeliveryRecoveryError::InvalidExecutable)?;
    if !executable.starts_with(&release_root) {
        return Err(DeliveryRecoveryError::InvalidExecutable);
    }
    Ok(VerifiedDeliveryRecoveryTarget {
        identity: staged.identity().clone(),
        release_root,
        profile_bridge_executable: executable,
    })
}

fn persist_exact(
    state_root: &Path,
    store: &mut DeliveryStateStore,
    expected: &DeliveryState,
) -> Result<(), DeliveryRecoveryError> {
    if let Err(error) = store.persist(expected) {
        let reopened = DeliveryStateStore::open(state_root)
            .map_err(|_| DeliveryRecoveryError::StateStore(error))?;
        if reopened.state() != expected {
            return Err(DeliveryRecoveryError::StateStore(error));
        }
        return Ok(());
    }
    let reopened =
        DeliveryStateStore::open(state_root).map_err(DeliveryRecoveryError::StateStore)?;
    if reopened.state() != expected {
        return Err(DeliveryRecoveryError::DurableCommit);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryRecoveryError {
    StateStore(DeliveryStateStoreError),
    State(DeliveryStateError),
    StateMismatch,
    StagedRelease,
    InvalidExecutable,
    DurableCommit,
}

impl fmt::Display for DeliveryRecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StateStore(_) => "Windows delivery recovery state store is unavailable",
            Self::State(_) => "Windows delivery recovery state transition was rejected",
            Self::StateMismatch => "Windows delivery recovery attempt identity is inconsistent",
            Self::StagedRelease => "Windows delivery last known good stage is invalid",
            Self::InvalidExecutable => "Windows delivery recovery executable is invalid",
            Self::DurableCommit => "Windows delivery recovery state did not commit durably",
        })
    }
}

impl std::error::Error for DeliveryRecoveryError {}

#[cfg(test)]
mod tests {
    use super::{
        DeliveryRecoveryCoordinator, DeliveryRecoveryDisposition, DeliveryRecoveryError,
        DeliveryRecoveryReason,
    };
    use crate::windows_delivery::{
        DeliveryActivationOutcome, DeliveryFailureKind, DeliveryIdentity, DeliveryState,
        DetachedSignatureVerifier, TrustedSigner, TrustedSignerSet, TrustedSignerStatus,
        WindowsDeliveryCompatibility, WindowsDeliveryComponent, WindowsDeliveryComponents,
        WindowsDeliveryEvidence, WindowsDeliveryManifest, verify_delivery_candidate,
    };
    use crate::windows_delivery_staging::{
        DeliveryArchiveEntry, DeliveryArchiveReader, DeliveryComponentKind, DeliveryStagingRoot,
        stage_verified_delivery,
    };
    use crate::windows_delivery_store::{DeliveryHandoffEvidence, DeliveryStateStore};
    use bridge_domain::CAMOUHOST_IPC_VERSION;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);
    const CERTIFICATE_SHA256: &str =
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, std::io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-recovery-{label}-{}-{sequence}",
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

    struct DigestVerifier;

    impl DetachedSignatureVerifier for DigestVerifier {
        type Error = ();

        fn verify_cms(
            &mut self,
            manifest_bytes: &[u8],
            cms_der: &[u8],
            expected_certificate_sha256: &str,
        ) -> Result<bool, Self::Error> {
            Ok(cms_der == Sha256::digest(manifest_bytes).as_slice()
                && expected_certificate_sha256 == CERTIFICATE_SHA256)
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
                    "profile-bridge.exe".to_owned(),
                    format!("profile-bridge-{label}").into_bytes(),
                )],
            );
            files.insert(
                DeliveryComponentKind::RuntimeBundle,
                vec![(
                    "camouhost/real.py".to_owned(),
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
        first: DeliveryIdentity,
        second: DeliveryIdentity,
        first_stage: PathBuf,
    }

    fn fixture_with_lkg(label: &str) -> Result<Fixture, Box<dyn std::error::Error>> {
        fixture_with_lkg_state(label, true)
    }

    fn fixture_with_lkg_state(
        label: &str,
        start_second: bool,
    ) -> Result<Fixture, Box<dyn std::error::Error>> {
        let directory = TestDirectory::create(label)?;
        let staging = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let state_root = directory.0.join("state");
        let mut store = DeliveryStateStore::initialize(&state_root)?;
        let first_candidate = candidate(&directory.0, 1, 'a', "first")?;
        let second_candidate = candidate(&directory.0, 2, 'b', "second")?;
        let first_identity = first_candidate.0.identity();
        let second_identity = second_candidate.0.identity();
        let mut first_reader = MemoryArchiveReader::for_release("first");
        let first_stage = stage_verified_delivery(
            &staging,
            &first_candidate.0,
            &first_candidate.1,
            &first_candidate.2,
            &mut first_reader,
        )?;
        let mut second_reader = MemoryArchiveReader::for_release("second");
        stage_verified_delivery(
            &staging,
            &second_candidate.0,
            &second_candidate.1,
            &second_candidate.2,
            &mut second_reader,
        )?;

        let mut state = DeliveryState::default();
        state.stage(&first_candidate.0)?;
        state.activate_staged(true)?;
        state.start_health_attempt(&first_identity, 1)?;
        state.confirm_health()?;
        state.stage(&second_candidate.0)?;
        state.activate_staged(true)?;
        if start_second {
            state.start_health_attempt(&second_identity, 2)?;
        }
        store.persist(&state)?;

        Ok(Fixture {
            _directory: directory,
            staging,
            state_root,
            first: first_identity,
            second: second_identity,
            first_stage: first_stage.path().to_path_buf(),
        })
    }

    fn persist_started_activation_handoff(
        state_root: &Path,
        candidate: &DeliveryIdentity,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut store = DeliveryStateStore::open(state_root)?;
        let evidence = DeliveryHandoffEvidence::activation_started(store.state(), candidate)?;
        let state = store.state().clone();
        store.persist_handoff(&state, evidence)?;
        Ok(())
    }

    #[test]
    fn failed_health_rolls_back_only_after_exact_lkg_stage_reopens()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture_with_lkg("health")?;
        let recovery = DeliveryRecoveryCoordinator::new(fixture.staging, &fixture.state_root);
        let disposition =
            recovery.recover(&fixture.second, 2, DeliveryRecoveryReason::HealthRejected)?;
        let DeliveryRecoveryDisposition::Handoff(target) = disposition else {
            return Err("expected verified LKG handoff".into());
        };
        assert_eq!(target.identity(), &fixture.first);
        assert!(target.release_root().is_dir());
        assert_eq!(
            target
                .profile_bridge_executable()
                .file_name()
                .and_then(|value| value.to_str()),
            Some("profile-bridge.exe")
        );
        assert!(target.profile_bridge_executable().is_file());

        let reopened = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(reopened.state().active(), Some(&fixture.first));
        assert!(reopened.state().active_health_confirmed());
        assert_eq!(
            reopened.state().last_failure().map(|value| value.kind),
            Some(DeliveryFailureKind::HealthRejected)
        );
        assert_eq!(
            reopened
                .state()
                .last_activation()
                .map(|value| value.outcome),
            Some(DeliveryActivationOutcome::RolledBack)
        );
        Ok(())
    }

    #[test]
    fn corrupted_lkg_stage_prevents_rollback_state_commit() -> Result<(), Box<dyn std::error::Error>>
    {
        let fixture = fixture_with_lkg("corrupt-lkg")?;
        fs::remove_dir_all(&fixture.first_stage)?;
        let before = DeliveryStateStore::open(&fixture.state_root)?
            .state()
            .clone();
        let recovery = DeliveryRecoveryCoordinator::new(fixture.staging, &fixture.state_root);
        assert_eq!(
            recovery.recover(&fixture.second, 2, DeliveryRecoveryReason::HealthRejected,),
            Err(DeliveryRecoveryError::StagedRelease)
        );
        let reopened = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(reopened.state(), &before);
        assert_eq!(reopened.state().active(), Some(&fixture.second));
        assert!(!reopened.state().active_health_confirmed());
        Ok(())
    }

    #[test]
    fn first_install_failure_persists_recovery_required_without_fake_target()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::create("first-install")?;
        let staging = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let state_root = directory.0.join("state");
        let mut store = DeliveryStateStore::initialize(&state_root)?;
        let first_candidate = candidate(&directory.0, 1, 'd', "first-only")?;
        let identity = first_candidate.0.identity();
        let mut reader = MemoryArchiveReader::for_release("first-only");
        stage_verified_delivery(
            &staging,
            &first_candidate.0,
            &first_candidate.1,
            &first_candidate.2,
            &mut reader,
        )?;
        let mut state = DeliveryState::default();
        state.stage(&first_candidate.0)?;
        state.activate_staged(true)?;
        state.start_health_attempt(&identity, 1)?;
        store.persist(&state)?;

        let recovery = DeliveryRecoveryCoordinator::new(staging, &state_root);
        assert_eq!(
            recovery.recover(&identity, 1, DeliveryRecoveryReason::HealthRejected)?,
            DeliveryRecoveryDisposition::RecoveryRequired
        );
        let reopened = DeliveryStateStore::open(&state_root)?;
        assert!(reopened.state().active().is_none());
        assert!(!reopened.state().active_health_confirmed());
        assert_eq!(
            reopened.state().last_failure().map(|value| value.kind),
            Some(DeliveryFailureKind::HealthRejected)
        );
        assert_eq!(
            reopened
                .state()
                .last_activation()
                .map(|value| value.outcome),
            Some(DeliveryActivationOutcome::RecoveryRequired)
        );
        Ok(())
    }

    #[test]
    fn interrupted_activation_uses_the_same_verified_lkg_transaction()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture_with_lkg("interrupted")?;
        let recovery = DeliveryRecoveryCoordinator::new(fixture.staging, &fixture.state_root);
        let disposition = recovery.recover(
            &fixture.second,
            2,
            DeliveryRecoveryReason::InterruptedActivation,
        )?;
        let DeliveryRecoveryDisposition::Handoff(target) = disposition else {
            return Err("expected interrupted activation LKG handoff".into());
        };
        assert_eq!(target.identity(), &fixture.first);
        let reopened = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(
            reopened.state().last_failure().map(|value| value.kind),
            Some(DeliveryFailureKind::InterruptedActivation)
        );
        Ok(())
    }

    #[test]
    fn interrupted_handoff_requires_exact_started_evidence_and_uses_verified_lkg()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture_with_lkg_state("handoff-interrupted", false)?;
        persist_started_activation_handoff(&fixture.state_root, &fixture.second)?;
        let recovery = DeliveryRecoveryCoordinator::new(fixture.staging, &fixture.state_root);
        let disposition = recovery.recover(
            &fixture.second,
            2,
            DeliveryRecoveryReason::InterruptedHandoff,
        )?;
        let DeliveryRecoveryDisposition::Handoff(target) = disposition else {
            return Err("expected exact LKG handoff target".into());
        };
        assert_eq!(target.identity(), &fixture.first);
        let reopened = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(reopened.state().active(), Some(&fixture.first));
        assert!(reopened.state().active_health_confirmed());
        assert!(reopened.handoff().is_none());
        assert_eq!(
            reopened.state().last_failure().map(|value| value.kind),
            Some(DeliveryFailureKind::InterruptedHandoff)
        );
        Ok(())
    }

    #[test]
    fn untouched_pending_cannot_masquerade_as_interrupted_handoff()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture_with_lkg_state("handoff-untouched", false)?;
        let before = DeliveryStateStore::open(&fixture.state_root)?
            .state()
            .clone();
        let recovery = DeliveryRecoveryCoordinator::new(fixture.staging, &fixture.state_root);
        assert_eq!(
            recovery.recover(
                &fixture.second,
                2,
                DeliveryRecoveryReason::InterruptedHandoff,
            ),
            Err(DeliveryRecoveryError::StateMismatch)
        );
        assert_eq!(
            DeliveryStateStore::open(&fixture.state_root)?.state(),
            &before
        );
        Ok(())
    }

    #[test]
    fn corrupted_lkg_stage_blocks_interrupted_handoff_rollback_commit()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture_with_lkg_state("handoff-corrupt-lkg", false)?;
        persist_started_activation_handoff(&fixture.state_root, &fixture.second)?;
        fs::remove_dir_all(&fixture.first_stage)?;
        let before = DeliveryStateStore::open(&fixture.state_root)?;
        let before_state = before.state().clone();
        let before_handoff = before.handoff().cloned();
        let recovery = DeliveryRecoveryCoordinator::new(fixture.staging, &fixture.state_root);
        assert_eq!(
            recovery.recover(
                &fixture.second,
                2,
                DeliveryRecoveryReason::InterruptedHandoff,
            ),
            Err(DeliveryRecoveryError::StagedRelease)
        );
        let reopened = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(reopened.state(), &before_state);
        assert_eq!(reopened.handoff(), before_handoff.as_ref());
        Ok(())
    }

    #[test]
    fn first_install_interrupted_handoff_persists_recovery_required_without_fake_target()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::create("first-install-handoff")?;
        let staging = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let state_root = directory.0.join("state");
        let mut store = DeliveryStateStore::initialize(&state_root)?;
        let first_candidate = candidate(&directory.0, 1, 'e', "first-handoff")?;
        let identity = first_candidate.0.identity();
        let mut reader = MemoryArchiveReader::for_release("first-handoff");
        stage_verified_delivery(
            &staging,
            &first_candidate.0,
            &first_candidate.1,
            &first_candidate.2,
            &mut reader,
        )?;
        let mut state = DeliveryState::default();
        state.stage(&first_candidate.0)?;
        state.activate_staged(true)?;
        let evidence = DeliveryHandoffEvidence::activation_started(&state, &identity)?;
        store.persist_handoff(&state, evidence)?;

        let recovery = DeliveryRecoveryCoordinator::new(staging, &state_root);
        assert_eq!(
            recovery.recover(&identity, 1, DeliveryRecoveryReason::InterruptedHandoff)?,
            DeliveryRecoveryDisposition::RecoveryRequired
        );
        let reopened = DeliveryStateStore::open(&state_root)?;
        assert!(reopened.state().active().is_none());
        assert!(!reopened.state().active_health_confirmed());
        assert!(reopened.handoff().is_none());
        assert_eq!(
            reopened.state().last_failure().map(|value| value.kind),
            Some(DeliveryFailureKind::InterruptedHandoff)
        );
        assert_eq!(
            reopened
                .state()
                .last_activation()
                .map(|value| value.outcome),
            Some(DeliveryActivationOutcome::RecoveryRequired)
        );
        Ok(())
    }

    #[test]
    fn untouched_pending_and_healthy_states_cannot_masquerade_as_interrupted()
    -> Result<(), Box<dyn std::error::Error>> {
        let pending = fixture_with_lkg_state("untouched-pending", false)?;
        let before = DeliveryStateStore::open(&pending.state_root)?
            .state()
            .clone();
        let recovery = DeliveryRecoveryCoordinator::new(pending.staging, &pending.state_root);
        assert_eq!(
            recovery.recover(
                &pending.second,
                2,
                DeliveryRecoveryReason::InterruptedActivation,
            ),
            Err(DeliveryRecoveryError::StateMismatch)
        );
        assert_eq!(
            DeliveryStateStore::open(&pending.state_root)?.state(),
            &before
        );

        let healthy = fixture_with_lkg("healthy")?;
        let mut store = DeliveryStateStore::open(&healthy.state_root)?;
        let mut state = store.state().clone();
        state.confirm_health()?;
        store.persist(&state)?;
        let before = DeliveryStateStore::open(&healthy.state_root)?
            .state()
            .clone();
        let recovery = DeliveryRecoveryCoordinator::new(healthy.staging, &healthy.state_root);
        assert_eq!(
            recovery.recover(
                &healthy.second,
                2,
                DeliveryRecoveryReason::InterruptedActivation,
            ),
            Err(DeliveryRecoveryError::StateMismatch)
        );
        assert_eq!(
            DeliveryStateStore::open(&healthy.state_root)?.state(),
            &before
        );
        Ok(())
    }

    #[test]
    fn candidate_or_attempt_substitution_fails_before_mutation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture_with_lkg("substitution")?;
        let before = DeliveryStateStore::open(&fixture.state_root)?
            .state()
            .clone();
        let recovery = DeliveryRecoveryCoordinator::new(fixture.staging, &fixture.state_root);
        assert_eq!(
            recovery.recover(&fixture.first, 2, DeliveryRecoveryReason::HealthRejected,),
            Err(DeliveryRecoveryError::StateMismatch)
        );
        assert_eq!(
            recovery.recover(&fixture.second, 1, DeliveryRecoveryReason::HealthRejected,),
            Err(DeliveryRecoveryError::StateMismatch)
        );
        assert_eq!(
            DeliveryStateStore::open(&fixture.state_root)?.state(),
            &before
        );
        Ok(())
    }

    type CandidateArtifacts = (
        crate::windows_delivery::VerifiedDeliveryCandidate,
        PathBuf,
        PathBuf,
    );

    fn candidate(
        root: &Path,
        sequence: u64,
        suffix: char,
        label: &str,
    ) -> Result<CandidateArtifacts, Box<dyn std::error::Error>> {
        let bridge_artifact = root.join(format!("{label}-bridge.zip"));
        let runtime_artifact = root.join(format!("{label}-runtime.tar"));
        let bridge_archive_bytes = format!("bridge-archive-{label}").into_bytes();
        let runtime_archive_bytes = format!("runtime-archive-{label}").into_bytes();
        fs::write(&bridge_artifact, &bridge_archive_bytes)?;
        fs::write(&runtime_artifact, &runtime_archive_bytes)?;
        let identity_digest: String = std::iter::repeat_n(suffix, 64).collect();
        let manifest = WindowsDeliveryManifest {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY".to_owned(),
            release_set_id: format!("release-set-v3-sha256-{identity_digest}"),
            sequence,
            source_commit_sha: "1".repeat(40),
            components: WindowsDeliveryComponents {
                profile_bridge: WindowsDeliveryComponent {
                    release_id: format!("profile-bridge-v2-sha256-{identity_digest}"),
                    artifact_sha256: sha256_hex(&bridge_archive_bytes),
                    artifact_size_bytes: bridge_archive_bytes.len() as u64,
                    component_manifest_sha256: identity_digest.clone(),
                },
                runtime_bundle: WindowsDeliveryComponent {
                    release_id: format!("runtime-bundle-v2-sha256-{identity_digest}"),
                    artifact_sha256: sha256_hex(&runtime_archive_bytes),
                    artifact_size_bytes: runtime_archive_bytes.len() as u64,
                    component_manifest_sha256: identity_digest.clone(),
                },
            },
            evidence: WindowsDeliveryEvidence {
                sbom_sha256: identity_digest.clone(),
                provenance_sha256: identity_digest,
            },
            compatibility: WindowsDeliveryCompatibility {
                profile_bridge_protocol_version: 1,
                camouhost_ipc_version: CAMOUHOST_IPC_VERSION,
                runtime_bundle_version: "2.0.0".to_owned(),
            },
        };
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        let cms_der_hex = sha256_hex(&manifest_bytes);
        let signature = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "kind": "WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS",
            "key_id": "recovery-test",
            "cms_der_hex": cms_der_hex,
        }))?;
        let trust = TrustedSignerSet::new([TrustedSigner::new(
            "recovery-test",
            CERTIFICATE_SHA256,
            TrustedSignerStatus::Active,
        )?])?;
        let verified = verify_delivery_candidate(
            &manifest_bytes,
            &signature,
            &trust,
            None,
            &mut DigestVerifier,
        )?;
        Ok((verified, bridge_artifact, runtime_artifact))
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            encoded.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
            encoded.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}
