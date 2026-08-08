use application_ports::{
    ConsumerIdempotencyPort, DeliveryTransitionWriteOutcome, NotificationDeliveryRepositoryPort,
    NotificationEventPort, NotificationPortError, NotificationPortErrorClass,
};
use contracts::IntegrationEventEnvelope;
use core::fmt;
use notification_domain::{DeliveryFailureClass, DeliveryState, DeliveryTransitionError};
use profile_platform_primitives::{OpaqueId, UnixMillis};

use crate::foundation_event_consumer::accept_foundation_delivery_once;
use crate::integration_events::{ConsumerDeliveryOutcome, IntegrationEventOperationError};
use crate::retry::{RetryPolicy, RetrySchedulingError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryProcessingOutcome {
    Delivered { duplicate: bool },
    RetryScheduled { retry_at: UnixMillis },
    DeadLetter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryProcessingError {
    DomainTransition(DeliveryTransitionError),
    Persistence(NotificationPortErrorClass),
    RetryScheduling(RetrySchedulingError),
    StateConflict,
}

impl fmt::Display for DeliveryProcessingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DomainTransition(_) => "notification delivery state transition failed",
            Self::Persistence(NotificationPortErrorClass::Conflict) => {
                "notification delivery persistence conflict"
            }
            Self::Persistence(NotificationPortErrorClass::IntegrityFailure) => {
                "notification delivery persistence integrity failure"
            }
            Self::Persistence(NotificationPortErrorClass::InternalFailure) => {
                "notification delivery persistence internal failure"
            }
            Self::Persistence(NotificationPortErrorClass::DependencyUnavailable) => {
                "notification delivery persistence dependency unavailable"
            }
            Self::RetryScheduling(_) => "notification delivery retry scheduling failed",
            Self::StateConflict => "notification delivery state conflict",
        })
    }
}

impl std::error::Error for DeliveryProcessingError {}

pub async fn process_foundation_delivery<D, C>(
    deliveries: &D,
    consumer: &C,
    consumer_id: &OpaqueId,
    event: &IntegrationEventEnvelope,
    now: UnixMillis,
    retry_policy: RetryPolicy,
) -> Result<DeliveryProcessingOutcome, DeliveryProcessingError>
where
    D: NotificationDeliveryRepositoryPort,
    C: ConsumerIdempotencyPort + NotificationEventPort,
{
    let state = deliveries
        .load_or_create_delivery(
            event.tenant_id(),
            consumer_id,
            event.event_id(),
            event.occurred_at(),
        )
        .await
        .map_err(map_persistence_error)?;

    match state {
        DeliveryState::Delivered { .. } => {
            return Ok(DeliveryProcessingOutcome::Delivered { duplicate: true });
        }
        DeliveryState::DeadLetter { .. } => return Ok(DeliveryProcessingOutcome::DeadLetter),
        DeliveryState::RetryScheduled {
            next_attempt_at, ..
        } if !state.is_due(now) => {
            return Ok(DeliveryProcessingOutcome::RetryScheduled {
                retry_at: next_attempt_at,
            });
        }
        DeliveryState::Ready { .. } | DeliveryState::RetryScheduled { .. } => {}
    }

    match accept_foundation_delivery_once(consumer, consumer_id, event, now).await {
        Ok(consumer_outcome) => {
            let next = state
                .record_success(now)
                .map_err(DeliveryProcessingError::DomainTransition)?;
            match deliveries
                .compare_and_swap_delivery(
                    event.tenant_id(),
                    consumer_id,
                    event.event_id(),
                    state,
                    next,
                )
                .await
                .map_err(map_persistence_error)?
            {
                DeliveryTransitionWriteOutcome::Applied => {
                    Ok(DeliveryProcessingOutcome::Delivered {
                        duplicate: consumer_outcome == ConsumerDeliveryOutcome::Duplicate,
                    })
                }
                DeliveryTransitionWriteOutcome::Stale => {
                    reconcile_after_stale(deliveries, consumer_id, event, now).await
                }
            }
        }
        Err(error) => {
            let next = retry_policy
                .transition_after_failure(
                    state,
                    now,
                    event.event_id(),
                    failure_class(error),
                )
                .map_err(DeliveryProcessingError::RetryScheduling)?;
            match deliveries
                .compare_and_swap_delivery(
                    event.tenant_id(),
                    consumer_id,
                    event.event_id(),
                    state,
                    next,
                )
                .await
                .map_err(map_persistence_error)?
            {
                DeliveryTransitionWriteOutcome::Applied => outcome_from_state(next),
                DeliveryTransitionWriteOutcome::Stale => {
                    reconcile_after_stale(deliveries, consumer_id, event, now).await
                }
            }
        }
    }
}

