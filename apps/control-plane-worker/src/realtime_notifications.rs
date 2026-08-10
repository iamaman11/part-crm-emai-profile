use crate::access_session::{correlation_hint, neutral_not_found, resolve_active_request_actor};
use application_ports::{
    NotificationAuthorizationPort, NotificationCapability, NotificationEventRecord,
    NotificationPortError, NotificationPortErrorClass, RealtimeNotificationSinkPort,
};
use cloudflare_adapters::d1_notification_operations::D1NotificationOperationsRepository;
use cloudflare_adapters::d1_notifications::D1NotificationRepository;
use cloudflare_adapters::d1_realtime_notifications::D1RealtimeNotificationAuthorization;
use contracts::RealtimeInvalidationSignal;
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, OpaqueId, OutboxEventId, TenantId, TenantScope, UnixMillis,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use use_cases_notifications::error::NotificationOperationError;
use use_cases_notifications::realtime::{publish_live_invalidation, synchronize_realtime_session};
use worker::{
    DurableObject, Env, Error, Method, Request, Response, Result, State, WebSocket,
    WebSocketIncomingMessage, WebSocketPair, durable_object,
};

pub const NOTIFICATION_HUB_BINDING: &str = "NOTIFICATION_HUB";
const ACCESS_TOKEN_HEADER: &str = "Cf-Access-Jwt-Assertion";
const INTERNAL_TENANT_HEADER: &str = "X-Internal-Realtime-Tenant-Id";
const INTERNAL_ACTOR_HEADER: &str = "X-Internal-Realtime-Actor-Id";
const INTERNAL_CORRELATION_HEADER: &str = "X-Internal-Realtime-Correlation-Id";
const REALTIME_CONNECT_SUFFIX: &str = "/notifications/realtime";
const INTERNAL_PUBLISH_PATH: &str = "/internal/realtime/publish";
const REAUTH_INTERVAL_SECONDS: u64 = 60;
const CATCH_UP_PAGE_SIZE: u32 = 200;
const POLICY_CLOSE_CODE: u16 = 1008;
const INTERNAL_ALARM_CORRELATION_ID: &str = "corr_realtime_alarm";

pub async fn connect(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    if !is_websocket_upgrade(request)? {
        return Response::error("WebSocket upgrade required", 426);
    }
    let Some(resolved) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };

    let mut internal = request.clone_mut()?;
    let headers = internal.headers_mut()?;
    // Credentials are consumed at the Worker boundary. The per-user Durable Object receives only
    // the verified opaque actor context required for a second current-membership check.
    headers.delete(ACCESS_TOKEN_HEADER)?;
    headers.delete("Authorization")?;
    headers.delete("Cookie")?;
    headers.set(
        INTERNAL_TENANT_HEADER,
        resolved.actor().tenant_scope().tenant_id().as_str(),
    )?;
    headers.set(INTERNAL_ACTOR_HEADER, resolved.actor().actor_id().as_str())?;
    headers.set(
        INTERNAL_CORRELATION_HEADER,
        resolved.actor().correlation_id().as_str(),
    )?;

    let namespace = env.durable_object(NOTIFICATION_HUB_BINDING)?;
    let object_id = namespace.id_from_name(&notification_hub_object_name(
        resolved.actor().tenant_scope().tenant_id(),
        resolved.actor().actor_id(),
    ))?;
    object_id.get_stub()?.fetch_with_request(internal).await
}

#[must_use]
pub fn notification_hub_object_name(tenant_id: &TenantId, actor_id: &ActorId) -> String {
    format!(
        "notification-hub-v1:{}:{}",
        tenant_id.as_str(),
        actor_id.as_str()
    )
}

#[durable_object]
pub struct NotificationHub {
    state: State,
    env: Env,
}

impl DurableObject for NotificationHub {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, mut request: Request) -> Result<Response> {
        match (request.method(), request.path()) {
            (Method::Get, path) if path.ends_with(REALTIME_CONNECT_SUFFIX) => {
                self.connect_socket(&request).await
            }
            (Method::Post, path) if path == INTERNAL_PUBLISH_PATH => {
                self.publish_event(&mut request).await
            }
            _ => Response::error("Not Found", 404),
        }
    }

    async fn alarm(&self) -> Result<Response> {
        let sockets = self.state.get_websockets();
        let Some(socket) = sockets.first() else {
            return Response::ok("no realtime connections");
        };
        let attachment = match socket.deserialize_attachment::<HubSocketAttachment>() {
            Ok(Some(value)) => value,
            _ => {
                close_all(&sockets, "invalid connection context");
                return Response::ok("invalid realtime connection context");
            }
        };
        let actor = match attachment.into_actor_context(INTERNAL_ALARM_CORRELATION_ID) {
            Ok(value) => value,
            Err(()) => {
                close_all(&sockets, "invalid connection context");
                return Response::ok("invalid realtime connection context");
            }
        };
        let authorization = D1NotificationOperationsRepository::new(
            self.env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
        );
        match authorization
            .is_authorized(&actor, NotificationCapability::CatchUp)
            .await
        {
            Ok(true) => {
                schedule_reauthorization(&self.state).await?;
                Response::ok("realtime authorization current")
            }
            Ok(false) => {
                close_all(&sockets, "authorization revoked");
                Response::ok("realtime authorization revoked")
            }
            Err(_) => {
                // Fail closed on current-membership uncertainty. Durable notification history is
                // preserved in D1 and a later authorized reconnect will catch up from its cursor.
                close_all(&sockets, "authorization unavailable");
                Response::ok("realtime authorization unavailable")
            }
        }
    }

    async fn websocket_message(
        &self,
        _socket: WebSocket,
        _message: WebSocketIncomingMessage,
    ) -> Result<()> {
        // The realtime channel is intentionally server-to-client invalidation only. Client
        // messages never become business commands or canonical state.
        Ok(())
    }

    async fn websocket_close(
        &self,
        _socket: WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        Ok(())
    }

    async fn websocket_error(&self, _socket: WebSocket, _error: Error) -> Result<()> {
        Ok(())
    }
}

