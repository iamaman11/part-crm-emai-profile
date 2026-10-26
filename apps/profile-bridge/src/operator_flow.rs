use crate::ProcessControlPort;
use crate::authoritative_generation::{
    AuthoritativeGenerationMaterializationError, ensure_authoritative_generation,
};
use crate::browser_execution::browser_writer_locks_absent;
#[cfg(any(test, feature = "synthetic-test-bin"))]
use crate::browser_mail_query::BrowserMailExecutionProof;
#[cfg(any(test, feature = "synthetic-test-bin"))]
use crate::dirty_close::RetainedDirtyCloseError;
use crate::dirty_close::{DirtyCloseCompletion, DirtyCloseLocalOutcome, RetainedDirtyClose};
#[cfg(any(test, feature = "synthetic-test-bin"))]
use crate::dirty_generation::PreparedDirtyGeneration;
use crate::dirty_generation::{DirtyGenerationError, GenerationSealingMaterialPort};
#[cfg(any(test, feature = "synthetic-test-bin"))]
use crate::dirty_generation_finalize::DirtyGenerationCommitClientPort;
use crate::generation_reopen::{
    GenerationObjectDownloadError, GenerationObjectDownloadPort, GenerationReopenControlError,
    GenerationReopenControlPort,
};
use crate::local_profile::{
    BridgeWorkspaceLock, GenerationWorkspace, LocalGenerationRecord, LocalGenerationState,
    LocalProfileError, MaterializationRoot,
};
use crate::runtime_bundle::{
    ApprovedRuntimeBundle, RuntimeLaunchError, RuntimeSessionOrchestrator,
};
use crate::shipping_control_plane::{MachineHttpPort, ShippingControlPlaneError};
use crate::shipping_generation_save::{
    CommittedGenerationSuccessor, ShippingGenerationSaveError, SignedGenerationObjectPutPort,
    prepare_retained_generation_successor, publish_verify_and_commit_successor,
};
use crate::shipping_generation_successor_control::{
    ControlPlaneGenerationSuccessor, GenerationSuccessorControlError,
};
use crate::shipping_save_attempt::{
    SaveAttemptEvidenceError, has_unresolved_save_attempt_for_profile, inspect_exact_save_attempt,
    persist_exact_save_attempt, persist_local_unsynced_intent, retire_fully_saved_attempt,
    retire_local_unsynced_after_exact_checkpoint,
};
#[cfg(any(test, feature = "synthetic-test-bin"))]
use application_ports::generation_objects::{
    GenerationObjectExactVerifyPort, GenerationObjectUploadPort,
};
use application_ports::{ProfileCoordinatorPort, ProfileCoordinatorRuntimePort};
use bridge_domain::{CamouhostPort, ClaimUri, DeviceIdentityPort, DeviceKeyPort};
#[cfg(any(test, feature = "synthetic-test-bin"))]
use profile_platform_primitives::TenantScope;
use profile_platform_primitives::{
    ActorContext, DeviceId, GenerationId, LaunchIntentId, ProfileId, SessionId, UnixMillis,
};
use session_domain::ProfileLease;
use std::fmt;

pub trait DeviceAuthenticationPort {
    type Error;

    fn authenticate(&mut self, device_id: &DeviceId, key_handle: &str) -> Result<(), Self::Error>;
}

pub trait EnrollmentPort {
    type Error;

    fn redeem_claim(
        &mut self,
        claim: &ClaimUri,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<OperatorEnrollment, Self::Error>;
}

pub trait RuntimeBundleSelectionPort {
    type Error;

    fn select_bundle(&mut self) -> Result<ApprovedRuntimeBundle, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationalRejectionReason {
    BrowserPreflightRejected,
    RuntimeCandidateMismatch,
    ConfigIntegrityMismatch,
    IdentityObservationMismatch,
    ProfileBusy,
    RecoveryRequired,
    RetryableNetworkRouteChurn,
    LocalCacheIncomplete,
    LocalCacheInaccessible,
    FilesystemProcessCapabilityUnavailable,
}

impl OperationalRejectionReason {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::BrowserPreflightRejected => "browser_preflight_rejected",
            Self::RuntimeCandidateMismatch => "runtime_candidate_mismatch",
            Self::ConfigIntegrityMismatch => "config_integrity_mismatch",
            Self::IdentityObservationMismatch => "identity_observation_mismatch",
            Self::ProfileBusy => "profile_busy",
            Self::RecoveryRequired => "recovery_required",
            Self::RetryableNetworkRouteChurn => "retryable_network_route_churn",
            Self::LocalCacheIncomplete => "local_cache_incomplete",
            Self::LocalCacheInaccessible => "local_cache_inaccessible",
            Self::FilesystemProcessCapabilityUnavailable => {
                "filesystem_process_capability_unavailable"
            }
        }
    }
}

pub trait BrowserLaunchPreflightPort {
    type Error;

    fn operational_rejection_reason(error: &Self::Error) -> OperationalRejectionReason {
        let _ = error;
        OperationalRejectionReason::BrowserPreflightRejected
    }

