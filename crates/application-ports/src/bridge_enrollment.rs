use crate::CommandExecutionEvidence;
use core::fmt;
use profile_platform_primitives::{ActorContext, ActorId, DeviceId, TenantId, UnixMillis};

const MAX_BRIDGE_ENROLLMENT_CSR_DER_BYTES: usize = 16 * 1024;

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
            BridgeEnrollmentAuthorityErrorClass::NotFound => {
                "bridge enrollment authority not found"
            }
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
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }
    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }
    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
    #[must_use]
    pub const fn csr_sha256(&self) -> &Sha256Hex {
        &self.csr_sha256
    }
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeEnrollmentCertificateProfile {
    WindowsRsaSha256ClientAuthV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeEnrollmentCertificateSignerErrorClass {
    MalformedCsr,
    UnsupportedKeyProfile,
    ProofOfPossessionRejected,
    CsrIdentityMismatch,
    CertificateIdentityMismatch,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BridgeEnrollmentCertificateSignerError {
    class: BridgeEnrollmentCertificateSignerErrorClass,
}

impl BridgeEnrollmentCertificateSignerError {
    #[must_use]
    pub const fn new(class: BridgeEnrollmentCertificateSignerErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> BridgeEnrollmentCertificateSignerErrorClass {
        self.class
    }
}

impl fmt::Display for BridgeEnrollmentCertificateSignerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            BridgeEnrollmentCertificateSignerErrorClass::MalformedCsr => {
                "bridge enrollment CSR is malformed"
            }
            BridgeEnrollmentCertificateSignerErrorClass::UnsupportedKeyProfile => {
                "bridge enrollment CSR key profile is unsupported"
            }
            BridgeEnrollmentCertificateSignerErrorClass::ProofOfPossessionRejected => {
                "bridge enrollment CSR proof of possession was rejected"
            }
            BridgeEnrollmentCertificateSignerErrorClass::CsrIdentityMismatch => {
                "bridge enrollment CSR identity mismatch"
            }
            BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch => {
                "bridge enrollment certificate identity mismatch"
            }
            BridgeEnrollmentCertificateSignerErrorClass::DependencyUnavailable => {
                "bridge enrollment certificate signer unavailable"
            }
        })
    }
}

impl std::error::Error for BridgeEnrollmentCertificateSignerError {}

/// Transient, exact-identity CSR input accepted by the protected certificate signer boundary.
///
/// Construction proves the input is one bounded DER SEQUENCE envelope and that the machine-owned
/// CSR digest supplied by the caller is exactly the digest already reserved by the one-shot
/// enrollment authority. The signer MUST independently recompute SHA-256 from `csr_der` before
/// signing, so caller digest drift or CSR substitution cannot cross the protected signer boundary.
pub struct BridgeEnrollmentCertificateSignRequest<'a> {
    reservation: &'a BridgeEnrollmentReservation,
    csr_der: &'a [u8],
    csr_sha256: Sha256Hex,
    profile: BridgeEnrollmentCertificateProfile,
}

impl<'a> BridgeEnrollmentCertificateSignRequest<'a> {
    pub fn new(
        reservation: &'a BridgeEnrollmentReservation,
        csr_der: &'a [u8],
        csr_sha256: Sha256Hex,
    ) -> Result<Self, BridgeEnrollmentCertificateSignerError> {
        if !is_exact_der_sequence(csr_der) {
            return Err(BridgeEnrollmentCertificateSignerError::new(
                BridgeEnrollmentCertificateSignerErrorClass::MalformedCsr,
            ));
        }
        if &csr_sha256 != reservation.csr_sha256() {
            return Err(BridgeEnrollmentCertificateSignerError::new(
                BridgeEnrollmentCertificateSignerErrorClass::CsrIdentityMismatch,
            ));
        }
        Ok(Self {
            reservation,
            csr_der,
            csr_sha256,
            profile: BridgeEnrollmentCertificateProfile::WindowsRsaSha256ClientAuthV1,
        })
    }

    #[must_use]
    pub const fn reservation(&self) -> &BridgeEnrollmentReservation {
        self.reservation
    }

    #[must_use]
    pub const fn csr_der(&self) -> &[u8] {
        self.csr_der
    }

    #[must_use]
    pub const fn csr_sha256(&self) -> &Sha256Hex {
        &self.csr_sha256
    }

    #[must_use]
    pub const fn profile(&self) -> BridgeEnrollmentCertificateProfile {
        self.profile
    }
}

impl fmt::Debug for BridgeEnrollmentCertificateSignRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeEnrollmentCertificateSignRequest")
            .field("device_id", self.reservation.device_id())
            .field("csr_sha256", &self.csr_sha256)
            .field("csr_der", &"[REDACTED]")
            .field("profile", &self.profile)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedBridgeEnrollmentCertificate {
    csr_sha256: Sha256Hex,
    certificate_sha256: Sha256Hex,
}

