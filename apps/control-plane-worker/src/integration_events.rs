use cloudflare_adapters::d1_integration_events::D1IntegrationEventRepository;
use cloudflare_adapters::d1_notification_operations::D1NotificationOperationsRepository;
use cloudflare_adapters::d1_notifications::D1NotificationRepository;
use cloudflare_adapters::integration_event_queue::{
    IntegrationEventQueueMessage, QueueIntegrationEventPublisher,
};
use profile_platform_primitives::{OpaqueId, UnixMillis};
use use_cases_notifications::delivery::{DeliveryProcessingOutcome, process_foundation_delivery};
use use_cases_notifications::error::NotificationOperationError;
use use_cases_notifications::integration_events::{
    IntegrationEventOperationError, dispatch_pending_events,
};
use use_cases_notifications::replay::dispatch_pending_replays;
use use_cases_notifications::retention::{NotificationRetentionPolicy, compact_notification_state};
use use_cases_notifications::retry::RetryPolicy;
use worker::{Date, Env, Error, MessageExt, QueueRetryOptionsBuilder, Result};

pub const INTEGRATION_EVENTS_QUEUE_BINDING: &str = "INTEGRATION_EVENTS";
const FOUNDATION_CONSUMER_ID: &str = "consumer_foundation_v1";
const DISPATCH_BATCH_LIMIT: u32 = 50;
const REPLAY_DISPATCH_BATCH_LIMIT: u32 = 50;
const RETENTION_TTL_MS: u64 = 30 * 86_400_000;
const RETENTION_BATCH_LIMIT: u32 = 100;
const RETRY_BASE_DELAY_MS: u64 = 1_000;
const RETRY_MAX_DELAY_MS: u64 = 60_000;
const RETRY_JITTER_BASIS_POINTS: u16 = 1_000;
const MAX_AUTOMATIC_DELIVERY_ATTEMPTS: u16 = 6;
const TRANSPORT_FAILURE_RETRY_SECONDS: u32 = 30;
const MAX_QUEUE_DELAY_SECONDS: u64 = 86_400;

pub async fn dispatch_pending(env: &Env) -> Result<()> {
    let now = UnixMillis::new(Date::now().as_millis());

    let database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let queue = env.queue(INTEGRATION_EVENTS_QUEUE_BINDING)?;
    let outbox = D1IntegrationEventRepository::new(database);
    let publisher = QueueIntegrationEventPublisher::new(queue);
    dispatch_pending_events(&outbox, &publisher, now, DISPATCH_BATCH_LIMIT)
        .await
        .map_err(operation_error)?;

    let replay_database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let source_database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let replay_queue = env.queue(INTEGRATION_EVENTS_QUEUE_BINDING)?;
    let replays = D1NotificationOperationsRepository::new(replay_database);
    let source = D1IntegrationEventRepository::new(source_database);
    let replay_publisher = QueueIntegrationEventPublisher::new(replay_queue);
    dispatch_pending_replays(
        &replays,
        &source,
        &replay_publisher,
        now,
        REPLAY_DISPATCH_BATCH_LIMIT,
    )
    .await
    .map_err(notification_operation_error)?;

    let retention_database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let retention = D1NotificationOperationsRepository::new(retention_database);
    let retention_policy =
        NotificationRetentionPolicy::new(RETENTION_TTL_MS, RETENTION_BATCH_LIMIT)
            .map_err(notification_operation_error)?;
    compact_notification_state(&retention, now, retention_policy)
        .await
        .map_err(notification_operation_error)?;
    Ok(())
}

pub async fn consume_one<T>(
    message: &worker::Message<T>,
    queued: IntegrationEventQueueMessage,
    env: &Env,
) -> Result<()> {
    let event = match queued.into_event() {
        Ok(event) => event,
        Err(_) => {
            retry_after_seconds(message, TRANSPORT_FAILURE_RETRY_SECONDS);
            return Ok(());
        }
    };
    let consumer_database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let delivery_database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let consumer = D1IntegrationEventRepository::new(consumer_database);
    let deliveries = D1NotificationRepository::new(delivery_database);
    let consumer_id = OpaqueId::parse(FOUNDATION_CONSUMER_ID).map_err(identifier_error)?;
    let retry_policy = delivery_retry_policy()?;
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
        Ok(DeliveryProcessingOutcome::Delivered { .. } | DeliveryProcessingOutcome::DeadLetter) => {
            message.ack();
        }
        Ok(DeliveryProcessingOutcome::RetryScheduled { retry_at }) => {
            retry_after_seconds(message, queue_delay_seconds(now, retry_at)?);
        }
        Err(_) => retry_after_seconds(message, TRANSPORT_FAILURE_RETRY_SECONDS),
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

fn notification_operation_error(error: NotificationOperationError) -> Error {
    Error::RustError(error.to_string())
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_QUEUE_DELAY_SECONDS, RETENTION_BATCH_LIMIT, RETENTION_TTL_MS,
        TRANSPORT_FAILURE_RETRY_SECONDS, queue_delay_seconds,
    };
    use profile_platform_primitives::UnixMillis;
    use use_cases_notifications::retention::NotificationRetentionPolicy;

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
    fn queue_retry_delay_is_bounded_to_platform_limit() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(0), UnixMillis::new(90_000_000))?,
            86_400
        );
        Ok(())
    }

    #[test]
    fn transport_failure_retry_is_delayed_and_platform_bounded() {
        assert!(TRANSPORT_FAILURE_RETRY_SECONDS > 0);
        assert!(u64::from(TRANSPORT_FAILURE_RETRY_SECONDS) <= MAX_QUEUE_DELAY_SECONDS);
    }

    #[test]
    fn scheduled_retention_configuration_is_within_application_bounds() {
        assert!(NotificationRetentionPolicy::new(RETENTION_TTL_MS, RETENTION_BATCH_LIMIT).is_ok());
    }
}
