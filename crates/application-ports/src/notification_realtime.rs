#![allow(async_fn_in_trait)]

use crate::notifications::{NotificationEventRecord, NotificationPortError};
use contracts::RealtimeInvalidationSignal;
use profile_platform_primitives::ActorContext;

/// Live-delivery authorization is intentionally separate from generic notification capability
/// authorization. Implementations must evaluate current tenant membership and current resource grants
/// for the specific durable event before any realtime signal is emitted.
pub trait RealtimeNotificationAuthorizationPort {
    async fn is_event_authorized(
        &self,
        actor: &ActorContext,
        event: &NotificationEventRecord,
    ) -> Result<bool, NotificationPortError>;
}

/// Provider-neutral outer boundary for a per-user realtime connection coordinator.
///
/// Implementations may use Durable Objects/WebSockets, but this port exposes only the canonical
/// metadata-safe invalidation contract. The sink is not allowed to become a business-state store.
pub trait RealtimeNotificationSinkPort {
    async fn publish_invalidation(
        &self,
        actor: &ActorContext,
        signal: &RealtimeInvalidationSignal,
    ) -> Result<(), NotificationPortError>;
}
