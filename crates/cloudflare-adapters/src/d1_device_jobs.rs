use application_ports::{
    DeviceJobInsertOutcome, DeviceJobPortError, DeviceJobPortErrorClass, DeviceJobQueryPort,
    DeviceJobRepositoryPort, DeviceJobWriteOutcome,
};
use device_domain::{
    DeviceClaimId, DeviceClaimSnapshot, DeviceJob, DeviceJobId, DeviceJobSnapshot, DeviceJobStatus,
    DeviceJobTarget,
};
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, GenerationId, ProfileId, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const INSERT_DEVICE_JOB: &str = r#"
INSERT INTO device_jobs (
    tenant_id,
    job_id,
    device_id,
    profile_id,
    generation_id,
    aggregate_version,
    status,
    attempt,
    max_attempts,
    last_fence,
    current_claim_id,
    claim_fence,
    claimed_at_ms,
    claim_heartbeat_at_ms,
    claim_lease_expires_at_ms,
    retry_at_ms,
    updated_at_ms
)
SELECT
    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
    NULLIF(?, ''), NULLIF(?, -1), NULLIF(?, -1), NULLIF(?, -1),
    NULLIF(?, -1), NULLIF(?, -1), ?
WHERE NOT EXISTS (
    SELECT 1 FROM device_jobs WHERE tenant_id = ? AND job_id = ?
)
RETURNING job_id
"#;

const LOAD_DEVICE_JOB: &str = r#"
SELECT
    tenant_id,
    job_id,
    device_id,
    profile_id,
    generation_id,
    aggregate_version,
    status,
    attempt,
    max_attempts,
    last_fence,
    current_claim_id,
    claim_fence,
    claimed_at_ms,
    claim_heartbeat_at_ms,
    claim_lease_expires_at_ms,
    retry_at_ms,
    updated_at_ms
FROM device_jobs
WHERE tenant_id = ? AND job_id = ?
"#;

const LIST_CLAIMABLE_DEVICE_JOBS: &str = r#"
SELECT
    job.tenant_id,
    job.job_id,
    job.device_id,
    job.profile_id,
    job.generation_id,
    job.aggregate_version,
    job.status,
    job.attempt,
    job.max_attempts,
    job.last_fence,
    job.current_claim_id,
    job.claim_fence,
    job.claimed_at_ms,
    job.claim_heartbeat_at_ms,
    job.claim_lease_expires_at_ms,
    job.retry_at_ms,
    job.updated_at_ms
FROM device_jobs AS job
WHERE job.tenant_id = ?
  AND job.device_id = ?
  AND EXISTS (
      SELECT 1
      FROM device_authorizations AS authorization
      WHERE authorization.tenant_id = job.tenant_id
        AND authorization.device_id = job.device_id
        AND authorization.profile_id = job.profile_id
        AND authorization.generation_id = job.generation_id
        AND authorization.status = 'ACTIVE'
        AND authorization.version >= 1
  )
  AND EXISTS (
      SELECT 1
      FROM memberships AS membership
      WHERE membership.tenant_id = job.tenant_id
        AND membership.actor_id = ?
        AND membership.status = 'ACTIVE'
        AND (
            membership.role = 'TENANT_OWNER'
            OR (
                membership.role = 'MEMBER'
                AND EXISTS (
                    SELECT 1
                    FROM profile_grants AS grant_row
                    WHERE grant_row.tenant_id = job.tenant_id
                      AND grant_row.profile_id = job.profile_id
                      AND grant_row.actor_id = membership.actor_id
                )
            )
        )
  )
  AND (
      job.status = 'PENDING_DEVICE'
      OR (
          job.status IN ('PROFILE_BUSY', 'RETRY_SCHEDULED')
          AND job.retry_at_ms IS NOT NULL
          AND job.retry_at_ms <= ?
      )
  )
ORDER BY COALESCE(job.retry_at_ms, job.updated_at_ms), job.updated_at_ms, job.job_id
LIMIT ?
"#;

