use crate::realtime_contract::{
    INTERNAL_ACTOR_HEADER, INTERNAL_CORRELATION_HEADER, INTERNAL_PUBLISH_PATH,
    INTERNAL_TENANT_HEADER, RealtimeInternalEvent,
};
use crate::realtime_notifications::{NOTIFICATION_HUB_BINDING, notification_hub_object_name};
use application_ports::{IntegrationEventEnvelope, NotificationEventRecord};
use cloudflare_adapters::d1_realtime_notifications::D1RealtimeNotificationAuthorization;
use profile_platform_primitives::ActorId;
use use_cases_notifications::realtime::{
    MAX_REALTIME_AUDIENCE_PAGE_SIZE, load_realtime_audience_page,
};
use wasm_bindgen::JsValue;
use worker::{Env, Error, Headers, Method, Request, RequestInit, Result};

const INTERNAL_PUBLISH_URL: &str = "https://notification-hub.internal/internal/realtime/publish";
const SYNC_RACE_STATUS: u16 = 409;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeFanoutOutcome {
    Accepted,
    RetrySynchronizationRace,
}

pub async fn publish_durable_event(
    event: &IntegrationEventEnvelope,
    env: &Env,
) -> Result<RealtimeFanoutOutcome> {
    let record = NotificationEventRecord::new(
        event.event_id().clone(),
        event.aggregate_type(),
        event.aggregate_id().clone(),
        event.event_type(),
        event.occurred_at(),
    );
    let audience = D1RealtimeNotificationAuthorization::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    let mut after: Option<ActorId> = None;
    let mut synchronization_race = false;

    loop {
        let actors = load_realtime_audience_page(
            &audience,
            event.tenant_id(),
            &record,
            after.as_ref(),
            MAX_REALTIME_AUDIENCE_PAGE_SIZE,
        )
        .await
        .map_err(|error| Error::RustError(error.to_string()))?;
        if actors.is_empty() {
            break;
        }
        let page_is_full = actors.len()
            == usize::try_from(MAX_REALTIME_AUDIENCE_PAGE_SIZE)
                .map_err(|_| Error::RustError("invalid realtime audience page size".to_owned()))?;

        for actor_id in &actors {
            let status = publish_to_actor(env, event, &record, actor_id).await?;
            match status {
                204 | 403 => {}
                SYNC_RACE_STATUS => synchronization_race = true,
                _ => {
                    worker::console_error!("realtime hub fanout transport failed");
                }
            }
        }

        after = actors.last().cloned();
        if !page_is_full {
            break;
        }
    }

    Ok(if synchronization_race {
        RealtimeFanoutOutcome::RetrySynchronizationRace
    } else {
        RealtimeFanoutOutcome::Accepted
    })
}

async fn publish_to_actor(
    env: &Env,
    event: &IntegrationEventEnvelope,
    record: &NotificationEventRecord,
    actor_id: &ActorId,
) -> Result<u16> {
    let payload = serde_json::to_string(&RealtimeInternalEvent::from_record(record))
        .map_err(|error| Error::RustError(error.to_string()))?;
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    headers.set(INTERNAL_TENANT_HEADER, event.tenant_id().as_str())?;
    headers.set(INTERNAL_ACTOR_HEADER, actor_id.as_str())?;
    headers.set(INTERNAL_CORRELATION_HEADER, event.event_id().as_str())?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&payload)));
    let request = Request::new_with_init(INTERNAL_PUBLISH_URL, &init)?;
    if request.path() != INTERNAL_PUBLISH_PATH {
        return Err(Error::RustError(
            "internal realtime publish path drift".to_owned(),
        ));
    }

    let namespace = env.durable_object(NOTIFICATION_HUB_BINDING)?;
    let object_id = namespace.id_from_name(&notification_hub_object_name(
        event.tenant_id(),
        actor_id,
    ))?;
    let response = object_id.get_stub()?.fetch_with_request(request).await?;
    Ok(response.status_code())
}

#[cfg(test)]
mod tests {
    use super::{INTERNAL_PUBLISH_URL, RealtimeFanoutOutcome};
    use crate::realtime_contract::INTERNAL_PUBLISH_PATH;

    #[test]
    fn internal_publish_url_and_contract_path_cannot_drift() {
        assert!(INTERNAL_PUBLISH_URL.ends_with(INTERNAL_PUBLISH_PATH));
    }

    #[test]
    fn synchronization_race_is_distinct_from_normal_acceptance() {
        assert_ne!(
            RealtimeFanoutOutcome::Accepted,
            RealtimeFanoutOutcome::RetrySynchronizationRace
        );
    }
}
