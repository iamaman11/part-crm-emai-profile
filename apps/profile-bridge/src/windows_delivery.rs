#![forbid(unsafe_code)]

use bridge_domain::CAMOUHOST_IPC_VERSION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

const DELIVERY_SCHEMA_VERSION: u32 = 1;
const DELIVERY_KIND: &str = "WINDOWS_PROFILE_BRIDGE_DELIVERY";
const SIGNATURE_SCHEMA_VERSION: u32 = 1;
const SIGNATURE_KIND: &str = "WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS";
const RELEASE_SET_PREFIX: &str = "release-set-v3-sha256-";
const PROFILE_BRIDGE_PREFIX: &str = "profile-bridge-v2-sha256-";
const RUNTIME_BUNDLE_PREFIX: &str = "runtime-bundle-v2-sha256-";
const PROFILE_BRIDGE_PROTOCOL_VERSION: u32 = 1;
const RUNTIME_BUNDLE_VERSION: &str = "2.0.0";
const MAX_SIGNATURE_BYTES: usize = 1024 * 1024;
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsDeliveryManifest {
    pub schema_version: u32,
    pub kind: String,
    pub release_set_id: String,
    pub sequence: u64,
    pub source_commit_sha: String,
    pub components: WindowsDeliveryComponents,
    pub evidence: WindowsDeliveryEvidence,
    pub compatibility: WindowsDeliveryCompatibility,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsDeliveryComponents {
    pub profile_bridge: WindowsDeliveryComponent,
    pub runtime_bundle: WindowsDeliveryComponent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsDeliveryComponent {
    pub release_id: String,
    pub artifact_sha256: String,
    pub artifact_size_bytes: u64,
    pub component_manifest_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsDeliveryEvidence {
    pub sbom_sha256: String,
    pub provenance_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsDeliveryCompatibility {
    pub profile_bridge_protocol_version: u32,
    pub camouhost_ipc_version: u16,
    pub runtime_bundle_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DetachedSignatureEnvelope {
    pub schema_version: u32,
    pub kind: String,
    pub key_id: String,
    pub cms_der_hex: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustedSignerStatus {
    Active,
    AcceptedPrevious,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedSigner {
    key_id: String,
    certificate_sha256: String,
    status: TrustedSignerStatus,
}

impl TrustedSigner {
    pub fn new(
        key_id: impl Into<String>,
        certificate_sha256: impl Into<String>,
        status: TrustedSignerStatus,
    ) -> Result<Self, DeliveryPolicyError> {
        let key_id = key_id.into();
        let certificate_sha256 = certificate_sha256.into();
        if !valid_key_id(&key_id) || !is_lower_hex(&certificate_sha256, 64) {
            return Err(DeliveryPolicyError::InvalidTrustPolicy);
        }
        Ok(Self {
            key_id,
            certificate_sha256,
            status,
        })
    }

    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    #[must_use]
    pub fn certificate_sha256(&self) -> &str {
        &self.certificate_sha256
    }

    #[must_use]
    pub const fn status(&self) -> TrustedSignerStatus {
        self.status
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedSignerSet {
    signers: Vec<TrustedSigner>,
}

impl TrustedSignerSet {
    pub fn new(
        signers: impl IntoIterator<Item = TrustedSigner>,
    ) -> Result<Self, DeliveryPolicyError> {
        let signers: Vec<_> = signers.into_iter().collect();
        if signers.is_empty() {
            return Err(DeliveryPolicyError::InvalidTrustPolicy);
        }
        for (index, signer) in signers.iter().enumerate() {
            if signers[..index]
                .iter()
                .any(|existing| existing.key_id == signer.key_id)
            {
                return Err(DeliveryPolicyError::InvalidTrustPolicy);
            }
        }
        if signers
            .iter()
            .filter(|signer| signer.status == TrustedSignerStatus::Active)
            .count()
            != 1
        {
            return Err(DeliveryPolicyError::InvalidTrustPolicy);
        }
        Ok(Self { signers })
    }

    fn admitted(&self, key_id: &str) -> Result<&TrustedSigner, DeliveryPolicyError> {
        let signer = self
            .signers
            .iter()
            .find(|signer| signer.key_id == key_id)
            .ok_or(DeliveryPolicyError::UnknownSigner)?;
        match signer.status {
            TrustedSignerStatus::Active | TrustedSignerStatus::AcceptedPrevious => Ok(signer),
            TrustedSignerStatus::Revoked => Err(DeliveryPolicyError::RevokedSigner),
        }
    }
}

pub trait DetachedSignatureVerifier {
    type Error;

    fn verify_cms(
        &mut self,
        manifest_bytes: &[u8],
        cms_der: &[u8],
        expected_certificate_sha256: &str,
    ) -> Result<bool, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedDeliveryFloor {
    sequence: u64,
    release_set_id: String,
    manifest_sha256: String,
}

impl AcceptedDeliveryFloor {
    #[must_use]
    pub fn from_candidate(candidate: &VerifiedDeliveryCandidate) -> Self {
        Self::from_identity(&candidate.identity())
    }

    #[must_use]
    pub fn from_identity(identity: &DeliveryIdentity) -> Self {
        Self {
            sequence: identity.sequence,
            release_set_id: identity.release_set_id.clone(),
            manifest_sha256: identity.manifest_sha256.clone(),
        }
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDeliveryCandidate {
    manifest: WindowsDeliveryManifest,
    manifest_sha256: String,
    signer_key_id: String,
}

impl VerifiedDeliveryCandidate {
    #[must_use]
    pub const fn manifest(&self) -> &WindowsDeliveryManifest {
        &self.manifest
    }

    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    #[must_use]
    pub fn signer_key_id(&self) -> &str {
        &self.signer_key_id
    }

    #[must_use]
    pub fn identity(&self) -> DeliveryIdentity {
        DeliveryIdentity {
            sequence: self.manifest.sequence,
            release_set_id: self.manifest.release_set_id.clone(),
            manifest_sha256: self.manifest_sha256.clone(),
            profile_bridge_release_id: self.manifest.components.profile_bridge.release_id.clone(),
            runtime_bundle_release_id: self.manifest.components.runtime_bundle.release_id.clone(),
        }
    }
}

pub fn verify_delivery_candidate<V: DetachedSignatureVerifier>(
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
    trust: &TrustedSignerSet,
    accepted_floor: Option<&AcceptedDeliveryFloor>,
    verifier: &mut V,
) -> Result<VerifiedDeliveryCandidate, DeliveryPolicyError> {
    let manifest: WindowsDeliveryManifest =
        serde_json::from_slice(manifest_bytes).map_err(|_| DeliveryPolicyError::InvalidManifest)?;
    validate_manifest(&manifest)?;
    let canonical_bytes =
        serde_json::to_vec(&manifest).map_err(|_| DeliveryPolicyError::InvalidManifest)?;
    if canonical_bytes != manifest_bytes {
        return Err(DeliveryPolicyError::NonCanonicalManifest);
    }

    let envelope: DetachedSignatureEnvelope = serde_json::from_slice(signature_bytes)
        .map_err(|_| DeliveryPolicyError::InvalidSignatureEnvelope)?;
    if envelope.schema_version != SIGNATURE_SCHEMA_VERSION
        || envelope.kind != SIGNATURE_KIND
        || !valid_key_id(&envelope.key_id)
    {
        return Err(DeliveryPolicyError::InvalidSignatureEnvelope);
    }
    let cms_der = decode_lower_hex(&envelope.cms_der_hex)
        .ok_or(DeliveryPolicyError::InvalidSignatureEnvelope)?;
    if cms_der.is_empty() || cms_der.len() > MAX_SIGNATURE_BYTES {
        return Err(DeliveryPolicyError::InvalidSignatureEnvelope);
    }

    let signer = trust.admitted(&envelope.key_id)?;
    let signature_valid = verifier
        .verify_cms(manifest_bytes, &cms_der, signer.certificate_sha256())
        .map_err(|_| DeliveryPolicyError::SignatureVerificationFailed)?;
    if !signature_valid {
        return Err(DeliveryPolicyError::SignatureInvalid);
    }

    let manifest_sha256 = sha256_hex(manifest_bytes);
    if let Some(floor) = accepted_floor {
        if manifest.sequence < floor.sequence {
            return Err(DeliveryPolicyError::DowngradeRejected);
        }
        if manifest.sequence == floor.sequence
            && (manifest.release_set_id != floor.release_set_id
                || manifest_sha256 != floor.manifest_sha256)
        {
            return Err(DeliveryPolicyError::ReplayConflict);
        }
    }

    Ok(VerifiedDeliveryCandidate {
        manifest,
        manifest_sha256,
        signer_key_id: envelope.key_id,
    })
}

fn validate_manifest(manifest: &WindowsDeliveryManifest) -> Result<(), DeliveryPolicyError> {
    if manifest.schema_version != DELIVERY_SCHEMA_VERSION
        || manifest.kind != DELIVERY_KIND
        || manifest.sequence == 0
        || !prefixed_sha256(&manifest.release_set_id, RELEASE_SET_PREFIX)
        || !is_lower_hex(&manifest.source_commit_sha, 40)
    {
        return Err(DeliveryPolicyError::InvalidManifest);
    }
    validate_component(&manifest.components.profile_bridge, PROFILE_BRIDGE_PREFIX)?;
    validate_component(&manifest.components.runtime_bundle, RUNTIME_BUNDLE_PREFIX)?;
    if !is_lower_hex(&manifest.evidence.sbom_sha256, 64)
        || !is_lower_hex(&manifest.evidence.provenance_sha256, 64)
        || manifest.compatibility.profile_bridge_protocol_version != PROFILE_BRIDGE_PROTOCOL_VERSION
        || manifest.compatibility.camouhost_ipc_version != CAMOUHOST_IPC_VERSION
        || manifest.compatibility.runtime_bundle_version != RUNTIME_BUNDLE_VERSION
    {
        return Err(DeliveryPolicyError::IncompatibleCandidate);
    }
    Ok(())
}

fn validate_component(
    component: &WindowsDeliveryComponent,
    release_prefix: &str,
) -> Result<(), DeliveryPolicyError> {
    if !prefixed_sha256(&component.release_id, release_prefix)
        || component.artifact_size_bytes == 0
        || !is_lower_hex(&component.artifact_sha256, 64)
        || !is_lower_hex(&component.component_manifest_sha256, 64)
    {
        return Err(DeliveryPolicyError::InvalidManifest);
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryIdentity {
    pub sequence: u64,
    pub release_set_id: String,
    pub manifest_sha256: String,
    pub profile_bridge_release_id: String,
    pub runtime_bundle_release_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryActivationOutcome {
    PendingHealth,
    HealthAttemptStarted,
    Healthy,
    RolledBack,
    RecoveryRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryActivationEvidence {
    pub attempt: u64,
    pub candidate: DeliveryIdentity,
    pub outcome: DeliveryActivationOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryFailureKind {
    HealthRejected,
    InterruptedActivation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryFailureEvidence {
    pub candidate: DeliveryIdentity,
    pub kind: DeliveryFailureKind,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryState {
    active: Option<DeliveryIdentity>,
    active_health_confirmed: bool,
    last_known_good: Option<DeliveryIdentity>,
    staged: Option<DeliveryIdentity>,
    activation_generation: u64,
    last_activation: Option<DeliveryActivationEvidence>,
    last_failure: Option<DeliveryFailureEvidence>,
}

impl DeliveryState {
    #[must_use]
    pub const fn active(&self) -> Option<&DeliveryIdentity> {
        self.active.as_ref()
    }

    #[must_use]
    pub const fn active_health_confirmed(&self) -> bool {
        self.active_health_confirmed
    }

    #[must_use]
    pub const fn last_known_good(&self) -> Option<&DeliveryIdentity> {
        self.last_known_good.as_ref()
    }

    #[must_use]
    pub const fn staged(&self) -> Option<&DeliveryIdentity> {
        self.staged.as_ref()
    }

    #[must_use]
    pub const fn activation_generation(&self) -> u64 {
        self.activation_generation
    }

    #[must_use]
    pub const fn last_activation(&self) -> Option<&DeliveryActivationEvidence> {
        self.last_activation.as_ref()
    }

    #[must_use]
    pub const fn last_failure(&self) -> Option<&DeliveryFailureEvidence> {
        self.last_failure.as_ref()
    }

    pub fn validate_persisted(&self) -> Result<(), DeliveryStateError> {
        for identity in [
            self.active.as_ref(),
            self.last_known_good.as_ref(),
            self.staged.as_ref(),
            self.last_activation
                .as_ref()
                .map(|evidence| &evidence.candidate),
            self.last_failure.as_ref().map(|failure| &failure.candidate),
        ]
        .into_iter()
        .flatten()
        {
            if !valid_delivery_identity(identity) {
                return Err(DeliveryStateError::CorruptPersistedState);
            }
        }
        if self.active_health_confirmed && self.active.is_none() {
            return Err(DeliveryStateError::CorruptPersistedState);
        }
        if self.last_known_good.is_some() && self.active.is_none() {
            return Err(DeliveryStateError::CorruptPersistedState);
        }
        if self.active.as_ref() == self.last_known_good.as_ref() && self.active.is_some() {
            return Err(DeliveryStateError::CorruptPersistedState);
        }
        if let (Some(active), Some(staged)) = (&self.active, &self.staged)
            && (staged.sequence < active.sequence
                || (staged.sequence == active.sequence && staged != active))
        {
            return Err(DeliveryStateError::CorruptPersistedState);
        }
        match &self.last_activation {
            None if self.activation_generation != 0 => {
                return Err(DeliveryStateError::CorruptPersistedState);
            }
            Some(evidence)
                if evidence.attempt == 0 || evidence.attempt > self.activation_generation =>
            {
                return Err(DeliveryStateError::CorruptPersistedState);
            }
            Some(evidence) => match evidence.outcome {
                DeliveryActivationOutcome::PendingHealth
                | DeliveryActivationOutcome::HealthAttemptStarted
                    if self.active.as_ref() != Some(&evidence.candidate)
                        || self.active_health_confirmed =>
                {
                    return Err(DeliveryStateError::CorruptPersistedState);
                }
                DeliveryActivationOutcome::Healthy
                    if self.active.as_ref() != Some(&evidence.candidate)
                        || !self.active_health_confirmed
                        || self.last_failure.is_some() =>
                {
                    return Err(DeliveryStateError::CorruptPersistedState);
                }
                DeliveryActivationOutcome::RolledBack
                    if self.active.as_ref() == Some(&evidence.candidate)
                        || !self.active_health_confirmed =>
                {
                    return Err(DeliveryStateError::CorruptPersistedState);
                }
                DeliveryActivationOutcome::RecoveryRequired
                    if self.active.is_some() || self.active_health_confirmed =>
                {
                    return Err(DeliveryStateError::CorruptPersistedState);
                }
                _ => {}
            },
            None => {}
        }
        if let Some(failure) = &self.last_failure {
            let Some(activation) = &self.last_activation else {
                return Err(DeliveryStateError::CorruptPersistedState);
            };
            if failure.candidate != activation.candidate
                || !matches!(
                    activation.outcome,
                    DeliveryActivationOutcome::RolledBack
                        | DeliveryActivationOutcome::RecoveryRequired
                )
            {
                return Err(DeliveryStateError::CorruptPersistedState);
            }
        }
        Ok(())
    }

    pub fn stage(
        &mut self,
        candidate: &VerifiedDeliveryCandidate,
    ) -> Result<(), DeliveryStateError> {
        self.validate_persisted()?;
        let identity = candidate.identity();
        if let Some(active) = &self.active {
            if identity.sequence < active.sequence {
                return Err(DeliveryStateError::DowngradeRejected);
            }
            if identity.sequence == active.sequence {
                if identity == *active {
                    self.staged = Some(identity);
                    return Ok(());
                }
                return Err(DeliveryStateError::ReplayConflict);
            }
        }
        if let Some(staged) = &self.staged {
            if identity.sequence < staged.sequence {
                return Err(DeliveryStateError::DowngradeRejected);
            }
            if identity.sequence == staged.sequence && identity != *staged {
                return Err(DeliveryStateError::ReplayConflict);
            }
        }
        self.staged = Some(identity);
        Ok(())
    }

    pub fn activate_staged(&mut self, quiescent: bool) -> Result<(), DeliveryStateError> {
        self.validate_persisted()?;
        if !quiescent {
            return Err(DeliveryStateError::ActiveRuntime);
        }
        let staged = self
            .staged
            .take()
            .ok_or(DeliveryStateError::NoStagedCandidate)?;
        if self.active.as_ref() == Some(&staged) {
            return Ok(());
        }
        if self.active.is_some() && !self.active_health_confirmed {
            self.staged = Some(staged);
            return Err(DeliveryStateError::HealthPending);
        }
        let attempt = self
            .activation_generation
            .checked_add(1)
            .ok_or(DeliveryStateError::AttemptCounterExhausted)?;
        if self.active_health_confirmed {
            self.last_known_good = self.active.take();
        } else {
            self.active = None;
        }
        self.active = Some(staged.clone());
        self.active_health_confirmed = false;
        self.activation_generation = attempt;
        self.last_activation = Some(DeliveryActivationEvidence {
            attempt,
            candidate: staged,
            outcome: DeliveryActivationOutcome::PendingHealth,
        });
        self.last_failure = None;
        Ok(())
    }

    pub fn start_health_attempt(
        &mut self,
        candidate: &DeliveryIdentity,
        attempt: u64,
    ) -> Result<(), DeliveryStateError> {
        self.validate_persisted()?;
        if attempt == 0
            || self.active.as_ref() != Some(candidate)
            || self.active_health_confirmed
            || self.activation_generation != attempt
        {
            return Err(DeliveryStateError::HealthAttemptMismatch);
        }
        let activation = self
            .last_activation
            .as_mut()
            .ok_or(DeliveryStateError::CorruptPersistedState)?;
        if activation.attempt != attempt || activation.candidate != *candidate {
            return Err(DeliveryStateError::HealthAttemptMismatch);
        }
        match activation.outcome {
            DeliveryActivationOutcome::PendingHealth => {
                activation.outcome = DeliveryActivationOutcome::HealthAttemptStarted;
                Ok(())
            }
            DeliveryActivationOutcome::HealthAttemptStarted => Ok(()),
            _ => Err(DeliveryStateError::HealthAttemptNotStarted),
        }
    }

    pub fn confirm_health(&mut self) -> Result<(), DeliveryStateError> {
        self.validate_persisted()?;
        let active = self
            .active
            .as_ref()
            .ok_or(DeliveryStateError::NoActiveCandidate)?;
        let activation = self
            .last_activation
            .as_mut()
            .ok_or(DeliveryStateError::CorruptPersistedState)?;
        if activation.candidate != *active {
            return Err(DeliveryStateError::CorruptPersistedState);
        }
        if activation.outcome != DeliveryActivationOutcome::HealthAttemptStarted {
            return Err(DeliveryStateError::HealthAttemptNotStarted);
        }
        activation.outcome = DeliveryActivationOutcome::Healthy;
        self.active_health_confirmed = true;
        self.last_failure = None;
        Ok(())
    }

    pub fn fail_health_and_rollback(&mut self) -> Result<(), DeliveryStateError> {
        self.rollback_after_failure(DeliveryFailureKind::HealthRejected)
    }

    pub fn recover_interrupted_activation(&mut self) -> Result<(), DeliveryStateError> {
        self.rollback_after_failure(DeliveryFailureKind::InterruptedActivation)
    }

    fn rollback_after_failure(
        &mut self,
        kind: DeliveryFailureKind,
    ) -> Result<(), DeliveryStateError> {
        self.validate_persisted()?;
        let failed = self
            .active
            .as_ref()
            .cloned()
            .ok_or(DeliveryStateError::NoActiveCandidate)?;
        let activation = self
            .last_activation
            .as_ref()
            .ok_or(DeliveryStateError::CorruptPersistedState)?;
        if activation.candidate != failed {
            return Err(DeliveryStateError::HealthAttemptMismatch);
        }
        if activation.outcome != DeliveryActivationOutcome::HealthAttemptStarted {
            return Err(DeliveryStateError::HealthAttemptNotStarted);
        }

        self.active = None;
        self.active_health_confirmed = false;
        self.staged = None;
        self.last_failure = Some(DeliveryFailureEvidence {
            candidate: failed,
            kind,
        });
        let outcome = if let Some(lkg) = self.last_known_good.take() {
            self.active = Some(lkg);
            self.active_health_confirmed = true;
            DeliveryActivationOutcome::RolledBack
        } else {
            DeliveryActivationOutcome::RecoveryRequired
        };
        if let Some(activation) = self.last_activation.as_mut() {
            activation.outcome = outcome;
        }
        if outcome == DeliveryActivationOutcome::RecoveryRequired {
            Err(DeliveryStateError::RecoveryRequired)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryPolicyError {
    InvalidManifest,
    NonCanonicalManifest,
    IncompatibleCandidate,
    InvalidSignatureEnvelope,
    InvalidTrustPolicy,
    UnknownSigner,
    RevokedSigner,
    SignatureVerificationFailed,
    SignatureInvalid,
    DowngradeRejected,
    ReplayConflict,
}

impl fmt::Display for DeliveryPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidManifest => "Windows delivery manifest is invalid",
            Self::NonCanonicalManifest => "Windows delivery manifest is not canonical",
            Self::IncompatibleCandidate => "Windows delivery candidate is incompatible",
            Self::InvalidSignatureEnvelope => "Windows delivery signature envelope is invalid",
            Self::InvalidTrustPolicy => "Windows delivery trust policy is invalid",
            Self::UnknownSigner => "Windows delivery signer is unknown",
            Self::RevokedSigner => "Windows delivery signer is revoked",
            Self::SignatureVerificationFailed => "Windows delivery signature verification failed",
            Self::SignatureInvalid => "Windows delivery signature is invalid",
            Self::DowngradeRejected => "Windows delivery downgrade is rejected",
            Self::ReplayConflict => "Windows delivery sequence conflicts with accepted identity",
        })
    }
}

impl std::error::Error for DeliveryPolicyError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryStateError {
    DowngradeRejected,
    ReplayConflict,
    ActiveRuntime,
    HealthPending,
    HealthAttemptMismatch,
    HealthAttemptNotStarted,
    NoStagedCandidate,
    NoActiveCandidate,
    RecoveryRequired,
    AttemptCounterExhausted,
    CorruptPersistedState,
}

impl fmt::Display for DeliveryStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DowngradeRejected => "Windows delivery downgrade is rejected",
            Self::ReplayConflict => "Windows delivery sequence conflicts with staged identity",
            Self::ActiveRuntime => "Windows delivery activation requires quiescence",
            Self::HealthPending => {
                "Windows delivery active release has not passed health confirmation"
            }
            Self::HealthAttemptMismatch => {
                "Windows delivery health attempt identity is inconsistent"
            }
            Self::HealthAttemptNotStarted => {
                "Windows delivery health attempt has not durably started"
            }
            Self::NoStagedCandidate => "Windows delivery has no staged candidate",
            Self::NoActiveCandidate => "Windows delivery has no active candidate",
            Self::RecoveryRequired => "Windows delivery has no last known good candidate",
            Self::AttemptCounterExhausted => "Windows delivery activation counter is exhausted",
            Self::CorruptPersistedState => "Windows delivery persisted state is inconsistent",
        })
    }
}

impl std::error::Error for DeliveryStateError {}

fn valid_delivery_identity(identity: &DeliveryIdentity) -> bool {
    identity.sequence != 0
        && prefixed_sha256(&identity.release_set_id, RELEASE_SET_PREFIX)
        && is_lower_hex(&identity.manifest_sha256, 64)
        && prefixed_sha256(&identity.profile_bridge_release_id, PROFILE_BRIDGE_PREFIX)
        && prefixed_sha256(&identity.runtime_bundle_release_id, RUNTIME_BUNDLE_PREFIX)
}

fn prefixed_sha256(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|digest| is_lower_hex(digest, 64))
}

fn valid_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn decode_lower_hex(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() || !value.len().is_multiple_of(2) || !is_lower_hex(value, value.len()) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const CERT_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const CERT_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    struct DigestBoundVerifier;

    impl DetachedSignatureVerifier for DigestBoundVerifier {
        type Error = ();

        fn verify_cms(
            &mut self,
            manifest_bytes: &[u8],
            cms_der: &[u8],
            expected_certificate_sha256: &str,
        ) -> Result<bool, Self::Error> {
            Ok(cms_der == Sha256::digest(manifest_bytes).as_slice()
                && matches!(expected_certificate_sha256, CERT_A | CERT_B))
        }
    }

    fn manifest(sequence: u64, suffix: char) -> WindowsDeliveryManifest {
        let digest: String = std::iter::repeat_n(suffix, 64).collect();
        WindowsDeliveryManifest {
            schema_version: DELIVERY_SCHEMA_VERSION,
            kind: DELIVERY_KIND.to_owned(),
            release_set_id: format!("{RELEASE_SET_PREFIX}{digest}"),
            sequence,
            source_commit_sha: "1".repeat(40),
            components: WindowsDeliveryComponents {
                profile_bridge: WindowsDeliveryComponent {
                    release_id: format!("{PROFILE_BRIDGE_PREFIX}{digest}"),
                    artifact_sha256: digest.clone(),
                    artifact_size_bytes: 123,
                    component_manifest_sha256: digest.clone(),
                },
                runtime_bundle: WindowsDeliveryComponent {
                    release_id: format!("{RUNTIME_BUNDLE_PREFIX}{digest}"),
                    artifact_sha256: digest.clone(),
                    artifact_size_bytes: 456,
                    component_manifest_sha256: digest.clone(),
                },
            },
            evidence: WindowsDeliveryEvidence {
                sbom_sha256: digest.clone(),
                provenance_sha256: digest,
            },
            compatibility: WindowsDeliveryCompatibility {
                profile_bridge_protocol_version: PROFILE_BRIDGE_PROTOCOL_VERSION,
                camouhost_ipc_version: CAMOUHOST_IPC_VERSION,
                runtime_bundle_version: RUNTIME_BUNDLE_VERSION.to_owned(),
            },
        }
    }

    fn signature_for(manifest_bytes: &[u8], key_id: &str) -> Vec<u8> {
        let digest = Sha256::digest(manifest_bytes);
        let mut cms_der_hex = String::with_capacity(64);
        for byte in digest {
            cms_der_hex.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
            cms_der_hex.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
        }
        format!(
            "{{\"schema_version\":{SIGNATURE_SCHEMA_VERSION},\"kind\":\"{SIGNATURE_KIND}\",\"key_id\":\"{key_id}\",\"cms_der_hex\":\"{cms_der_hex}\"}}"
        )
        .into_bytes()
    }

    fn trust() -> Result<TrustedSignerSet, DeliveryPolicyError> {
        let active = TrustedSigner::new("release-2026", CERT_A, TrustedSignerStatus::Active)?;
        let previous = TrustedSigner::new(
            "release-2025",
            CERT_B,
            TrustedSignerStatus::AcceptedPrevious,
        )?;
        TrustedSignerSet::new([active, previous])
    }

    fn revoked_trust() -> Result<TrustedSignerSet, DeliveryPolicyError> {
        let revoked = TrustedSigner::new("release-2026", CERT_A, TrustedSignerStatus::Revoked)?;
        let active = TrustedSigner::new("release-2027", CERT_B, TrustedSignerStatus::Active)?;
        TrustedSignerSet::new([revoked, active])
    }

    fn verify_with_key(
        manifest: &WindowsDeliveryManifest,
        trust: &TrustedSignerSet,
        floor: Option<&AcceptedDeliveryFloor>,
        key_id: &str,
    ) -> Result<VerifiedDeliveryCandidate, DeliveryPolicyError> {
        let bytes =
            serde_json::to_vec(manifest).map_err(|_| DeliveryPolicyError::InvalidManifest)?;
        let signature = signature_for(&bytes, key_id);
        verify_delivery_candidate(&bytes, &signature, trust, floor, &mut DigestBoundVerifier)
    }

    fn verify(
        manifest: &WindowsDeliveryManifest,
        trust: &TrustedSignerSet,
        floor: Option<&AcceptedDeliveryFloor>,
    ) -> Result<VerifiedDeliveryCandidate, DeliveryPolicyError> {
        verify_with_key(manifest, trust, floor, "release-2026")
    }

    #[test]
    fn exact_signed_candidate_is_admitted_and_same_identity_is_idempotent() -> TestResult {
        let trust = trust()?;
        let first_manifest = manifest(7, 'a');
        let first = verify(&first_manifest, &trust, None)?;
        let floor = AcceptedDeliveryFloor::from_candidate(&first);
        let replay = verify(&first_manifest, &trust, Some(&floor))?;
        assert_eq!(replay.identity(), first.identity());
        assert_eq!(
            first.identity().profile_bridge_release_id,
            first_manifest.components.profile_bridge.release_id
        );
        assert_eq!(
            first.identity().runtime_bundle_release_id,
            first_manifest.components.runtime_bundle.release_id
        );
        Ok(())
    }

    #[test]
    fn accepted_previous_signer_supports_rotation_overlap() -> TestResult {
        let trust = trust()?;
        let candidate = verify_with_key(&manifest(7, 'a'), &trust, None, "release-2025")?;
        assert_eq!(candidate.signer_key_id(), "release-2025");
        Ok(())
    }

    #[test]
    fn candidate_tamper_and_noncanonical_manifest_fail_closed() -> TestResult {
        let trust = trust()?;
        let original = serde_json::to_vec(&manifest(7, 'a'))?;
        let signature = signature_for(&original, "release-2026");
        let tampered = serde_json::to_vec(&manifest(8, 'a'))?;
        assert_eq!(
            verify_delivery_candidate(
                &tampered,
                &signature,
                &trust,
                None,
                &mut DigestBoundVerifier,
            ),
            Err(DeliveryPolicyError::SignatureInvalid)
        );

        let mut noncanonical = original.clone();
        noncanonical.push(b'\n');
        let noncanonical_signature = signature_for(&noncanonical, "release-2026");
        assert_eq!(
            verify_delivery_candidate(
                &noncanonical,
                &noncanonical_signature,
                &trust,
                None,
                &mut DigestBoundVerifier,
            ),
            Err(DeliveryPolicyError::NonCanonicalManifest)
        );
        Ok(())
    }

    #[test]
    fn revoked_unknown_and_ambiguous_trust_fail_closed() -> TestResult {
        let revoked = revoked_trust()?;
        assert_eq!(
            verify(&manifest(7, 'a'), &revoked, None),
            Err(DeliveryPolicyError::RevokedSigner)
        );

        let active = TrustedSigner::new("other", CERT_A, TrustedSignerStatus::Active)?;
        let unknown = TrustedSignerSet::new([active])?;
        let bytes = serde_json::to_vec(&manifest(7, 'a'))?;
        let signature = signature_for(&bytes, "release-2026");
        assert_eq!(
            verify_delivery_candidate(&bytes, &signature, &unknown, None, &mut DigestBoundVerifier),
            Err(DeliveryPolicyError::UnknownSigner)
        );

        let active_a = TrustedSigner::new("a", CERT_A, TrustedSignerStatus::Active)?;
        let active_b = TrustedSigner::new("b", CERT_B, TrustedSignerStatus::Active)?;
        assert_eq!(
            TrustedSignerSet::new([active_a, active_b]),
            Err(DeliveryPolicyError::InvalidTrustPolicy)
        );
        Ok(())
    }

    #[test]
    fn incompatible_manifest_and_malformed_signature_fail_closed() -> TestResult {
        let trust = trust()?;
        let mut incompatible = manifest(7, 'a');
        incompatible.compatibility.runtime_bundle_version = "3.0.0".to_owned();
        assert_eq!(
            verify(&incompatible, &trust, None),
            Err(DeliveryPolicyError::IncompatibleCandidate)
        );

        let mut predecessor = manifest(7, 'a');
        predecessor.components.runtime_bundle.release_id =
            format!("runtime-bundle-v1-sha256-{}", "a".repeat(64));
        assert_eq!(
            verify(&predecessor, &trust, None),
            Err(DeliveryPolicyError::InvalidManifest)
        );

        let bytes = serde_json::to_vec(&manifest(7, 'a'))?;
        assert_eq!(
            verify_delivery_candidate(
                &bytes,
                br#"{\"schema_version\":1,\"kind\":\"WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS\",\"key_id\":\"release-2026\",\"cms_der_hex\":\"ZZ\"}"#,
                &trust,
                None,
                &mut DigestBoundVerifier,
            ),
            Err(DeliveryPolicyError::InvalidSignatureEnvelope)
        );
        Ok(())
    }

    #[test]
    fn downgrade_and_same_sequence_conflict_are_rejected() -> TestResult {
        let trust = trust()?;
        let accepted = verify(&manifest(7, 'a'), &trust, None)?;
        let floor = AcceptedDeliveryFloor::from_candidate(&accepted);
        assert_eq!(
            verify(&manifest(6, 'b'), &trust, Some(&floor)),
            Err(DeliveryPolicyError::DowngradeRejected)
        );
        assert_eq!(
            verify(&manifest(7, 'b'), &trust, Some(&floor)),
            Err(DeliveryPolicyError::ReplayConflict)
        );
        Ok(())
    }

    #[test]
    fn delivery_state_never_promotes_unhealthy_active_to_lkg() -> TestResult {
        let trust = trust()?;
        let first = verify(&manifest(1, 'a'), &trust, None)?;
        let second = verify(&manifest(2, 'b'), &trust, None)?;
        let mut state = DeliveryState::default();

        state.stage(&first)?;
        assert_eq!(
            state.activate_staged(false),
            Err(DeliveryStateError::ActiveRuntime)
        );
        state.activate_staged(true)?;
        let first_identity = first.identity();
        assert!(!state.active_health_confirmed());
        assert_eq!(state.activation_generation(), 1);
        assert_eq!(
            state.last_activation().map(|evidence| evidence.outcome),
            Some(DeliveryActivationOutcome::PendingHealth)
        );
        assert_eq!(
            state.confirm_health(),
            Err(DeliveryStateError::HealthAttemptNotStarted)
        );
        assert_eq!(
            state.start_health_attempt(&second.identity(), 1),
            Err(DeliveryStateError::HealthAttemptMismatch)
        );
        state.start_health_attempt(&first_identity, 1)?;
        state.start_health_attempt(&first_identity, 1)?;
        assert_eq!(
            state.last_activation().map(|evidence| evidence.outcome),
            Some(DeliveryActivationOutcome::HealthAttemptStarted)
        );

        state.stage(&second)?;
        assert_eq!(
            state.activate_staged(true),
            Err(DeliveryStateError::HealthPending)
        );
        assert_eq!(state.staged(), Some(&second.identity()));
        assert!(state.last_known_good().is_none());

        state.confirm_health()?;
        state.activate_staged(true)?;
        assert_eq!(state.last_known_good(), Some(&first_identity));
        assert!(!state.active_health_confirmed());
        assert_eq!(state.activation_generation(), 2);
        Ok(())
    }

    #[test]
    fn health_failure_rolls_back_only_to_confirmed_lkg() -> TestResult {
        let trust = trust()?;
        let first = verify(&manifest(1, 'a'), &trust, None)?;
        let second = verify(&manifest(2, 'b'), &trust, None)?;
        let first_identity = first.identity();
        let second_identity = second.identity();
        let mut state = DeliveryState::default();

        state.stage(&first)?;
        state.activate_staged(true)?;
        assert_eq!(
            state.fail_health_and_rollback(),
            Err(DeliveryStateError::HealthAttemptNotStarted)
        );
        assert_eq!(state.active(), Some(&first_identity));
        state.start_health_attempt(&first_identity, 1)?;
        assert_eq!(
            state.fail_health_and_rollback(),
            Err(DeliveryStateError::RecoveryRequired)
        );
        assert!(state.active().is_none());
        assert_eq!(
            state.last_activation().map(|evidence| evidence.outcome),
            Some(DeliveryActivationOutcome::RecoveryRequired)
        );

        state.stage(&first)?;
        state.activate_staged(true)?;
        let first_retry_attempt = state.activation_generation();
        state.start_health_attempt(&first_identity, first_retry_attempt)?;
        state.confirm_health()?;
        state.stage(&second)?;
        state.activate_staged(true)?;
        let second_attempt = state.activation_generation();
        state.start_health_attempt(&second_identity, second_attempt)?;
        state.fail_health_and_rollback()?;
        assert_eq!(state.active(), Some(&first_identity));
        assert!(state.active_health_confirmed());
        assert!(state.staged().is_none());
        assert_eq!(
            state.last_failure().map(|failure| failure.kind),
            Some(DeliveryFailureKind::HealthRejected)
        );
        Ok(())
    }

    #[test]
    fn interrupted_activation_recovers_only_a_durably_started_attempt() -> TestResult {
        let trust = trust()?;
        let first = verify(&manifest(1, 'a'), &trust, None)?;
        let second = verify(&manifest(2, 'b'), &trust, None)?;
        let first_identity = first.identity();
        let second_identity = second.identity();
        let mut state = DeliveryState::default();
        state.stage(&first)?;
        state.activate_staged(true)?;
        state.start_health_attempt(&first_identity, 1)?;
        state.confirm_health()?;
        state.stage(&second)?;
        state.activate_staged(true)?;
        assert_eq!(
            state.recover_interrupted_activation(),
            Err(DeliveryStateError::HealthAttemptNotStarted)
        );
        state.start_health_attempt(&second_identity, 2)?;

        let persisted = serde_json::to_vec(&state)?;
        let mut restored: DeliveryState = serde_json::from_slice(&persisted)?;
        restored.validate_persisted()?;
        restored.recover_interrupted_activation()?;
        assert_eq!(restored.active(), Some(&first_identity));
        assert!(restored.active_health_confirmed());
        assert_eq!(
            restored.last_failure().map(|failure| failure.kind),
            Some(DeliveryFailureKind::InterruptedActivation)
        );
        assert_eq!(
            restored.last_activation().map(|evidence| evidence.outcome),
            Some(DeliveryActivationOutcome::RolledBack)
        );
        assert_eq!(
            restored.recover_interrupted_activation(),
            Err(DeliveryStateError::HealthAttemptMismatch)
        );
        Ok(())
    }

    #[test]
    fn persisted_state_inconsistency_fails_closed() -> TestResult {
        let trust = trust()?;
        let first = verify(&manifest(1, 'a'), &trust, None)?;
        let first_identity = first.identity();
        let mut state = DeliveryState::default();
        state.stage(&first)?;
        state.activate_staged(true)?;
        state.start_health_attempt(&first_identity, 1)?;
        state.confirm_health()?;
        let mut value = serde_json::to_value(&state)?;
        value["active_health_confirmed"] = serde_json::Value::Bool(false);
        let corrupt: DeliveryState = serde_json::from_value(value)?;
        assert_eq!(
            corrupt.validate_persisted(),
            Err(DeliveryStateError::CorruptPersistedState)
        );

        let mut value = serde_json::to_value(&state)?;
        value["active"]["runtime_bundle_release_id"] =
            serde_json::Value::String("runtime-bundle-v1-sha256-".to_owned() + &"a".repeat(64));
        let corrupt: DeliveryState = serde_json::from_value(value)?;
        assert_eq!(
            corrupt.validate_persisted(),
            Err(DeliveryStateError::CorruptPersistedState)
        );
        Ok(())
    }

    #[test]
    fn exact_active_candidate_reactivation_is_idempotent() -> TestResult {
        let trust = trust()?;
        let first = verify(&manifest(1, 'a'), &trust, None)?;
        let first_identity = first.identity();
        let mut state = DeliveryState::default();
        state.stage(&first)?;
        state.activate_staged(true)?;
        state.start_health_attempt(&first_identity, 1)?;
        state.confirm_health()?;
        let generation = state.activation_generation();

        state.stage(&first)?;
        state.activate_staged(true)?;
        assert_eq!(state.active(), Some(&first_identity));
        assert!(state.active_health_confirmed());
        assert!(state.last_known_good().is_none());
        assert_eq!(state.activation_generation(), generation);
        Ok(())
    }
}
