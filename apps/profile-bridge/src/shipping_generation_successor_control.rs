#[cfg(any(windows, test))]
use crate::browser_execution::{browser_writer_locks_absent, persist_committed_successor_binding};
#[cfg(any(windows, test))]
use crate::dirty_generation::recover_local_unsynced_native_clone;
use crate::dirty_generation::{
    GenerationSealingMaterial, GenerationSealingMaterialPort, PreparedDirtyGeneration,
};
use crate::local_profile::MaterializationRoot;
#[cfg(any(windows, test))]
use crate::local_profile::{BridgeWorkspaceLock, LocalGenerationRecord, LocalProfileError};
use crate::shipping_control_plane::{
    MachineHttpMethod, MachineHttpPort, ShippingControlPlaneError,
};
#[cfg(any(windows, test))]
use crate::shipping_generation_save::{
    CommittedGenerationSuccessor, SignedGenerationObjectStreamPutPort,
};
use crate::shipping_generation_save::{
    GenerationSuccessorCommitOutcome, GenerationSuccessorControlPort,
    GenerationUploadAuthorization, SignedGenerationUploadCapability,
};
#[cfg(any(windows, test))]
use crate::shipping_save_attempt::{ReadOnlySaveAttempt, discover_unresolved_exact_save_attempt};
use crate::shipping_save_attempt::{
    ReadOnlyStreamedSaveAttempt, discover_unresolved_streaming_exact_save_attempt,
    inspect_streaming_exact_save_attempt,
};
#[cfg(any(windows, test))]
use crate::shipping_save_attempt::{
    ScopedPendingSaveEvidence, classify_scoped_pending_save_evidence,
    deterministic_lone_unsynced_successor, local_unsynced_matches_fresh_lease,
    persist_streaming_exact_save_attempt_from_lone_intent,
    retire_active_exact_attempt_after_restart, retire_active_streamed_attempt_after_restart,
    retire_matching_local_unsynced_after_restart,
    retire_matching_streamed_local_unsynced_after_restart,
};
use control_plane_contract::profile_generation_api::{
    BRIDGE_PROFILE_GENERATION_COMMIT_PATH_TEMPLATE,
    BRIDGE_PROFILE_GENERATION_OBSERVE_PATH_TEMPLATE,
    BRIDGE_PROFILE_GENERATION_UPLOAD_CAPABILITY_PATH_TEMPLATE,
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationSuccessorObserveRequest, BridgeGenerationSuccessorObserveResponse,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
#[cfg(any(windows, test))]
use control_plane_contract::profile_generation_api::{
    BRIDGE_PROFILE_GENERATION_LOCAL_UNSYNCED_RETRY_DECISION_PATH_TEMPLATE,
    BRIDGE_PROFILE_GENERATION_RETRY_DECISION_PATH_TEMPLATE,
    BridgeGenerationLocalUnsyncedRetryDecisionRequest,
    BridgeGenerationSuccessorRetryDecisionRequest, BridgeGenerationSuccessorRetryDecisionResponse,
    BridgeGenerationSuccessorRetryVerdict,
};
#[cfg(any(windows, test))]
use profile_platform_primitives::UnixMillis;
use profile_platform_primitives::{CorrelationId, GenerationId, ProfileId, TenantId};
use session_domain::ProfileLease;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_UPLOAD_CAPABILITY_RESPONSE_BYTES: usize = 16_384;
const MAX_COMMIT_RESPONSE_BYTES: usize = 1_024;
const MAX_OBSERVE_RESPONSE_BYTES: usize = 256;
const MAX_SIGNED_UPLOAD_URL_BYTES: usize = 8_192;
const MAX_SIGNED_UPLOAD_HEADERS: usize = 16;
const MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES: usize = 128;
const MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES: usize = 1_024;
const MAX_UPLOAD_CAPABILITY_EXPIRES_SECONDS: u32 = 300;
static SAVE_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Evidence-driven post-commit local completion, never a new server write grant.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartExactLocalCompletion {
    NoCanonicalCheckpoint,
    /// Verified local source exists, but no cloud effect has been proved.
    /// Requires a *new* D1 affirmative decision and fresh writer fence.
    LocalUnsyncedAuthorizedAwaitingFirstSeal,
    NotActiveExact,
    /// D1 positively proved nonapplication; exact same-attempt resume is still pending.
    RetryDecisionConfirmedAwaitingResumption,
    RetryDecisionBlocked,
    ServerSelectedDifferentGeneration,
    ActiveExactLocalReady,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub enum RestartExactLocalCompletionError {
    LocalEvidenceChanged,
    WriterOrLockUnresolved,
    BindingOrRetirementFailed,
    /// Signed PUT or commit outcome is uncertain; never discard original bytes.
    UploadOrCommitUncertain,
    ControlPlane(ShippingControlPlaneError),
}

#[cfg(any(windows, test))]
impl RestartExactLocalCompletionError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::LocalEvidenceChanged => "P3_RECOVERY_LOCAL_EVIDENCE_CHANGED",
            Self::WriterOrLockUnresolved => "P3_RECOVERY_WRITER_UNRESOLVED",
            Self::BindingOrRetirementFailed => "P3_RECOVERY_LOCAL_COMPLETION_BLOCKED",
            Self::UploadOrCommitUncertain => "P3_RECOVERY_UPLOAD_OR_COMMIT_UNCERTAIN",
            Self::ControlPlane(_) => "P3_RECOVERY_CLOUD_OBSERVATION_FAILED",
        }
    }
}

/// Read-only evidence about the exact retained save attempt after restart.
/// No variant is a retry capability, a new lease, local browser readiness,
/// or a claim that `CLOUD_SAVED` has been reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartExactCommitObservation {
    /// No canonical exact checkpoint was discovered. This does NOT overrule
    /// the independent conservative guard for torn/pending siblings.
    NoCanonicalCheckpoint,
    /// Exact immutable successor is authoritative ACTIVE in D1/R2. Local
    /// readiness, owned cleanup and checkpoint retirement are still required.
    ActiveExact,
    /// NOT ACTIVE_EXACT: may mean a foreign/newer pointer, a changed record
    /// or an unapplied attempt. NEVER treat this as retry authorization.
    NotActiveExact,
}

#[derive(Debug, Eq, PartialEq)]
pub enum GenerationSuccessorControlError<S> {
    Sealing(S),
    ControlPlane(ShippingControlPlaneError),
}

impl<S: fmt::Display> fmt::Display for GenerationSuccessorControlError<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sealing(error) => {
                write!(formatter, "generation sealing material failed: {error}")
            }
            Self::ControlPlane(error) => {
                write!(formatter, "generation successor control failed: {error}")
            }
        }
    }
}

impl<S: std::error::Error> std::error::Error for GenerationSuccessorControlError<S> {}

/// Typed machine client for the P3 successor endpoints.
///
/// It is intentionally not a coordinator. The live coordinator remains the sole lease/fence and
/// sealing-material owner; this adapter only serializes the canonical successor request and
/// delegates sealing material back to that existing owner.
pub struct ControlPlaneGenerationSuccessor<'a, T, S> {
    transport: T,
    sealer: &'a mut S,
}

impl<'a, T, S> ControlPlaneGenerationSuccessor<'a, T, S> {
    #[must_use]
    pub const fn new(transport: T, sealer: &'a mut S) -> Self {
        Self { transport, sealer }
    }
}

impl<T, S> GenerationSealingMaterialPort for ControlPlaneGenerationSuccessor<'_, T, S>
where
    S: GenerationSealingMaterialPort,
{
    type Error = GenerationSuccessorControlError<S::Error>;

    fn material_for(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        generation_id: &GenerationId,
        plaintext_digest: [u8; 32],
    ) -> Result<GenerationSealingMaterial, Self::Error> {
        self.sealer
            .material_for(
                tenant_id,
                profile_id,
                base_generation_id,
                generation_id,
                plaintext_digest,
            )
            .map_err(GenerationSuccessorControlError::Sealing)
    }
}

impl<T, S> GenerationSuccessorControlPort for ControlPlaneGenerationSuccessor<'_, T, S>
where
    T: MachineHttpPort,
    S: GenerationSealingMaterialPort,
{
    fn upload_authorization(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        prepared: &PreparedDirtyGeneration,
        lease: &ProfileLease,
    ) -> Result<GenerationUploadAuthorization, Self::Error> {
        let request = successor_request(base_generation_id, prepared, lease)?;
        let response = post_json(
            &mut self.transport,
            &successor_path(
                BRIDGE_PROFILE_GENERATION_UPLOAD_CAPABILITY_PATH_TEMPLATE,
                tenant_id,
                profile_id,
            ),
            &request,
        )?;
        if response.len() > MAX_UPLOAD_CAPABILITY_RESPONSE_BYTES {
            return Err(control_error(ShippingControlPlaneError::InvalidResponse));
        }
        let decoded = serde_json::from_slice::<BridgeGenerationUploadCapabilityResponse>(&response)
            .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
        decode_upload_authorization(decoded)
    }

    fn commit_successor(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        prepared: &PreparedDirtyGeneration,
        lease: &ProfileLease,
    ) -> Result<GenerationSuccessorCommitOutcome, Self::Error> {
        let request = successor_request(base_generation_id, prepared, lease)?;
        let response = post_json(
            &mut self.transport,
            &successor_path(
                BRIDGE_PROFILE_GENERATION_COMMIT_PATH_TEMPLATE,
                tenant_id,
                profile_id,
            ),
            &request,
        )?;
        if response.len() > MAX_COMMIT_RESPONSE_BYTES {
            return Err(control_error(ShippingControlPlaneError::InvalidResponse));
        }
        let decoded = serde_json::from_slice::<BridgeGenerationSuccessorCommitResponse>(&response)
            .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
        Ok(match decoded.outcome {
            BridgeGenerationSuccessorCommitOutcomeDto::Activated => {
                GenerationSuccessorCommitOutcome::Activated
            }
            BridgeGenerationSuccessorCommitOutcomeDto::AlreadyActive => {
                GenerationSuccessorCommitOutcome::AlreadyActive
            }
        })
    }
}

/// Discover one durable exact checkpoint without requiring a surviving in-memory
/// successor/lease and observe the server's exact active-generation descriptor.
/// Only the signed-in machine transport is used; this function NEVER replays the
/// predecessor fence and NEVER uploads, commits, deletes or infers Saved. A
/// a NOT ACTIVE_EXACT observation leaves original bytes and attempt untouched.
/// This result cannot authorize a retry; server-positive authorization must be
/// implemented separately in the existing D1 owner before any resume/commit.
pub fn observe_discovered_successor_after_restart<T: MachineHttpPort>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    transport: &mut T,
) -> Result<RestartExactCommitObservation, ShippingControlPlaneError> {
    let attempt = discover_unresolved_streaming_exact_save_attempt(root, tenant_id, profile_id)
        .map_err(|_| ShippingControlPlaneError::InvalidResponse)?;
    match attempt {
        Some(attempt) => observe_verified_streamed_attempt(
            tenant_id, profile_id, &attempt, transport,
        )
        .map(|active| {
            if active {
                RestartExactCommitObservation::ActiveExact
            } else {
                RestartExactCommitObservation::NotActiveExact
            }
        }),
        None => Ok(RestartExactCommitObservation::NoCanonicalCheckpoint),
    }
}

