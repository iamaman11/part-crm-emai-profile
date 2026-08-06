#![forbid(unsafe_code)]

use core::fmt;
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId, UnixMillis};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MIN_NAME_BYTES: usize = 3;
const MAX_NAME_BYTES: usize = 96;
const MATRIX_SCHEMA: &[u8] = b"profile-platform-certification-matrix-v1";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SignalName(String);

impl SignalName {
    pub fn parse(value: impl Into<String>) -> Result<Self, CertificationError> {
        let value = value.into();
        let valid_length = (MIN_NAME_BYTES..=MAX_NAME_BYTES).contains(&value.len());
        let valid_chars = value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        });
        if !valid_length || !valid_chars {
            return Err(CertificationError::InvalidName);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalRequirement {
    Required,
    Optional,
    Prohibited,
}

impl SignalRequirement {
    const fn code(self) -> u8 {
        match self {
            Self::Required => 1,
            Self::Optional => 2,
            Self::Prohibited => 3,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalRule {
    name: SignalName,
    requirement: SignalRequirement,
    tolerance: u64,
}

impl SignalRule {
    pub fn new(
        name: SignalName,
        requirement: SignalRequirement,
        tolerance: u64,
    ) -> Result<Self, CertificationError> {
        if requirement == SignalRequirement::Prohibited && tolerance != 0 {
            return Err(CertificationError::InvalidTolerance);
        }
        Ok(Self {
            name,
            requirement,
            tolerance,
        })
    }

    #[must_use]
    pub const fn name(&self) -> &SignalName {
        &self.name
    }

    #[must_use]
    pub const fn requirement(&self) -> SignalRequirement {
        self.requirement
    }

    #[must_use]
    pub const fn tolerance(&self) -> u64 {
        self.tolerance
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificationPolicy {
    version: u32,
    rules: BTreeMap<SignalName, SignalRule>,
}

impl CertificationPolicy {
    pub fn new(version: u32, rules: Vec<SignalRule>) -> Result<Self, CertificationError> {
        if version == 0 || rules.is_empty() {
            return Err(CertificationError::InvalidPolicy);
        }
        let mut indexed = BTreeMap::new();
        let mut required_rules = 0_u32;
        for rule in rules {
            if rule.requirement == SignalRequirement::Required {
                required_rules = required_rules
                    .checked_add(1)
                    .ok_or(CertificationError::CounterOverflow)?;
            }
            if indexed.insert(rule.name.clone(), rule).is_some() {
                return Err(CertificationError::DuplicateSignal);
            }
        }
        if required_rules == 0 {
            return Err(CertificationError::InvalidPolicy);
        }
        Ok(Self {
            version,
            rules: indexed,
        })
    }

    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    pub fn rules(&self) -> impl Iterator<Item = &SignalRule> {
        self.rules.values()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ObservationSet {
    sequence: u32,
    values: BTreeMap<SignalName, i64>,
}

impl ObservationSet {
    pub fn new(sequence: u32, values: Vec<(SignalName, i64)>) -> Result<Self, CertificationError> {
        if sequence == 0 {
            return Err(CertificationError::InvalidObservationSequence);
        }
        let mut indexed = BTreeMap::new();
        for (name, value) in values {
            if indexed.insert(name, value).is_some() {
                return Err(CertificationError::DuplicateSignal);
            }
        }
        Ok(Self {
            sequence,
            values: indexed,
        })
    }

    #[must_use]
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CertificationOutcome {
    Stable,
    Drifted,
    Incomplete,
    Prohibited,
}

impl CertificationOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Drifted => "drifted",
            Self::Incomplete => "incomplete",
            Self::Prohibited => "prohibited",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatrixDigest([u8; 32]);

impl MatrixDigest {
    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in self.0 {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificationReport {
    policy_version: u32,
    observation_count: u32,
    evaluated_signals: u32,
    drifted_signals: u32,
    missing_required_signals: u32,
    prohibited_signals: u32,
    outcome: CertificationOutcome,
    matrix_digest: MatrixDigest,
}

impl CertificationReport {
    #[must_use]
    pub const fn outcome(&self) -> CertificationOutcome {
        self.outcome
    }

    #[must_use]
    pub const fn matrix_digest(&self) -> MatrixDigest {
        self.matrix_digest
    }

    #[must_use]
    pub fn render_metadata_only(&self) -> String {
        format!(
            "schema=certification-summary-v1\npolicy_version={}\nobservation_count={}\nevaluated_signals={}\ndrifted_signals={}\nmissing_required_signals={}\nprohibited_signals={}\noutcome={}\n",
            self.policy_version,
            self.observation_count,
            self.evaluated_signals,
            self.drifted_signals,
            self.missing_required_signals,
            self.prohibited_signals,
            self.outcome.as_str(),
        )
    }
}

pub fn evaluate_certification(
    policy: &CertificationPolicy,
    observations: &[ObservationSet],
) -> Result<CertificationReport, CertificationError> {
    if observations.is_empty() {
        return Err(CertificationError::EmptyObservationMatrix);
    }

    let mut ordered = observations.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|observation| observation.sequence);
    let mut sequences = BTreeSet::new();
    for observation in &ordered {
        if !sequences.insert(observation.sequence) {
            return Err(CertificationError::DuplicateObservationSequence);
        }
        for name in observation.values.keys() {
            if !policy.rules.contains_key(name) {
                return Err(CertificationError::UnknownSignal);
            }
        }
    }

    let mut evaluated_signals = 0_u32;
    let mut drifted_signals = 0_u32;
    let mut missing_required_signals = 0_u32;
    let mut prohibited_signals = 0_u32;

    for rule in policy.rules.values() {
        let values = ordered
            .iter()
            .filter_map(|observation| observation.values.get(&rule.name).copied())
            .collect::<Vec<_>>();

        match rule.requirement {
            SignalRequirement::Prohibited => {
                if !values.is_empty() {
                    prohibited_signals = prohibited_signals
                        .checked_add(1)
                        .ok_or(CertificationError::CounterOverflow)?;
                }
            }
            SignalRequirement::Required => {
                if values.len() != ordered.len() {
                    missing_required_signals = missing_required_signals
                        .checked_add(1)
                        .ok_or(CertificationError::CounterOverflow)?;
                    continue;
                }
                evaluated_signals = evaluated_signals
                    .checked_add(1)
                    .ok_or(CertificationError::CounterOverflow)?;
                if exceeds_tolerance(&values, rule.tolerance)? {
                    drifted_signals = drifted_signals
                        .checked_add(1)
                        .ok_or(CertificationError::CounterOverflow)?;
                }
            }
            SignalRequirement::Optional => {
                if values.is_empty() {
                    continue;
                }
                evaluated_signals = evaluated_signals
                    .checked_add(1)
                    .ok_or(CertificationError::CounterOverflow)?;
                if exceeds_tolerance(&values, rule.tolerance)? {
                    drifted_signals = drifted_signals
                        .checked_add(1)
                        .ok_or(CertificationError::CounterOverflow)?;
                }
            }
        }
    }

    let outcome = if prohibited_signals > 0 {
        CertificationOutcome::Prohibited
    } else if missing_required_signals > 0 {
        CertificationOutcome::Incomplete
    } else if drifted_signals > 0 {
        CertificationOutcome::Drifted
    } else {
        CertificationOutcome::Stable
    };
    let observation_count =
        u32::try_from(ordered.len()).map_err(|_| CertificationError::CounterOverflow)?;
    let matrix_digest = calculate_matrix_digest(policy, &ordered)?;

    Ok(CertificationReport {
        policy_version: policy.version,
        observation_count,
        evaluated_signals,
        drifted_signals,
        missing_required_signals,
        prohibited_signals,
        outcome,
        matrix_digest,
    })
}

fn exceeds_tolerance(values: &[i64], tolerance: u64) -> Result<bool, CertificationError> {
    let minimum = values
        .iter()
        .min()
        .copied()
        .ok_or(CertificationError::EmptyObservationMatrix)?;
    let maximum = values
        .iter()
        .max()
        .copied()
        .ok_or(CertificationError::EmptyObservationMatrix)?;
    let delta = i128::from(maximum) - i128::from(minimum);
    let delta = u128::try_from(delta).map_err(|_| CertificationError::CounterOverflow)?;
    Ok(delta > u128::from(tolerance))
}

fn calculate_matrix_digest(
    policy: &CertificationPolicy,
    observations: &[&ObservationSet],
) -> Result<MatrixDigest, CertificationError> {
    let mut digest = Sha256::new();
    digest.update(MATRIX_SCHEMA);
    digest.update(policy.version.to_be_bytes());
    digest.update(
        u32::try_from(policy.rules.len())
            .map_err(|_| CertificationError::CounterOverflow)?
            .to_be_bytes(),
    );
    for rule in policy.rules.values() {
        update_length_prefixed(&mut digest, rule.name.as_str().as_bytes())?;
        digest.update([rule.requirement.code()]);
        digest.update(rule.tolerance.to_be_bytes());
    }
    digest.update(
        u32::try_from(observations.len())
            .map_err(|_| CertificationError::CounterOverflow)?
            .to_be_bytes(),
    );
    for observation in observations {
        digest.update(observation.sequence.to_be_bytes());
        digest.update(
            u32::try_from(observation.values.len())
                .map_err(|_| CertificationError::CounterOverflow)?
                .to_be_bytes(),
        );
        for (name, value) in &observation.values {
            update_length_prefixed(&mut digest, name.as_str().as_bytes())?;
            digest.update(value.to_be_bytes());
        }
    }
    Ok(MatrixDigest(digest.finalize().into()))
}

fn update_length_prefixed(digest: &mut Sha256, value: &[u8]) -> Result<(), CertificationError> {
    digest.update(
        u32::try_from(value.len())
            .map_err(|_| CertificationError::CounterOverflow)?
            .to_be_bytes(),
    );
    digest.update(value);
    Ok(())
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DeviceGrantKey {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    device_id: DeviceId,
}

impl DeviceGrantKey {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        device_id: DeviceId,
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            generation_id,
            device_id,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceGrantStatus {
    Active,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceGrantSnapshot {
    version: u64,
    status: DeviceGrantStatus,
    changed_at: UnixMillis,
}

impl DeviceGrantSnapshot {
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub const fn status(&self) -> DeviceGrantStatus {
        self.status
    }

    #[must_use]
    pub const fn changed_at(&self) -> UnixMillis {
        self.changed_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceGrantEvent {
    key: DeviceGrantKey,
    snapshot: DeviceGrantSnapshot,
}

impl DeviceGrantEvent {
    #[must_use]
    pub const fn key(&self) -> &DeviceGrantKey {
        &self.key
    }

    #[must_use]
    pub const fn snapshot(&self) -> &DeviceGrantSnapshot {
        &self.snapshot
    }
}

#[derive(Default)]
pub struct DeviceAuthorizationRegistry {
    grants: BTreeMap<DeviceGrantKey, DeviceGrantSnapshot>,
    history: Vec<DeviceGrantEvent>,
}

impl DeviceAuthorizationRegistry {
    pub fn grant(
        &mut self,
        key: DeviceGrantKey,
        expected_version: u64,
        observed_at: UnixMillis,
    ) -> Result<DeviceGrantSnapshot, CertificationError> {
        let next = match self.grants.get(&key) {
            Some(current) => {
                if current.version != expected_version {
                    return Err(CertificationError::StaleGrantVersion);
                }
                if current.status == DeviceGrantStatus::Active {
                    return Err(CertificationError::GrantAlreadyActive);
                }
                if observed_at < current.changed_at {
                    return Err(CertificationError::TimeRegression);
                }
                DeviceGrantSnapshot {
                    version: current
                        .version
                        .checked_add(1)
                        .ok_or(CertificationError::CounterOverflow)?,
                    status: DeviceGrantStatus::Active,
                    changed_at: observed_at,
                }
            }
            None => {
                if expected_version != 0 {
                    return Err(CertificationError::StaleGrantVersion);
                }
                DeviceGrantSnapshot {
                    version: 1,
                    status: DeviceGrantStatus::Active,
                    changed_at: observed_at,
                }
            }
        };
        self.grants.insert(key.clone(), next.clone());
        self.history.push(DeviceGrantEvent {
            key,
            snapshot: next.clone(),
        });
        Ok(next)
    }

    pub fn revoke(
        &mut self,
        key: &DeviceGrantKey,
        expected_version: u64,
        observed_at: UnixMillis,
    ) -> Result<DeviceGrantSnapshot, CertificationError> {
        let current = self
            .grants
            .get(key)
            .ok_or(CertificationError::MissingGrant)?;
        if current.version != expected_version {
            return Err(CertificationError::StaleGrantVersion);
        }
        if current.status == DeviceGrantStatus::Revoked {
            return Err(CertificationError::GrantAlreadyRevoked);
        }
        if observed_at < current.changed_at {
            return Err(CertificationError::TimeRegression);
        }
        let next = DeviceGrantSnapshot {
            version: current
                .version
                .checked_add(1)
                .ok_or(CertificationError::CounterOverflow)?,
            status: DeviceGrantStatus::Revoked,
            changed_at: observed_at,
        };
        self.grants.insert(key.clone(), next.clone());
        self.history.push(DeviceGrantEvent {
            key: key.clone(),
            snapshot: next.clone(),
        });
        Ok(next)
    }

    #[must_use]
    pub fn history(&self) -> &[DeviceGrantEvent] {
        &self.history
    }

    pub fn authorize_unwrap(
        &self,
        key: &DeviceGrantKey,
        grant_version: u64,
    ) -> Result<(), CertificationError> {
        let current = self
            .grants
            .get(key)
            .ok_or(CertificationError::MissingGrant)?;
        if current.version != grant_version {
            return Err(CertificationError::StaleGrantVersion);
        }
        if current.status != DeviceGrantStatus::Active {
            return Err(CertificationError::GrantRevoked);
        }
        Ok(())
    }

    #[must_use]
    pub fn render_metadata_only(&self) -> String {
        let active = self
            .grants
            .values()
            .filter(|grant| grant.status == DeviceGrantStatus::Active)
            .count();
        let revoked = self.grants.len().saturating_sub(active);
        format!(
            "schema=device-authorization-summary-v1\ntotal_grants={}\nactive_grants={}\nrevoked_grants={}\nhistory_events={}\n",
            self.grants.len(),
            active,
            revoked,
            self.history.len(),
        )
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReleaseId(String);

impl ReleaseId {
    pub fn parse(value: impl Into<String>) -> Result<Self, CertificationError> {
        parse_name(value.into()).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentDigest([u8; 32]);

impl ContentDigest {
    pub fn new(bytes: [u8; 32]) -> Result<Self, CertificationError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(CertificationError::InvalidContentDigest);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VerificationEvidenceId(String);

impl VerificationEvidenceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, CertificationError> {
        parse_name(value.into()).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreverifiedSignatureEvidence {
    verifier: String,
    evidence_id: VerificationEvidenceId,
    release_id: ReleaseId,
    release_version: u64,
    content_digest: ContentDigest,
}

impl PreverifiedSignatureEvidence {
    pub fn new(
        verifier: impl Into<String>,
        evidence_id: VerificationEvidenceId,
        release_id: ReleaseId,
        release_version: u64,
        content_digest: ContentDigest,
    ) -> Result<Self, CertificationError> {
        if release_version == 0 {
            return Err(CertificationError::InvalidReleaseVersion);
        }
        let verifier = parse_name(verifier.into())?;
        Ok(Self {
            verifier,
            evidence_id,
            release_id,
            release_version,
            content_digest,
        })
    }

    #[must_use]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    #[must_use]
    pub const fn evidence_id(&self) -> &VerificationEvidenceId {
        &self.evidence_id
    }

    #[must_use]
    pub const fn release_id(&self) -> &ReleaseId {
        &self.release_id
    }

    #[must_use]
    pub const fn release_version(&self) -> u64 {
        self.release_version
    }

    #[must_use]
    pub const fn content_digest(&self) -> &ContentDigest {
        &self.content_digest
    }

    fn approves(
        &self,
        release_id: &ReleaseId,
        release_version: u64,
        content_digest: &ContentDigest,
    ) -> bool {
        self.release_id == *release_id
            && self.release_version == release_version
            && self.content_digest == *content_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseCandidate {
    release_id: ReleaseId,
    version: u64,
    content_digest: ContentDigest,
    verification: PreverifiedSignatureEvidence,
}

impl ReleaseCandidate {
    pub fn new(
        release_id: ReleaseId,
        version: u64,
        content_digest: ContentDigest,
        verification: PreverifiedSignatureEvidence,
    ) -> Result<Self, CertificationError> {
        if version == 0 {
            return Err(CertificationError::InvalidReleaseVersion);
        }
        if !verification.approves(&release_id, version, &content_digest) {
            return Err(CertificationError::VerificationEvidenceMismatch);
        }
        Ok(Self {
            release_id,
            version,
            content_digest,
            verification,
        })
    }

    #[must_use]
    pub const fn release_id(&self) -> &ReleaseId {
        &self.release_id
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub const fn content_digest(&self) -> &ContentDigest {
        &self.content_digest
    }

    #[must_use]
    pub const fn verification(&self) -> &PreverifiedSignatureEvidence {
        &self.verification
    }

    fn matches_identity(
        &self,
        release_id: &ReleaseId,
        version: u64,
        content_digest: &ContentDigest,
    ) -> bool {
        self.release_id == *release_id
            && self.version == version
            && self.content_digest == *content_digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RollbackOutcome {
    Restored(u64),
    NoPreviousRelease,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UpdateState {
    #[default]
    Idle,
    Staged,
    AwaitingHealth,
    Healthy,
    RolledBack,
    Failed,
}

impl UpdateState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Staged => "staged",
            Self::AwaitingHealth => "awaiting_health",
            Self::Healthy => "healthy",
            Self::RolledBack => "rolled_back",
            Self::Failed => "failed",
        }
    }
}

#[derive(Default)]
pub struct UpdateController {
    active: Option<ReleaseCandidate>,
    previous: Option<ReleaseCandidate>,
    staged: Option<ReleaseCandidate>,
    highest_seen_version: u64,
    state: UpdateState,
}

impl UpdateController {
    pub fn stage(
        &mut self,
        candidate: ReleaseCandidate,
        observed_digest: &ContentDigest,
    ) -> Result<(), CertificationError> {
        if candidate.content_digest != *observed_digest {
            return Err(CertificationError::ContentDigestMismatch);
        }
        if candidate.version <= self.highest_seen_version {
            return Err(CertificationError::StaleReleaseVersion);
        }
        if self.staged.is_some() || self.state == UpdateState::AwaitingHealth {
            return Err(CertificationError::UpdateBusy);
        }
        self.highest_seen_version = candidate.version;
        self.staged = Some(candidate);
        self.state = UpdateState::Staged;
        Ok(())
    }

    pub fn activate_staged(&mut self) -> Result<u64, CertificationError> {
        if self.state != UpdateState::Staged {
            return Err(CertificationError::InvalidUpdateTransition);
        }
        let candidate = self
            .staged
            .take()
            .ok_or(CertificationError::MissingStagedRelease)?;
        self.previous = self.active.take();
        let version = candidate.version;
        self.active = Some(candidate);
        self.state = UpdateState::AwaitingHealth;
        Ok(version)
    }

    pub fn confirm_health(
        &mut self,
        release_id: &ReleaseId,
        version: u64,
        content_digest: &ContentDigest,
    ) -> Result<(), CertificationError> {
        if self.state != UpdateState::AwaitingHealth {
            return Err(CertificationError::InvalidUpdateTransition);
        }
        let active = self
            .active
            .as_ref()
            .ok_or(CertificationError::MissingActiveRelease)?;
        if !active.matches_identity(release_id, version, content_digest) {
            return Err(CertificationError::ReleaseIdentityMismatch);
        }
        self.state = UpdateState::Healthy;
        Ok(())
    }

    pub fn fail_health_and_rollback(
        &mut self,
        release_id: &ReleaseId,
        version: u64,
        content_digest: &ContentDigest,
    ) -> Result<RollbackOutcome, CertificationError> {
        if self.state != UpdateState::AwaitingHealth {
            return Err(CertificationError::InvalidUpdateTransition);
        }
        let active = self
            .active
            .as_ref()
            .ok_or(CertificationError::MissingActiveRelease)?;
        if !active.matches_identity(release_id, version, content_digest) {
            return Err(CertificationError::ReleaseIdentityMismatch);
        }
        if let Some(previous) = self.previous.take() {
            let restored_version = previous.version;
            self.active = Some(previous);
            self.state = UpdateState::RolledBack;
            return Ok(RollbackOutcome::Restored(restored_version));
        }
        self.active = None;
        self.state = UpdateState::Failed;
        Ok(RollbackOutcome::NoPreviousRelease)
    }

    #[must_use]
    pub const fn state(&self) -> UpdateState {
        self.state
    }

    #[must_use]
    pub fn active_version(&self) -> Option<u64> {
        self.active.as_ref().map(|release| release.version)
    }

    #[must_use]
    pub fn render_metadata_only(&self) -> String {
        format!(
            "schema=update-summary-v1\nstate={}\nactive_version={}\nhighest_seen_version={}\nrollback_available={}\n",
            self.state.as_str(),
            self.active_version().unwrap_or(0),
            self.highest_seen_version,
            self.previous.is_some(),
        )
    }
}

fn parse_name(value: String) -> Result<String, CertificationError> {
    let valid_length = (MIN_NAME_BYTES..=MAX_NAME_BYTES).contains(&value.len());
    let valid_chars = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if !valid_length || !valid_chars {
        return Err(CertificationError::InvalidName);
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CertificationError {
    InvalidName,
    InvalidTolerance,
    InvalidPolicy,
    DuplicateSignal,
    InvalidObservationSequence,
    EmptyObservationMatrix,
    DuplicateObservationSequence,
    UnknownSignal,
    CounterOverflow,
    StaleGrantVersion,
    GrantAlreadyActive,
    GrantAlreadyRevoked,
    MissingGrant,
    GrantRevoked,
    TimeRegression,
    InvalidContentDigest,
    InvalidReleaseVersion,
    ContentDigestMismatch,
    StaleReleaseVersion,
    UpdateBusy,
    InvalidUpdateTransition,
    MissingStagedRelease,
    MissingActiveRelease,
    ReleaseIdentityMismatch,
    RollbackUnavailable,
    VerificationEvidenceMismatch,
}

impl fmt::Display for CertificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidName => "name is invalid",
            Self::InvalidTolerance => "signal tolerance is invalid",
            Self::InvalidPolicy => "certification policy is invalid",
            Self::DuplicateSignal => "signal is duplicated",
            Self::InvalidObservationSequence => "observation sequence is invalid",
            Self::EmptyObservationMatrix => "observation matrix is empty",
            Self::DuplicateObservationSequence => "observation sequence is duplicated",
            Self::UnknownSignal => "observation contains an unknown signal",
            Self::CounterOverflow => "counter overflow",
            Self::StaleGrantVersion => "device grant version is stale",
            Self::GrantAlreadyActive => "device grant is already active",
            Self::GrantAlreadyRevoked => "device grant is already revoked",
            Self::MissingGrant => "device grant is missing",
            Self::GrantRevoked => "device grant is revoked",
            Self::TimeRegression => "time moved backwards",
            Self::InvalidContentDigest => "content digest is invalid",
            Self::InvalidReleaseVersion => "release version is invalid",
            Self::ContentDigestMismatch => "release content digest does not match",
            Self::StaleReleaseVersion => "release version is stale",
            Self::UpdateBusy => "another update is staged or awaiting health",
            Self::InvalidUpdateTransition => "update transition is invalid",
            Self::MissingStagedRelease => "staged release is missing",
            Self::MissingActiveRelease => "active release is missing",
            Self::ReleaseIdentityMismatch => "release identity does not match",
            Self::RollbackUnavailable => "rollback release is unavailable",
            Self::VerificationEvidenceMismatch => {
                "signature verification evidence does not match the release"
            }
        })
    }
}

impl std::error::Error for CertificationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(value: &str) -> Result<SignalName, CertificationError> {
        SignalName::parse(value)
    }

    fn policy() -> Result<CertificationPolicy, CertificationError> {
        CertificationPolicy::new(
            1,
            vec![
                SignalRule::new(signal("canvas.hash")?, SignalRequirement::Required, 2)?,
                SignalRule::new(signal("timezone.offset")?, SignalRequirement::Optional, 0)?,
                SignalRule::new(signal("raw.secret")?, SignalRequirement::Prohibited, 0)?,
            ],
        )
    }

    fn observation(
        sequence: u32,
        canvas: Option<i64>,
        timezone: Option<i64>,
        prohibited: Option<i64>,
    ) -> Result<ObservationSet, CertificationError> {
        let mut values = Vec::new();
        if let Some(value) = canvas {
            values.push((signal("canvas.hash")?, value));
        }
        if let Some(value) = timezone {
            values.push((signal("timezone.offset")?, value));
        }
        if let Some(value) = prohibited {
            values.push((signal("raw.secret")?, value));
        }
        ObservationSet::new(sequence, values)
    }

    fn grant_key(device: &str) -> Result<DeviceGrantKey, Box<dyn std::error::Error>> {
        Ok(DeviceGrantKey::new(
            TenantId::parse("tenant_01JSTEP10")?,
            ProfileId::parse("profile_01JSTEP10")?,
            GenerationId::parse("generation_01JSTEP10")?,
            DeviceId::parse(device)?,
        ))
    }

    fn candidate(id: &str, version: u64, byte: u8) -> Result<ReleaseCandidate, CertificationError> {
        let release_id = ReleaseId::parse(id)?;
        let content_digest = ContentDigest::new([byte; 32])?;
        let verification = PreverifiedSignatureEvidence::new(
            "synthetic_verifier_01",
            VerificationEvidenceId::parse(format!("evidence_{id}"))?,
            release_id.clone(),
            version,
            content_digest.clone(),
        )?;
        ReleaseCandidate::new(release_id, version, content_digest, verification)
    }

    #[test]
    fn stable_matrix_is_order_independent_and_deterministic()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = observation(1, Some(100), Some(60), None)?;
        let second = observation(2, Some(102), Some(60), None)?;
        let forward = evaluate_certification(&policy()?, &[first.clone(), second.clone()])?;
        let reverse = evaluate_certification(&policy()?, &[second, first])?;
        assert_eq!(forward.outcome(), CertificationOutcome::Stable);
        assert_eq!(forward.matrix_digest(), reverse.matrix_digest());
        assert_eq!(
            forward.matrix_digest().to_hex(),
            "6667869272890abc935bdfe135c849e03bcb1ba5c93cd76f588dc390fae9f765"
        );
        Ok(())
    }

    #[test]
    fn drift_missing_and_prohibited_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
        let stable = observation(1, Some(100), None, None)?;
        let drifted = observation(2, Some(110), None, None)?;
        assert_eq!(
            evaluate_certification(&policy()?, &[stable.clone(), drifted])?.outcome(),
            CertificationOutcome::Drifted
        );
        assert_eq!(
            evaluate_certification(
                &policy()?,
                &[stable.clone(), observation(2, None, None, None)?]
            )?
            .outcome(),
            CertificationOutcome::Incomplete
        );
        assert_eq!(
            evaluate_certification(
                &policy()?,
                &[stable, observation(2, Some(100), None, Some(1))?],
            )?
            .outcome(),
            CertificationOutcome::Prohibited
        );
        Ok(())
    }

    #[test]
    fn policy_and_observation_validation_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let duplicated = signal("canvas.hash")?;
        assert_eq!(
            CertificationPolicy::new(
                1,
                vec![
                    SignalRule::new(duplicated.clone(), SignalRequirement::Required, 0,)?,
                    SignalRule::new(duplicated, SignalRequirement::Optional, 0)?,
                ],
            ),
            Err(CertificationError::DuplicateSignal)
        );
        assert_eq!(
            SignalRule::new(signal("raw.secret")?, SignalRequirement::Prohibited, 1),
            Err(CertificationError::InvalidTolerance)
        );
        assert_eq!(
            CertificationPolicy::new(
                1,
                vec![SignalRule::new(
                    signal("optional.signal")?,
                    SignalRequirement::Optional,
                    0,
                )?],
            ),
            Err(CertificationError::InvalidPolicy)
        );
        let unknown = ObservationSet::new(1, vec![(signal("unknown.signal")?, 1)])?;
        assert_eq!(
            evaluate_certification(&policy()?, &[unknown]),
            Err(CertificationError::UnknownSignal)
        );
        Ok(())
    }

    #[test]
    fn device_grant_revoke_and_regrant_are_versioned() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = DeviceAuthorizationRegistry::default();
        let first_device = grant_key("device_01JSTEP10A")?;
        let second_device = grant_key("device_01JSTEP10B")?;

        let first = registry.grant(first_device.clone(), 0, UnixMillis::new(1))?;
        let second = registry.grant(second_device.clone(), 0, UnixMillis::new(2))?;
        assert_eq!(first.version(), 1);
        assert_eq!(second.status(), DeviceGrantStatus::Active);
        registry.authorize_unwrap(&first_device, 1)?;
        registry.authorize_unwrap(&second_device, 1)?;

        let revoked = registry.revoke(&first_device, 1, UnixMillis::new(3))?;
        assert_eq!(revoked.version(), 2);
        assert_eq!(
            registry.authorize_unwrap(&first_device, 2),
            Err(CertificationError::GrantRevoked)
        );
        assert!(registry.authorize_unwrap(&second_device, 1).is_ok());
        assert_eq!(
            registry.grant(first_device.clone(), 1, UnixMillis::new(4)),
            Err(CertificationError::StaleGrantVersion)
        );
        let regranted = registry.grant(first_device.clone(), 2, UnixMillis::new(4))?;
        assert_eq!(regranted.version(), 3);
        registry.authorize_unwrap(&first_device, 3)?;
        assert_eq!(registry.history().len(), 4);
        assert_eq!(registry.history()[0].snapshot().version(), 1);
        assert_eq!(
            registry.history()[2].snapshot().status(),
            DeviceGrantStatus::Revoked
        );
        assert_eq!(registry.history()[3].snapshot().version(), 3);
        assert_eq!(registry.history()[3].key(), &first_device);
        assert_eq!(
            registry.history()[3].key().device_id(),
            &DeviceId::parse("device_01JSTEP10A")?
        );
        assert_eq!(
            registry.history()[3].snapshot().changed_at(),
            UnixMillis::new(4)
        );
        Ok(())
    }

    #[test]
    fn update_requires_digest_monotonic_version_and_health_confirmation()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut controller = UpdateController::default();
        let first = candidate("release_01JSTEP10A", 1, 0x11)?;
        let first_digest = first.content_digest.clone();
        controller.stage(first, &first_digest)?;
        assert_eq!(controller.activate_staged()?, 1);
        controller.confirm_health(&ReleaseId::parse("release_01JSTEP10A")?, 1, &first_digest)?;
        assert_eq!(controller.state(), UpdateState::Healthy);

        let second = candidate("release_01JSTEP10B", 2, 0x22)?;
        let second_digest = second.content_digest.clone();
        controller.stage(second, &second_digest)?;
        assert_eq!(controller.activate_staged()?, 2);
        assert_eq!(
            controller.confirm_health(&ReleaseId::parse("release_01JSTEP10B")?, 1, &second_digest,),
            Err(CertificationError::ReleaseIdentityMismatch)
        );
        assert_eq!(
            controller.fail_health_and_rollback(
                &ReleaseId::parse("release_01JSTEP10B")?,
                2,
                &second_digest,
            )?,
            RollbackOutcome::Restored(1)
        );
        assert_eq!(controller.state(), UpdateState::RolledBack);
        assert_eq!(controller.active_version(), Some(1));
        assert_eq!(
            controller.stage(candidate("release_01JSTEP10B", 2, 0x22)?, &second_digest),
            Err(CertificationError::StaleReleaseVersion)
        );
        Ok(())
    }

    #[test]
    fn update_evidence_is_bound_to_exact_release_identity() -> Result<(), Box<dyn std::error::Error>>
    {
        let release_id = ReleaseId::parse("release_01JSTEP10BOUND")?;
        let expected_digest = ContentDigest::new([0x66; 32])?;
        let wrong_digest = ContentDigest::new([0x67; 32])?;
        let verification = PreverifiedSignatureEvidence::new(
            "synthetic_verifier_01",
            VerificationEvidenceId::parse("evidence_release_01JSTEP10BOUND")?,
            release_id.clone(),
            7,
            wrong_digest,
        )?;
        assert_eq!(
            ReleaseCandidate::new(release_id, 7, expected_digest, verification),
            Err(CertificationError::VerificationEvidenceMismatch)
        );
        Ok(())
    }

    #[test]
    fn update_rejects_wrong_digest_and_first_install_has_no_rollback()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut controller = UpdateController::default();
        let first = candidate("release_01JSTEP10C", 1, 0x33)?;
        assert_eq!(
            controller.stage(first.clone(), &ContentDigest::new([0x44; 32])?),
            Err(CertificationError::ContentDigestMismatch)
        );
        controller.stage(first, &ContentDigest::new([0x33; 32])?)?;
        controller.activate_staged()?;
        assert_eq!(
            controller.fail_health_and_rollback(
                &ReleaseId::parse("release_01JSTEP10C")?,
                1,
                &ContentDigest::new([0x33; 32])?,
            )?,
            RollbackOutcome::NoPreviousRelease
        );
        assert_eq!(controller.state(), UpdateState::Failed);
        assert_eq!(controller.active_version(), None);
        Ok(())
    }

    #[test]
    fn support_outputs_are_metadata_only() -> Result<(), Box<dyn std::error::Error>> {
        let report = evaluate_certification(
            &policy()?,
            &[
                observation(1, Some(123_456), Some(60), None)?,
                observation(2, Some(123_457), Some(60), None)?,
            ],
        )?;
        let certification = report.render_metadata_only();
        assert!(!certification.contains("canvas.hash"));
        assert!(!certification.contains("123456"));
        assert!(!certification.contains("raw.secret"));
        assert!(!certification.contains(&report.matrix_digest().to_hex()));

        let mut registry = DeviceAuthorizationRegistry::default();
        registry.grant(grant_key("device_01JSTEP10PRIVATE")?, 0, UnixMillis::new(1))?;
        let device_summary = registry.render_metadata_only();
        assert!(!device_summary.contains("device_01JSTEP10PRIVATE"));
        assert!(!device_summary.contains("generation_01JSTEP10"));

        let mut controller = UpdateController::default();
        let release = candidate("release_01JSTEP10PRIVATE", 1, 0x55)?;
        controller.stage(release, &ContentDigest::new([0x55; 32])?)?;
        let update_summary = controller.render_metadata_only();
        assert!(!update_summary.contains("release_01JSTEP10PRIVATE"));
        assert!(!update_summary.contains("synthetic_verifier_01"));
        assert!(!update_summary.contains("evidence_release"));
        Ok(())
    }
}
