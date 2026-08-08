use application_ports::identity_governance::{
    IdentityGovernancePortError, IdentityGovernancePortErrorClass,
};
use worker::Error;

pub(crate) fn map_identity_dependency_error(_error: Error) -> IdentityGovernancePortError {
    IdentityGovernancePortError::new(IdentityGovernancePortErrorClass::DependencyUnavailable)
}

pub(crate) fn map_identity_write_error(error: Error) -> IdentityGovernancePortError {
    IdentityGovernancePortError::new(classify_identity_write_failure(&error.to_string()))
}

pub(crate) fn classify_identity_write_failure(message: &str) -> IdentityGovernancePortErrorClass {
    if message.contains("owner_required")
        || message.contains("profile_missing")
        || message.contains("target_missing")
        || message.contains("successor_mismatch")
        || message.contains("target_not_active_member")
        || message.contains("invitation_not_pending_or_expired")
    {
        return IdentityGovernancePortErrorClass::NotFound;
    }
    if message.contains("state_mismatch")
        || message.contains("version_mismatch")
        || message.contains("tenant_version_mismatch")
        || message.contains("owner_transfer_current_owner_mismatch")
    {
        return IdentityGovernancePortErrorClass::VersionConflict;
    }
    if message.contains("last_active_owner")
        || message.contains("invitation_create_expired")
        || message.contains("owner_transfer_owner_invariant")
        || message.contains("invalid_transition")
        || message.contains("time_regression")
    {
        return IdentityGovernancePortErrorClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return IdentityGovernancePortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
        || message.contains("identity_immutable")
    {
        return IdentityGovernancePortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return IdentityGovernancePortErrorClass::InternalFailure;
    }
    IdentityGovernancePortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::classify_identity_write_failure;
    use application_ports::identity_governance::IdentityGovernancePortErrorClass;

    #[test]
    fn identity_write_failures_match_the_stable_step4_taxonomy() {
        let cases = [
            (
                "owner_transfer_successor_mismatch",
                IdentityGovernancePortErrorClass::NotFound,
            ),
            (
                "membership_status_target_missing",
                IdentityGovernancePortErrorClass::NotFound,
            ),
            (
                "invitation_not_pending_or_expired",
                IdentityGovernancePortErrorClass::NotFound,
            ),
            (
                "owner_transfer_current_owner_mismatch",
                IdentityGovernancePortErrorClass::VersionConflict,
            ),
            (
                "owner_transfer_successor_version_mismatch",
                IdentityGovernancePortErrorClass::VersionConflict,
            ),
            (
                "invitation_create_tenant_version_mismatch",
                IdentityGovernancePortErrorClass::VersionConflict,
            ),
            (
                "last_active_owner",
                IdentityGovernancePortErrorClass::InvalidState,
            ),
            (
                "invitation_create_expired",
                IdentityGovernancePortErrorClass::InvalidState,
            ),
            (
                "UNIQUE constraint failed: memberships.tenant_id",
                IdentityGovernancePortErrorClass::Conflict,
            ),
            (
                "identity_immutable",
                IdentityGovernancePortErrorClass::IntegrityFailure,
            ),
            (
                "value exceeds SQLite INTEGER",
                IdentityGovernancePortErrorClass::InternalFailure,
            ),
            (
                "network request failed",
                IdentityGovernancePortErrorClass::DependencyUnavailable,
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(classify_identity_write_failure(message), expected);
        }
    }
}