/// Complete only an EXACT D1-committed N+1 on the same device, following
/// current ACL/CNG authentication and a FRESH coordinator Profile writer claim.
/// Never invoke PUT, D1 commit, rekey, cloud rollback or a second candidate.
/// Unknown, mutated, foreign or native-writer-owned bytes remain untouched.
#[cfg(any(windows, test))]
pub(crate) fn complete_active_exact_after_restart<T: MachineHttpPort>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    selected_generation: &GenerationId,
    fresh_lease: &ProfileLease,
    transport: &mut T,
) -> Result<RestartExactLocalCompletion, RestartExactLocalCompletionError> {
    if fresh_lease.tenant_id() != tenant_id
        || fresh_lease.profile_id() != profile_id
        || fresh_lease.epoch() == 0
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    // Inspect the ONE canonical BPAT with bounded memory before the legacy
    // lone-intent classifier: a 512 MiB verified checkpoint must not hit the
    // old full-ciphertext 64 MiB read. This is still the SAME completion owner.
    if let Ok(Some(streamed)) =
        discover_unresolved_streaming_exact_save_attempt(root, tenant_id, profile_id)
    {
        return complete_streamed_exact_under_fresh_claim(
            root,
            tenant_id,
            profile_id,
            selected_generation,
            fresh_lease,
            streamed,
            transport,
        );
    }
    let pending = classify_scoped_pending_save_evidence(root, tenant_id, profile_id)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if matches!(pending, ScopedPendingSaveEvidence::Exact(_)) {
        // A legacy Vec inspector may never overrule a failed streamed
        // identity/digest/inventory check. Large BPATs must not allocate.
        return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
    }
    let attempt = match pending {
        ScopedPendingSaveEvidence::None => {
            return Ok(RestartExactLocalCompletion::NoCanonicalCheckpoint);
        }
        ScopedPendingSaveEvidence::LocalUnsynced {
            base_generation,
            prior_epoch,
        } => {
            if fresh_lease.epoch() <= prior_epoch {
                return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
            }
            if &base_generation != selected_generation {
                return Ok(RestartExactLocalCompletion::ServerSelectedDifferentGeneration);
            }
            if !local_unsynced_matches_fresh_lease(
                root,
                tenant_id,
                profile_id,
                &base_generation,
                fresh_lease,
            )
            .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
            {
                return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
            }
            let verdict = request_local_unsynced_retry_decision(
                tenant_id,
                profile_id,
                &base_generation,
                fresh_lease,
                transport,
            )
            .map_err(RestartExactLocalCompletionError::ControlPlane)?;
            // No N+1 or PUT exists: original native bytes and intent remain.
            return Ok(match verdict {
                BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable => {
                    RestartExactLocalCompletion::LocalUnsyncedAuthorizedAwaitingFirstSeal
                }
                BridgeGenerationSuccessorRetryVerdict::Blocked => {
                    RestartExactLocalCompletion::RetryDecisionBlocked
                }
            });
        }
        ScopedPendingSaveEvidence::Exact(attempt) => *attempt,
    };
    if attempt.successor_id() != selected_generation.as_str() {
        if attempt.base_generation_id() != selected_generation.as_str() {
            return Ok(RestartExactLocalCompletion::ServerSelectedDifferentGeneration);
        }
        if fresh_lease.epoch() <= attempt.prior_epoch()
            || !attempt.matches_fresh_device(fresh_lease)
        {
            return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
        }
        let verdict =
            request_fresh_retry_decision(tenant_id, profile_id, &attempt, fresh_lease, transport)
                .map_err(RestartExactLocalCompletionError::ControlPlane)?;
        return Ok(match verdict {
            BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable => {
                RestartExactLocalCompletion::RetryDecisionConfirmedAwaitingResumption
            }
            BridgeGenerationSuccessorRetryVerdict::Blocked => {
                RestartExactLocalCompletion::RetryDecisionBlocked
            }
        });
    }
    let active = observe_verified_exact_attempt(tenant_id, profile_id, &attempt, transport)
        .map_err(RestartExactLocalCompletionError::ControlPlane)?;
    if !active {
        return Ok(RestartExactLocalCompletion::NotActiveExact);
    }

    let base_generation = GenerationId::parse(attempt.base_generation_id())
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let base = root
        .open_generation(tenant_id, profile_id, &base_generation)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let candidate = root
        .open_generation(tenant_id, profile_id, selected_generation)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if !matches!(browser_writer_locks_absent(&base), Ok(true))
        || !matches!(browser_writer_locks_absent(&candidate), Ok(true))
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    // Existing root-wide OS admission and same-device interrupted-lock path.
    // Do not remove browser parent.lock, previous content or another writer.
    let lock =
        match BridgeWorkspaceLock::acquire(&base, fresh_lease.device_id(), fresh_lease.epoch()) {
            Ok(value) => value,
            Err(LocalProfileError::LockBusy) => BridgeWorkspaceLock::recover_interrupted_launch(
                &base,
                fresh_lease.device_id(),
                fresh_lease.epoch(),
            )
            .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?,
            Err(_) => return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        };
    // Always attempt to release the *new* owned marker even when evidence
    // or binding validation fails. Failure keeps the durable checkpoint.
    let local_result = (|| {
        // Revalidate under the OS-backed admission before publishing.
        let verified = discover_unresolved_exact_save_attempt(root, tenant_id, profile_id)
            .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
            .ok_or(RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        if verified.successor_id() != attempt.successor_id()
            || verified.container_digest() != attempt.container_digest()
            || verified.ciphertext() != attempt.ciphertext()
            || !matches!(browser_writer_locks_absent(&base), Ok(true))
            || !matches!(browser_writer_locks_absent(&candidate), Ok(true))
        {
            return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
        }
        // A crash after publishing the immutable ciphertext checkpoint but
        // before retiring LOCAL_UNSYNCED may leave BOTH exact original records.
        // The original source/old-lease fingerprints were checked by the
        // discoverer under this fresh OS admission; D1 ACTIVE_EXACT is proved.
        // Retire the small intent first, keeping the complete checkpoint
        // throughout binding, lock release and final exact retirement.
        retire_matching_local_unsynced_after_restart(root, tenant_id, profile_id, &verified)
            .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        persist_committed_successor_binding(
            &candidate,
            tenant_id,
            profile_id,
            selected_generation,
            verified.container_digest(),
        )
        .map_err(|_| RestartExactLocalCompletionError::BindingOrRetirementFailed)?;
        Ok(verified)
    })();
    lock.release()
        .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?;
    let verified = local_result?;
    retire_active_exact_attempt_after_restart(root, tenant_id, profile_id, &verified)
        .map_err(|_| RestartExactLocalCompletionError::BindingOrRetirementFailed)?;
    Ok(RestartExactLocalCompletion::ActiveExactLocalReady)
}

/// Bounded-memory implementation of the existing restart-completion owner.
/// The server-selected generation, fresh CNG/ACL/coordinator claim, native
/// OS admission and D1 ACTIVE_EXACT still decide whether Open is allowed.
#[cfg(any(windows, test))]
#[allow(clippy::too_many_arguments)]
fn complete_streamed_exact_under_fresh_claim<T: MachineHttpPort>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    selected: &GenerationId,
    fresh_lease: &ProfileLease,
    mut original: ReadOnlyStreamedSaveAttempt,
    transport: &mut T,
) -> Result<RestartExactLocalCompletion, RestartExactLocalCompletionError> {
    if original.successor_id() != selected.as_str() {
        if original.base_generation_id() != selected.as_str() {
            return Ok(RestartExactLocalCompletion::ServerSelectedDifferentGeneration);
        }
        if fresh_lease.epoch() <= original.original_epoch()
            || !original.original_device(fresh_lease)
        {
            return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
        }
        return Ok(
            match request_fresh_retry_decision_streamed(
                tenant_id,
                profile_id,
                &original,
                fresh_lease,
                transport,
            )
            .map_err(RestartExactLocalCompletionError::ControlPlane)?
            {
                BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable => {
                    RestartExactLocalCompletion::RetryDecisionConfirmedAwaitingResumption
                }
                BridgeGenerationSuccessorRetryVerdict::Blocked => {
                    RestartExactLocalCompletion::RetryDecisionBlocked
                }
            },
        );
    }
    if !observe_verified_streamed_attempt(tenant_id, profile_id, &original, transport)
        .map_err(RestartExactLocalCompletionError::ControlPlane)?
    {
        return Ok(RestartExactLocalCompletion::NotActiveExact);
    }
    let base_id = GenerationId::parse(original.base_generation_id())
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let base = root
        .open_generation(tenant_id, profile_id, &base_id)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let candidate = root
        .open_generation(tenant_id, profile_id, selected)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if !matches!(browser_writer_locks_absent(&base), Ok(true))
        || !matches!(browser_writer_locks_absent(&candidate), Ok(true))
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    let lock =
        match BridgeWorkspaceLock::acquire(&base, fresh_lease.device_id(), fresh_lease.epoch()) {
            Ok(value) => value,
            Err(LocalProfileError::LockBusy) => BridgeWorkspaceLock::recover_interrupted_launch(
                &base,
                fresh_lease.device_id(),
                fresh_lease.epoch(),
            )
            .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?,
            Err(_) => return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        };
    let local_result = (|| {
        // Two separately opened verified handles prevent a pathname swap
        // or mutation between pre-claim D1 observation and binding.
        let mut exact =
            discover_unresolved_streaming_exact_save_attempt(root, tenant_id, profile_id)
                .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
                .ok_or(RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        if exact.successor_id() != original.successor_id()
            || exact.base_generation_id() != original.base_generation_id()
            || exact.object_key() != original.object_key()
            || exact.metadata_digest() != original.metadata_digest()
            || exact.container_digest() != original.container_digest()
            || exact.container_bytes() != original.container_bytes()
            || !matches!(browser_writer_locks_absent(&base), Ok(true))
            || !matches!(browser_writer_locks_absent(&candidate), Ok(true))
            || original.ciphertext().recheck_and_rewind().is_err()
            || exact.ciphertext().recheck_and_rewind().is_err()
        {
            return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
        }
        retire_matching_streamed_local_unsynced_after_restart(root, tenant_id, profile_id, &exact)
            .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        persist_committed_successor_binding(
            &candidate,
            tenant_id,
            profile_id,
            selected,
            exact.container_digest(),
        )
        .map_err(|_| RestartExactLocalCompletionError::BindingOrRetirementFailed)?;
        Ok(exact)
    })();
    lock.release()
        .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?;
    let mut verified = local_result?;
    retire_active_streamed_attempt_after_restart(
        root,
        tenant_id,
        profile_id,
        selected,
        &mut verified,
    )
    .map_err(|_| RestartExactLocalCompletionError::BindingOrRetirementFailed)?;
    Ok(RestartExactLocalCompletion::ActiveExactLocalReady)
}

/// First-seal the ONE original locally durable, unsealed close only after a
/// fresh same-device D1-positive base proof. An interrupted complete clone may
/// be reused only if its exact original native inventory still matches.
/// Never delete an incomplete clone, invent N+2, replay old fence or start a
/// browser. The resulting checkpoint keeps original intent provenance so a
/// second power cut can safely resume the same N+1 through the existing
/// exact-checkpoint owner. This uses the CURRENT coordinator as sealer.
#[cfg(any(windows, test))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn resume_lone_local_unsynced_under_fresh_lease<T, U, K>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    selected_generation: &GenerationId,
    fresh_lease: &ProfileLease,
    sealer: &mut K,
    transport: &mut T,
    upload: &mut U,
) -> Result<bool, RestartExactLocalCompletionError>
where
    T: MachineHttpPort,
    U: SignedGenerationObjectStreamPutPort,
    K: GenerationSealingMaterialPort,
{
    let selected = classify_scoped_pending_save_evidence(root, tenant_id, profile_id)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let ScopedPendingSaveEvidence::LocalUnsynced {
        base_generation,
        prior_epoch,
    } = selected
    else {
        return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
    };
    if base_generation != *selected_generation
        || fresh_lease.tenant_id() != tenant_id
        || fresh_lease.profile_id() != profile_id
        || fresh_lease.epoch() <= prior_epoch
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    let candidate_id = deterministic_lone_unsynced_successor(
        root,
        tenant_id,
        profile_id,
        selected_generation,
        fresh_lease,
    )
    .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
    .ok_or(RestartExactLocalCompletionError::WriterOrLockUnresolved)?;
    let source = root
        .open_generation(tenant_id, profile_id, selected_generation)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if !matches!(browser_writer_locks_absent(&source), Ok(true)) {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    let lock =
        match BridgeWorkspaceLock::acquire(&source, fresh_lease.device_id(), fresh_lease.epoch()) {
            Ok(lock) => lock,
            Err(LocalProfileError::LockBusy) => BridgeWorkspaceLock::recover_interrupted_launch(
                &source,
                fresh_lease.device_id(),
                fresh_lease.epoch(),
            )
            .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?,
            Err(_) => return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        };
    let result = (|| {
        // A stale D1 read or pre-lock native inventory is never a write grant.
        // Reinspect exact scoped evidence under the existing OS admission.
        if !matches!(
            classify_scoped_pending_save_evidence(root, tenant_id, profile_id),
            Ok(ScopedPendingSaveEvidence::LocalUnsynced {
                base_generation: ref base,
                prior_epoch: previous
            }) if base == selected_generation && previous == prior_epoch
        ) || !matches!(browser_writer_locks_absent(&source), Ok(true))
            || deterministic_lone_unsynced_successor(
                root,
                tenant_id,
                profile_id,
                selected_generation,
                fresh_lease,
            )
            .ok()
            .flatten()
            .as_ref()
                != Some(&candidate_id)
        {
            return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
        }
        if request_local_unsynced_retry_decision(
            tenant_id,
            profile_id,
            selected_generation,
            fresh_lease,
            transport,
        )
        .map_err(RestartExactLocalCompletionError::ControlPlane)?
            != BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable
        {
            return Ok(false);
        }
        let inventory = source
            .inventory()
            .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        let record = LocalGenerationRecord::from_verified_unsynced_close(
            selected_generation.clone(),
            inventory.total_bytes(),
            UnixMillis::new(0),
        );
        let candidate = recover_local_unsynced_native_clone(
            &record,
            &source,
            root,
            tenant_id,
            profile_id,
            &candidate_id,
        )
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        // First seal under CURRENT D1-positive lease. Original intent stays
        // until ACTIVE_EXACT local completion; torn BPAT stage is a blocker.
        persist_streaming_exact_save_attempt_from_lone_intent(
            root,
            &source,
            &candidate,
            fresh_lease,
            sealer,
        )
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        let mut verified =
            discover_unresolved_streaming_exact_save_attempt(root, tenant_id, profile_id)
                .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
                .ok_or(RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        if verified.successor_id() != candidate_id.as_str()
            || verified.base_generation_id() != selected_generation.as_str()
            || !verified.first_seal_under_current_lease(fresh_lease)
            || !verified.original_device(fresh_lease)
            || verified.ciphertext().recheck_and_rewind().is_err()
        {
            return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
        }
        Ok(true)
    })();
    // Never retain a synthetic writer marker across a failure. If releasing
    // its exact ownership fails, checkpoint/intent remain a hard blocker.
    lock.release()
        .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?;
    if !result? {
        return Ok(false);
    }
    // Perform ordinary freshly fenced upload and atomic D1 commit through the
    // EXISTING exact-checkpoint owner (which takes its own OS admission).
    // The source-N launch still remains blocked until next ACTIVE_EXACT claim.
    resume_exact_successor_under_fresh_lease(
        root,
        tenant_id,
        profile_id,
        selected_generation,
        fresh_lease,
        transport,
        upload,
    )
}

/// Resume ONLY the original unchanged N+1 ciphertext under a fresh writer.
/// The D1-positive retry decision is re-checked while the existing Bridge OS
/// workspace admission is held. Upload and commit perform their independent
/// live ACL/fence/CAS validation. Even after a successful commit, the original
/// checkpoint is retained; a NEW server-selected N+1 claim must complete
/// ACTIVE_EXACT local binding before any browser Open or Saved is allowed.
/// Never reconstruct a candidate, reuse an old fence or return a signed URL.
#[cfg(any(windows, test))]
pub(crate) fn resume_exact_successor_under_fresh_lease<T, U>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    selected_generation: &GenerationId,
    fresh_lease: &ProfileLease,
    transport: &mut T,
    upload: &mut U,
) -> Result<bool, RestartExactLocalCompletionError>
where
    T: MachineHttpPort,
    U: SignedGenerationObjectStreamPutPort,
{
    if fresh_lease.tenant_id() != tenant_id
        || fresh_lease.profile_id() != profile_id
        || fresh_lease.epoch() == 0
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    let mut original =
        discover_unresolved_streaming_exact_save_attempt(root, tenant_id, profile_id)
            .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
            .ok_or(RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if original.base_generation_id() != selected_generation.as_str()
        || (fresh_lease.epoch() <= original.original_epoch()
            && !original.first_seal_under_current_lease(fresh_lease))
        || !original.original_device(fresh_lease)
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    let source = root
        .open_generation(tenant_id, profile_id, selected_generation)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let candidate_id = GenerationId::parse(original.successor_id())
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let candidate = root
        .open_generation(tenant_id, profile_id, &candidate_id)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if !matches!(browser_writer_locks_absent(&source), Ok(true))
        || !matches!(browser_writer_locks_absent(&candidate), Ok(true))
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    let lock =
        match BridgeWorkspaceLock::acquire(&source, fresh_lease.device_id(), fresh_lease.epoch()) {
            Ok(lock) => lock,
            Err(LocalProfileError::LockBusy) => BridgeWorkspaceLock::recover_interrupted_launch(
                &source,
                fresh_lease.device_id(),
                fresh_lease.epoch(),
            )
            .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?,
            Err(_) => return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        };
    let result = (|| {
        // No cloud effect may be based on pre-lock evidence alone.
        let mut exact =
            discover_unresolved_streaming_exact_save_attempt(root, tenant_id, profile_id)
                .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
                .ok_or(RestartExactLocalCompletionError::LocalEvidenceChanged)?;
        if exact.base_generation_id() != original.base_generation_id()
            || exact.successor_id() != original.successor_id()
            || exact.object_key() != original.object_key()
            || exact.metadata_digest() != original.metadata_digest()
            || exact.container_digest() != original.container_digest()
            || !matches!(browser_writer_locks_absent(&source), Ok(true))
            || !matches!(browser_writer_locks_absent(&candidate), Ok(true))
        {
            return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
        }
        // Both original file handles were independently SHA-256 verified
        // under the current Bridge OS writer admission.
        if original.ciphertext().recheck_and_rewind().is_err()
            || exact.ciphertext().recheck_and_rewind().is_err()
            || original.container_bytes() != exact.container_bytes()
        {
            return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
        }
        Ok(publish_streamed_exact_under_writer(
            tenant_id,
            profile_id,
            selected_generation,
            &mut exact,
            fresh_lease,
            transport,
            upload,
        )?
        .is_some())
    })();
    lock.release()
        .map_err(|_| RestartExactLocalCompletionError::WriterOrLockUnresolved)?;
    result
}

/// Upload and commit exactly the already verified BPAT ciphertext while the
/// ONE existing Bridge writer admission is held. Neither this helper nor the
/// D1 retry decision is a second lease or storage state machine. Current
/// ACL/fence/pointer and independent R2 verification remain server-owned.
#[cfg(any(windows, test))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_streamed_exact_under_writer<T, U>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    selected_generation: &GenerationId,
    exact: &mut ReadOnlyStreamedSaveAttempt,
    fresh_lease: &ProfileLease,
    transport: &mut T,
    upload: &mut U,
) -> Result<Option<CommittedGenerationSuccessor>, RestartExactLocalCompletionError>
where
    T: MachineHttpPort,
    U: SignedGenerationObjectStreamPutPort,
{
    if fresh_lease.tenant_id() != tenant_id
        || fresh_lease.profile_id() != profile_id
        || fresh_lease.epoch() == 0
        || exact.base_generation_id() != selected_generation.as_str()
        || !exact.original_device(fresh_lease)
        || (fresh_lease.epoch() < exact.original_epoch())
        || (fresh_lease.epoch() == exact.original_epoch() && !exact.original_lease(fresh_lease))
    {
        return Err(RestartExactLocalCompletionError::WriterOrLockUnresolved);
    }
    exact
        .ciphertext()
        .recheck_and_rewind()
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if request_fresh_retry_decision_streamed(tenant_id, profile_id, exact, fresh_lease, transport)
        .map_err(RestartExactLocalCompletionError::ControlPlane)?
        != BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable
    {
        return Ok(None);
    }
    let request = BridgeProfileGenerationSuccessorRequest::new(
        exact.base_generation_id(),
        exact.successor_id(),
        exact.object_key(),
        exact.metadata_digest(),
        exact.container_digest(),
        exact.container_bytes(),
        fresh_lease.session_id().as_str(),
        fresh_lease.fencing_token().as_str(),
        fresh_lease.epoch(),
    );
    let authorization = |transport: &mut T| {
        let bytes = post_json::<T, core::convert::Infallible, _>(
            transport,
            &successor_path(
                BRIDGE_PROFILE_GENERATION_UPLOAD_CAPABILITY_PATH_TEMPLATE,
                tenant_id,
                profile_id,
            ),
            &request,
        )
        .map_err(|error| match error {
            GenerationSuccessorControlError::ControlPlane(reason) => {
                RestartExactLocalCompletionError::ControlPlane(reason)
            }
            GenerationSuccessorControlError::Sealing(never) => match never {},
        })?;
        if bytes.len() > MAX_UPLOAD_CAPABILITY_RESPONSE_BYTES {
            return Err(RestartExactLocalCompletionError::ControlPlane(
                ShippingControlPlaneError::InvalidResponse,
            ));
        }
        let decoded = serde_json::from_slice::<BridgeGenerationUploadCapabilityResponse>(&bytes)
            .map_err(|_| {
                RestartExactLocalCompletionError::ControlPlane(
                    ShippingControlPlaneError::InvalidResponse,
                )
            })?;
        decode_upload_authorization::<core::convert::Infallible>(decoded).map_err(|error| {
            match error {
                GenerationSuccessorControlError::ControlPlane(reason) => {
                    RestartExactLocalCompletionError::ControlPlane(reason)
                }
                GenerationSuccessorControlError::Sealing(never) => match never {},
            }
        })
    };
    match authorization(transport)? {
        GenerationUploadAuthorization::Verified => {}
        GenerationUploadAuthorization::UploadRequired(capability) => {
            upload
                .put_verified_stream(&capability, exact.ciphertext())
                .map_err(|_| RestartExactLocalCompletionError::UploadOrCommitUncertain)?;
            if !matches!(
                authorization(transport)?,
                GenerationUploadAuthorization::Verified
            ) {
                return Err(RestartExactLocalCompletionError::UploadOrCommitUncertain);
            }
        }
    }
    // Existing Worker independently rechecks the exact uploaded R2
    // descriptor and fresh coordinator fence before its atomic D1 commit.
    let response = post_json::<T, core::convert::Infallible, _>(
        transport,
        &successor_path(
            BRIDGE_PROFILE_GENERATION_COMMIT_PATH_TEMPLATE,
            tenant_id,
            profile_id,
        ),
        &request,
    )
    .map_err(|error| match error {
        GenerationSuccessorControlError::ControlPlane(reason) => {
            RestartExactLocalCompletionError::ControlPlane(reason)
        }
        GenerationSuccessorControlError::Sealing(never) => match never {},
    })?;
    if response.len() > MAX_COMMIT_RESPONSE_BYTES {
        return Err(RestartExactLocalCompletionError::ControlPlane(
            ShippingControlPlaneError::InvalidResponse,
        ));
    }
    let committed = serde_json::from_slice::<BridgeGenerationSuccessorCommitResponse>(&response)
        .map_err(|_| {
            RestartExactLocalCompletionError::ControlPlane(
                ShippingControlPlaneError::InvalidResponse,
            )
        })?;
    exact
        .ciphertext()
        .recheck_and_rewind()
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    let outcome = match committed.outcome {
        BridgeGenerationSuccessorCommitOutcomeDto::Activated => {
            GenerationSuccessorCommitOutcome::Activated
        }
        BridgeGenerationSuccessorCommitOutcomeDto::AlreadyActive => {
            GenerationSuccessorCommitOutcome::AlreadyActive
        }
    };
    CommittedGenerationSuccessor::confirmed_streamed(
        exact.successor_id(),
        exact.object_key(),
        exact.metadata_digest(),
        exact.container_digest(),
        exact.container_bytes(),
        outcome,
    )
    .map(Some)
    .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)
}

