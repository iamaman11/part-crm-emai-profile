use contracts::IntegrationEventEnvelope;
use core::fmt;
use profile_platform_primitives::{OpaqueId, OutboxEventId, TenantId, UnixMillis};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumerClaim {
    Claimed,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationEventPortErrorClass {
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegrationEventPortError {
    class: IntegrationEventPortErrorClass,
}

impl IntegrationEventPortError {
    #[must_use]
    pub const fn new(class: IntegrationEventPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> IntegrationEventPortErrorClass {
        self.class
    }
}

impl fmt::Display for IntegrationEventPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            IntegrationEventPortErrorClass::Conflict => "integration event port conflict",
            IntegrationEventPortErrorClass::IntegrityFailure => {
                "integration event port integrity failure"
            }
            IntegrationEventPortErrorClass::InternalFailure => {
                "integration event port internal failure"
            }
            IntegrationEventPortErrorClass::DependencyUnavailable => {
                "integration event dependency unavailable"
            }
        })
    }
}

impl std::error::Error for IntegrationEventPortError {}

pub trait IntegrationEventOutboxPort {
    async fn load_pending(
        &self,
        limit: u32,
    ) -> Result<Vec<IntegrationEventEnvelope>, IntegrationEventPortError>;

    async fn mark_published(
        &self,
        tenant_id: &TenantId,
        event_id: &OutboxEventId,
        published_at: UnixMillis,
    ) -> Result<(), IntegrationEventPortError>;
}

pub trait IntegrationEventPublisherPort {
    async fn publish(
        &self,
        event: &IntegrationEventEnvelope,
    ) -> Result<(), IntegrationEventPortError>;
}

pub trait ConsumerIdempotencyPort {
    async fn claim(
        &self,
        tenant_id: &TenantId,
        consumer_id: &OpaqueId,
        event_id: &OutboxEventId,
        consumed_at: UnixMillis,
    ) -> Result<ConsumerClaim, IntegrationEventPortError>;
}
