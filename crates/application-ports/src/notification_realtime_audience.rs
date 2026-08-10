#![allow(async_fn_in_trait)]

use crate::notifications::{NotificationEventRecord, NotificationPortError};
use profile_platform_primitives::{ActorId, TenantId};

/// Returns a bounded actor-id page currently eligible for one durable notification event.
/// Per-user realtime delivery must still reauthorize before emitting a signal.
pub trait RealtimeNotificationAudiencePort {
    async fn load_authorized_actor_page(
        &self,
        tenant_id: &TenantId,
        event: &NotificationEventRecord,
        after_actor_id: Option<&ActorId>,
        limit: u32,
    ) -> Result<Vec<ActorId>, NotificationPortError>;
}
