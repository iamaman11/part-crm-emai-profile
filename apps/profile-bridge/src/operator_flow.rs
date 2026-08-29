use crate::ProcessControlPort;
use crate::authoritative_generation::ensure_authoritative_generation;
#[cfg(any(test, feature = "synthetic-test-bin"))]
use crate::browser_mail_query::BrowserMailExecutionProof;
use crate::dirty_close::{
    DirtyCloseCompletion, DirtyCloseLocalOutcome, RetainedDirtyClose, RetainedDirtyCloseError,
};
use crate::dirty_generation::{GenerationSealingMaterialPort, PreparedDirtyGeneration};
#[cfg(any(test, feature = "synthetic-test-bin"))]
use crate::dirty_generation_finalize::DirtyGenerationCommitClientPort;
use crate::generation_reopen::{GenerationObjectDownloadPort, GenerationReopenControlPort};
use crate::local_profile::{
    BridgeWorkspaceLock, GenerationWorkspace, LocalGenerationRecord, LocalGenerationState,
    LocalProfileError, MaterializationRoot,
};
use crate::runtime_bundle::{
    ApprovedRuntimeBundle, RuntimeLaunchError, RuntimeSessionOrchestrator,
};
use crate::shipping_control_plane::MachineHttpPort;
use crate::shipping_generation_save::{
    CommittedGenerationSuccessor, SignedGenerationObjectPutPort,
    prepare_retained_generation_successor, publish_verify_and_commit_successor,
};
use crate::shipping_generation_successor_control::ControlPlaneGenerationSuccessor;
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

    fn select_bundle(
        &mut self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<ApprovedRuntimeBundle, Self::Error>;
}

pub trait BrowserLaunchPreflightPort {
    type Error;

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
        self.local.is_fully_saved_locally()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorFlowError {
    Busy,
    CleanupRequired,
    Stage(OperatorFailureStage),
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

impl fmt::Display for OperatorFlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => formatter.write_str("an operator session is already active"),
            Self::CleanupRequired => formatter.write_str("previous operator cleanup is unresolved"),
            Self::Stage(stage) => write!(formatter, "operator flow failed at {stage:?}"),
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
        Dl: GenerationObjectDownloadPort,
    {
        self.open_with_materialization(
            claim,
            root,
            now,
            |coordinator, tenant_id, profile_id, generation_id| {
                ensure_authoritative_generation(
                    root,
                    tenant_id,
                    profile_id,
                    generation_id,
                    coordinator,
                    downloader,
                )
                .map_err(|_| ())
            },
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
            |_coordinator, _tenant_id, _profile_id, _generation_id| Ok(()),
        )
    }

    fn open_with_materialization<F>(
        &mut self,
        claim: &ClaimUri,
        root: &MaterializationRoot,
        now: UnixMillis,
        mut ensure_materialized: F,
    ) -> Result<(), OperatorFlowError>
    where
        F: FnMut(
            &mut C,
            &profile_platform_primitives::TenantId,
            &ProfileId,
            &GenerationId,
        ) -> Result<(), ()>,
    {
        if self.cleanup_blocked {
            return Err(OperatorFlowError::CleanupRequired);
        }
        if self.active.is_some() || self.retained_dirty.is_some() {
            return Err(OperatorFlowError::Busy);
        }
        self.last_terminal = None;

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
        let runtime_bundle = self
            .runtime_bundles
            .select_bundle(
                enrollment.actor(),
                enrollment.profile_id(),
                enrollment.generation_id(),
            )
            .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::RuntimeBundle))?;
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

