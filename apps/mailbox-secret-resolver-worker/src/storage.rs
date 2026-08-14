use crate::crypto::{
    AuthenticatedContext, CryptoError, EncryptedValue, NonceSource, ResolverCrypto,
};
use crate::model::MAX_SECRET_DOCUMENT_BYTES;
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordStoreError {
    InvalidInput,
    NotFound,
    Expired,
    Discarded,
    ReplayRejected,
    Crypto,
    StorageUnavailable,
    ConcurrentMutation,
}

pub struct StoredSecret {
    pub document: Zeroizing<Vec<u8>>,
    pub reencrypted: bool,
}

pub struct RecordIdentity<'a> {
    pub tenant_id: &'a str,
    pub raw_handle: &'a str,
    pub provider: &'a str,
    pub record_kind: &'a str,
}

pub struct EncryptedRecordStore<N> {
    database: D1Database,
    crypto: ResolverCrypto<N>,
}

impl<N: NonceSource> EncryptedRecordStore<N> {
    #[must_use]
    pub const fn new(database: D1Database, crypto: ResolverCrypto<N>) -> Self {
        Self { database, crypto }
    }

    pub fn random_handle(&self, prefix: &str) -> Result<String, RecordStoreError> {
        self.crypto
            .random_handle(prefix, 24)
            .map_err(map_crypto_error)
    }

    pub fn deterministic_handle(
        &self,
        tenant_id: &str,
        prefix: &str,
        idempotency_key: &str,
    ) -> Result<String, RecordStoreError> {
        let digest = self
            .crypto
            .lookup_digest(tenant_id, idempotency_key)
            .map_err(map_crypto_error)?;
        Ok(format!("{prefix}{digest}"))
    }

    pub fn lookup_digest(&self, tenant_id: &str, value: &str) -> Result<String, RecordStoreError> {
        self.crypto
            .lookup_digest(tenant_id, value)
            .map_err(map_crypto_error)
    }

