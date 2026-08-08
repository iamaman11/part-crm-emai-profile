use cloudflare_adapters::d1_integration_events::D1IntegrationEventRepository;
use cloudflare_adapters::d1_notifications::D1NotificationRepository;
use cloudflare_adapters::integration_event_queue::{
    IntegrationEventQueueMessage, QueueIntegrationEventPublisher,
};
use profile_platform_primitives::{OpaqueId, UnixMillis};
use use_cases_notifications::delivery::{
    DeliveryProcessingOutcome, process_foundation_delivery,
};
use use_cases_notifications::integration_events::{
    IntegrationEventOperationError, dispatch_pending_events,
};
use use_cases_notifications::retry::RetryPolicy;
use worker::{Date, Env, Error, MessageBatch, QueueRetryOptionsBuilder, Result};

pub const INTEGRATION_EVENTS_QUEUE_BINDING: &str = "INTEGRATION_EVENTS";
const FOUNDATION_CONSUMER_ID: &str = "consumer_foundation_v1";
const DISPATCH_BATCH_LIMIT: u32 = 50;
const RETRY_BASE_DELAY_MS: u64 = 1_000;
const RETRY_MAX_DELAY_MS: u64 = 60_000;
const RETRY_JITTER_BASIS_POINTS: u16 = 1_000;
const MAX_AUTOMATIC_DELIVERY_ATTEMPTS: u16 = 6;
const TRANSPORT_FAILURE_RETRY_SECONDS: u32 = 30;
const MAX_QUEUE_DELAY_SECONDS: u64 = 86_400;

pub async fn dispatch_pending(env: &Env) -> Result<()> {
    let database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let queue = env.queue(INTEGRATION_EVENTS_QUEUE_BINDING)?;
    let outbox = D1IntegrationEventRepository::new(database);
    let publisher = QueueIntegrationEventPublisher::new(queue);
    dispatch_pending_events(
        &outbox,
        &publisher,
        UnixMillis::new(Date::now().as_millis()),
        DISPATCH_BATCH_LIMIT,
    )
    .await
    .map_err(operation_error)?;
    Ok(())
}

pub async fn consume(
    message_batch: MessageBatch<IntegrationEventQueueMessage>,
    env: &Env,
) -> Result<()> {
    let consumer_database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let delivery_database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let consumer = D1IntegrationEventRepository::new(consumer_database);
    let deliveries = D1NotificationRepository::new(delivery_database);
    let consumer_id = OpaqueId::parse(FOUNDATION_CONSUMER_ID).map_err(identifier_error)?;
    let retry_policy = delivery_retry_policy()?;

    for message in message_batch.messages()? {
        let event = match message.body().clone().into_event() {
            Ok(event) => event,
            Err(_) => {
                retry_after_seconds(&message, TRANSPORT_FAILURE_RETRY_SECONDS);
                continue;
            }
        };
        let now = UnixMillis::new(Date::now().as_millis());
        match process_foundation_delivery(
            &deliveries,
            &consumer,
            &consumer_id,
            &event,
            now,
            retry_policy,
        )
        .await
        {
            Ok(
                DeliveryProcessingOutcome::Delivered { .. }
                | DeliveryProcessingOutcome::DeadLetter,
            ) => message.ack(),
            Ok(DeliveryProcessingOutcome::RetryScheduled { retry_at }) => {
                retry_after_seconds(&message, queue_delay_seconds(now, retry_at)?);
            }
            Err(_) => retry_after_seconds(&message, TRANSPORT_FAILURE_RETRY_SECONDS),
        }
    }
    Ok(())
}

fn delivery_retry_policy() -> Result<RetryPolicy> {
    RetryPolicy::configured(
        RETRY_BASE_DELAY_MS,
        RETRY_MAX_DELAY_MS,
        RETRY_JITTER_BASIS_POINTS,
        MAX_AUTOMATIC_DELIVERY_ATTEMPTS,
    )
    .map_err(|_| Error::RustError("invalid notification retry configuration".into()))
}

fn queue_delay_seconds(now: UnixMillis, retry_at: UnixMillis) -> Result<u32> {
    if retry_at.value() <= now.value() {
        return Ok(1);
    }
    let delay_ms = retry_at
        .value()
        .checked_sub(now.value())
        .ok_or_else(|| Error::RustError("notification retry delay underflow".into()))?;
    let rounded_ms = delay_ms
        .checked_add(999)
        .ok_or_else(|| Error::RustError("notification retry delay overflow".into()))?;
    let delay_seconds = rounded_ms / 1_000;
    let bounded_seconds = delay_seconds.clamp(1, MAX_QUEUE_DELAY_SECONDS);
    u32::try_from(bounded_seconds)
        .map_err(|_| Error::RustError("notification retry delay conversion failed".into()))
}

fn retry_after_seconds<T>(message: &worker::Message<T>, delay_seconds: u32) {
    let options = QueueRetryOptionsBuilder::new()
        .with_delay_seconds(delay_seconds.max(1))
        .build();
    message.retry_with_options(&options);
}

fn operation_error(error: IntegrationEventOperationError) -> Error {
    Error::RustError(error.to_string())
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::queue_delay_seconds;
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn queue_retry_delay_never_becomes_zero() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(1_000), UnixMillis::new(1_000))?,
            1
        );
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(1_000), UnixMillis::new(1_001))?,
            1
        );
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(1_000), UnixMillis::new(2_001))?,
            2
        );
        Ok(())
    }

    #[test]
    fn queue_retry_delay_is_bounded_to_platform_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(0), UnixMillis::new(90_000_000))?,
            86_400
        );
        Ok(())
    }
}
