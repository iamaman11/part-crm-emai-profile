use application_ports::mailbox_jobs::{MailboxJobPortError, MailboxJobPortErrorClass};
use application_ports::mailbox_scheduling::{
    MailboxExecutionClaimOutcome, MailboxExecutionClaimWrite, MailboxExecutionCompletionOutcome,
    MailboxExecutionLease, MailboxJobDispatch, MailboxSchedulingRepositoryPort,
};
use profile_platform_primitives::{
    ActorId, AggregateVersion, MailboxBindingId, MailboxJobId, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_DUE_DISPATCHES: &str = r#"
SELECT
    job.tenant_id,
    owner.actor_id,
    job.binding_id,
    job.job_id,
    job.version AS expected_job_version,
    job.next_run_at_ms
FROM mailbox_jobs AS job
JOIN mailbox_bindings AS binding
  ON binding.tenant_id = job.tenant_id
 AND binding.binding_id = job.binding_id
JOIN memberships AS owner
  ON owner.tenant_id = job.tenant_id
 AND owner.role = 'TENANT_OWNER'
 AND owner.status = 'ACTIVE'
LEFT JOIN mailbox_job_queue_dispatches AS dispatched
  ON dispatched.tenant_id = job.tenant_id
 AND dispatched.binding_id = job.binding_id
 AND dispatched.job_id = job.job_id
 AND dispatched.expected_job_version = job.version
WHERE job.lifecycle_status IN ('SCHEDULED', 'RETRY_PENDING')
  AND job.next_run_at_ms <= ?
  AND job.attempt < job.max_attempts
  AND binding.status = 'ACTIVE'
  AND binding.execution_status = 'ACTIVE'
  AND dispatched.expected_job_version IS NULL
ORDER BY job.next_run_at_ms ASC, job.tenant_id ASC, job.job_id ASC
LIMIT ?
"#;

const MARK_DISPATCHED: &str = r#"
INSERT INTO mailbox_job_queue_dispatches (
    tenant_id,
    binding_id,
    job_id,
    expected_job_version,
    published_at_ms
) VALUES (?, ?, ?, ?, ?)
ON CONFLICT (tenant_id, binding_id, job_id, expected_job_version) DO NOTHING
"#;

const INSERT_EXECUTION_CLAIM: &str = r#"
INSERT INTO mailbox_job_execution_leases (
    tenant_id,
    binding_id,
    job_id,
    expected_job_version,
    fence,
    lease_state,
    claimed_at_ms,
    lease_expires_at_ms,
    completed_at_ms
)
SELECT
    job.tenant_id,
    job.binding_id,
    job.job_id,
    job.version,
    1,
    'ACTIVE',
    ?,
    ?,
    NULL
FROM mailbox_jobs AS job
JOIN mailbox_bindings AS binding
  ON binding.tenant_id = job.tenant_id
 AND binding.binding_id = job.binding_id
WHERE job.tenant_id = ?
  AND job.binding_id = ?
  AND job.job_id = ?
  AND job.version = ?
  AND job.lifecycle_status IN ('SCHEDULED', 'RETRY_PENDING')
  AND job.next_run_at_ms <= ?
  AND job.attempt < job.max_attempts
  AND binding.status = 'ACTIVE'
  AND binding.execution_status = 'ACTIVE'
  AND EXISTS (
      SELECT 1
      FROM memberships AS owner
      WHERE owner.tenant_id = job.tenant_id
        AND owner.actor_id = ?
        AND owner.role = 'TENANT_OWNER'
        AND owner.status = 'ACTIVE'
  )
ON CONFLICT (tenant_id, binding_id, job_id, expected_job_version) DO NOTHING
RETURNING fence, lease_expires_at_ms
"#;

const LOAD_EXECUTION_LEASE: &str = r#"
SELECT fence, lease_state, lease_expires_at_ms
FROM mailbox_job_execution_leases
WHERE tenant_id = ?
  AND binding_id = ?
  AND job_id = ?
  AND expected_job_version = ?
"#;

const RECLAIM_EXECUTION_LEASE: &str = r#"
UPDATE mailbox_job_execution_leases
SET fence = fence + 1,
    claimed_at_ms = ?,
    lease_expires_at_ms = ?,
    completed_at_ms = NULL
WHERE tenant_id = ?
  AND binding_id = ?
  AND job_id = ?
  AND expected_job_version = ?
  AND lease_state = 'ACTIVE'
  AND lease_expires_at_ms <= ?
  AND fence < 9223372036854775807
  AND EXISTS (
      SELECT 1
      FROM mailbox_jobs AS job
      JOIN mailbox_bindings AS binding
        ON binding.tenant_id = job.tenant_id
       AND binding.binding_id = job.binding_id
      WHERE job.tenant_id = mailbox_job_execution_leases.tenant_id
        AND job.binding_id = mailbox_job_execution_leases.binding_id
        AND job.job_id = mailbox_job_execution_leases.job_id
        AND job.version = mailbox_job_execution_leases.expected_job_version
        AND job.lifecycle_status IN ('SCHEDULED', 'RETRY_PENDING')
        AND job.next_run_at_ms <= ?
        AND job.attempt < job.max_attempts
        AND binding.status = 'ACTIVE'
        AND binding.execution_status = 'ACTIVE'
        AND EXISTS (
            SELECT 1
            FROM memberships AS owner
            WHERE owner.tenant_id = job.tenant_id
              AND owner.actor_id = ?
              AND owner.role = 'TENANT_OWNER'
              AND owner.status = 'ACTIVE'
        )
  )
RETURNING fence, lease_expires_at_ms
"#;

const COMPLETE_EXECUTION_LEASE: &str = r#"
UPDATE mailbox_job_execution_leases
SET lease_state = 'COMPLETED',
    completed_at_ms = ?
WHERE tenant_id = ?
  AND binding_id = ?
  AND job_id = ?
  AND expected_job_version = ?
  AND fence = ?
  AND lease_state = 'ACTIVE'
RETURNING fence
"#;

#[derive(Debug, Deserialize)]
struct DueDispatchRow {
    tenant_id: String,
    actor_id: String,
    binding_id: String,
    job_id: String,
    expected_job_version: i64,
    next_run_at_ms: i64,
}

impl DueDispatchRow {
    fn into_dispatch(self) -> Result<MailboxJobDispatch, MailboxJobPortError> {
        Ok(MailboxJobDispatch::new(
            TenantId::parse(self.tenant_id).map_err(|_| integrity_failure())?,
            ActorId::parse(self.actor_id).map_err(|_| integrity_failure())?,
            MailboxBindingId::parse(self.binding_id).map_err(|_| integrity_failure())?,
            MailboxJobId::parse(self.job_id).map_err(|_| integrity_failure())?,
            aggregate_from_i64(self.expected_job_version)?,
            unix_from_i64(self.next_run_at_ms)?,
        ))
    }
}

#[derive(Clone, Debug, Deserialize)]
struct LeaseRow {
    fence: i64,
    lease_state: String,
    lease_expires_at_ms: i64,
}

#[derive(Debug, Deserialize)]
struct LeaseProjection {
    fence: i64,
    lease_expires_at_ms: i64,
}

pub struct D1MailboxSchedulingRepository {
    database: D1Database,
}

impl D1MailboxSchedulingRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn load_lease(
        &self,
        dispatch: &MailboxJobDispatch,
    ) -> Result<Option<LeaseRow>, MailboxJobPortError> {
        query!(
            &self.database,
            LOAD_EXECUTION_LEASE,
            dispatch.tenant_id().as_str(),
            dispatch.binding_id().as_str(),
            dispatch.job_id().as_str(),
            sqlite_version(dispatch.expected_version())?
        )
        .map_err(map_worker_error)?
        .first::<LeaseRow>(None)
        .await
        .map_err(map_worker_error)
    }

    fn classify_existing_lease(
        row: LeaseRow,
        now: UnixMillis,
    ) -> Result<MailboxExecutionClaimOutcome, MailboxJobPortError> {
        match row.lease_state.as_str() {
            "COMPLETED" => Ok(MailboxExecutionClaimOutcome::Completed),
            "ACTIVE" => {
                let retry_at = unix_from_i64(row.lease_expires_at_ms)?;
                if retry_at > now {
                    Ok(MailboxExecutionClaimOutcome::InFlight { retry_at })
                } else {
                    Ok(MailboxExecutionClaimOutcome::Stale)
                }
            }
            _ => Err(integrity_failure()),
        }
    }
}

