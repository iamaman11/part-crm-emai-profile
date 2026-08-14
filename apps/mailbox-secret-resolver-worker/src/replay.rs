use worker::d1::D1Database;
use worker::query;

const REPLAY_RETENTION_MS: u64 = 10 * 60 * 1000;
const _: () = assert!(REPLAY_RETENTION_MS > crate::model::MAX_CLOCK_SKEW_MS);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayClaimError {
    ReplayRejected,
    StorageUnavailable,
}

pub async fn claim_request_nonce(
    database: &D1Database,
    tenant_id: &str,
    nonce: &str,
    path: &str,
    body_digest: &str,
    authenticated_at_ms: u64,
) -> Result<(), ReplayClaimError> {
    let authenticated_at =
        i64::try_from(authenticated_at_ms).map_err(|_| ReplayClaimError::StorageUnavailable)?;
    let expires_at = i64::try_from(authenticated_at_ms.saturating_add(REPLAY_RETENTION_MS))
        .map_err(|_| ReplayClaimError::StorageUnavailable)?;
    query!(
        database,
        "DELETE FROM resolver_request_nonces WHERE expires_at_ms <= ?",
        authenticated_at
    )
    .map_err(|_| ReplayClaimError::StorageUnavailable)?
    .run()
    .await
    .map_err(|_| ReplayClaimError::StorageUnavailable)?;
    let result = query!(
        database,
        r#"
        INSERT INTO resolver_request_nonces (
            tenant_id, nonce, request_path, body_sha256,
            authenticated_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT (tenant_id, nonce) DO NOTHING
        "#,
        tenant_id,
        nonce,
        path,
        body_digest,
        authenticated_at,
        expires_at
    )
    .map_err(|_| ReplayClaimError::StorageUnavailable)?
    .run()
    .await
    .map_err(|_| ReplayClaimError::StorageUnavailable)?;
    let changes = result
        .meta()
        .map_err(|_| ReplayClaimError::StorageUnavailable)?
        .and_then(|meta| meta.changes)
        .unwrap_or_default();
    if changes != 1 {
        return Err(ReplayClaimError::ReplayRejected);
    }
    Ok(())
}
