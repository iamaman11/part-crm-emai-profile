#![forbid(unsafe_code)]

pub mod integration_event_registry;
pub mod integration_events;

use core::fmt;
use profile_platform_primitives::{ActorContext, IdempotencyKey};

pub use integration_event_registry::{FOUNDATION_EVENT_TYPES_V1, is_foundation_event_type};
pub use integration_events::{
    INTEGRATION_EVENT_ENVELOPE_VERSION, IntegrationEventContractError, IntegrationEventEnvelope,
    IntegrationEventPayload,
};

pub const WEB_API_VERSION: ContractVersion = ContractVersion::new(1, 0);
pub const BRIDGE_PROTOCOL_VERSION: ContractVersion = ContractVersion::new(1, 0);
pub const CRM_EVENT_VERSION: ContractVersion = ContractVersion::new(1, 0);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContractVersion {
    major: u16,
    minor: u16,
}

impl ContractVersion {
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
}

impl fmt::Display for ContractVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProblemCode {
    NotFound,
    Forbidden,
    InvalidRequest,
    InvalidState,
    VersionConflict,
    LeaseConflict,
    ReplayRejected,
    DependencyUnavailable,
    IntegrityFailure,
    InternalFailure,
}

impl ProblemCode {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::InvalidRequest => "invalid_request",
            Self::InvalidState => "invalid_state",
            Self::VersionConflict => "version_conflict",
            Self::LeaseConflict => "lease_conflict",
            Self::ReplayRejected => "replay_rejected",
            Self::DependencyUnavailable => "dependency_unavailable",
            Self::IntegrityFailure => "integrity_failure",
            Self::InternalFailure => "internal_failure",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandEnvelope<T> {
    version: ContractVersion,
    actor: ActorContext,
    idempotency_key: IdempotencyKey,
    payload: T,
}

impl<T> CommandEnvelope<T> {
    #[must_use]
    pub const fn new(
        version: ContractVersion,
        actor: ActorContext,
        idempotency_key: IdempotencyKey,
        payload: T,
    ) -> Self {
        Self {
            version,
            actor,
            idempotency_key,
            payload,
        }
    }

    #[must_use]
    pub const fn version(&self) -> ContractVersion {
        self.version
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorContext {
        &self.actor
    }

    #[must_use]
    pub const fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }
}

#[cfg(test)]
mod tests {
    use super::{ContractVersion, ProblemCode};

    #[test]
    fn problem_codes_are_stable_machine_values() {
        assert_eq!(
            ProblemCode::VersionConflict.stable_code(),
            "version_conflict"
        );
        assert_eq!(ProblemCode::ReplayRejected.stable_code(), "replay_rejected");
    }

    #[test]
    fn contract_versions_are_explicit() {
        let version = ContractVersion::new(1, 2);
        assert_eq!(version.major(), 1);
        assert_eq!(version.minor(), 2);
        assert_eq!(version.to_string(), "1.2");
    }
}