/// A controlled close still held by the ORIGINAL process may retry the
/// EXACT checkpoint only after a separately accepted current-session DO
/// heartbeat and this affirmative D1 nonapplication response. Unlike a
/// process-restart path this does NOT mint a new lease; the caller MUST
/// separately verify the current coordinator deadline immediately before PUT.
/// False/unknown/changed checkpoint never grants upload, commit or Saved.
#[cfg(any(windows, test))]
pub(crate) fn confirm_retained_exact_retry_nonapplication<T: MachineHttpPort>(
    root: &MaterializationRoot,
    original_lease: &ProfileLease,
    transport: &mut T,
) -> Result<bool, RestartExactLocalCompletionError> {
    let tenant = original_lease.tenant_id();
    let profile = original_lease.profile_id();
    let exact = discover_unresolved_streaming_exact_save_attempt(root, tenant, profile)
        .map_err(|_| RestartExactLocalCompletionError::LocalEvidenceChanged)?
        .ok_or(RestartExactLocalCompletionError::LocalEvidenceChanged)?;
    if !exact.original_lease(original_lease) || !exact.original_device(original_lease) {
        return Err(RestartExactLocalCompletionError::LocalEvidenceChanged);
    }
    Ok(
        request_fresh_retry_decision_streamed(tenant, profile, &exact, original_lease, transport)
            .map_err(RestartExactLocalCompletionError::ControlPlane)?
            == BridgeGenerationSuccessorRetryVerdict::NotAppliedRetryable,
    )
}

