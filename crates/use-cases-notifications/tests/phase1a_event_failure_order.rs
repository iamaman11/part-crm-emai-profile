use application_ports::{
    ConsumerClaim, ConsumerIdempotencyPort, IntegrationEventPortError,
    IntegrationEventPortErrorClass, NotificationEventPort,
};
use contracts::{IntegrationEventEnvelope, IntegrationEventPayload};
use profile_platform_primitives::{
    AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
};
use std::cell::Cell;
use use_cases_notifications::foundation_event_consumer::accept_foundation_delivery_once;
use use_cases_notifications::integration_events::IntegrationEventOperationError;

struct FailingNotificationConsumer {
    claim_calls: Cell<u32>,
}

impl NotificationEventPort for FailingNotificationConsumer {
    async fn persist_notification_event(
        &self,
        _event: &IntegrationEventEnvelope,
        _persisted_at: UnixMillis,
    ) -> Result<(), IntegrationEventPortError> {
        Err(IntegrationEventPortError::new(
            IntegrationEventPortErrorClass::DependencyUnavailable,
        ))
    }
}

impl ConsumerIdempotencyPort for FailingNotificationConsumer {
    async fn claim(
        &self,
        _consumer_id: &OpaqueId,
        _event: &IntegrationEventEnvelope,
        _consumed_at: UnixMillis,
    ) -> Result<ConsumerClaim, IntegrationEventPortError> {
        self.claim_calls.set(self.claim_calls.get() + 1);
        Ok(ConsumerClaim::Claimed)
    }
}

fn event() -> Result<IntegrationEventEnvelope, Box<dyn std::error::Error>> {
    Ok(IntegrationEventEnvelope::new(
        OutboxEventId::parse("outbox_01JFAILORDER")?,
        TenantId::parse("tenant_01JFAILORDER")?,
        "client",
        OpaqueId::parse("client_01JFAILORDER")?,
        AggregateVersion::INITIAL,
        "client.created.v1",
        1,
        IntegrationEventPayload::empty(),
        UnixMillis::new(10),
    )?)
}

#[test]
fn notification_failure_stops_before_consumer_claim() -> Result<(), Box<dyn std::error::Error>> {
    let consumer = FailingNotificationConsumer {
        claim_calls: Cell::new(0),
    };
    let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
    let result = block_on(accept_foundation_delivery_once(
        &consumer,
        &consumer_id,
        &event()?,
        UnixMillis::new(20),
    ));
    assert_eq!(
        result,
        Err(IntegrationEventOperationError::DependencyUnavailable)
    );
    assert_eq!(consumer.claim_calls.get(), 0);
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
