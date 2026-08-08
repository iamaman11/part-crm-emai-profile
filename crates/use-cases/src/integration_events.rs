use application_ports::{
    ConsumerClaim, ConsumerIdempotencyPort, IntegrationEventOutboxPort, IntegrationEventPortError,
    IntegrationEventPortErrorClass, IntegrationEventPublisherPort,
};
use contracts::IntegrationEventEnvelope;
use core::fmt;
use profile_platform_primitives::{OpaqueId, UnixMillis};

const MAX_DISPATCH_BATCH: u32 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchSummary {
    published: u32,
}

impl DispatchSummary {
    #[must_use]
    pub const fn published(self) -> u32 {
        self.published
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumerDeliveryOutcome {
    Accepted,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationEventOperationError {
    InvalidRequest,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for IntegrationEventOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "integration event request is invalid",
            Self::Conflict => "integration event operation conflict",
            Self::IntegrityFailure => "integration event integrity failure",
            Self::InternalFailure => "integration event internal failure",
            Self::DependencyUnavailable => "integration event dependency unavailable",
        })
    }
}

impl std::error::Error for IntegrationEventOperationError {}

pub async fn dispatch_pending_events<O, P>(
    outbox: &O,
    publisher: &P,
    published_at: UnixMillis,
    limit: u32,
) -> Result<DispatchSummary, IntegrationEventOperationError>
where
    O: IntegrationEventOutboxPort,
    P: IntegrationEventPublisherPort,
{
    if limit == 0 || limit > MAX_DISPATCH_BATCH {
        return Err(IntegrationEventOperationError::InvalidRequest);
    }

    let events = outbox.load_pending(limit).await.map_err(map_port_error)?;
    let mut published = 0_u32;
    for event in events {
        publisher.publish(&event).await.map_err(map_port_error)?;
        outbox
            .mark_published(event.tenant_id(), event.event_id(), published_at)
            .await
            .map_err(map_port_error)?;
        published = published
            .checked_add(1)
            .ok_or(IntegrationEventOperationError::InternalFailure)?;
    }

    Ok(DispatchSummary { published })
}

pub async fn accept_delivery_once<C>(
    consumer: &C,
    consumer_id: &OpaqueId,
    event: &IntegrationEventEnvelope,
    consumed_at: UnixMillis,
) -> Result<ConsumerDeliveryOutcome, IntegrationEventOperationError>
where
    C: ConsumerIdempotencyPort,
{
    match consumer
        .claim(
            event.tenant_id(),
            consumer_id,
            event.event_id(),
            consumed_at,
        )
        .await
        .map_err(map_port_error)?
    {
        ConsumerClaim::Claimed => Ok(ConsumerDeliveryOutcome::Accepted),
        ConsumerClaim::Duplicate => Ok(ConsumerDeliveryOutcome::Duplicate),
    }
}