impl SignedBridgeEnrollmentCertificate {
    #[must_use]
    pub fn for_request(
        request: &BridgeEnrollmentCertificateSignRequest<'_>,
        certificate_sha256: Sha256Hex,
    ) -> Self {
        Self {
            csr_sha256: request.csr_sha256().clone(),
            certificate_sha256,
        }
    }

    #[must_use]
    pub const fn csr_sha256(&self) -> &Sha256Hex {
        &self.csr_sha256
    }

    #[must_use]
    pub const fn certificate_sha256(&self) -> &Sha256Hex {
        &self.certificate_sha256
    }

    pub fn validate_for_request(
        &self,
        request: &BridgeEnrollmentCertificateSignRequest<'_>,
    ) -> Result<(), BridgeEnrollmentCertificateSignerError> {
        if self.csr_sha256 != *request.csr_sha256() {
            return Err(BridgeEnrollmentCertificateSignerError::new(
                BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
            ));
        }
        Ok(())
    }
}

#[allow(async_fn_in_trait)]
pub trait BridgeEnrollmentCertificateSignerPort {
    /// Validates and signs exactly the reserved CSR without persisting its bytes.
    ///
    /// Implementations MUST independently SHA-256 the exact `csr_der` bytes and compare that value
    /// with `request.csr_sha256()`, parse exactly one DER PKCS#10 request, verify its signature/proof
    /// of possession, enforce `WindowsRsaSha256ClientAuthV1` (RSA signing key of at least 2048 bits,
    /// SHA-256 signature and ClientAuth-only certificate intent), and issue a certificate whose
    /// public key is the CSR subject public key. Malformed/unsupported/substituted requests fail
    /// closed. Provider credentials and signing keys remain private to the concrete signer adapter.
    async fn sign_bridge_enrollment_certificate(
        &self,
        request: &BridgeEnrollmentCertificateSignRequest<'_>,
    ) -> Result<SignedBridgeEnrollmentCertificate, BridgeEnrollmentCertificateSignerError>;
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
        Self {
            reservation,
            certificate_sha256,
            replayed,
        }
    }

    #[must_use]
    pub const fn reservation(&self) -> &BridgeEnrollmentReservation {
        &self.reservation
    }
    #[must_use]
    pub const fn certificate_sha256(&self) -> &Sha256Hex {
        &self.certificate_sha256
    }
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
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

