use application_ports::{CommandExecutionEvidence, Sha256Hex};
use application_ports::bridge_enrollment::{
    BridgeEnrollmentAuthorityError, BridgeEnrollmentAuthorityErrorClass,
    BridgeEnrollmentAuthorityPort, BridgeEnrollmentReservation,
    CompletedBridgeEnrollmentAuthority, IssuedBridgeEnrollmentAuthority,
};
use hmac::{Hmac, KeyInit, Mac};
use profile_platform_primitives::{ActorContext, ActorId, DeviceId, TenantId, UnixMillis};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::d1::D1Database;
use worker::query;
use zeroize::Zeroizing;

const CLAIM_TTL_MS: u64 = 300_000;
const CLAIM_DOMAIN: &str = "part-crm:bridge-device-enrollment:v1";
const MIN_KEY_BYTES: usize = 32;
const MAX_KEY_BYTES: usize = 128;

type HmacSha256 = Hmac<Sha256>;

const LOAD_BY_IDEMPOTENCY: &str = r#"
SELECT tenant_id, actor_id, idempotency_key, payload_fingerprint, claim_digest, device_id,
       issued_at_ms, expires_at_ms, reserved_csr_sha256, reserved_at_ms,
       certificate_sha256, consumed_at_ms
FROM bridge_device_enrollment_claims
WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
"#;

const LOAD_BY_DIGEST: &str = r#"
SELECT tenant_id, actor_id, idempotency_key, payload_fingerprint, claim_digest, device_id,
       issued_at_ms, expires_at_ms, reserved_csr_sha256, reserved_at_ms,
       certificate_sha256, consumed_at_ms
FROM bridge_device_enrollment_claims
WHERE claim_digest = ?
"#;

const INSERT_AUTHORITY: &str = r#"
INSERT INTO bridge_device_enrollment_claims (
    tenant_id, actor_id, idempotency_key, payload_fingerprint, claim_digest, device_id,
    correlation_id, audit_event_id, issued_at_ms, expires_at_ms,
    reserved_csr_sha256, reserved_at_ms, certificate_sha256, consumed_at_ms
)
SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL
WHERE NOT EXISTS (
    SELECT 1 FROM bridge_device_enrollment_claims
    WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
)
RETURNING claim_digest
"#;

const RESERVE_CSR: &str = r#"
UPDATE bridge_device_enrollment_claims
SET reserved_csr_sha256 = ?, reserved_at_ms = ?
WHERE claim_digest = ?
  AND device_id = ?
  AND reserved_csr_sha256 IS NULL
  AND certificate_sha256 IS NULL
  AND expires_at_ms > ?
RETURNING claim_digest
"#;

const FINALIZE_CERTIFICATE: &str = r#"
UPDATE bridge_device_enrollment_claims
SET certificate_sha256 = ?, consumed_at_ms = ?
WHERE claim_digest = ?
  AND device_id = ?
  AND reserved_csr_sha256 = ?
  AND reserved_at_ms IS NOT NULL
  AND certificate_sha256 IS NULL
RETURNING claim_digest
"#;

#[derive(Deserialize)]
struct EnrollmentRow {
    tenant_id: String,
    actor_id: String,
    idempotency_key: String,
    payload_fingerprint: String,
    claim_digest: String,
    device_id: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
    reserved_csr_sha256: Option<String>,
    reserved_at_ms: Option<i64>,
    certificate_sha256: Option<String>,
    consumed_at_ms: Option<i64>,
}

pub struct D1BridgeEnrollmentAuthority {
    database: D1Database,
    derivation_key: Zeroizing<String>,
}

impl D1BridgeEnrollmentAuthority {
    pub fn new(
        database: D1Database,
        derivation_key: String,
    ) -> Result<Self, BridgeEnrollmentAuthorityError> {
        if !(MIN_KEY_BYTES..=MAX_KEY_BYTES).contains(&derivation_key.len())
            || derivation_key
                .bytes()
                .any(|byte| matches!(byte, b'\r' | b'\n' | 0))
        {
            return Err(integrity_failure());
        }
        Ok(Self {
            database,
            derivation_key: Zeroizing::new(derivation_key),
        })
    }

