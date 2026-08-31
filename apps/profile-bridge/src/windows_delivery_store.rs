#![forbid(unsafe_code)]

use crate::windows_delivery::{
    DeliveryActivationOutcome, DeliveryIdentity, DeliveryState, DeliveryStateError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const STATE_SCHEMA_VERSION: u32 = 1;
const STATE_KIND: &str = "WINDOWS_PROFILE_BRIDGE_DELIVERY_STATE";
const FINAL_PREFIX: &str = "state-v1-";
const PENDING_PREFIX: &str = ".pending-state-v1-";
const STATE_SUFFIX: &str = ".json";
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryHandoffKind {
    Activation,
    Recovery,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryHandoffOutcome {
    Started,
    Arrived,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryHandoffEvidence {
    kind: DeliveryHandoffKind,
    source_candidate: DeliveryIdentity,
    source_attempt: u64,
    target: DeliveryIdentity,
    outcome: DeliveryHandoffOutcome,
}

impl DeliveryHandoffEvidence {
    pub fn activation_started(
        state: &DeliveryState,
        target: &DeliveryIdentity,
    ) -> Result<Self, DeliveryStateStoreError> {
        let evidence = Self {
            kind: DeliveryHandoffKind::Activation,
            source_candidate: target.clone(),
            source_attempt: state.activation_generation(),
            target: target.clone(),
            outcome: DeliveryHandoffOutcome::Started,
        };
        evidence.validate(state)?;
        Ok(evidence)
    }

    pub fn recovery_started(
        state: &DeliveryState,
        source_candidate: &DeliveryIdentity,
        source_attempt: u64,
        target: &DeliveryIdentity,
    ) -> Result<Self, DeliveryStateStoreError> {
        let evidence = Self {
            kind: DeliveryHandoffKind::Recovery,
            source_candidate: source_candidate.clone(),
            source_attempt,
            target: target.clone(),
            outcome: DeliveryHandoffOutcome::Started,
        };
        evidence.validate(state)?;
        Ok(evidence)
    }

    pub fn arrived(&self, state: &DeliveryState) -> Result<Self, DeliveryStateStoreError> {
        if self.outcome != DeliveryHandoffOutcome::Started {
            return Err(DeliveryStateStoreError::HandoffEvidence);
        }
        let mut arrived = self.clone();
        arrived.outcome = DeliveryHandoffOutcome::Arrived;
        arrived.validate(state)?;
        Ok(arrived)
    }

    #[must_use]
    pub const fn kind(&self) -> DeliveryHandoffKind {
        self.kind
    }

    #[must_use]
    pub const fn source_candidate(&self) -> &DeliveryIdentity {
        &self.source_candidate
    }

    #[must_use]
    pub const fn source_attempt(&self) -> u64 {
        self.source_attempt
    }

    #[must_use]
    pub const fn target(&self) -> &DeliveryIdentity {
        &self.target
    }

    #[must_use]
    pub const fn outcome(&self) -> DeliveryHandoffOutcome {
        self.outcome
    }

    fn validate(&self, state: &DeliveryState) -> Result<(), DeliveryStateStoreError> {
        state
            .validate_persisted()
            .map_err(DeliveryStateStoreError::State)?;
        if self.source_attempt == 0 {
            return Err(DeliveryStateStoreError::HandoffEvidence);
        }
        match (self.kind, self.outcome) {
            (DeliveryHandoffKind::Activation, DeliveryHandoffOutcome::Started) => {
                let activation = state
                    .last_activation()
                    .ok_or(DeliveryStateStoreError::HandoffEvidence)?;
                if self.source_candidate != self.target
                    || state.active() != Some(&self.target)
                    || state.active_health_confirmed()
                    || state.staged().is_some()
                    || state.activation_generation() != self.source_attempt
                    || activation.attempt != self.source_attempt
                    || activation.candidate != self.target
                    || activation.outcome != DeliveryActivationOutcome::PendingHealth
                {
                    return Err(DeliveryStateStoreError::HandoffEvidence);
                }
            }
            (DeliveryHandoffKind::Activation, DeliveryHandoffOutcome::Arrived) => {
                let activation = state
                    .last_activation()
                    .ok_or(DeliveryStateStoreError::HandoffEvidence)?;
                if self.source_candidate != self.target
                    || state.active() != Some(&self.target)
                    || state.staged().is_some()
                    || state.activation_generation() != self.source_attempt
                    || activation.attempt != self.source_attempt
                    || activation.candidate != self.target
                {
                    return Err(DeliveryStateStoreError::HandoffEvidence);
                }
                match activation.outcome {
                    DeliveryActivationOutcome::PendingHealth
                    | DeliveryActivationOutcome::HealthAttemptStarted
                        if state.active_health_confirmed() =>
                    {
                        return Err(DeliveryStateStoreError::HandoffEvidence);
                    }
                    DeliveryActivationOutcome::Healthy if !state.active_health_confirmed() => {
                        return Err(DeliveryStateStoreError::HandoffEvidence);
                    }
                    DeliveryActivationOutcome::PendingHealth
                    | DeliveryActivationOutcome::HealthAttemptStarted
                    | DeliveryActivationOutcome::Healthy => {}
                    DeliveryActivationOutcome::RolledBack
                    | DeliveryActivationOutcome::RecoveryRequired => {
                        return Err(DeliveryStateStoreError::HandoffEvidence);
                    }
                }
            }
            (DeliveryHandoffKind::Recovery, _) => {
                let activation = state
                    .last_activation()
                    .ok_or(DeliveryStateStoreError::HandoffEvidence)?;
                let failure = state
                    .last_failure()
                    .ok_or(DeliveryStateStoreError::HandoffEvidence)?;
                if self.source_candidate == self.target
                    || state.active() != Some(&self.target)
                    || !state.active_health_confirmed()
                    || activation.attempt != self.source_attempt
                    || activation.candidate != self.source_candidate
                    || activation.outcome != DeliveryActivationOutcome::RolledBack
                    || failure.candidate != self.source_candidate
                {
                    return Err(DeliveryStateStoreError::HandoffEvidence);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedStateEnvelope {
    schema_version: u32,
    kind: String,
    revision: u64,
    previous_snapshot_sha256: Option<String>,
    state: DeliveryState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handoff: Option<DeliveryHandoffEvidence>,
}

impl PersistedStateEnvelope {
    fn new(
        revision: u64,
        previous_snapshot_sha256: Option<String>,
        state: DeliveryState,
        handoff: Option<DeliveryHandoffEvidence>,
    ) -> Result<Self, DeliveryStateStoreError> {
        if revision == 0 {
            return Err(DeliveryStateStoreError::CorruptState);
        }
        if revision == 1 && previous_snapshot_sha256.is_some() {
            return Err(DeliveryStateStoreError::CorruptState);
        }
        if revision > 1
            && previous_snapshot_sha256
                .as_deref()
                .is_none_or(|digest| !is_lower_hex(digest, 64))
        {
            return Err(DeliveryStateStoreError::CorruptState);
        }
        state
            .validate_persisted()
            .map_err(DeliveryStateStoreError::State)?;
        if let Some(evidence) = &handoff {
            evidence.validate(&state)?;
        }
        Ok(Self {
            schema_version: STATE_SCHEMA_VERSION,
            kind: STATE_KIND.to_owned(),
            revision,
            previous_snapshot_sha256,
            state,
            handoff,
        })
    }

    fn validate(
        &self,
        expected_revision: u64,
        expected_previous: Option<&str>,
    ) -> Result<(), DeliveryStateStoreError> {
        if self.schema_version != STATE_SCHEMA_VERSION
            || self.kind != STATE_KIND
            || self.revision != expected_revision
            || self.previous_snapshot_sha256.as_deref() != expected_previous
        {
            return Err(DeliveryStateStoreError::CorruptState);
        }
        self.state
            .validate_persisted()
            .map_err(DeliveryStateStoreError::State)?;
        if let Some(evidence) = &self.handoff {
            evidence.validate(&self.state)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryStateStore {
    root: PathBuf,
    revision: u64,
    snapshot_sha256: String,
    state: DeliveryState,
    handoff: Option<DeliveryHandoffEvidence>,
}

impl DeliveryStateStore {
    pub fn initialize(root: impl AsRef<Path>) -> Result<Self, DeliveryStateStoreError> {
        let root = validated_absolute(root.as_ref())?;
        match fs::create_dir(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(DeliveryStateStoreError::AlreadyInitialized);
            }
            Err(_) => return Err(DeliveryStateStoreError::Io),
        }
        let state = DeliveryState::default();
        let envelope = PersistedStateEnvelope::new(1, None, state.clone(), None)?;
        let snapshot_sha256 = write_snapshot(&root, &envelope)?;
        Ok(Self {
            root,
            revision: 1,
            snapshot_sha256,
            state,
            handoff: None,
        })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, DeliveryStateStoreError> {
        let root = validated_existing_root(root.as_ref())?;
        let loaded = load_chain(&root)?;
        Ok(Self {
            root,
            revision: loaded.revision,
            snapshot_sha256: loaded.snapshot_sha256,
            state: loaded.state,
            handoff: loaded.handoff,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }

    #[must_use]
    pub const fn state(&self) -> &DeliveryState {
        &self.state
    }

    #[must_use]
    pub const fn handoff(&self) -> Option<&DeliveryHandoffEvidence> {
        self.handoff.as_ref()
    }

    pub fn persist(&mut self, state: &DeliveryState) -> Result<(), DeliveryStateStoreError> {
        self.persist_snapshot(state, None)
    }

    pub fn persist_preserving_handoff(
        &mut self,
        state: &DeliveryState,
    ) -> Result<(), DeliveryStateStoreError> {
        self.persist_snapshot(state, self.handoff.clone())
    }

    pub fn persist_handoff(
        &mut self,
        state: &DeliveryState,
        handoff: DeliveryHandoffEvidence,
    ) -> Result<(), DeliveryStateStoreError> {
        self.persist_snapshot(state, Some(handoff))
    }

    fn persist_snapshot(
        &mut self,
        state: &DeliveryState,
        handoff: Option<DeliveryHandoffEvidence>,
    ) -> Result<(), DeliveryStateStoreError> {
        state
            .validate_persisted()
            .map_err(DeliveryStateStoreError::State)?;
        if let Some(evidence) = &handoff {
            evidence.validate(state)?;
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DeliveryStateStoreError::RevisionExhausted)?;
        let envelope = PersistedStateEnvelope::new(
            revision,
            Some(self.snapshot_sha256.clone()),
            state.clone(),
            handoff.clone(),
        )?;
        let snapshot_sha256 = write_snapshot(&self.root, &envelope)?;
        self.revision = revision;
        self.snapshot_sha256 = snapshot_sha256;
        self.state = state.clone();
        self.handoff = handoff;
        Ok(())
    }

    #[must_use]
    pub fn into_state(self) -> DeliveryState {
        self.state
    }
}

struct LoadedState {
    revision: u64,
    snapshot_sha256: String,
    state: DeliveryState,
    handoff: Option<DeliveryHandoffEvidence>,
}

fn validated_absolute(root: &Path) -> Result<PathBuf, DeliveryStateStoreError> {
    if !root.is_absolute() {
        return Err(DeliveryStateStoreError::InvalidRoot);
    }
    if let Some(parent) = root.parent() {
        let metadata = fs::symlink_metadata(parent).map_err(|_| DeliveryStateStoreError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(DeliveryStateStoreError::InvalidRoot);
        }
    }
    Ok(root.to_path_buf())
}

fn validated_existing_root(root: &Path) -> Result<PathBuf, DeliveryStateStoreError> {
    let root = validated_absolute(root)?;
    let metadata = fs::symlink_metadata(&root).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            DeliveryStateStoreError::MissingState
        } else {
            DeliveryStateStoreError::Io
        }
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(DeliveryStateStoreError::InvalidRoot);
    }
    Ok(root)
}

fn load_chain(root: &Path) -> Result<LoadedState, DeliveryStateStoreError> {
    let mut finals = Vec::new();
    let mut pending = Vec::new();
    for entry in fs::read_dir(root).map_err(|_| DeliveryStateStoreError::Io)? {
        let entry = entry.map_err(|_| DeliveryStateStoreError::Io)?;
        let file_type = entry.file_type().map_err(|_| DeliveryStateStoreError::Io)?;
        if !file_type.is_file() || file_type.is_symlink() {
            return Err(DeliveryStateStoreError::AmbiguousState);
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| DeliveryStateStoreError::AmbiguousState)?;
        if let Some(identity) = parse_snapshot_name(&name, FINAL_PREFIX) {
            finals.push((identity, entry.path()));
        } else if let Some(identity) = parse_snapshot_name(&name, PENDING_PREFIX) {
            pending.push((identity, entry.path()));
        } else {
            return Err(DeliveryStateStoreError::AmbiguousState);
        }
    }

    if finals.is_empty() {
        return Err(DeliveryStateStoreError::MissingState);
    }
    finals.sort_by_key(|((revision, _), _)| *revision);
    validate_unique_revisions(&finals)?;
    let mut loaded = load_final_chain(&finals)?;

    if !pending.is_empty() {
        if pending.len() != 1 {
            return Err(DeliveryStateStoreError::AmbiguousState);
        }
        let ((pending_revision, pending_digest), pending_path) = &pending[0];
        let expected_revision = loaded
            .revision
            .checked_add(1)
            .ok_or(DeliveryStateStoreError::RevisionExhausted)?;
        if *pending_revision != expected_revision {
            return Err(DeliveryStateStoreError::AmbiguousState);
        }
        let (envelope, bytes_digest) = read_snapshot(pending_path)?;
        if bytes_digest != *pending_digest {
            return Err(DeliveryStateStoreError::CorruptState);
        }
        envelope.validate(expected_revision, Some(&loaded.snapshot_sha256))?;
        let final_path = root.join(snapshot_name(
            FINAL_PREFIX,
            expected_revision,
            pending_digest,
        ));
        if final_path.exists() {
            return Err(DeliveryStateStoreError::AmbiguousState);
        }
        fs::rename(pending_path, &final_path).map_err(|_| DeliveryStateStoreError::Io)?;
        OpenOptions::new()
            .write(true)
            .open(&final_path)
            .and_then(|file| file.sync_all())
            .map_err(|_| DeliveryStateStoreError::Io)?;
        loaded = LoadedState {
            revision: expected_revision,
            snapshot_sha256: pending_digest.clone(),
            state: envelope.state,
            handoff: envelope.handoff,
        };
    }

    Ok(loaded)
}

fn load_final_chain(
    finals: &[((u64, String), PathBuf)],
) -> Result<LoadedState, DeliveryStateStoreError> {
    let mut previous_digest: Option<String> = None;
    let mut loaded_state: Option<DeliveryState> = None;
    let mut loaded_handoff: Option<DeliveryHandoffEvidence> = None;
    let mut expected_revision = 1_u64;
    for ((revision, filename_digest), path) in finals {
        if *revision != expected_revision {
            return Err(DeliveryStateStoreError::AmbiguousState);
        }
        let (envelope, bytes_digest) = read_snapshot(path)?;
        if bytes_digest != *filename_digest {
            return Err(DeliveryStateStoreError::CorruptState);
        }
        envelope.validate(*revision, previous_digest.as_deref())?;
        previous_digest = Some(bytes_digest);
        loaded_state = Some(envelope.state);
        loaded_handoff = envelope.handoff;
        expected_revision = expected_revision
            .checked_add(1)
            .ok_or(DeliveryStateStoreError::RevisionExhausted)?;
    }
    let snapshot_sha256 = previous_digest.ok_or(DeliveryStateStoreError::MissingState)?;
    let state = loaded_state.ok_or(DeliveryStateStoreError::MissingState)?;
    Ok(LoadedState {
        revision: expected_revision - 1,
        snapshot_sha256,
        state,
        handoff: loaded_handoff,
    })
}

fn validate_unique_revisions(
    snapshots: &[((u64, String), PathBuf)],
) -> Result<(), DeliveryStateStoreError> {
    for window in snapshots.windows(2) {
        if window[0].0.0 == window[1].0.0 {
            return Err(DeliveryStateStoreError::AmbiguousState);
        }
    }
    Ok(())
}

fn write_snapshot(
    root: &Path,
    envelope: &PersistedStateEnvelope,
) -> Result<String, DeliveryStateStoreError> {
    envelope.validate(
        envelope.revision,
        envelope.previous_snapshot_sha256.as_deref(),
    )?;
    let bytes = serde_json::to_vec(envelope).map_err(|_| DeliveryStateStoreError::Serialization)?;
    let digest = sha256_hex(&bytes);
    let pending_path = root.join(snapshot_name(PENDING_PREFIX, envelope.revision, &digest));
    let final_path = root.join(snapshot_name(FINAL_PREFIX, envelope.revision, &digest));
    if final_path.exists() || pending_path.exists() {
        return Err(DeliveryStateStoreError::AmbiguousState);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending_path)
        .map_err(|_| DeliveryStateStoreError::Io)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| DeliveryStateStoreError::Io)?;
    drop(file);
    fs::rename(&pending_path, &final_path).map_err(|_| DeliveryStateStoreError::Io)?;
    OpenOptions::new()
        .write(true)
        .open(&final_path)
        .and_then(|file| file.sync_all())
        .map_err(|_| DeliveryStateStoreError::Io)?;
    Ok(digest)
}

fn read_snapshot(path: &Path) -> Result<(PersistedStateEnvelope, String), DeliveryStateStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryStateStoreError::Io)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(DeliveryStateStoreError::CorruptState);
    }
    let bytes = fs::read(path).map_err(|_| DeliveryStateStoreError::Io)?;
    let digest = sha256_hex(&bytes);
    let envelope =
        serde_json::from_slice(&bytes).map_err(|_| DeliveryStateStoreError::CorruptState)?;
    Ok((envelope, digest))
}

fn snapshot_name(prefix: &str, revision: u64, digest: &str) -> String {
    format!("{prefix}{revision:020}-{digest}{STATE_SUFFIX}")
}

fn parse_snapshot_name(name: &str, prefix: &str) -> Option<(u64, String)> {
    let body = name.strip_prefix(prefix)?.strip_suffix(STATE_SUFFIX)?;
    let (revision_text, digest) = body.split_once('-')?;
    if revision_text.len() != 20
        || !revision_text.bytes().all(|byte| byte.is_ascii_digit())
        || !is_lower_hex(digest, 64)
    {
        return None;
    }
    let revision = revision_text.parse::<u64>().ok()?;
    if revision == 0 || format!("{revision:020}") != revision_text {
        return None;
    }
    Some((revision, digest.to_owned()))
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryStateStoreError {
    InvalidRoot,
    MissingState,
    AlreadyInitialized,
    AmbiguousState,
    CorruptState,
    RevisionExhausted,
    Serialization,
    Io,
    State(DeliveryStateError),
    HandoffEvidence,
}

impl fmt::Display for DeliveryStateStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRoot => "Windows delivery state root is invalid",
            Self::MissingState => "Windows delivery state is missing",
            Self::AlreadyInitialized => "Windows delivery state is already initialized",
            Self::AmbiguousState => "Windows delivery state journal is ambiguous",
            Self::CorruptState => "Windows delivery state journal is corrupt",
            Self::RevisionExhausted => "Windows delivery state revision is exhausted",
            Self::Serialization => "Windows delivery state serialization failed",
            Self::Io => "Windows delivery state filesystem operation failed",
            Self::State(_) => "Windows delivery state machine rejected persisted state",
            Self::HandoffEvidence => "Windows delivery handoff evidence is inconsistent",
        })
    }
}

impl std::error::Error for DeliveryStateStoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, io::Error> {
            let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-state-{label}-{}-{counter}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }

        fn state_root(&self) -> PathBuf {
            self.0.join("state")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn identity() -> DeliveryIdentity {
        DeliveryIdentity {
            sequence: 1,
            release_set_id: format!("release-set-v3-sha256-{}", "1".repeat(64)),
            manifest_sha256: "2".repeat(64),
            profile_bridge_release_id: format!("profile-bridge-v2-sha256-{}", "3".repeat(64)),
            runtime_bundle_release_id: format!("runtime-bundle-v2-sha256-{}", "4".repeat(64)),
        }
    }

    fn pending_activation_state() -> Result<DeliveryState, serde_json::Error> {
        serde_json::from_value(json!({
            "active": identity(),
            "active_health_confirmed": false,
            "last_known_good": null,
            "staged": null,
            "activation_generation": 1,
            "last_activation": {
                "attempt": 1,
                "candidate": identity(),
                "outcome": "pending_health"
            },
            "last_failure": null
        }))
    }

    #[test]
    fn explicit_initialize_and_restart_round_trip_are_deterministic() -> TestResult {
        let directory = TestDirectory::create("restart")?;
        let root = directory.state_root();
        let mut store = DeliveryStateStore::initialize(&root)?;
        assert_eq!(store.revision(), 1);
        let initial_digest = store.snapshot_sha256().to_owned();
        let state = store.state().clone();
        store.persist(&state)?;
        assert_eq!(store.revision(), 2);
        assert_ne!(store.snapshot_sha256(), initial_digest);

        let reopened = DeliveryStateStore::open(&root)?;
        assert_eq!(reopened.revision(), 2);
        assert_eq!(reopened.state(), &state);
        assert_eq!(reopened.handoff(), None);
        assert_eq!(reopened.snapshot_sha256(), store.snapshot_sha256());
        Ok(())
    }

    #[test]
    fn handoff_evidence_round_trips_inside_the_existing_state_journal() -> TestResult {
        let directory = TestDirectory::create("handoff")?;
        let root = directory.state_root();
        let mut store = DeliveryStateStore::initialize(&root)?;
        let state = pending_activation_state()?;
        let evidence = DeliveryHandoffEvidence::activation_started(&state, &identity())?;
        store.persist_handoff(&state, evidence.clone())?;

        let reopened = DeliveryStateStore::open(&root)?;
        assert_eq!(reopened.state(), &state);
        assert_eq!(reopened.handoff(), Some(&evidence));

        let plain_state = reopened.state().clone();
        let mut reopened = reopened;
        reopened.persist(&plain_state)?;
        assert_eq!(reopened.handoff(), None);
        assert_eq!(DeliveryStateStore::open(&root)?.handoff(), None);
        Ok(())
    }

    #[test]
    fn arrived_handoff_can_be_preserved_through_exact_health_transitions() -> TestResult {
        let directory = TestDirectory::create("arrived-health")?;
        let root = directory.state_root();
        let mut store = DeliveryStateStore::initialize(&root)?;
        let mut state = pending_activation_state()?;
        let started = DeliveryHandoffEvidence::activation_started(&state, &identity())?;
        let arrived = started.arrived(&state)?;
        store.persist_handoff(&state, arrived.clone())?;

        state.start_health_attempt(&identity(), 1)?;
        store.persist_preserving_handoff(&state)?;
        assert_eq!(store.handoff(), Some(&arrived));
        assert_eq!(DeliveryStateStore::open(&root)?.handoff(), Some(&arrived));

        state.confirm_health()?;
        store.persist_preserving_handoff(&state)?;
        assert_eq!(store.handoff(), Some(&arrived));
        assert_eq!(DeliveryStateStore::open(&root)?.handoff(), Some(&arrived));
        Ok(())
    }

    #[test]
    fn inconsistent_handoff_evidence_fails_closed() {
        let state = DeliveryState::default();
        assert_eq!(
            DeliveryHandoffEvidence::activation_started(&state, &identity()),
            Err(DeliveryStateStoreError::HandoffEvidence)
        );
    }

    #[test]
    fn missing_and_ambiguous_state_fail_closed() -> TestResult {
        let directory = TestDirectory::create("missing")?;
        let empty = directory.0.join("empty");
        fs::create_dir(&empty)?;
        assert_eq!(
            DeliveryStateStore::open(&empty),
            Err(DeliveryStateStoreError::MissingState)
        );

        let root = directory.state_root();
        let _store = DeliveryStateStore::initialize(&root)?;
        fs::write(root.join("unexpected.txt"), b"ambiguous")?;
        assert_eq!(
            DeliveryStateStore::open(&root),
            Err(DeliveryStateStoreError::AmbiguousState)
        );
        Ok(())
    }

    #[test]
    fn corrupted_snapshot_never_falls_back_silently() -> TestResult {
        let directory = TestDirectory::create("corrupt")?;
        let root = directory.state_root();
        let store = DeliveryStateStore::initialize(&root)?;
        let final_path = root.join(snapshot_name(FINAL_PREFIX, 1, store.snapshot_sha256()));
        let mut bytes = fs::read(&final_path)?;
        bytes.push(b'\n');
        fs::write(&final_path, bytes)?;
        assert_eq!(
            DeliveryStateStore::open(&root),
            Err(DeliveryStateStoreError::CorruptState)
        );
        Ok(())
    }

    #[test]
    fn complete_pending_snapshot_is_finalized_after_restart() -> TestResult {
        let directory = TestDirectory::create("pending")?;
        let root = directory.state_root();
        let store = DeliveryStateStore::initialize(&root)?;
        let envelope = PersistedStateEnvelope::new(
            2,
            Some(store.snapshot_sha256().to_owned()),
            store.state().clone(),
            None,
        )?;
        let bytes = serde_json::to_vec(&envelope)?;
        let digest = sha256_hex(&bytes);
        let pending_path = root.join(snapshot_name(PENDING_PREFIX, 2, &digest));
        let mut pending = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending_path)?;
        pending.write_all(&bytes)?;
        pending.sync_all()?;
        drop(pending);

        let reopened = DeliveryStateStore::open(&root)?;
        assert_eq!(reopened.revision(), 2);
        assert_eq!(reopened.snapshot_sha256(), digest);
        assert_eq!(reopened.handoff(), None);
        assert!(!pending_path.exists());
        assert!(root.join(snapshot_name(FINAL_PREFIX, 2, &digest)).is_file());
        Ok(())
    }

    #[test]
    fn incomplete_pending_snapshot_fails_closed() -> TestResult {
        let directory = TestDirectory::create("pending-corrupt")?;
        let root = directory.state_root();
        let store = DeliveryStateStore::initialize(&root)?;
        let digest = "a".repeat(64);
        let pending_path = root.join(snapshot_name(PENDING_PREFIX, 2, &digest));
        fs::write(&pending_path, b"{")?;
        assert_eq!(
            DeliveryStateStore::open(&root),
            Err(DeliveryStateStoreError::CorruptState)
        );
        assert_eq!(store.revision(), 1);
        Ok(())
    }
}