impl MailboxSchedulingRepositoryPort for D1MailboxSchedulingRepository {
    async fn load_due_dispatches(
        &self,
        now: UnixMillis,
        limit: u32,
    ) -> Result<Vec<MailboxJobDispatch>, MailboxJobPortError> {
        let rows = query!(
            &self.database,
            LOAD_DUE_DISPATCHES,
            sqlite_unix(now)?,
            i64::from(limit)
        )
        .map_err(map_worker_error)?
        .all()
        .await
        .map_err(map_worker_error)?
        .results::<DueDispatchRow>()
        .map_err(map_worker_error)?;
        rows.into_iter().map(DueDispatchRow::into_dispatch).collect()
    }

    async fn mark_dispatched(
        &self,
        dispatch: &MailboxJobDispatch,
        published_at: UnixMillis,
    ) -> Result<(), MailboxJobPortError> {
        query!(
            &self.database,
            MARK_DISPATCHED,
            dispatch.tenant_id().as_str(),
            dispatch.binding_id().as_str(),
            dispatch.job_id().as_str(),
            sqlite_version(dispatch.expected_version())?,
            sqlite_unix(published_at)?
        )
        .map_err(map_worker_error)?
        .run()
        .await
        .map_err(map_worker_error)?;
        Ok(())
    }

