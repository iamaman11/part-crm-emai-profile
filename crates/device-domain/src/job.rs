use core::fmt;
use profile_platform_primitives::{AggregateVersion, UnixMillis, VersionOverflow};

use crate::{
    claim::{DeviceClaim, DeviceClaimSnapshot},
    id::{DeviceClaimId, DeviceJobId},
    target::DeviceJobTarget,
};

const MAX_DEVICE_JOB_ATTEMPTS: u32 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobStatus {
    PendingDevice,
    ProfileBusy,
    Running,
    RetryScheduled,
    AuthRequired,
    RecoveryRequired,
    Succeeded,
    Failed,
    Cancelled,
}

impl DeviceJobStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceJob {
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    version: AggregateVersion,
    status: DeviceJobStatus,
    attempt: u32,
    max_attempts: u32,
    last_fence: u64,
    active_claim: Option<DeviceClaim>,
    retry_at: Option<UnixMillis>,
    updated_at: UnixMillis,
}

/// Bounded metadata persisted by outer adapters. It may represent corrupted
/// storage and only becomes a trusted aggregate through [`DeviceJob::restore`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceJobSnapshot {
    pub job_id: DeviceJobId,
    pub target: DeviceJobTarget,
    pub aggregate_version: u64,
    pub status: DeviceJobStatus,
    pub attempt: u32,
    pub max_attempts: u32,
    pub last_fence: u64,
    pub active_claim: Option<DeviceClaimSnapshot>,
    pub retry_at: Option<UnixMillis>,
    pub updated_at: UnixMillis,
}

impl DeviceJob {
    #[must_use]
    pub fn snapshot(&self) -> DeviceJobSnapshot {
        DeviceJobSnapshot {
            job_id: self.job_id.clone(),
            target: self.target.clone(),
            aggregate_version: self.version.value(),
            status: self.status,
            attempt: self.attempt,
            max_attempts: self.max_attempts,
            last_fence: self.last_fence,
            active_claim: self.active_claim.as_ref().map(DeviceClaim::snapshot),
            retry_at: self.retry_at,
            updated_at: self.updated_at,
        }
    }

    pub fn restore(snapshot: DeviceJobSnapshot) -> Result<Self, DeviceJobError> {
        let DeviceJobSnapshot {
            job_id,
            target,
            aggregate_version,
            status,
            attempt,
            max_attempts,
            last_fence,
            active_claim,
            retry_at,
            updated_at,
        } = snapshot;

        if max_attempts == 0
            || max_attempts > MAX_DEVICE_JOB_ATTEMPTS
            || attempt > max_attempts
            || last_fence != u64::from(attempt)
            || aggregate_version <= u64::from(attempt)
        {
            return Err(DeviceJobError::InvalidSnapshot);
        }

        let requires_attempt = matches!(
            status,
            DeviceJobStatus::ProfileBusy
                | DeviceJobStatus::Running
                | DeviceJobStatus::RetryScheduled
                | DeviceJobStatus::AuthRequired
                | DeviceJobStatus::RecoveryRequired
                | DeviceJobStatus::Succeeded
                | DeviceJobStatus::Failed
        );
        if requires_attempt && attempt == 0 {
            return Err(DeviceJobError::InvalidSnapshot);
        }

        if (status == DeviceJobStatus::Running) != active_claim.is_some() {
            return Err(DeviceJobError::InvalidSnapshot);
        }

        let requires_retry = matches!(
            status,
            DeviceJobStatus::ProfileBusy | DeviceJobStatus::RetryScheduled
        );
        if requires_retry != retry_at.is_some()
            || retry_at.is_some_and(|retry_at| retry_at <= updated_at)
        {
            return Err(DeviceJobError::InvalidSnapshot);
        }

        let active_claim = match active_claim {
            Some(snapshot) => {
                let claim =
                    DeviceClaim::restore(snapshot).map_err(|_| DeviceJobError::InvalidSnapshot)?;
                if claim.job_id() != &job_id
                    || claim.target() != &target
                    || claim.fence() != last_fence
                    || claim.last_heartbeat_at() != updated_at
                    || updated_at >= claim.lease_expires_at()
                {
                    return Err(DeviceJobError::InvalidSnapshot);
                }
                Some(claim)
            }
            None => None,
        };

        let version = AggregateVersion::new(aggregate_version)
            .map_err(|_| DeviceJobError::InvalidSnapshot)?;
        Ok(Self {
            job_id,
            target,
            version,
            status,
            attempt,
            max_attempts,
            last_fence,
            active_claim,
            retry_at,
            updated_at,
        })
    }