        if ensure_materialized(
            &mut self.coordinator,
            enrollment.actor().tenant_scope().tenant_id(),
            enrollment.profile_id(),
            enrollment.generation_id(),
        )
        .is_err()
        {
            return Err(
                self.fail_before_local_use(OperatorFailureStage::AuthoritativeGeneration, lease)
            );
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
        let workspace_lock =
            match BridgeWorkspaceLock::acquire(&workspace, &device_id, lease.epoch()) {
                Ok(value) => value,
                Err(_) => {
                    return Err(
                        self.fail_before_local_use(OperatorFailureStage::LocalWorkspace, lease)
                    );
                }
            };
        if self
            .browser_preflight
            .evaluate_before_launch(&workspace, &device_id, lease.epoch(), &runtime_bundle)
            .is_err()
        {
            return Err(self.fail_with_lock_before_use(
                OperatorFailureStage::BrowserPreflight,
                lease,
                workspace_lock,
            ));
        }
        let inventory = match workspace.inventory() {
            Ok(value) => value,
            Err(_) => {
                return Err(self.fail_with_lock_before_use(
                    OperatorFailureStage::LocalWorkspace,
                    lease,
                    workspace_lock,
                ));
            }
        };
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
        if self.coordinator.heartbeat_lease(&lease).is_ok() {
            return Ok(());
        }
        Err(self.fail_active_runtime(OperatorFailureStage::CoordinatorHeartbeat, now))
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
            let retained = self.retained_dirty.as_ref().ok_or(OperatorFlowError::Stage(
                OperatorFailureStage::GenerationSave,
            ))?;
            let workspace = retained
                .open_base_workspace(root)
                .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::GenerationSave))?;
            (
                retained.lease().clone(),
                retained.base_record().clone(),
                workspace,
            )
        };

        let mut control = ControlPlaneGenerationSuccessor::new(transport, &mut self.coordinator);
        let prepared = prepare_retained_generation_successor(
            &base_record,
            &workspace,
            root,
            lease.tenant_id(),
            lease.profile_id(),
            &lease,
            &mut control,
        )
        .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::GenerationSave))?;
        let committed = publish_verify_and_commit_successor(
            lease.tenant_id(),
            lease.profile_id(),
            base_record.generation_id(),
            &prepared,
            &lease,
            &mut control,
            upload,
        )
        .map_err(|_| OperatorFlowError::Stage(OperatorFailureStage::GenerationSave))?;
        drop(control);

        let (completion, session_id, local_state) = {
            let retained = self
                .retained_dirty
                .as_mut()
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
            let local_state = retained.base_record().state();
            (completion, session_id, local_state)
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
        let rematerialization_blocked = matches!(
            completion.local_outcome(),
            DirtyCloseLocalOutcome::RematerializationBlocked { .. }
        );
        self.retained_dirty = None;
        self.last_terminal = Some(OperatorTerminalRecord {
            session_id,
            generation_id: committed.generation_id().clone(),
            local_state,
            cleanup_failures: cleanup,
        });
        if cleanup.any() || rematerialization_blocked {
            self.cleanup_blocked = true;
        }

        Ok(OperatorGenerationSaveCompletion {
            committed,
            local: completion,
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

    fn fail_with_lock_before_use(
        &mut self,
        stage: OperatorFailureStage,
        lease: ProfileLease,
        workspace_lock: BridgeWorkspaceLock,
    ) -> OperatorFlowError {
        let workspace_lock_failed = workspace_lock.release().is_err();
        let coordinator_lease = self.coordinator.close_lease(&lease).is_err();
        let cleanup = CleanupFailures {
            process: false,
            workspace_lock: workspace_lock_failed,
            coordinator_lease,
        };
        if cleanup.any() {
            self.cleanup_blocked = true;
            OperatorFlowError::Terminal {
                stage,
                local_state: LocalGenerationState::MaterializedClean,
                cleanup,
            }
        } else {
            OperatorFlowError::Stage(stage)
        }
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
    use super::{
        BrowserLaunchPreflightPort, DeviceAuthenticationPort, EnrollmentPort, OperatorEnrollment,
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
        close_fail: bool,
        claimed: u64,
        heartbeats: u64,
        closed: u64,
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
    }

    #[derive(Clone, Debug)]
    struct FakeRuntimeBundles {
        bundle: ApprovedRuntimeBundle,
        allow: bool,
    }

    impl RuntimeBundleSelectionPort for FakeRuntimeBundles {
        type Error = BridgePortError;

        fn select_bundle(
            &mut self,
            _actor: &ActorContext,
            _profile_id: &ProfileId,
            _generation_id: &GenerationId,
        ) -> Result<ApprovedRuntimeBundle, Self::Error> {
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
                close_fail: false,
                claimed: 0,
                heartbeats: 0,
                closed: 0,
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
            Err(OperatorFlowError::Stage(
                OperatorFailureStage::BrowserPreflight
            ))
        );
        assert_eq!(operator.coordinator().closed, 1);
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
