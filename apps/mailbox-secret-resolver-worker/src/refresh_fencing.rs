//! Durable OAuth refresh fencing semantics.
//!
//! This module intentionally contains no token material and no in-memory lock authority. The
//! persisted resolver row is the authority; these types define the decisions that storage must
//! enforce atomically.

pub const REFRESH_LEASE_TTL_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialLifecycleState {
    Active,
    ReauthRequired,
}

impl CredentialLifecycleState {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::ReauthRequired => "REAUTH_REQUIRED",
        }
    }

    #[must_use]
    pub const fn parse(value: &str) -> Option<Self> {
        match value {
            "ACTIVE" => Some(Self::Active),
            "REAUTH_REQUIRED" => Some(Self::ReauthRequired),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshLeaseSnapshot {
    generation: u64,
    lifecycle: CredentialLifecycleState,
    owner_digest: Option<String>,
    expires_at_ms: Option<u64>,
}

impl RefreshLeaseSnapshot {
    #[must_use]
    pub fn new(
        generation: u64,
        lifecycle: CredentialLifecycleState,
        owner_digest: Option<String>,
        expires_at_ms: Option<u64>,
    ) -> Option<Self> {
        if generation == 0
            || owner_digest
                .as_deref()
                .is_some_and(|value| !valid_owner_digest(value))
            || owner_digest.is_some() != expires_at_ms.is_some()
        {
            return None;
        }
        Some(Self {
            generation,
            lifecycle,
            owner_digest,
            expires_at_ms,
        })
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn lifecycle(&self) -> CredentialLifecycleState {
        self.lifecycle
    }

    #[must_use]
    pub fn owner_digest(&self) -> Option<&str> {
        self.owner_digest.as_deref()
    }

    #[must_use]
    pub const fn expires_at_ms(&self) -> Option<u64> {
        self.expires_at_ms
    }

    #[must_use]
    pub fn acquire_decision(&self, owner_digest: &str, now_ms: u64) -> RefreshAcquireDecision {
        if now_ms == 0 || !valid_owner_digest(owner_digest) {
            return RefreshAcquireDecision::Invalid;
        }
        if self.lifecycle == CredentialLifecycleState::ReauthRequired {
            return RefreshAcquireDecision::ReauthRequired;
        }
        if let (Some(current_owner), Some(expires_at)) =
            (self.owner_digest.as_deref(), self.expires_at_ms)
            && expires_at > now_ms
            && current_owner != owner_digest
        {
            return RefreshAcquireDecision::Busy;
        }
        RefreshAcquireDecision::Acquire {
            expected_generation: self.generation,
            expires_at_ms: now_ms.saturating_add(REFRESH_LEASE_TTL_MS),
        }
    }

    #[must_use]
    pub fn commit_decision(
        &self,
        owner_digest: &str,
        expected_generation: u64,
        now_ms: u64,
    ) -> RefreshCommitDecision {
        if now_ms == 0 || !valid_owner_digest(owner_digest) || expected_generation == 0 {
            return RefreshCommitDecision::Invalid;
        }
        if self.lifecycle == CredentialLifecycleState::ReauthRequired {
            return RefreshCommitDecision::ReauthRequired;
        }
        if self.generation != expected_generation {
            return RefreshCommitDecision::StaleGeneration;
        }
        match (self.owner_digest.as_deref(), self.expires_at_ms) {
            (Some(current_owner), Some(expires_at))
                if current_owner == owner_digest && expires_at > now_ms =>
            {
                RefreshCommitDecision::Commit {
                    next_generation: expected_generation.saturating_add(1),
                }
            }
            _ => RefreshCommitDecision::LeaseLost,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshAcquireDecision {
    Acquire {
        expected_generation: u64,
        expires_at_ms: u64,
    },
    Busy,
    ReauthRequired,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshCommitDecision {
    Commit { next_generation: u64 },
    StaleGeneration,
    LeaseLost,
    ReauthRequired,
    Invalid,
}

#[must_use]
pub fn valid_owner_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::{
        CredentialLifecycleState, REFRESH_LEASE_TTL_MS, RefreshAcquireDecision,
        RefreshCommitDecision, RefreshLeaseSnapshot,
    };

    const OWNER_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OWNER_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn active(generation: u64) -> RefreshLeaseSnapshot {
        RefreshLeaseSnapshot::new(generation, CredentialLifecycleState::Active, None, None)
            .expect("valid active snapshot")
    }

    #[test]
    fn one_generation_has_one_live_refresh_owner() {
        let snapshot = active(7);
        assert_eq!(
            snapshot.acquire_decision(OWNER_A, 1_000),
            RefreshAcquireDecision::Acquire {
                expected_generation: 7,
                expires_at_ms: 1_000 + REFRESH_LEASE_TTL_MS,
            }
        );

        let leased = RefreshLeaseSnapshot::new(
            7,
            CredentialLifecycleState::Active,
            Some(OWNER_A.to_owned()),
            Some(1_000 + REFRESH_LEASE_TTL_MS),
        )
        .expect("valid lease");
        assert_eq!(
            leased.acquire_decision(OWNER_B, 1_001),
            RefreshAcquireDecision::Busy
        );
    }

    #[test]
    fn expired_lease_is_recoverable_after_crash() {
        let expired = RefreshLeaseSnapshot::new(
            11,
            CredentialLifecycleState::Active,
            Some(OWNER_A.to_owned()),
            Some(5_000),
        )
        .expect("valid lease");
        assert_eq!(
            expired.acquire_decision(OWNER_B, 5_000),
            RefreshAcquireDecision::Acquire {
                expected_generation: 11,
                expires_at_ms: 5_000 + REFRESH_LEASE_TTL_MS,
            }
        );
    }

    #[test]
    fn stale_generation_and_wrong_owner_cannot_commit() {
        let leased = RefreshLeaseSnapshot::new(
            9,
            CredentialLifecycleState::Active,
            Some(OWNER_A.to_owned()),
            Some(40_000),
        )
        .expect("valid lease");
        assert_eq!(
            leased.commit_decision(OWNER_A, 8, 20_000),
            RefreshCommitDecision::StaleGeneration
        );
        assert_eq!(
            leased.commit_decision(OWNER_B, 9, 20_000),
            RefreshCommitDecision::LeaseLost
        );
        assert_eq!(
            leased.commit_decision(OWNER_A, 9, 20_000),
            RefreshCommitDecision::Commit {
                next_generation: 10
            }
        );
    }

    #[test]
    fn expired_owner_cannot_commit_provider_result() {
        let leased = RefreshLeaseSnapshot::new(
            3,
            CredentialLifecycleState::Active,
            Some(OWNER_A.to_owned()),
            Some(2_000),
        )
        .expect("valid lease");
        assert_eq!(
            leased.commit_decision(OWNER_A, 3, 2_000),
            RefreshCommitDecision::LeaseLost
        );
    }

    #[test]
    fn reauth_required_is_fail_closed_for_acquire_and_commit() {
        let snapshot = RefreshLeaseSnapshot::new(
            4,
            CredentialLifecycleState::ReauthRequired,
            None,
            None,
        )
        .expect("valid reauth snapshot");
        assert_eq!(
            snapshot.acquire_decision(OWNER_A, 100),
            RefreshAcquireDecision::ReauthRequired
        );
        assert_eq!(
            snapshot.commit_decision(OWNER_A, 4, 100),
            RefreshCommitDecision::ReauthRequired
        );
    }

    #[test]
    fn malformed_persisted_lease_is_rejected() {
        assert!(
            RefreshLeaseSnapshot::new(
                1,
                CredentialLifecycleState::Active,
                Some("not-a-digest".to_owned()),
                Some(10),
            )
            .is_none()
        );
        assert!(
            RefreshLeaseSnapshot::new(
                1,
                CredentialLifecycleState::Active,
                Some(OWNER_A.to_owned()),
                None,
            )
            .is_none()
        );
    }
}
