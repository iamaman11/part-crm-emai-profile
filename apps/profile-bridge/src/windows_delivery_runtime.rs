#![forbid(unsafe_code)]

use crate::operator_flow::RuntimeBundleSelectionPort;
use crate::runtime_bundle::{
    ApprovedRuntimeBundle, FilesystemRuntimeBundleSelection, RuntimeBundleSelectionError,
};
use crate::windows_delivery::{DeliveryActivationOutcome, DeliveryIdentity, DeliveryState};
use crate::windows_delivery_staging::{DeliveryStagingRoot, reopen_staged_delivery};
use crate::windows_delivery_store::DeliveryStateStore;
use profile_platform_primitives::{ActorContext, GenerationId, ProfileId};
use serde_json::Value;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_BRIDGE_DIRECTORY: &str = "profile-bridge";
const PROFILE_BRIDGE_EXECUTABLE: &str = "profile-bridge.exe";
const RELEASES_DIRECTORY: &str = "releases";
const STATE_DIRECTORY: &str = "state";
const RUNTIME_MANIFEST: &str = "runtime-manifest.json";
const RUNTIME_RELEASE_PREFIX: &str = "runtime-bundle-v2-sha256-";
const MAX_RUNTIME_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

#[derive(Debug)]
pub struct ActiveWindowsDeliveryRuntime {
    runtime_root: PathBuf,
    bundles: DeliveryBoundRuntimeBundleSelection,
    pending_health: Option<PendingWindowsDeliveryHealthConfirmation>,
}

