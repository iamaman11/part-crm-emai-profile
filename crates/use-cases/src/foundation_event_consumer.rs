use application_ports::{ConsumerIdempotencyPort, NotificationEventPort};
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
    C: ConsumerIdempotencyPort + NotificationEventPort,
{
    if !is_foundation_event_type(event.event_type()) {
        return Err(IntegrationEventOperationError::InvalidRequest);
    }
    consumer
        .persist_notification_event(event, consumed_at)
        .await
        .map_err(crate::integration_events::map_port_error)?;
    accept_delivery_once(consumer, consumer_id, event, consumed_at).await
}

#[cfg(test)]
mod tests {
    use super::accept_foundation_delivery_once;
    use application_ports::{
        ConsumerClaim, ConsumerIdempotencyPort, IntegrationEventPortError, NotificationEventPort,
    };
    use contracts::{IntegrationEventEnvelope, IntegrationEventPayload};
    use profile_platform_primitives::{
        AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
    };
    use std::cell::Cell;

    struct ConsumerProbe {
        notification_calls: Cell<u32>,
        claim_calls: Cell<u32>,
    }

    impl NotificationEventPort for ConsumerProbe {
        async fn persist_notification_event(
            &self,
            _event: &IntegrationEventEnvelope,
            _persisted_at: UnixMillis,
        ) -> Result<(), IntegrationEventPortError> {
            self.notification_calls.set(self.notification_calls.get() + 1);
            Ok(())
        }
    }

    impl ConsumerIdempotencyPort for ConsumerProbe {
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
    fn unknown_event_is_rejected_before_notification_or_claim()
    -> Result<(), Box<dyn std::error::Error>> {
        let consumer = ConsumerProbe {
            notification_calls: Cell::new(0),
            claim_calls: Cell::new(0),
        };
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let result = block_on(accept_foundation_delivery_once(
            &consumer,
            &consumer_id,
            &event("unknown.event.v1")?,
            UnixMillis::new(2),
        ));
        assert!(result.is_err());
        assert_eq!(consumer.notification_calls.get(), 0);
        assert_eq!(consumer.claim_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn known_event_persists_notification_before_durable_claim()
    -> Result<(), Box<dyn std::error::Error>> {
        let consumer = ConsumerProbe {
            notification_calls: Cell::new(0),
            claim_calls: Cell::new(0),
        };
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let result = block_on(accept_foundation_delivery_once(
            &consumer,
            &consumer_id,
            &event("client.created.v1")?,
            UnixMillis::new(2),
        ));
        assert!(result.is_ok());
        assert_eq!(consumer.notification_calls.get(), 1);
        assert_eq!(consumer.claim_calls.get(), 1);
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