/// D1-positive base-only proof for a lone LOCAL_UNSYNCED native close.
#[cfg(any(windows, test))]
fn request_local_unsynced_retry_decision<T: MachineHttpPort>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    base: &GenerationId,
    fresh_lease: &ProfileLease,
    transport: &mut T,
) -> Result<BridgeGenerationSuccessorRetryVerdict, ShippingControlPlaneError> {
    let request = BridgeGenerationLocalUnsyncedRetryDecisionRequest {
        base_generation_id: base.as_str().to_owned(),
        coordinator_session_id: fresh_lease.session_id().as_str().to_owned(),
        coordinator_fencing_token: fresh_lease.fencing_token().as_str().to_owned(),
        coordinator_epoch: fresh_lease.epoch(),
    };
    let response = post_json::<T, core::convert::Infallible, _>(
        transport,
        &successor_path(
            BRIDGE_PROFILE_GENERATION_LOCAL_UNSYNCED_RETRY_DECISION_PATH_TEMPLATE,
            tenant_id,
            profile_id,
        ),
        &request,
    )
    .map_err(|error| match error {
        GenerationSuccessorControlError::ControlPlane(reason) => reason,
        GenerationSuccessorControlError::Sealing(never) => match never {},
    })?;
    if response.len() > MAX_OBSERVE_RESPONSE_BYTES {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let result =
        serde_json::from_slice::<BridgeGenerationSuccessorRetryDecisionResponse>(&response)
            .map_err(|_| ShippingControlPlaneError::InvalidResponse)?;
    Ok(result.verdict)
}

/// Typed server-positive eligibility for the exact original sealed attempt.
/// Caller has a freshly claimed selected base. No PUT, commit or cleanup.
#[cfg(any(windows, test))]
fn request_fresh_retry_decision<T: MachineHttpPort>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    attempt: &ReadOnlySaveAttempt,
    fresh_lease: &ProfileLease,
    transport: &mut T,
) -> Result<BridgeGenerationSuccessorRetryVerdict, ShippingControlPlaneError> {
    request_fresh_retry_decision_for_descriptor(
        tenant_id,
        profile_id,
        BridgeGenerationSuccessorObserveRequest {
            base_generation_id: attempt.base_generation_id().to_owned(),
            generation_id: attempt.successor_id().to_owned(),
            object_key: attempt.object_key().to_owned(),
            metadata_digest: attempt.metadata_digest().to_owned(),
            container_digest: attempt.container_digest().to_owned(),
            container_bytes: attempt.container_bytes(),
        },
        fresh_lease,
        transport,
    )
}

#[cfg(any(windows, test))]
fn request_fresh_retry_decision_streamed<T: MachineHttpPort>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    attempt: &ReadOnlyStreamedSaveAttempt,
    fresh_lease: &ProfileLease,
    transport: &mut T,
) -> Result<BridgeGenerationSuccessorRetryVerdict, ShippingControlPlaneError> {
    request_fresh_retry_decision_for_descriptor(
        tenant_id,
        profile_id,
        BridgeGenerationSuccessorObserveRequest {
            base_generation_id: attempt.base_generation_id().to_owned(),
            generation_id: attempt.successor_id().to_owned(),
            object_key: attempt.object_key().to_owned(),
            metadata_digest: attempt.metadata_digest().to_owned(),
            container_digest: attempt.container_digest().to_owned(),
            container_bytes: attempt.container_bytes(),
        },
        fresh_lease,
        transport,
    )
}

#[cfg(any(windows, test))]
fn request_fresh_retry_decision_for_descriptor<T: MachineHttpPort>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    attempt: BridgeGenerationSuccessorObserveRequest,
    fresh_lease: &ProfileLease,
    transport: &mut T,
) -> Result<BridgeGenerationSuccessorRetryVerdict, ShippingControlPlaneError> {
    let request = BridgeGenerationSuccessorRetryDecisionRequest {
        attempt,
        coordinator_session_id: fresh_lease.session_id().as_str().to_owned(),
        coordinator_fencing_token: fresh_lease.fencing_token().as_str().to_owned(),
        coordinator_epoch: fresh_lease.epoch(),
    };
    let response = post_json::<T, core::convert::Infallible, _>(
        transport,
        &successor_path(
            BRIDGE_PROFILE_GENERATION_RETRY_DECISION_PATH_TEMPLATE,
            tenant_id,
            profile_id,
        ),
        &request,
    )
    .map_err(|error| match error {
        GenerationSuccessorControlError::ControlPlane(reason) => reason,
        GenerationSuccessorControlError::Sealing(never) => match never {},
    })?;
    if response.len() > MAX_OBSERVE_RESPONSE_BYTES {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let result =
        serde_json::from_slice::<BridgeGenerationSuccessorRetryDecisionResponse>(&response)
            .map_err(|_| ShippingControlPlaneError::InvalidResponse)?;
    Ok(result.verdict)
}

/// Read-only server probe from crash checkpoint after a lost response.
/// Requires fresh machine CNG authentication via the transport. Never submits
/// the old fence, mutates the Profile, or grants permission to repeat PUT.
pub fn observe_retained_successor_after_restart<T: MachineHttpPort>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    successor_id: &GenerationId,
    transport: &mut T,
) -> Result<bool, ShippingControlPlaneError> {
    let attempt = inspect_streaming_exact_save_attempt(root, tenant_id, profile_id, successor_id)
        .map_err(|_| ShippingControlPlaneError::InvalidResponse)?
        .ok_or(ShippingControlPlaneError::InvalidResponse)?;
    observe_verified_streamed_attempt(tenant_id, profile_id, &attempt, transport)
}

/// One existing read-only control-plane observation, over *already validated*
/// exact local attempt evidence. Neither the discovered path nor the explicit
/// generation-ID entrypoint rereads/duplicates the full sealed record.
#[cfg(any(windows, test))]
fn observe_verified_exact_attempt<T: MachineHttpPort>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    attempt: &ReadOnlySaveAttempt,
    transport: &mut T,
) -> Result<bool, ShippingControlPlaneError> {
    let request = BridgeGenerationSuccessorObserveRequest {
        base_generation_id: attempt.base_generation_id().to_owned(),
        generation_id: attempt.successor_id().to_owned(),
        object_key: attempt.object_key().to_owned(),
        metadata_digest: attempt.metadata_digest().to_owned(),
        container_digest: attempt.container_digest().to_owned(),
        container_bytes: attempt.container_bytes(),
    };
    observe_exact_descriptor(tenant_id, profile_id, &request, transport)
}

/// The same typed observer accepts a verified file-backed BPAT descriptor:
/// no generation-sized ciphertext allocation and no cloud write authority.
fn observe_verified_streamed_attempt<T: MachineHttpPort>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    attempt: &ReadOnlyStreamedSaveAttempt,
    transport: &mut T,
) -> Result<bool, ShippingControlPlaneError> {
    let request = BridgeGenerationSuccessorObserveRequest {
        base_generation_id: attempt.base_generation_id().to_owned(),
        generation_id: attempt.successor_id().to_owned(),
        object_key: attempt.object_key().to_owned(),
        metadata_digest: attempt.metadata_digest().to_owned(),
        container_digest: attempt.container_digest().to_owned(),
        container_bytes: attempt.container_bytes(),
    };
    observe_exact_descriptor(tenant_id, profile_id, &request, transport)
}