async fn reconcile_after_stale<D>(
    deliveries: &D,
    consumer_id: &OpaqueId,
    event: &IntegrationEventEnvelope,
    now: UnixMillis,
) -> Result<DeliveryProcessingOutcome, DeliveryProcessingError>
where
    D: NotificationDeliveryRepositoryPort,
{
    let current = deliveries
        .load_or_create_delivery(
            event.tenant_id(),
            consumer_id,
            event.event_id(),
            event.occurred_at(),
        )
        .await
        .map_err(map_persistence_error)?;
    match current {
        DeliveryState::Delivered { .. } => {
            Ok(DeliveryProcessingOutcome::Delivered { duplicate: true })
        }
        DeliveryState::DeadLetter { .. } => Ok(DeliveryProcessingOutcome::DeadLetter),
        DeliveryState::RetryScheduled {
            next_attempt_at, ..
        } => Ok(DeliveryProcessingOutcome::RetryScheduled {
            retry_at: next_attempt_at,
        }),
        DeliveryState::Ready { .. } if current.is_due(now) => {
            Err(DeliveryProcessingError::StateConflict)
        }
        DeliveryState::Ready { .. } => Err(DeliveryProcessingError::StateConflict),
    }
}

fn outcome_from_state(
    state: DeliveryState,
) -> Result<DeliveryProcessingOutcome, DeliveryProcessingError> {
    match state {
        DeliveryState::RetryScheduled {
            next_attempt_at, ..
        } => Ok(DeliveryProcessingOutcome::RetryScheduled {
            retry_at: next_attempt_at,
        }),
        DeliveryState::DeadLetter { .. } => Ok(DeliveryProcessingOutcome::DeadLetter),
        DeliveryState::Delivered { .. } => {
            Ok(DeliveryProcessingOutcome::Delivered { duplicate: false })
        }
        DeliveryState::Ready { .. } => Err(DeliveryProcessingError::StateConflict),
    }
}

const fn failure_class(error: IntegrationEventOperationError) -> DeliveryFailureClass {
    match error {
        IntegrationEventOperationError::InvalidRequest => DeliveryFailureClass::Rejected,
        IntegrationEventOperationError::Conflict
        | IntegrationEventOperationError::IntegrityFailure => DeliveryFailureClass::IntegrityFailure,
        IntegrationEventOperationError::InternalFailure => DeliveryFailureClass::InternalFailure,
        IntegrationEventOperationError::DependencyUnavailable => {
            DeliveryFailureClass::DependencyUnavailable
        }
    }
}

fn map_persistence_error(error: NotificationPortError) -> DeliveryProcessingError {
    DeliveryProcessingError::Persistence(error.class())
}

#[cfg(test)]
mod tests {
    use super::{DeliveryProcessingOutcome, process_foundation_delivery};
    use application_ports::{
        ConsumerClaim, ConsumerIdempotencyPort, DeliveryTransitionWriteOutcome,
        IntegrationEventPortError, IntegrationEventPortErrorClass,
        NotificationDeliveryRepositoryPort, NotificationEventPort, NotificationPortError,
    };
    use contracts::{IntegrationEventEnvelope, IntegrationEventPayload};
    use notification_domain::{AttemptLimit, DeliveryState};
    use profile_platform_primitives::{
        AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
    };
    use std::cell::{Cell, RefCell};