const CAS_DEVICE_JOB: &str = r#"
UPDATE device_jobs
SET aggregate_version = ?,
    status = ?,
    attempt = ?,
    max_attempts = ?,
    last_fence = ?,
    current_claim_id = NULLIF(?, ''),
    claim_fence = NULLIF(?, -1),
    claimed_at_ms = NULLIF(?, -1),
    claim_heartbeat_at_ms = NULLIF(?, -1),
    claim_lease_expires_at_ms = NULLIF(?, -1),
    retry_at_ms = NULLIF(?, -1),
    updated_at_ms = ?
WHERE tenant_id = ?
  AND job_id = ?
  AND device_id = ?
  AND profile_id = ?
  AND generation_id = ?
  AND aggregate_version = ?
RETURNING job_id
"#;

#[derive(Deserialize)]
struct DeviceJobRow {
    tenant_id: String,
    job_id: String,
    device_id: String,
    profile_id: String,
    generation_id: String,
    aggregate_version: i64,
    status: String,
    attempt: i64,
    max_attempts: i64,
    last_fence: i64,
    current_claim_id: Option<String>,
    claim_fence: Option<i64>,
    claimed_at_ms: Option<i64>,
    claim_heartbeat_at_ms: Option<i64>,
    claim_lease_expires_at_ms: Option<i64>,
    retry_at_ms: Option<i64>,
    updated_at_ms: i64,
}

struct StoredClaimFields {
    claim_id: String,
    fence: i64,
    claimed_at_ms: i64,
    heartbeat_at_ms: i64,
    lease_expires_at_ms: i64,
}

pub struct D1DeviceJobRepository {
    database: D1Database,
}

impl D1DeviceJobRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl DeviceJobQueryPort for D1DeviceJobRepository {
    async fn list_claimable_device_jobs(
        &self,
        actor: &ActorContext,
        device_id: &DeviceId,
        now: UnixMillis,
        limit: u16,
    ) -> Result<Vec<DeviceJob>, DeviceJobPortError> {
        if limit == 0 {
            return Err(integrity_failure());
        }
        let tenant_id = actor.tenant_scope().tenant_id();
        let result = query!(
            &self.database,
            LIST_CLAIMABLE_DEVICE_JOBS,
            tenant_id.as_str(),
            device_id.as_str(),
            actor.actor_id().as_str(),
            unix_to_i64(now)?,
            i64::from(limit),
        )
        .map_err(map_worker_error)?
        .all()
        .await
        .map_err(map_worker_error)?;
        let rows = result
            .results::<DeviceJobRow>()
            .map_err(map_worker_error)?;
        if rows.len() > usize::from(limit) {
            return Err(integrity_failure());
        }
        rows.into_iter()
            .map(|row| {
                let job_id = DeviceJobId::parse(row.job_id.as_str())
                    .map_err(|_| integrity_failure())?;
                restore_row(tenant_id, &job_id, row)
            })
            .collect()
    }
}