    pub async fn store(
        &self,
        identity: &RecordIdentity<'_>,
        document: &[u8],
        now_ms: u64,
        expires_at_ms: Option<u64>,
    ) -> Result<(), RecordStoreError> {
        validate_record_input(identity, document, now_ms, expires_at_ms)?;
        let lookup_digest = self
            .crypto
            .lookup_digest(identity.tenant_id, identity.raw_handle)
            .map_err(map_crypto_error)?;
        let context = AuthenticatedContext {
            tenant_id: identity.tenant_id,
            provider: identity.provider,
            record_kind: identity.record_kind,
            logical_id: &lookup_digest,
        };
        let encrypted = self
            .crypto
            .encrypt(document, &context)
            .map_err(map_crypto_error)?;
        let now = sqlite_millis(now_ms)?;
        let expires_at = expires_at_ms.map(sqlite_millis).transpose()?;
        query!(
            &self.database,
            r#"
            INSERT INTO resolver_encrypted_records (
                tenant_id, lookup_digest, provider, record_kind, logical_id,
                key_version, nonce_hex, ciphertext_hex, created_at_ms,
                updated_at_ms, expires_at_ms, consumed_at_ms, discarded_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)
            ON CONFLICT (tenant_id, lookup_digest, record_kind) DO UPDATE SET
                provider = excluded.provider,
                logical_id = excluded.logical_id,
                key_version = excluded.key_version,
                nonce_hex = excluded.nonce_hex,
                ciphertext_hex = excluded.ciphertext_hex,
                updated_at_ms = excluded.updated_at_ms,
                expires_at_ms = excluded.expires_at_ms,
                consumed_at_ms = NULL,
                discarded_at_ms = NULL
            "#,
            identity.tenant_id,
            lookup_digest.as_str(),
            identity.provider,
            identity.record_kind,
            lookup_digest.as_str(),
            i64::from(encrypted.key_version),
            encrypted.nonce_hex.as_str(),
            encrypted.ciphertext_hex.as_str(),
            now,
            now,
            expires_at
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        Ok(())
    }

    pub async fn load(
        &self,
        identity: &RecordIdentity<'_>,
        now_ms: u64,
    ) -> Result<StoredSecret, RecordStoreError> {
        validate_identity(identity)?;
        let lookup_digest = self
            .crypto
            .lookup_digest(identity.tenant_id, identity.raw_handle)
            .map_err(map_crypto_error)?;
        let row = query!(
            &self.database,
            r#"
            SELECT provider, logical_id, key_version, nonce_hex, ciphertext_hex,
                   expires_at_ms, discarded_at_ms
            FROM resolver_encrypted_records
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ?
            "#,
            identity.tenant_id,
            lookup_digest.as_str(),
            identity.record_kind
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<EncryptedRecordRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::NotFound)?;
        if row.provider != identity.provider || row.logical_id != lookup_digest {
            return Err(RecordStoreError::NotFound);
        }
        if row.discarded_at_ms.is_some() {
            return Err(RecordStoreError::Discarded);
        }
        if row
            .expires_at_ms
            .is_some_and(|expiry| expiry <= sqlite_millis(now_ms).unwrap_or(i64::MAX))
        {
            return Err(RecordStoreError::Expired);
        }
        let key_version = u32::try_from(row.key_version).map_err(|_| RecordStoreError::Crypto)?;
        let encrypted = EncryptedValue {
            key_version,
            nonce_hex: row.nonce_hex,
            ciphertext_hex: row.ciphertext_hex,
        };
        let context = AuthenticatedContext {
            tenant_id: identity.tenant_id,
            provider: identity.provider,
            record_kind: identity.record_kind,
            logical_id: &lookup_digest,
        };
        let document = self
            .crypto
            .decrypt(&encrypted, &context)
            .map_err(map_crypto_error)?;
        let reencrypted = key_version != self.crypto.active_key_version();
        if reencrypted {
            self.reencrypt(
                identity,
                &lookup_digest,
                &context,
                key_version,
                &document,
                now_ms,
            )
            .await?;
        }
        Ok(StoredSecret {
            document,
            reencrypted,
        })
    }

    pub async fn discard(
        &self,
        tenant_id: &str,
        raw_handle: &str,
        record_kind: &str,
        now_ms: u64,
    ) -> Result<(), RecordStoreError> {
        let lookup_digest = self
            .crypto
            .lookup_digest(tenant_id, raw_handle)
            .map_err(map_crypto_error)?;
        let now = sqlite_millis(now_ms)?;
        query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET discarded_at_ms = COALESCE(discarded_at_ms, ?), updated_at_ms = ?
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ?
            "#,
            now,
            now,
            tenant_id,
            lookup_digest,
            record_kind
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        Ok(())
    }

    pub async fn consume(
        &self,
        identity: &RecordIdentity<'_>,
        now_ms: u64,
    ) -> Result<StoredSecret, RecordStoreError> {
        validate_identity(identity)?;
        let lookup_digest = self
            .crypto
            .lookup_digest(identity.tenant_id, identity.raw_handle)
            .map_err(map_crypto_error)?;
        let now = sqlite_millis(now_ms)?;
        let row = query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET consumed_at_ms = ?, updated_at_ms = ?
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ?
              AND provider = ? AND consumed_at_ms IS NULL AND discarded_at_ms IS NULL
              AND (expires_at_ms IS NULL OR expires_at_ms > ?)
            RETURNING provider, logical_id, key_version, nonce_hex, ciphertext_hex,
                      expires_at_ms, discarded_at_ms
            "#,
            now,
            now,
            identity.tenant_id,
            lookup_digest.as_str(),
            identity.record_kind,
            identity.provider,
            now
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<EncryptedRecordRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::ReplayRejected)?;
        if row.provider != identity.provider || row.logical_id != lookup_digest {
            return Err(RecordStoreError::NotFound);
        }
        let key_version = u32::try_from(row.key_version).map_err(|_| RecordStoreError::Crypto)?;
        let context = AuthenticatedContext {
            tenant_id: identity.tenant_id,
            provider: identity.provider,
            record_kind: identity.record_kind,
            logical_id: &lookup_digest,
        };
        let document = self
            .crypto
            .decrypt(
                &EncryptedValue {
                    key_version,
                    nonce_hex: row.nonce_hex,
                    ciphertext_hex: row.ciphertext_hex,
                },
                &context,
            )
            .map_err(map_crypto_error)?;
        Ok(StoredSecret {
            document,
            reencrypted: false,
        })
    }

