use super::{
    DeliveryActivationGuard, DeliveryActivationOutcome, DeliveryHandoffCoordinator,
    DeliveryHandoffError, DeliveryHandoffEvidence, DeliveryHandoffKind, DeliveryHandoffOutcome,
    canonical_regular_executable,
};
use std::path::Path;

impl DeliveryHandoffCoordinator {
    /// Complete the first installation from the exact verified staged executable already running.
    ///
    /// There is no previous process/release to impersonate or transfer from. The existing journal
    /// atomically records activation and this actual target's arrival. Runtime health remains pending
    /// until the normal shipping preflight confirms it. No rollback source is invented.
    pub fn complete_first_installation(
        &self,
        guard: &DeliveryActivationGuard,
        current_executable: impl AsRef<Path>,
    ) -> Result<(), DeliveryHandoffError> {
        let mut store = self.open_store()?;
        let state = store.state();
        state
            .validate_persisted()
            .map_err(DeliveryHandoffError::State)?;

        if let Some(active) = state.active() {
            // An installer restart can only acknowledge this exact completed first installation.
            // It cannot reset an interrupted health attempt, recover a failure or replay an upgrade.
            let arrived = store.handoff().is_some_and(|evidence| {
                evidence.kind() == DeliveryHandoffKind::Activation
                    && evidence.outcome() == DeliveryHandoffOutcome::Arrived
                    && evidence.source_candidate() == active
                    && evidence.target() == active
                    && evidence.source_attempt() == 1
            });
            let retryable = state.activation_generation() == 1
                && state.last_known_good().is_none()
                && state.staged().is_none()
                && state.last_failure().is_none()
                && state.last_activation().is_some_and(|activation| {
                    activation.attempt == 1
                        && activation.candidate == *active
                        && matches!(
                            activation.outcome,
                            DeliveryActivationOutcome::PendingHealth
                                | DeliveryActivationOutcome::Healthy
                        )
                });
            if !arrived || !retryable {
                return Err(DeliveryHandoffError::FirstInstallationConflict);
            }
            return self.verify_current_identity(active, current_executable.as_ref());
        }

        if state.activation_generation() != 0
            || state.last_known_good().is_some()
            || state.last_activation().is_some()
            || state.last_failure().is_some()
            || store.handoff().is_some()
        {
            return Err(DeliveryHandoffError::FirstInstallationConflict);
        }
        let identity = state
            .staged()
            .cloned()
            .ok_or(DeliveryHandoffError::NoStagedTarget)?;
        // Reopen through the existing staging owner, including all file sizes and hashes.
        let target = self.target_for_identity(&identity)?;
        let current = canonical_regular_executable(current_executable.as_ref())?;
        if target.profile_bridge_executable() != current {
            return Err(DeliveryHandoffError::CurrentExecutableMismatch);
        }

        let mut next = state.clone();
        next.activate_staged(guard.confirms_quiescence())
            .map_err(DeliveryHandoffError::State)?;
        let arrival = DeliveryHandoffEvidence::activation_started(&next, &identity)
            .and_then(|started| started.arrived(&next))
            .map_err(DeliveryHandoffError::Store)?;
        self.persist_handoff_exact(&mut store, &next, arrival)
    }
}
