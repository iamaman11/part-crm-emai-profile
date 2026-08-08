pub const FOUNDATION_EVENT_TYPES_V1: &[&str] = &[
    "client.access_granted.v1",
    "client.access_revoked.v1",
    "client.created.v1",
    "invitation.created.v1",
    "mailbox.binding_created.v1",
    "mailbox.binding_revoked.v1",
    "mailbox.job_created.v1",
    "mailbox.job_failed.v1",
    "mailbox.job_retry_scheduled.v1",
    "mailbox.job_succeeded.v1",
    "membership.activated.v1",
    "membership.owner_transferred.v1",
    "membership.revoked.v1",
    "membership.suspended.v1",
    "profile.access_granted.v1",
    "profile.access_revoked.v1",
    "profile.client_assigned.v1",
    "profile.created.v1",
    "profile.generation_activated.v1",
    "profile.generation_deactivated.v1",
    "profile_coordinator.drain_started.v1",
    "profile_coordinator.heartbeat_accepted.v1",
    "profile_coordinator.launch_intent_expired.v1",
    "profile_coordinator.launch_intent_issued.v1",
    "profile_coordinator.lease_claimed.v1",
    "profile_coordinator.no_change.v1",
    "profile_coordinator.recovered.v1",
    "profile_coordinator.released.v1",
    "profile_coordinator.snapshot.v1",
    "profile_coordinator.timed_out.v1",
    "profile_generation.quarantined.v1",
    "profile_generation.registered.v1",
    "profile_generation.verified.v1",
    "tenant.owner_bootstrapped.v1",
];

#[must_use]
pub fn is_foundation_event_type(event_type: &str) -> bool {
    FOUNDATION_EVENT_TYPES_V1.binary_search(&event_type).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{FOUNDATION_EVENT_TYPES_V1, is_foundation_event_type};

    #[test]
    fn registry_is_sorted_unique_and_versioned() {
        for pair in FOUNDATION_EVENT_TYPES_V1.windows(2) {
            assert!(pair[0] < pair[1], "registry must remain sorted and unique");
        }
        assert!(
            FOUNDATION_EVENT_TYPES_V1
                .iter()
                .all(|value| value.ends_with(".v1"))
        );
    }

    #[test]
    fn registry_accepts_known_events_and_rejects_unknown_events() {
        assert!(is_foundation_event_type("client.created.v1"));
        assert!(is_foundation_event_type("profile_coordinator.timed_out.v1"));
        assert!(!is_foundation_event_type("client.created.v2"));
        assert!(!is_foundation_event_type("unknown.event.v1"));
    }
}