    pub fn issue(
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        max_attempts: u32,
        issued_at: UnixMillis,
    ) -> Result<Self, DeviceJobError> {
        if max_attempts == 0 || max_attempts > MAX_DEVICE_JOB_ATTEMPTS {
            return Err(DeviceJobError::InvalidMaxAttempts);
        }
        Ok(Self {
            job_id,
            target,
            version: AggregateVersion::INITIAL,
            status: DeviceJobStatus::PendingDevice,
            attempt: 0,
            max_attempts,
            last_fence: 0,
            active_claim: None,
            retry_at: None,
            updated_at: issued_at,
        })
    }

    #[must_use]
    pub const fn job_id(&self) -> &DeviceJobId {
        &self.job_id
    }

    #[must_use]
    pub const fn target(&self) -> &DeviceJobTarget {
        &self.target
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn status(&self) -> DeviceJobStatus {
        self.status
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    #[must_use]
    pub const fn last_fence(&self) -> u64 {
        self.last_fence
    }

    #[must_use]
    pub const fn active_claim(&self) -> Option<&DeviceClaim> {
        self.active_claim.as_ref()
    }

    #[must_use]
    pub const fn retry_at(&self) -> Option<UnixMillis> {
        self.retry_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> UnixMillis {
        self.updated_at
    }

    pub fn claim(
        &mut self,
        claim_id: DeviceClaimId,
        observed_at: UnixMillis,
        lease_expires_at: UnixMillis,
    ) -> Result<DeviceClaim, DeviceJobError> {
        self.ensure_time(observed_at)?;
        if self.status.is_terminal()
            || matches!(
                self.status,
                DeviceJobStatus::Running | DeviceJobStatus::AuthRequired
            )
        {
            return Err(DeviceJobError::InvalidState);
        }
        if self.status == DeviceJobStatus::RecoveryRequired {
            return Err(DeviceJobError::RecoveryRequired);
        }
        if self.retry_at.is_some_and(|retry_at| observed_at < retry_at) {
            return Err(DeviceJobError::NotDue);
        }
        if self.active_claim.is_some() {
            return Err(DeviceJobError::ClaimAlreadyActive);
        }
        if self.attempt >= self.max_attempts {
            return Err(DeviceJobError::AttemptsExhausted);
        }

        let fence = self
            .last_fence
            .checked_add(1)
            .ok_or(DeviceJobError::FenceOverflow)?;
        let claim = DeviceClaim::issue(
            claim_id,
            self.job_id.clone(),
            self.target.clone(),
            fence,
            observed_at,
            lease_expires_at,
        )
        .map_err(|_| DeviceJobError::InvalidLease)?;
        self.attempt = self
            .attempt
            .checked_add(1)
            .ok_or(DeviceJobError::AttemptOverflow)?;
        self.last_fence = fence;
        self.active_claim = Some(claim.clone());
        self.retry_at = None;
        self.status = DeviceJobStatus::Running;
        self.advance(observed_at)?;
        Ok(claim)
    }

    pub fn heartbeat(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
        lease_expires_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        self.ensure_time(observed_at)?;
        let claim = self.current_claim_mut(claim_id, fence, observed_at)?;
        claim
            .heartbeat(observed_at, lease_expires_at)
            .map_err(|_| DeviceJobError::InvalidLease)?;
        self.advance(observed_at)
    }

    pub fn mark_profile_busy(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
        retry_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        if retry_at <= observed_at {
            return Err(DeviceJobError::InvalidRetryAt);
        }
        self.release_running_claim(claim_id, fence, observed_at)?;
        self.status = DeviceJobStatus::ProfileBusy;
        self.retry_at = Some(retry_at);
        self.advance(observed_at)
    }

    pub fn schedule_retry(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
        retry_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        if self.attempt >= self.max_attempts {
            return Err(DeviceJobError::AttemptsExhausted);
        }
        if retry_at <= observed_at {
            return Err(DeviceJobError::InvalidRetryAt);
        }
        self.release_running_claim(claim_id, fence, observed_at)?;
        self.status = DeviceJobStatus::RetryScheduled;
        self.retry_at = Some(retry_at);
        self.advance(observed_at)
    }

    pub fn expire_claim(
        &mut self,
        observed_at: UnixMillis,
        retry_at: UnixMillis,
    ) -> Result<DeviceJobStatus, DeviceJobError> {
        self.ensure_time(observed_at)?;
        if self.status != DeviceJobStatus::Running {
            return Err(DeviceJobError::InvalidState);
        }
        let claim = self
            .active_claim
            .as_ref()
            .ok_or(DeviceJobError::MissingActiveClaim)?;
        if !claim.is_expired(observed_at) {
            return Err(DeviceJobError::LeaseStillActive);
        }
        let next_status = if self.attempt >= self.max_attempts {
            DeviceJobStatus::Failed
        } else {
            if retry_at <= observed_at {
                return Err(DeviceJobError::InvalidRetryAt);
            }
            DeviceJobStatus::RetryScheduled
        };
        self.active_claim = None;
        self.status = next_status;
        self.retry_at = (next_status == DeviceJobStatus::RetryScheduled).then_some(retry_at);
        self.advance(observed_at)?;
        Ok(next_status)
    }

    pub fn require_auth(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        self.release_running_claim(claim_id, fence, observed_at)?;
        self.status = DeviceJobStatus::AuthRequired;
        self.retry_at = None;
        self.advance(observed_at)
    }

    pub fn resume_after_auth(&mut self, observed_at: UnixMillis) -> Result<(), DeviceJobError> {
        self.ensure_time(observed_at)?;
        if self.status != DeviceJobStatus::AuthRequired || self.active_claim.is_some() {
            return Err(DeviceJobError::InvalidState);
        }
        self.status = DeviceJobStatus::PendingDevice;
        self.advance(observed_at)
    }

    pub fn require_recovery(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        self.release_running_claim(claim_id, fence, observed_at)?;
        self.status = DeviceJobStatus::RecoveryRequired;
        self.retry_at = None;
        self.advance(observed_at)
    }

    pub fn resume_after_recovery(&mut self, observed_at: UnixMillis) -> Result<(), DeviceJobError> {
        self.ensure_time(observed_at)?;
        if self.status != DeviceJobStatus::RecoveryRequired || self.active_claim.is_some() {
            return Err(DeviceJobError::InvalidState);
        }
        self.status = DeviceJobStatus::PendingDevice;
        self.advance(observed_at)
    }

    pub fn succeed(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        self.release_running_claim(claim_id, fence, observed_at)?;
        self.status = DeviceJobStatus::Succeeded;
        self.retry_at = None;
        self.advance(observed_at)
    }

    pub fn fail(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        self.release_running_claim(claim_id, fence, observed_at)?;
        self.status = DeviceJobStatus::Failed;
        self.retry_at = None;
        self.advance(observed_at)
    }

    pub fn cancel(&mut self, observed_at: UnixMillis) -> Result<(), DeviceJobError> {
        self.ensure_time(observed_at)?;
        if self.status.is_terminal() {
            return Err(DeviceJobError::InvalidState);
        }
        self.active_claim = None;
        self.retry_at = None;
        self.status = DeviceJobStatus::Cancelled;
        self.advance(observed_at)
    }

    fn current_claim_mut(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
    ) -> Result<&mut DeviceClaim, DeviceJobError> {
        if self.status != DeviceJobStatus::Running {
            return Err(DeviceJobError::InvalidState);
        }
        let claim = self
            .active_claim
            .as_mut()
            .ok_or(DeviceJobError::MissingActiveClaim)?;
        if claim.claim_id() != claim_id || claim.fence() != fence {
            return Err(DeviceJobError::StaleClaim);
        }
        if claim.is_expired(observed_at) {
            return Err(DeviceJobError::LeaseExpired);
        }
        Ok(claim)
    }

    fn release_running_claim(
        &mut self,
        claim_id: &DeviceClaimId,
        fence: u64,
        observed_at: UnixMillis,
    ) -> Result<(), DeviceJobError> {
        self.ensure_time(observed_at)?;
        self.current_claim_mut(claim_id, fence, observed_at)?;
        self.active_claim = None;
        Ok(())
    }

    fn ensure_time(&self, observed_at: UnixMillis) -> Result<(), DeviceJobError> {
        if observed_at < self.updated_at {
            Err(DeviceJobError::TimeRegression)
        } else {
            Ok(())
        }
    }

    fn advance(&mut self, observed_at: UnixMillis) -> Result<(), DeviceJobError> {
        self.version = self.version.next().map_err(map_version_overflow)?;
        self.updated_at = observed_at;
        Ok(())
    }
}

fn map_version_overflow(_: VersionOverflow) -> DeviceJobError {
    DeviceJobError::VersionOverflow
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceJobError {
    InvalidSnapshot,
    InvalidMaxAttempts,
    InvalidState,
    InvalidLease,
    InvalidRetryAt,
    NotDue,
    RecoveryRequired,
    ClaimAlreadyActive,
    MissingActiveClaim,
    StaleClaim,
    LeaseExpired,
    LeaseStillActive,
    AttemptsExhausted,
    AttemptOverflow,
    FenceOverflow,
    VersionOverflow,
    TimeRegression,
}

impl fmt::Display for DeviceJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSnapshot => "device job persisted snapshot is invalid",
            Self::InvalidMaxAttempts => "device job max attempts are outside the bounded range",
            Self::InvalidState => "device job transition is invalid for the current state",
            Self::InvalidLease => "device job claim lease is invalid",
            Self::InvalidRetryAt => "device job retry time must be in the future",
            Self::NotDue => "device job retry is not due yet",
            Self::RecoveryRequired => "device job requires explicit recovery before another claim",
            Self::ClaimAlreadyActive => "device job already has an active claim",
            Self::MissingActiveClaim => "device job has no active claim",
            Self::StaleClaim => "device job claim or fencing evidence is stale",
            Self::LeaseExpired => "device job claim lease expired",
            Self::LeaseStillActive => "device job claim lease is still active",
            Self::AttemptsExhausted => "device job attempts are exhausted",
            Self::AttemptOverflow => "device job attempt counter overflow",
            Self::FenceOverflow => "device job fencing counter overflow",
            Self::VersionOverflow => "device job aggregate version overflow",
            Self::TimeRegression => "device job transition time regressed",
        })
    }
}

