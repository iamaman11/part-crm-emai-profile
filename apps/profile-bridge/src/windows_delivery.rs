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
const RUNTIME_BUNDLE_PREFIX: &str = "runtime-bundle-v1-sha256-";
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
    pub camouhost_ipc_version: u32,
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
    pub fn new(signers: impl IntoIterator<Item = TrustedSigner>) -> Result<Self, DeliveryPolicyError> {
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
        Self {
            sequence: candidate.manifest.sequence,
            release_set_id: candidate.manifest.release_set_id.clone(),
            manifest_sha256: candidate.manifest_sha256.clone(),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryIdentity {
    pub sequence: u64,
    pub release_set_id: String,
    pub manifest_sha256: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeliveryState {
    active: Option<DeliveryIdentity>,
    last_known_good: Option<DeliveryIdentity>,
    staged: Option<DeliveryIdentity>,
}

impl DeliveryState {
    #[must_use]
    pub const fn active(&self) -> Option<&DeliveryIdentity> {
        self.active.as_ref()
    }

    #[must_use]
    pub const fn last_known_good(&self) -> Option<&DeliveryIdentity> {
        self.last_known_good.as_ref()
    }

    #[must_use]
    pub const fn staged(&self) -> Option<&DeliveryIdentity> {
        self.staged.as_ref()
    }

    pub fn stage(&mut self, candidate: &VerifiedDeliveryCandidate) -> Result<(), DeliveryStateError> {
        let identity = candidate.identity();
        if let Some(active) = &self.active {
            if identity.sequence < active.sequence {
                return Err(DeliveryStateError::DowngradeRejected);
            }
            if identity.sequence == active.sequence {
                if identity == *active {
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
        if !quiescent {
            return Err(DeliveryStateError::ActiveRuntime);
        }
        let staged = self.staged.take().ok_or(DeliveryStateError::NoStagedCandidate)?;
        if self.active.as_ref() == Some(&staged) {
            return Ok(());
        }
        self.last_known_good = self.active.take();
        self.active = Some(staged);
        Ok(())
    }

    pub fn confirm_health(&mut self) -> Result<(), DeliveryStateError> {
        if self.active.is_none() {
            return Err(DeliveryStateError::NoActiveCandidate);
        }
        Ok(())
    }

    pub fn fail_health_and_rollback(&mut self) -> Result<(), DeliveryStateError> {
        let Some(lkg) = self.last_known_good.take() else {
            return Err(DeliveryStateError::RecoveryRequired);
        };
        self.active = Some(lkg);
        self.staged = None;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryPolicyError {
    InvalidManifest,
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
    NoStagedCandidate,
    NoActiveCandidate,
    RecoveryRequired,
}

impl fmt::Display for DeliveryStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DowngradeRejected => "Windows delivery downgrade is rejected",
            Self::ReplayConflict => "Windows delivery sequence conflicts with staged identity",
            Self::ActiveRuntime => "Windows delivery activation requires quiescence",
            Self::NoStagedCandidate => "Windows delivery has no staged candidate",
            Self::NoActiveCandidate => "Windows delivery has no active candidate",
            Self::RecoveryRequired => "Windows delivery has no last known good candidate",
        })
    }
}

impl std::error::Error for DeliveryStateError {}

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
    if value.is_empty() || value.len() % 2 != 0 || !is_lower_hex(value, value.len()) {
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
                && expected_certificate_sha256 == CERT_A)
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
        serde_json::to_vec(&DetachedSignatureEnvelope {
            schema_version: SIGNATURE_SCHEMA_VERSION,
            kind: SIGNATURE_KIND.to_owned(),
            key_id: key_id.to_owned(),
            cms_der_hex,
        })
        .expect("serialize signature fixture")
    }

    fn trust(status: TrustedSignerStatus) -> TrustedSignerSet {
        TrustedSignerSet::new([
            TrustedSigner::new("release-2026", CERT_A, status).expect("valid signer"),
            TrustedSigner::new("release-2025", CERT_B, TrustedSignerStatus::AcceptedPrevious)
                .expect("valid previous signer"),
        ])
        .expect("valid trust set")
    }

    fn verify(
        manifest: &WindowsDeliveryManifest,
        trust: &TrustedSignerSet,
        floor: Option<&AcceptedDeliveryFloor>,
    ) -> Result<VerifiedDeliveryCandidate, DeliveryPolicyError> {
        let bytes = serde_json::to_vec(manifest).expect("serialize manifest fixture");
        let signature = signature_for(&bytes, "release-2026");
        verify_delivery_candidate(
            &bytes,
            &signature,
            trust,
            floor,
            &mut DigestBoundVerifier,
        )
    }

    #[test]
    fn exact_signed_candidate_is_admitted_and_same_identity_is_idempotent() {
        let trust = trust(TrustedSignerStatus::Active);
        let first = verify(&manifest(7, 'a'), &trust, None).expect("candidate admitted");
        let floor = AcceptedDeliveryFloor::from_candidate(&first);
        let replay = verify(&manifest(7, 'a'), &trust, Some(&floor)).expect("exact replay admitted");
        assert_eq!(replay.identity(), first.identity());
    }

    #[test]
    fn candidate_tamper_fails_signature_verification() {
        let trust = trust(TrustedSignerStatus::Active);
        let original = serde_json::to_vec(&manifest(7, 'a')).expect("serialize original");
        let signature = signature_for(&original, "release-2026");
        let tampered = serde_json::to_vec(&manifest(8, 'a')).expect("serialize tampered");
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
    }

    #[test]
    fn revoked_unknown_and_ambiguous_trust_fail_closed() {
        assert_eq!(
            verify(&manifest(7, 'a'), &trust(TrustedSignerStatus::Revoked), None),
            Err(DeliveryPolicyError::RevokedSigner)
        );
        let active = TrustedSigner::new("other", CERT_A, TrustedSignerStatus::Active)
            .expect("valid signer");
        let unknown = TrustedSignerSet::new([active]).expect("valid trust set");
        let bytes = serde_json::to_vec(&manifest(7, 'a')).expect("serialize manifest");
        let signature = signature_for(&bytes, "release-2026");
        assert_eq!(
            verify_delivery_candidate(
                &bytes,
                &signature,
                &unknown,
                None,
                &mut DigestBoundVerifier,
            ),
            Err(DeliveryPolicyError::UnknownSigner)
        );
        assert_eq!(
            TrustedSignerSet::new([
                TrustedSigner::new("a", CERT_A, TrustedSignerStatus::Active).expect("valid"),
                TrustedSigner::new("b", CERT_B, TrustedSignerStatus::Active).expect("valid"),
            ]),
            Err(DeliveryPolicyError::InvalidTrustPolicy)
        );
    }

    #[test]
    fn incompatible_manifest_and_malformed_signature_fail_closed() {
        let trust = trust(TrustedSignerStatus::Active);
        let mut incompatible = manifest(7, 'a');
        incompatible.compatibility.runtime_bundle_version = "3.0.0".to_owned();
        assert_eq!(
            verify(&incompatible, &trust, None),
            Err(DeliveryPolicyError::IncompatibleCandidate)
        );

        let bytes = serde_json::to_vec(&manifest(7, 'a')).expect("serialize manifest");
        assert_eq!(
            verify_delivery_candidate(
                &bytes,
                br#"{"schema_version":1,"kind":"WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS","key_id":"release-2026","cms_der_hex":"ZZ"}"#,
                &trust,
                None,
                &mut DigestBoundVerifier,
            ),
            Err(DeliveryPolicyError::InvalidSignatureEnvelope)
        );
    }

    #[test]
    fn downgrade_and_same_sequence_conflict_are_rejected() {
        let trust = trust(TrustedSignerStatus::Active);
        let accepted = verify(&manifest(7, 'a'), &trust, None).expect("candidate admitted");
        let floor = AcceptedDeliveryFloor::from_candidate(&accepted);
        assert_eq!(
            verify(&manifest(6, 'b'), &trust, Some(&floor)),
            Err(DeliveryPolicyError::DowngradeRejected)
        );
        assert_eq!(
            verify(&manifest(7, 'b'), &trust, Some(&floor)),
            Err(DeliveryPolicyError::ReplayConflict)
        );
    }

    #[test]
    fn delivery_state_requires_quiescence_and_rolls_back_only_to_lkg() {
        let trust = trust(TrustedSignerStatus::Active);
        let first = verify(&manifest(1, 'a'), &trust, None).expect("first candidate");
        let second = verify(&manifest(2, 'b'), &trust, None).expect("second candidate");
        let mut state = DeliveryState::default();

        state.stage(&first).expect("stage first");
        assert_eq!(
            state.activate_staged(false),
            Err(DeliveryStateError::ActiveRuntime)
        );
        state.activate_staged(true).expect("activate first");
        assert_eq!(
            state.fail_health_and_rollback(),
            Err(DeliveryStateError::RecoveryRequired)
        );
        state.confirm_health().expect("first healthy");

        state.stage(&second).expect("stage second");
        state.activate_staged(true).expect("activate second");
        assert_eq!(state.last_known_good(), Some(&first.identity()));
        state.fail_health_and_rollback().expect("rollback to first");
        assert_eq!(state.active(), Some(&first.identity()));
        assert!(state.staged().is_none());
    }
}
