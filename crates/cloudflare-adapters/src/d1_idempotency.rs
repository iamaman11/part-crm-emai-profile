use profile_platform_primitives::{ActorId, IdempotencyKey, TenantScope, UnixMillis};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, Result, query};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl IdempotencyReceipt {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn result_reference(&self) -> Option<&str> {
        self.result_reference.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdempotencyDecision {
    Miss,
    Replay(IdempotencyReceipt),
    Conflict,
}

pub struct D1IdempotencyRepository {
    database: D1Database,
}

impl D1IdempotencyRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn decide(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        key: &IdempotencyKey,
        command_name: &str,
        request_digest: &str,
        now: UnixMillis,
    ) -> Result<IdempotencyDecision> {
        let row = query!(
            &self.database,
            r#"
            SELECT command_name, request_digest, result_code,
                   result_reference, expires_at_ms
            FROM idempotency_records
            WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
            "#,
            scope.tenant_id().as_str(),
            actor_id.as_str(),
            key.as_str()
        )?
        .first::<IdempotencyRow>(None)
        .await?;
        row.map(|row| decide_row(row, command_name, request_digest, now))
            .transpose()
            .map(|decision| decision.unwrap_or(IdempotencyDecision::Miss))
    }
}

#[derive(Deserialize)]
struct IdempotencyRow {
    command_name: String,
    request_digest: String,
    result_code: String,
    result_reference: Option<String>,
    expires_at_ms: i64,
}

fn decide_row(
    row: IdempotencyRow,
    command_name: &str,
    request_digest: &str,
    now: UnixMillis,
) -> Result<IdempotencyDecision> {
    let expires_at = u64::try_from(row.expires_at_ms)
        .map_err(|_| Error::RustError("negative idempotency expiry".to_owned()))?;
    if row.command_name != command_name
        || row.request_digest != request_digest
        || now.value() >= expires_at
    {
        return Ok(IdempotencyDecision::Conflict);
    }
    Ok(IdempotencyDecision::Replay(IdempotencyReceipt {
        result_code: row.result_code,
        result_reference: row.result_reference,
    }))
}

#[cfg(test)]
mod tests {
    use super::{IdempotencyDecision, IdempotencyRow, decide_row};
    use profile_platform_primitives::UnixMillis;

    fn row() -> IdempotencyRow {
        IdempotencyRow {
            command_name: "profile_generation.activate".to_owned(),
            request_digest: "digest_01JIDEMPOTENCY".to_owned(),
            result_code: "activated".to_owned(),
            result_reference: Some("generation_01JIDEMPOTENCY".to_owned()),
            expires_at_ms: 100,
        }
    }

    #[test]
    fn replays_only_exact_live_request() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(
            decide_row(
                row(),
                "profile_generation.activate",
                "digest_01JIDEMPOTENCY",
                UnixMillis::new(99),
            )?,
            IdempotencyDecision::Replay(_)
        ));
        assert_eq!(
            decide_row(
                row(),
                "profile_generation.verify",
                "digest_01JIDEMPOTENCY",
                UnixMillis::new(99),
            )?,
            IdempotencyDecision::Conflict
        );
        assert_eq!(
            decide_row(
                row(),
                "profile_generation.activate",
                "digest_other_01JIDEMPOTENCY",
                UnixMillis::new(99),
            )?,
            IdempotencyDecision::Conflict
        );
        assert_eq!(
            decide_row(
                row(),
                "profile_generation.activate",
                "digest_01JIDEMPOTENCY",
                UnixMillis::new(100),
            )?,
            IdempotencyDecision::Conflict
        );
        Ok(())
    }
}
