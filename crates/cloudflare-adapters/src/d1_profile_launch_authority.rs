use application_ports::CommandExecutionEvidence;
use application_ports::profile_launch::{
    IssuedProfileLaunchAuthority, ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError,
    ProfileLaunchAuthorityErrorClass, ProfileLaunchAuthorityPort,
};
use hmac::{Hmac, KeyInit, Mac};
use profile_platform_primitives::{
    ActorContext, ActorId, DeviceId, GenerationId, ProfileId, TenantId, UnixMillis,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::d1::D1Database;
use worker::query;
use zeroize::Zeroizing;

const CLAIM_TTL_MS: u64 = 300_000;
const CLAIM_DOMAIN: &str = "part-crm:profile-launch-claim:v1";
const MIN_KEY_BYTES: usize = 32;
const MAX_KEY_BYTES: usize = 128;

type HmacSha256 = Hmac<Sha256>;

const LOAD_BY_IDEMPOTENCY: &str = r#"
SELECT
    tenant_id, actor_id, idempotency_key, payload_fingerprint, claim_digest,
    profile_id, generation_id, device_id, issued_at_ms, expires_at_ms, redeemed_at_ms
FROM profile_launch_claims
WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
"#;

const LOAD_BY_DIGEST: &str = r#"
SELECT
    tenant_id, actor_id, idempotency_key, payload_fingerprint, claim_digest,
    profile_id, generation_id, device_id, issued_at_ms, expires_at_ms, redeemed_at_ms
FROM profile_launch_claims
WHERE claim_digest = ?
"#;

const INSERT_AUTHORITY: &str = r#"
INSERT INTO profile_launch_claims (
    tenant_id, actor_id, idempotency_key, payload_fingerprint, claim_digest,
    profile_id, generation_id, device_id, correlation_id, audit_event_id,
    issued_at_ms, expires_at_ms, redeemed_at_ms
)
SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL
WHERE NOT EXISTS (
    SELECT 1 FROM profile_launch_claims
    WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
)
RETURNING claim_digest
"#;

const CONSUME_AUTHORITY: &str = r#"
UPDATE profile_launch_claims
SET redeemed_at_ms = ?
WHERE claim_digest = ?
  AND device_id = ?
  AND redeemed_at_ms IS NULL
  AND expires_at_ms > ?
RETURNING claim_digest
"#;

#[derive(Deserialize)]
struct LaunchAuthorityRow {
    tenant_id: String,
    actor_id: String,
    idempotency_key: String,
    payload_fingerprint: String,
    claim_digest: String,
    profile_id: String,
    generation_id: String,
    device_id: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
    redeemed_at_ms: Option<i64>,
}

pub struct D1ProfileLaunchAuthority {
    database: D1Database,
    derivation_key: Zeroizing<String>,
}

impl D1ProfileLaunchAuthority {
    pub fn new(
        database: D1Database,
        derivation_key: String,
    ) -> Result<Self, ProfileLaunchAuthorityError> {
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
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        device_id: &DeviceId,
        evidence: &CommandExecutionEvidence,
    ) -> Result<String, ProfileLaunchAuthorityError> {
        let canonical = format!(
            "{CLAIM_DOMAIN}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            profile_id.as_str(),
            generation_id.as_str(),
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
    ) -> Result<Option<LaunchAuthorityRow>, ProfileLaunchAuthorityError> {
        query!(
            &self.database,
            LOAD_BY_IDEMPOTENCY,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            idempotency_key,
        )
        .map_err(map_worker_error)?
        .first::<LaunchAuthorityRow>(None)
        .await
        .map_err(map_worker_error)
    }

    async fn load_by_claim(
        &self,
        claim_code: &str,
    ) -> Result<Option<LaunchAuthorityRow>, ProfileLaunchAuthorityError> {
        validate_claim_code(claim_code)?;
        let claim_digest = digest_claim_code(claim_code);
        query!(&self.database, LOAD_BY_DIGEST, claim_digest.as_str())
            .map_err(map_worker_error)?
            .first::<LaunchAuthorityRow>(None)
            .await
            .map_err(map_worker_error)
    }
}

impl ProfileLaunchAuthorityPort for D1ProfileLaunchAuthority {
    async fn issue_profile_launch_authority(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        device_id: &DeviceId,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IssuedProfileLaunchAuthority, ProfileLaunchAuthorityError> {
        let claim_code =
            self.derive_claim_code(actor, profile_id, generation_id, device_id, evidence)?;
        let claim_digest = digest_claim_code(&claim_code);

        if let Some(row) = self
            .load_by_idempotency(actor, evidence.idempotency_key().as_str())
            .await?
        {
            return replay_issue(
                row,
                actor,
                profile_id,
                generation_id,
                device_id,
                evidence,
                claim_code,
                &claim_digest,
            );
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
            profile_id.as_str(),
            generation_id.as_str(),
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
            return Ok(IssuedProfileLaunchAuthority::new(
                claim_code, expires_at, false,
            ));
        }

        let row = self
            .load_by_idempotency(actor, evidence.idempotency_key().as_str())
            .await?
            .ok_or_else(integrity_failure)?;
        replay_issue(
            row,
            actor,
            profile_id,
            generation_id,
            device_id,
            evidence,
            claim_code,
            &claim_digest,
        )
    }

    async fn inspect_profile_launch_authority(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError> {
        let row = self
            .load_by_claim(claim_code)
            .await?
            .ok_or_else(not_found)?;
        classify_redeemable(&row, device_id, now)?;
        binding(&row)
    }

    async fn consume_profile_launch_authority(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError> {
        validate_claim_code(claim_code)?;
        let claim_digest = digest_claim_code(claim_code);
        let row = self
            .load_by_claim(claim_code)
            .await?
            .ok_or_else(not_found)?;
        classify_redeemable(&row, device_id, now)?;

        let consumed_at_ms = unix_to_i64(now)?;
        let returned = query!(
            &self.database,
            CONSUME_AUTHORITY,
            consumed_at_ms,
            claim_digest.as_str(),
            device_id.as_str(),
            consumed_at_ms,
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("claim_digest"))
        .await
        .map_err(map_worker_error)?;

        if returned.is_none() {
            let current = self
                .load_by_claim(claim_code)
                .await?
                .ok_or_else(not_found)?;
            classify_redeemable(&current, device_id, now)?;
            return Err(integrity_failure());
        }
        binding(&row)
    }
}

#[allow(clippy::too_many_arguments)]
fn replay_issue(
    row: LaunchAuthorityRow,
    actor: &ActorContext,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    device_id: &DeviceId,
    evidence: &CommandExecutionEvidence,
    claim_code: String,
    claim_digest: &str,
) -> Result<IssuedProfileLaunchAuthority, ProfileLaunchAuthorityError> {
    if row.tenant_id != actor.tenant_scope().tenant_id().as_str()
        || row.actor_id != actor.actor_id().as_str()
        || row.idempotency_key != evidence.idempotency_key().as_str()
    {
        return Err(integrity_failure());
    }
    if row.payload_fingerprint != evidence.payload_fingerprint().as_str()
        || row.profile_id != profile_id.as_str()
        || row.generation_id != generation_id.as_str()
        || row.device_id != device_id.as_str()
        || row.claim_digest != claim_digest
    {
        return Err(conflict());
    }
    let expires_at = i64_to_unix(row.expires_at_ms)?;
    let _issued_at = i64_to_unix(row.issued_at_ms)?;
    if let Some(redeemed_at) = row.redeemed_at_ms {
        let _redeemed_at = i64_to_unix(redeemed_at)?;
    }
    Ok(IssuedProfileLaunchAuthority::new(
        claim_code, expires_at, true,
    ))
}

fn validate_claim_code(claim_code: &str) -> Result<(), ProfileLaunchAuthorityError> {
    if claim_code.len() != 64
        || !claim_code
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(not_found());
    }
    Ok(())
}

fn classify_redeemable(
    row: &LaunchAuthorityRow,
    device_id: &DeviceId,
    now: UnixMillis,
) -> Result<(), ProfileLaunchAuthorityError> {
    if row.device_id != device_id.as_str() {
        return Err(not_found());
    }
    let expires_at = i64_to_unix(row.expires_at_ms)?;
    if row.redeemed_at_ms.is_some() || now >= expires_at {
        return Err(replay_rejected());
    }
    Ok(())
}

fn binding(
    row: &LaunchAuthorityRow,
) -> Result<ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError> {
    Ok(ProfileLaunchAuthorityBinding::new(
        TenantId::parse(row.tenant_id.clone()).map_err(|_| integrity_failure())?,
        ActorId::parse(row.actor_id.clone()).map_err(|_| integrity_failure())?,
        DeviceId::parse(row.device_id.clone()).map_err(|_| integrity_failure())?,
        ProfileId::parse(row.profile_id.clone()).map_err(|_| integrity_failure())?,
        GenerationId::parse(row.generation_id.clone()).map_err(|_| integrity_failure())?,
    ))
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

fn unix_to_i64(value: UnixMillis) -> Result<i64, ProfileLaunchAuthorityError> {
    i64::try_from(value.value()).map_err(|_| integrity_failure())
}

fn i64_to_unix(value: i64) -> Result<UnixMillis, ProfileLaunchAuthorityError> {
    u64::try_from(value)
        .map(UnixMillis::new)
        .map_err(|_| integrity_failure())
}

fn conflict() -> ProfileLaunchAuthorityError {
    ProfileLaunchAuthorityError::new(ProfileLaunchAuthorityErrorClass::Conflict)
}

fn not_found() -> ProfileLaunchAuthorityError {
    ProfileLaunchAuthorityError::new(ProfileLaunchAuthorityErrorClass::NotFound)
}

fn replay_rejected() -> ProfileLaunchAuthorityError {
    ProfileLaunchAuthorityError::new(ProfileLaunchAuthorityErrorClass::ReplayRejected)
}

fn integrity_failure() -> ProfileLaunchAuthorityError {
    ProfileLaunchAuthorityError::new(ProfileLaunchAuthorityErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> ProfileLaunchAuthorityError {
    ProfileLaunchAuthorityError::new(ProfileLaunchAuthorityErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{CLAIM_DOMAIN, CLAIM_TTL_MS, CONSUME_AUTHORITY, INSERT_AUTHORITY, LOAD_BY_DIGEST};

    #[test]
    fn claim_storage_never_persists_raw_bearer_code() {
        assert!(!INSERT_AUTHORITY.contains("claim_code"));
        assert!(INSERT_AUTHORITY.contains("claim_digest"));
        assert!(INSERT_AUTHORITY.contains("payload_fingerprint"));
        assert_eq!(CLAIM_TTL_MS, 300_000);
        assert_eq!(CLAIM_DOMAIN, "part-crm:profile-launch-claim:v1");
    }

    #[test]
    fn inspection_is_read_only_and_precedes_consumption_by_contract() {
        assert!(LOAD_BY_DIGEST.contains("SELECT"));
        assert!(!LOAD_BY_DIGEST.contains("UPDATE"));
        assert!(!LOAD_BY_DIGEST.contains("redeemed_at_ms ="));
    }

    #[test]
    fn consumption_is_atomic_device_bound_and_expiry_bound() {
        for required in [
            "redeemed_at_ms IS NULL",
            "expires_at_ms > ?",
            "device_id = ?",
            "RETURNING claim_digest",
        ] {
            assert!(CONSUME_AUTHORITY.contains(required));
        }
    }
}
