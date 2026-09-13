use device_domain::DevicePublicKey;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, DeviceId, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::d1::{D1Database, D1Result};
use worker::{Error, Result, query};

const CREATE_PAIRING: &str = r#"
INSERT INTO device_pairing_transactions (
    tenant_id, pairing_digest, device_id, public_key_spki_der_hex,
    issued_at_ms, expires_at_ms, authorized_actor_id, authorized_at_ms, consumed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, NULL)
"#;

const AUTHORIZE_PAIRING: &str = r#"
INSERT INTO device_pairing_authorization_commands (
    tenant_id, pairing_digest, actor_id, authorized_at_ms
) VALUES (?, ?, ?, ?)
"#;

const CREATE_PAIRING_CHALLENGE: &str = r#"
INSERT INTO device_proof_challenges (
    tenant_id, challenge_digest, purpose, actor_id, device_id, pairing_digest,
    device_binding_version, nonce_hex, issued_at_ms, expires_at_ms, consumed_at_ms
) VALUES (?, ?, 'PAIRING', ?, ?, ?, NULL, ?, ?, ?, NULL)
"#;

const CREATE_AUTHORIZED_PAIRING_CHALLENGE: &str = r#"
INSERT INTO device_proof_challenges (
    tenant_id, challenge_digest, purpose, actor_id, device_id, pairing_digest,
    device_binding_version, nonce_hex, issued_at_ms, expires_at_ms, consumed_at_ms
) VALUES (
    ?, ?, 'PAIRING', ?,
    (
        SELECT device_id
        FROM device_pairing_transactions
        WHERE tenant_id = ?
          AND pairing_digest = ?
          AND authorized_actor_id = ?
          AND consumed_at_ms IS NULL
    ),
    ?, NULL, ?, ?, ?, NULL
)
"#;

const CREATE_SESSION_CHALLENGE: &str = r#"
INSERT INTO device_proof_challenges (
    tenant_id, challenge_digest, purpose, actor_id, device_id, pairing_digest,
    device_binding_version, nonce_hex, issued_at_ms, expires_at_ms, consumed_at_ms
) VALUES (?, ?, 'SESSION', ?, ?, NULL, ?, ?, ?, ?, NULL)
"#;

const LOAD_PAIRING_PROOF: &str = r#"
SELECT
    pairing.authorized_actor_id AS actor_id,
    pairing.device_id,
    pairing.public_key_spki_der_hex,
    challenge.nonce_hex,
    challenge.expires_at_ms,
    auth.auth_epoch,
    COALESCE((
        SELECT MAX(binding.version)
        FROM device_actor_bindings AS binding
        WHERE binding.tenant_id = pairing.tenant_id
          AND binding.actor_id = pairing.authorized_actor_id
    ), 0) AS latest_binding_version
FROM device_pairing_transactions AS pairing
JOIN memberships AS membership
  ON membership.tenant_id = pairing.tenant_id
 AND membership.actor_id = pairing.authorized_actor_id
 AND membership.status = 'ACTIVE'
JOIN device_user_authorization_state AS auth
  ON auth.tenant_id = pairing.tenant_id
 AND auth.actor_id = pairing.authorized_actor_id
JOIN device_proof_challenges AS challenge
  ON challenge.tenant_id = pairing.tenant_id
 AND challenge.pairing_digest = pairing.pairing_digest
 AND challenge.purpose = 'PAIRING'
 AND challenge.actor_id = pairing.authorized_actor_id
 AND challenge.device_id = pairing.device_id
WHERE pairing.tenant_id = ?
  AND pairing.pairing_digest = ?
  AND challenge.challenge_digest = ?
  AND pairing.consumed_at_ms IS NULL
  AND pairing.expires_at_ms > ?
  AND challenge.consumed_at_ms IS NULL
  AND challenge.expires_at_ms > ?
LIMIT 2
"#;