impl DeviceJobRepositoryPort for D1DeviceJobRepository {
    async fn insert_device_job(
        &self,
        tenant_id: &TenantId,
        job: &DeviceJob,
    ) -> Result<DeviceJobInsertOutcome, DeviceJobPortError> {
        require_tenant_binding(tenant_id, job.target())?;
        let snapshot = job.snapshot();
        if snapshot.aggregate_version != AggregateVersion::INITIAL.value()
            || snapshot.status != DeviceJobStatus::PendingDevice
            || snapshot.attempt != 0
            || snapshot.last_fence != 0
            || snapshot.active_claim.is_some()
            || snapshot.retry_at.is_some()
        {
            return Err(integrity_failure());
        }
        let stored = stored_values(&snapshot)?;
        let aggregate_version = u64_to_i64(snapshot.aggregate_version)?;
        let last_fence = u64_to_i64(snapshot.last_fence)?;
        let retry_at_ms = optional_unix_to_i64(snapshot.retry_at)?;
        let updated_at_ms = unix_to_i64(snapshot.updated_at)?;
        let target = &snapshot.target;
        let returned = query!(
            &self.database,
            INSERT_DEVICE_JOB,
            tenant_id.as_str(),
            snapshot.job_id.as_str(),
            target.device_id().as_str(),
            target.profile_id().as_str(),
            target.generation_id().as_str(),
            aggregate_version,
            status_to_storage(snapshot.status),
            i64::from(snapshot.attempt),
            i64::from(snapshot.max_attempts),
            last_fence,
            stored.claim_id.as_str(),
            stored.fence,
            stored.claimed_at_ms,
            stored.heartbeat_at_ms,
            stored.lease_expires_at_ms,
            retry_at_ms,
            updated_at_ms,
            tenant_id.as_str(),
            snapshot.job_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("job_id"))
        .await
        .map_err(map_worker_error)?;

        Ok(if returned.is_some() {
            DeviceJobInsertOutcome::Inserted
        } else {
            DeviceJobInsertOutcome::Conflict
        })
    }