    async fn acquire_execution(
        &self,
        dispatch: &MailboxJobDispatch,
        write: MailboxExecutionClaimWrite,
    ) -> Result<MailboxExecutionClaimOutcome, MailboxJobPortError> {
        if write.lease_expires_at() <= write.claimed_at() {
            return Err(integrity_failure());
        }
        let expected_version = sqlite_version(dispatch.expected_version())?;
        let claimed_at = sqlite_unix(write.claimed_at())?;
        let lease_expires_at = sqlite_unix(write.lease_expires_at())?;
        let inserted = query!(
            &self.database,
            INSERT_EXECUTION_CLAIM,
            claimed_at,
            lease_expires_at,
            dispatch.tenant_id().as_str(),
            dispatch.binding_id().as_str(),
            dispatch.job_id().as_str(),
            expected_version,
            claimed_at,
            dispatch.actor_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<LeaseProjection>(None)
        .await
        .map_err(map_worker_error)?;
        if let Some(row) = inserted {
            return Ok(MailboxExecutionClaimOutcome::Acquired(
                MailboxExecutionLease::new(
                    fence_from_i64(row.fence)?,
                    unix_from_i64(row.lease_expires_at_ms)?,
                ),
            ));
        }

        let Some(existing) = self.load_lease(dispatch).await? else {
            return Ok(MailboxExecutionClaimOutcome::Stale);
        };
        if existing.lease_state == "COMPLETED" {
            return Ok(MailboxExecutionClaimOutcome::Completed);
        }
        if existing.lease_state != "ACTIVE" {
            return Err(integrity_failure());
        }
        let existing_expires_at = unix_from_i64(existing.lease_expires_at_ms)?;
        if existing_expires_at > write.claimed_at() {
            return Ok(MailboxExecutionClaimOutcome::InFlight {
                retry_at: existing_expires_at,
            });
        }

        let reclaimed = query!(
            &self.database,
            RECLAIM_EXECUTION_LEASE,
            claimed_at,
            lease_expires_at,
            dispatch.tenant_id().as_str(),
            dispatch.binding_id().as_str(),
            dispatch.job_id().as_str(),
            expected_version,
            claimed_at,
            claimed_at,
            dispatch.actor_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<LeaseProjection>(None)
        .await
        .map_err(map_worker_error)?;
        if let Some(row) = reclaimed {
            return Ok(MailboxExecutionClaimOutcome::Acquired(
                MailboxExecutionLease::new(
                    fence_from_i64(row.fence)?,
                    unix_from_i64(row.lease_expires_at_ms)?,
                ),
            ));
        }

        match self.load_lease(dispatch).await? {
            Some(row) => Self::classify_existing_lease(row, write.claimed_at()),
            None => Ok(MailboxExecutionClaimOutcome::Stale),
        }
    }

    async fn complete_execution(
        &self,
        dispatch: &MailboxJobDispatch,
        fence: u64,
        completed_at: UnixMillis,
    ) -> Result<MailboxExecutionCompletionOutcome, MailboxJobPortError> {
        let row = query!(
            &self.database,
            COMPLETE_EXECUTION_LEASE,
            sqlite_unix(completed_at)?,
            dispatch.tenant_id().as_str(),
            dispatch.binding_id().as_str(),
            dispatch.job_id().as_str(),
            sqlite_version(dispatch.expected_version())?,
            sqlite_fence(fence)?
        )
        .map_err(map_worker_error)?
        .first::<i64>(Some("fence"))
        .await
        .map_err(map_worker_error)?;
        Ok(if row.is_some() {
            MailboxExecutionCompletionOutcome::Applied
        } else {
            MailboxExecutionCompletionOutcome::Stale
        })
    }
}

fn sqlite_version(value: AggregateVersion) -> Result<i64, MailboxJobPortError> {
    i64::try_from(value.value()).map_err(|_| integrity_failure())
}

fn sqlite_unix(value: UnixMillis) -> Result<i64, MailboxJobPortError> {
    i64::try_from(value.value()).map_err(|_| integrity_failure())
}

fn sqlite_fence(value: u64) -> Result<i64, MailboxJobPortError> {
    if value == 0 {
        return Err(integrity_failure());
    }
    i64::try_from(value).map_err(|_| integrity_failure())
}

fn aggregate_from_i64(value: i64) -> Result<AggregateVersion, MailboxJobPortError> {
    u64::try_from(value)
        .ok()
        .and_then(|value| AggregateVersion::new(value).ok())
        .ok_or_else(integrity_failure)
}

fn unix_from_i64(value: i64) -> Result<UnixMillis, MailboxJobPortError> {
    u64::try_from(value)
        .map(UnixMillis::new)
        .map_err(|_| integrity_failure())
}

fn fence_from_i64(value: i64) -> Result<u64, MailboxJobPortError> {
    let fence = u64::try_from(value).map_err(|_| integrity_failure())?;
    if fence == 0 {
        return Err(integrity_failure());
    }
    Ok(fence)
}

fn integrity_failure() -> MailboxJobPortError {
    MailboxJobPortError::new(MailboxJobPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> MailboxJobPortError {
    MailboxJobPortError::new(MailboxJobPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{fence_from_i64, unix_from_i64};

    #[test]
    fn lease_projection_rejects_invalid_numeric_state() {
        assert!(fence_from_i64(0).is_err());
        assert!(fence_from_i64(-1).is_err());
        assert!(unix_from_i64(-1).is_err());
        assert_eq!(fence_from_i64(1), Ok(1));
    }
}