impl ActiveWindowsDeliveryRuntime {
    pub fn resolve_current() -> Result<Self, WindowsDeliveryRuntimeError> {
        let current_executable = std::env::current_exe()
            .map_err(|_| WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        let current_executable = fs::canonicalize(current_executable)
            .map_err(|_| WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        validate_regular_path(&current_executable)
            .map_err(|_| WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        let layout = InstalledDeliveryLayout::from_executable(&current_executable)?;

        validate_existing_directory(&layout.releases_root)
            .map_err(|_| WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        let staging = DeliveryStagingRoot::open_or_create(&layout.releases_root)
            .map_err(|_| WindowsDeliveryRuntimeError::StagedDelivery)?;
        let state = DeliveryStateStore::open(&layout.state_root)
            .map_err(|_| WindowsDeliveryRuntimeError::PersistedState)?;
        let active = state
            .state()
            .active()
            .cloned()
            .ok_or(WindowsDeliveryRuntimeError::NoActiveDelivery)?;
        let pending_health = pending_health_confirmation(&state, &layout.state_root, &active)?;
        let staged = reopen_staged_delivery(&staging, &active)
            .map_err(|_| WindowsDeliveryRuntimeError::StagedDelivery)?;

        let staged_release = fs::canonicalize(staged.path())
            .map_err(|_| WindowsDeliveryRuntimeError::StagedDelivery)?;
        let installed_release = fs::canonicalize(&layout.release_root)
            .map_err(|_| WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        if staged_release != installed_release {
            return Err(WindowsDeliveryRuntimeError::ActiveExecutableMismatch);
        }

        let expected_executable = staged.profile_bridge_root().join(PROFILE_BRIDGE_EXECUTABLE);
        let expected_executable = fs::canonicalize(expected_executable)
            .map_err(|_| WindowsDeliveryRuntimeError::StagedDelivery)?;
        if expected_executable != current_executable {
            return Err(WindowsDeliveryRuntimeError::ActiveExecutableMismatch);
        }

        let runtime_root = staged.runtime_root();
        let bundles = DeliveryBoundRuntimeBundleSelection::open(
            runtime_root.clone(),
            active.runtime_bundle_release_id.clone(),
        )
        .map_err(|_| WindowsDeliveryRuntimeError::RuntimeIdentity)?;
        Ok(Self {
            runtime_root,
            bundles,
            pending_health,
        })
    }

    #[must_use]
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    #[must_use]
    pub(crate) fn pending_health_confirmation(
        &self,
    ) -> Option<PendingWindowsDeliveryHealthConfirmation> {
        self.pending_health.clone()
    }

    #[must_use]
    pub fn into_bundle_selection(self) -> DeliveryBoundRuntimeBundleSelection {
        self.bundles
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingWindowsDeliveryHealthConfirmation {
    state_root: PathBuf,
    candidate: DeliveryIdentity,
    activation_attempt: u64,
}

impl PendingWindowsDeliveryHealthConfirmation {
    pub(crate) fn confirm_after_runtime_ready(self) -> Result<(), WindowsDeliveryRuntimeError> {
        let mut store = DeliveryStateStore::open(&self.state_root)
            .map_err(|_| WindowsDeliveryRuntimeError::PersistedState)?;
        match exact_health_state(store.state(), &self.candidate, self.activation_attempt)? {
            ExactHealthState::Healthy => return Ok(()),
            ExactHealthState::Pending => {}
        }

        let mut next = store.state().clone();
        next.confirm_health()
            .map_err(|_| WindowsDeliveryRuntimeError::PersistedState)?;
        if store.persist(&next).is_err() {
            let reopened = DeliveryStateStore::open(&self.state_root)
                .map_err(|_| WindowsDeliveryRuntimeError::PersistedState)?;
            return match exact_health_state(
                reopened.state(),
                &self.candidate,
                self.activation_attempt,
            )? {
                ExactHealthState::Healthy => Ok(()),
                ExactHealthState::Pending => Err(WindowsDeliveryRuntimeError::PersistedState),
            };
        }

        let reopened = DeliveryStateStore::open(&self.state_root)
            .map_err(|_| WindowsDeliveryRuntimeError::PersistedState)?;
        match exact_health_state(reopened.state(), &self.candidate, self.activation_attempt)? {
            ExactHealthState::Healthy => Ok(()),
            ExactHealthState::Pending => Err(WindowsDeliveryRuntimeError::PersistedState),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactHealthState {
    Pending,
    Healthy,
}

fn pending_health_confirmation(
    store: &DeliveryStateStore,
    state_root: &Path,
    active: &DeliveryIdentity,
) -> Result<Option<PendingWindowsDeliveryHealthConfirmation>, WindowsDeliveryRuntimeError> {
    match exact_health_state(store.state(), active, store.state().activation_generation())? {
        ExactHealthState::Healthy => Ok(None),
        ExactHealthState::Pending => Ok(Some(PendingWindowsDeliveryHealthConfirmation {
            state_root: state_root.to_path_buf(),
            candidate: active.clone(),
            activation_attempt: store.state().activation_generation(),
        })),
    }
}

fn exact_health_state(
    state: &DeliveryState,
    candidate: &DeliveryIdentity,
    activation_attempt: u64,
) -> Result<ExactHealthState, WindowsDeliveryRuntimeError> {
    state
        .validate_persisted()
        .map_err(|_| WindowsDeliveryRuntimeError::PersistedState)?;
    if activation_attempt == 0
        || state.active() != Some(candidate)
        || state.activation_generation() != activation_attempt
    {
        return Err(WindowsDeliveryRuntimeError::PersistedState);
    }
    let activation = state
        .last_activation()
        .ok_or(WindowsDeliveryRuntimeError::PersistedState)?;
    if activation.attempt != activation_attempt || activation.candidate != *candidate {
        return Err(WindowsDeliveryRuntimeError::PersistedState);
    }
    match (state.active_health_confirmed(), activation.outcome) {
        (false, DeliveryActivationOutcome::PendingHealth) => Ok(ExactHealthState::Pending),
        (true, DeliveryActivationOutcome::Healthy) => Ok(ExactHealthState::Healthy),
        _ => Err(WindowsDeliveryRuntimeError::PersistedState),
    }
}

#[derive(Debug)]
pub struct DeliveryBoundRuntimeBundleSelection {
    inner: FilesystemRuntimeBundleSelection,
    expected_release_id: String,
}

impl DeliveryBoundRuntimeBundleSelection {
    fn open(
        runtime_root: impl Into<PathBuf>,
        expected_release_id: String,
    ) -> Result<Self, RuntimeBundleSelectionError> {
        if !valid_runtime_release_id(&expected_release_id) {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        }
        let inner = FilesystemRuntimeBundleSelection::open(runtime_root)?;
        verify_embedded_release_id(inner.runtime_root(), &expected_release_id)?;
        Ok(Self {
            inner,
            expected_release_id,
        })
    }
}

impl RuntimeBundleSelectionPort for DeliveryBoundRuntimeBundleSelection {
    type Error = RuntimeBundleSelectionError;

    fn select_bundle(
        &mut self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<ApprovedRuntimeBundle, Self::Error> {
        verify_embedded_release_id(self.inner.runtime_root(), &self.expected_release_id)?;
        let bundle = self.inner.select_bundle(actor, profile_id, generation_id)?;
        verify_embedded_release_id(self.inner.runtime_root(), &self.expected_release_id)?;
        Ok(bundle)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InstalledDeliveryLayout {
    release_root: PathBuf,
    releases_root: PathBuf,
    state_root: PathBuf,
}

impl InstalledDeliveryLayout {
    fn from_executable(executable: &Path) -> Result<Self, WindowsDeliveryRuntimeError> {
        if !executable.is_absolute()
            || executable.file_name().and_then(|value| value.to_str())
                != Some(PROFILE_BRIDGE_EXECUTABLE)
        {
            return Err(WindowsDeliveryRuntimeError::InvalidInstalledLayout);
        }
        let profile_bridge_root = executable
            .parent()
            .ok_or(WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        if profile_bridge_root
            .file_name()
            .and_then(|value| value.to_str())
            != Some(PROFILE_BRIDGE_DIRECTORY)
        {
            return Err(WindowsDeliveryRuntimeError::InvalidInstalledLayout);
        }
        let release_root = profile_bridge_root
            .parent()
            .ok_or(WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        let releases_root = release_root
            .parent()
            .ok_or(WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        if releases_root.file_name().and_then(|value| value.to_str()) != Some(RELEASES_DIRECTORY) {
            return Err(WindowsDeliveryRuntimeError::InvalidInstalledLayout);
        }
        let delivery_root = releases_root
            .parent()
            .ok_or(WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
        Ok(Self {
            release_root: release_root.to_path_buf(),
            releases_root: releases_root.to_path_buf(),
            state_root: delivery_root.join(STATE_DIRECTORY),
        })
    }
}

fn verify_embedded_release_id(
    runtime_root: &Path,
    expected_release_id: &str,
) -> Result<(), RuntimeBundleSelectionError> {
    let manifest_path = runtime_root.join(RUNTIME_MANIFEST);
    let metadata = fs::symlink_metadata(&manifest_path)
        .map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    if metadata_is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_RUNTIME_MANIFEST_BYTES
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    let bytes =
        fs::read(&manifest_path).map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    let manifest: Value =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    let release_id = manifest
        .as_object()
        .and_then(|object| object.get("release_id"))
        .and_then(Value::as_str)
        .ok_or(RuntimeBundleSelectionError::InvalidRuntime)?;
    if release_id != expected_release_id {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(())
}

fn valid_runtime_release_id(value: &str) -> bool {
    value
        .strip_prefix(RUNTIME_RELEASE_PREFIX)
        .is_some_and(|digest| is_lower_hex(digest, 64))
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_regular_path(path: &Path) -> Result<(), WindowsDeliveryRuntimeError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(WindowsDeliveryRuntimeError::InvalidInstalledLayout);
    }
    Ok(())
}

fn validate_existing_directory(path: &Path) -> Result<(), WindowsDeliveryRuntimeError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| WindowsDeliveryRuntimeError::InvalidInstalledLayout)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(WindowsDeliveryRuntimeError::InvalidInstalledLayout);
    }
    Ok(())
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsDeliveryRuntimeError {
    InvalidInstalledLayout,
    PersistedState,
    NoActiveDelivery,
    StagedDelivery,
    ActiveExecutableMismatch,
    RuntimeIdentity,
}

impl fmt::Display for WindowsDeliveryRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInstalledLayout => "installed Windows delivery layout is invalid",
            Self::PersistedState => "persisted Windows delivery state is unavailable",
            Self::NoActiveDelivery => "persisted Windows delivery state has no active candidate",
            Self::StagedDelivery => "active Windows delivery stage is invalid",
            Self::ActiveExecutableMismatch => {
                "running Profile Bridge is not the persisted active delivery executable"
            }
            Self::RuntimeIdentity => "active Windows runtime bundle identity is invalid",
        })
    }
}

impl std::error::Error for WindowsDeliveryRuntimeError {}

#[cfg(test)]
mod tests {
    use super::{
        ExactHealthState, InstalledDeliveryLayout, RUNTIME_MANIFEST, RUNTIME_RELEASE_PREFIX,
        RuntimeBundleSelectionError, WindowsDeliveryRuntimeError, exact_health_state,
        pending_health_confirmation, valid_runtime_release_id, verify_embedded_release_id,
    };
    use crate::windows_delivery::{DeliveryActivationOutcome, DeliveryIdentity, DeliveryState};
    use crate::windows_delivery_store::DeliveryStateStore;
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, std::io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-active-runtime-{label}-{}-{sequence}",
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

    fn delivery_identity(suffix: char) -> DeliveryIdentity {
        let digest: String = std::iter::repeat_n(suffix, 64).collect();
        DeliveryIdentity {
            sequence: 1,
            release_set_id: format!("release-set-v3-sha256-{digest}"),
            manifest_sha256: digest.clone(),
            profile_bridge_release_id: format!("profile-bridge-v2-sha256-{digest}"),
            runtime_bundle_release_id: format!("runtime-bundle-v2-sha256-{digest}"),
        }
    }

    fn pending_state(identity: &DeliveryIdentity) -> Result<DeliveryState, serde_json::Error> {
        serde_json::from_value(json!({
            "active": identity,
            "active_health_confirmed": false,
            "last_known_good": null,
            "staged": null,
            "activation_generation": 1,
            "last_activation": {
                "attempt": 1,
                "candidate": identity,
                "outcome": "pending_health"
            },
            "last_failure": null
        }))
    }

    #[test]
    fn installed_layout_is_derived_only_from_the_shipping_executable_path()
    -> Result<(), Box<dyn std::error::Error>> {
        let executable = Path::new(
            r"C:\ProfileBridge\releases\release-00000000000000000001-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\profile-bridge\profile-bridge.exe",
        );
        let layout = InstalledDeliveryLayout::from_executable(executable)?;
        assert_eq!(
            layout.releases_root,
            PathBuf::from(r"C:\ProfileBridge\releases")
        );
        assert_eq!(layout.state_root, PathBuf::from(r"C:\ProfileBridge\state"));
        assert_eq!(
            InstalledDeliveryLayout::from_executable(Path::new(
                r"C:\ProfileBridge\arbitrary\profile-bridge.exe"
            )),
            Err(WindowsDeliveryRuntimeError::InvalidInstalledLayout)
        );
        Ok(())
    }

    #[test]
    fn runtime_release_identity_must_be_exact_content_addressed_v2() {
        assert!(valid_runtime_release_id(&format!(
            "{RUNTIME_RELEASE_PREFIX}{}",
            "a".repeat(64)
        )));
        assert!(!valid_runtime_release_id("latest"));
        assert!(!valid_runtime_release_id(&format!(
            "{RUNTIME_RELEASE_PREFIX}{}",
            "A".repeat(64)
        )));
    }

    #[test]
    fn embedded_runtime_release_id_is_bound_to_active_delivery_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::create("release-id")?;
        let expected = format!("{RUNTIME_RELEASE_PREFIX}{}", "b".repeat(64));
        fs::write(
            directory.0.join(RUNTIME_MANIFEST),
            serde_json::to_vec(&json!({"release_id": expected}))?,
        )?;
        verify_embedded_release_id(&directory.0, &expected)?;
        assert_eq!(
            verify_embedded_release_id(
                &directory.0,
                &format!("{RUNTIME_RELEASE_PREFIX}{}", "c".repeat(64))
            ),
            Err(RuntimeBundleSelectionError::InvalidRuntime)
        );
        Ok(())
    }

    #[test]
    fn pending_candidate_health_confirmation_is_exact_and_idempotent()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::create("pending-health")?;
        let state_root = directory.0.join("state");
        let mut store = DeliveryStateStore::initialize(&state_root)?;
        let identity = delivery_identity('d');
        let pending = pending_state(&identity)?;
        pending.validate_persisted()?;
        store.persist(&pending)?;

        assert_eq!(
            exact_health_state(store.state(), &identity, 1)?,
            ExactHealthState::Pending
        );
        let confirmation = pending_health_confirmation(&store, &state_root, &identity)?
            .ok_or("pending candidate did not expose exact health confirmation")?;
        confirmation.clone().confirm_after_runtime_ready()?;
        confirmation.confirm_after_runtime_ready()?;

        let reopened = DeliveryStateStore::open(&state_root)?;
        assert!(reopened.state().active_health_confirmed());
        assert_eq!(reopened.state().active(), Some(&identity));
        assert_eq!(
            reopened
                .state()
                .last_activation()
                .map(|value| value.outcome),
            Some(DeliveryActivationOutcome::Healthy)
        );
        assert_eq!(
            exact_health_state(reopened.state(), &identity, 1)?,
            ExactHealthState::Healthy
        );
        Ok(())
    }

    #[test]
    fn health_confirmation_rejects_candidate_or_attempt_substitution()
    -> Result<(), Box<dyn std::error::Error>> {
        let identity = delivery_identity('e');
        let state = pending_state(&identity)?;
        assert_eq!(
            exact_health_state(&state, &delivery_identity('f'), 1),
            Err(WindowsDeliveryRuntimeError::PersistedState)
        );
        assert_eq!(
            exact_health_state(&state, &identity, 2),
            Err(WindowsDeliveryRuntimeError::PersistedState)
        );
        Ok(())
    }
}