    async fn load_device_job(
        &self,
        tenant_id: &TenantId,
        job_id: &DeviceJobId,
    ) -> Result<Option<DeviceJob>, DeviceJobPortError> {
        let row = query!(
            &self.database,
            LOAD_DEVICE_JOB,
            tenant_id.as_str(),
            job_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<DeviceJobRow>(None)
        .await
        .map_err(map_worker_error)?;

        row.map(|row| restore_row(tenant_id, job_id, row))
            .transpose()
    }

    async fn compare_and_swap_device_job(
        &self,
        tenant_id: &TenantId,
        expected_version: AggregateVersion,
        job: &DeviceJob,
    ) -> Result<DeviceJobWriteOutcome, DeviceJobPortError> {
        require_tenant_binding(tenant_id, job.target())?;
        let required_version = expected_version.next().map_err(|_| integrity_failure())?;
        if job.version() != required_version {
            return Err(integrity_failure());
        }

        let snapshot = job.snapshot();
        let stored = stored_values(&snapshot)?;
        let aggregate_version = u64_to_i64(snapshot.aggregate_version)?;
        let last_fence = u64_to_i64(snapshot.last_fence)?;
        let retry_at_ms = optional_unix_to_i64(snapshot.retry_at)?;
        let updated_at_ms = unix_to_i64(snapshot.updated_at)?;
        let expected_aggregate_version = u64_to_i64(expected_version.value())?;
        let target = &snapshot.target;
        let returned = query!(
            &self.database,
            CAS_DEVICE_JOB,
            aggregate_version,
            status_to_storage(snapshot.status),
            i64::from(snapshot.attempt),
            i64::from(snapshot.max_attempts),
            last_fence,
            stored.claim_id.as_str(),
            stored.fence,
            stored.claimed_at_ms,
            stored.heartbeat_at_ms,
            stored.lease_expires_at_ms,
            retry_at_ms,
            updated_at_ms,
            tenant_id.as_str(),
            snapshot.job_id.as_str(),
            target.device_id().as_str(),
            target.profile_id().as_str(),
            target.generation_id().as_str(),
            expected_aggregate_version
        )
        .map_err(map_worker_error)?
        .first::<String>(Some("job_id"))
        .await
        .map_err(map_worker_error)?;

        Ok(if returned.is_some() {
            DeviceJobWriteOutcome::Applied
        } else {
            DeviceJobWriteOutcome::VersionConflict
        })
    }
}

fn stored_values(snapshot: &DeviceJobSnapshot) -> Result<StoredClaimFields, DeviceJobPortError> {
    let Some(claim) = snapshot.active_claim.as_ref() else {
        return Ok(StoredClaimFields {
            claim_id: String::new(),
            fence: -1,
            claimed_at_ms: -1,
            heartbeat_at_ms: -1,
            lease_expires_at_ms: -1,
        });
    };
    Ok(StoredClaimFields {
        claim_id: claim.claim_id.as_str().to_owned(),
        fence: u64_to_i64(claim.fence)?,
        claimed_at_ms: unix_to_i64(claim.claimed_at)?,
        heartbeat_at_ms: unix_to_i64(claim.last_heartbeat_at)?,
        lease_expires_at_ms: unix_to_i64(claim.lease_expires_at)?,
    })
}

fn restore_row(
    requested_tenant: &TenantId,
    requested_job: &DeviceJobId,
    row: DeviceJobRow,
) -> Result<DeviceJob, DeviceJobPortError> {
    let tenant_id = TenantId::parse(row.tenant_id.as_str()).map_err(|_| integrity_failure())?;
    let job_id = DeviceJobId::parse(row.job_id.as_str()).map_err(|_| integrity_failure())?;
    if &tenant_id != requested_tenant || &job_id != requested_job {
        return Err(integrity_failure());
    }
    let target = DeviceJobTarget::new(
        tenant_id,
        DeviceId::parse(row.device_id.as_str()).map_err(|_| integrity_failure())?,
        ProfileId::parse(row.profile_id.as_str()).map_err(|_| integrity_failure())?,
        GenerationId::parse(row.generation_id.as_str()).map_err(|_| integrity_failure())?,
    );
    let active_claim = restore_claim(&job_id, &target, &row)?;
    let snapshot = DeviceJobSnapshot {
        job_id,
        target,
        aggregate_version: non_negative_u64(row.aggregate_version)?,
        status: status_from_storage(&row.status)?,
        attempt: non_negative_u32(row.attempt)?,
        max_attempts: non_negative_u32(row.max_attempts)?,
        last_fence: non_negative_u64(row.last_fence)?,
        active_claim,
        retry_at: row.retry_at_ms.map(unix_from_i64).transpose()?,
        updated_at: unix_from_i64(row.updated_at_ms)?,
    };
    DeviceJob::restore(snapshot).map_err(|_| integrity_failure())
}

fn restore_claim(
    job_id: &DeviceJobId,
    target: &DeviceJobTarget,
    row: &DeviceJobRow,
) -> Result<Option<DeviceClaimSnapshot>, DeviceJobPortError> {
    match (
        row.current_claim_id.as_deref(),
        row.claim_fence,
        row.claimed_at_ms,
        row.claim_heartbeat_at_ms,
        row.claim_lease_expires_at_ms,
    ) {
        (None, None, None, None, None) => Ok(None),
        (Some(claim_id), Some(fence), Some(claimed_at), Some(heartbeat), Some(expires_at)) => {
            Ok(Some(DeviceClaimSnapshot {
                claim_id: DeviceClaimId::parse(claim_id).map_err(|_| integrity_failure())?,
                job_id: job_id.clone(),
                target: target.clone(),
                fence: non_negative_u64(fence)?,
                claimed_at: unix_from_i64(claimed_at)?,
                last_heartbeat_at: unix_from_i64(heartbeat)?,
                lease_expires_at: unix_from_i64(expires_at)?,
            }))
        }
        _ => Err(integrity_failure()),
    }
}

fn require_tenant_binding(
    tenant_id: &TenantId,
    target: &DeviceJobTarget,
) -> Result<(), DeviceJobPortError> {
    if target.tenant_id() == tenant_id {
        Ok(())
    } else {
        Err(integrity_failure())
    }
}

const fn status_to_storage(status: DeviceJobStatus) -> &'static str {
    match status {
        DeviceJobStatus::PendingDevice => "PENDING_DEVICE",
        DeviceJobStatus::ProfileBusy => "PROFILE_BUSY",
        DeviceJobStatus::Running => "RUNNING",
        DeviceJobStatus::RetryScheduled => "RETRY_SCHEDULED",
        DeviceJobStatus::AuthRequired => "AUTH_REQUIRED",
        DeviceJobStatus::RecoveryRequired => "RECOVERY_REQUIRED",
        DeviceJobStatus::Succeeded => "SUCCEEDED",
        DeviceJobStatus::Failed => "FAILED",
        DeviceJobStatus::Cancelled => "CANCELLED",
    }
}

