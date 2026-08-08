use application_ports::{
    NotificationAuthorizationPort, NotificationCapability, NotificationPortError,
    NotificationReplayIntent, NotificationReplayRepositoryPort, PendingNotificationReplay,
    ReplayPreparationOutcome, ReplayReasonClass,
};
use profile_platform_primitives::{
    ActorContext, ActorId, AuditEventId, CorrelationId, OpaqueId, OutboxEventId, TenantId,
    TenantScope, UnixMillis,
};
use std::cell::Cell;
use use_cases_notifications::error::NotificationOperationError;
use use_cases_notifications::replay::prepare_replay;

struct Authorization(bool);

impl NotificationAuthorizationPort for Authorization {
    async fn is_authorized(
        &self,
        _actor: &ActorContext,
        _capability: NotificationCapability,
    ) -> Result<bool, NotificationPortError> {
        Ok(self.0)
    }
}

struct ReplayRepository {
    prepare_calls: Cell<u32>,
}

impl NotificationReplayRepositoryPort for ReplayRepository {
    async fn prepare_replay(
        &self,
        _actor: &ActorContext,
        _intent: &NotificationReplayIntent,
    ) -> Result<ReplayPreparationOutcome, NotificationPortError> {
        self.prepare_calls.set(self.prepare_calls.get() + 1);
        Ok(ReplayPreparationOutcome::Prepared)
    }

    async fn load_pending_replays(
        &self,
        _limit: u32,
    ) -> Result<Vec<PendingNotificationReplay>, NotificationPortError> {
        Ok(Vec::new())
    }

    async fn mark_replay_published(
        &self,
        _tenant_id: &TenantId,
        _replay_id: &OpaqueId,
        _published_at: UnixMillis,
    ) -> Result<(), NotificationPortError> {
        Ok(())
    }
}

fn fixture() -> Result<(ActorContext, NotificationReplayIntent), Box<dyn std::error::Error>> {
    let tenant_id = TenantId::parse("tenant_01JREPLAYAUTH")?;
    let actor = ActorContext::new(
        TenantScope::new(tenant_id),
        ActorId::parse("actor_01JREPLAYAUTH")?,
        CorrelationId::parse("corr_01JREPLAYAUTH")?,
    );
    let intent = NotificationReplayIntent::new(
        OpaqueId::parse("replay_01JREPLAYAUTH")?,
        OpaqueId::parse("consumer_01JREPLAYAUTH")?,
        OutboxEventId::parse("outbox_01JREPLAYAUTH")?,
        AuditEventId::parse("audit_01JREPLAYAUTH")?,
        ReplayReasonClass::OperatorRemediation,
        UnixMillis::new(100),
    );
    Ok((actor, intent))
}

#[test]
fn forbidden_actor_never_reaches_replay_repository() -> Result<(), Box<dyn std::error::Error>> {
    let (actor, intent) = fixture()?;
    let repository = ReplayRepository {
        prepare_calls: Cell::new(0),
    };
    let result = block_on(prepare_replay(
        &Authorization(false),
        &repository,
        &actor,
        &intent,
    ));
    assert_eq!(result, Err(NotificationOperationError::Forbidden));
    assert_eq!(repository.prepare_calls.get(), 0);
    Ok(())
}

#[test]
fn authorized_actor_prepares_exactly_once() -> Result<(), Box<dyn std::error::Error>> {
    let (actor, intent) = fixture()?;
    let repository = ReplayRepository {
        prepare_calls: Cell::new(0),
    };
    let result = block_on(prepare_replay(
        &Authorization(true),
        &repository,
        &actor,
        &intent,
    ));
    assert_eq!(result, Ok(ReplayPreparationOutcome::Prepared));
    assert_eq!(repository.prepare_calls.get(), 1);
    Ok(())
}

fn block_on<F: core::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Waker};
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::hint::spin_loop(),
        }
    }
}
