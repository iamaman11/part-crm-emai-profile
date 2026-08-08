use crate::error::NotificationOperationError;
use application_ports::{
    IntegrationEventPublisherPort, NotificationAuthorizationPort, NotificationCapability,
    NotificationReplayIntent, NotificationReplayRepositoryPort, ReplayPreparationOutcome,
};
use profile_platform_primitives::{ActorContext, UnixMillis};

pub const MAX_REPLAY_DISPATCH_BATCH: u32 = 100;

pub async fn prepare_replay<A, R>(
    authorization: &A,
    replays: &R,
    actor: &ActorContext,
    intent: &NotificationReplayIntent,
) -> Result<ReplayPreparationOutcome, NotificationOperationError>
where
    A: NotificationAuthorizationPort,
    R: NotificationReplayRepositoryPort,
{
    if !authorization
        .is_authorized(actor, NotificationCapability::Remediate)
        .await?
    {
        return Err(NotificationOperationError::Forbidden);
    }

    // The repository command is atomic: immutable audit + replay intent + pending dispatch +
    // DeadLetter -> Ready. Publication happens only after that durable preparation succeeds.
    replays
        .prepare_replay(actor, intent)
        .await
        .map_err(Into::into)
}

pub async fn dispatch_pending_replays<R, P>(
    replays: &R,
    publisher: &P,
    published_at: UnixMillis,
    limit: u32,
) -> Result<u32, NotificationOperationError>
where
    R: NotificationReplayRepositoryPort,
    P: IntegrationEventPublisherPort,
{
    validate_dispatch_limit(limit)?;
    let pending = replays.load_pending_replays(limit).await?;
    if pending.len()
        > usize::try_from(limit).map_err(|_| NotificationOperationError::InvalidInput)?
    {
        return Err(NotificationOperationError::IntegrityFailure);
    }

    let mut published = 0_u32;
    for replay in pending {
        // Re-publish the canonical envelope unchanged. A crash after publish but before the durable
        // mark intentionally causes at-least-once duplicate publication of the same event identity.
        publisher.publish(replay.event()).await?;
        replays
            .mark_replay_published(
                replay.event().tenant_id(),
                replay.replay_id(),
                published_at,
            )
            .await?;
        published = published
            .checked_add(1)
            .ok_or(NotificationOperationError::IntegrityFailure)?;
    }
    Ok(published)
}

fn validate_dispatch_limit(limit: u32) -> Result<(), NotificationOperationError> {
    if limit == 0 || limit > MAX_REPLAY_DISPATCH_BATCH {
        Err(NotificationOperationError::InvalidInput)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_REPLAY_DISPATCH_BATCH, validate_dispatch_limit};

    #[test]
    fn replay_dispatch_batch_is_positive_and_bounded() {
        assert!(validate_dispatch_limit(0).is_err());
        assert!(validate_dispatch_limit(1).is_ok());
        assert!(validate_dispatch_limit(MAX_REPLAY_DISPATCH_BATCH).is_ok());
        assert!(validate_dispatch_limit(MAX_REPLAY_DISPATCH_BATCH + 1).is_err());
    }
}