/// One existing D1 ACTIVE_EXACT query. Both inspectors must prove the
/// ciphertext/local inventory *before* this read-only request is constructed.
fn observe_exact_descriptor<T: MachineHttpPort>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    request: &BridgeGenerationSuccessorObserveRequest,
    transport: &mut T,
) -> Result<bool, ShippingControlPlaneError> {
    let response = post_json::<T, core::convert::Infallible, _>(
        transport,
        &successor_path(
            BRIDGE_PROFILE_GENERATION_OBSERVE_PATH_TEMPLATE,
            tenant_id,
            profile_id,
        ),
        request,
    )
    .map_err(|error| match error {
        GenerationSuccessorControlError::ControlPlane(reason) => reason,
        GenerationSuccessorControlError::Sealing(never) => match never {},
    })?;
    if response.len() > MAX_OBSERVE_RESPONSE_BYTES {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let result = serde_json::from_slice::<BridgeGenerationSuccessorObserveResponse>(&response)
        .map_err(|_| ShippingControlPlaneError::InvalidResponse)?;
    Ok(result.active_exact)
}

fn successor_request<S>(
    base_generation_id: &GenerationId,
    prepared: &PreparedDirtyGeneration,
    lease: &ProfileLease,
) -> Result<BridgeProfileGenerationSuccessorRequest, GenerationSuccessorControlError<S>> {
    let container_bytes = u64::try_from(prepared.sealed().container().len())
        .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
    Ok(BridgeProfileGenerationSuccessorRequest::new(
        base_generation_id.as_str(),
        prepared.sealed().metadata().generation_id().as_str(),
        &prepared.object_key(),
        prepared.metadata_digest(),
        prepared.container_digest(),
        container_bytes,
        lease.session_id().as_str(),
        lease.fencing_token().as_str(),
        lease.epoch(),
    ))
}

fn post_json<T, S, B>(
    transport: &mut T,
    path: &str,
    body: &B,
) -> Result<Vec<u8>, GenerationSuccessorControlError<S>>
where
    T: MachineHttpPort,
    B: serde::Serialize,
{
    let correlation_id = next_correlation_id()?;
    let mut encoded = serde_json::to_vec(body)
        .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
    let response = transport
        .request(
            MachineHttpMethod::PostJson,
            path,
            &correlation_id,
            Some(&encoded),
        )
        .map_err(|_| control_error(ShippingControlPlaneError::Transport));
    encoded.fill(0);
    let response = response?;
    if matches!(response.status(), 408 | 429) || (500..=599).contains(&response.status()) {
        // Commit or capability effects may be ambiguous. Preserve exact
        // persisted attempt identity; never treat this as authority to retry
        // an upload/commit or to label the browser profile Saved.
        return Err(control_error(
            ShippingControlPlaneError::HttpServerUncertain,
        ));
    }
    if !(200..300).contains(&response.status()) {
        return Err(control_error(ShippingControlPlaneError::HttpStatus));
    }
    Ok(response.body().to_vec())
}

pub(crate) fn decode_upload_authorization<S>(
    decoded: BridgeGenerationUploadCapabilityResponse,
) -> Result<GenerationUploadAuthorization, GenerationSuccessorControlError<S>> {
    match decoded.state.as_str() {
        "verified"
            if decoded.method.is_none()
                && decoded.url.is_none()
                && decoded.headers.is_empty()
                && decoded.expires_seconds.is_none() =>
        {
            Ok(GenerationUploadAuthorization::Verified)
        }
        "uploadRequired" => {
            let method = decoded
                .method
                .ok_or_else(|| control_error(ShippingControlPlaneError::InvalidResponse))?;
            let url = decoded
                .url
                .ok_or_else(|| control_error(ShippingControlPlaneError::InvalidResponse))?;
            let expires_seconds = decoded
                .expires_seconds
                .ok_or_else(|| control_error(ShippingControlPlaneError::InvalidResponse))?;
            if method != "PUT"
                || !valid_signed_upload_url(&url)
                || !(1..=MAX_UPLOAD_CAPABILITY_EXPIRES_SECONDS).contains(&expires_seconds)
                || decoded.headers.is_empty()
                || decoded.headers.len() > MAX_SIGNED_UPLOAD_HEADERS
            {
                return Err(control_error(ShippingControlPlaneError::InvalidResponse));
            }
            let mut headers = Vec::with_capacity(decoded.headers.len());
            for header in decoded.headers {
                if !valid_header_name(&header.name) || !valid_header_value(&header.value) {
                    return Err(control_error(ShippingControlPlaneError::InvalidResponse));
                }
                headers.push((header.name, header.value));
            }
            Ok(GenerationUploadAuthorization::UploadRequired(
                SignedGenerationUploadCapability::new(url, headers, expires_seconds),
            ))
        }
        _ => Err(control_error(ShippingControlPlaneError::InvalidResponse)),
    }
}

fn valid_signed_upload_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    !rest.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_URL_BYTES
        && !value.contains('#')
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

fn valid_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES
        && !value
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0)
}

fn successor_path(template: &str, tenant_id: &TenantId, profile_id: &ProfileId) -> String {
    template
        .replace("{tenantId}", tenant_id.as_str())
        .replace("{profileId}", profile_id.as_str())
}

fn next_correlation_id<S>() -> Result<CorrelationId, GenerationSuccessorControlError<S>> {
    let sequence = SAVE_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    CorrelationId::parse(format!("corr_p3_generation_save_{sequence:020}"))
        .map_err(|_| control_error(ShippingControlPlaneError::Clock))
}

const fn control_error<S>(error: ShippingControlPlaneError) -> GenerationSuccessorControlError<S> {
    GenerationSuccessorControlError::ControlPlane(error)
}

#[cfg(test)]
mod tests {
    use super::{
        ControlPlaneGenerationSuccessor, GenerationSuccessorControlError,
        RestartExactCommitObservation, RestartExactLocalCompletion,
        RestartExactLocalCompletionError, complete_active_exact_after_restart,
        observe_discovered_successor_after_restart, observe_retained_successor_after_restart,
    };
    use crate::browser_execution::load_materialization_binding;
    use crate::dirty_generation::{
        GenerationSealingMaterial, GenerationSealingMaterialPort,
        prepare_dirty_generation_candidate,
    };
    use crate::local_profile::{BridgeWorkspaceLock, LocalGenerationRecord, MaterializationRoot};
    use crate::shipping_control_plane::{
        MachineHttpMethod, MachineHttpPort, MachineHttpResponse, ShippingControlPlaneError,
    };
    use crate::shipping_generation_save::{
        GenerationSuccessorCommitOutcome, GenerationSuccessorControlPort,
        GenerationUploadAuthorization,
    };
    use crate::shipping_save_attempt::{
        discover_unresolved_exact_save_attempt, discover_unresolved_streaming_exact_save_attempt,
        has_unresolved_save_attempt_for_profile, inspect_exact_save_attempt,
        persist_exact_save_attempt, persist_local_unsynced_intent,
        retire_local_unsynced_after_exact_checkpoint,
    };
    use bridge_domain::BridgePortError;
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{
        CorrelationId, DeviceId, FencingToken, GenerationId, ProfileId, SessionId, TenantId,
        UnixMillis,
    };
    use session_domain::ProfileLease;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[derive(Default)]
    struct Transport {
        responses: VecDeque<MachineHttpResponse>,
        paths: Vec<String>,
    }

    impl MachineHttpPort for Transport {
        type Error = BridgePortError;

        fn request(
            &mut self,
            method: MachineHttpMethod,
            path: &str,
            _correlation_id: &CorrelationId,
            body: Option<&[u8]>,
        ) -> Result<MachineHttpResponse, Self::Error> {
            if method != MachineHttpMethod::PostJson || body.is_none() {
                return Err(BridgePortError::InvalidResponse);
            }
            self.paths.push(path.to_owned());
            self.responses
                .pop_front()
                .ok_or(BridgePortError::Unavailable)
        }
    }

    #[derive(Default)]
    struct Sealer;

    impl GenerationSealingMaterialPort for Sealer {
        type Error = BridgePortError;

        fn material_for(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            _base_generation_id: &GenerationId,
            _generation_id: &GenerationId,
            _plaintext_digest: [u8; 32],
        ) -> Result<GenerationSealingMaterial, Self::Error> {
            Ok(GenerationSealingMaterial::new(
                GenerationDek::new(
                    KeyId::parse("profile-generation-root-v1-7")
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                    [7; 32],
                ),
                NoncePrefix::new([8; 16]),
                4096,
            ))
        }
    }