    async fn reencrypt(
        &self,
        identity: &RecordIdentity<'_>,
        lookup_digest: &str,
        context: &AuthenticatedContext<'_>,
        previous_key_version: u32,
        document: &[u8],
        now_ms: u64,
    ) -> Result<(), RecordStoreError> {
        let encrypted = self
            .crypto
            .encrypt(document, context)
            .map_err(map_crypto_error)?;
        let now = sqlite_millis(now_ms)?;
        let result = query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET key_version = ?, nonce_hex = ?, ciphertext_hex = ?, updated_at_ms = ?
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ?
              AND key_version = ? AND discarded_at_ms IS NULL
            "#,
            i64::from(encrypted.key_version),
            encrypted.nonce_hex,
            encrypted.ciphertext_hex,
            now,
            identity.tenant_id,
            lookup_digest,
            identity.record_kind,
            i64::from(previous_key_version)
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        let changes = result
            .meta()
            .map_err(|_| RecordStoreError::StorageUnavailable)?
            .and_then(|meta| meta.changes)
            .unwrap_or_default();
        if changes != 1 {
            return Err(RecordStoreError::ConcurrentMutation);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct EncryptedRecordRow {
    provider: String,
    logical_id: String,
    key_version: i64,
    nonce_hex: String,
    ciphertext_hex: String,
    expires_at_ms: Option<i64>,
    discarded_at_ms: Option<i64>,
}

fn validate_record_input(
    identity: &RecordIdentity<'_>,
    document: &[u8],
    now_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<(), RecordStoreError> {
    validate_identity(identity)?;
    if document.is_empty()
        || document.len() > MAX_SECRET_DOCUMENT_BYTES
        || now_ms == 0
        || expires_at_ms.is_some_and(|expiry| expiry <= now_ms)
    {
        return Err(RecordStoreError::InvalidInput);
    }
    Ok(())
}

fn validate_identity(identity: &RecordIdentity<'_>) -> Result<(), RecordStoreError> {
    if !bounded_identifier(identity.tenant_id, 128)
        || !bounded_identifier(identity.raw_handle, 192)
        || !bounded_identifier(identity.provider, 64)
        || !bounded_identifier(identity.record_kind, 64)
    {
        return Err(RecordStoreError::InvalidInput);
    }
    Ok(())
}

fn bounded_identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

fn sqlite_millis(value: u64) -> Result<i64, RecordStoreError> {
    i64::try_from(value).map_err(|_| RecordStoreError::InvalidInput)
}

const fn map_crypto_error(_error: CryptoError) -> RecordStoreError {
    RecordStoreError::Crypto
}

#[cfg(test)]
mod tests {
    use super::{RecordIdentity, RecordStoreError, validate_record_input};

    #[test]
    fn record_input_is_bounded_and_expiry_is_forward_only() {
        assert!(validate_record_input(&identity(), b"{}", 100, Some(101)).is_ok());
        assert_eq!(
            validate_record_input(&identity(), b"{}", 100, Some(100)),
            Err(RecordStoreError::InvalidInput)
        );
    }

    const fn identity() -> RecordIdentity<'static> {
        RecordIdentity {
            tenant_id: "tenant_01",
            raw_handle: "secret_01",
            provider: "GMAIL_API",
            record_kind: "credential",
        }
    }
}
