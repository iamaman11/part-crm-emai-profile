use core::fmt;
use profile_platform_primitives::UnixMillis;

use crate::{
    id::{DeviceClaimId, DeviceJobId},
    target::DeviceJobTarget,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceClaim {
    claim_id: DeviceClaimId,
    job_id: DeviceJobId,
    target: DeviceJobTarget,
    fence: u64,
    claimed_at: UnixMillis,
    last_heartbeat_at: UnixMillis,
    lease_expires_at: UnixMillis,
}

/// Bounded metadata persisted by outer adapters. Construction is intentionally
/// untrusted until passed through [`DeviceClaim::restore`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceClaimSnapshot {
    pub claim_id: DeviceClaimId,
    pub job_id: DeviceJobId,
    pub target: DeviceJobTarget,
    pub fence: u64,
    pub claimed_at: UnixMillis,
    pub last_heartbeat_at: UnixMillis,
    pub lease_expires_at: UnixMillis,
}

impl DeviceClaim {
    #[must_use]
    pub fn snapshot(&self) -> DeviceClaimSnapshot {
        DeviceClaimSnapshot {
            claim_id: self.claim_id.clone(),
            job_id: self.job_id.clone(),
            target: self.target.clone(),
            fence: self.fence,
            claimed_at: self.claimed_at,
            last_heartbeat_at: self.last_heartbeat_at,
            lease_expires_at: self.lease_expires_at,
        }
    }

    pub fn restore(snapshot: DeviceClaimSnapshot) -> Result<Self, DeviceClaimError> {
        if snapshot.fence == 0 {
            return Err(DeviceClaimError::InvalidLease);
        }
        if snapshot.claimed_at > snapshot.last_heartbeat_at
            || snapshot.last_heartbeat_at >= snapshot.lease_expires_at
        {
            return Err(DeviceClaimError::InvalidTimeline);
        }
        Ok(Self {
            claim_id: snapshot.claim_id,
            job_id: snapshot.job_id,
            target: snapshot.target,
            fence: snapshot.fence,
            claimed_at: snapshot.claimed_at,
            last_heartbeat_at: snapshot.last_heartbeat_at,
            lease_expires_at: snapshot.lease_expires_at,
        })
    }

    pub fn issue(
        claim_id: DeviceClaimId,
        job_id: DeviceJobId,
        target: DeviceJobTarget,
        fence: u64,
        claimed_at: UnixMillis,
        lease_expires_at: UnixMillis,
    ) -> Result<Self, DeviceClaimError> {
        if fence == 0 || lease_expires_at <= claimed_at {
            return Err(DeviceClaimError::InvalidLease);
        }
        Ok(Self {
            claim_id,
            job_id,
            target,
            fence,
            claimed_at,
            last_heartbeat_at: claimed_at,
            lease_expires_at,
        })
    }

    #[must_use]
    pub const fn claim_id(&self) -> &DeviceClaimId {
        &self.claim_id
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
    pub const fn fence(&self) -> u64 {
        self.fence
    }

    #[must_use]
    pub const fn claimed_at(&self) -> UnixMillis {
        self.claimed_at
    }

    #[must_use]
    pub const fn last_heartbeat_at(&self) -> UnixMillis {
        self.last_heartbeat_at
    }

    #[must_use]
    pub const fn lease_expires_at(&self) -> UnixMillis {
        self.lease_expires_at
    }

    #[must_use]
    pub fn is_expired(&self, now: UnixMillis) -> bool {
        now >= self.lease_expires_at
    }

    pub fn heartbeat(
        &mut self,
        observed_at: UnixMillis,
        lease_expires_at: UnixMillis,
    ) -> Result<(), DeviceClaimError> {
        if observed_at < self.last_heartbeat_at {
            return Err(DeviceClaimError::TimeRegression);
        }
        if observed_at >= self.lease_expires_at {
            return Err(DeviceClaimError::LeaseExpired);
        }
        if lease_expires_at <= self.lease_expires_at || lease_expires_at <= observed_at {
            return Err(DeviceClaimError::InvalidLeaseExtension);
        }
        self.last_heartbeat_at = observed_at;
        self.lease_expires_at = lease_expires_at;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceClaimError {
    InvalidLease,
    InvalidTimeline,
    InvalidLeaseExtension,
    LeaseExpired,
    TimeRegression,
}

impl fmt::Display for DeviceClaimError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLease => "device claim fence and lease must be strictly valid",
            Self::InvalidTimeline => "device claim persisted timeline is invalid",
            Self::InvalidLeaseExtension => "device claim lease extension must advance expiry",
            Self::LeaseExpired => "device claim lease has expired",
            Self::TimeRegression => "device claim heartbeat time regressed",
        })
    }
}

