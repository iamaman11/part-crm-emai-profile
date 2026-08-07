use crate::access_session::{correlation_hint, neutral_not_found, problem};
use worker::{Error, Request, Response, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MutationFailureClass {
    NeutralNotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

pub(crate) fn classify_mutation_failure(message: &str) -> MutationFailureClass {
    if message.contains("owner_required")
        || message.contains("profile_missing")
        || message.contains("successor_mismatch")
        || message.contains("target_not_active_member")
        || message.contains("client_not_active")
        || message.contains("invitation_not_pending_or_expired")
    {
        return MutationFailureClass::NeutralNotFound;
    }

    if message.contains("state_mismatch")
        || message.contains("version_mismatch")
        || message.contains("tenant_version_mismatch")
        || message.contains("owner_transfer_current_owner_mismatch")
    {
        return MutationFailureClass::VersionConflict;
    }

    if message.contains("not_verified")
        || message.contains("active_profile_generation_cannot_be_quarantined")
        || message.contains("time_regression")
        || message.contains("invalid_transition")
        || message.contains("last_active_owner")
        || message.contains("invitation_create_expired")
        || message.contains("grant_missing")
        || message.contains("owner_transfer_owner_invariant")
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

    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return MutationFailureClass::InternalFailure;
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
        MutationFailureClass::Conflict => {
            problem(&correlation_hint(request), 409, "conflict", "Conflict")
        }
        MutationFailureClass::IntegrityFailure => problem(
            &correlation_hint(request),
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        MutationFailureClass::InternalFailure => problem(
            &correlation_hint(request),
            500,
            "internal_failure",
            "Internal Failure",
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
    fn governed_failures_have_stable_public_classes() {
        assert_eq!(
            classify_mutation_failure("profile_generation_activate_profile_state_mismatch"),
            MutationFailureClass::VersionConflict
        );
        assert_eq!(
            classify_mutation_failure("membership_status_version_mismatch"),
            MutationFailureClass::VersionConflict
        );
        assert_eq!(
            classify_mutation_failure("profile_assignment_client_not_active"),
            MutationFailureClass::NeutralNotFound
        );
        assert_eq!(
            classify_mutation_failure("invitation_not_pending_or_expired"),
            MutationFailureClass::NeutralNotFound
        );
        assert_eq!(
            classify_mutation_failure("profile_generation_not_verified"),
            MutationFailureClass::InvalidState
        );
        assert_eq!(
            classify_mutation_failure("last_active_owner"),
            MutationFailureClass::InvalidState
        );
        assert_eq!(
            classify_mutation_failure("UNIQUE constraint failed: clients.tenant_id"),
            MutationFailureClass::Conflict
        );
        assert_eq!(
            classify_mutation_failure("profile_generation_activation_not_governed"),
            MutationFailureClass::IntegrityFailure
        );
        assert_eq!(
            classify_mutation_failure("aggregate version overflow"),
            MutationFailureClass::InternalFailure
        );
        assert_eq!(
            classify_mutation_failure("value exceeds SQLite INTEGER"),
            MutationFailureClass::InternalFailure
        );
        assert_eq!(
            classify_mutation_failure("network request failed"),
            MutationFailureClass::DependencyUnavailable
        );
    }
}