impl std::error::Error for DeviceJobError {}

#[cfg(test)]
mod tests {
    use super::{DeviceJob, DeviceJobError, DeviceJobStatus};
    use crate::{
        id::{DeviceClaimId, DeviceJobId},
        target::DeviceJobTarget,
    };
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId, UnixMillis};

    fn target() -> Result<DeviceJobTarget, Box<dyn std::error::Error>> {
        Ok(DeviceJobTarget::new(
            TenantId::parse("tenant_01JDEVICE")?,
            DeviceId::parse("device_01JDEVICE")?,
            ProfileId::parse("profile_01JDEVICE")?,
            GenerationId::parse("generation_01JDEVICE")?,
        ))
    }

    fn job(max_attempts: u32) -> Result<DeviceJob, Box<dyn std::error::Error>> {
        Ok(DeviceJob::issue(
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            max_attempts,
            UnixMillis::new(100),
        )?)
    }

    #[test]
    fn claim_turnover_advances_fence_and_rejects_stale_result()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job(3)?;
        let first_id = DeviceClaimId::parse("devclaim_01JDEVICE1")?;
        let first = job.claim(first_id.clone(), UnixMillis::new(110), UnixMillis::new(200))?;
        job.mark_profile_busy(
            &first_id,
            first.fence(),
            UnixMillis::new(120),
            UnixMillis::new(130),
        )?;
        let second_id = DeviceClaimId::parse("devclaim_01JDEVICE2")?;
        let second = job.claim(second_id, UnixMillis::new(130), UnixMillis::new(220))?;
        assert!(second.fence() > first.fence());
        assert_eq!(
            job.succeed(&first_id, first.fence(), UnixMillis::new(140)),
            Err(DeviceJobError::StaleClaim)
        );
        assert_eq!(job.status(), DeviceJobStatus::Running);
        Ok(())
    }

    #[test]
    fn expired_claim_cannot_commit_success() -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job(2)?;
        let claim_id = DeviceClaimId::parse("devclaim_01JDEVICE")?;
        let claim = job.claim(claim_id.clone(), UnixMillis::new(110), UnixMillis::new(120))?;
        assert_eq!(
            job.succeed(&claim_id, claim.fence(), UnixMillis::new(120)),
            Err(DeviceJobError::LeaseExpired)
        );
        assert_eq!(job.status(), DeviceJobStatus::Running);
        Ok(())
    }

    #[test]
    fn expired_claim_retries_or_fails_but_never_succeeds() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut retrying = job(2)?;
        retrying.claim(
            DeviceClaimId::parse("devclaim_01JDEVICE1")?,
            UnixMillis::new(110),
            UnixMillis::new(120),
        )?;
        assert_eq!(
            retrying.expire_claim(UnixMillis::new(120), UnixMillis::new(150))?,
            DeviceJobStatus::RetryScheduled
        );

        let mut exhausted = job(1)?;
        exhausted.claim(
            DeviceClaimId::parse("devclaim_01JDEVICE2")?,
            UnixMillis::new(110),
            UnixMillis::new(120),
        )?;
        assert_eq!(
            exhausted.expire_claim(UnixMillis::new(120), UnixMillis::new(150))?,
            DeviceJobStatus::Failed
        );
        assert!(exhausted.status().is_terminal());
        Ok(())
    }

    #[test]
    fn retry_auth_and_recovery_states_never_become_false_success()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job(3)?;
        let claim_id = DeviceClaimId::parse("devclaim_01JDEVICE1")?;
        let claim = job.claim(claim_id.clone(), UnixMillis::new(110), UnixMillis::new(200))?;
        job.schedule_retry(
            &claim_id,
            claim.fence(),
            UnixMillis::new(120),
            UnixMillis::new(150),
        )?;
        assert_eq!(job.status(), DeviceJobStatus::RetryScheduled);
        assert_eq!(
            job.claim(
                DeviceClaimId::parse("devclaim_01JDEVICE2")?,
                UnixMillis::new(149),
                UnixMillis::new(240),
            ),
            Err(DeviceJobError::NotDue)
        );
        let second_id = DeviceClaimId::parse("devclaim_01JDEVICE2")?;
        let second = job.claim(
            second_id.clone(),
            UnixMillis::new(150),
            UnixMillis::new(240),
        )?;
        job.require_auth(&second_id, second.fence(), UnixMillis::new(160))?;
        assert_eq!(job.status(), DeviceJobStatus::AuthRequired);
        job.resume_after_auth(UnixMillis::new(170))?;
        let third_id = DeviceClaimId::parse("devclaim_01JDEVICE3")?;
        let third = job.claim(third_id.clone(), UnixMillis::new(180), UnixMillis::new(260))?;
        job.require_recovery(&third_id, third.fence(), UnixMillis::new(190))?;
        assert_eq!(job.status(), DeviceJobStatus::RecoveryRequired);
        assert!(
            job.claim(
                DeviceClaimId::parse("devclaim_01JDEVICE4")?,
                UnixMillis::new(200),
                UnixMillis::new(280),
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn invalid_retry_does_not_partially_release_running_claim()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job(2)?;
        let claim_id = DeviceClaimId::parse("devclaim_01JDEVICE")?;
        let claim = job.claim(claim_id.clone(), UnixMillis::new(110), UnixMillis::new(200))?;
        assert_eq!(
            job.mark_profile_busy(
                &claim_id,
                claim.fence(),
                UnixMillis::new(120),
                UnixMillis::new(120),
            ),
            Err(DeviceJobError::InvalidRetryAt)
        );
        assert_eq!(job.status(), DeviceJobStatus::Running);
        assert!(job.active_claim().is_some());
        Ok(())
    }

    #[test]
    fn terminal_state_is_sticky() -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job(1)?;
        job.cancel(UnixMillis::new(110))?;
        assert_eq!(job.status(), DeviceJobStatus::Cancelled);
        assert!(job.status().is_terminal());
        assert_eq!(
            job.cancel(UnixMillis::new(120)),
            Err(DeviceJobError::InvalidState)
        );
        Ok(())
    }

    #[test]
    fn snapshot_round_trip_preserves_running_claim_binding()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut original = job(3)?;
        original.claim(
            DeviceClaimId::parse("devclaim_01JRESTORE")?,
            UnixMillis::new(110),
            UnixMillis::new(200),
        )?;
        let restored = DeviceJob::restore(original.snapshot())?;
        assert_eq!(restored, original);
        Ok(())
    }

    #[test]
    fn restore_rejects_impossible_status_claim_retry_and_version_combinations()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut running = job(3)?;
        running.claim(
            DeviceClaimId::parse("devclaim_01JRESTORE")?,
            UnixMillis::new(110),
            UnixMillis::new(200),
        )?;
        let valid = running.snapshot();

        let mut missing_claim = valid.clone();
        missing_claim.active_claim = None;
        assert_eq!(
            DeviceJob::restore(missing_claim),
            Err(DeviceJobError::InvalidSnapshot)
        );

        let mut stale_fence = valid.clone();
        stale_fence.last_fence = stale_fence
            .last_fence
            .checked_add(1)
            .ok_or(DeviceJobError::FenceOverflow)?;
        assert_eq!(
            DeviceJob::restore(stale_fence),
            Err(DeviceJobError::InvalidSnapshot)
        );

        let mut zero_version = valid.clone();
        zero_version.aggregate_version = 0;
        assert_eq!(
            DeviceJob::restore(zero_version),
            Err(DeviceJobError::InvalidSnapshot)
        );

        let mut impossible_version = valid.clone();
        impossible_version.aggregate_version = u64::from(impossible_version.attempt);
        assert_eq!(
            DeviceJob::restore(impossible_version),
            Err(DeviceJobError::InvalidSnapshot)
        );

        let mut too_many_attempts = valid.clone();
        too_many_attempts.attempt = too_many_attempts
            .max_attempts
            .checked_add(1)
            .ok_or(DeviceJobError::AttemptOverflow)?;
        too_many_attempts.last_fence = u64::from(too_many_attempts.attempt);
        assert_eq!(
            DeviceJob::restore(too_many_attempts),
            Err(DeviceJobError::InvalidSnapshot)
        );

        let mut retry_without_time = valid;
        retry_without_time.status = DeviceJobStatus::RetryScheduled;
        retry_without_time.active_claim = None;
        retry_without_time.retry_at = None;
        assert_eq!(
            DeviceJob::restore(retry_without_time),
            Err(DeviceJobError::InvalidSnapshot)
        );
        Ok(())
    }

    #[test]
    fn restore_rejects_claim_binding_and_timeline_corruption()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut running = job(3)?;
        running.claim(
            DeviceClaimId::parse("devclaim_01JRESTORE")?,
            UnixMillis::new(110),
            UnixMillis::new(200),
        )?;
        let valid = running.snapshot();

        let mut wrong_target = valid.clone();
        if let Some(claim) = wrong_target.active_claim.as_mut() {
            claim.target = DeviceJobTarget::new(
                TenantId::parse("tenant_01JOTHER")?,
                DeviceId::parse("device_01JOTHER")?,
                ProfileId::parse("profile_01JOTHER")?,
                GenerationId::parse("generation_01JOTHER")?,
            );
        }
        assert_eq!(
            DeviceJob::restore(wrong_target),
            Err(DeviceJobError::InvalidSnapshot)
        );

        let mut future_heartbeat = valid.clone();
        if let Some(claim) = future_heartbeat.active_claim.as_mut() {
            claim.last_heartbeat_at = UnixMillis::new(111);
        }
        assert_eq!(
            DeviceJob::restore(future_heartbeat),
            Err(DeviceJobError::InvalidSnapshot)
        );

        let mut expired_at_update = valid;
        expired_at_update.updated_at = UnixMillis::new(200);
        assert_eq!(
            DeviceJob::restore(expired_at_update),
            Err(DeviceJobError::InvalidSnapshot)
        );
        Ok(())
    }
}