    fn derive_claim_code(
        &self,
        actor: &ActorContext,
        device_id: &DeviceId,
        evidence: &CommandExecutionEvidence,
    ) -> Result<String, BridgeEnrollmentAuthorityError> {
        let canonical = format!(
            "{CLAIM_DOMAIN}\n{}\n{}\n{}\n{}\n{}",
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            device_id.as_str(),
            evidence.idempotency_key().as_str(),
            evidence.payload_fingerprint().as_str(),
        );
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(self.derivation_key.as_bytes())
            .map_err(|_| integrity_failure())?;
        mac.update(canonical.as_bytes());
        Ok(hex_encode(mac.finalize().into_bytes().as_slice()))
    }

    async fn load_by_idempotency(
        &self,
        actor: &ActorContext,
        idempotency_key: &str,
    ) -> Result<Option<EnrollmentRow>, BridgeEnrollmentAuthorityError> {
        query!(
            &self.database,
            LOAD_BY_IDEMPOTENCY,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            idempotency_key,
        )
        .map_err(map_worker_error)?
        .first::<EnrollmentRow>(None)
        .await
        .map_err(map_worker_error)
    }

    async fn load_by_claim(
        &self,
        claim_code: &str,
    ) -> Result<Option<EnrollmentRow>, BridgeEnrollmentAuthorityError> {
        validate_claim_code(claim_code)?;
        let claim_digest = digest_claim_code(claim_code);
        query!(&self.database, LOAD_BY_DIGEST, claim_digest.as_str())
            .map_err(map_worker_error)?
            .first::<EnrollmentRow>(None)
            .await
            .map_err(map_worker_error)
    }
}