fn map_port_error(error: IntegrationEventPortError) -> IntegrationEventOperationError {
    match error.class() {
        IntegrationEventPortErrorClass::Conflict => IntegrationEventOperationError::Conflict,
        IntegrationEventPortErrorClass::IntegrityFailure => {
            IntegrationEventOperationError::IntegrityFailure
        }
        IntegrationEventPortErrorClass::InternalFailure => {
            IntegrationEventOperationError::InternalFailure
        }
        IntegrationEventPortErrorClass::DependencyUnavailable => {
            IntegrationEventOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConsumerDeliveryOutcome, IntegrationEventOperationError, accept_delivery_once,
        dispatch_pending_events,
    };
    use application_ports::{
        ConsumerClaim, ConsumerIdempotencyPort, IntegrationEventOutboxPort,
        IntegrationEventPortError, IntegrationEventPortErrorClass, IntegrationEventPublisherPort,
    };
    use contracts::{IntegrationEventEnvelope, IntegrationEventPayload};
    use profile_platform_primitives::{
        AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
    };
    use std::cell::{Cell, RefCell};

    fn event(suffix: &str) -> Result<IntegrationEventEnvelope, Box<dyn std::error::Error>> {
        Ok(IntegrationEventEnvelope::new(
            OutboxEventId::parse(format!("outbox_01J{suffix}"))?,
            TenantId::parse("tenant_01JEVENT")?,
            "client",
            OpaqueId::parse(format!("client_01J{suffix}"))?,
            AggregateVersion::INITIAL,
            "client.created.v1",
            1,
            IntegrationEventPayload::empty(),
            UnixMillis::new(10),
        )?)
    }

    struct FakeOutbox {
        pending: RefCell<Vec<IntegrationEventEnvelope>>,
        published: RefCell<Vec<String>>,
    }

    impl FakeOutbox {
        fn new(events: Vec<IntegrationEventEnvelope>) -> Self {
            Self {
                pending: RefCell::new(events),
                published: RefCell::new(Vec::new()),
            }
        }
    }

    impl IntegrationEventOutboxPort for FakeOutbox {
        async fn load_pending(
            &self,
            limit: u32,
        ) -> Result<Vec<IntegrationEventEnvelope>, IntegrationEventPortError> {
            let limit = usize::try_from(limit).map_err(|_| {
                IntegrationEventPortError::new(IntegrationEventPortErrorClass::InternalFailure)
            })?;
            Ok(self.pending.borrow().iter().take(limit).cloned().collect())
        }

        async fn mark_published(
            &self,
            _tenant_id: &TenantId,
            event_id: &OutboxEventId,
            _published_at: UnixMillis,
        ) -> Result<(), IntegrationEventPortError> {
            self.published
                .borrow_mut()
                .push(event_id.as_str().to_owned());
            Ok(())
        }
    }

    struct FakePublisher {
        published: RefCell<Vec<String>>,
        fail_on_call: Option<u32>,
        calls: Cell<u32>,
    }

    impl FakePublisher {
        fn accepting() -> Self {
            Self {
                published: RefCell::new(Vec::new()),
                fail_on_call: None,
                calls: Cell::new(0),
            }
        }

        fn failing_on(call: u32) -> Self {
            Self {
                published: RefCell::new(Vec::new()),
                fail_on_call: Some(call),
                calls: Cell::new(0),
            }
        }
    }

    impl IntegrationEventPublisherPort for FakePublisher {
        async fn publish(
            &self,
            event: &IntegrationEventEnvelope,
        ) -> Result<(), IntegrationEventPortError> {
            let call = self.calls.get().checked_add(1).ok_or_else(|| {
                IntegrationEventPortError::new(IntegrationEventPortErrorClass::InternalFailure)
            })?;
            self.calls.set(call);
            if self.fail_on_call == Some(call) {
                return Err(IntegrationEventPortError::new(
                    IntegrationEventPortErrorClass::DependencyUnavailable,
                ));
            }
            self.published
                .borrow_mut()
                .push(event.event_id().as_str().to_owned());
            Ok(())
        }
    }

    #[test]
    fn dispatcher_marks_only_successfully_published_events()
    -> Result<(), Box<dyn std::error::Error>> {
        let outbox = FakeOutbox::new(vec![event("EVENT_A")?, event("EVENT_B")?]);
        let publisher = FakePublisher::failing_on(2);
        let result = futures_lite_block_on(dispatch_pending_events(
            &outbox,
            &publisher,
            UnixMillis::new(20),
            10,
        ));
        assert_eq!(
            result,
            Err(IntegrationEventOperationError::DependencyUnavailable)
        );
        assert_eq!(publisher.published.borrow().len(), 1);
        assert_eq!(outbox.published.borrow().len(), 1);
        Ok(())
    }

    #[test]
    fn dispatcher_is_bounded_and_reports_success() -> Result<(), Box<dyn std::error::Error>> {
        let outbox = FakeOutbox::new(vec![event("EVENT_A")?, event("EVENT_B")?]);
        let publisher = FakePublisher::accepting();
        let summary = futures_lite_block_on(dispatch_pending_events(
            &outbox,
            &publisher,
            UnixMillis::new(20),
            10,
        ))?;
        assert_eq!(summary.published(), 2);
        assert_eq!(outbox.published.borrow().len(), 2);
        assert_eq!(
            futures_lite_block_on(dispatch_pending_events(
                &outbox,
                &publisher,
                UnixMillis::new(20),
                0,
            )),
            Err(IntegrationEventOperationError::InvalidRequest)
        );
        Ok(())
    }

    struct FakeConsumer {
        claimed: Cell<bool>,
    }

    impl ConsumerIdempotencyPort for FakeConsumer {
        async fn claim(
            &self,
            _tenant_id: &TenantId,
            _consumer_id: &OpaqueId,
            _event_id: &OutboxEventId,
            _consumed_at: UnixMillis,
        ) -> Result<ConsumerClaim, IntegrationEventPortError> {
            if self.claimed.replace(true) {
                Ok(ConsumerClaim::Duplicate)
            } else {
                Ok(ConsumerClaim::Claimed)
            }
        }
    }

    #[test]
    fn duplicate_consumer_delivery_is_neutral() -> Result<(), Box<dyn std::error::Error>> {
        let consumer = FakeConsumer {
            claimed: Cell::new(false),
        };
        let consumer_id = OpaqueId::parse("consumer_01JEVENT")?;
        let event = event("EVENT_A")?;
        assert_eq!(
            futures_lite_block_on(accept_delivery_once(
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(30),
            ))?,
            ConsumerDeliveryOutcome::Accepted
        );
        assert_eq!(
            futures_lite_block_on(accept_delivery_once(
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(31),
            ))?,
            ConsumerDeliveryOutcome::Duplicate
        );
        Ok(())
    }

    fn futures_lite_block_on<F: core::future::Future>(future: F) -> F::Output {
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
