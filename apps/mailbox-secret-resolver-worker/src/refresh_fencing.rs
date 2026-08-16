//! Shared persisted vocabulary for OAuth credential refresh fencing.
//!
//! Executable refresh ownership and generation rules live in `storage`: the D1 row is the
//! authority and all acquire/commit/release transitions are enforced atomically there. This module
//! deliberately contains no parallel refresh state machine.

pub const REFRESH_LEASE_TTL_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialLifecycleState {
    Active,
    ReauthRequired,
}

impl CredentialLifecycleState {
    #[must_use]
    pub const fn parse(value: &str) -> Option<Self> {
        match value {
            "ACTIVE" => Some(Self::Active),
            "REAUTH_REQUIRED" => Some(Self::ReauthRequired),
            _ => None,
        }
    }
}
