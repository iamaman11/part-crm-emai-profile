use device_domain::DevicePublicKey;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, DeviceId, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::d1::{D1Database, D1Result};
use worker::{Error, Result, query};

const P256_EVIDENCE_PREFIX: &str = "p256_spki_der:";

const BIND_PUBLIC_KEY: &str = r#"
INSERT INTO device_public_key_binding_commands (
    tenant_id, pairing_digest, actor_id, device_id, public_key_spki_der_hex,
    expected_previous_version, next_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#;

const LOAD_ACTIVE_PUBLIC_KEY_BINDING: &str = r#"
SELECT binding.tenant_id, binding.actor_id, binding.device_id, binding.version
FROM device_actor_bindings AS binding
JOIN memberships AS membership
  ON membership.tenant_id = binding.tenant_id
 AND membership.actor_id = binding.actor_id
 AND membership.status = 'ACTIVE'
WHERE binding.evidence_reference = ?
  AND binding.status = 'ACTIVE'
ORDER BY binding.version DESC
LIMIT 2
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicePublicKeyBinding {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
    version: AggregateVersion,
}

impl DevicePublicKeyBinding {
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
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

#[derive(Deserialize)]
struct PublicKeyBindingRow {
    tenant_id: String,
    actor_id: String,
    device_id: String,
    version: i64,
}

pub struct D1DevicePublicKeyBinding {
    database: D1Database,
}

impl D1DevicePublicKeyBinding {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    /// Atomically projects one already-authorized and proof-verified pairing into the canonical
    /// device_actor_bindings owner. This adapter does not authenticate the browser user and does
    /// not verify a signature; those checks must happen before this persistence boundary.
    pub async fn bind_verified_pairing(
        &self,
        actor: &ActorContext,
        pairing_digest: &str,
        device_id: &DeviceId,
        public_key: &DevicePublicKey,
        expected_previous_version: Option<AggregateVersion>,
        next_version: AggregateVersion,
        now: UnixMillis,
    ) -> Result<Vec<D1Result>> {
        if !is_canonical_sha256_hex(pairing_digest) {
            return Err(Error::RustError("pairing digest is invalid".to_owned()));
        }
        let expected_previous_version = expected_previous_version
            .map(sqlite_version)
            .transpose()?;
        let next_version = sqlite_version(next_version)?;
        let now = sqlite_integer(now.value())?;
        let public_key_spki_der_hex = hex_encode(public_key.spki_der());
        if public_key_spki_der_hex.len() != 182 {
            return Err(Error::RustError(
                "P-256 public key transport length is invalid".to_owned(),
            ));
        }
        let statement = query!(
            &self.database,
            BIND_PUBLIC_KEY,
            actor.tenant_scope().tenant_id().as_str(),
            pairing_digest,
            actor.actor_id().as_str(),
            device_id.as_str(),
            public_key_spki_der_hex.as_str(),
            expected_previous_version,
            next_version,
            now,
        )?;
        self.database.batch(vec![statement]).await
    }

    /// Resolves only an ACTIVE exact-key binding whose user membership is still ACTIVE. Duplicate
    /// rows are treated as integrity failure even though the migration also enforces uniqueness.
    pub async fn resolve_active_public_key(
        &self,
        public_key: &DevicePublicKey,
    ) -> Result<Option<DevicePublicKeyBinding>> {
        let evidence_reference = public_key_evidence_reference(public_key);
        let result = query!(
            &self.database,
            LOAD_ACTIVE_PUBLIC_KEY_BINDING,
            evidence_reference.as_str(),
        )?
        .all()
        .await?;
        let rows = result.results::<PublicKeyBindingRow>()?;
        let [row] = rows.as_slice() else {
            return if rows.is_empty() {
                Ok(None)
            } else {
                Err(Error::RustError(
                    "duplicate active P-256 device binding".to_owned(),
                ))
            };
        };
        let version = u64::try_from(row.version)
            .ok()
            .and_then(|value| AggregateVersion::new(value).ok())
            .ok_or_else(|| Error::RustError("device binding version is invalid".to_owned()))?;
        Ok(Some(DevicePublicKeyBinding {
            tenant_id: TenantId::parse(row.tenant_id.clone())
                .map_err(|error| Error::RustError(error.to_string()))?,
            actor_id: ActorId::parse(row.actor_id.clone())
                .map_err(|error| Error::RustError(error.to_string()))?,
            device_id: DeviceId::parse(row.device_id.clone())
                .map_err(|error| Error::RustError(error.to_string()))?,
            version,
        }))
    }
}

fn public_key_evidence_reference(public_key: &DevicePublicKey) -> String {
    format!("{P256_EVIDENCE_PREFIX}{}", hex_encode(public_key.spki_der()))
}

fn is_canonical_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn sqlite_version(value: AggregateVersion) -> Result<i64> {
    sqlite_integer(value.value())
}

fn sqlite_integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        BIND_PUBLIC_KEY, LOAD_ACTIVE_PUBLIC_KEY_BINDING, P256_EVIDENCE_PREFIX,
        is_canonical_sha256_hex, public_key_evidence_reference,
    };
    use device_domain::DevicePublicKey;

    fn public_key() -> Result<DevicePublicKey, Box<dyn std::error::Error>> {
        let mut der = vec![
            0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01,
            0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
        ];
        der.extend([0x11; 64]);
        Ok(DevicePublicKey::p256_spki_der(der)?)
    }

    #[test]
    fn exact_p256_spki_is_the_persisted_device_evidence() -> Result<(), Box<dyn std::error::Error>> {
        let reference = public_key_evidence_reference(&public_key()?);
        assert!(reference.starts_with(P256_EVIDENCE_PREFIX));
        assert_eq!(reference.len(), P256_EVIDENCE_PREFIX.len() + 182);
        assert!(!reference.contains("mtls"));
        assert!(!reference.contains("certificate"));
        Ok(())
    }

    #[test]
    fn pairing_write_targets_only_the_canonical_binding_owner() {
        for required in [
            "device_public_key_binding_commands",
            "pairing_digest",
            "public_key_spki_der_hex",
            "expected_previous_version",
            "next_version",
        ] {
            assert!(BIND_PUBLIC_KEY.contains(required));
        }
        for forbidden in ["private_key", "certificate", "csr", "pfx", "pkcs12"] {
            assert!(!BIND_PUBLIC_KEY.contains(forbidden));
        }
    }

    #[test]
    fn active_key_resolution_rechecks_membership_and_fails_closed_on_duplicates() {
        for required in [
            "device_actor_bindings",
            "membership.status = 'ACTIVE'",
            "binding.status = 'ACTIVE'",
            "binding.evidence_reference = ?",
            "LIMIT 2",
        ] {
            assert!(LOAD_ACTIVE_PUBLIC_KEY_BINDING.contains(required));
        }
    }

    #[test]
    fn pairing_digest_is_exact_lowercase_sha256_hex() {
        assert!(is_canonical_sha256_hex(&"ab".repeat(32)));
        assert!(!is_canonical_sha256_hex(&"AB".repeat(32)));
        assert!(!is_canonical_sha256_hex(&"a".repeat(63)));
        assert!(!is_canonical_sha256_hex(&"g".repeat(64)));
    }
}