fn is_exact_der_sequence(value: &[u8]) -> bool {
    if value.len() < 2 || value.len() > MAX_BRIDGE_ENROLLMENT_CSR_DER_BYTES || value[0] != 0x30 {
        return false;
    }
    let first_length = value[1];
    let (header_length, content_length) = if first_length & 0x80 == 0 {
        (2_usize, usize::from(first_length))
    } else {
        let length_octets = usize::from(first_length & 0x7f);
        if length_octets == 0 || length_octets > 4 || value.len() < 2 + length_octets {
            return false;
        }
        if value[2] == 0 {
            return false;
        }
        let mut content_length = 0_usize;
        for byte in &value[2..2 + length_octets] {
            let Some(next) = content_length
                .checked_mul(256)
                .and_then(|length| length.checked_add(usize::from(*byte)))
            else {
                return false;
            };
            content_length = next;
        }
        if content_length < 128 {
            return false;
        }
        (2 + length_octets, content_length)
    };
    header_length
        .checked_add(content_length)
        .is_some_and(|total| total == value.len())
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeEnrollmentAuthorityErrorClass, BridgeEnrollmentCertificateSignRequest,
        BridgeEnrollmentCertificateSignerErrorClass, BridgeEnrollmentReservation, Sha256Hex,
        SignedBridgeEnrollmentCertificate,
    };
    use profile_platform_primitives::{ActorId, DeviceId, TenantId};

    fn reservation_for(csr_sha256: &str) -> Result<BridgeEnrollmentReservation, Box<dyn std::error::Error>> {
        Ok(BridgeEnrollmentReservation::new(
            TenantId::parse("tenant-001")?,
            ActorId::parse("actor-001")?,
            DeviceId::parse("device-001")?,
            Sha256Hex::parse(csr_sha256)?,
            false,
        ))
    }

    #[test]
    fn sha256_identity_is_canonical_lowercase_hex() {
        assert!(Sha256Hex::parse("ab".repeat(32)).is_ok());
        assert!(matches!(
            Sha256Hex::parse("AB".repeat(32)),
            Err(error)
                if error.class() == BridgeEnrollmentAuthorityErrorClass::IntegrityFailure
        ));
        assert!(Sha256Hex::parse("ab".repeat(31)).is_err());
    }

    #[test]
    fn signer_request_requires_exact_reserved_csr_identity() -> Result<(), Box<dyn std::error::Error>> {
        let csr_der = [0x30, 0x03, 0x02, 0x01, 0x00];
        let csr_sha256 = "b560833d6f787af46113b96aad4dd5b5d1ae00dccc69cf30cc92bed651c56617";
        let reservation = reservation_for(csr_sha256)?;
        let request = BridgeEnrollmentCertificateSignRequest::new(
            &reservation,
            &csr_der,
            Sha256Hex::parse(csr_sha256)?,
        )?;
        assert_eq!(request.csr_sha256(), reservation.csr_sha256());
        assert!(!format!("{request:?}").contains("30, 3, 2, 1, 0"));

        let substituted = [0x30, 0x03, 0x02, 0x01, 0x01];
        let substituted_sha256 =
            "1b65f68a522c858715f5dd951cd0402dc16691778814bf0759822b7a257421d0";
        assert!(matches!(
            BridgeEnrollmentCertificateSignRequest::new(
                &reservation,
                &substituted,
                Sha256Hex::parse(substituted_sha256)?,
            ),
            Err(error)
                if error.class()
                    == BridgeEnrollmentCertificateSignerErrorClass::CsrIdentityMismatch
        ));
        Ok(())
    }

    #[test]
    fn signer_request_rejects_malformed_or_noncanonical_der_before_signer_boundary(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let malformed = [0x31, 0x00];
        let malformed_sha256 =
            "e79e418e48623569d75e2a7b09ae88ed9b77b126a445b9ff9dc6989a08efa079";
        let reservation = reservation_for(malformed_sha256)?;
        assert!(matches!(
            BridgeEnrollmentCertificateSignRequest::new(
                &reservation,
                &malformed,
                Sha256Hex::parse(malformed_sha256)?,
            ),
            Err(error)
                if error.class() == BridgeEnrollmentCertificateSignerErrorClass::MalformedCsr
        ));

        let trailing = [0x30, 0x00, 0x00];
        let trailing_sha256 =
            "b0efbbc43054beee753cd10fab49ea0fe2fabdba420e72d0ba74fe2a0222dbf9";
        let reservation = reservation_for(trailing_sha256)?;
        assert!(matches!(
            BridgeEnrollmentCertificateSignRequest::new(
                &reservation,
                &trailing,
                Sha256Hex::parse(trailing_sha256)?,
            ),
            Err(error)
                if error.class() == BridgeEnrollmentCertificateSignerErrorClass::MalformedCsr
        ));

        let noncanonical_long_length = [0x30, 0x81, 0x01, 0x00];
        let noncanonical_sha256 =
            "8bbb0488be844a0217e1a6f93bef03c33d30d120a8b457a5f7e9b299ecff1db2";
        let reservation = reservation_for(noncanonical_sha256)?;
        assert!(matches!(
            BridgeEnrollmentCertificateSignRequest::new(
                &reservation,
                &noncanonical_long_length,
                Sha256Hex::parse(noncanonical_sha256)?,
            ),
            Err(error)
                if error.class() == BridgeEnrollmentCertificateSignerErrorClass::MalformedCsr
        ));
        Ok(())
    }

    #[test]
    fn signed_certificate_identity_cannot_be_reused_for_a_different_reserved_csr(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let first_der = [0x30, 0x03, 0x02, 0x01, 0x00];
        let first_sha256 =
            "b560833d6f787af46113b96aad4dd5b5d1ae00dccc69cf30cc92bed651c56617";
        let first_reservation = reservation_for(first_sha256)?;
        let first_request = BridgeEnrollmentCertificateSignRequest::new(
            &first_reservation,
            &first_der,
            Sha256Hex::parse(first_sha256)?,
        )?;
        let signed = SignedBridgeEnrollmentCertificate::for_request(
            &first_request,
            Sha256Hex::parse("cd".repeat(32))?,
        );
        signed.validate_for_request(&first_request)?;

        let second_der = [0x30, 0x03, 0x02, 0x01, 0x01];
        let second_sha256 =
            "1b65f68a522c858715f5dd951cd0402dc16691778814bf0759822b7a257421d0";
        let second_reservation = reservation_for(second_sha256)?;
        let second_request = BridgeEnrollmentCertificateSignRequest::new(
            &second_reservation,
            &second_der,
            Sha256Hex::parse(second_sha256)?,
        )?;
        assert!(matches!(
            signed.validate_for_request(&second_request),
            Err(error)
                if error.class()
                    == BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch
        ));
        Ok(())
    }
}