    fn evaluate_before_launch(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        workspace_epoch: u64,
        runtime_bundle: &ApprovedRuntimeBundle,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorEnrollment {
    actor: ActorContext,
    profile_id: ProfileId,
    generation_id: GenerationId,
    launch_intent_id: LaunchIntentId,
}

impl OperatorEnrollment {
    #[must_use]
    pub const fn new(
        actor: ActorContext,
        profile_id: ProfileId,
        generation_id: GenerationId,
        launch_intent_id: LaunchIntentId,
    ) -> Self {
        Self {
            actor,
            profile_id,
            generation_id,
            launch_intent_id,
        }
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorContext {
        &self.actor
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn launch_intent_id(&self) -> &LaunchIntentId {
        &self.launch_intent_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorFailureStage {
    DeviceIdentity,
    DeviceKey,
    DeviceAuthentication,
    Enrollment,
    RuntimeBundle,
    DeliveryHealthStart,
    CoordinatorClaim,
    CoordinatorHeartbeat,
    LeaseValidation,
    AuthoritativeGeneration,
    LocalWorkspace,
    BrowserPreflight,
    LocalLifecycle,
    RuntimeLaunch,
    RuntimeClose,
    GenerationSave,
    RuntimeAbort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanupFailures {
    process: bool,
    workspace_lock: bool,
    coordinator_lease: bool,
}

impl CleanupFailures {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            process: false,
            workspace_lock: false,
            coordinator_lease: false,
        }
    }

    #[must_use]
    pub const fn process(self) -> bool {
        self.process
    }

    #[must_use]
    pub const fn workspace_lock(self) -> bool {
        self.workspace_lock
    }

    #[must_use]
    pub const fn coordinator_lease(self) -> bool {
        self.coordinator_lease
    }

    #[must_use]
    pub const fn any(self) -> bool {
        self.process || self.workspace_lock || self.coordinator_lease
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorTerminalRecord {
    session_id: SessionId,
    generation_id: GenerationId,
    local_state: LocalGenerationState,
    cleanup_failures: CleanupFailures,
}

impl OperatorTerminalRecord {
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn local_state(&self) -> LocalGenerationState {
        self.local_state
    }

    #[must_use]
    pub const fn cleanup_failures(&self) -> CleanupFailures {
        self.cleanup_failures
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorGenerationSaveCompletion {
    committed: CommittedGenerationSuccessor,
    local: DirtyCloseCompletion,
    checkpoint_retired: bool,
}

impl OperatorGenerationSaveCompletion {
    #[must_use]
    pub const fn committed(&self) -> &CommittedGenerationSuccessor {
        &self.committed
    }

    #[must_use]
    pub const fn local(&self) -> &DirtyCloseCompletion {
        &self.local
    }

    /// `Saved` is intentionally stronger than backend commit. It requires the exact committed
    /// candidate to remain locally accepted and both retained ownership layers to be released.
    #[must_use]
    pub fn is_saved(&self) -> bool {
        self.local.is_fully_saved_locally() && self.checkpoint_retired
    }
}

/// Read-only projection of the *existing* Save owners' typed failure.
/// This is not a persisted phase, retry grant, cleanup policy, or second SM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationSaveFailureReason {
    OwnerState,
    LocalSource,
    LocalIntentWrite,
    CandidateIdentity,
    NativeSourceChanged,
    SnapshotTooLarge,
    SealingMaterial,
    Encryption,
    LocalPreparation,
    CheckpointWrite,
    CheckpointRead,
    CheckpointIntegrity,
    CheckpointRetirement,
    ControlTransport,
    ControlEffectUncertain,
    ControlRejected,
    ControlInvalidResponse,
    ControlClock,
    UploadOutcomeUncertain,
    VerificationUnproven,
    DescriptorMismatch,
}

impl GenerationSaveFailureReason {
    /// Only an ambiguous cloud interaction may enter the existing bounded
    /// same-attempt reconnect loop. Local corruption and explicit rejection
    /// are never "fixed" by another PUT.
    #[must_use]
    pub const fn allows_bounded_cloud_reconcile(self) -> bool {
        matches!(
            self,
            Self::ControlTransport
                | Self::ControlEffectUncertain
                | Self::UploadOutcomeUncertain
                | Self::VerificationUnproven
        )
    }

    /// Stable, redacted user/support code, never a storage key or raw HTTP.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::OwnerState => "BRIDGE_SAVE_OWNER_STATE_INVALID",
            Self::LocalSource => "BRIDGE_SAVE_LOCAL_SOURCE_UNAVAILABLE",
            Self::LocalIntentWrite => "BRIDGE_SAVE_LOCAL_INTENT_FAILED",
            Self::CandidateIdentity => "BRIDGE_SAVE_CANDIDATE_IDENTITY_INVALID",
            Self::NativeSourceChanged => "BRIDGE_SAVE_NATIVE_SOURCE_CHANGED",
            Self::SnapshotTooLarge => "BRIDGE_SAVE_SNAPSHOT_TOO_LARGE",
            Self::SealingMaterial => "BRIDGE_SAVE_SEALING_MATERIAL_UNAVAILABLE",
            Self::Encryption => "BRIDGE_SAVE_ENCRYPTION_FAILED",
            Self::LocalPreparation => "BRIDGE_SAVE_LOCAL_PREPARATION_FAILED",
            Self::CheckpointWrite => "BRIDGE_SAVE_CHECKPOINT_WRITE_FAILED",
            Self::CheckpointRead => "BRIDGE_SAVE_CHECKPOINT_READ_FAILED",
            Self::CheckpointIntegrity => "BRIDGE_SAVE_CHECKPOINT_INTEGRITY_FAILED",
            Self::CheckpointRetirement => "BRIDGE_SAVE_CHECKPOINT_RETIRE_FAILED",
            Self::ControlTransport => "BRIDGE_SAVE_CONTROL_TRANSPORT_UNKNOWN",
            Self::ControlEffectUncertain => "BRIDGE_SAVE_CONTROL_EFFECT_UNCERTAIN",
            Self::ControlRejected => "BRIDGE_SAVE_CONTROL_REJECTED",
            Self::ControlInvalidResponse => "BRIDGE_SAVE_CONTROL_RESPONSE_INVALID",
            Self::ControlClock => "BRIDGE_SAVE_CONTROL_CLOCK_FAILED",
            Self::UploadOutcomeUncertain => "BRIDGE_SAVE_UPLOAD_OUTCOME_UNCERTAIN",
            Self::VerificationUnproven => "BRIDGE_SAVE_SERVER_VERIFY_NOT_PROVEN",
            Self::DescriptorMismatch => "BRIDGE_SAVE_DESCRIPTOR_MISMATCH",
        }
    }
}

fn save_control_reason<S>(
    source: GenerationSuccessorControlError<S>,
) -> GenerationSaveFailureReason {
    use GenerationSaveFailureReason as Reason;
    match source {
        GenerationSuccessorControlError::Sealing(_) => Reason::SealingMaterial,
        GenerationSuccessorControlError::ControlPlane(error) => match error {
            ShippingControlPlaneError::Transport => Reason::ControlTransport,
            ShippingControlPlaneError::HttpServerUncertain => Reason::ControlEffectUncertain,
            ShippingControlPlaneError::HttpStatus => Reason::ControlRejected,
            ShippingControlPlaneError::InvalidResponse => Reason::ControlInvalidResponse,
            ShippingControlPlaneError::Clock => Reason::ControlClock,
        },
    }
}

fn save_generation_reason<S, U>(
    source: ShippingGenerationSaveError<GenerationSuccessorControlError<S>, U>,
) -> GenerationSaveFailureReason {
    use GenerationSaveFailureReason as Reason;
    match source {
        ShippingGenerationSaveError::InvalidRetainedAuthority => Reason::OwnerState,
        ShippingGenerationSaveError::CandidateIdentity => Reason::CandidateIdentity,
        ShippingGenerationSaveError::Prepare(error) => match error {
            DirtyGenerationError::InvalidState => Reason::OwnerState,
            DirtyGenerationError::IdentityMismatch | DirtyGenerationError::CandidateMatchesBase => {
                Reason::CandidateIdentity
            }
            DirtyGenerationError::SnapshotTooLarge => Reason::SnapshotTooLarge,
            DirtyGenerationError::SourceChanged => Reason::NativeSourceChanged,
            DirtyGenerationError::KeyUnavailable => Reason::SealingMaterial,
            DirtyGenerationError::EncryptionFailed => Reason::Encryption,
            DirtyGenerationError::Local(_) => Reason::LocalPreparation,
        },
        ShippingGenerationSaveError::Control(source) => save_control_reason(source),
        // A signed PUT error cannot tell whether the storage effect applied.
        ShippingGenerationSaveError::Upload(_) => Reason::UploadOutcomeUncertain,
        ShippingGenerationSaveError::VerificationNotProven => Reason::VerificationUnproven,
        ShippingGenerationSaveError::DescriptorMismatch => Reason::DescriptorMismatch,
    }
}

fn save_checkpoint_reason(source: SaveAttemptEvidenceError) -> GenerationSaveFailureReason {
    match source {
        SaveAttemptEvidenceError::Io(_) | SaveAttemptEvidenceError::Unavailable => {
            GenerationSaveFailureReason::CheckpointWrite
        }
        SaveAttemptEvidenceError::Invalid | SaveAttemptEvidenceError::Changed => {
            GenerationSaveFailureReason::CheckpointIntegrity
        }
    }
}

fn save_checkpoint_read_reason(source: SaveAttemptEvidenceError) -> GenerationSaveFailureReason {
    match source {
        SaveAttemptEvidenceError::Io(_) | SaveAttemptEvidenceError::Unavailable => {
            GenerationSaveFailureReason::CheckpointRead
        }
        SaveAttemptEvidenceError::Invalid | SaveAttemptEvidenceError::Changed => {
            GenerationSaveFailureReason::CheckpointIntegrity
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorFlowError {
    Busy,
    CleanupRequired,
    /// The HTTP effect/response is uncertain; the last confirmed coordinator
    /// deadline has not been extended. Caller must retry or quiesce before it.
    HeartbeatTransportUncertain,
    Stage(OperatorFailureStage),
    GenerationSave {
        reason: GenerationSaveFailureReason,
    },
    AuthoritativeGeneration {
        reason: AuthoritativeGenerationMaterializationError,
        cleanup: CleanupFailures,
    },
    Preflight {
        reason: OperationalRejectionReason,
        local_state: LocalGenerationState,
        cleanup: CleanupFailures,
    },
    Runtime {
        stage: OperatorFailureStage,
        source: RuntimeLaunchError,
        cleanup: CleanupFailures,
    },
    Terminal {
        stage: OperatorFailureStage,
        local_state: LocalGenerationState,
        cleanup: CleanupFailures,
    },
}

impl OperatorFlowError {
    #[must_use]
    pub const fn stage(&self) -> Option<OperatorFailureStage> {
        match self {
            Self::Busy | Self::CleanupRequired => None,
            Self::GenerationSave { .. } => Some(OperatorFailureStage::GenerationSave),
            Self::HeartbeatTransportUncertain => Some(OperatorFailureStage::CoordinatorHeartbeat),
            Self::Stage(stage) | Self::Runtime { stage, .. } | Self::Terminal { stage, .. } => {
                Some(*stage)
            }
            Self::AuthoritativeGeneration { .. } => {
                Some(OperatorFailureStage::AuthoritativeGeneration)
            }
            Self::Preflight { .. } => Some(OperatorFailureStage::BrowserPreflight),
        }
    }
}

impl fmt::Display for OperatorFlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => formatter.write_str("an operator session is already active"),
            Self::CleanupRequired => formatter.write_str("previous operator cleanup is unresolved"),
            Self::HeartbeatTransportUncertain => {
                formatter.write_str("coordinator heartbeat transport result is uncertain")
            }
            Self::Stage(stage) => write!(formatter, "operator flow failed at {stage:?}"),
            Self::GenerationSave { reason } => {
                write!(
                    formatter,
                    "operator generation save failed: {}",
                    reason.code()
                )
            }
            Self::AuthoritativeGeneration { reason, cleanup } => write!(
                formatter,
                "authoritative generation preparation failed: {reason}; cleanup={cleanup:?}"
            ),
            Self::Preflight {
                reason,
                local_state,
                cleanup,
            } => write!(
                formatter,
                "operator browser preflight rejected: reason={}; local_state={local_state:?}; cleanup={cleanup:?}",
                reason.code()
            ),
            Self::Runtime {
                stage,
                source,
                cleanup,
            } => write!(
                formatter,
                "operator runtime failed at {stage:?}: {source}; cleanup={cleanup:?}"
            ),
            Self::Terminal {
                stage,
                local_state,
                cleanup,
            } => write!(
                formatter,
                "operator flow failed at {stage:?}; local_state={local_state:?}; cleanup={cleanup:?}"
            ),
        }
    }
}

impl std::error::Error for OperatorFlowError {}

struct ActiveOperatorSession {
    lease: ProfileLease,
    workspace_lock: Option<BridgeWorkspaceLock>,
    local_record: LocalGenerationRecord,
    runtime_bundle: ApprovedRuntimeBundle,
}


/* A transient, read-only observation from the ONE operator owner. Not a
   persisted ProfileStatus, permission, cloud fact, or second state machine. */
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperatorDiagnosticFacts {
    pub(crate) active_local: Option<LocalGenerationState>,
    pub(crate) retained_local: Option<LocalGenerationState>,
    pub(crate) candidate_prepared: bool,
    pub(crate) cleanup_blocked: bool,
}

pub struct ProfileBridgeOperator<D, K, A, E, C, R, B, P, H> {
    device_identity: D,
    device_keys: K,
    device_authentication: A,
    enrollment: E,
    coordinator: C,
    runtime_bundles: R,
    browser_preflight: B,
    process: P,
    camouhost: H,
    active: Option<ActiveOperatorSession>,
    retained_dirty: Option<RetainedDirtyClose>,
    last_terminal: Option<OperatorTerminalRecord>,
    cleanup_blocked: bool,
}

impl<D, K, A, E, C, R, B, P, H> ProfileBridgeOperator<D, K, A, E, C, R, B, P, H>
where
    D: DeviceIdentityPort,
    K: DeviceKeyPort,
    A: DeviceAuthenticationPort,
    E: EnrollmentPort,
    C: ProfileCoordinatorPort,
    R: RuntimeBundleSelectionPort,
    B: BrowserLaunchPreflightPort,
    P: ProcessControlPort,
    H: CamouhostPort,
{
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        device_identity: D,
        device_keys: K,
        device_authentication: A,
        enrollment: E,
        coordinator: C,
        runtime_bundles: R,
        browser_preflight: B,
        process: P,
        camouhost: H,
    ) -> Self {
        Self {
            device_identity,
            device_keys,
            device_authentication,
            enrollment,
            coordinator,
            runtime_bundles,
            browser_preflight,
            process,
            camouhost,
            active: None,
            retained_dirty: None,
            last_terminal: None,
            cleanup_blocked: false,
        }
    }

    /// Production P2 launch entrypoint. The server-selected authoritative generation is proven
    /// and rematerialized, when absent locally, after the exact coordinator lease is acquired and
    /// before the existing local preflight/runtime path is allowed to observe the workspace.
    pub fn open_authoritative<Dl>(
        &mut self,
        claim: &ClaimUri,
        root: &MaterializationRoot,
        downloader: &mut Dl,
        now: UnixMillis,
    ) -> Result<(), OperatorFlowError>
    where
        C: GenerationReopenControlPort,
        <C as GenerationReopenControlPort>::Error: Into<GenerationReopenControlError>,
        Dl: GenerationObjectDownloadPort,
        Dl::Error: Into<GenerationObjectDownloadError>,
    {
        self.open_authoritative_with_runtime_health(claim, root, downloader, now, || Ok(()))
    }

    /// Installed delivery may durably begin its one-time health attempt exactly when the
    /// verified profile is ready to start the real managed runtime. A profile/cache/network
    /// refusal before this callback must not consume the installation's health attempt.
    pub fn open_authoritative_with_runtime_health<Dl, BeforeRuntime>(
        &mut self,
        claim: &ClaimUri,
        root: &MaterializationRoot,
        downloader: &mut Dl,
        now: UnixMillis,
        before_runtime_start: BeforeRuntime,
    ) -> Result<(), OperatorFlowError>
    where
        C: GenerationReopenControlPort,
        <C as GenerationReopenControlPort>::Error: Into<GenerationReopenControlError>,
        Dl: GenerationObjectDownloadPort,
        Dl::Error: Into<GenerationObjectDownloadError>,
        BeforeRuntime: FnMut() -> Result<(), ()>,
    {
        // Preserve the one accepted authoritative launch body. Pending-save
        // observation is optional and never replaces its lease/admission checks.
        self.open_with_materialization(
            claim,
            root,
            now,
            |coordinator, tenant_id, profile_id, generation_id, device_id, epoch| {
                ensure_authoritative_generation(
                    root,
                    tenant_id,
                    profile_id,
                    generation_id,
                    device_id,
                    epoch,
                    coordinator,
                    downloader,
                )
            },
            before_runtime_start,
            |_coordinator, _tenant, _profile, _selected, _lease| false,
        )
    }

    /// Recover interrupted local generation evidence BEFORE browser use,
    /// under the single existing freshly claimed coordinator writer. The
    /// callback may first-seal the original N+1 or finish an exact checkpoint;
    /// only a completed ACTIVE_EXACT binding may allow an ordinary Open.
    /// The unresolved-checkpoint guard remains authoritative.
    pub fn open_authoritative_with_runtime_health_and_pending_recovery<
        Dl,
        BeforeRuntime,
        ObservePending,
    >(
        &mut self,
        claim: &ClaimUri,
        root: &MaterializationRoot,
        downloader: &mut Dl,
        now: UnixMillis,
        before_runtime_start: BeforeRuntime,
        observe_pending: ObservePending,
    ) -> Result<(), OperatorFlowError>
    where
        C: GenerationReopenControlPort,
        <C as GenerationReopenControlPort>::Error: Into<GenerationReopenControlError>,
        Dl: GenerationObjectDownloadPort,
        Dl::Error: Into<GenerationObjectDownloadError>,
        BeforeRuntime: FnMut() -> Result<(), ()>,
        ObservePending: FnMut(
            &mut C,
            &profile_platform_primitives::TenantId,
            &ProfileId,
            &GenerationId,
            &ProfileLease,
        ) -> bool,
    {
        self.open_with_materialization(
            claim,
            root,
            now,
            |coordinator, tenant_id, profile_id, generation_id, device_id, epoch| {
                ensure_authoritative_generation(
                    root,
                    tenant_id,
                    profile_id,
                    generation_id,
                    device_id,
                    epoch,
                    coordinator,
                    downloader,
                )
            },
            before_runtime_start,
            observe_pending,
        )
    }

    /// Test/synthetic entrypoint for already-materialized fixtures. Shipping code cannot compile
    /// against this predecessor shortcut; the production binary must use `open_authoritative`.
    #[cfg(any(test, feature = "synthetic-test-bin"))]
    pub fn open(
        &mut self,
        claim: &ClaimUri,
        root: &MaterializationRoot,
        now: UnixMillis,
    ) -> Result<(), OperatorFlowError> {
        self.open_with_materialization(
            claim,
            root,
            now,
            |_coordinator, _tenant_id, _profile_id, _generation_id, _device_id, _epoch| Ok(()),
            || Ok(()),
            |_coordinator, _tenant, _profile, _selected, _lease| false,
        )
    }

    fn open_with_materialization<F, BeforeRuntime, ObservePending>(
        &mut self,
        claim: &ClaimUri,
        root: &MaterializationRoot,
        now: UnixMillis,
        mut ensure_materialized: F,
        mut before_runtime_start: BeforeRuntime,
        mut observe_pending: ObservePending,
    ) -> Result<(), OperatorFlowError>
    where
        F: FnMut(
            &mut C,
            &profile_platform_primitives::TenantId,
            &ProfileId,
            &GenerationId,
            &DeviceId,
            u64,
        ) -> Result<(), AuthoritativeGenerationMaterializationError>,
        BeforeRuntime: FnMut() -> Result<(), ()>,
        ObservePending: FnMut(
            &mut C,
            &profile_platform_primitives::TenantId,
            &ProfileId,
            &GenerationId,
            &ProfileLease,
        ) -> bool,
    {
        if self.cleanup_blocked {
            return Err(OperatorFlowError::CleanupRequired);
        }
        if self.active.is_some() || self.retained_dirty.is_some() {
            return Err(OperatorFlowError::Busy);
        }
        self.last_terminal = None;

        // Verify installed bytes before creating any short-lived authenticated launch intent.
        // Exact generation/runtime compatibility is still enforced by browser_preflight below.
        let runtime_bundle = self
            .runtime_bundles
            .select_bundle()
            .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::RuntimeBundle))?;
        let device_id = self
            .device_identity
            .device_id()
            .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::DeviceIdentity))?;
        let key_handle = self
            .device_keys
            .ensure_key_handle(&device_id)
            .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::DeviceKey))?;
        self.device_authentication
            .authenticate(&device_id, &key_handle)
            .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::DeviceAuthentication))?;
        let enrollment = self
            .enrollment
            .redeem_claim(claim, &device_id, now)
            .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::Enrollment))?;
        let lease = self
            .coordinator
            .claim_launch_intent(
                enrollment.actor(),
                enrollment.profile_id(),
                &device_id,
                enrollment.launch_intent_id(),
            )
            .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::CoordinatorClaim))?;

        if lease.tenant_id() != enrollment.actor().tenant_scope().tenant_id()
            || lease.profile_id() != enrollment.profile_id()
            || lease.device_id() != &device_id
        {
            return Err(self.fail_before_local_use(OperatorFailureStage::LeaseValidation, lease));
        }

        // Any surviving exact save or interrupted checkpoint stage is unresolved
        // recovery evidence. Even if the old cloud pointer is still accessible,
        // launching it as a clean browser could discard newer local SQLite/WAL.
        // Do not consume the browser/runtime path or start a download. The
        // coordinator lease is conservatively closed, never marked Clean.
        if !matches!(
            has_unresolved_save_attempt_for_profile(
                root,
                enrollment.actor().tenant_scope().tenant_id(),
                enrollment.profile_id(),
            ),
            Ok(false)
        ) {
            // Freshly authenticated device/enrollment and lease have been
            // checked. Reconcile the exact server pointer READ-ONLY; an HTTP
            // miss, uncertain reply or commit observation is not enough to
            // declare local successor ready or re-enter the old writer fence.
            let recovered = observe_pending(
                &mut self.coordinator,
                enrollment.actor().tenant_scope().tenant_id(),
                enrollment.profile_id(),
                enrollment.generation_id(),
                &lease,
            );
            // A recovery callback never overrides the conservative guard.
            // Only the specific fully completed checkpoint may disappear; a
            // torn sibling, unsafe writer or failed local binding still blocks.
            if !recovered
                || !matches!(
                    has_unresolved_save_attempt_for_profile(
                        root,
                        enrollment.actor().tenant_scope().tenant_id(),
                        enrollment.profile_id(),
                    ),
                    Ok(false)
                )
            {
                return Err(self.fail_before_local_use(OperatorFailureStage::GenerationSave, lease));
            }
        }

        if let Err(reason) = ensure_materialized(
            &mut self.coordinator,
            enrollment.actor().tenant_scope().tenant_id(),
            enrollment.profile_id(),
            enrollment.generation_id(),
            &device_id,
            lease.epoch(),
        ) {
            // The natural materialization owner certifies no writes only for a failed read
            // before workspace creation. Verify exact absence again before requesting the
            // typed no-browser-use release. Any new directory, dirty/unknown evidence,
            // failed cleanup, stale fence or potential writer remains Uncertain.
            let clean_no_use = reason.proves_no_local_write_and_absent_workspace()
                && matches!(
                    root.open_generation(
                        enrollment.actor().tenant_scope().tenant_id(),
                        enrollment.profile_id(),
                        enrollment.generation_id(),
                    ),
                    Err(LocalProfileError::Io(std::io::ErrorKind::NotFound))
                );
            let coordinator_lease = if clean_no_use {
                self.coordinator.close_without_browser_use(&lease).is_err()
            } else {
                self.coordinator.close_lease(&lease).is_err()
            };
            let cleanup = CleanupFailures {
                process: false,
                workspace_lock: false,
                coordinator_lease,
            };
            if cleanup.any() {
                self.cleanup_blocked = true;
            }
            return Err(OperatorFlowError::AuthoritativeGeneration { reason, cleanup });
        }

        let workspace = match root.open_generation(
            enrollment.actor().tenant_scope().tenant_id(),
            enrollment.profile_id(),
            enrollment.generation_id(),
        ) {
            Ok(value) => value,
            Err(_) => {
                return Err(self.fail_before_local_use(OperatorFailureStage::LocalWorkspace, lease));
            }
        };
        // A new server-authorized lease may follow a Bridge/Windows crash, leaving
        // only the previous generation's persistent marker. Do not treat the marker
        // as a live writer, but do not unlink it blindly: the recovery owner proves
        // an exclusive OS root admission, same device/older epoch and no active
        // native Firefox writer before it replaces *only* that marker.
        let workspace_lock =
            match BridgeWorkspaceLock::acquire(&workspace, &device_id, lease.epoch()) {
                Ok(value) => value,
                Err(LocalProfileError::LockBusy) => {
                    match BridgeWorkspaceLock::recover_interrupted_launch(
                        &workspace,
                        &device_id,
                        lease.epoch(),
                    ) {
                        Ok(value) => value,
                        Err(_) => {
                            return Err(self.fail_before_local_use(
                                OperatorFailureStage::LocalWorkspace,
                                lease,
                            ));
                        }
                    }
                }
                Err(_) => {
                    return Err(
                        self.fail_before_local_use(OperatorFailureStage::LocalWorkspace, lease)
                    );
                }
            };
        if let Err(error) = self.browser_preflight.evaluate_before_launch(
            &workspace,
            &device_id,
            lease.epoch(),
            &runtime_bundle,
        ) {
            let reason = B::operational_rejection_reason(&error);
            return Err(self.fail_preflight_with_lock_before_use(
                reason,
                lease,
                workspace_lock,
                &workspace,
            ));
        }
        let inventory = match workspace.inventory() {
            Ok(value) => value,
            Err(_) => {
                return Err(self.fail_with_lock_before_use(
                    OperatorFailureStage::LocalWorkspace,
                    lease,
                    workspace_lock,
                    &workspace,
                ));
            }
        };
        // Only this exact boundary may consume pending delivery health. The runtime has
        // not been spawned or allowed to mutate the workspace at this point.
        if before_runtime_start().is_err() {
            return Err(self.fail_with_lock_before_use(
                OperatorFailureStage::DeliveryHealthStart,
                lease,
                workspace_lock,
                &workspace,
            ));
        }
        let mut local_record = LocalGenerationRecord::new(
            enrollment.generation_id().clone(),
            inventory.total_bytes(),
            now,
        );
        if local_record.set_locked(true).is_err() || local_record.begin_use(now).is_err() {
            return Err(self.fail_with_lock_before_use(
                OperatorFailureStage::LocalLifecycle,
                lease,
                workspace_lock,
                &workspace,
            ));
        }

        if let Err(source) = RuntimeSessionOrchestrator::launch(
            &runtime_bundle,
            lease.session_id(),
            &mut self.process,
            &mut self.camouhost,
        ) {
            let _ = local_record.observe_crash(now);
            let process_failed = matches!(source, RuntimeLaunchError::Rollback { .. });
            let cleanup = self.cleanup_after_local_use(&lease, workspace_lock, process_failed);
            self.record_terminal(&lease, &local_record, cleanup);
            if cleanup.any() {
                self.cleanup_blocked = true;
            }
            return Err(OperatorFlowError::Runtime {
                stage: OperatorFailureStage::RuntimeLaunch,
                source,
                cleanup,
            });
        }

        self.active = Some(ActiveOperatorSession {
            lease,
            workspace_lock: Some(workspace_lock),
            local_record,
            runtime_bundle,
        });
        Ok(())
    }

    pub fn heartbeat(&mut self, now: UnixMillis) -> Result<(), OperatorFlowError>
    where
        C: ProfileCoordinatorRuntimePort,
    {
        let lease = self
            .active
            .as_ref()
            .map(|session| session.lease.clone())
            .ok_or(OperatorFlowError::Stage(
                OperatorFailureStage::CoordinatorHeartbeat,
            ))?;
        if !matches!(self.process.is_running(lease.session_id()), Ok(true)) {
            return Err(self.fail_active_runtime(OperatorFailureStage::RuntimeAbort, now));
        }
        match self.coordinator.heartbeat_lease(&lease) {
            Ok(()) => Ok(()),
            // No coordinator acknowledgment, no extension of authority. The
            // shipping owner may make one bounded retry strictly before the
            // previously confirmed lease deadline, never publish or claim Saved.
            Err(error) if self.coordinator.heartbeat_transport_uncertain(&error) => {
                Err(OperatorFlowError::HeartbeatTransportUncertain)
            }
            // Explicit HTTP rejection, malformed authority or unknown port
            // failures are NOT proof that we may retain a browser writer.
            Err(_) => {
                Err(self.fail_active_runtime(OperatorFailureStage::CoordinatorHeartbeat, now))
            }
        }
    }

    /// A clean native Firefox close retains the original exclusive workspace
    /// and the LIVE coordinator session. Reconfirm that exact session with
    /// the authoritative DO before retrying ANY failed exact cloud effect.
    /// This never reopens the browser, renews from a timeout, or mints a fence.
    /// An unacknowledged response leaves all native data and intent untouched.
    pub fn reprove_retained_save_writer(&mut self) -> Result<(), OperatorFlowError>
    where
        C: ProfileCoordinatorRuntimePort,
    {
        if self.cleanup_blocked || self.active.is_some() {
            return Err(OperatorFlowError::CleanupRequired);
        }
        let lease = self
            .retained_dirty
            .as_ref()
            .map(|retained| retained.lease().clone())
            .ok_or(OperatorFlowError::Stage(
                OperatorFailureStage::GenerationSave,
            ))?;
        match self.coordinator.heartbeat_lease(&lease) {
            Ok(()) => Ok(()),
            Err(error) if self.coordinator.heartbeat_transport_uncertain(&error) => {
                Err(OperatorFlowError::HeartbeatTransportUncertain)
            }
            Err(_) => Err(OperatorFlowError::Stage(
                OperatorFailureStage::CoordinatorHeartbeat,
            )),
        }
    }

    /// Only the shipping owner may call this after local monotonic observation
    /// passes the last *server-confirmed* idle/hard deadline. Unlike temporary
    /// transport uncertainty, positively expired writer authority demands
    /// immediate process fencing, with all native browser bytes retained.
    pub fn stop_after_confirmed_lease_expiry(&mut self, now: UnixMillis) -> OperatorFlowError {
        self.fail_active_runtime(OperatorFailureStage::CoordinatorHeartbeat, now)
    }

    pub fn close(&mut self, now: UnixMillis) -> Result<(), OperatorFlowError> {
        if self.retained_dirty.is_some() {
            return Err(OperatorFlowError::Busy);
        }
        let Some(mut session) = self.active.take() else {
            return Err(OperatorFlowError::Stage(OperatorFailureStage::RuntimeClose));
        };
        let runtime_result = RuntimeSessionOrchestrator::close(
            &session.runtime_bundle,
            session.lease.session_id(),
            &mut self.process,
            &mut self.camouhost,
        );

        match runtime_result {
            Ok(()) => {
                let Some(workspace_lock) = session.workspace_lock.take() else {
                    let _ = session.local_record.observe_crash(now);
                    return Err(self.finish_failed_session(
                        OperatorFailureStage::LocalLifecycle,
                        session,
                        false,
                    ));
                };
                let retained = RetainedDirtyClose::begin_after_browser_close(
                    session.lease,
                    workspace_lock,
                    session.local_record,
                    now,
                )
                .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::LocalLifecycle))?;
                self.retained_dirty = Some(retained);
                Ok(())
            }
            Err(source) => {
                let process_failed = self
                    .process
                    .force_terminate(session.lease.session_id())
                    .is_err();
                let _ = session.local_record.observe_crash(now);
                let cleanup = self.cleanup_active_session(&mut session, process_failed);
                self.record_terminal(&session.lease, &session.local_record, cleanup);
                if cleanup.any() {
                    self.cleanup_blocked = true;
                }
                Err(OperatorFlowError::Runtime {
                    stage: OperatorFailureStage::RuntimeClose,
                    source,
                    cleanup,
                })
            }
        }
    }

    /// Installed controlled-close boundary: after native Firefox has ceased
    /// writing, persist minimal LOCAL_UNSYNCED evidence before cloud-dependent
    /// generation sealing, upload or coordinator commit. The original retained
    /// workspace lock and all original user_data bytes are left in place.
    ///
    /// A failed marker write stops cloud save and never declares Clean/Saved.
    /// The ordinary synthetic close remains available only for its existing
    /// targeted tests and does not create a second shipping entrypoint.
    pub fn close_with_durable_local_intent(
        &mut self,
        root: &MaterializationRoot,
        now: UnixMillis,
    ) -> Result<(), OperatorFlowError> {
        self.close(now)?;
        let retained = self
            .retained_dirty
            .as_ref()
            .ok_or(OperatorFlowError::Stage(
                OperatorFailureStage::GenerationSave,
            ))?;
        let source =
            retained
                .open_base_workspace(root)
                .map_err(|_| OperatorFlowError::GenerationSave {
                    reason: GenerationSaveFailureReason::LocalSource,
                })?;
        persist_local_unsynced_intent(root, &source, retained.lease()).map_err(|_| {
            OperatorFlowError::GenerationSave {
                reason: GenerationSaveFailureReason::LocalIntentWrite,
            }
        })
    }

    /// Canonical ordinary P3 save. Pre-commit failures retain dirty local ownership and the exact
    /// coordinator lease for retry. Once `publish_verify_and_commit_successor` returns, N+1 is the
    /// backend authority permanently; local completion can require recovery but can never fall back
    /// to N or undo the commit.
    pub fn save_retained_successor<T, U>(
        &mut self,
        root: &MaterializationRoot,
        transport: T,
        upload: &mut U,
        now: UnixMillis,
    ) -> Result<OperatorGenerationSaveCompletion, OperatorFlowError>
    where
        C: GenerationSealingMaterialPort,
        T: MachineHttpPort,
        U: SignedGenerationObjectPutPort,
    {
        if self.cleanup_blocked {
            return Err(OperatorFlowError::CleanupRequired);
        }
        let (lease, base_record, workspace) = {
            let retained = self
                .retained_dirty
                .as_ref()
                .ok_or(OperatorFlowError::Stage(
                    OperatorFailureStage::GenerationSave,
                ))?;
            let workspace = retained.open_base_workspace(root).map_err(|_| {
                OperatorFlowError::GenerationSave {
                    reason: GenerationSaveFailureReason::LocalSource,
                }
            })?;
            (
                retained.lease().clone(),
                retained.base_record().clone(),
                workspace,
            )
        };

        let mut control = ControlPlaneGenerationSuccessor::new(transport, &mut self.coordinator);
        if self
            .retained_dirty
            .as_ref()
            .is_some_and(|retained| retained.prepared_successor().is_none())
        {
            let prepared = prepare_retained_generation_successor(
                &base_record,
                &workspace,
                root,
                lease.tenant_id(),
                lease.profile_id(),
                &lease,
                &mut control,
            )
            .map_err(|source| OperatorFlowError::GenerationSave {
                reason: save_generation_reason(source),
            })?;
            self.retained_dirty
                .as_mut()
                .ok_or(OperatorFlowError::Stage(
                    OperatorFailureStage::GenerationSave,
                ))?
                .remember_prepared_successor(prepared)
                .map_err(|_| OperatorFlowError::GenerationSave {
                    reason: GenerationSaveFailureReason::OwnerState,
                })?;
        }

        // No re-seal/re-key/new candidate on an ordinary retry after PUT/verify/
        // commit response loss. A different candidate or changed browser bytes
        // invalidate this attempt rather than causing a destructive rebuild.
        let prepared = self
            .retained_dirty
            .as_ref()
            .and_then(RetainedDirtyClose::prepared_successor)
            .ok_or(OperatorFlowError::Stage(
                OperatorFailureStage::GenerationSave,
            ))?;
        if workspace.inventory().ok().as_ref() != Some(prepared.candidate_inventory())
            || prepared.candidate_workspace().inventory().ok().as_ref()
                != Some(prepared.candidate_inventory())
        {
            return Err(OperatorFlowError::GenerationSave {
                reason: GenerationSaveFailureReason::NativeSourceChanged,
            });
        }

        // Crash-surviving ciphertext and exact attempt identity must be
        // published and re-read BEFORE the first network upload or commit.
        // A conflicting/torn checkpoint is a hard stop; no local bytes are
        // cleaned, no old generation is invalidated, no cloud effect is made.
        persist_exact_save_attempt(root, &workspace, prepared, &lease).map_err(|source| {
            OperatorFlowError::GenerationSave {
                reason: save_checkpoint_reason(source),
            }
        })?;
        let recovered = inspect_exact_save_attempt(
            root,
            lease.tenant_id(),
            lease.profile_id(),
            prepared.candidate_workspace().generation_id(),
        )
        .map_err(|source| OperatorFlowError::GenerationSave {
            reason: save_checkpoint_read_reason(source),
        })?
        .ok_or(OperatorFlowError::GenerationSave {
            reason: GenerationSaveFailureReason::CheckpointIntegrity,
        })?;
        if recovered.ciphertext() != prepared.sealed().container()
            || recovered.container_digest() != prepared.container_digest()
            || !recovered.matches_original_lease(&lease)
        {
            return Err(OperatorFlowError::GenerationSave {
                reason: GenerationSaveFailureReason::CheckpointIntegrity,
            });
        }

        // The complete sealed checkpoint is already crash-durable and
        // independently verified. Retire only the matching tiny offline
        // close intent before PUT: on a crash its full exact checkpoint
        // remains the sole local unsynced proof, never missing evidence.
        retire_local_unsynced_after_exact_checkpoint(root, &workspace, &lease, &recovered)
            .map_err(|_| OperatorFlowError::GenerationSave {
                reason: GenerationSaveFailureReason::CheckpointRetirement,
            })?;

        let committed = publish_verify_and_commit_successor(
            lease.tenant_id(),
            lease.profile_id(),
            base_record.generation_id(),
            prepared,
            &lease,
            &mut control,
            upload,
        )
        .map_err(|source| OperatorFlowError::GenerationSave {
            reason: save_generation_reason(source),
        })?;
        drop(control);

        let (completion, session_id, local_state, checkpoint_retired) = {
            let retained = self
                .retained_dirty
                .as_mut()
                .ok_or(OperatorFlowError::Stage(
                    OperatorFailureStage::GenerationSave,
                ))?;
            let prepared = retained
                .take_prepared_successor()
                .ok_or(OperatorFlowError::Stage(
                    OperatorFailureStage::GenerationSave,
                ))?;
            let session_id = retained.lease().session_id().clone();
            let completion = retained.complete_committed_successor(
                root,
                &prepared,
                &committed,
                &mut self.coordinator,
                now,
            );
            // Retire ONLY after the canonical server commit, exact local
            // readiness publication and both writer/lease releases succeed.
            // Failure preserves the checkpoint and blocks claiming Saved.
            // Neither dirty/unknown nor locally blocked generations are
            // eligible for checkpoint deletion.
            let checkpoint_retired = completion.as_ref().is_ok_and(|outcome| {
                outcome.is_fully_saved_locally()
                    && retire_fully_saved_attempt(root, &prepared, &committed, &lease).is_ok()
            });
            let local_state = retained.base_record().state();
            (completion, session_id, local_state, checkpoint_retired)
        };

        let completion = match completion {
            Ok(value) => value,
            Err(_) => {
                let cleanup = CleanupFailures {
                    process: false,
                    workspace_lock: true,
                    coordinator_lease: true,
                };
                self.retained_dirty = None;
                self.last_terminal = Some(OperatorTerminalRecord {
                    session_id,
                    generation_id: committed.generation_id().clone(),
                    local_state,
                    cleanup_failures: cleanup,
                });
                self.cleanup_blocked = true;
                return Err(OperatorFlowError::Terminal {
                    stage: OperatorFailureStage::GenerationSave,
                    local_state,
                    cleanup,
                });
            }
        };

        let cleanup = CleanupFailures {
            process: false,
            workspace_lock: !completion.workspace_lock_released(),
            coordinator_lease: !completion.coordinator_lease_released(),
        };
        let local_readiness_blocked = matches!(
            completion.local_outcome(),
            DirtyCloseLocalOutcome::RematerializationBlocked { .. }
                | DirtyCloseLocalOutcome::MaterializationBindingBlocked { .. }
        );
        self.retained_dirty = None;
        self.last_terminal = Some(OperatorTerminalRecord {
            session_id,
            generation_id: committed.generation_id().clone(),
            local_state,
            cleanup_failures: cleanup,
        });
        if cleanup.any() || local_readiness_blocked || !checkpoint_retired {
            self.cleanup_blocked = true;
        }

        Ok(OperatorGenerationSaveCompletion {
            committed,
            local: completion,
            checkpoint_retired,
        })
    }

    /// Historical DeviceJob-shaped Bridge finalize remains available only to test/synthetic
    /// fixtures while they are migrated. Shipping production code cannot compile against it.
    #[cfg(any(test, feature = "synthetic-test-bin"))]
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_dirty_close<U, V, M>(
        &mut self,
        scope: &TenantScope,
        proof: &BrowserMailExecutionProof,
        prepared: &PreparedDirtyGeneration,
        upload: &U,
        verifier: &V,
        commit: &M,
        now: UnixMillis,
    ) -> Result<DirtyCloseCompletion, RetainedDirtyCloseError<M::Error>>
    where
        U: GenerationObjectUploadPort,
        V: GenerationObjectExactVerifyPort,
        M: DirtyGenerationCommitClientPort,
    {
        let retained = self
            .retained_dirty
            .as_mut()
            .ok_or(RetainedDirtyCloseError::InvalidRetainedOwnership)?;
        let completion = retained
            .finalize(
                scope,
                proof,
                prepared,
                upload,
                verifier,
                commit,
                &mut self.coordinator,
                now,
            )
            .await?;
        let cleanup = CleanupFailures {
            process: false,
            workspace_lock: !completion.workspace_lock_released(),
            coordinator_lease: !completion.coordinator_lease_released(),
        };
        let terminal = OperatorTerminalRecord {
            session_id: retained.lease().session_id().clone(),
            generation_id: retained.base_record().generation_id().clone(),
            local_state: retained.base_record().state(),
            cleanup_failures: cleanup,
        };
        self.retained_dirty = None;
        self.last_terminal = Some(terminal);
        if cleanup.any() {
            self.cleanup_blocked = true;
        }
        Ok(completion)
    }

    pub fn abort(&mut self, now: UnixMillis) -> Result<OperatorTerminalRecord, OperatorFlowError> {
        let Some(mut session) = self.active.take() else {
            return Err(OperatorFlowError::Stage(OperatorFailureStage::RuntimeAbort));
        };
        let process_failed = self
            .process
            .force_terminate(session.lease.session_id())
            .is_err();
        if session.local_record.observe_crash(now).is_err() {
            return Err(self.finish_failed_session(
                OperatorFailureStage::LocalLifecycle,
                session,
                process_failed,
            ));
        }
        self.finish_session(session, process_failed)
    }

    #[must_use]
    pub fn active_session_id(&self) -> Option<&SessionId> {
        self.active
            .as_ref()
            .map(|session| session.lease.session_id())
    }

    #[must_use]
    pub fn active_local_state(&self) -> Option<LocalGenerationState> {
        self.active
            .as_ref()
            .map(|session| session.local_record.state())
    }

    #[must_use]
    pub fn has_pending_dirty_close(&self) -> bool {
        self.retained_dirty.is_some()
    }

    /// Observe existing local owner facts without I/O, lease refresh, or an
    /// assertion that the browser is still alive or cloud state is current.
    #[must_use]
    pub(crate) fn diagnostic_facts(&self) -> OperatorDiagnosticFacts {
        OperatorDiagnosticFacts {
            active_local: self.active_local_state(),
            retained_local: self.pending_dirty_local_state(),
            candidate_prepared: self
                .retained_dirty
                .as_ref()
                .is_some_and(|dirty| dirty.prepared_successor().is_some()),
            cleanup_blocked: self.cleanup_blocked,
        }
    }

    /// Original closed writer context, descriptive only. This MUST be
    /// re-proved by the server before any retry and NEVER outlive its expiry.
    pub fn retained_save_lease(&self) -> Option<&ProfileLease> {
        self.retained_dirty.as_ref().map(RetainedDirtyClose::lease)
    }

    #[must_use]
    pub fn pending_dirty_local_state(&self) -> Option<LocalGenerationState> {
        self.retained_dirty
            .as_ref()
            .map(|retained| retained.base_record().state())
    }

    #[must_use]
    pub const fn last_terminal(&self) -> Option<&OperatorTerminalRecord> {
        self.last_terminal.as_ref()
    }

    #[must_use]
    pub const fn cleanup_blocked(&self) -> bool {
        self.cleanup_blocked
    }

    #[must_use]
    pub const fn coordinator(&self) -> &C {
        &self.coordinator
    }

    #[must_use]
    pub const fn process(&self) -> &P {
        &self.process
    }

    #[cfg(test)]
    fn process_mut(&mut self) -> &mut P {
        &mut self.process
    }

    fn fail_before_local_use(
        &mut self,
        stage: OperatorFailureStage,
        lease: ProfileLease,
    ) -> OperatorFlowError {
        let coordinator_lease = self.coordinator.close_lease(&lease).is_err();
        if coordinator_lease {
            self.cleanup_blocked = true;
            return OperatorFlowError::Terminal {
                stage,
                local_state: LocalGenerationState::MaterializedClean,
                cleanup: CleanupFailures {
                    process: false,
                    workspace_lock: false,
                    coordinator_lease,
                },
            };
        }
        OperatorFlowError::Stage(stage)
    }

    fn fail_preflight_with_lock_before_use(
        &mut self,
        reason: OperationalRejectionReason,
        lease: ProfileLease,
        workspace_lock: BridgeWorkspaceLock,
        workspace: &GenerationWorkspace,
    ) -> OperatorFlowError {
        // Preflight precedes begin_use/spawn. An unknown lock, writer/recovery verdict or
        // unsuccessful local lock release rules out clean cancellation.
        let eligible = !matches!(
            reason,
            OperationalRejectionReason::ProfileBusy | OperationalRejectionReason::RecoveryRequired
        );
        let cleanup = self.cleanup_before_browser_use(&lease, workspace_lock, workspace, eligible);
        OperatorFlowError::Preflight {
            reason,
            local_state: LocalGenerationState::MaterializedClean,
            cleanup,
        }
    }

    fn fail_with_lock_before_use(
        &mut self,
        stage: OperatorFailureStage,
        lease: ProfileLease,
        workspace_lock: BridgeWorkspaceLock,
        workspace: &GenerationWorkspace,
    ) -> OperatorFlowError {
        let cleanup = self.cleanup_before_browser_use(&lease, workspace_lock, workspace, true);
        if cleanup.any() {
            OperatorFlowError::Terminal {
                stage,
                local_state: LocalGenerationState::MaterializedClean,
                cleanup,
            }
        } else {
            OperatorFlowError::Stage(stage)
        }
    }

    fn cleanup_before_browser_use(
        &mut self,
        lease: &ProfileLease,
        workspace_lock: BridgeWorkspaceLock,
        workspace: &GenerationWorkspace,
        eligible: bool,
    ) -> CleanupFailures {
        // Caller owns this exact Bridge lock; no managed runtime spawn has been tried.
        // Unknown/foreign Firefox locks or failed local cleanup forbid Clean.
        let no_writer = eligible && matches!(browser_writer_locks_absent(workspace), Ok(true));
        let workspace_lock_failed = workspace_lock.release().is_err();
        let coordinator_lease = if no_writer && !workspace_lock_failed {
            self.coordinator.close_without_browser_use(lease).is_err()
        } else {
            self.coordinator.close_lease(lease).is_err()
        };
        let cleanup = CleanupFailures {
            process: false,
            workspace_lock: workspace_lock_failed,
            coordinator_lease,
        };
        if cleanup.any() {
            self.cleanup_blocked = true;
        }
        cleanup
    }

    fn fail_active_runtime(
        &mut self,
        stage: OperatorFailureStage,
        now: UnixMillis,
    ) -> OperatorFlowError {
        let Some(mut session) = self.active.take() else {
            return OperatorFlowError::Stage(stage);
        };
        let process_failed = self
            .process
            .force_terminate(session.lease.session_id())
            .is_err();
        let _ = session.local_record.observe_crash(now);
        self.finish_failed_session(stage, session, process_failed)
    }

    fn cleanup_after_local_use(
        &mut self,
        lease: &ProfileLease,
        workspace_lock: BridgeWorkspaceLock,
        process_failed: bool,
    ) -> CleanupFailures {
        CleanupFailures {
            process: process_failed,
            workspace_lock: workspace_lock.release().is_err(),
            coordinator_lease: self.coordinator.close_lease(lease).is_err(),
        }
    }

    fn cleanup_active_session(
        &mut self,
        session: &mut ActiveOperatorSession,
        process_failed: bool,
    ) -> CleanupFailures {
        let workspace_lock = session
            .workspace_lock
            .take()
            .is_some_and(|lock| lock.release().is_err());
        CleanupFailures {
            process: process_failed,
            workspace_lock,
            coordinator_lease: self.coordinator.close_lease(&session.lease).is_err(),
        }
    }

    fn finish_session(
        &mut self,
        mut session: ActiveOperatorSession,
        process_failed: bool,
    ) -> Result<OperatorTerminalRecord, OperatorFlowError> {
        let cleanup = self.cleanup_active_session(&mut session, process_failed);
        self.record_terminal(&session.lease, &session.local_record, cleanup);
        if cleanup.any() {
            self.cleanup_blocked = true;
            return Err(OperatorFlowError::Terminal {
                stage: if process_failed {
                    OperatorFailureStage::RuntimeAbort
                } else {
                    OperatorFailureStage::RuntimeClose
                },
                local_state: session.local_record.state(),
                cleanup,
            });
        }
        self.last_terminal.clone().ok_or(OperatorFlowError::Stage(
            OperatorFailureStage::LocalLifecycle,
        ))
    }

    fn finish_failed_session(
        &mut self,
        stage: OperatorFailureStage,
        mut session: ActiveOperatorSession,
        process_failed: bool,
    ) -> OperatorFlowError {
        let cleanup = self.cleanup_active_session(&mut session, process_failed);
        self.record_terminal(&session.lease, &session.local_record, cleanup);
        if cleanup.any() {
            self.cleanup_blocked = true;
        }
        OperatorFlowError::Terminal {
            stage,
            local_state: session.local_record.state(),
            cleanup,
        }
    }

    fn record_terminal(
        &mut self,
        lease: &ProfileLease,
        local_record: &LocalGenerationRecord,
        cleanup_failures: CleanupFailures,
    ) {
        self.last_terminal = Some(OperatorTerminalRecord {
            session_id: lease.session_id().clone(),
            generation_id: local_record.generation_id().clone(),
            local_state: local_record.state(),
            cleanup_failures,
        });
    }
}

