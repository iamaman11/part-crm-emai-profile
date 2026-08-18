use crate::crypto::{
    AuthenticatedContext, CryptoError, EncryptedValue, LookupDigest, NonceSource, ResolverCrypto,
};
use crate::model::MAX_SECRET_DOCUMENT_BYTES;
use serde::Deserialize;
use worker::d1::{D1Database, D1ResultMeta};
use worker::query;
use zeroize::Zeroizing;

const REFRESH_LEASE_TTL_MS: u64 = 30_000;
const MAX_LOOKUP_HMAC_CANDIDATES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CredentialLifecycleState {
    Active,
    ReauthRequired,
}

impl CredentialLifecycleState {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "ACTIVE" => Some(Self::Active),
            "REAUTH_REQUIRED" => Some(Self::ReauthRequired),
            _ => None,
        }
    }
}

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
    pub(crate) mutation_generation: u64,
    pub(crate) lifecycle: CredentialLifecycleState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdempotencyClaim {
    pub hmac_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandleHmacDependencyStatus {
    pub version: u32,
    pub active: bool,
    pub retained: bool,
    pub live_records: u32,
    pub idempotency_records: u32,
    pub live_refresh_leases: u32,
}

impl HandleHmacDependencyStatus {
    #[must_use]
    pub const fn can_retire(self) -> bool {
        self.retained
            && !self.active
            && self.live_records == 0
            && self.idempotency_records == 0
            && self.live_refresh_leases == 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefreshLease {
    mutation_generation: u64,
    owner_digest: String,
    owner_hmac_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RefreshAcquireError {
    Busy,
    ReauthRequired,
    Store(RecordStoreError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconciliationResult {
    pub active_key_version: u32,
    pub scanned_records: u32,
    pub reencrypted_records: u32,
    pub remaining_records: u32,
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
            .active_lookup_digest(tenant_id, idempotency_key)
            .map_err(map_crypto_error)?;
        Ok(format!("{prefix}{}", digest.digest))
    }

    pub fn deterministic_handle_for_version(
        &self,
        tenant_id: &str,
        prefix: &str,
        idempotency_key: &str,
        hmac_version: u32,
    ) -> Result<String, RecordStoreError> {
        let digest = self
            .crypto
            .lookup_digest_for_version(hmac_version, tenant_id, idempotency_key)
            .map_err(map_crypto_error)?;
        Ok(format!("{prefix}{}", digest.digest))
    }

    pub fn lookup_digest(&self, tenant_id: &str, value: &str) -> Result<String, RecordStoreError> {
        self.crypto
            .active_lookup_digest(tenant_id, value)
            .map(|candidate| candidate.digest)
            .map_err(map_crypto_error)
    }

    pub async fn claim_idempotency(
        &self,
        tenant_id: &str,
        idempotency_key: &str,
        operation: &str,
        request_digest: &str,
        now_ms: u64,
    ) -> Result<IdempotencyClaim, RecordStoreError> {
        if !bounded_identifier(tenant_id, 128)
            || !bounded_identifier(idempotency_key, 192)
            || !bounded_identifier(operation, 64)
            || request_digest.len() != 64
            || !request_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(RecordStoreError::InvalidInput);
        }
        if let Some(existing) = self
            .find_idempotency_claim(tenant_id, idempotency_key, operation)
            .await?
        {
            if existing.request_sha256 != request_digest {
                return Err(RecordStoreError::ReplayRejected);
            }
            return Ok(IdempotencyClaim {
                hmac_version: existing.lookup.version,
            });
        }

        let active = self
            .crypto
            .active_lookup_digest(tenant_id, idempotency_key)
            .map_err(map_crypto_error)?;
        let created_at = sqlite_millis(now_ms)?;
        query!(
            &self.database,
            r#"
            INSERT INTO resolver_idempotency_records (
                tenant_id, idempotency_digest, operation, request_sha256, created_at_ms,
                hmac_version
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (tenant_id, idempotency_digest, operation) DO NOTHING
            "#,
            tenant_id,
            active.digest.as_str(),
            operation,
            request_digest,
            created_at,
            i64::from(active.version)
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?;

        let claimed = self
            .find_idempotency_claim(tenant_id, idempotency_key, operation)
            .await?
            .ok_or(RecordStoreError::StorageUnavailable)?;
        if claimed.request_sha256 != request_digest {
            return Err(RecordStoreError::ReplayRejected);
        }
        Ok(IdempotencyClaim {
            hmac_version: claimed.lookup.version,
        })
    }

    pub async fn store(
        &self,
        identity: &RecordIdentity<'_>,
        document: &[u8],
        now_ms: u64,
        expires_at_ms: Option<u64>,
    ) -> Result<(), RecordStoreError> {
        validate_record_input(identity, document, now_ms, expires_at_ms)?;
        let lookup = match self
            .resolve_record_lookup(
                identity.tenant_id,
                identity.raw_handle,
                identity.record_kind,
            )
            .await?
        {
            Some(existing) => existing,
            None => self
                .crypto
                .active_lookup_digest(identity.tenant_id, identity.raw_handle)
                .map_err(map_crypto_error)?,
        };
        let context = AuthenticatedContext {
            tenant_id: identity.tenant_id,
            provider: identity.provider,
            record_kind: identity.record_kind,
            logical_id: &lookup.digest,
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
                lookup_hmac_version, key_version, nonce_hex, ciphertext_hex, created_at_ms,
                updated_at_ms, expires_at_ms, consumed_at_ms, discarded_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)
            ON CONFLICT (tenant_id, lookup_digest, record_kind) DO UPDATE SET
                provider = excluded.provider,
                logical_id = excluded.logical_id,
                lookup_hmac_version = excluded.lookup_hmac_version,
                key_version = excluded.key_version,
                nonce_hex = excluded.nonce_hex,
                ciphertext_hex = excluded.ciphertext_hex,
                updated_at_ms = excluded.updated_at_ms,
                expires_at_ms = excluded.expires_at_ms,
                consumed_at_ms = NULL,
                discarded_at_ms = NULL,
                mutation_generation = resolver_encrypted_records.mutation_generation + 1,
                credential_state = 'ACTIVE',
                refresh_owner_digest = NULL,
                refresh_owner_hmac_version = NULL,
                refresh_started_at_ms = NULL,
                refresh_expires_at_ms = NULL
            "#,
            identity.tenant_id,
            lookup.digest.as_str(),
            identity.provider,
            identity.record_kind,
            lookup.digest.as_str(),
            i64::from(lookup.version),
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
        let lookup = self
            .resolve_record_lookup(
                identity.tenant_id,
                identity.raw_handle,
                identity.record_kind,
            )
            .await?
            .ok_or(RecordStoreError::NotFound)?;
        let row = query!(
            &self.database,
            r#"
            SELECT provider, logical_id, lookup_hmac_version, key_version, nonce_hex,
                   ciphertext_hex, expires_at_ms, discarded_at_ms, mutation_generation,
                   credential_state
            FROM resolver_encrypted_records
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ?
            "#,
            identity.tenant_id,
            lookup.digest.as_str(),
            identity.record_kind
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<EncryptedRecordRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::NotFound)?;
        let lookup_hmac_version =
            u32::try_from(row.lookup_hmac_version).map_err(|_| RecordStoreError::Crypto)?;
        if row.provider != identity.provider
            || row.logical_id != lookup.digest
            || lookup_hmac_version != lookup.version
        {
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
        let mutation_generation =
            u64::try_from(row.mutation_generation).map_err(|_| RecordStoreError::Crypto)?;
        if mutation_generation == 0 {
            return Err(RecordStoreError::Crypto);
        }
        let lifecycle = CredentialLifecycleState::parse(&row.credential_state)
            .ok_or(RecordStoreError::Crypto)?;
        let encrypted = EncryptedValue {
            key_version,
            nonce_hex: row.nonce_hex,
            ciphertext_hex: row.ciphertext_hex,
        };
        let context = AuthenticatedContext {
            tenant_id: identity.tenant_id,
            provider: identity.provider,
            record_kind: identity.record_kind,
            logical_id: &row.logical_id,
        };
        let document = self
            .crypto
            .decrypt(&encrypted, &context)
            .map_err(map_crypto_error)?;
        let reencrypted = key_version != self.crypto.active_key_version();
        if reencrypted {
            self.reencrypt(
                identity,
                &lookup.digest,
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
            mutation_generation,
            lifecycle,
        })
    }

    pub(crate) async fn acquire_refresh_lease(
        &self,
        identity: &RecordIdentity<'_>,
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<RefreshLease, RefreshAcquireError> {
        validate_identity(identity).map_err(RefreshAcquireError::Store)?;
        if expected_generation == 0 || now_ms == 0 {
            return Err(RefreshAcquireError::Store(RecordStoreError::InvalidInput));
        }
        let lookup = self
            .resolve_record_lookup(
                identity.tenant_id,
                identity.raw_handle,
                identity.record_kind,
            )
            .await
            .map_err(RefreshAcquireError::Store)?
            .ok_or(RefreshAcquireError::Store(RecordStoreError::NotFound))?;
        let owner = Zeroizing::new(
            self.crypto
                .random_handle("refresh_", 24)
                .map_err(map_crypto_error)
                .map_err(RefreshAcquireError::Store)?,
        );
        let owner_lookup = self
            .crypto
            .active_lookup_digest(identity.tenant_id, owner.as_str())
            .map_err(map_crypto_error)
            .map_err(RefreshAcquireError::Store)?;
        let now = sqlite_millis(now_ms).map_err(RefreshAcquireError::Store)?;
        let expires_at_ms = now_ms.saturating_add(REFRESH_LEASE_TTL_MS);
        let expires = sqlite_millis(expires_at_ms).map_err(RefreshAcquireError::Store)?;
        let generation = i64::try_from(expected_generation)
            .map_err(|_| RefreshAcquireError::Store(RecordStoreError::InvalidInput))?;
        let acquired = query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET refresh_owner_digest = ?, refresh_owner_hmac_version = ?,
                refresh_started_at_ms = ?, refresh_expires_at_ms = ?, updated_at_ms = ?
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ? AND provider = ?
              AND mutation_generation = ? AND credential_state = 'ACTIVE'
              AND discarded_at_ms IS NULL
              AND (refresh_owner_digest IS NULL OR refresh_expires_at_ms <= ?)
            RETURNING mutation_generation, refresh_owner_digest, refresh_owner_hmac_version,
                      refresh_expires_at_ms
            "#,
            owner_lookup.digest.as_str(),
            i64::from(owner_lookup.version),
            now,
            expires,
            now,
            identity.tenant_id,
            lookup.digest.as_str(),
            identity.record_kind,
            identity.provider,
            generation,
            now
        )
        .map_err(|_| RefreshAcquireError::Store(RecordStoreError::StorageUnavailable))?
        .first::<RefreshLeaseRow>(None)
        .await
        .map_err(|_| RefreshAcquireError::Store(RecordStoreError::StorageUnavailable))?;
        if let Some(row) = acquired {
            let stored_generation = u64::try_from(row.mutation_generation)
                .map_err(|_| RefreshAcquireError::Store(RecordStoreError::Crypto))?;
            let stored_expiry = u64::try_from(row.refresh_expires_at_ms)
                .map_err(|_| RefreshAcquireError::Store(RecordStoreError::Crypto))?;
            let stored_owner_version = u32::try_from(row.refresh_owner_hmac_version)
                .map_err(|_| RefreshAcquireError::Store(RecordStoreError::Crypto))?;
            if stored_generation != expected_generation
                || row.refresh_owner_digest != owner_lookup.digest
                || stored_owner_version != owner_lookup.version
                || stored_expiry != expires_at_ms
            {
                return Err(RefreshAcquireError::Store(RecordStoreError::Crypto));
            }
            return Ok(RefreshLease {
                mutation_generation: expected_generation,
                owner_digest: owner_lookup.digest,
                owner_hmac_version: owner_lookup.version,
            });
        }

        let row = query!(
            &self.database,
            r#"
            SELECT mutation_generation, credential_state, refresh_owner_digest,
                   refresh_owner_hmac_version, refresh_expires_at_ms
            FROM resolver_encrypted_records
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ? AND provider = ?
              AND discarded_at_ms IS NULL
            "#,
            identity.tenant_id,
            lookup.digest,
            identity.record_kind,
            identity.provider
        )
        .map_err(|_| RefreshAcquireError::Store(RecordStoreError::StorageUnavailable))?
        .first::<RefreshStateRow>(None)
        .await
        .map_err(|_| RefreshAcquireError::Store(RecordStoreError::StorageUnavailable))?
        .ok_or(RefreshAcquireError::Store(RecordStoreError::NotFound))?;
        if row.credential_state == "REAUTH_REQUIRED" {
            return Err(RefreshAcquireError::ReauthRequired);
        }
        if row.credential_state != "ACTIVE" {
            return Err(RefreshAcquireError::Store(RecordStoreError::Crypto));
        }
        let current_generation = u64::try_from(row.mutation_generation)
            .map_err(|_| RefreshAcquireError::Store(RecordStoreError::Crypto))?;
        if current_generation != expected_generation {
            return Err(RefreshAcquireError::Store(
                RecordStoreError::ConcurrentMutation,
            ));
        }
        match (
            row.refresh_owner_digest.as_ref(),
            row.refresh_owner_hmac_version,
        ) {
            (None, None) => {}
            (Some(_), Some(version)) if version > 0 => {}
            _ => return Err(RefreshAcquireError::Store(RecordStoreError::Crypto)),
        }
        if row.refresh_owner_digest.is_some()
            && row.refresh_expires_at_ms.is_some_and(|value| value > now)
        {
            return Err(RefreshAcquireError::Busy);
        }
        Err(RefreshAcquireError::Store(
            RecordStoreError::ConcurrentMutation,
        ))
    }

    pub(crate) async fn commit_refresh(
        &self,
        identity: &RecordIdentity<'_>,
        lease: &RefreshLease,
        document: &[u8],
        now_ms: u64,
    ) -> Result<u64, RecordStoreError> {
        validate_record_input(identity, document, now_ms, None)?;
        let lookup = self
            .resolve_record_lookup(
                identity.tenant_id,
                identity.raw_handle,
                identity.record_kind,
            )
            .await?
            .ok_or(RecordStoreError::NotFound)?;
        let context = AuthenticatedContext {
            tenant_id: identity.tenant_id,
            provider: identity.provider,
            record_kind: identity.record_kind,
            logical_id: &lookup.digest,
        };
        let encrypted = self
            .crypto
            .encrypt(document, &context)
            .map_err(map_crypto_error)?;
        let now = sqlite_millis(now_ms)?;
        let generation =
            i64::try_from(lease.mutation_generation).map_err(|_| RecordStoreError::InvalidInput)?;
        let next_generation = lease
            .mutation_generation
            .checked_add(1)
            .ok_or(RecordStoreError::InvalidInput)?;
        let next_generation_sql =
            i64::try_from(next_generation).map_err(|_| RecordStoreError::InvalidInput)?;
        let result = query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET key_version = ?, nonce_hex = ?, ciphertext_hex = ?, updated_at_ms = ?,
                mutation_generation = ?, refresh_owner_digest = NULL,
                refresh_owner_hmac_version = NULL, refresh_started_at_ms = NULL,
                refresh_expires_at_ms = NULL
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ? AND provider = ?
              AND mutation_generation = ? AND credential_state = 'ACTIVE'
              AND refresh_owner_digest = ? AND refresh_owner_hmac_version = ?
              AND refresh_expires_at_ms > ? AND discarded_at_ms IS NULL
              AND key_version <= ?
            "#,
            i64::from(encrypted.key_version),
            encrypted.nonce_hex.as_str(),
            encrypted.ciphertext_hex.as_str(),
            now,
            next_generation_sql,
            identity.tenant_id,
            lookup.digest.as_str(),
            identity.record_kind,
            identity.provider,
            generation,
            lease.owner_digest.as_str(),
            i64::from(lease.owner_hmac_version),
            now,
            i64::from(encrypted.key_version)
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        require_one_change(
            result
                .meta()
                .map_err(|_| RecordStoreError::StorageUnavailable)?,
        )?;
        Ok(next_generation)
    }

    pub(crate) async fn release_refresh_lease(
        &self,
        identity: &RecordIdentity<'_>,
        lease: &RefreshLease,
        now_ms: u64,
    ) -> Result<(), RecordStoreError> {
        validate_identity(identity)?;
        let lookup = self
            .resolve_record_lookup(
                identity.tenant_id,
                identity.raw_handle,
                identity.record_kind,
            )
            .await?
            .ok_or(RecordStoreError::NotFound)?;
        let now = sqlite_millis(now_ms)?;
        let generation =
            i64::try_from(lease.mutation_generation).map_err(|_| RecordStoreError::InvalidInput)?;
        let result = query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET refresh_owner_digest = NULL, refresh_owner_hmac_version = NULL,
                refresh_started_at_ms = NULL, refresh_expires_at_ms = NULL, updated_at_ms = ?
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ? AND provider = ?
              AND mutation_generation = ? AND refresh_owner_digest = ?
              AND refresh_owner_hmac_version = ?
            "#,
            now,
            identity.tenant_id,
            lookup.digest,
            identity.record_kind,
            identity.provider,
            generation,
            lease.owner_digest.as_str(),
            i64::from(lease.owner_hmac_version)
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        require_one_change(
            result
                .meta()
                .map_err(|_| RecordStoreError::StorageUnavailable)?,
        )
    }

    pub(crate) async fn mark_reauth_required(
        &self,
        identity: &RecordIdentity<'_>,
        lease: &RefreshLease,
        now_ms: u64,
    ) -> Result<u64, RecordStoreError> {
        validate_identity(identity)?;
        let lookup = self
            .resolve_record_lookup(
                identity.tenant_id,
                identity.raw_handle,
                identity.record_kind,
            )
            .await?
            .ok_or(RecordStoreError::NotFound)?;
        let now = sqlite_millis(now_ms)?;
        let generation =
            i64::try_from(lease.mutation_generation).map_err(|_| RecordStoreError::InvalidInput)?;
        let next_generation = lease
            .mutation_generation
            .checked_add(1)
            .ok_or(RecordStoreError::InvalidInput)?;
        let next_generation_sql =
            i64::try_from(next_generation).map_err(|_| RecordStoreError::InvalidInput)?;
        let result = query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET credential_state = 'REAUTH_REQUIRED', mutation_generation = ?,
                refresh_owner_digest = NULL, refresh_owner_hmac_version = NULL,
                refresh_started_at_ms = NULL, refresh_expires_at_ms = NULL, updated_at_ms = ?
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ? AND provider = ?
              AND mutation_generation = ? AND credential_state = 'ACTIVE'
              AND refresh_owner_digest = ? AND refresh_owner_hmac_version = ?
              AND refresh_expires_at_ms > ? AND discarded_at_ms IS NULL
            "#,
            next_generation_sql,
            now,
            identity.tenant_id,
            lookup.digest,
            identity.record_kind,
            identity.provider,
            generation,
            lease.owner_digest.as_str(),
            i64::from(lease.owner_hmac_version),
            now
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        require_one_change(
            result
                .meta()
                .map_err(|_| RecordStoreError::StorageUnavailable)?,
        )?;
        Ok(next_generation)
    }

    pub async fn discard(
        &self,
        tenant_id: &str,
        raw_handle: &str,
        record_kind: &str,
        now_ms: u64,
    ) -> Result<(), RecordStoreError> {
        let Some(lookup) = self
            .resolve_record_lookup(tenant_id, raw_handle, record_kind)
            .await?
        else {
            return Ok(());
        };
        let now = sqlite_millis(now_ms)?;
        query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET discarded_at_ms = COALESCE(discarded_at_ms, ?), updated_at_ms = ?,
                refresh_owner_digest = NULL, refresh_owner_hmac_version = NULL,
                refresh_started_at_ms = NULL, refresh_expires_at_ms = NULL
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ?
            "#,
            now,
            now,
            tenant_id,
            lookup.digest,
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
        let lookup = self
            .resolve_record_lookup(
                identity.tenant_id,
                identity.raw_handle,
                identity.record_kind,
            )
            .await?
            .ok_or(RecordStoreError::ReplayRejected)?;
        let now = sqlite_millis(now_ms)?;
        let row = query!(
            &self.database,
            r#"
            UPDATE resolver_encrypted_records
            SET consumed_at_ms = ?, updated_at_ms = ?
            WHERE tenant_id = ? AND lookup_digest = ? AND record_kind = ?
              AND provider = ? AND consumed_at_ms IS NULL AND discarded_at_ms IS NULL
              AND (expires_at_ms IS NULL OR expires_at_ms > ?)
            RETURNING provider, logical_id, lookup_hmac_version, key_version, nonce_hex,
                      ciphertext_hex, expires_at_ms, discarded_at_ms, mutation_generation,
                      credential_state
            "#,
            now,
            now,
            identity.tenant_id,
            lookup.digest.as_str(),
            identity.record_kind,
            identity.provider,
            now
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<EncryptedRecordRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::ReplayRejected)?;
        let lookup_hmac_version =
            u32::try_from(row.lookup_hmac_version).map_err(|_| RecordStoreError::Crypto)?;
        if row.provider != identity.provider
            || row.logical_id != lookup.digest
            || lookup_hmac_version != lookup.version
        {
            return Err(RecordStoreError::NotFound);
        }
        let key_version = u32::try_from(row.key_version).map_err(|_| RecordStoreError::Crypto)?;
        let mutation_generation =
            u64::try_from(row.mutation_generation).map_err(|_| RecordStoreError::Crypto)?;
        if mutation_generation == 0 {
            return Err(RecordStoreError::Crypto);
        }
        let lifecycle = CredentialLifecycleState::parse(&row.credential_state)
            .ok_or(RecordStoreError::Crypto)?;
        let context = AuthenticatedContext {
            tenant_id: identity.tenant_id,
            provider: identity.provider,
            record_kind: identity.record_kind,
            logical_id: &row.logical_id,
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
            mutation_generation,
            lifecycle,
        })
    }

    pub async fn lookup_hmac_dependency_status(
        &self,
        version: u32,
        now_ms: u64,
    ) -> Result<HandleHmacDependencyStatus, RecordStoreError> {
        if version == 0 || now_ms == 0 {
            return Err(RecordStoreError::InvalidInput);
        }
        let version_sql = i64::from(version);
        let now = sqlite_millis(now_ms)?;
        let live_records = query!(
            &self.database,
            r#"
            SELECT COUNT(*) AS count
            FROM resolver_encrypted_records
            WHERE lookup_hmac_version = ?
              AND discarded_at_ms IS NULL
              AND consumed_at_ms IS NULL
              AND (expires_at_ms IS NULL OR expires_at_ms > ?)
            "#,
            version_sql,
            now
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<CountRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::StorageUnavailable)?;
        let idempotency_records = query!(
            &self.database,
            r#"
            SELECT COUNT(*) AS count
            FROM resolver_idempotency_records
            WHERE hmac_version = ?
            "#,
            version_sql
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<CountRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::StorageUnavailable)?;
        let live_refresh_leases = query!(
            &self.database,
            r#"
            SELECT COUNT(*) AS count
            FROM resolver_encrypted_records
            WHERE refresh_owner_hmac_version = ?
              AND refresh_owner_digest IS NOT NULL
              AND refresh_expires_at_ms > ?
              AND discarded_at_ms IS NULL
            "#,
            version_sql,
            now
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<CountRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::StorageUnavailable)?;
        let retained_versions = self.crypto.retained_lookup_hmac_versions();
        Ok(HandleHmacDependencyStatus {
            version,
            active: self.crypto.active_lookup_hmac_version() == version,
            retained: retained_versions.contains(&version),
            live_records: count_to_u32(live_records.count)?,
            idempotency_records: count_to_u32(idempotency_records.count)?,
            live_refresh_leases: count_to_u32(live_refresh_leases.count)?,
        })
    }

    pub async fn reconcile_key_rotation(
        &self,
        now_ms: u64,
        limit: u32,
    ) -> Result<ReconciliationResult, RecordStoreError> {
        if now_ms == 0 || !(1..=100).contains(&limit) {
            return Err(RecordStoreError::InvalidInput);
        }
        let active_key_version = self.crypto.active_key_version();
        let active_sql = i64::from(active_key_version);
        let limit_sql = i64::from(limit);
        let rows = query!(
            &self.database,
            r#"
            SELECT tenant_id, lookup_digest, provider, record_kind, logical_id,
                   key_version, nonce_hex, ciphertext_hex
            FROM resolver_encrypted_records
            WHERE key_version <> ? AND discarded_at_ms IS NULL
            ORDER BY key_version, tenant_id, lookup_digest, record_kind
            LIMIT ?
            "#,
            active_sql,
            limit_sql
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .all()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .results::<ReconciliationRow>()
        .map_err(|_| RecordStoreError::StorageUnavailable)?;

        let scanned_records =
            u32::try_from(rows.len()).map_err(|_| RecordStoreError::StorageUnavailable)?;
        let mut reencrypted_records = 0_u32;
        let mut observed_versions: Vec<(u32, u32, u32)> = Vec::new();
        for row in rows {
            if row.logical_id != row.lookup_digest {
                return Err(RecordStoreError::Crypto);
            }
            let previous_key_version =
                u32::try_from(row.key_version).map_err(|_| RecordStoreError::Crypto)?;
            let context = AuthenticatedContext {
                tenant_id: &row.tenant_id,
                provider: &row.provider,
                record_kind: &row.record_kind,
                logical_id: &row.logical_id,
            };
            let document = self
                .crypto
                .decrypt(
                    &EncryptedValue {
                        key_version: previous_key_version,
                        nonce_hex: row.nonce_hex,
                        ciphertext_hex: row.ciphertext_hex,
                    },
                    &context,
                )
                .map_err(map_crypto_error)?;
            let encrypted = self
                .crypto
                .encrypt(&document, &context)
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
                row.tenant_id,
                row.lookup_digest,
                row.record_kind,
                row.key_version
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
            let reencrypted = u32::from(changes == 1);
            reencrypted_records = reencrypted_records.saturating_add(reencrypted);
            if let Some(entry) = observed_versions
                .iter_mut()
                .find(|entry| entry.0 == previous_key_version)
            {
                entry.1 = entry.1.saturating_add(1);
                entry.2 = entry.2.saturating_add(reencrypted);
            } else {
                observed_versions.push((previous_key_version, 1, reencrypted));
            }
        }

        let remaining = query!(
            &self.database,
            r#"
            SELECT COUNT(*) AS count
            FROM resolver_encrypted_records
            WHERE key_version <> ? AND discarded_at_ms IS NULL
            "#,
            active_sql
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .first::<CountRow>(None)
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .ok_or(RecordStoreError::StorageUnavailable)?;
        let remaining_records = count_to_u32(remaining.count)?;

        for (from_version, scanned, reencrypted) in observed_versions {
            let rotation_id = format!("from-v{from_version}-to-v{active_key_version}");
            let from_sql = i64::from(from_version);
            let scanned_sql = i64::from(scanned);
            let reencrypted_sql = i64::from(reencrypted);
            let now = sqlite_millis(now_ms)?;
            query!(
                &self.database,
                r#"
                INSERT INTO resolver_key_rotation_runs (
                    rotation_id, from_key_version, to_key_version, status,
                    scanned_records, reencrypted_records, started_at_ms, verified_at_ms
                ) VALUES (?, ?, ?, 'RUNNING', ?, ?, ?, NULL)
                ON CONFLICT (rotation_id) DO UPDATE SET
                    scanned_records = resolver_key_rotation_runs.scanned_records
                        + excluded.scanned_records,
                    reencrypted_records = resolver_key_rotation_runs.reencrypted_records
                        + excluded.reencrypted_records
                "#,
                rotation_id,
                from_sql,
                active_sql,
                scanned_sql,
                reencrypted_sql,
                now
            )
            .map_err(|_| RecordStoreError::StorageUnavailable)?
            .run()
            .await
            .map_err(|_| RecordStoreError::StorageUnavailable)?;

            let old_remaining = query!(
                &self.database,
                r#"
                SELECT COUNT(*) AS count
                FROM resolver_encrypted_records
                WHERE key_version = ? AND discarded_at_ms IS NULL
                "#,
                from_sql
            )
            .map_err(|_| RecordStoreError::StorageUnavailable)?
            .first::<CountRow>(None)
            .await
            .map_err(|_| RecordStoreError::StorageUnavailable)?
            .ok_or(RecordStoreError::StorageUnavailable)?;
            if old_remaining.count == 0 {
                query!(
                    &self.database,
                    r#"
                    UPDATE resolver_key_rotation_runs
                    SET status = 'VERIFIED', verified_at_ms = ?
                    WHERE rotation_id = ? AND status = 'RUNNING'
                    "#,
                    now,
                    rotation_id
                )
                .map_err(|_| RecordStoreError::StorageUnavailable)?
                .run()
                .await
                .map_err(|_| RecordStoreError::StorageUnavailable)?;
            }
        }

        Ok(ReconciliationResult {
            active_key_version,
            scanned_records,
            reencrypted_records,
            remaining_records,
        })
    }

    async fn find_idempotency_claim(
        &self,
        tenant_id: &str,
        idempotency_key: &str,
        operation: &str,
    ) -> Result<Option<ResolvedIdempotencyClaim>, RecordStoreError> {
        let candidates = self
            .crypto
            .lookup_candidates(tenant_id, idempotency_key)
            .map_err(map_crypto_error)?;
        let [digest_0, digest_1, digest_2, digest_3] = candidate_digest_slots(&candidates)?;
        let rows = query!(
            &self.database,
            r#"
            SELECT idempotency_digest, request_sha256, hmac_version
            FROM resolver_idempotency_records
            WHERE tenant_id = ? AND operation = ?
              AND idempotency_digest IN (?, ?, ?, ?)
            "#,
            tenant_id,
            operation,
            digest_0,
            digest_1,
            digest_2,
            digest_3
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .all()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .results::<IdempotencyRow>()
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        resolve_idempotency_rows(&candidates, rows)
    }

    async fn resolve_record_lookup(
        &self,
        tenant_id: &str,
        raw_handle: &str,
        record_kind: &str,
    ) -> Result<Option<LookupDigest>, RecordStoreError> {
        if !bounded_identifier(tenant_id, 128)
            || !bounded_identifier(raw_handle, 192)
            || !bounded_identifier(record_kind, 64)
        {
            return Err(RecordStoreError::InvalidInput);
        }
        let candidates = self
            .crypto
            .lookup_candidates(tenant_id, raw_handle)
            .map_err(map_crypto_error)?;
        let [digest_0, digest_1, digest_2, digest_3] = candidate_digest_slots(&candidates)?;
        let rows = query!(
            &self.database,
            r#"
            SELECT lookup_digest, lookup_hmac_version, logical_id
            FROM resolver_encrypted_records
            WHERE tenant_id = ? AND record_kind = ?
              AND lookup_digest IN (?, ?, ?, ?)
            "#,
            tenant_id,
            record_kind,
            digest_0,
            digest_1,
            digest_2,
            digest_3
        )
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .all()
        .await
        .map_err(|_| RecordStoreError::StorageUnavailable)?
        .results::<LookupMetadataRow>()
        .map_err(|_| RecordStoreError::StorageUnavailable)?;
        resolve_lookup_rows(&candidates, rows)
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

struct ResolvedIdempotencyClaim {
    lookup: LookupDigest,
    request_sha256: String,
}

#[derive(Deserialize)]
struct LookupMetadataRow {
    lookup_digest: String,
    lookup_hmac_version: i64,
    logical_id: String,
}

#[derive(Deserialize)]
struct EncryptedRecordRow {
    provider: String,
    logical_id: String,
    lookup_hmac_version: i64,
    key_version: i64,
    nonce_hex: String,
    ciphertext_hex: String,
    expires_at_ms: Option<i64>,
    discarded_at_ms: Option<i64>,
    mutation_generation: i64,
    credential_state: String,
}

#[derive(Deserialize)]
struct RefreshLeaseRow {
    mutation_generation: i64,
    refresh_owner_digest: String,
    refresh_owner_hmac_version: i64,
    refresh_expires_at_ms: i64,
}

#[derive(Deserialize)]
struct RefreshStateRow {
    mutation_generation: i64,
    credential_state: String,
    refresh_owner_digest: Option<String>,
    refresh_owner_hmac_version: Option<i64>,
    refresh_expires_at_ms: Option<i64>,
}

#[derive(Deserialize)]
struct ReconciliationRow {
    tenant_id: String,
    lookup_digest: String,
    provider: String,
    record_kind: String,
    logical_id: String,
    key_version: i64,
    nonce_hex: String,
    ciphertext_hex: String,
}

#[derive(Deserialize)]
struct CountRow {
    count: i64,
}

#[derive(Deserialize)]
struct IdempotencyRow {
    idempotency_digest: String,
    request_sha256: String,
    hmac_version: i64,
}

fn candidate_digest_slots(
    candidates: &[LookupDigest],
) -> Result<[&str; MAX_LOOKUP_HMAC_CANDIDATES], RecordStoreError> {
    if candidates.is_empty() || candidates.len() > MAX_LOOKUP_HMAC_CANDIDATES {
        return Err(RecordStoreError::Crypto);
    }
    let mut slots = [""; MAX_LOOKUP_HMAC_CANDIDATES];
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.version == 0
            || candidate.digest.len() != 64
            || !candidate
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || candidates[..index].iter().any(|existing| {
                existing.version == candidate.version || existing.digest == candidate.digest
            })
        {
            return Err(RecordStoreError::Crypto);
        }
        slots[index] = candidate.digest.as_str();
    }
    Ok(slots)
}

fn resolve_idempotency_rows(
    candidates: &[LookupDigest],
    rows: Vec<IdempotencyRow>,
) -> Result<Option<ResolvedIdempotencyClaim>, RecordStoreError> {
    let mut resolved = None;
    for row in rows {
        let stored_version =
            u32::try_from(row.hmac_version).map_err(|_| RecordStoreError::Crypto)?;
        let candidate = candidates
            .iter()
            .find(|candidate| {
                candidate.version == stored_version && candidate.digest == row.idempotency_digest
            })
            .ok_or(RecordStoreError::Crypto)?;
        if resolved.is_some() {
            return Err(RecordStoreError::Crypto);
        }
        resolved = Some(ResolvedIdempotencyClaim {
            lookup: candidate.clone(),
            request_sha256: row.request_sha256,
        });
    }
    Ok(resolved)
}

fn resolve_lookup_rows(
    candidates: &[LookupDigest],
    rows: Vec<LookupMetadataRow>,
) -> Result<Option<LookupDigest>, RecordStoreError> {
    let mut resolved = None;
    for row in rows {
        let stored_version =
            u32::try_from(row.lookup_hmac_version).map_err(|_| RecordStoreError::Crypto)?;
        let candidate = candidates
            .iter()
            .find(|candidate| {
                candidate.version == stored_version && candidate.digest == row.lookup_digest
            })
            .ok_or(RecordStoreError::Crypto)?;
        if row.logical_id != candidate.digest || resolved.is_some() {
            return Err(RecordStoreError::Crypto);
        }
        resolved = Some(candidate.clone());
    }
    Ok(resolved)
}

fn require_one_change(meta: Option<D1ResultMeta>) -> Result<(), RecordStoreError> {
    let changes = meta.and_then(|value| value.changes).unwrap_or_default();
    if changes == 1 {
        Ok(())
    } else {
        Err(RecordStoreError::ConcurrentMutation)
    }
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

fn count_to_u32(value: i64) -> Result<u32, RecordStoreError> {
    u32::try_from(value).map_err(|_| RecordStoreError::StorageUnavailable)
}

const fn map_crypto_error(_error: CryptoError) -> RecordStoreError {
    RecordStoreError::Crypto
}

#[cfg(test)]
mod tests {
    use super::{
        HandleHmacDependencyStatus, IdempotencyRow, LookupMetadataRow, RecordIdentity,
        RecordStoreError, candidate_digest_slots, resolve_idempotency_rows, resolve_lookup_rows,
        validate_record_input,
    };
    use crate::crypto::LookupDigest;

    #[test]
    fn record_input_is_bounded_and_expiry_is_forward_only() {
        assert!(validate_record_input(&identity(), b"{}", 100, Some(101)).is_ok());
        assert_eq!(
            validate_record_input(&identity(), b"{}", 100, Some(100)),
            Err(RecordStoreError::InvalidInput)
        );
    }

    #[test]
    fn handle_hmac_retirement_requires_zero_live_dependencies_and_non_active_version() {
        let clear = HandleHmacDependencyStatus {
            version: 1,
            active: false,
            retained: true,
            live_records: 0,
            idempotency_records: 0,
            live_refresh_leases: 0,
        };
        assert!(clear.can_retire());
        assert!(
            !HandleHmacDependencyStatus {
                active: true,
                ..clear
            }
            .can_retire()
        );
        assert!(
            !HandleHmacDependencyStatus {
                idempotency_records: 1,
                ..clear
            }
            .can_retire()
        );
        assert!(
            !HandleHmacDependencyStatus {
                retained: false,
                ..clear
            }
            .can_retire()
        );
    }

    #[test]
    fn lookup_candidate_slots_are_fixed_bounded_and_unique() {
        let candidates = vec![candidate(1, "a"), candidate(2, "b")];
        assert_eq!(
            candidate_digest_slots(&candidates),
            Ok([
                candidates[0].digest.as_str(),
                candidates[1].digest.as_str(),
                "",
                "",
            ])
        );

        assert_eq!(candidate_digest_slots(&[]), Err(RecordStoreError::Crypto));
        assert_eq!(
            candidate_digest_slots(&[
                candidate(1, "a"),
                candidate(2, "b"),
                candidate(3, "c"),
                candidate(4, "d"),
                candidate(5, "e"),
            ]),
            Err(RecordStoreError::Crypto)
        );
        assert_eq!(
            candidate_digest_slots(&[candidate(1, "a"), candidate(1, "b")]),
            Err(RecordStoreError::Crypto)
        );
        assert_eq!(
            candidate_digest_slots(&[candidate(1, "a"), candidate(2, "a")]),
            Err(RecordStoreError::Crypto)
        );
    }

    #[test]
    fn idempotency_lookup_accepts_one_exact_pair_and_rejects_mismatch_or_ambiguity() {
        let candidates = vec![candidate(1, "a"), candidate(2, "b")];
        let exact_result = resolve_idempotency_rows(
            &candidates,
            vec![IdempotencyRow {
                idempotency_digest: candidates[1].digest.clone(),
                request_sha256: "f".repeat(64),
                hmac_version: 2,
            }],
        );
        assert!(matches!(
            exact_result.as_ref(),
            Ok(Some(exact))
                if exact.lookup == candidates[1] && exact.request_sha256 == "f".repeat(64)
        ));

        assert_eq!(
            resolve_idempotency_rows(
                &candidates,
                vec![IdempotencyRow {
                    idempotency_digest: candidates[1].digest.clone(),
                    request_sha256: "f".repeat(64),
                    hmac_version: 1,
                }],
            )
            .err(),
            Some(RecordStoreError::Crypto)
        );
        assert_eq!(
            resolve_idempotency_rows(
                &candidates,
                vec![
                    IdempotencyRow {
                        idempotency_digest: candidates[0].digest.clone(),
                        request_sha256: "e".repeat(64),
                        hmac_version: 1,
                    },
                    IdempotencyRow {
                        idempotency_digest: candidates[1].digest.clone(),
                        request_sha256: "f".repeat(64),
                        hmac_version: 2,
                    },
                ],
            )
            .err(),
            Some(RecordStoreError::Crypto)
        );
    }

    #[test]
    fn record_lookup_accepts_one_exact_pair_and_rejects_malformed_metadata_or_ambiguity() {
        let candidates = vec![candidate(1, "a"), candidate(2, "b")];
        assert_eq!(
            resolve_lookup_rows(
                &candidates,
                vec![lookup_row(&candidates[0], candidates[0].digest.clone())],
            ),
            Ok(Some(candidates[0].clone()))
        );
        assert_eq!(
            resolve_lookup_rows(
                &candidates,
                vec![LookupMetadataRow {
                    lookup_digest: candidates[0].digest.clone(),
                    lookup_hmac_version: 2,
                    logical_id: candidates[0].digest.clone(),
                }],
            ),
            Err(RecordStoreError::Crypto)
        );
        assert_eq!(
            resolve_lookup_rows(
                &candidates,
                vec![lookup_row(&candidates[0], "c".repeat(64))],
            ),
            Err(RecordStoreError::Crypto)
        );
        assert_eq!(
            resolve_lookup_rows(
                &candidates,
                vec![
                    lookup_row(&candidates[0], candidates[0].digest.clone()),
                    lookup_row(&candidates[1], candidates[1].digest.clone()),
                ],
            ),
            Err(RecordStoreError::Crypto)
        );
    }

    fn candidate(version: u32, nibble: &str) -> LookupDigest {
        LookupDigest {
            version,
            digest: nibble.repeat(64),
        }
    }

    fn lookup_row(candidate: &LookupDigest, logical_id: String) -> LookupMetadataRow {
        LookupMetadataRow {
            lookup_digest: candidate.digest.clone(),
            lookup_hmac_version: i64::from(candidate.version),
            logical_id,
        }
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