impl BridgeEnrollmentAuthorityPort for D1BridgeEnrollmentAuthority {
    async fn issue_bridge_enrollment_authority(
        &self,
        actor: &ActorContext,
        device_id: &DeviceId,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IssuedBridgeEnrollmentAuthority, BridgeEnrollmentAuthorityError> {
        let claim_code = self.derive_claim_code(actor, device_id, evidence)?;
        let claim_digest = digest_claim_code(&claim_code);

        if let Some(row) = self
            .load_by_idempotency(actor, evidence.idempotency_key().as_str())
            .await?
        {
            return replay_issue(row, actor, device_id, evidence, claim_code, &claim_digest);
        }

        let expires_at = evidence
            .now()
            .value()
            .checked_add(CLAIM_TTL_MS)
            .map(UnixMillis::new)
            .ok_or_else(integrity_failure)?;
        let issued_at_ms = unix_to_i64(evidence.now())?;
        let expires_at_ms = unix_to_i64(expires_at)?;
        let returned = query!(
            &self.database,
            INSERT_AUTHORITY,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            evidence.idempotency_key().as_str(),
            evidence.payload_fingerprint().as_str(),
            claim_digest.as_str(),
            device_id.as_str(),
            actor.correlation_id().as_str(),
            evidence.audit_event_id().as_str(),
            issued_at_ms,
            expires_at_ms,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            evidence.idempotency_key().as_str(),
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("claim_digest"))
        .await
        .map_err(map_worker_error)?;

        if returned.is_some() {
            return Ok(IssuedBridgeEnrollmentAuthority::new(
                claim_code, expires_at, false,
            ));
        }

        let row = self
            .load_by_idempotency(actor, evidence.idempotency_key().as_str())
            .await?
            .ok_or_else(integrity_failure)?;
        replay_issue(row, actor, device_id, evidence, claim_code, &claim_digest)
    }

    async fn reserve_bridge_enrollment_csr(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        csr_sha256: &Sha256Hex,
        now: UnixMillis,
    ) -> Result<BridgeEnrollmentReservation, BridgeEnrollmentAuthorityError> {
        let row = self
            .load_by_claim(claim_code)
            .await?
            .ok_or_else(not_found)?;
        classify_claim_identity(&row, device_id)?;

        if row.certificate_sha256.is_some() || row.consumed_at_ms.is_some() {
            return Err(replay_rejected());
        }
        if let Some(existing) = &row.reserved_csr_sha256 {
            if existing != csr_sha256.as_str() {
                return Err(conflict());
            }
            return reservation(&row, csr_sha256.clone(), true);
        }
        if now >= i64_to_unix(row.expires_at_ms)? {
            return Err(replay_rejected());
        }

        let reserved_at_ms = unix_to_i64(now)?;
        let claim_digest = digest_claim_code(claim_code);
        let returned = query!(
            &self.database,
            RESERVE_CSR,
            csr_sha256.as_str(),
            reserved_at_ms,
            claim_digest.as_str(),
            device_id.as_str(),
            reserved_at_ms,
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("claim_digest"))
        .await
        .map_err(map_worker_error)?;

        if returned.is_some() {
            return reservation(&row, csr_sha256.clone(), false);
        }

        let current = self
            .load_by_claim(claim_code)
            .await?
            .ok_or_else(not_found)?;
        classify_claim_identity(&current, device_id)?;
        if current.certificate_sha256.is_some() {
            return Err(replay_rejected());
        }
        match current.reserved_csr_sha256.as_deref() {
            Some(existing) if existing == csr_sha256.as_str() => {
                reservation(&current, csr_sha256.clone(), true)
            }
            Some(_) => Err(conflict()),
            None => Err(replay_rejected()),
        }
    }

    async fn finalize_bridge_enrollment_certificate(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        csr_sha256: &Sha256Hex,
        certificate_sha256: &Sha256Hex,
        now: UnixMillis,
    ) -> Result<CompletedBridgeEnrollmentAuthority, BridgeEnrollmentAuthorityError> {
        let row = self
            .load_by_claim(claim_code)
            .await?
            .ok_or_else(not_found)?;
        classify_claim_identity(&row, device_id)?;
        let existing_csr = row.reserved_csr_sha256.as_deref().ok_or_else(conflict)?;
        if existing_csr != csr_sha256.as_str() {
            return Err(conflict());
        }
        if row.reserved_at_ms.is_none() {
            return Err(integrity_failure());
        }
        if let Some(existing_certificate) = &row.certificate_sha256 {
            if existing_certificate != certificate_sha256.as_str() || row.consumed_at_ms.is_none() {
                return Err(conflict());
            }
            return completion(&row, csr_sha256.clone(), certificate_sha256.clone(), true);
        }
        if row.consumed_at_ms.is_some() {
            return Err(integrity_failure());
        }

        let consumed_at_ms = unix_to_i64(now)?;
        let claim_digest = digest_claim_code(claim_code);
        let returned = query!(
            &self.database,
            FINALIZE_CERTIFICATE,
            certificate_sha256.as_str(),
            consumed_at_ms,
            claim_digest.as_str(),
            device_id.as_str(),
            csr_sha256.as_str(),
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("claim_digest"))
        .await
        .map_err(map_worker_error)?;

        if returned.is_some() {
            return completion(&row, csr_sha256.clone(), certificate_sha256.clone(), false);
        }

        let current = self
            .load_by_claim(claim_code)
            .await?
            .ok_or_else(not_found)?;
        classify_claim_identity(&current, device_id)?;
        if current.reserved_csr_sha256.as_deref() != Some(csr_sha256.as_str()) {
            return Err(conflict());
        }
        match current.certificate_sha256.as_deref() {
            Some(existing) if existing == certificate_sha256.as_str() && current.consumed_at_ms.is_some() => {
                completion(&current, csr_sha256.clone(), certificate_sha256.clone(), true)
            }
            Some(_) => Err(conflict()),
            None => Err(integrity_failure()),
        }
    }
}

fn replay_issue(
    row: EnrollmentRow,
    actor: &ActorContext,
    device_id: &DeviceId,
    evidence: &CommandExecutionEvidence,
    claim_code: String,
    claim_digest: &str,
) -> Result<IssuedBridgeEnrollmentAuthority, BridgeEnrollmentAuthorityError> {
    if row.tenant_id != actor.tenant_scope().tenant_id().as_str()
        || row.actor_id != actor.actor_id().as_str()
        || row.idempotency_key != evidence.idempotency_key().as_str()
    {
        return Err(integrity_failure());
    }
    if row.payload_fingerprint != evidence.payload_fingerprint().as_str()
        || row.device_id != device_id.as_str()
        || row.claim_digest != claim_digest
    {
        return Err(conflict());
    }
    let expires_at = i64_to_unix(row.expires_at_ms)?;
    let _issued_at = i64_to_unix(row.issued_at_ms)?;
    Ok(IssuedBridgeEnrollmentAuthority::new(claim_code, expires_at, true))
}

fn classify_claim_identity(
    row: &EnrollmentRow,
    device_id: &DeviceId,
) -> Result<(), BridgeEnrollmentAuthorityError> {
    if row.device_id != device_id.as_str() {
        return Err(not_found());
    }
    Ok(())
}

fn reservation(
    row: &EnrollmentRow,
    csr_sha256: Sha256Hex,
    replayed: bool,
) -> Result<BridgeEnrollmentReservation, BridgeEnrollmentAuthorityError> {
    Ok(BridgeEnrollmentReservation::new(
        TenantId::parse(row.tenant_id.clone()).map_err(|_| integrity_failure())?,
        ActorId::parse(row.actor_id.clone()).map_err(|_| integrity_failure())?,
        DeviceId::parse(row.device_id.clone()).map_err(|_| integrity_failure())?,
        csr_sha256,
        replayed,
    ))
}

fn completion(
    row: &EnrollmentRow,
    csr_sha256: Sha256Hex,
    certificate_sha256: Sha256Hex,
    replayed: bool,
) -> Result<CompletedBridgeEnrollmentAuthority, BridgeEnrollmentAuthorityError> {
    let reserved = reservation(row, csr_sha256, replayed)?;
    Ok(CompletedBridgeEnrollmentAuthority::new(
        reserved,
        certificate_sha256,
        replayed,
    ))
}

fn validate_claim_code(claim_code: &str) -> Result<(), BridgeEnrollmentAuthorityError> {
    if claim_code.len() != 64
        || !claim_code
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(not_found());
    }
    Ok(())
}

