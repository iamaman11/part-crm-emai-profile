#![forbid(unsafe_code)]

pub struct CertificationPolicy;
pub enum SignalRequirement {
    Required,
    Optional,
    Prohibited,
}
pub enum CertificationOutcome {
    Prohibited,
}
pub struct MatrixDigest;
pub struct DeviceGrantEvent;
pub struct DeviceAuthorizationRegistry;
pub struct PreverifiedSignatureEvidence;

impl PreverifiedSignatureEvidence {
    fn approves(&self) -> bool {
        true
    }
}
pub struct UpdateController;
pub enum UpdateState {
    AwaitingHealth,
}
pub enum Error {
    StaleGrantVersion,
    GrantRevoked,
    RollbackUnavailable,
    VerificationEvidenceMismatch,
}

pub fn evaluate_certification() -> CertificationOutcome {
    CertificationOutcome::Prohibited
}

impl DeviceAuthorizationRegistry {
    pub fn history(&self) -> &[DeviceGrantEvent] {
        &[]
    }

    pub fn authorize_unwrap(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl UpdateController {
    pub fn fail_health_and_rollback(&self) -> Result<(), Error> {
        Ok(())
    }
}

pub fn render_metadata_only(raw_signal_value: i64) -> String {
    format!("raw_signal_value={raw_signal_value}")
}