const LOAD_ACTIVE_DEVICE: &str = r#"
SELECT
    binding.actor_id,
    binding.device_id,
    binding.version,
    binding.evidence_reference,
    auth.auth_epoch
FROM device_actor_bindings AS binding
JOIN memberships AS membership
  ON membership.tenant_id = binding.tenant_id
 AND membership.actor_id = binding.actor_id
 AND membership.status = 'ACTIVE'
JOIN device_user_authorization_state AS auth
  ON auth.tenant_id = binding.tenant_id
 AND auth.actor_id = binding.actor_id
WHERE binding.tenant_id = ?
  AND binding.device_id = ?
  AND binding.status = 'ACTIVE'
  AND binding.evidence_reference LIKE 'p256_spki_der:%'
ORDER BY binding.version DESC
LIMIT 2
"#;

const LOAD_SESSION_PROOF: &str = r#"
SELECT
    challenge.actor_id,
    challenge.device_id,
    challenge.device_binding_version,
    challenge.nonce_hex,
    challenge.expires_at_ms,
    auth.auth_epoch,
    binding.evidence_reference
FROM device_proof_challenges AS challenge
JOIN memberships AS membership
  ON membership.tenant_id = challenge.tenant_id
 AND membership.actor_id = challenge.actor_id
 AND membership.status = 'ACTIVE'
JOIN device_user_authorization_state AS auth
  ON auth.tenant_id = challenge.tenant_id
 AND auth.actor_id = challenge.actor_id
JOIN device_actor_bindings AS binding
  ON binding.tenant_id = challenge.tenant_id
 AND binding.actor_id = challenge.actor_id
 AND binding.device_id = challenge.device_id
 AND binding.version = challenge.device_binding_version
 AND binding.status = 'ACTIVE'
 AND binding.evidence_reference LIKE 'p256_spki_der:%'
WHERE challenge.tenant_id = ?
  AND challenge.challenge_digest = ?
  AND challenge.purpose = 'SESSION'
  AND challenge.consumed_at_ms IS NULL
  AND challenge.expires_at_ms > ?
LIMIT 2
"#;