impl NotificationHub {
    async fn connect_socket(&self, request: &Request) -> Result<Response> {
        if !is_websocket_upgrade(request)? {
            return Response::error("WebSocket upgrade required", 426);
        }
        let actor = match internal_actor(request) {
            Ok(value) => value,
            Err(()) => return Response::error("Forbidden", 403),
        };
        let authorization = D1NotificationOperationsRepository::new(
            self.env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
        );
        match authorization
            .is_authorized(&actor, NotificationCapability::CatchUp)
            .await
        {
            Ok(true) => {}
            Ok(false) => return Response::error("Forbidden", 403),
            Err(_) => return Response::error("Dependency unavailable", 503),
        }

        let pair = WebSocketPair::new()?;
        let attachment = HubSocketAttachment::from_actor(&actor);
        pair.server.serialize_attachment(&attachment)?;
        self.state.accept_web_socket(&pair.server);

        let cursors = D1NotificationRepository::new(
            self.env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
        );
        let history = D1NotificationOperationsRepository::new(
            self.env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
        );
        let sink = SingleSocketSink {
            socket: &pair.server,
        };
        if let Err(error) = synchronize_realtime_session(
            &authorization,
            &cursors,
            &history,
            &sink,
            &actor,
            CATCH_UP_PAGE_SIZE,
            UnixMillis::new(worker::Date::now().as_millis()),
        )
        .await
        {
            let _close_result = pair
                .server
                .close(Some(POLICY_CLOSE_CODE), Some("realtime synchronization failed"));
            return synchronization_failure(error);
        }

        schedule_reauthorization(&self.state).await?;
        Response::from_websocket(pair.client)
    }

    async fn publish_event(&self, request: &mut Request) -> Result<Response> {
        let actor = match internal_actor(request) {
            Ok(value) => value,
            Err(()) => return Response::error("Forbidden", 403),
        };
        let body = match request.json::<RealtimeInternalEvent>().await {
            Ok(value) => value,
            Err(_) => return Response::error("Invalid request", 400),
        };
        let event = match body.into_record() {
            Ok(value) => value,
            Err(()) => return Response::error("Invalid request", 400),
        };
        let authorization = D1NotificationOperationsRepository::new(
            self.env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
        );
        let event_authorization = D1RealtimeNotificationAuthorization::new(
            self.env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
        );
        let sink = HubBroadcastSink { state: &self.state };
        match publish_live_invalidation(
            &authorization,
            &event_authorization,
            &sink,
            &actor,
            &event,
        )
        .await
        {
            Ok(()) => Response::empty().map(|response| response.with_status(204)),
            Err(NotificationOperationError::Forbidden) => Response::error("Forbidden", 403),
            Err(NotificationOperationError::DependencyUnavailable) => {
                Response::error("Dependency unavailable", 503)
            }
            Err(_) => Response::error("Realtime delivery failed", 500),
        }
    }
}

struct SingleSocketSink<'a> {
    socket: &'a WebSocket,
}

impl RealtimeNotificationSinkPort for SingleSocketSink<'_> {
    async fn publish_invalidation(
        &self,
        _actor: &ActorContext,
        signal: &RealtimeInvalidationSignal,
    ) -> Result<(), NotificationPortError> {
        self.socket
            .send_with_str(signal.canonical_json())
            .map_err(|_| NotificationPortError::new(NotificationPortErrorClass::DependencyUnavailable))
    }
}

struct HubBroadcastSink<'a> {
    state: &'a State,
}