    fn event() -> Result<IntegrationEventEnvelope, Box<dyn std::error::Error>> {
        Ok(IntegrationEventEnvelope::new(
            OutboxEventId::parse("outbox_01JDELIVERY_A")?,
            TenantId::parse("tenant_01JDELIVERY")?,
            "client",
            OpaqueId::parse("client_01JDELIVERY")?,
            AggregateVersion::INITIAL,
            "client.created.v1",
            1,
            IntegrationEventPayload::empty(),
            UnixMillis::new(10),
        )?)
    }

    fn policy(attempts: u16) -> Result<RetryPolicy, Box<dyn std::error::Error>> {
        Ok(RetryPolicy::new(
            100,
            1_000,
            0,
            AttemptLimit::new(attempts)?,
        )?)
    }

    struct FakeDeliveryRepository {
        state: RefCell<Option<DeliveryState>>,
        stale_replacement: RefCell<Option<DeliveryState>>,
    }

    impl FakeDeliveryRepository {
        fn new() -> Self {
            Self {
                state: RefCell::new(None),
                stale_replacement: RefCell::new(None),
            }
        }

        fn with_stale_replacement(replacement: DeliveryState) -> Self {
            Self {
                state: RefCell::new(None),
                stale_replacement: RefCell::new(Some(replacement)),
            }
        }

        fn state(&self) -> Option<DeliveryState> {
            *self.state.borrow()
        }
    }

    impl NotificationDeliveryRepositoryPort for FakeDeliveryRepository {
        async fn load_or_create_delivery(
            &self,
            _tenant_id: &TenantId,
            _consumer_id: &OpaqueId,
            _event_id: &OutboxEventId,
            _created_at: UnixMillis,
        ) -> Result<DeliveryState, NotificationPortError> {
            let mut state = self.state.borrow_mut();
            if state.is_none() {
                *state = Some(DeliveryState::new());
            }
            state.ok_or_else(|| {
                NotificationPortError::new(
                    application_ports::NotificationPortErrorClass::InternalFailure,
                )
            })
        }

        async fn compare_and_swap_delivery(
            &self,
            _tenant_id: &TenantId,
            _consumer_id: &OpaqueId,
            _event_id: &OutboxEventId,
            expected: DeliveryState,
            next: DeliveryState,
        ) -> Result<DeliveryTransitionWriteOutcome, NotificationPortError> {
            if let Some(replacement) = self.stale_replacement.borrow_mut().take() {
                *self.state.borrow_mut() = Some(replacement);
                return Ok(DeliveryTransitionWriteOutcome::Stale);
            }
            let mut current = self.state.borrow_mut();
            if *current != Some(expected) {
                return Ok(DeliveryTransitionWriteOutcome::Stale);
            }
            *current = Some(next);
            Ok(DeliveryTransitionWriteOutcome::Applied)
        }
    }

    struct ConsumerProbe {
        notification_calls: Cell<u32>,
        claim_calls: Cell<u32>,
        persist_failures_remaining: Cell<u32>,
        always_fail: bool,
        claimed: Cell<bool>,
    }

    impl ConsumerProbe {
        fn accepting() -> Self {
            Self {
                notification_calls: Cell::new(0),
                claim_calls: Cell::new(0),
                persist_failures_remaining: Cell::new(0),
                always_fail: false,
                claimed: Cell::new(false),
            }
        }

        fn fail_persistence(times: u32) -> Self {
            Self {
                notification_calls: Cell::new(0),
                claim_calls: Cell::new(0),
                persist_failures_remaining: Cell::new(times),
                always_fail: false,
                claimed: Cell::new(false),
            }
        }

        fn always_failing() -> Self {
            Self {
                notification_calls: Cell::new(0),
                claim_calls: Cell::new(0),
                persist_failures_remaining: Cell::new(0),
                always_fail: true,
                claimed: Cell::new(false),
            }
        }
    }

