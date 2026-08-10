use profile_platform_primitives::{OutboxEventId, UnixMillis};

/// Version of the canonical realtime invalidation envelope.
///
/// Realtime is deliberately metadata-only. The contract carries no aggregate identifier,
/// contact value, mailbox metadata/body, credential, secret handle or arbitrary payload.
pub const REALTIME_INVALIDATION_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeResourceKind {
    Clients,
    Profiles,
    Mailboxes,
    Memberships,
    Devices,
    Platform,
}

impl RealtimeResourceKind {
    #[must_use]
    pub const fn stable_key(self) -> &'static str {
        match self {
            Self::Clients => "clients",
            Self::Profiles => "profiles",
            Self::Mailboxes => "mailboxes",
            Self::Memberships => "memberships",
            Self::Devices => "devices",
            Self::Platform => "platform",
        }
    }
}

/// Provider-neutral signal used only to invalidate/refetch authoritative HTTPS projections.
///
/// `event_id` is the same opaque durable ordering identity used by notification catch-up. It is
/// suitable for duplicate suppression but is not business authority. No arbitrary payload exists
/// on this type by design, so confidential content cannot be smuggled through the realtime lane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealtimeInvalidationSignal {
    version: u16,
    event_id: OutboxEventId,
    resource: RealtimeResourceKind,
    occurred_at: UnixMillis,
}

impl RealtimeInvalidationSignal {
    #[must_use]
    pub const fn new(
        event_id: OutboxEventId,
        resource: RealtimeResourceKind,
        occurred_at: UnixMillis,
    ) -> Self {
        Self {
            version: REALTIME_INVALIDATION_VERSION,
            event_id,
            resource,
            occurred_at,
        }
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    #[must_use]
    pub const fn event_id(&self) -> &OutboxEventId {
        &self.event_id
    }

    #[must_use]
    pub const fn resource(&self) -> RealtimeResourceKind {
        self.resource
    }

    #[must_use]
    pub const fn occurred_at(&self) -> UnixMillis {
        self.occurred_at
    }
}

#[cfg(test)]
mod tests {
    use super::{
        REALTIME_INVALIDATION_VERSION, RealtimeInvalidationSignal, RealtimeResourceKind,
    };
    use profile_platform_primitives::{OutboxEventId, UnixMillis};

    #[test]
    fn invalidation_contract_is_versioned_and_metadata_only() -> Result<(), Box<dyn std::error::Error>>
    {
        let signal = RealtimeInvalidationSignal::new(
            OutboxEventId::parse("outbox_01JREALTIME")?,
            RealtimeResourceKind::Clients,
            UnixMillis::new(42),
        );
        assert_eq!(signal.version(), REALTIME_INVALIDATION_VERSION);
        assert_eq!(signal.event_id().as_str(), "outbox_01JREALTIME");
        assert_eq!(signal.resource().stable_key(), "clients");
        assert_eq!(signal.occurred_at(), UnixMillis::new(42));
        Ok(())
    }

    #[test]
    fn resource_keys_are_low_cardinality_and_stable() {
        let keys = [
            RealtimeResourceKind::Clients,
            RealtimeResourceKind::Profiles,
            RealtimeResourceKind::Mailboxes,
            RealtimeResourceKind::Memberships,
            RealtimeResourceKind::Devices,
            RealtimeResourceKind::Platform,
        ]
        .map(RealtimeResourceKind::stable_key);
        assert_eq!(
            keys,
            [
                "clients",
                "profiles",
                "mailboxes",
                "memberships",
                "devices",
                "platform",
            ]
        );
    }
}
