use crate::error::NotificationOperationError;
use application_ports::{NotificationRetentionOutcome, NotificationRetentionRepositoryPort};
use profile_platform_primitives::UnixMillis;

const MIN_RETENTION_MS: u64 = 86_400_000;
const MAX_RETENTION_MS: u64 = 31_536_000_000;
pub const MAX_RETENTION_BATCH: u32 = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationRetentionPolicy {
    ttl_ms: u64,
    batch_limit: u32,
}

impl NotificationRetentionPolicy {
    pub const fn new(ttl_ms: u64, batch_limit: u32) -> Result<Self, NotificationOperationError> {
        if ttl_ms < MIN_RETENTION_MS
            || ttl_ms > MAX_RETENTION_MS
            || batch_limit == 0
            || batch_limit > MAX_RETENTION_BATCH
        {
            return Err(NotificationOperationError::InvalidInput);
        }
        Ok(Self {
            ttl_ms,
            batch_limit,
        })
    }

    #[must_use]
    pub const fn ttl_ms(self) -> u64 {
        self.ttl_ms
    }

    #[must_use]
    pub const fn batch_limit(self) -> u32 {
        self.batch_limit
    }
}

pub async fn compact_notification_state<R>(
    repository: &R,
    now: UnixMillis,
    policy: NotificationRetentionPolicy,
) -> Result<NotificationRetentionOutcome, NotificationOperationError>
where
    R: NotificationRetentionRepositoryPort,
{
    let Some(before) = now.value().checked_sub(policy.ttl_ms()) else {
        return Ok(NotificationRetentionOutcome::new(0, 0, 0));
    };
    repository
        .compact_operational_state(UnixMillis::new(before), policy.batch_limit())
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_RETENTION_BATCH, MAX_RETENTION_MS, MIN_RETENTION_MS, NotificationRetentionPolicy,
    };

    #[test]
    fn retention_policy_is_positive_bounded_and_never_unlimited() {
        assert!(NotificationRetentionPolicy::new(MIN_RETENTION_MS - 1, 1).is_err());
        assert!(NotificationRetentionPolicy::new(MIN_RETENTION_MS, 0).is_err());
        assert!(NotificationRetentionPolicy::new(MIN_RETENTION_MS, 1).is_ok());
        assert!(NotificationRetentionPolicy::new(MAX_RETENTION_MS, MAX_RETENTION_BATCH).is_ok());
        assert!(NotificationRetentionPolicy::new(MAX_RETENTION_MS + 1, 1).is_err());
        assert!(
            NotificationRetentionPolicy::new(MIN_RETENTION_MS, MAX_RETENTION_BATCH + 1).is_err()
        );
    }
}