    impl NotificationEventPort for ConsumerProbe {
        async fn persist_notification_event(
            &self,
            _event: &IntegrationEventEnvelope,
            _persisted_at: UnixMillis,
        ) -> Result<(), IntegrationEventPortError> {
            self.notification_calls
                .set(self.notification_calls.get().saturating_add(1));
            let remaining = self.persist_failures_remaining.get();
            if self.always_fail || remaining > 0 {
                if remaining > 0 {
                    self.persist_failures_remaining.set(remaining - 1);
                }
                return Err(IntegrationEventPortError::new(
                    IntegrationEventPortErrorClass::DependencyUnavailable,
                ));
            }
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
            self.claim_calls.set(self.claim_calls.get().saturating_add(1));
            if self.claimed.replace(true) {
                Ok(ConsumerClaim::Duplicate)
            } else {
                Ok(ConsumerClaim::Claimed)
            }
        }
    }

    #[test]
    fn successful_duplicate_queue_delivery_is_neutral()
    -> Result<(), Box<dyn std::error::Error>> {
        let repository = FakeDeliveryRepository::new();
        let consumer = ConsumerProbe::accepting();
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let event = event()?;
        let retry_policy = policy(3)?;

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(20),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::Delivered { duplicate: false }
        );
        assert_eq!(consumer.notification_calls.get(), 1);
        assert_eq!(consumer.claim_calls.get(), 1);

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(21),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::Delivered { duplicate: true }
        );
        assert_eq!(consumer.notification_calls.get(), 1);
        assert_eq!(consumer.claim_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn retry_is_deferred_until_due_then_can_succeed() -> Result<(), Box<dyn std::error::Error>> {
        let repository = FakeDeliveryRepository::new();
        let consumer = ConsumerProbe::fail_persistence(1);
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let event = event()?;
        let retry_policy = policy(3)?;

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(20),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::RetryScheduled {
                retry_at: UnixMillis::new(120)
            }
        );
        assert_eq!(consumer.notification_calls.get(), 1);

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(119),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::RetryScheduled {
                retry_at: UnixMillis::new(120)
            }
        );
        assert_eq!(consumer.notification_calls.get(), 1);

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(120),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::Delivered { duplicate: false }
        );
        assert_eq!(consumer.notification_calls.get(), 2);
        assert_eq!(repository.state().map(DeliveryState::attempts).map(|v| v.value()), Some(2));
        Ok(())
    }

    #[test]
    fn max_attempts_reach_dead_letter_and_stop_consumer_calls()
    -> Result<(), Box<dyn std::error::Error>> {
        let repository = FakeDeliveryRepository::new();
        let consumer = ConsumerProbe::always_failing();
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let event = event()?;
        let retry_policy = policy(2)?;

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(20),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::RetryScheduled {
                retry_at: UnixMillis::new(120)
            }
        );
        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(120),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::DeadLetter
        );
        assert_eq!(consumer.notification_calls.get(), 2);

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(10_000),
                retry_policy,
            ))?,
            DeliveryProcessingOutcome::DeadLetter
        );
        assert_eq!(consumer.notification_calls.get(), 2);
        Ok(())
    }

    #[test]
    fn stale_cas_reconciles_to_concurrent_terminal_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let concurrent = DeliveryState::restore_delivered(1, UnixMillis::new(20))?;
        let repository = FakeDeliveryRepository::with_stale_replacement(concurrent);
        let consumer = ConsumerProbe::accepting();
        let consumer_id = OpaqueId::parse("consumer_foundation_v1")?;
        let event = event()?;

        assert_eq!(
            block_on(process_foundation_delivery(
                &repository,
                &consumer,
                &consumer_id,
                &event,
                UnixMillis::new(20),
                policy(3)?,
            ))?,
            DeliveryProcessingOutcome::Delivered { duplicate: true }
        );
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