impl From<LocalProfileError> for OperatorFlowError {
    fn from(_: LocalProfileError) -> Self {
        Self::Stage(OperatorFailureStage::LocalWorkspace)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn save_owner_maps_control_and_local_causes_without_granting_retries() {
        use super::{
            GenerationSaveFailureReason as Reason, save_checkpoint_read_reason,
            save_checkpoint_reason, save_control_reason, save_generation_reason,
        };
        use crate::dirty_generation::DirtyGenerationError;
        use crate::shipping_control_plane::ShippingControlPlaneError as Http;
        use crate::shipping_generation_save::ShippingGenerationSaveError as Save;
        use crate::shipping_generation_successor_control::GenerationSuccessorControlError as Control;
        use crate::shipping_save_attempt::SaveAttemptEvidenceError as Evidence;
        assert_eq!(
            save_control_reason(Control::<()>::ControlPlane(Http::Transport)),
            Reason::ControlTransport
        );
        assert_eq!(
            save_control_reason(Control::<()>::ControlPlane(Http::HttpServerUncertain)),
            Reason::ControlEffectUncertain
        );
        assert_eq!(
            save_control_reason(Control::<()>::ControlPlane(Http::HttpStatus)),
            Reason::ControlRejected
        );
        assert_eq!(
            save_generation_reason(Save::<Control<()>, ()>::Prepare(
                DirtyGenerationError::SourceChanged
            )),
            Reason::NativeSourceChanged
        );
        assert_eq!(
            save_generation_reason(Save::<Control<()>, ()>::Prepare(
                DirtyGenerationError::SnapshotTooLarge
            )),
            Reason::SnapshotTooLarge
        );
        assert_eq!(
            save_generation_reason(Save::<Control<()>, ()>::Upload(())),
            Reason::UploadOutcomeUncertain
        );
        assert_eq!(
            save_checkpoint_reason(Evidence::Invalid),
            Reason::CheckpointIntegrity
        );
        assert_eq!(
            save_checkpoint_reason(Evidence::Io(std::io::ErrorKind::PermissionDenied)),
            Reason::CheckpointWrite
        );
        assert_eq!(
            save_checkpoint_read_reason(Evidence::Io(std::io::ErrorKind::PermissionDenied)),
            Reason::CheckpointRead
        );
        for reason in [
            Reason::ControlRejected,
            Reason::NativeSourceChanged,
            Reason::CheckpointIntegrity,
            Reason::SealingMaterial,
            Reason::SnapshotTooLarge,
            Reason::ControlInvalidResponse,
        ] {
            assert!(!reason.allows_bounded_cloud_reconcile());
        }
        for reason in [
            Reason::ControlTransport,
            Reason::ControlEffectUncertain,
            Reason::UploadOutcomeUncertain,
            Reason::VerificationUnproven,
        ] {
            assert!(reason.allows_bounded_cloud_reconcile());
        }
    }

    use super::{
        AuthoritativeGenerationMaterializationError, BrowserLaunchPreflightPort,
        DeviceAuthenticationPort, EnrollmentPort, OperationalRejectionReason, OperatorEnrollment,
        OperatorFailureStage, OperatorFlowError, ProfileBridgeOperator, RuntimeBundleSelectionPort,
    };
    use crate::local_profile::{
        BridgeWorkspaceLock, GenerationWorkspace, LocalGenerationState, LocalProfileError,
        MaterializationRoot,
    };
    use crate::runtime_bundle::ApprovedRuntimeBundle;
    use crate::{
        FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl, ProcessAction,
    };
    use application_ports::{ProfileCoordinatorPort, ProfileCoordinatorRuntimePort};
    use bridge_domain::{
        BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort, ClaimCode,
        ClaimUri, EnrollmentClaim,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, GenerationId, LaunchIntentId,
        ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
    };
    use runtime_bundle_domain::{
        BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
        Sha256Digest,
    };
    use session_domain::ProfileLease;
    use sha2::{Digest, Sha256};
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Debug)]
    struct FakeDeviceAuthentication {
        allow: bool,
    }

    impl DeviceAuthenticationPort for FakeDeviceAuthentication {
        type Error = BridgePortError;

        fn authenticate(
            &mut self,
            _device_id: &DeviceId,
            _key_handle: &str,
        ) -> Result<(), Self::Error> {
            if self.allow {
                Ok(())
            } else {
                Err(BridgePortError::Unavailable)
            }
        }
    }

    #[derive(Clone, Debug)]
    struct FakeEnrollment {
        claim: EnrollmentClaim,
        result: OperatorEnrollment,
    }

    impl EnrollmentPort for FakeEnrollment {
        type Error = BridgePortError;

        fn redeem_claim(
            &mut self,
            claim: &ClaimUri,
            device_id: &DeviceId,
            now: UnixMillis,
        ) -> Result<OperatorEnrollment, Self::Error> {
            self.claim
                .redeem(claim.claim_code(), device_id, now)
                .map_err(|_| BridgePortError::InvalidResponse)?;
            Ok(self.result.clone())
        }
    }

    #[derive(Clone, Debug)]
    struct FakeCoordinator {
        lease: ProfileLease,
        expected_launch_intent_id: LaunchIntentId,
        claim_fail: bool,
        heartbeat_fail: bool,
        heartbeat_transport_uncertain: bool,
        close_fail: bool,
        claimed: u64,
        heartbeats: u64,
        closed: u64,
        clean_closed: u64,
    }

    impl ProfileCoordinatorPort for FakeCoordinator {
        type Error = BridgePortError;

        fn claim_launch_intent(
            &mut self,
            _actor: &ActorContext,
            _profile_id: &ProfileId,
            _device_id: &DeviceId,
            launch_intent_id: &LaunchIntentId,
        ) -> Result<ProfileLease, Self::Error> {
            self.claimed += 1;
            if self.claim_fail || launch_intent_id != &self.expected_launch_intent_id {
                Err(BridgePortError::Unavailable)
            } else {
                Ok(self.lease.clone())
            }
        }

        fn close_lease(&mut self, _lease: &ProfileLease) -> Result<(), Self::Error> {
            self.closed += 1;
            if self.close_fail {
                Err(BridgePortError::Unavailable)
            } else {
                Ok(())
            }
        }

        fn close_without_browser_use(&mut self, _lease: &ProfileLease) -> Result<(), Self::Error> {
            self.closed += 1;
            self.clean_closed += 1;
            if self.close_fail {
                Err(BridgePortError::Unavailable)
            } else {
                Ok(())
            }
        }
    }

    impl ProfileCoordinatorRuntimePort for FakeCoordinator {
        fn heartbeat_lease(&mut self, _lease: &ProfileLease) -> Result<(), Self::Error> {
            self.heartbeats += 1;
            if self.heartbeat_fail {
                Err(BridgePortError::Unavailable)
            } else {
                Ok(())
            }
        }

        fn heartbeat_transport_uncertain(&self, _error: &Self::Error) -> bool {
            self.heartbeat_transport_uncertain
        }
    }

    #[derive(Clone, Debug)]
    struct FakeRuntimeBundles {
        bundle: ApprovedRuntimeBundle,
        allow: bool,
    }

    impl RuntimeBundleSelectionPort for FakeRuntimeBundles {
        type Error = BridgePortError;

        fn select_bundle(&mut self) -> Result<ApprovedRuntimeBundle, Self::Error> {
            if self.allow {
                Ok(self.bundle.clone())
            } else {
                Err(BridgePortError::Unavailable)
            }
        }
    }

    #[derive(Clone, Debug)]
    struct FakeBrowserPreflight {
        allow: bool,
        calls: u64,
    }

    impl BrowserLaunchPreflightPort for FakeBrowserPreflight {
        type Error = BridgePortError;

        fn evaluate_before_launch(
            &mut self,
            _workspace: &GenerationWorkspace,
            _device_id: &DeviceId,
            _workspace_epoch: u64,
            _runtime_bundle: &ApprovedRuntimeBundle,
        ) -> Result<(), Self::Error> {
            self.calls += 1;
            if self.allow {
                Ok(())
            } else {
                Err(BridgePortError::Unavailable)
            }
        }
    }

    type TestOperator<H> = ProfileBridgeOperator<
        FakeDeviceIdentity,
        FakeDeviceKeyStore,
        FakeDeviceAuthentication,
        FakeEnrollment,
        FakeCoordinator,
        FakeRuntimeBundles,
        FakeBrowserPreflight,
        FakeProcessControl,
        H,
    >;

    #[derive(Default)]
    struct CloseRejectingCamouhost {
        negotiated: bool,
        active: Option<SessionId>,
    }

    impl CamouhostPort for CloseRejectingCamouhost {
        fn exchange(
            &mut self,
            message: &CamouhostMessage,
        ) -> Result<CamouhostMessage, BridgePortError> {
            match message {
                CamouhostMessage::Hello { version } if *version == CAMOUHOST_IPC_VERSION => {
                    self.negotiated = true;
                    Ok(CamouhostMessage::HelloAck { version: *version })
                }
                CamouhostMessage::Launch { session_id }
                    if self.negotiated && self.active.is_none() =>
                {
                    self.active = Some(session_id.clone());
                    Ok(CamouhostMessage::Ready {
                        session_id: session_id.clone(),
                    })
                }
                CamouhostMessage::Close { .. } => Err(BridgePortError::InvalidResponse),
                _ => Err(BridgePortError::InvalidResponse),
            }
        }
    }

    fn digest(character: char) -> Result<Sha256Digest, Box<dyn std::error::Error>> {
        Ok(Sha256Digest::parse(character.to_string().repeat(64))?)
    }

    fn approved_bundle() -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
        let calculated = digest('a')?;
        let entrypoint = BundleRelativePath::parse("camouhost/main.py")?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(entrypoint, 10, digest('b')?)])?;
        Ok(ApprovedRuntimeBundle::validate(
            manifest,
            inventory,
            &calculated,
        )?)
    }

    struct Fixture {
        root_path: PathBuf,
        root: MaterializationRoot,
        claim_uri: ClaimUri,
        actor: ActorContext,
        profile_id: ProfileId,
        generation_id: GenerationId,
        device_id: DeviceId,
        launch_intent_id: LaunchIntentId,
        lease: ProfileLease,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-operator-{}-{counter}",
                std::process::id()
            ));
            if root_path.exists() {
                fs::remove_dir_all(&root_path)?;
            }
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant_id = TenantId::parse(format!("tenant_01JOPERATOR{counter}"))?;
            let profile_id = ProfileId::parse(format!("profile_01JOPERATOR{counter}"))?;
            let generation_id = GenerationId::parse(format!("generation_01JOPERATOR{counter}"))?;
            let device_id = DeviceId::parse(format!("device_01JOPERATOR{counter}"))?;
            root.create_generation(&tenant_id, &profile_id, &generation_id)?;
            let actor = ActorContext::new(
                TenantScope::new(tenant_id.clone()),
                ActorId::parse(format!("actor_01JOPERATOR{counter}"))?,
                CorrelationId::parse(format!("corr_01JOPERATOR{counter}"))?,
            );
            let session_id = SessionId::parse(format!("session_01JOPERATOR{counter}"))?;
            let lease = ProfileLease::issue(
                tenant_id,
                profile_id.clone(),
                session_id,
                device_id.clone(),
                counter.max(1),
                FencingToken::parse(format!("fence_01JOPERATOR{counter}"))?,
            )?;
            let launch_intent_id = LaunchIntentId::parse(format!("launch_01JOPERATOR{counter}"))?;
            let claim_code = ClaimCode::parse(format!("claim_01JOPERATOR{counter:024}"))?;
            let claim_uri = ClaimUri::parse(&format!(
                "profilebridge://claim/claim_01JOPERATOR{counter:024}"
            ))?;
            let _claim = claim_code;
            Ok(Self {
                root_path,
                root,
                claim_uri,
                actor,
                profile_id,
                generation_id,
                device_id,
                launch_intent_id,
                lease,
            })
        }

        fn enrollment(&self) -> Result<FakeEnrollment, Box<dyn std::error::Error>> {
            let claim = EnrollmentClaim::issue(
                self.claim_uri.claim_code().clone(),
                UnixMillis::new(1),
                UnixMillis::new(1_000),
            )?;
            Ok(FakeEnrollment {
                claim,
                result: OperatorEnrollment::new(
                    self.actor.clone(),
                    self.profile_id.clone(),
                    self.generation_id.clone(),
                    self.launch_intent_id.clone(),
                ),
            })
        }

        fn coordinator(&self) -> FakeCoordinator {
            FakeCoordinator {
                lease: self.lease.clone(),
                expected_launch_intent_id: self.launch_intent_id.clone(),
                claim_fail: false,
                heartbeat_fail: false,
                heartbeat_transport_uncertain: false,
                close_fail: false,
                claimed: 0,
                heartbeats: 0,
                closed: 0,
                clean_closed: 0,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root_path);
        }
    }

    fn operator<H: CamouhostPort>(
        fixture: &Fixture,
        camouhost: H,
    ) -> Result<TestOperator<H>, Box<dyn std::error::Error>> {
        Ok(ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: true },
            fixture.enrollment()?,
            fixture.coordinator(),
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: true,
                calls: 0,
            },
            FakeProcessControl::default(),
            camouhost,
        ))
    }

    #[test]
    fn missing_cache_control_read_refusal_cleanly_releases_without_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::generation_reopen::GenerationReopenControlError;
        let mut fixture = Fixture::new()?;
        fixture.generation_id = GenerationId::parse("generation_audit_absent")?;
        assert!(matches!(
            fixture.root.open_generation(
                fixture.actor.tenant_scope().tenant_id(),
                &fixture.profile_id,
                &fixture.generation_id,
            ),
            Err(LocalProfileError::Io(std::io::ErrorKind::NotFound))
        ));
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        let mut runtime_health_started = false;
        let result = operator.open_with_materialization(
            &fixture.claim_uri,
            &fixture.root,
            UnixMillis::new(10),
            |_coordinator, _tenant, _profile, _generation, _device, _epoch| {
                Err(
                    AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                        GenerationReopenControlError::Transport,
                    ),
                )
            },
            || {
                runtime_health_started = true;
                Ok(())
            },
            |_coordinator, _tenant, _profile, _selected, _lease| false,
        );
        assert!(matches!(
            result,
            Err(OperatorFlowError::AuthoritativeGeneration { .. })
        ));
        assert!(!runtime_health_started);
        assert!(operator.process().actions().is_empty());
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(operator.coordinator().clean_closed, 1);
        assert!(!operator.cleanup_blocked());
        Ok(())
    }

    #[test]
    fn object_get_refusal_cleanly_releases_only_if_exact_workspace_remains_absent()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::authoritative_generation::AuthoritativeGenerationPreWriteError;
        use crate::generation_reopen::GenerationObjectDownloadError;
        for create_during_attempt in [false, true] {
            let mut fixture = Fixture::new()?;
            fixture.generation_id = GenerationId::parse("generation_get_read_no_local_writer")?;
            let root = &fixture.root;
            let mut operator = operator(&fixture, FakeCamouhost::default())?;
            let mut runtime_health_started = false;
            let result = operator.open_with_materialization(
                &fixture.claim_uri,
                root,
                UnixMillis::new(10),
                |_coordinator, tenant, profile, generation, _device, _epoch| {
                    if create_during_attempt {
                        assert!(
                            root.create_generation(tenant, profile, generation).is_ok(),
                            "simulated concurrent writer changes workspace"
                        );
                    }
                    Err(
                        AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                            AuthoritativeGenerationPreWriteError::Download(
                                GenerationObjectDownloadError::Transport,
                            ),
                        ),
                    )
                },
                || {
                    runtime_health_started = true;
                    Ok(())
                },
                |_coordinator, _tenant, _profile, _selected, _lease| false,
            );
            assert!(matches!(
                result,
                Err(OperatorFlowError::AuthoritativeGeneration { .. })
            ));
            assert!(!runtime_health_started);
            assert!(operator.process().actions().is_empty());
            assert_eq!(operator.coordinator().closed, 1);
            assert_eq!(
                operator.coordinator().clean_closed,
                u64::from(!create_during_attempt)
            );
        }
        Ok(())
    }

    #[test]
    fn ambiguous_or_new_local_workspace_refuses_clean_materialization_cancel()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::generation_reopen::GenerationReopenControlError;
        for create_during_attempt in [false, true] {
            let mut fixture = Fixture::new()?;
            if create_during_attempt {
                fixture.generation_id = GenerationId::parse("generation_raced_workspace")?;
            }
            let root = &fixture.root;
            let mut operator = operator(&fixture, FakeCamouhost::default())?;
            let result = operator.open_with_materialization(
                &fixture.claim_uri,
                root,
                UnixMillis::new(10),
                |_coordinator, tenant, profile, generation, _device, _epoch| {
                    if create_during_attempt {
                        assert!(root.create_generation(tenant, profile, generation).is_ok());
                    }
                    Err(
                        AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                            GenerationReopenControlError::HttpStatus,
                        ),
                    )
                },
                || Ok(()),
                |_coordinator, _tenant, _profile, _selected, _lease| false,
            );
            assert!(matches!(
                result,
                Err(OperatorFlowError::AuthoritativeGeneration { .. })
            ));
            assert_eq!(operator.coordinator().closed, 1);
            assert_eq!(operator.coordinator().clean_closed, 0);
            assert!(operator.process().actions().is_empty());
        }
        Ok(())
    }

    #[test]
    fn unproven_missing_cache_refusal_stays_uncertain() -> Result<(), Box<dyn std::error::Error>> {
        use crate::generation_reopen::GenerationReopenControlError;
        let mut fixture = Fixture::new()?;
        fixture.generation_id = GenerationId::parse("generation_unknown_absent")?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        let result = operator.open_with_materialization(
            &fixture.claim_uri,
            &fixture.root,
            UnixMillis::new(10),
            |_coordinator, _tenant, _profile, _generation, _device, _epoch| {
                Err(AuthoritativeGenerationMaterializationError::ControlPlane(
                    GenerationReopenControlError::InvalidResponse,
                ))
            },
            || Ok(()),
            |_coordinator, _tenant, _profile, _selected, _lease| false,
        );
        assert!(matches!(
            result,
            Err(OperatorFlowError::AuthoritativeGeneration { .. })
        ));
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(operator.coordinator().clean_closed, 0);
        Ok(())
    }

    #[test]
    fn shipping_controlled_close_preserves_wal_with_durable_offline_intent()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::shipping_save_attempt::has_unresolved_save_attempt_for_profile;
        let fixture = Fixture::new()?;
        let mut owner = operator(&fixture, FakeCamouhost::default())?;
        owner.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        let workspace = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let wal = workspace.path().join("user_data/cookies.sqlite-wal");
        fs::create_dir_all(wal.parent().ok_or("missing WAL parent")?)?;
        fs::write(&wal, b"local-firefox-unsynced-wal")?;
        owner.close_with_durable_local_intent(&fixture.root, UnixMillis::new(20))?;
        assert!(owner.has_pending_dirty_close());
        assert_eq!(fs::read(&wal)?, b"local-firefox-unsynced-wal");
        assert!(has_unresolved_save_attempt_for_profile(
            &fixture.root,
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
        )?);
        // The old process has vanished; the checkpoint prefix remains at the
        // canonical root, so a new process cannot silently use an old N
        // and discard the just-closed unsynced WAL.
        drop(owner);
        let mut restarted = operator(&fixture, FakeCamouhost::default())?;
        assert_eq!(
            restarted.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(21)),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::GenerationSave
            )),
        );
        assert_eq!(restarted.coordinator().claimed, 1);
        assert_eq!(restarted.coordinator().closed, 1);
        assert!(restarted.process().actions().is_empty());
        assert_eq!(fs::read(&wal)?, b"local-firefox-unsynced-wal");
        Ok(())
    }

    #[test]
    fn retained_native_close_reproves_same_live_writer_without_browser_restart()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut owner = operator(&fixture, FakeCamouhost::default())?;
        owner.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        owner.close_with_durable_local_intent(&fixture.root, UnixMillis::new(20))?;
        assert_eq!(owner.retained_save_lease(), Some(&fixture.lease));
        let original = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let before = original.inventory()?;
        owner.reprove_retained_save_writer()?;
        assert_eq!(owner.coordinator().heartbeats, 1);
        assert_eq!(owner.coordinator().claimed, 1);
        assert_eq!(owner.coordinator().closed, 0);
        assert!(owner.has_pending_dirty_close());
        assert_eq!(original.inventory()?, before);
        owner.coordinator.heartbeat_fail = true;
        owner.coordinator.heartbeat_transport_uncertain = true;
        assert_eq!(
            owner.reprove_retained_save_writer(),
            Err(OperatorFlowError::HeartbeatTransportUncertain),
        );
        assert!(owner.has_pending_dirty_close());
        assert_eq!(original.inventory()?, before);
        owner.coordinator.heartbeat_transport_uncertain = false;
        assert_eq!(
            owner.reprove_retained_save_writer(),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::CoordinatorHeartbeat
            )),
        );
        assert_eq!(owner.coordinator().claimed, 1);
        assert_eq!(original.inventory()?, before);
        Ok(())
    }

    #[test]
    fn composed_operator_close_retains_dirty_ownership_until_commit()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        assert_eq!(
            operator.active_session_id(),
            Some(fixture.lease.session_id())
        );
        assert_eq!(
            operator.active_local_state(),
            Some(LocalGenerationState::InUse)
        );
        assert_eq!(operator.coordinator().claimed, 1);
        assert_eq!(operator.coordinator().closed, 0);
        assert_eq!(
            operator.process().actions(),
            [ProcessAction::Spawn(fixture.lease.session_id().clone())]
        );

        operator.close(UnixMillis::new(20))?;
        assert_eq!(
            operator.pending_dirty_local_state(),
            Some(LocalGenerationState::DirtyLocal)
        );
        assert!(operator.has_pending_dirty_close());
        assert_eq!(operator.coordinator().closed, 0);
        assert_eq!(
            operator.process().actions(),
            [
                ProcessAction::Spawn(fixture.lease.session_id().clone()),
                ProcessAction::GracefulClose(fixture.lease.session_id().clone()),
                ProcessAction::ConfirmStopped(fixture.lease.session_id().clone()),
            ]
        );
        let workspace = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        assert!(matches!(
            BridgeWorkspaceLock::acquire(&workspace, &fixture.device_id, fixture.lease.epoch()),
            Err(LocalProfileError::LockBusy)
        ));
        Ok(())
    }

    #[test]
    fn healthy_heartbeat_preserves_active_runtime_ownership()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        operator.heartbeat(UnixMillis::new(11))?;
        assert_eq!(operator.coordinator().heartbeats, 1);
        assert_eq!(
            operator.active_session_id(),
            Some(fixture.lease.session_id())
        );
        assert_eq!(
            operator.process().actions(),
            [ProcessAction::Spawn(fixture.lease.session_id().clone())]
        );
        Ok(())
    }

    #[test]
    fn dead_runtime_is_fenced_before_coordinator_heartbeat()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        operator
            .process_mut()
            .simulate_exit(fixture.lease.session_id())?;
        assert_eq!(
            operator.heartbeat(UnixMillis::new(11)),
            Err(OperatorFlowError::Terminal {
                stage: OperatorFailureStage::RuntimeAbort,
                local_state: LocalGenerationState::RecoveryRequired,
                cleanup: super::CleanupFailures::none(),
            })
        );
        assert_eq!(operator.coordinator().heartbeats, 0);
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(operator.active_session_id(), None);
        assert_eq!(
            operator.last_terminal().map(|value| value.local_state()),
            Some(LocalGenerationState::RecoveryRequired)
        );
        assert_eq!(
            operator.process().actions(),
            [
                ProcessAction::Spawn(fixture.lease.session_id().clone()),
                ProcessAction::ForceTerminate(fixture.lease.session_id().clone()),
            ]
        );
        Ok(())
    }

    #[test]
    fn transient_heartbeat_transport_failure_preserves_native_working_browser_and_allows_reconfirmation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut coordinator = fixture.coordinator();
        coordinator.heartbeat_fail = true;
        coordinator.heartbeat_transport_uncertain = true;
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: true },
            fixture.enrollment()?,
            coordinator,
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: true,
                calls: 0,
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        let working = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let wal = working.path().join("user_data").join("cookies.sqlite-wal");
        fs::create_dir_all(wal.parent().ok_or("no WAL parent")?)?;
        fs::write(&wal, b"native-unsynced-wal-original")?;

        assert_eq!(
            operator.heartbeat(UnixMillis::new(11)),
            Err(OperatorFlowError::HeartbeatTransportUncertain)
        );
        assert_eq!(
            operator.active_session_id(),
            Some(fixture.lease.session_id())
        );
        assert_eq!(
            operator.active_local_state(),
            Some(LocalGenerationState::InUse)
        );
        assert_eq!(operator.coordinator().closed, 0);
        assert!(operator.last_terminal().is_none());
        assert_eq!(
            operator.process().actions(),
            [ProcessAction::Spawn(fixture.lease.session_id().clone())]
        );
        assert_eq!(fs::read(&wal)?, b"native-unsynced-wal-original");

        operator.coordinator.heartbeat_fail = false;
        operator.heartbeat(UnixMillis::new(12))?;
        assert_eq!(operator.coordinator().heartbeats, 2);
        assert_eq!(operator.coordinator().closed, 0);
        assert_eq!(fs::read(&wal)?, b"native-unsynced-wal-original");
        operator.close(UnixMillis::new(13))?;
        assert!(operator.has_pending_dirty_close());
        assert_eq!(fs::read(&wal)?, b"native-unsynced-wal-original");
        Ok(())
    }

    #[test]
    fn lost_heartbeat_stops_runtime_and_enters_recovery() -> Result<(), Box<dyn std::error::Error>>
    {
        let fixture = Fixture::new()?;
        let mut coordinator = fixture.coordinator();
        coordinator.heartbeat_fail = true;
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: true },
            fixture.enrollment()?,
            coordinator,
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: true,
                calls: 0,
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        assert_eq!(
            operator.heartbeat(UnixMillis::new(11)),
            Err(OperatorFlowError::Terminal {
                stage: OperatorFailureStage::CoordinatorHeartbeat,
                local_state: LocalGenerationState::RecoveryRequired,
                cleanup: super::CleanupFailures::none(),
            })
        );
        assert_eq!(operator.coordinator().heartbeats, 1);
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(operator.active_session_id(), None);
        assert_eq!(
            operator.last_terminal().map(|value| value.local_state()),
            Some(LocalGenerationState::RecoveryRequired)
        );
        assert_eq!(
            operator.process().actions(),
            [
                ProcessAction::Spawn(fixture.lease.session_id().clone()),
                ProcessAction::ForceTerminate(fixture.lease.session_id().clone()),
            ]
        );
        Ok(())
    }

    #[test]
    fn pending_attempt_observation_requires_fresh_lease_and_never_starts_browser()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let tenant = fixture.actor.tenant_scope().tenant_id().as_str();
        let profile = fixture.profile_id.as_str();
        let mut hash = Sha256::new();
        hash.update(b"bridge-p3-save-attempt-profile-scope-v1\0");
        for part in [tenant, profile] {
            hash.update((part.len() as u64).to_be_bytes());
            hash.update(part.as_bytes());
        }
        // An interrupted/staged checkpoint must be noticed even if its bytes
        // are torn and unfit for any cloud operation.
        let fingerprint = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let pending = fixture.root.path().join(format!(
            ".profile-save-attempt-v1-{fingerprint}-untrusted.pending"
        ));
        fs::write(&pending, b"torn-attempt")?;

        let mut owner = operator(&fixture, FakeCamouhost::default())?;
        let observed = Cell::new(0_u32);
        let materializations = Cell::new(0_u32);
        let runtime_starts = Cell::new(0_u32);
        assert_eq!(
            owner.open_with_materialization(
                &fixture.claim_uri,
                &fixture.root,
                UnixMillis::new(10),
                |_coordinator, _tenant, _profile, _generation, _device, _epoch| {
                    materializations.set(materializations.get() + 1);
                    Ok(())
                },
                || {
                    runtime_starts.set(runtime_starts.get() + 1);
                    Ok(())
                },
                |_live_coordinator,
                 actual_tenant,
                 actual_profile,
                 selected_generation,
                 fresh_lease| {
                    assert_eq!(actual_tenant.as_str(), tenant);
                    assert_eq!(actual_profile.as_str(), profile);
                    assert_eq!(selected_generation, &fixture.generation_id);
                    assert_eq!(fresh_lease.device_id(), &fixture.device_id);
                    observed.set(observed.get() + 1);
                    false
                },
            ),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::GenerationSave
            ))
        );
        assert_eq!(observed.get(), 1);
        assert_eq!(materializations.get(), 0);
        assert_eq!(runtime_starts.get(), 0);
        assert_eq!(owner.coordinator().claimed, 1);
        assert_eq!(owner.coordinator().closed, 1);
        assert!(owner.process().actions().is_empty());
        assert_eq!(fs::read(&pending)?, b"torn-attempt");

        // The same torn evidence never triggers observation or browser use
        // if the coordinator refused a fresh launch claim.
        let mut rejected = operator(&fixture, FakeCamouhost::default())?;
        rejected.coordinator.claim_fail = true;
        let unauthorized_observations = Cell::new(0_u32);
        let unauthorized_materializations = Cell::new(0_u32);
        let unauthorized_starts = Cell::new(0_u32);
        assert_eq!(
            rejected.open_with_materialization(
                &fixture.claim_uri,
                &fixture.root,
                UnixMillis::new(11),
                |_coordinator, _tenant, _profile, _generation, _device, _epoch| {
                    unauthorized_materializations.set(unauthorized_materializations.get() + 1);
                    Ok(())
                },
                || {
                    unauthorized_starts.set(unauthorized_starts.get() + 1);
                    Ok(())
                },
                |_coordinator, _tenant, _profile, _selected, _lease| {
                    unauthorized_observations.set(1);
                    false
                },
            ),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::CoordinatorClaim
            ))
        );
        assert_eq!(unauthorized_observations.get(), 0);
        assert_eq!(unauthorized_materializations.get(), 0);
        assert_eq!(unauthorized_starts.get(), 0);
        assert!(rejected.process().actions().is_empty());
        assert_eq!(fs::read(&pending)?, b"torn-attempt");
        Ok(())
    }

    #[test]
    fn rejected_runtime_does_not_consume_the_launch_claim() -> Result<(), Box<dyn std::error::Error>>
    {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.runtime_bundles.allow = false;
        assert_eq!(
            operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::RuntimeBundle
            ))
        );
        assert_eq!(operator.coordinator().claimed, 0);
        assert!(operator.process().actions().is_empty());
        // This must still be a first redemption, not ReplayRejected after failed local work.
        assert!(
            operator
                .enrollment
                .claim
                .redeem(
                    fixture.claim_uri.claim_code(),
                    &fixture.device_id,
                    UnixMillis::new(11),
                )
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn authentication_failure_prevents_coordinator_and_runtime_mutation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: false },
            fixture.enrollment()?,
            fixture.coordinator(),
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: true,
                calls: 0,
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        assert_eq!(
            operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::DeviceAuthentication
            ))
        );
        assert_eq!(operator.coordinator().claimed, 0);
        assert!(operator.process().actions().is_empty());
        Ok(())
    }

    #[test]
    fn pending_dirty_close_blocks_second_ownership_before_claim_replay()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        operator.close(UnixMillis::new(20))?;
        assert_eq!(
            operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(21)),
            Err(OperatorFlowError::Busy)
        );
        assert_eq!(operator.coordinator().claimed, 1);
        assert_eq!(operator.coordinator().closed, 0);
        Ok(())
    }

    #[test]
    fn mismatched_coordinator_lease_is_closed_before_local_or_runtime_use()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let wrong_lease = ProfileLease::issue(
            fixture.actor.tenant_scope().tenant_id().clone(),
            fixture.profile_id.clone(),
            SessionId::parse("session_01JOPERATOR_WRONG")?,
            DeviceId::parse("device_01JOPERATOR_WRONG")?,
            99,
            FencingToken::parse("fence_01JOPERATOR_WRONG")?,
        )?;
        let mut coordinator = fixture.coordinator();
        coordinator.lease = wrong_lease;
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: true },
            fixture.enrollment()?,
            coordinator,
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: true,
                calls: 0,
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        assert_eq!(
            operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::LeaseValidation
            ))
        );
        assert_eq!(operator.coordinator().closed, 1);
        assert!(operator.process().actions().is_empty());
        Ok(())
    }

    #[test]
    fn local_lock_contention_prevents_runtime_spawn_and_closes_lease()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let workspace = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let busy_lock =
            BridgeWorkspaceLock::acquire(&workspace, &fixture.device_id, fixture.lease.epoch())?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        assert_eq!(
            operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::LocalWorkspace
            ))
        );
        assert_eq!(operator.coordinator().closed, 1);
        assert!(operator.process().actions().is_empty());
        busy_lock.release()?;
        Ok(())
    }

    #[test]
    fn preflight_refusal_does_not_consume_pending_runtime_health()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: true },
            fixture.enrollment()?,
            fixture.coordinator(),
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: false,
                calls: 0,
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        let mut started_health = false;
        let result = operator.open_with_materialization(
            &fixture.claim_uri,
            &fixture.root,
            UnixMillis::new(10),
            |_coordinator, _tenant, _profile, _generation, _device, _epoch| Ok(()),
            || {
                started_health = true;
                Ok(())
            },
            |_coordinator, _tenant, _profile, _selected, _lease| false,
        );
        assert!(matches!(result, Err(OperatorFlowError::Preflight { .. })));
        assert!(!started_health);
        assert!(operator.process().actions().is_empty());
        Ok(())
    }

    #[test]
    fn runtime_health_witness_runs_before_the_first_managed_spawn()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        let mut started_health = false;
        operator.open_with_materialization(
            &fixture.claim_uri,
            &fixture.root,
            UnixMillis::new(10),
            |_coordinator, _tenant, _profile, _generation, _device, _epoch| Ok(()),
            || {
                started_health = true;
                Ok(())
            },
            |_coordinator, _tenant, _profile, _selected, _lease| false,
        )?;
        assert!(started_health);
        assert_eq!(
            operator.process().actions(),
            [ProcessAction::Spawn(fixture.lease.session_id().clone())]
        );
        Ok(())
    }

    #[test]
    fn durable_health_start_failure_prevents_browser_spawn()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        let mut start_attempts = 0;
        let error = operator.open_with_materialization(
            &fixture.claim_uri,
            &fixture.root,
            UnixMillis::new(10),
            |_coordinator, _tenant, _profile, _generation, _device, _epoch| Ok(()),
            || {
                start_attempts += 1;
                Err(())
            },
            |_coordinator, _tenant, _profile, _selected, _lease| false,
        );
        assert_eq!(
            error,
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::DeliveryHealthStart
            ))
        );
        assert_eq!(start_attempts, 1);
        assert_eq!(operator.coordinator().clean_closed, 1);
        assert!(operator.process().actions().is_empty());
        Ok(())
    }

    #[test]
    fn browser_preflight_failure_prevents_runtime_spawn_and_releases_ownership()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: true },
            fixture.enrollment()?,
            fixture.coordinator(),
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: false,
                calls: 0,
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        assert_eq!(
            operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
            Err(OperatorFlowError::Preflight {
                reason: OperationalRejectionReason::BrowserPreflightRejected,
                local_state: LocalGenerationState::MaterializedClean,
                cleanup: super::CleanupFailures::none(),
            })
        );
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(operator.coordinator().clean_closed, 1);
        assert!(operator.process().actions().is_empty());
        let workspace = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let lock =
            BridgeWorkspaceLock::acquire(&workspace, &fixture.device_id, fixture.lease.epoch())?;
        lock.release()?;
        Ok(())
    }

    #[test]
    fn existing_firefox_writer_lock_blocks_clean_preflight_cancellation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let workspace = fixture.root.open_generation(
            fixture.actor.tenant_scope().tenant_id(),
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        #[cfg(windows)]
        let lock_path = {
            fs::create_dir_all(workspace.path().join("user_data"))?;
            workspace.path().join("user_data/parent.lock")
        };
        #[cfg(not(windows))]
        let lock_path = workspace.path().join(".parentlock");
        fs::write(&lock_path, b"foreign-writer")?;
        // On Windows hold the OS handle, not merely a stale name.
        #[cfg(windows)]
        let _live_parent_lock = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&lock_path)?
        };
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(fixture.device_id.clone()),
            FakeDeviceKeyStore::default(),
            FakeDeviceAuthentication { allow: true },
            fixture.enrollment()?,
            fixture.coordinator(),
            FakeRuntimeBundles {
                bundle: approved_bundle()?,
                allow: true,
            },
            FakeBrowserPreflight {
                allow: false,
                calls: 0,
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        assert!(matches!(
            operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10)),
            Err(OperatorFlowError::Preflight { .. })
        ));
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(operator.coordinator().clean_closed, 0);
        assert!(operator.process().actions().is_empty());
        #[cfg(windows)]
        drop(_live_parent_lock);
        assert_eq!(fs::read(&lock_path)?, b"foreign-writer");
        Ok(())
    }

    #[test]
    fn runtime_negotiation_failure_becomes_recovery_required()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, CloseRejectingCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        let error = operator.close(UnixMillis::new(20));
        assert!(matches!(
            error,
            Err(OperatorFlowError::Runtime {
                stage: OperatorFailureStage::RuntimeClose,
                ..
            })
        ));
        assert_eq!(
            operator.last_terminal().map(|value| value.local_state()),
            Some(LocalGenerationState::RecoveryRequired)
        );
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(
            operator.process().actions(),
            [
                ProcessAction::Spawn(fixture.lease.session_id().clone()),
                ProcessAction::GracefulClose(fixture.lease.session_id().clone()),
                ProcessAction::ForceTerminate(fixture.lease.session_id().clone()),
            ]
        );
        Ok(())
    }

    #[test]
    fn explicit_abort_marks_recovery_and_releases_ownership()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        let terminal = operator.abort(UnixMillis::new(11))?;
        assert_eq!(
            terminal.local_state(),
            LocalGenerationState::RecoveryRequired
        );
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(
            operator.process().actions(),
            [
                ProcessAction::Spawn(fixture.lease.session_id().clone()),
                ProcessAction::ForceTerminate(fixture.lease.session_id().clone()),
            ]
        );
        Ok(())
    }
}