impl RealtimeNotificationSinkPort for HubBroadcastSink<'_> {
    async fn publish_invalidation(
        &self,
        _actor: &ActorContext,
        signal: &RealtimeInvalidationSignal,
    ) -> Result<(), NotificationPortError> {
        let payload = signal.canonical_json();
        for socket in self.state.get_websockets() {
            if socket.send_with_str(&payload).is_err() {
                let _close_result = socket.close(Some(1011), Some("realtime socket unavailable"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct HubSocketAttachment {
    tenant_id: String,
    actor_id: String,
}

impl HubSocketAttachment {
    fn from_actor(actor: &ActorContext) -> Self {
        Self {
            tenant_id: actor.tenant_scope().tenant_id().as_str().to_owned(),
            actor_id: actor.actor_id().as_str().to_owned(),
        }
    }

    fn into_actor_context(self, correlation_id: &str) -> Result<ActorContext, ()> {
        let tenant_id = TenantId::parse(self.tenant_id).map_err(|_| ())?;
        let actor_id = ActorId::parse(self.actor_id).map_err(|_| ())?;
        let correlation_id = CorrelationId::parse(correlation_id.to_owned()).map_err(|_| ())?;
        Ok(ActorContext::new(
            TenantScope::new(tenant_id),
            actor_id,
            correlation_id,
        ))
    }
}

#[derive(Deserialize, Serialize)]
pub struct RealtimeInternalEvent {
    pub event_id: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub occurred_at_ms: u64,
}

impl RealtimeInternalEvent {
    #[must_use]
    pub fn from_record(event: &NotificationEventRecord) -> Self {
        Self {
            event_id: event.event_id().as_str().to_owned(),
            aggregate_type: event.aggregate_type().to_owned(),
            aggregate_id: event.aggregate_id().as_str().to_owned(),
            event_type: event.event_type().to_owned(),
            occurred_at_ms: event.occurred_at().value(),
        }
    }

    fn into_record(self) -> Result<NotificationEventRecord, ()> {
        if !valid_symbol(&self.aggregate_type, 80) || !valid_symbol(&self.event_type, 160) {
            return Err(());
        }
        Ok(NotificationEventRecord::new(
            OutboxEventId::parse(self.event_id).map_err(|_| ())?,
            self.aggregate_type,
            OpaqueId::parse(self.aggregate_id).map_err(|_| ())?,
            self.event_type,
            UnixMillis::new(self.occurred_at_ms),
        ))
    }
}

fn internal_actor(request: &Request) -> Result<ActorContext, ()> {
    let tenant_id = request
        .headers()
        .get(INTERNAL_TENANT_HEADER)
        .map_err(|_| ())?
        .ok_or(())?;
    let actor_id = request
        .headers()
        .get(INTERNAL_ACTOR_HEADER)
        .map_err(|_| ())?
        .ok_or(())?;
    let correlation_id = request
        .headers()
        .get(INTERNAL_CORRELATION_HEADER)
        .map_err(|_| ())?
        .ok_or(())?;
    HubSocketAttachment {
        tenant_id,
        actor_id,
    }
    .into_actor_context(&correlation_id)
}

fn is_websocket_upgrade(request: &Request) -> Result<bool> {
    Ok(request
        .headers()
        .get("Upgrade")?
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket")))
}

async fn schedule_reauthorization(state: &State) -> Result<()> {
    state
        .storage()
        .set_alarm(Duration::from_secs(REAUTH_INTERVAL_SECONDS))
        .await
}

fn close_all(sockets: &[WebSocket], reason: &str) {
    for socket in sockets {
        let _close_result = socket.close(Some(POLICY_CLOSE_CODE), Some(reason));
    }
}

fn synchronization_failure(error: NotificationOperationError) -> Result<Response> {
    match error {
        NotificationOperationError::Forbidden => Response::error("Forbidden", 403),
        NotificationOperationError::DependencyUnavailable => {
            Response::error("Dependency unavailable", 503)
        }
        _ => Response::error("Realtime synchronization failed", 500),
    }
}

fn valid_symbol(value: &str, maximum: usize) -> bool {
    let length = value.len();
    (1..=maximum).contains(&length)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::{RealtimeInternalEvent, notification_hub_object_name};
    use application_ports::NotificationEventRecord;
    use profile_platform_primitives::{
        ActorId, OpaqueId, OutboxEventId, TenantId, UnixMillis,
    };

    #[test]
    fn object_name_is_stable_and_opaque_identity_only() -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_01JREALTIME")?;
        let actor = ActorId::parse("actor_01JREALTIME")?;
        assert_eq!(
            notification_hub_object_name(&tenant, &actor),
            "notification-hub-v1:tenant_01JREALTIME:actor_01JREALTIME"
        );
        Ok(())
    }

    #[test]
    fn internal_event_round_trip_contains_no_integration_payload()
    -> Result<(), Box<dyn std::error::Error>> {
        let record = NotificationEventRecord::new(
            OutboxEventId::parse("outbox_01JREALTIME")?,
            "client",
            OpaqueId::parse("client_01JREALTIME")?,
            "client.changed.v1",
            UnixMillis::new(42),
        );
        let internal = RealtimeInternalEvent::from_record(&record);
        let encoded = serde_json::to_string(&internal)?;
        assert!(!encoded.contains("payload"));
        assert!(!encoded.contains("subject"));
        assert!(!encoded.contains("body"));
        let restored = serde_json::from_str::<RealtimeInternalEvent>(&encoded)?
            .into_record()
            .map_err(|()| "invalid realtime internal event")?;
        assert_eq!(restored, record);
        Ok(())
    }
}
