use cloudflare_adapters::d1_integration_events::D1IntegrationEventRepository;
use cloudflare_adapters::integration_event_queue::{
    IntegrationEventQueueMessage, QueueIntegrationEventPublisher,
};
use profile_platform_primitives::{OpaqueId, UnixMillis};
use use_cases::integration_events::{accept_delivery_once, dispatch_pending_events};
use worker::{Date, Env, Error, MessageBatch, Result};

pub const INTEGRATION_EVENTS_QUEUE_BINDING: &str = "INTEGRATION_EVENTS";
const FOUNDATION_CONSUMER_ID: &str = "consumer_foundation_v1";
const DISPATCH_BATCH_LIMIT: u32 = 50;

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
    let database = env.d1(control_plane_contract::D1_CATALOG_BINDING)?;
    let consumer = D1IntegrationEventRepository::new(database);
    let consumer_id = OpaqueId::parse(FOUNDATION_CONSUMER_ID).map_err(identifier_error)?;
    let consumed_at = UnixMillis::new(Date::now().as_millis());

    for message in message_batch.messages()? {
        let event = message.body().clone().into_event().map_err(port_error)?;
        accept_delivery_once(&consumer, &consumer_id, &event, consumed_at)
            .await
            .map_err(operation_error)?;
        message.ack();
    }
    Ok(())
}

fn port_error(error: application_ports::IntegrationEventPortError) -> Error {
    Error::RustError(error.to_string())
}

fn operation_error(error: use_cases::integration_events::IntegrationEventOperationError) -> Error {
    Error::RustError(error.to_string())
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}
