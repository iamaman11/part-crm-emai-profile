use crate::contact_protection::{ContactNonceSource, RustCryptoContactProtection};
use application_ports::clients::{
    ContactEncryptionRequest, ContactExactLookupRequest, ContactProtectionPort,
};
use client_domain::{
    ContactKind, ContactNormalizationVersion, ContactProtectionVersion, EncryptedContactValue,
    EncryptionKeyVersion, LookupKeyVersion, exact_lookup_hmac_input, normalize_contact_value,
};
use profile_platform_primitives::{ContactPointId, TenantId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const MAX_RECONCILIATION_BATCH: u32 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactKeyLifecycleError {
    InvalidInput,
    InvalidStoredMetadata,
    KeyUnavailable,
    StorageUnavailable,
    ConcurrentMutation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContactKeyDependencyStatus {
    pub version: u32,
    pub active: bool,
    pub retained: bool,
    pub physical_rows: u32,
}

impl ContactKeyDependencyStatus {
    #[must_use]
    pub const fn can_retire(self) -> bool {
        self.retained && !self.active && self.physical_rows == 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactKeyLifecycleSnapshot {
    pub encryption: Vec<ContactKeyDependencyStatus>,
    pub lookup: Vec<ContactKeyDependencyStatus>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContactKeyReconciliationResult {
    pub active_encryption_version: u32,
    pub active_lookup_version: u32,
    pub scanned_rows: u32,
    pub reprotected_rows: u32,
    pub remaining_rows: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContactKeyLifecycleMetadata {
    active_encryption_version: u32,
    active_lookup_version: u32,
    retained_encryption_versions: Vec<u32>,
    retained_lookup_versions: Vec<u32>,
}

impl ContactKeyLifecycleMetadata {
    pub(crate) fn new(
        active_encryption_version: u32,
        active_lookup_version: u32,
        retained_encryption_versions: Vec<u32>,
        retained_lookup_versions: Vec<u32>,
    ) -> Result<Self, ContactKeyLifecycleError> {
        if active_encryption_version == 0
            || active_lookup_version == 0
            || !valid_retained_versions(&retained_encryption_versions, active_encryption_version)
            || !valid_retained_versions(&retained_lookup_versions, active_lookup_version)
        {
            return Err(ContactKeyLifecycleError::InvalidInput);
        }
        Ok(Self {
            active_encryption_version,
            active_lookup_version,
            retained_encryption_versions,
            retained_lookup_versions,
        })
    }
}

pub struct D1ContactKeyLifecycle<N> {
    database: D1Database,
    protection: RustCryptoContactProtection<N>,
    metadata: ContactKeyLifecycleMetadata,
}

impl<N> D1ContactKeyLifecycle<N> {
    #[must_use]
    pub(crate) const fn new(
        database: D1Database,
        protection: RustCryptoContactProtection<N>,
        metadata: ContactKeyLifecycleMetadata,
    ) -> Self {
        Self {
            database,
            protection,
            metadata,
        }
    }

    pub async fn status(&self) -> Result<ContactKeyLifecycleSnapshot, ContactKeyLifecycleError> {
        let encryption_rows = query!(
            &self.database,
            r#"
            SELECT encryption_key_version AS version, COUNT(*) AS physical_rows
            FROM client_contact_points
            GROUP BY encryption_key_version
            ORDER BY encryption_key_version
            "#
        )
        .all()
        .await
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
        .results::<DependencyCountRow>()
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?;
        let lookup_rows = query!(
            &self.database,
            r#"
            SELECT lookup_key_version AS version, COUNT(*) AS physical_rows
            FROM client_contact_points
            GROUP BY lookup_key_version
            ORDER BY lookup_key_version
            "#
        )
        .all()
        .await
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
        .results::<DependencyCountRow>()
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?;

        Ok(ContactKeyLifecycleSnapshot {
            encryption: merge_dependency_counts(
                self.metadata.active_encryption_version,
                &self.metadata.retained_encryption_versions,
                encryption_rows,
            )?,
            lookup: merge_dependency_counts(
                self.metadata.active_lookup_version,
                &self.metadata.retained_lookup_versions,
                lookup_rows,
            )?,
        })
    }
}

impl<N: ContactNonceSource> D1ContactKeyLifecycle<N> {
    pub async fn reconcile(
        &self,
        limit: u32,
    ) -> Result<ContactKeyReconciliationResult, ContactKeyLifecycleError> {
        if !(1..=MAX_RECONCILIATION_BATCH).contains(&limit) {
            return Err(ContactKeyLifecycleError::InvalidInput);
        }
        let active_encryption = i64::from(self.metadata.active_encryption_version);
        let active_lookup = i64::from(self.metadata.active_lookup_version);
        let limit_sql = i64::from(limit);
        let rows = query!(
            &self.database,
            r#"
            SELECT tenant_id, contact_point_id, kind, normalization_version, protection_version,
                   hex(ciphertext) AS ciphertext_hex, hex(nonce) AS nonce_hex,
                   encryption_key_version, hex(exact_lookup_token) AS exact_lookup_hex,
                   lookup_key_version
            FROM client_contact_points
            WHERE encryption_key_version <> ? OR lookup_key_version <> ?
            ORDER BY tenant_id, contact_point_id
            LIMIT ?
            "#,
            active_encryption,
            active_lookup,
            limit_sql
        )
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
        .all()
        .await
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
        .results::<ProtectedContactRow>()
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?;

        let scanned_rows =
            u32::try_from(rows.len()).map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?;
        let mut reprotected_rows = 0_u32;
        for row in rows {
            self.reconcile_row(row).await?;
            reprotected_rows = reprotected_rows
                .checked_add(1)
                .ok_or(ContactKeyLifecycleError::InvalidStoredMetadata)?;
        }

        let remaining = query!(
            &self.database,
            r#"
            SELECT COUNT(*) AS physical_rows
            FROM client_contact_points
            WHERE encryption_key_version <> ? OR lookup_key_version <> ?
            "#,
            active_encryption,
            active_lookup
        )
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
        .first::<RemainingCountRow>(None)
        .await
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
        .ok_or(ContactKeyLifecycleError::StorageUnavailable)?;

        Ok(ContactKeyReconciliationResult {
            active_encryption_version: self.metadata.active_encryption_version,
            active_lookup_version: self.metadata.active_lookup_version,
            scanned_rows,
            reprotected_rows,
            remaining_rows: count_to_u32(remaining.physical_rows)?,
        })
    }

    async fn reconcile_row(
        &self,
        row: ProtectedContactRow,
    ) -> Result<(), ContactKeyLifecycleError> {
        let tenant_id = TenantId::parse(&row.tenant_id)
            .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        let contact_point_id = ContactPointId::parse(&row.contact_point_id)
            .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        let kind = parse_kind(&row.kind)?;
        let normalization_version = parse_normalization_version(row.normalization_version)?;
        let protection_version = parse_protection_version(row.protection_version)?;
        let old_encryption_version = EncryptionKeyVersion::new(
            u32::try_from(row.encryption_key_version)
                .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?,
        )
        .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        let old_lookup_version = LookupKeyVersion::new(
            u32::try_from(row.lookup_key_version)
                .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?,
        )
        .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        let ciphertext = decode_hex(&row.ciphertext_hex)?;
        let nonce = decode_hex(&row.nonce_hex)?;
        let old_lookup_bytes: [u8; 32] = decode_hex(&row.exact_lookup_hex)?
            .try_into()
            .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        let encrypted = EncryptedContactValue::new(ciphertext, nonce, old_encryption_version)
            .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        let plaintext = self
            .protection
            .decrypt_contact_display(
                &tenant_id,
                &contact_point_id,
                protection_version,
                &encrypted,
            )
            .map_err(map_crypto_error)?;
        let normalized = normalize_contact_value(kind, normalization_version, plaintext.as_str())
            .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        if normalized.expose() != plaintext.as_str() {
            return Err(ContactKeyLifecycleError::InvalidStoredMetadata);
        }
        let hmac_input =
            exact_lookup_hmac_input(&tenant_id, kind, normalization_version, &normalized);
        let old_candidate = self
            .protection
            .derive_lookup_candidates(&tenant_id, &hmac_input)
            .map_err(map_crypto_error)?
            .into_iter()
            .find(|candidate| candidate.key_version() == old_lookup_version)
            .ok_or(ContactKeyLifecycleError::KeyUnavailable)?;
        if !constant_time_eq(old_candidate.bytes(), &old_lookup_bytes) {
            return Err(ContactKeyLifecycleError::InvalidStoredMetadata);
        }

        let replacement_encrypted = self
            .protection
            .encrypt_contact_display(ContactEncryptionRequest::new(
                &tenant_id,
                &contact_point_id,
                protection_version,
                &normalized,
            ))
            .await
            .map_err(|_| ContactKeyLifecycleError::KeyUnavailable)?;
        let replacement_lookup = self
            .protection
            .derive_exact_lookup_token(ContactExactLookupRequest::new(
                &tenant_id,
                kind,
                normalization_version,
                &hmac_input,
            ))
            .await
            .map_err(|_| ContactKeyLifecycleError::KeyUnavailable)?;
        if replacement_encrypted.key_version().value() != self.metadata.active_encryption_version
            || replacement_lookup.key_version().value() != self.metadata.active_lookup_version
        {
            return Err(ContactKeyLifecycleError::InvalidStoredMetadata);
        }

        let result = query!(
            &self.database,
            r#"
            UPDATE client_contact_points
            SET ciphertext = ?, nonce = ?, encryption_key_version = ?,
                exact_lookup_token = ?, lookup_key_version = ?
            WHERE tenant_id = ? AND contact_point_id = ?
              AND encryption_key_version = ? AND lookup_key_version = ?
              AND hex(ciphertext) = ? AND hex(nonce) = ? AND hex(exact_lookup_token) = ?
            "#,
            replacement_encrypted.ciphertext(),
            replacement_encrypted.nonce(),
            i64::from(replacement_encrypted.key_version().value()),
            replacement_lookup.bytes().as_slice(),
            i64::from(replacement_lookup.key_version().value()),
            row.tenant_id,
            row.contact_point_id,
            row.encryption_key_version,
            row.lookup_key_version,
            row.ciphertext_hex,
            row.nonce_hex,
            row.exact_lookup_hex
        )
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
        .run()
        .await
        .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?;
        let changes = result
            .meta()
            .map_err(|_| ContactKeyLifecycleError::StorageUnavailable)?
            .and_then(|meta| meta.changes)
            .unwrap_or_default();
        if changes != 1 {
            return Err(ContactKeyLifecycleError::ConcurrentMutation);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct DependencyCountRow {
    version: i64,
    physical_rows: i64,
}

#[derive(Deserialize)]
struct RemainingCountRow {
    physical_rows: i64,
}

#[derive(Deserialize)]
struct ProtectedContactRow {
    tenant_id: String,
    contact_point_id: String,
    kind: String,
    normalization_version: i64,
    protection_version: i64,
    ciphertext_hex: String,
    nonce_hex: String,
    encryption_key_version: i64,
    exact_lookup_hex: String,
    lookup_key_version: i64,
}

fn merge_dependency_counts(
    active_version: u32,
    retained_versions: &[u32],
    rows: Vec<DependencyCountRow>,
) -> Result<Vec<ContactKeyDependencyStatus>, ContactKeyLifecycleError> {
    let mut statuses = retained_versions
        .iter()
        .copied()
        .map(|version| ContactKeyDependencyStatus {
            version,
            active: version == active_version,
            retained: true,
            physical_rows: 0,
        })
        .collect::<Vec<_>>();
    for row in rows {
        let version = u32::try_from(row.version)
            .map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)?;
        let physical_rows = count_to_u32(row.physical_rows)?;
        if version == 0 || physical_rows == 0 {
            return Err(ContactKeyLifecycleError::InvalidStoredMetadata);
        }
        if let Some(status) = statuses.iter_mut().find(|status| status.version == version) {
            status.physical_rows = physical_rows;
        } else {
            statuses.push(ContactKeyDependencyStatus {
                version,
                active: version == active_version,
                retained: false,
                physical_rows,
            });
        }
    }
    statuses.sort_by_key(|status| status.version);
    Ok(statuses)
}

fn valid_retained_versions(versions: &[u32], active_version: u32) -> bool {
    !versions.is_empty()
        && versions.contains(&active_version)
        && versions.iter().all(|version| *version > 0)
        && versions
            .iter()
            .enumerate()
            .all(|(index, version)| !versions[..index].contains(version))
}

fn parse_kind(value: &str) -> Result<ContactKind, ContactKeyLifecycleError> {
    match value {
        "EMAIL" => Ok(ContactKind::Email),
        "PHONE" => Ok(ContactKind::Phone),
        "URL" => Ok(ContactKind::Url),
        _ => Err(ContactKeyLifecycleError::InvalidStoredMetadata),
    }
}

fn parse_normalization_version(
    value: i64,
) -> Result<ContactNormalizationVersion, ContactKeyLifecycleError> {
    match value {
        1 => Ok(ContactNormalizationVersion::V1),
        _ => Err(ContactKeyLifecycleError::InvalidStoredMetadata),
    }
}

fn parse_protection_version(
    value: i64,
) -> Result<ContactProtectionVersion, ContactKeyLifecycleError> {
    match value {
        1 => Ok(ContactProtectionVersion::V1),
        _ => Err(ContactKeyLifecycleError::InvalidStoredMetadata),
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ContactKeyLifecycleError> {
    if !value.len().is_multiple_of(2) {
        return Err(ContactKeyLifecycleError::InvalidStoredMetadata);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high =
                hex_nibble(pair[0]).ok_or(ContactKeyLifecycleError::InvalidStoredMetadata)?;
            let low = hex_nibble(pair[1]).ok_or(ContactKeyLifecycleError::InvalidStoredMetadata)?;
            Ok((high << 4) | low)
        })
        .collect()
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (*left ^ *right)
        })
        == 0
}

fn count_to_u32(value: i64) -> Result<u32, ContactKeyLifecycleError> {
    u32::try_from(value).map_err(|_| ContactKeyLifecycleError::InvalidStoredMetadata)
}

const fn map_crypto_error(
    error: crate::contact_protection::ContactCryptoError,
) -> ContactKeyLifecycleError {
    match error {
        crate::contact_protection::ContactCryptoError::KeyUnavailable
        | crate::contact_protection::ContactCryptoError::InvalidKeyring => {
            ContactKeyLifecycleError::KeyUnavailable
        }
        crate::contact_protection::ContactCryptoError::RandomnessUnavailable
        | crate::contact_protection::ContactCryptoError::EncryptionFailed
        | crate::contact_protection::ContactCryptoError::AuthenticationFailed
        | crate::contact_protection::ContactCryptoError::InvalidProtectedValue
        | crate::contact_protection::ContactCryptoError::InvalidUtf8 => {
            ContactKeyLifecycleError::InvalidStoredMetadata
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContactKeyDependencyStatus, ContactKeyLifecycleError, ContactKeyLifecycleMetadata,
        DependencyCountRow, merge_dependency_counts,
    };

    #[test]
    fn retirement_requires_retained_non_active_zero_physical_dependencies() {
        let clear = ContactKeyDependencyStatus {
            version: 1,
            active: false,
            retained: true,
            physical_rows: 0,
        };
        assert!(clear.can_retire());
        assert!(
            !ContactKeyDependencyStatus {
                active: true,
                ..clear
            }
            .can_retire()
        );
        assert!(
            !ContactKeyDependencyStatus {
                retained: false,
                ..clear
            }
            .can_retire()
        );
        assert!(
            !ContactKeyDependencyStatus {
                physical_rows: 1,
                ..clear
            }
            .can_retire()
        );
    }

    #[test]
    fn snapshot_surfaces_missing_retained_key_versions_fail_closed() {
        let statuses = merge_dependency_counts(
            2,
            &[1, 2],
            vec![
                DependencyCountRow {
                    version: 1,
                    physical_rows: 4,
                },
                DependencyCountRow {
                    version: 3,
                    physical_rows: 2,
                },
            ],
        );
        assert!(matches!(
            statuses.as_ref(),
            Ok(values)
                if values.iter().any(|status| status.version == 3
                    && !status.retained
                    && status.physical_rows == 2)
        ));
    }

    #[test]
    fn lifecycle_metadata_requires_explicit_active_versions_inside_unique_retained_sets() {
        assert!(ContactKeyLifecycleMetadata::new(2, 3, vec![1, 2], vec![1, 3]).is_ok());
        assert_eq!(
            ContactKeyLifecycleMetadata::new(4, 3, vec![1, 2], vec![1, 3]),
            Err(ContactKeyLifecycleError::InvalidInput)
        );
        assert_eq!(
            ContactKeyLifecycleMetadata::new(2, 3, vec![1, 2, 2], vec![1, 3]),
            Err(ContactKeyLifecycleError::InvalidInput)
        );
    }
}