const COMPLETE_PAIRING: &str = r#"
INSERT INTO verified_device_pairing_completion_commands (
    tenant_id, pairing_digest, challenge_digest, session_digest,
    expected_previous_version, next_binding_version, user_auth_epoch,
    completed_at_ms, session_expires_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const COMPLETE_SESSION_RENEWAL: &str = r#"
INSERT INTO verified_device_session_renewal_commands (
    tenant_id, challenge_digest, session_digest, user_auth_epoch,
    device_binding_version, completed_at_ms, session_expires_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const RESOLVE_ACTIVE_SESSION: &str = r#"
SELECT
    session.tenant_id,
    session.actor_id,
    session.device_id,
    session.user_auth_epoch,
    session.device_binding_version,
    session.issued_at_ms,
    session.expires_at_ms
FROM device_application_sessions AS session
JOIN memberships AS membership
  ON membership.tenant_id = session.tenant_id
 AND membership.actor_id = session.actor_id
 AND membership.status = 'ACTIVE'
JOIN device_user_authorization_state AS auth
  ON auth.tenant_id = session.tenant_id
 AND auth.actor_id = session.actor_id
 AND auth.auth_epoch = session.user_auth_epoch
JOIN device_actor_bindings AS binding
  ON binding.tenant_id = session.tenant_id
 AND binding.actor_id = session.actor_id
 AND binding.device_id = session.device_id
 AND binding.version = session.device_binding_version
 AND binding.status = 'ACTIVE'
 AND binding.evidence_reference LIKE 'p256_spki_der:%'
WHERE session.tenant_id = ?
  AND session.session_digest = ?
  AND session.revoked_at_ms IS NULL
  AND session.issued_at_ms <= ?
  AND session.expires_at_ms > ?
LIMIT 2
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingProofMaterial {
    actor_id: ActorId,
    device_id: DeviceId,
    public_key: DevicePublicKey,
    nonce: [u8; 32],
    expires_at: UnixMillis,
    auth_epoch: u64,
    latest_binding_version: Option<AggregateVersion>,
}

impl PairingProofMaterial {
    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn public_key(&self) -> &DevicePublicKey {
        &self.public_key
    }

    #[must_use]
    pub const fn nonce(&self) -> &[u8; 32] {
        &self.nonce
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn auth_epoch(&self) -> u64 {
        self.auth_epoch
    }

    #[must_use]
    pub const fn latest_binding_version(&self) -> Option<AggregateVersion> {
        self.latest_binding_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveDeviceAuthorization {
    actor_id: ActorId,
    device_id: DeviceId,
    binding_version: AggregateVersion,
    public_key: DevicePublicKey,
    auth_epoch: u64,
}

impl ActiveDeviceAuthorization {
    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn binding_version(&self) -> AggregateVersion {
        self.binding_version
    }

    #[must_use]
    pub const fn public_key(&self) -> &DevicePublicKey {
        &self.public_key
    }

    #[must_use]
    pub const fn auth_epoch(&self) -> u64 {
        self.auth_epoch
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionProofMaterial {
    actor_id: ActorId,
    device_id: DeviceId,
    binding_version: AggregateVersion,
    public_key: DevicePublicKey,
    nonce: [u8; 32],
    expires_at: UnixMillis,
    auth_epoch: u64,
}

impl SessionProofMaterial {
    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn binding_version(&self) -> AggregateVersion {
        self.binding_version
    }

    #[must_use]
    pub const fn public_key(&self) -> &DevicePublicKey {
        &self.public_key
    }

    #[must_use]
    pub const fn nonce(&self) -> &[u8; 32] {
        &self.nonce
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn auth_epoch(&self) -> u64 {
        self.auth_epoch
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveApplicationSession {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
    user_auth_epoch: u64,
    device_binding_version: AggregateVersion,
    issued_at: UnixMillis,
    expires_at: UnixMillis,
}

impl ActiveApplicationSession {
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
    pub const fn user_auth_epoch(&self) -> u64 {
        self.user_auth_epoch
    }

    #[must_use]
    pub const fn device_binding_version(&self) -> AggregateVersion {
        self.device_binding_version
    }

    #[must_use]
    pub const fn issued_at(&self) -> UnixMillis {
        self.issued_at
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}

pub struct D1DeviceApplicationAuthority {
    database: D1Database,
}

impl D1DeviceApplicationAuthority {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_pairing(
        &self,
        tenant_id: &TenantId,
        pairing_digest: &str,
        device_id: &DeviceId,
        public_key: &DevicePublicKey,
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        require_digest(pairing_digest, "pairing")?;
        let statement = query!(
            &self.database,
            CREATE_PAIRING,
            tenant_id.as_str(),
            pairing_digest,
            device_id.as_str(),
            hex_encode(public_key.spki_der()).as_str(),
            sqlite_integer(issued_at.value())?,
            sqlite_integer(expires_at.value())?,
        )?;
        self.database.batch(vec![statement]).await
    }

    pub async fn authorize_pairing(
        &self,
        actor: &ActorContext,
        pairing_digest: &str,
        authorized_at: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        require_digest(pairing_digest, "pairing")?;
        let statement = query!(
            &self.database,
            AUTHORIZE_PAIRING,
            actor.tenant_scope().tenant_id().as_str(),
            pairing_digest,
            actor.actor_id().as_str(),
            sqlite_integer(authorized_at.value())?,
        )?;
        self.database.batch(vec![statement]).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn authorize_pairing_with_challenge(
        &self,
        actor: &ActorContext,
        pairing_digest: &str,
        challenge_digest: &str,
        nonce: &[u8; 32],
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        require_digest(pairing_digest, "pairing")?;
        require_digest(challenge_digest, "challenge")?;
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let authorized_at = sqlite_integer(issued_at.value())?;
        let authorize = query!(
            &self.database,
            AUTHORIZE_PAIRING,
            tenant_id,
            pairing_digest,
            actor_id,
            authorized_at,
        )?;
        let nonce_hex = hex_encode(nonce);
        let challenge = query!(
            &self.database,
            CREATE_AUTHORIZED_PAIRING_CHALLENGE,
            tenant_id,
            challenge_digest,
            actor_id,
            tenant_id,
            pairing_digest,
            actor_id,
            pairing_digest,
            nonce_hex.as_str(),
            authorized_at,
            sqlite_integer(expires_at.value())?,
        )?;
        self.database.batch(vec![authorize, challenge]).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_pairing_challenge(
        &self,
        actor: &ActorContext,
        pairing_digest: &str,
        challenge_digest: &str,
        device_id: &DeviceId,
        nonce: &[u8; 32],
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        require_digest(pairing_digest, "pairing")?;
        require_digest(challenge_digest, "challenge")?;
        let nonce_hex = hex_encode(nonce);
        let statement = query!(
            &self.database,
            CREATE_PAIRING_CHALLENGE,
            actor.tenant_scope().tenant_id().as_str(),
            challenge_digest,
            actor.actor_id().as_str(),
            device_id.as_str(),
            pairing_digest,
            nonce_hex.as_str(),
            sqlite_integer(issued_at.value())?,
            sqlite_integer(expires_at.value())?,
        )?;
        self.database.batch(vec![statement]).await
    }

    pub async fn resolve_active_device(
        &self,
        tenant_id: &TenantId,
        device_id: &DeviceId,
    ) -> Result<Option<ActiveDeviceAuthorization>> {
        let rows = query!(
            &self.database,
            LOAD_ACTIVE_DEVICE,
            tenant_id.as_str(),
            device_id.as_str(),
        )?
        .all()
        .await?
        .results::<ActiveDeviceRow>()?;
        let Some(row) = unique_row(rows, "active P-256 device authorization")? else {
            return Ok(None);
        };
        Ok(Some(ActiveDeviceAuthorization {
            actor_id: parse_actor(&row.actor_id)?,
            device_id: parse_device(&row.device_id)?,
            binding_version: aggregate_version(row.version, "device binding")?,
            public_key: parse_evidence_public_key(&row.evidence_reference)?,
            auth_epoch: positive_u64(row.auth_epoch, "auth epoch")?,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_session_challenge(
        &self,
        tenant_id: &TenantId,
        challenge_digest: &str,
        device: &ActiveDeviceAuthorization,
        nonce: &[u8; 32],
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        require_digest(challenge_digest, "challenge")?;
        let nonce_hex = hex_encode(nonce);
        let statement = query!(
            &self.database,
            CREATE_SESSION_CHALLENGE,
            tenant_id.as_str(),
            challenge_digest,
            device.actor_id().as_str(),
            device.device_id().as_str(),
            sqlite_version(device.binding_version())?,
            nonce_hex.as_str(),
            sqlite_integer(issued_at.value())?,
            sqlite_integer(expires_at.value())?,
        )?;
        self.database.batch(vec![statement]).await
    }

    pub async fn load_pairing_proof(
        &self,
        tenant_id: &TenantId,
        pairing_digest: &str,
        challenge_digest: &str,
        now: UnixMillis,
    ) -> Result<Option<PairingProofMaterial>> {
        require_digest(pairing_digest, "pairing")?;
        require_digest(challenge_digest, "challenge")?;
        let now = sqlite_integer(now.value())?;
        let rows = query!(
            &self.database,
            LOAD_PAIRING_PROOF,
            tenant_id.as_str(),
            pairing_digest,
            challenge_digest,
            now,
            now,
        )?
        .all()
        .await?
        .results::<PairingProofRow>()?;
        let Some(row) = unique_row(rows, "pairing proof material")? else {
            return Ok(None);
        };
        let latest_binding_version = if row.latest_binding_version == 0 {
            None
        } else {
            Some(aggregate_version(
                row.latest_binding_version,
                "latest device binding",
            )?)
        };
        Ok(Some(PairingProofMaterial {
            actor_id: parse_actor(&row.actor_id)?,
            device_id: parse_device(&row.device_id)?,
            public_key: public_key_from_hex(&row.public_key_spki_der_hex)?,
            nonce: nonce_from_hex(&row.nonce_hex)?,
            expires_at: unix_millis(row.expires_at_ms, "challenge expiry")?,
            auth_epoch: positive_u64(row.auth_epoch, "auth epoch")?,
            latest_binding_version,
        }))
    }

    pub async fn load_session_proof(
        &self,
        tenant_id: &TenantId,
        challenge_digest: &str,
        now: UnixMillis,
    ) -> Result<Option<SessionProofMaterial>> {
        require_digest(challenge_digest, "challenge")?;
        let rows = query!(
            &self.database,
            LOAD_SESSION_PROOF,
            tenant_id.as_str(),
            challenge_digest,
            sqlite_integer(now.value())?,
        )?
        .all()
        .await?
        .results::<SessionProofRow>()?;
        let Some(row) = unique_row(rows, "session proof material")? else {
            return Ok(None);
        };
        Ok(Some(SessionProofMaterial {
            actor_id: parse_actor(&row.actor_id)?,
            device_id: parse_device(&row.device_id)?,
            binding_version: aggregate_version(row.device_binding_version, "device binding")?,
            public_key: parse_evidence_public_key(&row.evidence_reference)?,
            nonce: nonce_from_hex(&row.nonce_hex)?,
            expires_at: unix_millis(row.expires_at_ms, "challenge expiry")?,
            auth_epoch: positive_u64(row.auth_epoch, "auth epoch")?,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn complete_verified_pairing(
        &self,
        tenant_id: &TenantId,
        pairing_digest: &str,
        challenge_digest: &str,
        session_digest: &str,
        expected_previous_version: Option<AggregateVersion>,
        next_binding_version: AggregateVersion,
        auth_epoch: u64,
        completed_at: UnixMillis,
        session_expires_at: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        require_digest(pairing_digest, "pairing")?;
        require_digest(challenge_digest, "challenge")?;
        require_digest(session_digest, "session")?;
        let expected_previous_version =
            expected_previous_version.map(sqlite_version).transpose()?;
        let statement = query!(
            &self.database,
            COMPLETE_PAIRING,
            tenant_id.as_str(),
            pairing_digest,
            challenge_digest,
            session_digest,
            expected_previous_version,
            sqlite_version(next_binding_version)?,
            sqlite_positive(auth_epoch, "auth epoch")?,
            sqlite_integer(completed_at.value())?,
            sqlite_integer(session_expires_at.value())?,
        )?;
        self.database.batch(vec![statement]).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn complete_verified_session_renewal(
        &self,
        tenant_id: &TenantId,
        challenge_digest: &str,
        session_digest: &str,
        auth_epoch: u64,
        binding_version: AggregateVersion,
        completed_at: UnixMillis,
        session_expires_at: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        require_digest(challenge_digest, "challenge")?;
        require_digest(session_digest, "session")?;
        let statement = query!(
            &self.database,
            COMPLETE_SESSION_RENEWAL,
            tenant_id.as_str(),
            challenge_digest,
            session_digest,
            sqlite_positive(auth_epoch, "auth epoch")?,
            sqlite_version(binding_version)?,
            sqlite_integer(completed_at.value())?,
            sqlite_integer(session_expires_at.value())?,
        )?;
        self.database.batch(vec![statement]).await
    }

    pub async fn resolve_active_session(
        &self,
        tenant_id: &TenantId,
        session_digest: &str,
        now: UnixMillis,
    ) -> Result<Option<ActiveApplicationSession>> {
        require_digest(session_digest, "session")?;
        let now = sqlite_integer(now.value())?;
        let rows = query!(
            &self.database,
            RESOLVE_ACTIVE_SESSION,
            tenant_id.as_str(),
            session_digest,
            now,
            now,
        )?
        .all()
        .await?
        .results::<ActiveSessionRow>()?;
        let Some(row) = unique_row(rows, "active application session")? else {
            return Ok(None);
        };
        Ok(Some(ActiveApplicationSession {
            tenant_id: TenantId::parse(row.tenant_id)
                .map_err(|error| Error::RustError(error.to_string()))?,
            actor_id: parse_actor(&row.actor_id)?,
            device_id: parse_device(&row.device_id)?,
            user_auth_epoch: positive_u64(row.user_auth_epoch, "session auth epoch")?,
            device_binding_version: aggregate_version(
                row.device_binding_version,
                "session device binding",
            )?,
            issued_at: unix_millis(row.issued_at_ms, "session issued_at")?,
            expires_at: unix_millis(row.expires_at_ms, "session expires_at")?,
        }))
    }
}

#[derive(Deserialize)]
struct PairingProofRow {
    actor_id: String,
    device_id: String,
    public_key_spki_der_hex: String,
    nonce_hex: String,
    expires_at_ms: i64,
    auth_epoch: i64,
    latest_binding_version: i64,
}

#[derive(Deserialize)]
struct ActiveDeviceRow {
    actor_id: String,
    device_id: String,
    version: i64,
    evidence_reference: String,
    auth_epoch: i64,
}

#[derive(Deserialize)]
struct SessionProofRow {
    actor_id: String,
    device_id: String,
    device_binding_version: i64,
    nonce_hex: String,
    expires_at_ms: i64,
    auth_epoch: i64,
    evidence_reference: String,
}

#[derive(Deserialize)]
struct ActiveSessionRow {
    tenant_id: String,
    actor_id: String,
    device_id: String,
    user_auth_epoch: i64,
    device_binding_version: i64,
    issued_at_ms: i64,
    expires_at_ms: i64,
}

fn unique_row<T>(rows: Vec<T>, label: &str) -> Result<Option<T>> {
    let mut rows = rows.into_iter();
    let first = rows.next();
    if rows.next().is_some() {
        return Err(Error::RustError(format!("duplicate {label}")));
    }
    Ok(first)
}

fn parse_actor(value: &str) -> Result<ActorId> {
    ActorId::parse(value).map_err(|error| Error::RustError(error.to_string()))
}

fn parse_device(value: &str) -> Result<DeviceId> {
    DeviceId::parse(value).map_err(|error| Error::RustError(error.to_string()))
}

fn parse_evidence_public_key(value: &str) -> Result<DevicePublicKey> {
    let encoded = value.strip_prefix("p256_spki_der:").ok_or_else(|| {
        Error::RustError("active device evidence is not a P-256 public key".to_owned())
    })?;
    public_key_from_hex(encoded)
}

fn public_key_from_hex(value: &str) -> Result<DevicePublicKey> {
    let bytes = hex_decode(value)?;
    DevicePublicKey::p256_spki_der(bytes).map_err(|error| Error::RustError(error.to_string()))
}

fn nonce_from_hex(value: &str) -> Result<[u8; 32]> {
    let bytes = hex_decode(value)?;
    bytes
        .try_into()
        .map_err(|_| Error::RustError("device proof nonce length is invalid".to_owned()))
}

fn require_digest(value: &str, label: &str) -> Result<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(Error::RustError(format!("{label} digest is invalid")))
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn hex_decode(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(Error::RustError("hex value length is invalid".to_owned()));
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_nibble(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(Error::RustError(
            "hex value is not lowercase canonical".to_owned(),
        )),
    }
}

fn aggregate_version(value: i64, label: &str) -> Result<AggregateVersion> {
    let value = positive_u64(value, label)?;
    AggregateVersion::new(value).map_err(|error| Error::RustError(error.to_string()))
}

fn positive_u64(value: i64, label: &str) -> Result<u64> {
    let value =
        u64::try_from(value).map_err(|_| Error::RustError(format!("{label} is negative")))?;
    if value == 0 {
        return Err(Error::RustError(format!("{label} is zero")));
    }
    Ok(value)
}

fn unix_millis(value: i64, label: &str) -> Result<UnixMillis> {
    let value =
        u64::try_from(value).map_err(|_| Error::RustError(format!("{label} is negative")))?;
    Ok(UnixMillis::new(value))
}

fn sqlite_version(value: AggregateVersion) -> Result<i64> {
    sqlite_integer(value.value())
}

fn sqlite_positive(value: u64, label: &str) -> Result<i64> {
    if value == 0 {
        return Err(Error::RustError(format!("{label} is zero")));
    }
    sqlite_integer(value)
}

fn sqlite_integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        COMPLETE_PAIRING, COMPLETE_SESSION_RENEWAL, CREATE_AUTHORIZED_PAIRING_CHALLENGE,
        LOAD_ACTIVE_DEVICE, LOAD_PAIRING_PROOF, LOAD_SESSION_PROOF, RESOLVE_ACTIVE_SESSION,
        hex_decode, require_digest,
    };

    #[test]
    fn proof_and_session_reads_recheck_all_live_authorities() {
        for sql in [
            LOAD_PAIRING_PROOF,
            LOAD_SESSION_PROOF,
            RESOLVE_ACTIVE_SESSION,
        ] {
            assert!(sql.contains("memberships"));
            assert!(sql.contains("device_user_authorization_state"));
        }
        assert!(LOAD_SESSION_PROOF.contains("binding.status = 'ACTIVE'"));
        assert!(RESOLVE_ACTIVE_SESSION.contains("binding.status = 'ACTIVE'"));
        assert!(RESOLVE_ACTIVE_SESSION.contains("session.revoked_at_ms IS NULL"));
        assert!(RESOLVE_ACTIVE_SESSION.contains("session.expires_at_ms > ?"));
        assert!(LOAD_ACTIVE_DEVICE.contains("p256_spki_der:%"));
    }

    #[test]
    fn pairing_authorization_challenge_uses_existing_pairing_identity() {
        assert!(CREATE_AUTHORIZED_PAIRING_CHALLENGE.contains("device_pairing_transactions"));
        assert!(CREATE_AUTHORIZED_PAIRING_CHALLENGE.contains("authorized_actor_id = ?"));
        assert!(CREATE_AUTHORIZED_PAIRING_CHALLENGE.contains("consumed_at_ms IS NULL"));
        assert!(!CREATE_AUTHORIZED_PAIRING_CHALLENGE.contains("device_actor_bindings"));
    }

    #[test]
    fn completion_uses_verified_command_surfaces_only() {
        assert!(COMPLETE_PAIRING.contains("verified_device_pairing_completion_commands"));
        assert!(COMPLETE_SESSION_RENEWAL.contains("verified_device_session_renewal_commands"));
        for sql in [COMPLETE_PAIRING, COMPLETE_SESSION_RENEWAL] {
            assert!(!sql.contains("INSERT INTO device_actor_bindings"));
            assert!(!sql.contains("private_key"));
            assert!(!sql.contains("certificate"));
        }
    }

    #[test]
    fn opaque_credentials_are_digest_only_and_lowercase() {
        assert!(require_digest(&"ab".repeat(32), "session").is_ok());
        assert!(require_digest(&"AB".repeat(32), "session").is_err());
        assert!(require_digest(&"a".repeat(63), "session").is_err());
        assert_eq!(hex_decode("00ff10").ok(), Some(vec![0x00, 0xff, 0x10]));
        assert!(hex_decode("00FF10").is_err());
    }
}
