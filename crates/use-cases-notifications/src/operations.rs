use crate::error::NotificationOperationError;
use application_ports::{
    NotificationAuthorizationPort, NotificationCapability, NotificationOperationsRepositoryPort,
    NotificationOperationsSnapshot,
};
use profile_platform_primitives::{ActorContext, UnixMillis};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SanitizedNotificationOperations {
    ready_count: u64,
    retry_scheduled_count: u64,
    delivered_count: u64,
    dead_letter_count: u64,
    pending_replay_count: u64,
    max_attempt_count: u16,
    oldest_open_age_ms: Option<u64>,
    catch_up_lag_count: u64,
}

impl SanitizedNotificationOperations {
    #[must_use]
    pub const fn ready_count(self) -> u64 {
        self.ready_count
    }

    #[must_use]
    pub const fn retry_scheduled_count(self) -> u64 {
        self.retry_scheduled_count
    }

    #[must_use]
    pub const fn delivered_count(self) -> u64 {
        self.delivered_count
    }

    #[must_use]
    pub const fn dead_letter_count(self) -> u64 {
        self.dead_letter_count
    }

    #[must_use]
    pub const fn pending_replay_count(self) -> u64 {
        self.pending_replay_count
    }

    #[must_use]
    pub const fn max_attempt_count(self) -> u16 {
        self.max_attempt_count
    }

    #[must_use]
    pub const fn oldest_open_age_ms(self) -> Option<u64> {
        self.oldest_open_age_ms
    }

    #[must_use]
    pub const fn catch_up_lag_count(self) -> u64 {
        self.catch_up_lag_count
    }
}

pub async fn load_operations<A, R>(
    authorization: &A,
    repository: &R,
    actor: &ActorContext,
    now: UnixMillis,
) -> Result<SanitizedNotificationOperations, NotificationOperationError>
where
    A: NotificationAuthorizationPort,
    R: NotificationOperationsRepositoryPort,
{
    if !authorization
        .is_authorized(actor, NotificationCapability::ObserveOperations)
        .await?
    {
        return Err(NotificationOperationError::Forbidden);
    }

    // The repository surface is aggregate-only; authorization deliberately precedes the query so
    // non-owners cannot infer queue depth, terminal counts or catch-up lag.
    let snapshot = repository.load_operations_snapshot(actor).await?;
    map_snapshot(snapshot, now)
}

fn map_snapshot(
    snapshot: NotificationOperationsSnapshot,
    now: UnixMillis,
) -> Result<SanitizedNotificationOperations, NotificationOperationError> {
    let oldest_open_age_ms = snapshot
        .oldest_open_created_at()
        .map(|created_at| {
            now.value()
                .checked_sub(created_at.value())
                .ok_or(NotificationOperationError::IntegrityFailure)
        })
        .transpose()?;

    Ok(SanitizedNotificationOperations {
        ready_count: snapshot.ready_count(),
        retry_scheduled_count: snapshot.retry_scheduled_count(),
        delivered_count: snapshot.delivered_count(),
        dead_letter_count: snapshot.dead_letter_count(),
        pending_replay_count: snapshot.pending_replay_count(),
        max_attempt_count: snapshot.max_attempt_count(),
        oldest_open_age_ms,
        catch_up_lag_count: snapshot.catch_up_lag_count(),
    })
}

#[cfg(test)]
mod tests {
    use super::map_snapshot;
    use application_ports::NotificationOperationsSnapshot;
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn operations_exposes_age_and_counts_only() -> Result<(), Box<dyn std::error::Error>> {
        let view = map_snapshot(
            NotificationOperationsSnapshot::new(
                1,
                2,
                3,
                4,
                5,
                6,
                Some(UnixMillis::new(90)),
                7,
            ),
            UnixMillis::new(100),
        )?;
        assert_eq!(view.ready_count(), 1);
        assert_eq!(view.retry_scheduled_count(), 2);
        assert_eq!(view.delivered_count(), 3);
        assert_eq!(view.dead_letter_count(), 4);
        assert_eq!(view.pending_replay_count(), 5);
        assert_eq!(view.max_attempt_count(), 6);
        assert_eq!(view.oldest_open_age_ms(), Some(10));
        assert_eq!(view.catch_up_lag_count(), 7);
        Ok(())
    }

    #[test]
    fn future_created_at_is_fail_closed() {
        assert!(
            map_snapshot(
                NotificationOperationsSnapshot::new(
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    Some(UnixMillis::new(101)),
                    0,
                ),
                UnixMillis::new(100),
            )
            .is_err()
        );
    }
}