impl std::error::Error for DeviceClaimError {}

#[cfg(test)]
mod tests {
    use super::{DeviceClaim, DeviceClaimError, DeviceClaimSnapshot};
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

    #[test]
    fn claim_binds_job_target_fence_and_monotonic_lease() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut claim = DeviceClaim::issue(
            DeviceClaimId::parse("devclaim_01JDEVICE")?,
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            1,
            UnixMillis::new(100),
            UnixMillis::new(200),
        )?;
        claim.heartbeat(UnixMillis::new(150), UnixMillis::new(300))?;
        assert_eq!(claim.fence(), 1);
        assert_eq!(claim.last_heartbeat_at(), UnixMillis::new(150));
        assert_eq!(claim.lease_expires_at(), UnixMillis::new(300));
        assert!(!claim.is_expired(UnixMillis::new(299)));
        assert!(claim.is_expired(UnixMillis::new(300)));
        Ok(())
    }

    #[test]
    fn heartbeat_fails_closed_after_expiry_or_time_regression()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut claim = DeviceClaim::issue(
            DeviceClaimId::parse("devclaim_01JDEVICE")?,
            DeviceJobId::parse("devjob_01JDEVICE")?,
            target()?,
            1,
            UnixMillis::new(100),
            UnixMillis::new(200),
        )?;
        assert_eq!(
            claim.heartbeat(UnixMillis::new(200), UnixMillis::new(300)),
            Err(DeviceClaimError::LeaseExpired)
        );
        assert_eq!(
            claim.heartbeat(UnixMillis::new(90), UnixMillis::new(300)),
            Err(DeviceClaimError::TimeRegression)
        );
        Ok(())
    }

    #[test]
    fn restore_rejects_corrupt_persisted_claim_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = DeviceClaimSnapshot {
            claim_id: DeviceClaimId::parse("devclaim_01JRESTORE")?,
            job_id: DeviceJobId::parse("devjob_01JRESTORE")?,
            target: target()?,
            fence: 7,
            claimed_at: UnixMillis::new(100),
            last_heartbeat_at: UnixMillis::new(150),
            lease_expires_at: UnixMillis::new(200),
        };
        assert_eq!(DeviceClaim::restore(base.clone())?.snapshot(), base);

        let mut heartbeat_before_claim = base.clone();
        heartbeat_before_claim.last_heartbeat_at = UnixMillis::new(99);
        assert_eq!(
            DeviceClaim::restore(heartbeat_before_claim),
            Err(DeviceClaimError::InvalidTimeline)
        );

        let mut heartbeat_at_expiry = base.clone();
        heartbeat_at_expiry.last_heartbeat_at = UnixMillis::new(200);
        assert_eq!(
            DeviceClaim::restore(heartbeat_at_expiry),
            Err(DeviceClaimError::InvalidTimeline)
        );

        let mut zero_fence = base;
        zero_fence.fence = 0;
        assert_eq!(
            DeviceClaim::restore(zero_fence),
            Err(DeviceClaimError::InvalidLease)
        );
        Ok(())
    }
}