    struct Fixture {
        root_path: std::path::PathBuf,
        tenant: TenantId,
        profile: ProfileId,
        base: GenerationId,
        prepared: crate::dirty_generation::PreparedDirtyGeneration,
        lease: ProfileLease,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-successor-control-{}-{sequence}",
                std::process::id()
            ));
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant = TenantId::parse(format!("tenant_successor_control_{sequence}"))?;
            let profile = ProfileId::parse(format!("profile_successor_control_{sequence}"))?;
            let base =
                GenerationId::parse(format!("generation_successor_control_base_{sequence}"))?;
            let candidate =
                GenerationId::parse(format!("generation_successor_control_next_{sequence}"))?;
            let device = DeviceId::parse(format!("device_successor_control_{sequence}"))?;
            let workspace = root.create_generation(&tenant, &profile, &base)?;
            std::fs::write(workspace.path().join("prefs.js"), b"save")?;
            let identity = crate::test_support::browser_identity_fixture(
                "camoufox-test-runtime-v1",
                "a".repeat(64),
                "successor-control-profile-stable",
                "b".repeat(64),
            )?;
            crate::browser_execution::persist_browser_identity(&workspace, &identity)?;
            let mut record = LocalGenerationRecord::new(base.clone(), 4, UnixMillis::new(1));
            record.set_locked(true)?;
            record.begin_use(UnixMillis::new(2))?;
            record.graceful_close(UnixMillis::new(3))?;
            let mut sealer = Sealer;
            let prepared = prepare_dirty_generation_candidate(
                &record,
                &workspace,
                &root,
                &tenant,
                &profile,
                &candidate,
                &mut sealer,
            )?;
            let lease = ProfileLease::issue(
                tenant.clone(),
                profile.clone(),
                SessionId::parse(format!("session_successor_control_{sequence}"))?,
                device,
                4,
                FencingToken::parse(format!("fence_successor_control_{sequence}"))?,
            )?;
            Ok(Self {
                root_path,
                tenant,
                profile,
                base,
                prepared,
                lease,
            })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    #[test]
    fn local_unsynced_intent_is_retired_only_after_complete_exact_checkpoint()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        // Byte-for-byte idempotence requires the identical original scoped
        // lease, inventory and local browser working bytes.
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert!(
            discover_unresolved_exact_save_attempt(&root, &fixture.tenant, &fixture.profile,)
                .is_err()
        );
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let checkpoint = inspect_exact_save_attempt(
            &root,
            &fixture.tenant,
            &fixture.profile,
            fixture.prepared.candidate_workspace().generation_id(),
        )?
        .ok_or("missing exact sealed checkpoint")?;
        retire_local_unsynced_after_exact_checkpoint(&root, &source, &fixture.lease, &checkpoint)?;
        // The small intent is gone; the full ciphertext checkpoint is NOT
        // retired, so lost PUT/commit still blocks unsafe future Open.
        let discovered =
            discover_unresolved_exact_save_attempt(&root, &fixture.tenant, &fixture.profile)?
                .ok_or("checkpoint unexpectedly retired")?;
        assert_eq!(discovered.ciphertext(), checkpoint.ciphertext());
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        Ok(())
    }

    #[test]
    fn lone_local_unsynced_is_verified_but_never_grants_cloud_retry()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        let original = std::fs::read(source.path().join("prefs.js"))?;
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        assert!(
            discover_unresolved_exact_save_attempt(&root, &fixture.tenant, &fixture.profile,)
                .is_err()
        );
        let fresh = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_lone_local_unsynced")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_lone_local_unsynced")?,
        )?;
        let mut transport = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        let disposition = complete_active_exact_after_restart(
            &root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.base,
            &fresh,
            &mut transport,
        );
        assert_eq!(
            disposition,
            Ok(RestartExactLocalCompletion::LocalUnsyncedAuthorizedAwaitingFirstSeal),
        );
        assert_eq!(transport.paths.len(), 1);
        assert!(
            transport.paths[0].ends_with("/generation-successor/local-unsynced/retry-decision")
        );
        assert_eq!(std::fs::read(source.path().join("prefs.js"))?, original);
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        // D1 may explicitly refuse the same locally intact source.
        let mut denied = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"BLOCKED"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut denied,
            ),
            Ok(RestartExactLocalCompletion::RetryDecisionBlocked),
        );
        assert_eq!(denied.paths.len(), 1);
        let foreign_device = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_lone_foreign")?,
            DeviceId::parse("device_lone_foreign")?,
            fresh.epoch() + 1,
            FencingToken::parse("fence_lone_foreign")?,
        )?;
        let mut no_network = Transport::default();
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &foreign_device,
                &mut no_network,
            ),
            Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        );
        assert!(no_network.paths.is_empty());
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        // A stale epoch or an unrelated selected base never makes the
        // offline source an authorized fresh writer or allows N+2.
        let mut denied_transport = Transport::default();
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.lease,
                &mut denied_transport,
            ),
            Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        );
        assert!(denied_transport.paths.is_empty());
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                fixture.prepared.candidate_workspace().generation_id(),
                &fresh,
                &mut denied_transport,
            ),
            Ok(RestartExactLocalCompletion::ServerSelectedDifferentGeneration),
        );
        assert!(denied_transport.paths.is_empty());
        Ok(())
    }

    #[test]
    fn tampered_lone_local_unsynced_preserves_native_bytes_and_blocks_recovery()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        std::fs::write(
            source.path().join("prefs.js"),
            b"changed-without-checkpoint",
        )?;
        let fresh = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_lone_tampered")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_lone_tampered")?,
        )?;
        let mut transport = Transport::default();
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut transport,
            ),
            Err(RestartExactLocalCompletionError::LocalEvidenceChanged),
        );
        assert!(transport.paths.is_empty());
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert_eq!(
            std::fs::read(source.path().join("prefs.js"))?,
            b"changed-without-checkpoint",
        );
        Ok(())
    }

    #[test]
    fn crash_with_local_intent_and_exact_checkpoint_resumes_active_exact_without_new_put()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        let original = std::fs::read(source.path().join("prefs.js"))?;
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        // Simulate process death EXACTLY between checkpoint hardlink and
        // small intent retirement: two valid files belong to one attempt.
        let successor = fixture.prepared.candidate_workspace().generation_id();
        let checkpoint =
            discover_unresolved_exact_save_attempt(&root, &fixture.tenant, &fixture.profile)?
                .ok_or("matching exact pair not discoverable")?;
        assert_eq!(checkpoint.successor_id(), successor.as_str());
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);

        let fresh = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_fresh_crash_pair")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_fresh_crash_pair")?,
        )?;
        let mut transport = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":true}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &fresh,
                &mut transport,
            ),
            Ok(RestartExactLocalCompletion::ActiveExactLocalReady),
        );
        assert_eq!(transport.paths.len(), 1);
        assert!(transport.paths[0].ends_with("/generation-successor/observe"));
        assert!(!has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert_eq!(std::fs::read(source.path().join("prefs.js"))?, original);
        let bound = load_materialization_binding(
            fixture.prepared.candidate_workspace(),
            &fixture.tenant,
            &fixture.profile,
            successor,
        )?;
        assert_eq!(
            bound.source_container_sha256(),
            checkpoint.container_digest()
        );
        Ok(())
    }

    #[test]
    fn crash_pair_refuses_foreign_intent_and_pending_stage_before_server_effect()
    -> Result<(), Box<dyn std::error::Error>> {
        for tamper in ["mismatched_intent", "extra_pending"] {
            let fixture = Fixture::new()?;
            let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
            let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
            let bytes_before = std::fs::read(source.path().join("prefs.js"))?;
            persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
            persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
            let marker = std::fs::read_dir(root.path())?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name().is_some_and(|name| {
                        name.to_string_lossy()
                            .ends_with("-local-unsynced-v1.intent")
                    })
                })
                .ok_or("missing durable local intent")?;
            if tamper == "mismatched_intent" {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&marker)?)?;
                value["epoch"] = serde_json::json!(fixture.lease.epoch() + 5);
                std::fs::write(&marker, serde_json::to_vec(&value)?)?;
            } else {
                let extra = root.path().join(format!(
                    "{}.pending-foreign",
                    marker
                        .file_name()
                        .ok_or("intent filename")?
                        .to_string_lossy()
                        .trim_end_matches("-local-unsynced-v1.intent")
                ));
                std::fs::write(extra, b"unexpected-third-save-evidence")?;
            }
            assert!(
                discover_unresolved_exact_save_attempt(&root, &fixture.tenant, &fixture.profile,)
                    .is_err()
            );
            let fresh = ProfileLease::issue(
                fixture.tenant.clone(),
                fixture.profile.clone(),
                SessionId::parse("session_crash_negative")?,
                fixture.lease.device_id().clone(),
                fixture.lease.epoch() + 1,
                FencingToken::parse("fence_crash_negative")?,
            )?;
            let mut network = Transport::default();
            assert_eq!(
                complete_active_exact_after_restart(
                    &root,
                    &fixture.tenant,
                    &fixture.profile,
                    fixture.prepared.candidate_workspace().generation_id(),
                    &fresh,
                    &mut network,
                ),
                Err(RestartExactLocalCompletionError::LocalEvidenceChanged),
            );
            assert!(network.paths.is_empty());
            assert!(has_unresolved_save_attempt_for_profile(
                &root,
                &fixture.tenant,
                &fixture.profile,
            )?);
            assert_eq!(std::fs::read(source.path().join("prefs.js"))?, bytes_before);
        }
        Ok(())
    }

    #[test]
    fn changed_source_cannot_retire_local_unsynced_intent() -> Result<(), Box<dyn std::error::Error>>
    {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let checkpoint = inspect_exact_save_attempt(
            &root,
            &fixture.tenant,
            &fixture.profile,
            fixture.prepared.candidate_workspace().generation_id(),
        )?
        .ok_or("missing checkpoint")?;
        std::fs::write(source.path().join("prefs.js"), b"modified-after-close")?;
        assert!(
            retire_local_unsynced_after_exact_checkpoint(
                &root,
                &source,
                &fixture.lease,
                &checkpoint,
            )
            .is_err()
        );
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert_eq!(
            std::fs::read(source.path().join("prefs.js"))?,
            b"modified-after-close",
        );
        Ok(())
    }

    #[test]
    fn verified_upload_response_and_commit_are_strictly_typed()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let transport = Transport {
            responses: VecDeque::from([
                MachineHttpResponse::new(
                    200,
                    br#"{"state":"verified","method":null,"url":null,"headers":[],"expiresSeconds":null}"#
                        .to_vec(),
                ),
                MachineHttpResponse::new(200, br#"{"outcome":"ACTIVATED"}"#.to_vec()),
            ]),
            paths: Vec::new(),
        };
        let mut sealer = Sealer;
        let mut client = ControlPlaneGenerationSuccessor::new(transport, &mut sealer);
        assert_eq!(
            client.upload_authorization(
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.prepared,
                &fixture.lease,
            )?,
            GenerationUploadAuthorization::Verified
        );
        assert_eq!(
            client.commit_successor(
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.prepared,
                &fixture.lease,
            )?,
            GenerationSuccessorCommitOutcome::Activated
        );
        Ok(())
    }

    #[test]
    fn restarted_active_exact_repairs_binding_and_retires_only_completed_checkpoint()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let successor = fixture.prepared.candidate_workspace().generation_id();
        // Windows/Bridge crash leaves a persistent old device marker but no
        // live root admission. Recovery must prove fresh epoch and same writer.
        let old = BridgeWorkspaceLock::acquire(
            &source,
            fixture.lease.device_id(),
            fixture.lease.epoch(),
        )?;
        drop(old);
        let new_lease = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_fresh_local_recovery")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_fresh_local_recovery")?,
        )?;
        let mut transport = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":true}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &new_lease,
                &mut transport,
            ),
            Ok(RestartExactLocalCompletion::ActiveExactLocalReady),
        );
        assert_eq!(transport.paths.len(), 1);
        assert!(transport.paths[0].ends_with("/generation-successor/observe"));
        let bound = load_materialization_binding(
            fixture.prepared.candidate_workspace(),
            &fixture.tenant,
            &fixture.profile,
            successor,
        )?;
        assert_eq!(
            bound.source_container_sha256(),
            fixture.prepared.container_digest(),
        );
        assert!(!has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert!(!source.path().join(".profile-platform.lock").exists());
        assert_eq!(std::fs::read(source.path().join("prefs.js"))?, b"save");
        assert_eq!(
            std::fs::read(
                fixture
                    .prepared
                    .candidate_workspace()
                    .path()
                    .join("prefs.js")
            )?,
            b"save",
        );
        let mut no_network = Transport::default();
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &new_lease,
                &mut no_network,
            ),
            Ok(RestartExactLocalCompletion::NoCanonicalCheckpoint),
        );
        assert!(no_network.paths.is_empty());
        Ok(())
    }

    #[test]
    fn restarted_local_completion_refuses_not_active_foreign_and_tampered_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let successor = fixture.prepared.candidate_workspace().generation_id();
        let fresh = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_fresh_negative_recovery")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_fresh_negative_recovery")?,
        )?;
        let original = std::fs::read(source.path().join("prefs.js"))?;
        let mut blocked = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"BLOCKED"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut blocked,
            ),
            Ok(RestartExactLocalCompletion::RetryDecisionBlocked),
        );
        assert_eq!(blocked.paths.len(), 1);
        assert!(blocked.paths[0].ends_with("/generation-successor/retry-decision"));
        let mut permitted = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut permitted,
            ),
            Ok(RestartExactLocalCompletion::RetryDecisionConfirmedAwaitingResumption),
        );
        assert_eq!(permitted.paths.len(), 1);
        assert!(permitted.paths[0].ends_with("/generation-successor/retry-decision"));
        assert_eq!(std::fs::read(source.path().join("prefs.js"))?, original);
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        let mut stale = Transport::default();
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.lease,
                &mut stale,
            ),
            Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        );
        assert!(stale.paths.is_empty());
        let mut not_active = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":false}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &fresh,
                &mut not_active,
            ),
            Ok(RestartExactLocalCompletion::NotActiveExact),
        );
        assert_eq!(not_active.paths.len(), 1);
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert!(
            load_materialization_binding(
                fixture.prepared.candidate_workspace(),
                &fixture.tenant,
                &fixture.profile,
                successor,
            )
            .is_err()
        );

        std::fs::write(
            fixture
                .prepared
                .candidate_workspace()
                .path()
                .join("prefs.js"),
            b"mutated-browser-after-seal",
        )?;
        let mut must_not_observe = Transport::default();
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &fresh,
                &mut must_not_observe,
            ),
            Err(RestartExactLocalCompletionError::LocalEvidenceChanged),
        );
        assert!(must_not_observe.paths.is_empty());
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        Ok(())
    }

    #[derive(Default)]
    struct RecoveryPut {
        calls: usize,
        exact_bytes: Vec<u8>,
    }

    impl crate::shipping_generation_save::SignedGenerationObjectPutPort for RecoveryPut {
        type Error = BridgePortError;

        fn put_exact(
            &mut self,
            _capability: &crate::shipping_generation_save::SignedGenerationUploadCapability,
            ciphertext: &[u8],
        ) -> Result<(), Self::Error> {
            self.calls += 1;
            self.exact_bytes = ciphertext.to_vec();
            Ok(())
        }
    }

    impl crate::shipping_generation_save::SignedGenerationObjectStreamPutPort for RecoveryPut {
        fn put_verified_stream(
            &mut self,
            _capability: &crate::shipping_generation_save::SignedGenerationUploadCapability,
            object: &mut crate::generation_object_spool::VerifiedGenerationObjectFile,
        ) -> Result<(), Self::Error> {
            object
                .recheck_and_rewind()
                .map_err(|_| BridgePortError::InvalidResponse)?;
            self.calls += 1;
            self.exact_bytes.clear();
            std::io::copy(object.reader(), &mut self.exact_bytes)
                .map_err(|_| BridgePortError::Unavailable)?;
            object
                .recheck_and_rewind()
                .map_err(|_| BridgePortError::InvalidResponse)?;
            Ok(())
        }
    }

    #[test]
    fn retained_live_retry_requires_exact_original_provenance_and_d1_positive()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        // Lone intent is NOT an uploaded exact N+1.
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        let mut no_network = Transport::default();
        assert_eq!(
            super::confirm_retained_exact_retry_nonapplication(
                &root,
                &fixture.lease,
                &mut no_network,
            ),
            Err(RestartExactLocalCompletionError::LocalEvidenceChanged),
        );
        assert!(no_network.paths.is_empty());
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        retire_local_unsynced_after_exact_checkpoint(
            &root,
            &source,
            &fixture.lease,
            &inspect_exact_save_attempt(
                &root,
                &fixture.tenant,
                &fixture.profile,
                fixture.prepared.candidate_workspace().generation_id(),
            )?
            .ok_or("missing exact checkpoint")?,
        )?;
        let mut allowed = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            super::confirm_retained_exact_retry_nonapplication(&root, &fixture.lease, &mut allowed,),
            Ok(true),
        );
        assert_eq!(allowed.paths.len(), 1);
        assert!(allowed.paths[0].ends_with("/generation-successor/retry-decision"));
        let mut blocked = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"BLOCKED"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            super::confirm_retained_exact_retry_nonapplication(&root, &fixture.lease, &mut blocked,),
            Ok(false),
        );
        let foreign = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_forged_lost_writer")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_forged_lost_writer")?,
        )?;
        let mut foreign_transport = Transport::default();
        assert_eq!(
            super::confirm_retained_exact_retry_nonapplication(
                &root,
                &foreign,
                &mut foreign_transport,
            ),
            Err(RestartExactLocalCompletionError::LocalEvidenceChanged),
        );
        assert!(foreign_transport.paths.is_empty());
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        Ok(())
    }

    #[test]
    fn fresh_retry_resumes_exact_ciphertext_and_retains_checkpoint_until_active_exact()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let expected = fixture.prepared.sealed().container();
        let fresh = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_retry_fresh_authorized")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_retry_fresh_authorized")?,
        )?;
        let mut put = RecoveryPut::default();
        let mut transport = Transport {
            responses: VecDeque::from([
                MachineHttpResponse::new(
                    200,
                    br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                ),
                MachineHttpResponse::new(
                    200,
                    br#"{"state":"uploadRequired","method":"PUT","url":"https://r2.example.test/exact","headers":[{"name":"x-amz-date","value":"20261026T000000Z"}],"expiresSeconds":120}"#.to_vec(),
                ),
                MachineHttpResponse::new(
                    200,
                    br#"{"state":"verified","method":null,"url":null,"headers":[],"expiresSeconds":null}"#.to_vec(),
                ),
                MachineHttpResponse::new(200, br#"{"outcome":"ACTIVATED"}"#.to_vec()),
            ]),
            paths: Vec::new(),
        };
        assert_eq!(
            super::resume_exact_successor_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut transport,
                &mut put,
            ),
            Ok(true),
        );
        assert_eq!(put.calls, 1);
        assert_eq!(put.exact_bytes, expected);
        assert_eq!(transport.paths.len(), 4);
        assert!(transport.paths[0].ends_with("/generation-successor/retry-decision"));
        assert!(transport.paths[1].ends_with("/generation-successor/upload-capability"));
        assert!(transport.paths[2].ends_with("/generation-successor/upload-capability"));
        assert!(transport.paths[3].ends_with("/generation-successor/commit"));
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert_eq!(std::fs::read(source.path().join("prefs.js"))?, b"save");
        Ok(())
    }

    #[test]
    fn rejected_or_uncertain_retry_does_not_put_commit_or_remove_checkpoint()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let fresh = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_retry_rejected")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_retry_rejected")?,
        )?;
        let mut uploader = RecoveryPut::default();
        let mut denied = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"BLOCKED"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            super::resume_exact_successor_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut denied,
                &mut uploader,
            ),
            Ok(false),
        );
        assert_eq!(denied.paths.len(), 1);
        assert_eq!(uploader.calls, 0);
        let mut lost_commit = Transport {
            responses: VecDeque::from([
                MachineHttpResponse::new(
                    200,
                    br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                ),
                MachineHttpResponse::new(
                    200,
                    br#"{"state":"verified","method":null,"url":null,"headers":[],"expiresSeconds":null}"#.to_vec(),
                ),
                MachineHttpResponse::new(503, Vec::new()),
            ]),
            paths: Vec::new(),
        };
        assert_eq!(
            super::resume_exact_successor_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut lost_commit,
                &mut uploader,
            ),
            Err(RestartExactLocalCompletionError::ControlPlane(
                ShippingControlPlaneError::HttpServerUncertain,
            )),
        );
        assert_eq!(lost_commit.paths.len(), 3);
        assert_eq!(uploader.calls, 0);
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        let mut stale = Transport::default();
        assert_eq!(
            super::resume_exact_successor_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.lease,
                &mut stale,
                &mut uploader,
            ),
            Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        );
        assert!(stale.paths.is_empty());
        let foreign = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_retry_foreign_device")?,
            DeviceId::parse("device_retry_foreign_device")?,
            fresh.epoch() + 1,
            FencingToken::parse("fence_retry_foreign_device")?,
        )?;
        let mut no_network = Transport::default();
        assert_eq!(
            super::resume_exact_successor_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &foreign,
                &mut no_network,
                &mut uploader,
            ),
            Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        );
        assert!(no_network.paths.is_empty());
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        Ok(())
    }

    #[test]
    fn fresh_lone_intent_first_seals_then_commit_and_next_claim_finishes_active_exact()
    -> Result<(), Box<dyn std::error::Error>> {
        // The third case is above the former 64 MiB in-memory limit. It
        // proves the actual K1 post-crash first-seal/retry/binding path uses
        // the disk-backed BPAT, not merely the ordinary K2 Save happy path.
        for (existing_clone, large_native_profile) in [(false, false), (true, false), (true, true)]
        {
            let fixture = Fixture::new()?;
            let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
            let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
            if large_native_profile {
                let native = source.path().join("user_data");
                std::fs::create_dir_all(&native)?;
                let wal = std::fs::File::create(native.join("cookies.sqlite-wal"))?;
                wal.set_len(65 * 1024 * 1024)?;
                wal.sync_all()?;
            }
            persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
            let fresh = ProfileLease::issue(
                fixture.tenant.clone(),
                fixture.profile.clone(),
                SessionId::parse("session_reseal_fresh")?,
                fixture.lease.device_id().clone(),
                fixture.lease.epoch() + 1,
                FencingToken::parse("fence_reseal_fresh")?,
            )?;
            let candidate = crate::shipping_save_attempt::deterministic_lone_unsynced_successor(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
            )?
            .ok_or("missing deterministic candidate")?;
            if existing_clone {
                let precrash = crate::local_profile::RecoveryClone::create(
                    &source,
                    &root,
                    &fixture.tenant,
                    &fixture.profile,
                    &candidate,
                )?;
                assert_eq!(precrash.verify_clone_only()?, source.inventory()?);
            }
            let renewed = ProfileLease::issue(
                fixture.tenant.clone(),
                fixture.profile.clone(),
                SessionId::parse("session_reseal_after_another_crash")?,
                fixture.lease.device_id().clone(),
                fixture.lease.epoch() + 2,
                FencingToken::parse("fence_reseal_after_another_crash")?,
            )?;
            assert_eq!(
                crate::shipping_save_attempt::deterministic_lone_unsynced_successor(
                    &root,
                    &fixture.tenant,
                    &fixture.profile,
                    &fixture.base,
                    &renewed,
                )?,
                Some(candidate.clone()),
                "interrupted clone may never cause N+2",
            );
            let mut network = Transport {
                responses: VecDeque::from([
                    MachineHttpResponse::new(
                        200, br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                    ),
                    MachineHttpResponse::new(
                        200, br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                    ),
                    MachineHttpResponse::new(
                        200,
                        br#"{"state":"verified","method":null,"url":null,"headers":[],"expiresSeconds":null}"#.to_vec(),
                    ),
                    MachineHttpResponse::new(
                        200, br#"{"outcome":"ACTIVATED"}"#.to_vec(),
                    ),
                ]),
                paths: Vec::new(),
            };
            let mut sealer = Sealer;
            let mut upload = RecoveryPut::default();
            assert_eq!(
                super::resume_lone_local_unsynced_under_fresh_lease(
                    &root,
                    &fixture.tenant,
                    &fixture.profile,
                    &fixture.base,
                    &fresh,
                    &mut sealer,
                    &mut network,
                    &mut upload,
                ),
                Ok(true),
            );
            assert_eq!(network.paths.len(), 4);
            assert!(
                network.paths[0].ends_with("/generation-successor/local-unsynced/retry-decision")
            );
            assert!(network.paths[1].ends_with("/generation-successor/retry-decision"));
            assert!(network.paths[3].ends_with("/generation-successor/commit"));
            assert_eq!(upload.calls, 0);
            assert!(has_unresolved_save_attempt_for_profile(
                &root,
                &fixture.tenant,
                &fixture.profile,
            )?);
            let exact = discover_unresolved_streaming_exact_save_attempt(
                &root,
                &fixture.tenant,
                &fixture.profile,
            )?
            .ok_or("sealed streamed checkpoint was not preserved")?;
            assert!(exact.first_seal_under_current_lease(&fresh));
            assert_eq!(exact.successor_id(), candidate.as_str());
            if large_native_profile {
                assert!(
                    exact.container_bytes() > 64 * 1024 * 1024,
                    "recovered K1 checkpoint must contain the large native WAL"
                );
            }
            assert_eq!(std::fs::read(source.path().join("prefs.js"))?, b"save");
            let mut active = Transport {
                responses: VecDeque::from([MachineHttpResponse::new(
                    200,
                    br#"{"activeExact":true}"#.to_vec(),
                )]),
                paths: Vec::new(),
            };
            assert_eq!(
                complete_active_exact_after_restart(
                    &root,
                    &fixture.tenant,
                    &fixture.profile,
                    &candidate,
                    &renewed,
                    &mut active,
                ),
                Ok(RestartExactLocalCompletion::ActiveExactLocalReady),
            );
            assert!(!has_unresolved_save_attempt_for_profile(
                &root,
                &fixture.tenant,
                &fixture.profile,
            )?);
            let candidate_workspace =
                root.open_generation(&fixture.tenant, &fixture.profile, &candidate)?;
            assert_eq!(
                std::fs::read(candidate_workspace.path().join("prefs.js"))?,
                b"save"
            );
            if large_native_profile {
                assert_eq!(
                    std::fs::metadata(
                        candidate_workspace
                            .path()
                            .join("user_data/cookies.sqlite-wal")
                    )?
                    .len(),
                    65 * 1024 * 1024,
                    "authorized ACTIVE_EXACT N+1 must retain every native WAL byte"
                );
            }
            let binding = load_materialization_binding(
                &candidate_workspace,
                &fixture.tenant,
                &fixture.profile,
                &candidate,
            )?;
            assert_eq!(binding.source_container_sha256(), exact.container_digest());
        }
        Ok(())
    }

    #[test]
    fn lost_first_lone_commit_response_reuses_original_streamed_n_plus_one_after_restart()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        let first = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_lone_first_lost_commit")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_lone_first_lost_commit")?,
        )?;
        let candidate = crate::shipping_save_attempt::deterministic_lone_unsynced_successor(
            &root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.base,
            &first,
        )?
        .ok_or("original N+1 missing")?;
        let mut first_network = Transport {
            responses: VecDeque::from([
                MachineHttpResponse::new(
                    200,
                    br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                ),
                MachineHttpResponse::new(
                    200,
                    br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                ),
                MachineHttpResponse::new(
                    200,
                    br#"{"state":"verified","method":null,"url":null,"headers":[],"expiresSeconds":null}"#.to_vec(),
                ),
                MachineHttpResponse::new(503, Vec::new()),
            ]),
            paths: Vec::new(),
        };
        let mut sealer = Sealer;
        let mut put = RecoveryPut::default();
        assert_eq!(
            super::resume_lone_local_unsynced_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &first,
                &mut sealer,
                &mut first_network,
                &mut put,
            ),
            Err(RestartExactLocalCompletionError::ControlPlane(
                ShippingControlPlaneError::HttpServerUncertain,
            )),
        );
        assert_eq!(first_network.paths.len(), 4);
        assert_eq!(put.calls, 0);
        let before = discover_unresolved_streaming_exact_save_attempt(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?
        .ok_or("lost commit must retain fsynced BPAT")?;
        assert!(before.first_seal_under_current_lease(&first));
        assert_eq!(before.successor_id(), candidate.as_str());
        let original_digest = before.container_digest().to_owned();
        drop(before);
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);

        // Fresh coordinator authority is mandatory after another process
        // restart; old lease cannot repeat PUT/commit or create N+2.
        let renewed = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_lone_lost_commit_reauthorized")?,
            fixture.lease.device_id().clone(),
            first.epoch() + 1,
            FencingToken::parse("fence_lone_lost_commit_reauthorized")?,
        )?;
        assert_eq!(
            crate::shipping_save_attempt::deterministic_lone_unsynced_successor(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &renewed,
            )?,
            Some(candidate.clone()),
        );
        let mut stale = Transport::default();
        assert_eq!(
            super::resume_exact_successor_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.lease,
                &mut stale,
                &mut put,
            ),
            Err(RestartExactLocalCompletionError::WriterOrLockUnresolved),
        );
        assert!(stale.paths.is_empty());

        let mut retry = Transport {
            responses: VecDeque::from([
                MachineHttpResponse::new(
                    200,
                    br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
                ),
                MachineHttpResponse::new(
                    200,
                    br#"{"state":"verified","method":null,"url":null,"headers":[],"expiresSeconds":null}"#.to_vec(),
                ),
                MachineHttpResponse::new(
                    200,
                    br#"{"outcome":"ACTIVATED"}"#.to_vec(),
                ),
            ]),
            paths: Vec::new(),
        };
        assert_eq!(
            super::resume_exact_successor_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &renewed,
                &mut retry,
                &mut put,
            ),
            Ok(true),
        );
        assert_eq!(retry.paths.len(), 3);
        assert_eq!(put.calls, 0);
        let exact = discover_unresolved_streaming_exact_save_attempt(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?
        .ok_or("original N+1 checkpoint lost on authorized retry")?;
        assert_eq!(exact.successor_id(), candidate.as_str());
        assert_eq!(exact.container_digest(), original_digest);
        assert_eq!(std::fs::read(source.path().join("prefs.js"))?, b"save");

        let active_lease = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_lone_lost_commit_active")?,
            fixture.lease.device_id().clone(),
            renewed.epoch() + 1,
            FencingToken::parse("fence_lone_lost_commit_active")?,
        )?;
        let mut active = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":true}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &candidate,
                &active_lease,
                &mut active,
            ),
            Ok(RestartExactLocalCompletion::ActiveExactLocalReady),
        );
        assert!(!has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        let restored = root.open_generation(&fixture.tenant, &fixture.profile, &candidate)?;
        assert_eq!(std::fs::read(restored.path().join("prefs.js"))?, b"save");
        let binding =
            load_materialization_binding(&restored, &fixture.tenant, &fixture.profile, &candidate)?;
        assert_eq!(binding.source_container_sha256(), original_digest);
        Ok(())
    }

    #[test]
    fn partial_or_mutated_interrupted_lone_clone_never_seals_or_uploads()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_local_unsynced_intent(&root, &source, &fixture.lease)?;
        let fresh = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_partial_clone_retry")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_partial_clone_retry")?,
        )?;
        let candidate = crate::shipping_save_attempt::deterministic_lone_unsynced_successor(
            &root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.base,
            &fresh,
        )?
        .ok_or("candidate missing")?;
        let incomplete = root.create_generation(&fixture.tenant, &fixture.profile, &candidate)?;
        std::fs::write(
            incomplete.path().join("prefs.js"),
            b"malformed-staged-native-file",
        )?;
        let untouched = std::fs::read(incomplete.path().join("prefs.js"))?;
        let mut network = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"verdict":"NOT_APPLIED_RETRYABLE"}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        let mut sealer = Sealer;
        let mut upload = RecoveryPut::default();
        assert_eq!(
            super::resume_lone_local_unsynced_under_fresh_lease(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fresh,
                &mut sealer,
                &mut network,
                &mut upload,
            ),
            Err(RestartExactLocalCompletionError::LocalEvidenceChanged),
        );
        assert_eq!(network.paths.len(), 1);
        assert_eq!(upload.calls, 0);
        assert_eq!(
            std::fs::read(incomplete.path().join("prefs.js"))?,
            untouched
        );
        assert_eq!(std::fs::read(source.path().join("prefs.js"))?, b"save");
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert!(
            discover_unresolved_exact_save_attempt(&root, &fixture.tenant, &fixture.profile,)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn active_exact_foreign_binding_retains_checkpoint_and_releases_owned_lock()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let successor = fixture.prepared.candidate_workspace().generation_id();
        let parent = fixture
            .prepared
            .candidate_workspace()
            .path()
            .parent()
            .ok_or("candidate parent missing")?;
        let foreign = parent.join(format!(".{}.materialization", successor.as_str()));
        std::fs::write(&foreign, b"foreign-materialization-must-not-be-overwritten")?;
        let current = ProfileLease::issue(
            fixture.tenant.clone(),
            fixture.profile.clone(),
            SessionId::parse("session_conflicting_binding")?,
            fixture.lease.device_id().clone(),
            fixture.lease.epoch() + 1,
            FencingToken::parse("fence_conflicting_binding")?,
        )?;
        let mut transport = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":true}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            complete_active_exact_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &current,
                &mut transport,
            ),
            Err(RestartExactLocalCompletionError::BindingOrRetirementFailed),
        );
        assert_eq!(transport.paths.len(), 1);
        assert!(has_unresolved_save_attempt_for_profile(
            &root,
            &fixture.tenant,
            &fixture.profile,
        )?);
        assert_eq!(
            std::fs::read(&foreign)?,
            b"foreign-materialization-must-not-be-overwritten",
        );
        assert!(!source.path().join(".profile-platform.lock").exists());
        assert_eq!(
            std::fs::read(
                fixture
                    .prepared
                    .candidate_workspace()
                    .path()
                    .join("prefs.js")
            )?,
            b"save",
        );
        Ok(())
    }

    #[test]
    fn malformed_upload_capability_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let transport = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"state":"uploadRequired","method":"GET","url":"http://bad.example/object","headers":[],"expiresSeconds":0}"#
                    .to_vec(),
            )]),
            paths: Vec::new(),
        };
        let mut sealer = Sealer;
        let mut client = ControlPlaneGenerationSuccessor::new(transport, &mut sealer);
        assert!(matches!(
            client.upload_authorization(
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.prepared,
                &fixture.lease,
            ),
            Err(GenerationSuccessorControlError::ControlPlane(_))
        ));
        Ok(())
    }
    #[test]
    fn read_only_observe_recovers_exact_committed_answer_without_old_fencing_token()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let root = MaterializationRoot::open_or_create(&fixture.root_path)?;
        let source = root.open_generation(&fixture.tenant, &fixture.profile, &fixture.base)?;
        persist_exact_save_attempt(&root, &source, &fixture.prepared, &fixture.lease)?;
        let successor = fixture.prepared.candidate_workspace().generation_id();

        let mut transport = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":true}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert!(observe_retained_successor_after_restart(
            &root,
            &fixture.tenant,
            &fixture.profile,
            successor,
            &mut transport,
        )?);
        assert_eq!(transport.paths.len(), 1);
        assert!(transport.paths[0].ends_with("/generation-successor/observe"));
        assert!(!transport.paths[0].ends_with("/commit"));

        // No one supplied the successor ID or the former lease after restart.
        // The discovered local exact checkpoint produces only ONE read-only
        // server observe; it must not repeat the old fence or any cloud PUT.
        let mut discovered = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":true}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            observe_discovered_successor_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &mut discovered,
            )?,
            RestartExactCommitObservation::ActiveExact
        );
        assert_eq!(discovered.paths.len(), 1);
        assert!(discovered.paths[0].ends_with("/generation-successor/observe"));
        let mut unrelated = Transport::default();
        assert_eq!(
            observe_discovered_successor_after_restart(
                &root,
                &fixture.tenant,
                &ProfileId::parse("profile_unrelated_pending_recovery")?,
                &mut unrelated,
            )?,
            RestartExactCommitObservation::NoCanonicalCheckpoint
        );
        assert!(unrelated.paths.is_empty());

        // A negative exact-pointer observation is NOT proof of an unapplied
        // command and NEVER permits a blind retry using the old fencing token.
        let mut not_active = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":false}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert_eq!(
            observe_discovered_successor_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                &mut not_active,
            )?,
            RestartExactCommitObservation::NotActiveExact
        );
        assert_eq!(not_active.paths.len(), 1);
        assert!(not_active.paths[0].ends_with("/generation-successor/observe"));

        let mut uncertain = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":false}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert!(!observe_retained_successor_after_restart(
            &root,
            &fixture.tenant,
            &fixture.profile,
            successor,
            &mut uncertain,
        )?);
        assert_eq!(uncertain.paths.len(), 1);

        let mut malformed = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"activeExact":true,"untrustedField":1}"#.to_vec(),
            )]),
            paths: Vec::new(),
        };
        assert!(
            observe_retained_successor_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &mut malformed,
            )
            .is_err()
        );

        // A failed authoritative read never grants a publish/reopen outcome.
        // Preserve its error category: an explicit server refusal is not a
        // dropped response and must not be misreported as retryable transport.
        for status in [401, 403, 409, 408, 429, 500, 502, 503, 504] {
            let mut rejected = Transport {
                responses: VecDeque::from([MachineHttpResponse::new(status, Vec::new())]),
                paths: Vec::new(),
            };
            assert_eq!(
                observe_retained_successor_after_restart(
                    &root,
                    &fixture.tenant,
                    &fixture.profile,
                    successor,
                    &mut rejected,
                ),
                Err(if matches!(status, 408 | 429) || status >= 500 {
                    ShippingControlPlaneError::HttpServerUncertain
                } else {
                    ShippingControlPlaneError::HttpStatus
                })
            );
            assert_eq!(rejected.paths.len(), 1);
        }
        let mut unavailable = Transport::default();
        assert_eq!(
            observe_retained_successor_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &mut unavailable,
            ),
            Err(ShippingControlPlaneError::Transport)
        );
        assert_eq!(unavailable.paths.len(), 1);

        let mutated = fixture
            .prepared
            .candidate_workspace()
            .path()
            .join("prefs.js");
        std::fs::write(mutated, b"mutated-after-checkpoint")?;
        let mut never_network = Transport::default();
        assert!(
            observe_retained_successor_after_restart(
                &root,
                &fixture.tenant,
                &fixture.profile,
                successor,
                &mut never_network,
            )
            .is_err()
        );
        assert!(never_network.paths.is_empty());
        Ok(())
    }
}