fn status_from_storage(value: &str) -> Result<DeviceJobStatus, DeviceJobPortError> {
    match value {
        "PENDING_DEVICE" => Ok(DeviceJobStatus::PendingDevice),
        "PROFILE_BUSY" => Ok(DeviceJobStatus::ProfileBusy),
        "RUNNING" => Ok(DeviceJobStatus::Running),
        "RETRY_SCHEDULED" => Ok(DeviceJobStatus::RetryScheduled),
        "AUTH_REQUIRED" => Ok(DeviceJobStatus::AuthRequired),
        "RECOVERY_REQUIRED" => Ok(DeviceJobStatus::RecoveryRequired),
        "SUCCEEDED" => Ok(DeviceJobStatus::Succeeded),
        "FAILED" => Ok(DeviceJobStatus::Failed),
        "CANCELLED" => Ok(DeviceJobStatus::Cancelled),
        _ => Err(integrity_failure()),
    }
}

fn optional_unix_to_i64(value: Option<UnixMillis>) -> Result<i64, DeviceJobPortError> {
    value
        .map(unix_to_i64)
        .transpose()
        .map(|value| value.unwrap_or(-1))
}

fn unix_to_i64(value: UnixMillis) -> Result<i64, DeviceJobPortError> {
    u64_to_i64(value.value())
}

fn unix_from_i64(value: i64) -> Result<UnixMillis, DeviceJobPortError> {
    non_negative_u64(value).map(UnixMillis::new)
}

fn u64_to_i64(value: u64) -> Result<i64, DeviceJobPortError> {
    i64::try_from(value).map_err(|_| integrity_failure())
}

fn non_negative_u64(value: i64) -> Result<u64, DeviceJobPortError> {
    u64::try_from(value).map_err(|_| integrity_failure())
}

fn non_negative_u32(value: i64) -> Result<u32, DeviceJobPortError> {
    u32::try_from(value).map_err(|_| integrity_failure())
}

fn integrity_failure() -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{LIST_CLAIMABLE_DEVICE_JOBS, status_from_storage, status_to_storage};
    use device_domain::DeviceJobStatus;

    #[test]
    fn storage_status_is_bounded_and_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        for status in [
            DeviceJobStatus::PendingDevice,
            DeviceJobStatus::ProfileBusy,
            DeviceJobStatus::Running,
            DeviceJobStatus::RetryScheduled,
            DeviceJobStatus::AuthRequired,
            DeviceJobStatus::RecoveryRequired,
            DeviceJobStatus::Succeeded,
            DeviceJobStatus::Failed,
            DeviceJobStatus::Cancelled,
        ] {
            assert_eq!(status_from_storage(status_to_storage(status))?, status);
        }
        assert!(status_from_storage("UNKNOWN").is_err());
        Ok(())
    }

    #[test]
    fn claimable_query_is_live_grant_device_authorization_and_due_scoped() {
        for required in [
            "job.tenant_id = ?",
            "job.device_id = ?",
            "authorization.status = 'ACTIVE'",
            "membership.status = 'ACTIVE'",
            "profile_grants",
            "job.status = 'PENDING_DEVICE'",
            "job.status IN ('PROFILE_BUSY', 'RETRY_SCHEDULED')",
            "job.retry_at_ms <= ?",
            "LIMIT ?",
        ] {
            assert!(LIST_CLAIMABLE_DEVICE_JOBS.contains(required));
        }
        assert!(!LIST_CLAIMABLE_DEVICE_JOBS.contains("profile_assignments"));
    }
}
