use crate::access_session::{correlation_hint, neutral_not_found, problem};
use worker::{Error, Request, Response, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MutationFailureClass {
    NeutralNotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    DependencyUnavailable,
}

#[must_use]
pub(crate) fn classify_mutation_failure(message: &str) -> MutationFailureClass {
    if message.contains("owner_required")
        || message.contains("profile_missing")
        || message.contains("client_missing")
        || message.contains("target_not_active_member")
    {
        return MutationFailureClass::NeutralNotFound;
    }
    if message.contains("version_mismatch")
        || message.contains("tenant_version_mismatch")
        || message.contains("current_owner_mismatch")
        || message.contains("state_mismatch")
    {
        return MutationFailureClass::VersionConflict;
    }
    if message.contains("successor_mismatch")
        || message.contains("owner_invariant")
        || message.contains("last_active_owner")
        || message.contains("invalid_transition")
        || message.contains("_expired")
        || message.contains("client_not_active")
        || message.contains("grant_missing")
        || message.contains("not_verified")
        || message.contains("active_profile_generation_cannot_be_quarantined")
        || message.contains("time_regression")
    {
        return MutationFailureClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return MutationFailureClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
        || message.contains("identity_immutable")
    {
        return MutationFailureClass::IntegrityFailure;
    }
    MutationFailureClass::DependencyUnavailable
}

pub(crate) fn mutation_failure(request: &Request, error: Error) -> Result<Response> {
    match classify_mutation_failure(&error.to_string()) {
        MutationFailureClass::NeutralNotFound => neutral_not_found(&correlation_hint(request)),
        MutationFailureClass::VersionConflict => problem(
            &correlation_hint(request),
            409,
            "version_conflict",
            "Version Conflict",
        ),
        MutationFailureClass::InvalidState => problem(
            &correlation_hint(request),
            409,
            "invalid_state",
            "Invalid State",
        ),
        MutationFailureClass::Conflict => problem(
            &correlation_hint(request),
            409,
            "conflict",
            "Conflict",
        ),
        MutationFailureClass::IntegrityFailure => problem(
            &correlation_hint(request),
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        MutationFailureClass::DependencyUnavailable => problem(
            &correlation_hint(request),
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{MutationFailureClass, classify_mutation_failure};

    #[test]
    fn classifies_known_governed_failures_without_provider_leakage() {
        assert_eq!(
            classify_mutation_failure("membership_status_version_mismatch"),
            MutationFailureClass::VersionConflict
        );
        assert_eq!(
            classify_mutation_failure("owner_transfer_successor_mismatch"),
            MutationFailureClass::InvalidState
        );
        assert_eq!(
            classify_mutation_failure("profile_grant_target_not_active_member"),
            MutationFailureClass::NeutralNotFound
        );
        assert_eq!(
            classify_mutation_failure("FOREIGN KEY constraint failed"),
            MutationFailureClass::IntegrityFailure
        );
        assert_eq!(
            classify_mutation_failure("network transport reset by peer"),
            MutationFailureClass::DependencyUnavailable
        );
    }
}
