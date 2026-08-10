use super::LocalProfileError;
use profile_platform_primitives::{GenerationId, UnixMillis};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LocalGenerationState {
    MaterializedClean,
    InUse,
    DirtyLocal,
    RecoveryRequired,
    Quarantined,
    SyncedEvictable,
    SupersededEvictable,
    Evicted,
}

impl LocalGenerationState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MaterializedClean => "materialized_clean",
            Self::InUse => "in_use",
            Self::DirtyLocal => "dirty_local",
            Self::RecoveryRequired => "recovery_required",
            Self::Quarantined => "quarantined",
            Self::SyncedEvictable => "synced_evictable",
            Self::SupersededEvictable => "superseded_evictable",
            Self::Evicted => "evicted",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalGenerationRecord {
    generation_id: GenerationId,
    state: LocalGenerationState,
    bytes: u64,
    last_activity_at: UnixMillis,
    session_started_at: Option<UnixMillis>,
    locked: bool,
}

impl LocalGenerationRecord {
    #[must_use]
    pub const fn new(generation_id: GenerationId, bytes: u64, observed_at: UnixMillis) -> Self {
        Self {
            generation_id,
            state: LocalGenerationState::MaterializedClean,
            bytes,
            last_activity_at: observed_at,
            session_started_at: None,
            locked: false,
        }
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn state(&self) -> LocalGenerationState {
        self.state
    }

    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn last_activity_at(&self) -> UnixMillis {
        self.last_activity_at
    }

    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn set_locked(&mut self, locked: bool) -> Result<(), LocalProfileError> {
        if self.state == LocalGenerationState::Evicted && locked {
            return Err(LocalProfileError::InvalidTransition);
        }
        self.locked = locked;
        Ok(())
    }

    pub fn begin_use(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if !self.locked
            || !matches!(
                self.state,
                LocalGenerationState::MaterializedClean | LocalGenerationState::SyncedEvictable
            )
        {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::InUse;
        self.last_activity_at = now;
        self.session_started_at = Some(now);
        Ok(())
    }

    pub fn observe_activity(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::InUse {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn graceful_close(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::InUse {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::DirtyLocal;
        self.last_activity_at = now;
        self.session_started_at = None;
        Ok(())
    }

    pub fn observe_crash(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::InUse {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::RecoveryRequired;
        self.last_activity_at = now;
        self.session_started_at = None;
        Ok(())
    }

    pub fn complete_recovery(
        &mut self,
        clone_integrity_passed: bool,
        now: UnixMillis,
    ) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::RecoveryRequired {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = if clone_integrity_passed {
            LocalGenerationState::MaterializedClean
        } else {
            LocalGenerationState::Quarantined
        };
        self.last_activity_at = now;
        self.locked = false;
        Ok(())
    }

    pub fn mark_synced(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::DirtyLocal {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::SyncedEvictable;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn mark_superseded(&mut self, now: UnixMillis) -> Result<(), LocalProfileError> {
        if self.state != LocalGenerationState::DirtyLocal {
            return Err(LocalProfileError::InvalidTransition);
        }
        ensure_monotonic(self.last_activity_at, now)?;
        self.state = LocalGenerationState::SupersededEvictable;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn supersede_with_successor(
        &mut self,
        successor_generation_id: GenerationId,
        successor_bytes: u64,
        now: UnixMillis,
    ) -> Result<Self, LocalProfileError> {
        if successor_generation_id == self.generation_id {
            return Err(LocalProfileError::InvalidTransition);
        }
        self.mark_superseded(now)?;
        Ok(Self::new(successor_generation_id, successor_bytes, now))
    }

    pub fn evict(&mut self) -> Result<(), LocalProfileError> {
        if !matches!(
            self.state,
            LocalGenerationState::SyncedEvictable | LocalGenerationState::SupersededEvictable
        ) || self.locked
        {
            return Err(LocalProfileError::InvalidTransition);
        }
        self.state = LocalGenerationState::Evicted;
        self.bytes = 0;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForgottenWindowAction {
    None,
    Warn,
    Drain,
    ForceClose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForgottenWindowPolicy {
    warn_after_ms: u64,
    drain_after_ms: u64,
    hard_ttl_ms: u64,
}

impl ForgottenWindowPolicy {
    pub const fn new(
        warn_after_ms: u64,
        drain_after_ms: u64,
        hard_ttl_ms: u64,
    ) -> Result<Self, LocalProfileError> {
        if warn_after_ms == 0 || warn_after_ms >= drain_after_ms || drain_after_ms >= hard_ttl_ms {
            return Err(LocalProfileError::InvalidPolicy);
        }
        Ok(Self {
            warn_after_ms,
            drain_after_ms,
            hard_ttl_ms,
        })
    }

    pub fn evaluate(
        self,
        generation: &LocalGenerationRecord,
        now: UnixMillis,
    ) -> Result<ForgottenWindowAction, LocalProfileError> {
        if generation.state != LocalGenerationState::InUse {
            return Ok(ForgottenWindowAction::None);
        }
        let started_at = generation
            .session_started_at
            .ok_or(LocalProfileError::InvalidTransition)?;
        let age = elapsed(started_at, now)?;
        let idle = elapsed(generation.last_activity_at, now)?;
        if age >= self.hard_ttl_ms {
            Ok(ForgottenWindowAction::ForceClose)
        } else if idle >= self.drain_after_ms {
            Ok(ForgottenWindowAction::Drain)
        } else if idle >= self.warn_after_ms {
            Ok(ForgottenWindowAction::Warn)
        } else {
            Ok(ForgottenWindowAction::None)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuotaPlan {
    total_bytes: u64,
    bytes_to_reclaim: u64,
    reclaimable_bytes: u64,
    candidates: Vec<GenerationId>,
}

impl QuotaPlan {
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    #[must_use]
    pub const fn bytes_to_reclaim(&self) -> u64 {
        self.bytes_to_reclaim
    }

    #[must_use]
    pub const fn reclaimable_bytes(&self) -> u64 {
        self.reclaimable_bytes
    }

    #[must_use]
    pub fn candidates(&self) -> &[GenerationId] {
        &self.candidates
    }

    #[must_use]
    pub const fn is_satisfied(&self) -> bool {
        self.reclaimable_bytes >= self.bytes_to_reclaim
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotaPolicy {
    maximum_bytes: u64,
}

impl QuotaPolicy {
    pub const fn new(maximum_bytes: u64) -> Result<Self, LocalProfileError> {
        if maximum_bytes == 0 {
            return Err(LocalProfileError::InvalidPolicy);
        }
        Ok(Self { maximum_bytes })
    }

    pub fn plan(
        self,
        generations: &[LocalGenerationRecord],
    ) -> Result<QuotaPlan, LocalProfileError> {
        let mut total_bytes = 0_u64;
        for generation in generations {
            total_bytes = total_bytes
                .checked_add(generation.bytes)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
        }
        let bytes_to_reclaim = total_bytes.saturating_sub(self.maximum_bytes);
        let mut eligible = generations
            .iter()
            .filter(|generation| {
                matches!(
                    generation.state,
                    LocalGenerationState::SyncedEvictable
                        | LocalGenerationState::SupersededEvictable
                ) && !generation.locked
            })
            .collect::<Vec<_>>();
        eligible.sort_by(|left, right| {
            left.last_activity_at
                .cmp(&right.last_activity_at)
                .then_with(|| left.generation_id.cmp(&right.generation_id))
        });

        let mut reclaimable_bytes = 0_u64;
        let mut candidates = Vec::new();
        for generation in eligible {
            if reclaimable_bytes >= bytes_to_reclaim {
                break;
            }
            reclaimable_bytes = reclaimable_bytes
                .checked_add(generation.bytes)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
            candidates.push(generation.generation_id.clone());
        }
        Ok(QuotaPlan {
            total_bytes,
            bytes_to_reclaim,
            reclaimable_bytes,
            candidates,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportBundleSummary {
    total_generations: u64,
    total_bytes: u64,
    state_counts: BTreeMap<LocalGenerationState, u64>,
    inventory_failures: u64,
}

impl SupportBundleSummary {
    pub fn from_records(
        records: &[LocalGenerationRecord],
        inventory_failures: u64,
    ) -> Result<Self, LocalProfileError> {
        let mut total_bytes = 0_u64;
        let mut state_counts = BTreeMap::new();
        for record in records {
            total_bytes = total_bytes
                .checked_add(record.bytes)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
            let count = state_counts.entry(record.state).or_insert(0_u64);
            *count = count
                .checked_add(1)
                .ok_or(LocalProfileError::InventorySizeOverflow)?;
        }
        let total_generations =
            u64::try_from(records.len()).map_err(|_| LocalProfileError::InventorySizeOverflow)?;
        Ok(Self {
            total_generations,
            total_bytes,
            state_counts,
            inventory_failures,
        })
    }

    #[must_use]
    pub fn render_metadata_only(&self) -> String {
        let mut output = format!(
            "schema=local-profile-support-v1\ntotal_generations={}\ntotal_bytes={}\ninventory_failures={}\n",
            self.total_generations, self.total_bytes, self.inventory_failures
        );
        for (state, count) in &self.state_counts {
            output.push_str(&format!("state.{}={}\n", state.as_str(), count));
        }
        output
    }
}

fn ensure_monotonic(previous: UnixMillis, now: UnixMillis) -> Result<(), LocalProfileError> {
    if now < previous {
        Err(LocalProfileError::ClockRegression)
    } else {
        Ok(())
    }
}

fn elapsed(previous: UnixMillis, now: UnixMillis) -> Result<u64, LocalProfileError> {
    now.value()
        .checked_sub(previous.value())
        .ok_or(LocalProfileError::ClockRegression)
}

#[cfg(test)]
mod tests {
    use super::{LocalGenerationRecord, LocalGenerationState, QuotaPolicy};
    use crate::local_profile::LocalProfileError;
    use profile_platform_primitives::{GenerationId, UnixMillis};

    #[test]
    fn superseded_base_cannot_reopen_and_successor_has_exact_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let base_id = GenerationId::parse("generation_local_successor_base_01")?;
        let candidate_id = GenerationId::parse("generation_local_successor_candidate_01")?;
        let mut base = LocalGenerationRecord::new(base_id.clone(), 128, UnixMillis::new(10));
        base.set_locked(true)?;
        base.begin_use(UnixMillis::new(11))?;
        base.graceful_close(UnixMillis::new(12))?;

        let candidate = base.supersede_with_successor(candidate_id.clone(), 256, UnixMillis::new(13))?;
        assert_eq!(base.generation_id(), &base_id);
        assert_eq!(base.state(), LocalGenerationState::SupersededEvictable);
        assert_eq!(candidate.generation_id(), &candidate_id);
        assert_eq!(candidate.state(), LocalGenerationState::MaterializedClean);
        assert_eq!(candidate.bytes(), 256);
        assert_eq!(base.begin_use(UnixMillis::new(14)), Err(LocalProfileError::InvalidTransition));

        base.set_locked(false)?;
        let plan = QuotaPolicy::new(256)?.plan(&[base.clone(), candidate])?;
        assert_eq!(plan.candidates(), [base_id]);
        base.evict()?;
        assert_eq!(base.state(), LocalGenerationState::Evicted);
        Ok(())
    }

    #[test]
    fn successor_must_use_a_new_generation_identity() -> Result<(), Box<dyn std::error::Error>> {
        let base_id = GenerationId::parse("generation_local_successor_same_01")?;
        let mut base = LocalGenerationRecord::new(base_id.clone(), 128, UnixMillis::new(10));
        base.set_locked(true)?;
        base.begin_use(UnixMillis::new(11))?;
        base.graceful_close(UnixMillis::new(12))?;

        assert_eq!(
            base.supersede_with_successor(base_id, 128, UnixMillis::new(13)),
            Err(LocalProfileError::InvalidTransition)
        );
        assert_eq!(base.state(), LocalGenerationState::DirtyLocal);
        Ok(())
    }
}
