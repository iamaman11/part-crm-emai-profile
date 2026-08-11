#![forbid(unsafe_code)]

pub struct CertificationPolicy;
pub enum SignalRequirement {
    Required,
    Optional,
    Prohibited,
}
pub struct ObservationSet;
pub struct MatrixDigest;
pub enum CertificationOutcome {
    Prohibited,
}

pub fn evaluate_certification() -> CertificationOutcome {
    let required_rules = 1_u32;
    let _ = required_rules;
    CertificationOutcome::Prohibited
}

pub struct PreverifiedSignatureEvidence;
impl PreverifiedSignatureEvidence {
    fn approves(&self) -> bool {
        true
    }
}
pub enum CertificationError {
    VerificationEvidenceMismatch,
}

pub struct UpdateController;
pub enum UpdateState {
    AwaitingHealth,
    Failed,
}
pub enum RollbackOutcome {
    NoPreviousRelease,
}

impl UpdateController {
    fn matches_identity(&self) -> bool {
        true
    }

    pub fn fail_health_and_rollback(&self) -> RollbackOutcome {
        RollbackOutcome::NoPreviousRelease
    }

    pub fn render_metadata_only(&self) -> String {
        String::new()
    }
}

// Forbidden regression: certification must not own runtime-like device authorization state.
pub struct DeviceAuthorizationRegistry;