fn digest_claim_code(claim_code: &str) -> String {
    hex_encode(Sha256::digest(claim_code.as_bytes()).as_slice())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn unix_to_i64(value: UnixMillis) -> Result<i64, BridgeEnrollmentAuthorityError> {
    i64::try_from(value.value()).map_err(|_| integrity_failure())
}

fn i64_to_unix(value: i64) -> Result<UnixMillis, BridgeEnrollmentAuthorityError> {
    u64::try_from(value)
        .map(UnixMillis::new)
        .map_err(|_| integrity_failure())
}

fn conflict() -> BridgeEnrollmentAuthorityError {
    BridgeEnrollmentAuthorityError::new(BridgeEnrollmentAuthorityErrorClass::Conflict)
}

fn not_found() -> BridgeEnrollmentAuthorityError {
    BridgeEnrollmentAuthorityError::new(BridgeEnrollmentAuthorityErrorClass::NotFound)
}

fn replay_rejected() -> BridgeEnrollmentAuthorityError {
    BridgeEnrollmentAuthorityError::new(BridgeEnrollmentAuthorityErrorClass::ReplayRejected)
}

fn integrity_failure() -> BridgeEnrollmentAuthorityError {
    BridgeEnrollmentAuthorityError::new(BridgeEnrollmentAuthorityErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> BridgeEnrollmentAuthorityError {
    BridgeEnrollmentAuthorityError::new(BridgeEnrollmentAuthorityErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{CLAIM_DOMAIN, CLAIM_TTL_MS, FINALIZE_CERTIFICATE, INSERT_AUTHORITY, RESERVE_CSR};

    #[test]
    fn claim_storage_never_persists_raw_bearer_or_key_material() {
        assert!(!INSERT_AUTHORITY.contains("claim_code"));
        assert!(INSERT_AUTHORITY.contains("claim_digest"));
        assert!(!INSERT_AUTHORITY.contains("private_key"));
        assert!(!INSERT_AUTHORITY.contains("certificate_pem"));
        assert_eq!(CLAIM_TTL_MS, 300_000);
        assert_eq!(CLAIM_DOMAIN, "part-crm:bridge-device-enrollment:v1");
    }

    #[test]
    fn reservation_is_atomic_device_csr_and_expiry_bound() {
        for required in [
            "device_id = ?",
            "reserved_csr_sha256 IS NULL",
            "certificate_sha256 IS NULL",
            "expires_at_ms > ?",
            "RETURNING claim_digest",
        ] {
            assert!(RESERVE_CSR.contains(required));
        }
    }

    #[test]
    fn finalization_requires_exact_reserved_csr_and_unconsumed_certificate() {
        for required in [
            "device_id = ?",
            "reserved_csr_sha256 = ?",
            "reserved_at_ms IS NOT NULL",
            "certificate_sha256 IS NULL",
            "RETURNING claim_digest",
        ] {
            assert!(FINALIZE_CERTIFICATE.contains(required));
        }
        assert!(!FINALIZE_CERTIFICATE.contains("expires_at_ms >"));
    }
}
