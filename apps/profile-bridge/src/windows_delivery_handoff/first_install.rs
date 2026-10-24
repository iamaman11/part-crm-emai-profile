use super::{DeliveryHandoffError, DeliveryHandoffOutcome};
#[cfg(any(windows, test))]
use std::path::Path;

/// Admit pristine setup or an arrived installation; interrupted health needs a newer target.
pub(crate) fn installation_candidate(
    state: &crate::windows_delivery::DeliveryState,
    handoff: Option<&crate::windows_delivery_store::DeliveryHandoffEvidence>,
    target: Option<&crate::windows_delivery::DeliveryIdentity>,
) -> Result<Option<crate::windows_delivery::DeliveryIdentity>, DeliveryHandoffError> {
    use crate::windows_delivery::DeliveryActivationOutcome;
    use crate::windows_delivery_store::DeliveryHandoffKind;
    state
        .validate_persisted()
        .map_err(DeliveryHandoffError::State)?;
    if let Some(active) = state.active() {
        // An interrupted runtime may be repaired, but never by resetting the same candidate.
        // The preparation caller verifies this target; completion reopens its exact staged files.
        // Neither caller may replace an unfinished process transfer.
        let arrived = handoff.is_some_and(|evidence| {
            evidence.kind() == DeliveryHandoffKind::Activation
                && evidence.outcome() == DeliveryHandoffOutcome::Arrived
                && evidence.source_candidate() == active
                && evidence.target() == active
                && evidence.source_attempt() == state.activation_generation()
        });
        let installable = state.last_failure().is_none()
            && state.last_activation().is_some_and(|activation| {
                activation.attempt == state.activation_generation()
                    && activation.candidate == *active
                    && match activation.outcome {
                        DeliveryActivationOutcome::PendingHealth
                        | DeliveryActivationOutcome::Healthy => true,
                        DeliveryActivationOutcome::HealthAttemptStarted => {
                            target.is_some_and(|target| target.sequence > active.sequence)
                        }
                        _ => false,
                    }
            });
        if !arrived || !installable {
            return Err(DeliveryHandoffError::FirstInstallationConflict);
        }
        return Ok(Some(active.clone()));
    }

    if state.activation_generation() != 0
        || state.last_known_good().is_some()
        || state.last_activation().is_some()
        || state.last_failure().is_some()
        || handoff.is_some()
    {
        return Err(DeliveryHandoffError::FirstInstallationConflict);
    }
    Ok(None)
}

#[cfg(any(windows, test))]
use super::{
    DeliveryActivationGuard, DeliveryHandoffCoordinator, DeliveryHandoffEvidence,
    canonical_regular_executable,
};
#[cfg(any(windows, test))]
impl DeliveryHandoffCoordinator {
    /// Complete setup from the exact verified staged executable already running under quiescence.
    ///
    /// The installer starts the actual target, without impersonating an old process. The journal
    /// atomically records activation and this actual target's arrival. Runtime health remains pending
    /// until the normal shipping preflight confirms it. No rollback source is invented.
    pub fn complete_installation(
        &self,
        guard: &DeliveryActivationGuard,
        current_executable: impl AsRef<Path>,
    ) -> Result<(), DeliveryHandoffError> {
        let mut store = self.open_store()?;
        let state = store.state();
        let active =
            installation_candidate(state, store.handoff(), state.staged().or(state.active()))?;
        if let Some(active) = active
            && state.staged().is_none()
        {
            return self.verify_current_identity(&active, current_executable.as_ref());
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
