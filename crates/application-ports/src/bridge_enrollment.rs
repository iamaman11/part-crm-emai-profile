use crate::CommandExecutionEvidence;
use core::fmt;
use profile_platform_primitives::{ActorContext, ActorId, DeviceId, TenantId, UnixMillis};

#[derive(Clone, Eq, PartialEq)]
pub struct Sha256Hex(String);

impl Sha256Hex {
    pub fn parse(value: impl Into<String>) -> Result<Self, BridgeEnrollmentAuthorityError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(BridgeEnrollmentAuthorityError::new(
                BridgeEnrollmentAuthorityErrorClass::IntegrityFailure,
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Sha256Hex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeEnrollmentAuthorityErrorClass {
    Conflict,
    NotFound,
    ReplayRejected,
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BridgeEnrollmentAuthorityError {
    class: BridgeEnrollmentAuthorityErrorClass,
}

impl BridgeEnrollmentAuthorityError {
    #[must_use]
    pub const fn new(class: BridgeEnrollmentAuthorityErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> BridgeEnrollmentAuthorityErrorClass {
        self.class
    }
}

impl fmt::Display for BridgeEnrollmentAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            BridgeEnrollmentAuthorityErrorClass::Conflict => "bridge enrollment authority conflict",
            BridgeEnrollmentAuthorityErrorClass::NotFound => "bridge enrollment authority not found",
            BridgeEnrollmentAuthorityErrorClass::ReplayRejected => {
                "bridge enrollment authority replay rejected"
            }
            BridgeEnrollmentAuthorityErrorClass::IntegrityFailure => {
                "bridge enrollment authority integrity failure"
            }
            BridgeEnrollmentAuthorityErrorClass::DependencyUnavailable => {
                "bridge enrollment authority dependency unavailable"
            }
        })
    }
}

impl std::error::Error for BridgeEnrollmentAuthorityError {}

#[derive(Clone, Eq, PartialEq)]
pub struct IssuedBridgeEnrollmentAuthority {
    claim_code: String,
    expires_at: UnixMillis,
    replayed: bool,
}

impl IssuedBridgeEnrollmentAuthority {
    #[must_use]
    pub fn new(claim_code: String, expires_at: UnixMillis, replayed: bool) -> Self {
        Self {
            claim_code,
            expires_at,
            replayed,
        }
    }

    #[must_use]
    pub fn claim_code(&self) -> &str {
        &self.claim_code
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl fmt::Debug for IssuedBridgeEnrollmentAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedBridgeEnrollmentAuthority")
            .field("claim_code", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("replayed", &self.replayed)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeEnrollmentReservation {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
    csr_sha256: Sha256Hex,
    replayed: bool,
}

impl BridgeEnrollmentReservation {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        actor_id: ActorId,
        device_id: DeviceId,
        csr_sha256: Sha256Hex,
        replayed: bool,
    ) -> Self {
        Self {
            tenant_id,
            actor_id,
            device_id,
            csr_sha256,
            replayed,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId { &self.tenant_id }
    #[must_use]
    pub const fn actor_id(&self) -> &ActorId { &self.actor_id }
    #[must_use]
    pub const fn device_id(&self) -> &DeviceId { &self.device_id }
    #[must_use]
    pub const fn csr_sha256(&self) -> &Sha256Hex { &self.csr_sha256 }
    #[must_use]
    pub const fn replayed(&self) -> bool { self.replayed }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedBridgeEnrollmentAuthority {
    reservation: BridgeEnrollmentReservation,
    certificate_sha256: Sha256Hex,
    replayed: bool,
}

impl CompletedBridgeEnrollmentAuthority {
    #[must_use]
    pub const fn new(
        reservation: BridgeEnrollmentReservation,
        certificate_sha256: Sha256Hex,
        replayed: bool,
    ) -> Self {
        Self { reservation, certificate_sha256, replayed }
    }

    #[must_use]
    pub const fn reservation(&self) -> &BridgeEnrollmentReservation { &self.reservation }
    #[must_use]
    pub const fn certificate_sha256(&self) -> &Sha256Hex { &self.certificate_sha256 }
    #[must_use]
    pub const fn replayed(&self) -> bool { self.replayed }
}

#[allow(async_fn_in_trait)]
pub trait BridgeEnrollmentAuthorityPort {
    async fn issue_bridge_enrollment_authority(
        &self,
        actor: &ActorContext,
        device_id: &DeviceId,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IssuedBridgeEnrollmentAuthority, BridgeEnrollmentAuthorityError>;

    /// Atomically reserves this one-shot authority for one exact local-key CSR. Exact replay with
    /// the same CSR is idempotent; a different CSR, device, expired or consumed claim fails closed.
    async fn reserve_bridge_enrollment_csr(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        csr_sha256: &Sha256Hex,
        now: UnixMillis,
    ) -> Result<BridgeEnrollmentReservation, BridgeEnrollmentAuthorityError>;

    /// Finalizes only the exact previously reserved CSR with its public certificate fingerprint.
    /// Finalization is replay-safe for the same certificate identity and never stores private key
    /// or certificate bytes.
    async fn finalize_bridge_enrollment_certificate(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        csr_sha256: &Sha256Hex,
        certificate_sha256: &Sha256Hex,
        now: UnixMillis,
    ) -> Result<CompletedBridgeEnrollmentAuthority, BridgeEnrollmentAuthorityError>;
}

#[cfg(test)]
mod tests {
    use super::{BridgeEnrollmentAuthorityErrorClass, Sha256Hex};

    #[test]
    fn sha256_identity_is_canonical_lowercase_hex() {
        assert!(Sha256Hex::parse("ab".repeat(32)).is_ok());
        let error = Sha256Hex::parse("AB".repeat(32)).expect_err("uppercase must fail");
        assert_eq!(error.class(), BridgeEnrollmentAuthorityErrorClass::IntegrityFailure);
        assert!(Sha256Hex::parse("ab".repeat(31)).is_err());
    }
}
