use application_ports::ConsumerIdempotencyPort;
use contracts::{IntegrationEventEnvelope, is_foundation_event_type};
use profile_platform_primitives::{OpaqueId, UnixMillis};

use crate::integration_events::{
    ConsumerDeliveryOutcome, IntegrationEventOperationError, accept_delivery_once,
};

pub async fn accept_foundation_delivery_once<C>(
    consumer: &C,
    consumer_id: &OpaqueId,
    event: &IntegrationEventEnvelope,
    consumed_at: UnixMillis,
) -> Result<ConsumerDeliveryOutcome, IntegrationEventOperationError>
where
    C: ConsumerIdempotencyPort,
{
    if !is_foundation_event_type(event.event_type()) {
        return Err(IntegrationEventOperationError::InvalidRequest);
    }
    accept_delivery_once(consumer, consumer_id, event, consumed_at).await
}

#[cfg(test)]
mod tests {
    use super::accept_foundation_delivery_once;
    use application_ports::{
        ConsumerClaim, ConsumerIdempotencyPort, IntegrationEventPortError,
    };
    use contracts::{IntegrationEventEnvelope, IntegrationEventPayload};
    use profile_platform_primitives::{
        AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
    };
    use std::cell::Cell;

    struct ClaimCounter {
        calls: Cell<u32>,
    }

    impl ConsumerIdempotencyPort for ClaimCounter {
        async fn claim(
            &self,
            _consumer_id: &OpaqueId,
            _event: &IntegrationEventEnvelope,
            _consumed_at: UnixMillis,
        ) -> Result<ConsumerClaim, IntegrationEventPortError> {
            self.calls.set(self.calls.get() + 1);
            Ok(ConsumerClaim::Claimed)
        }
    }

    fn event(event_type: &str) -> Result<IntegrationEventEnvelope, Box<dyn std::error::Error>> {
        Ok(IntegrationEventEnvelope::new(
            OutboxEventId::parse("outbox_01JEVENT")?,
            TenantId::parse("tenant_01JEVENT")?,
            "client",
            OpaqueId::parse("client_01JEVENT")?,
            AggregateVersion::INITIAL,
            event_type,
            1,
            IntegrationEventPayload::empty(),
            UnixMillis::new(1),
        )?)
    }

    #[test]
    fn unknown_event_is_rejected_before_durable_claim() -> Result<(), Box<dyn std::error::Error>> {
        let consumer = ClaimCounter {
            calls: Cell::new(0),
        };
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let result = block_on(accept_foundation_delivery_once(
            &consumer,
            &consumer_id,
            &event("unknown.event.v1")?,
            UnixMillis::new(2),
        ));
        assert!(result.is_err());
        assert_eq!(consumer.calls.get(), 0);
        Ok(())
    }

    #[test]
    fn known_event_reaches_durable_claim() -> Result<(), Box<dyn std::error::Error>> {
        let consumer = ClaimCounter {
            calls: Cell::new(0),
        };
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let result = block_on(accept_foundation_delivery_once(
            &consumer,
            &consumer_id,
            &event("client.created.v1")?,
            UnixMillis::new(2),
        ));
        assert!(result.is_ok());
        assert_eq!(consumer.calls.get(), 1);
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
}
