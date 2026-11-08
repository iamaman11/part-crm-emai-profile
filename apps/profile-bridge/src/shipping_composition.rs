#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;

/// Internal one-shot maintenance command, never a normal Profile Open.
/// It requires a fresh ordinary device-bound one-time launch claim.

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShippingDeliveryCommand {
    InstallStaged(crate::installed_connection::ControlPlaneOrigin),
    ActivateStaged,
    HandoffArrived,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShippingCompositionError {
    UnsupportedPlatform,
    Configuration,
    ExternalNetworkConfiguration,
    Clock,
    ControlPlane,
    DeliveryHealth,
    DeliveryHandoff,
    FirstInstallation(crate::windows_delivery_handoff::DeliveryHandoffError),
    InstalledConnection(crate::installed_connection::InstalledConnectionError),
    DeliveryRecoveryRequired,
    /// Cloud CURRENT recovery was required but not proven. Local bytes untouched.
    RestoreUnavailable,
    #[cfg(windows)]
    DeliveryRuntime(crate::windows_delivery_runtime::WindowsDeliveryRuntimeError),
    Operator(crate::operator_flow::OperatorFlowError),
    ActiveSessionMissing,
    CloseObservation(bridge_domain::BridgePortError),
    CommittedRecoveryRequired,
    HeartbeatAuthorityUncertain,
    Initialization(crate::generation_initialization::InitialProfileError),
    #[cfg(windows)]
    LocalOpen(crate::local_browser_open::LocalOpenError),
}

impl core::fmt::Display for ShippingCompositionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "shipping Profile Bridge requires Windows",
            Self::Configuration => "shipping Profile Bridge configuration is invalid",
            Self::ExternalNetworkConfiguration => "external manual network settings require repair",
            Self::Clock => "shipping Profile Bridge clock is unavailable",
            Self::ControlPlane => "shipping Profile Bridge control-plane lease state is invalid",
            Self::DeliveryHealth => {
                "shipping Profile Bridge candidate health could not be confirmed"
            }
            Self::DeliveryHandoff => "shipping Profile Bridge delivery handoff failed closed",
            Self::FirstInstallation(_) => "Profile Bridge first installation failed closed",
            Self::InstalledConnection(_) => "Profile Bridge installed connection is invalid",
            Self::DeliveryRecoveryRequired => {
                "shipping Profile Bridge delivery recovery is required"
            }
            Self::RestoreUnavailable => {
                "Verified cloud CURRENT could not be restored; local profile was not opened or overwritten"
            }
            #[cfg(windows)]
            Self::DeliveryRuntime(error) => return core::fmt::Display::fmt(error, formatter),
            Self::Operator(_) => "shipping Profile Bridge operator flow failed",
            Self::ActiveSessionMissing => "shipping Profile Bridge active session is missing",
            Self::CloseObservation(bridge_domain::BridgePortError::UncleanBrowserExit) => {
                "Camoufox exited without a clean shutdown checkpoint; local recovery is required"
            }
            Self::CloseObservation(_) => "shipping Profile Bridge close observation failed",
            Self::Initialization(_) => "profile first-open preparation failed",
            #[cfg(windows)]
            Self::LocalOpen(error) => return core::fmt::Display::fmt(error, formatter),
            Self::HeartbeatAuthorityUncertain => {
                "browser closed safely: coordinator heartbeat was not confirmed; local data retained for recovery"
            },
            Self::CommittedRecoveryRequired => {
                "generation successor committed but local recovery is required"
            }
        })
    }
}

impl std::error::Error for ShippingCompositionError {}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloseObservationDisposition {
    ContinueRuntime,
    BeginConfirmedSave,
    RecoverUncleanExit,
}

#[cfg(any(windows, test))]
fn close_observation_disposition(
    observation: Result<bool, bridge_domain::BridgePortError>,
) -> Result<CloseObservationDisposition, bridge_domain::BridgePortError> {
    match observation {
        Ok(true) => Ok(CloseObservationDisposition::BeginConfirmedSave),
        Ok(false) => Ok(CloseObservationDisposition::ContinueRuntime),
        Err(bridge_domain::BridgePortError::UncleanBrowserExit) => {
            Ok(CloseObservationDisposition::RecoverUncleanExit)
        }
        Err(error) => Err(error),
    }
}

/// The existing pending-recovery observation is read-only. Preserve an
/// exact native-size refusal through the Open owner's normal lease cleanup;
/// all other errors remain blocked without synthetic browser authority.
#[cfg(any(windows, test))]
fn pending_recovery_failure_disposition(
    error: crate::shipping_generation_successor_control::RestartExactLocalCompletionError,
) -> crate::operator_flow::PendingSaveRecovery {
    use crate::operator_flow::PendingSaveRecovery;
    use crate::shipping_generation_successor_control::RestartExactLocalCompletionError;
    if error == RestartExactLocalCompletionError::NativeSnapshotTooLarge {
        PendingSaveRecovery::NativeSnapshotTooLarge
    } else {
        PendingSaveRecovery::Blocked
    }
}

/// One user-triggered Open may continue once after its local N->N+1
/// recovery was proved committed and its unused Coordinator lease was
/// released cleanly. The existing machine authority issues a fresh claim;
/// no stale claim replay, hidden local grant, background daemon or loop.
// A rejected or malformed *read-only* D1 retry decision can never be
// retried on the strength of the old writer fence. Only an uncertain
// transport/408/429/5xx may be re-observed while the original lease is live.
#[cfg(any(windows, test))]
fn retryable_reconnect_observation(
    error: crate::shipping_generation_successor_control::RestartExactLocalCompletionError,
) -> bool {
    use crate::shipping_control_plane::ShippingControlPlaneError as Http;
    use crate::shipping_generation_successor_control::RestartExactLocalCompletionError as Restart;
    matches!(
        error,
        Restart::ControlPlane(Http::Transport | Http::HttpServerUncertain)
    )
}

/// Diagnostic only. A failed recovery never grants permission to replay
/// a sealing request, mutate a LOCAL_UNSYNCED profile or open stale N.
/// Keep the next action specific so an oversized native snapshot or unknown
/// cloud effect is not presented as an ordinary safe retry.
#[cfg(any(windows, test))]
fn recovery_next_safe_step(
    error: crate::shipping_generation_successor_control::RestartExactLocalCompletionError,
) -> &'static str {
    use crate::shipping_control_plane::ShippingControlPlaneError as Http;
    use crate::shipping_generation_successor_control::RestartExactLocalCompletionError as Restart;
    match error {
        Restart::NativeSnapshotTooLarge => "PRESERVE_LOCAL_BYTES;NATIVE_RESCUE_REQUIRED",
        Restart::PreSealTransport
        | Restart::PreSealResponseUncertain
        | Restart::UploadOrCommitUncertain
        | Restart::ControlPlane(Http::Transport | Http::HttpServerUncertain) => {
            "PRESERVE_LOCAL_BYTES;FRESH_AUTHORIZED_OBSERVATION"
        }
        Restart::PreSealRejected
        | Restart::ControlPlane(Http::HttpStatus | Http::LaunchLeaseConflict) => {
            "PRESERVE_LOCAL_BYTES;CHECK_SERVER_AUTHORITY"
        }
        Restart::PreSealUnavailable => "PRESERVE_LOCAL_BYTES;CHECK_SEALING_SERVICE",
        Restart::LocalEvidenceChanged => "PRESERVE_LOCAL_BYTES;CHECK_LOCAL_EVIDENCE",
        Restart::WriterOrLockUnresolved => "PRESERVE_LOCAL_BYTES;CHECK_NATIVE_WRITER",
        Restart::BindingOrRetirementFailed => "PRESERVE_LOCAL_BYTES;RECONCILE_ACTIVE_EXACT",
        Restart::ControlPlane(Http::InvalidResponse) => {
            "PRESERVE_LOCAL_BYTES;INSPECT_SERVER_RESPONSE"
        }
        Restart::ControlPlane(Http::Clock) => "PRESERVE_LOCAL_BYTES;CHECK_CLOCK",
    }
}

#[cfg(any(windows, test))]
fn open_with_one_fresh_claim<Open, NewClaim>(
    original: &ClaimUri,
    mut open: Open,
    new_claim: NewClaim,
) -> Result<(), ShippingCompositionError>
where
    Open: FnMut(&ClaimUri) -> Result<(), ShippingCompositionError>,
    NewClaim: FnOnce() -> Result<ClaimUri, ShippingCompositionError>,
{
    match open(original) {
        Err(ShippingCompositionError::Operator(
            crate::operator_flow::OperatorFlowError::FreshAuthorizedOpenRequired,
        )) => {
            let fresh = new_claim()?;
            if &fresh == original {
                return Err(ShippingCompositionError::ControlPlane);
            }
            open(&fresh)
        }
        outcome => outcome,
    }
}

pub fn run_claim(claim: &ClaimUri) -> Result<(), ShippingCompositionError> {
    #[cfg(windows)]
    {
        return windows::run(claim);
    }
    #[cfg(not(windows))]
    {
        let _ = claim;
        Err(ShippingCompositionError::UnsupportedPlatform)
    }
}

pub fn run_initial_profile(
    intent: &crate::generation_initialization::InitialProfileOpen,
) -> Result<(), ShippingCompositionError> {
    #[cfg(windows)]
    {
        windows::run_initial(intent)
    }
    #[cfg(not(windows))]
    {
        let _ = intent;
        Err(ShippingCompositionError::UnsupportedPlatform)
    }
}

pub fn run_delivery_command(
    command: ShippingDeliveryCommand,
) -> Result<(), ShippingCompositionError> {
    #[cfg(windows)]
    {
        return windows::run_delivery_command(command);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
        Err(ShippingCompositionError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod windows {
    use super::{
        CloseObservationDisposition, ShippingCompositionError, ShippingDeliveryCommand,
        close_observation_disposition, pending_recovery_failure_disposition,
        recovery_next_safe_step, retryable_reconnect_observation,
    };
    use crate::authoritative_generation::ensure_authoritative_current;
    use crate::camouhost_process::{
        ManagedCamouhostConfig, ManagedCamouhostProcess, RuntimeBindingSlot, RuntimeDisplayMode,
    };
    use crate::dirty_generation::prepare_closed_current_successor;
    use crate::generation_initialization::{
        InitialProfileError, InitialProfileStatus, reauthorize_current_ready_generation,
        sync_closed_local_draft,
    };
    use crate::generation_reopen::{
        GenerationReopenControlPort, VerifiedGenerationObjectStreamDownloader,
    };
    use crate::generation_snapshot::encode_workspace_snapshot;
    use crate::local_profile::{DeliveryActivationGuard, MaterializationRoot};
    use crate::operator_flow::EnrollmentPort;
    use crate::operator_flow::{
        OperatorFailureStage, OperatorFlowError, PendingSaveRecovery, ProfileBridgeOperator,
    };
    use crate::shipping_control_plane::{
        ControlPlaneCoordinator, ControlPlaneEnrollment, ControlPlaneLeaseTiming,
        SharedControlPlaneCoordinator, ShippingControlPlaneError,
    };
    use crate::shipping_generation_save::publish_verify_and_commit_successor;
    use crate::shipping_generation_successor_control::{
        ControlPlaneGenerationSuccessor, observe_closed_current_exact,
    };
    use crate::shipping_generation_successor_control::{
        RestartExactCommitObservation, RestartExactLocalCompletion,
        RestartExactLocalCompletionError, complete_active_exact_after_restart,
        confirm_retained_exact_retry_nonapplication, resume_exact_successor_under_fresh_lease,
        resume_lone_local_unsynced_under_fresh_lease,
    };
    use crate::shipping_preflight::ShippingBrowserLaunchPreflight;
    use crate::user_diagnostics::report::{self, Cause, Cloud, Outcome, Stage};
    use crate::user_diagnostics::{PendingSaveReadOnlyDiagnosis, ReadOnlyProfileDiagnosis};
    use crate::windows_delivery_handoff::{
        DeliveryHandoffCoordinator, DeliveryHandoffRestartDisposition,
    };
    use crate::windows_delivery_runtime::ActiveWindowsDeliveryRuntime;
    use crate::windows_device_application::{
        WindowsApplicationSessionHttp, WindowsDeviceApplication, WindowsDeviceApplicationBinding,
    };
    use crate::windows_generation_get::WindowsSignedGenerationObjectGet;
    use crate::windows_generation_put::WindowsSignedGenerationObjectPut;
    use crate::windows_native::WindowsDeviceIdentity;
    use application_ports::{ProfileCoordinatorPort, ProfileCoordinatorRuntimePort};
    use bridge_domain::ClaimUri;
    use profile_platform_primitives::UnixMillis;
    use session_domain::ProfileLease;
    use std::env;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const NETWORK_POLICY_PATH_ENV: &str = "PROFILE_BRIDGE_NETWORK_POLICY_PATH";
    const PROXY_CONFIG_PATH_ENV: &str = "PROFILE_BRIDGE_PROXY_CONFIG_PATH";
    const PACKAGED_PYTHON_EXECUTABLE: &str = "python/python.exe";
    // Exactly one protected staging slot. A crashed/uncertain cold probe
    // leaves this slot occupied rather than silently deleting a potentially
    // still-running Firefox workspace or accumulating unlimited full clones.
    const CONTROLLED_CLOSE_POLL_MS: u64 = 250;
    const HEARTBEAT_TRANSPORT_RETRY_MS: u64 = 1_000;
    const SAVE_RECONNECT_PAUSE_MS: u64 = 1_000;
    const SAVE_RECONNECT_AUTHORITY_RESERVE_MS: u64 = 5_000;
    const SAVE_HEARTBEAT_PERIOD_MS: u64 = 5_000;

    /// A scoped helper for the long, synchronous native-seal/R2-PUT path.
    /// It shares the operator's actual cursor. There is no additional writer,
    /// persisted state or independent sequence; dropping the scope joins the
    /// heartbeat before the Bridge process exits.
    struct RetainedSaveHeartbeat {
        stop: mpsc::Sender<()>,
        thread: Option<thread::JoinHandle<()>>,
        healthy: Arc<AtomicBool>,
    }

    impl RetainedSaveHeartbeat {
        fn start(
            mut coordinator: SharedControlPlaneCoordinator<WindowsApplicationSessionHttp>,
            lease: ProfileLease,
        ) -> Self {
            let (stop, receiver) = mpsc::channel();
            let healthy = Arc::new(AtomicBool::new(true));
            let observed = Arc::clone(&healthy);
            let thread = thread::spawn(move || {
                while matches!(
                    receiver.recv_timeout(Duration::from_millis(SAVE_HEARTBEAT_PERIOD_MS)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    match (coordinator.runtime_timing(), now()) {
                        (Ok(timing), Ok(observed_at))
                            if safe_reconnect_writer_window(
                                timing.idle_expires_at_ms(),
                                timing.hard_expires_at_ms(),
                                observed_at,
                            ) => {}
                        _ => {
                            observed.store(false, Ordering::Release);
                            return;
                        }
                    }
                    // One exact-key retry on transport uncertainty only.
                    // The shared coordinator retains the same command and
                    // proves a replay against fresh DO state; no guessed
                    // authority or second sequence is ever minted.
                    let acknowledged = match coordinator.heartbeat_lease(&lease) {
                        Ok(()) => true,
                        Err(
                            ShippingControlPlaneError::Transport
                            | ShippingControlPlaneError::HttpServerUncertain,
                        ) => {
                            thread::sleep(Duration::from_millis(HEARTBEAT_TRANSPORT_RETRY_MS));
                            matches!(
                                (coordinator.runtime_timing(), now()),
                                (Ok(timing), Ok(observed_at))
                                    if safe_reconnect_writer_window(
                                        timing.idle_expires_at_ms(),
                                        timing.hard_expires_at_ms(),
                                        observed_at,
                                    )
                            ) && coordinator.heartbeat_lease(&lease).is_ok()
                        }
                        Err(_) => false,
                    };
                    // Only confirmed authority permits the ongoing Save to
                    // finish; the original native checkpoint always survives.
                    if !acknowledged {
                        observed.store(false, Ordering::Release);
                        eprintln!(
                            "P3_SAVE_HEARTBEAT=NOT_CONFIRMED;NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES"
                        );
                        return;
                    }
                }
            });
            Self {
                stop,
                thread: Some(thread),
                healthy,
            }
        }

        fn is_healthy(&self) -> bool {
            self.healthy.load(Ordering::Acquire)
        }
    }

    impl Drop for RetainedSaveHeartbeat {
        fn drop(&mut self) {
            let _ = self.stop.send(());
            if let Some(task) = self.thread.take() {
                let _ = task.join();
            }
        }
    }

    struct ShippingConfig {
        device_application_binding_path: PathBuf,
        control_plane_origin: String,
        materialization_root: PathBuf,
    }

    impl ShippingConfig {
        fn from_installation() -> Result<Self, ShippingCompositionError> {
            // Never silently discard restrictions left by a previous manual setup.
            // New installations select network behavior from the authenticated generation.
            if env::var_os(NETWORK_POLICY_PATH_ENV).is_some()
                || env::var_os(PROXY_CONFIG_PATH_ENV).is_some()
            {
                return Err(ShippingCompositionError::ExternalNetworkConfiguration);
            }
            let connection = crate::installed_connection::InstalledConnection::open_current()
                .map_err(ShippingCompositionError::InstalledConnection)?;
            let device_application_binding_path = connection.binding_path();
            let control_plane_origin = connection.origin().as_str().to_owned();
            let materialization_root = materialization_root_path()?;
            Ok(Self {
                device_application_binding_path,
                control_plane_origin,
                materialization_root,
            })
        }
    }

    /// Reuse the existing authenticated Profile claim/DO fence, encrypted
    /// successor writer, signed R2 PUT and D1 CAS ONLY after native Close.
    /// Ordinary browser Open remains local and never acquires this authority.
    ///
    /// This does not launch Camoufox, clone Firefox into a generation folder,
    /// introduce a daemon or replace the stable local current path.
    fn sync_closed_ready(
        intent: &crate::generation_initialization::InitialProfileOpen,
        binding: &WindowsDeviceApplicationBinding,
        root: &MaterializationRoot,
        origin: &str,
    ) -> Result<(), ()> {
        use crate::browser_execution::browser_writer_locks_absent;
        use crate::local_profile::BridgeWorkspaceLock;
        use std::fs;

        let workspace = root
            .open_profile_current(&intent.tenant_id, &intent.profile_id)
            .map_err(|_| ())?;
        let Some(base) = workspace.base_generation_id().cloned() else {
            return Err(());
        };
        if workspace.profile_version() != intent.expected_version
            || fs::read(workspace.path().join(".local-unsynced")).map_err(|_| ())?
                != b"local-unsynced-v2"
        {
            return Err(());
        }
        let device = binding.device_id();
        let lock = BridgeWorkspaceLock::acquire(&workspace, device, now().map_err(|_| ())?.value())
            .map_err(|_| ())?;
        // The snapshot, not the browser, is the only temporary payload.
        let captured = (|| {
            if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
                return Err(());
            }
            let inventory = workspace.inventory().map_err(|_| ())?;
            let snapshot = zeroize::Zeroizing::new(
                encode_workspace_snapshot(&workspace, &inventory).map_err(|_| ())?,
            );
            if workspace.inventory().map_err(|_| ())? != inventory {
                return Err(());
            }
            Ok((snapshot, inventory))
        })();
        let unlock = lock.release().map_err(|_| ());
        let (snapshot, original_inventory) = captured?;
        unlock?;

        let application = WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
            .map_err(|_| ())?;
        let mut transport = application.transport();
        let actor = profile_platform_primitives::ActorContext::new(
            profile_platform_primitives::TenantScope::new(intent.tenant_id.clone()),
            intent.actor_id.clone(),
            profile_platform_primitives::CorrelationId::parse("corr_closed_ready_sync")
                .map_err(|_| ())?,
        );
        // Fresh authorization is acquired for Sync, not for browser Open.
        let claim = reauthorize_current_ready_generation(intent, &actor, device, &mut transport)
            .map_err(|_| ())?;
        let mut enrollment = ControlPlaneEnrollment::new(transport.clone());
        let enrolled = enrollment
            .redeem_claim(&claim, device, now().map_err(|_| ())?)
            .map_err(|_| ())?;
        if enrolled.actor().actor_id() != actor.actor_id()
            || enrolled.actor().tenant_scope().tenant_id() != actor.tenant_scope().tenant_id()
            || enrolled.profile_id() != &intent.profile_id
            || enrolled.generation_id() != &base
        {
            return Err(());
        }

        let mut coordinator =
            SharedControlPlaneCoordinator::new(ControlPlaneCoordinator::new(transport.clone()));
        let lease = coordinator
            .claim_launch_intent(
                enrolled.actor(),
                &intent.profile_id,
                device,
                enrolled.launch_intent_id(),
            )
            .map_err(|_| ())?;
        // The existing heartbeat owner maintains the same fence during long PUT.
        let heartbeat = RetainedSaveHeartbeat::start(coordinator.clone(), lease.clone());
        let outcome = (|| {
            let mut sealer = coordinator.clone();
            let mut observation_transport = transport.clone();
            let mut control = ControlPlaneGenerationSuccessor::new(transport, &mut sealer);
            let prepared = prepare_closed_current_successor(
                &workspace,
                &original_inventory,
                &snapshot,
                &intent.tenant_id,
                &intent.profile_id,
                &base,
                &mut control,
            )
            .map_err(|_| ())?;
            let mut upload = WindowsSignedGenerationObjectPut::from_system().map_err(|_| ())?;
            if !heartbeat.is_healthy() {
                return Err(());
            }
            let committed = match publish_verify_and_commit_successor(
                &intent.tenant_id,
                &intent.profile_id,
                &base,
                &prepared,
                &lease,
                &mut control,
                &mut upload,
            ) {
                Ok(receipt) => receipt.generation_id().clone(),
                Err(_) => {
                    // An HTTP reply may be lost *after* D1 committed.
                    // Observe the EXACT originally sealed object read-only;
                    // negative/uncertain observation never replays a PUT.
                    if !observe_closed_current_exact(
                        &intent.tenant_id,
                        &intent.profile_id,
                        &base,
                        &prepared,
                        &mut observation_transport,
                    )
                    .map_err(|_| ())?
                    {
                        return Err(());
                    }
                    prepared.sealed().metadata().generation_id().clone()
                }
            };
            if !heartbeat.is_healthy() {
                return Err(());
            }
            Ok(committed)
        })();
        drop(heartbeat);
        if outcome.is_ok() {
            coordinator.close_confirmed_save(&lease).map_err(|_| ())?;
        } else {
            let _ = coordinator.close_lease(&lease);
        }
        let committed = outcome?;

        // Apply only the confirmed server revision; if another local browser
        // already holds the writer, do not touch its bytes or clear dirty.
        let active = root
            .open_profile_current(&intent.tenant_id, &intent.profile_id)
            .map_err(|_| ())?;
        let guard = BridgeWorkspaceLock::acquire(&active, device, now().map_err(|_| ())?.value())
            .map_err(|_| ())?;
        let applied = (|| {
            if active.base_generation_id() != Some(&base)
                || active.profile_version() != intent.expected_version
                || !matches!(browser_writer_locks_absent(&active), Ok(true))
            {
                return Err(());
            }
            let unchanged = active.inventory().map_err(|_| ())? == original_inventory;
            root.update_profile_current_sync_base(
                &active,
                &committed,
                intent.expected_version.checked_add(1).ok_or(())?,
            )
            .map_err(|_| ())?;
            if unchanged {
                let dirty = active.path().join(".local-unsynced");
                let metadata = fs::symlink_metadata(&dirty).map_err(|_| ())?;
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || fs::read(&dirty).map_err(|_| ())? != b"local-unsynced-v2"
                {
                    return Err(());
                }
                fs::remove_file(&dirty).map_err(|_| ())?;
            }
            Ok(())
        })();
        let unlocked = guard.release().map_err(|_| ());
        applied?;
        unlocked
    }

    /// The SAME post-close handler runs after normal Open and READY restore.
    /// Neither Firefox closure nor its local bytes depend on the cloud result.
    fn sync_after_native_close(
        intent: &crate::generation_initialization::InitialProfileOpen,
        binding: &WindowsDeviceApplicationBinding,
        root: &MaterializationRoot,
        origin: &str,
    ) {
        if intent.status == InitialProfileStatus::Ready {
            if let Err(()) = sync_closed_ready(intent, binding, root, origin) {
                eprintln!("bridge_closed_ready_sync_pending;NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES");
            }
        } else if intent.status == InitialProfileStatus::Draft {
            let sync = (|| -> Result<(), InitialProfileError> {
                let application =
                    WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
                        .map_err(|_| InitialProfileError::Authorization)?;
                let mut transport = application.transport();
                let mut uploader = WindowsSignedGenerationObjectPut::from_system()
                    .map_err(|_| InitialProfileError::Upload)?;
                let actor = profile_platform_primitives::ActorContext::new(
                    profile_platform_primitives::TenantScope::new(intent.tenant_id.clone()),
                    intent.actor_id.clone(),
                    profile_platform_primitives::CorrelationId::parse("corr_local_draft_sync")
                        .map_err(|_| InitialProfileError::Authorization)?,
                );
                sync_closed_local_draft(
                    intent,
                    &actor,
                    binding.device_id(),
                    &root,
                    &mut transport,
                    &mut uploader,
                )
            })();
            if let Err(reason) = sync {
                // Diagnostic classification only. A network outage,
                // lost response or local conflict is NOT a Save receipt.
                eprintln!("bridge_closed_draft_sync_pending reason={reason:?}");
            }
        }
    }

    pub fn run_initial(
        intent: &crate::generation_initialization::InitialProfileOpen,
    ) -> Result<(), ShippingCompositionError> {
        // The CRM web page already supplied the selected profile. Opening the
        // browser is now a local filesystem/process operation, not an R2
        // initialization or a Coordinator lease. Sync belongs to a different
        // operation and cannot block a local Firefox window.
        if recover_delivery_before_claim()? {
            return Ok(());
        }
        let config = ShippingConfig::from_installation()?;
        let delivery = ActiveWindowsDeliveryRuntime::resolve_for_browser_open()
            .map_err(ShippingCompositionError::DeliveryRuntime)?;
        let binding =
            WindowsDeviceApplicationBinding::open(&config.device_application_binding_path)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        if binding.tenant_id() != &intent.tenant_id || binding.actor_id() != &intent.actor_id {
            return Err(ShippingCompositionError::Configuration);
        }
        let root = MaterializationRoot::open_or_create(&config.materialization_root)
            .map_err(|_| ShippingCompositionError::Configuration)?;
        match crate::local_browser_open::open(
            &root,
            intent,
            binding.device_id(),
            delivery.runtime_root(),
        ) {
            Ok(()) => {
                sync_after_native_close(intent, &binding, &root, &config.control_plane_origin);
                Ok(())
            }
            Err(crate::local_browser_open::LocalOpenError::RestoreRequired) => {
                // Only a missing READY local folder crosses the cloud boundary.
                // Ordinary Open/Close never requires a network request.
                let device = binding.device_id().clone();
                let application = WindowsDeviceApplication::from_system(
                    config.control_plane_origin.clone(),
                    binding.clone(),
                )
                .map_err(|_| ShippingCompositionError::RestoreUnavailable)?;
                let mut remote =
                    ControlPlaneCoordinator::for_local_current_restore(application.transport());
                // Worker selects CURRENT; neither the browser URI nor local files
                // may choose a cloud generation or an historical fallback.
                let selected = remote
                    .download_capability(&intent.tenant_id, &intent.profile_id)
                    .map_err(|_| ShippingCompositionError::RestoreUnavailable)?;
                let reader = WindowsSignedGenerationObjectGet::from_system()
                    .map_err(|_| ShippingCompositionError::RestoreUnavailable)?;
                let mut downloader = VerifiedGenerationObjectStreamDownloader::new(reader);
                let epoch = now()?.value();
                ensure_authoritative_current(
                    &root,
                    &intent.tenant_id,
                    &intent.profile_id,
                    selected.generation_id(),
                    intent.expected_version,
                    &device,
                    epoch,
                    &mut remote,
                    &mut downloader,
                )
                .map_err(|_| ShippingCompositionError::RestoreUnavailable)?;
                // Re-enter the SAME local native Open; no managed Camoufox path
                // and no Coordinator lease are introduced by recovery.
                crate::local_browser_open::open(&root, intent, &device, delivery.runtime_root())
                    .map_err(ShippingCompositionError::LocalOpen)?;
                sync_after_native_close(intent, &binding, &root, &config.control_plane_origin);
                Ok(())
            }
            Err(error) => Err(ShippingCompositionError::LocalOpen(error)),
        }
    }

    pub fn run(claim: &ClaimUri) -> Result<(), ShippingCompositionError> {
        #[cfg(windows)]
        let preparation_started = std::time::Instant::now();
        if recover_delivery_before_claim()? {
            return Ok(());
        }
        #[cfg(windows)]
        eprintln!(
            "bridge_open_phase name=delivery_recovery_readback elapsed_ms={}",
            preparation_started.elapsed().as_millis()
        );
        #[cfg(windows)]
        let config_started = std::time::Instant::now();
        let config = ShippingConfig::from_installation()?;
        #[cfg(windows)]
        eprintln!(
            "bridge_open_phase name=installation_config elapsed_ms={}",
            config_started.elapsed().as_millis()
        );
        let active_delivery =
            ActiveWindowsDeliveryRuntime::resolve_for_browser_open().map_err(|error| {
                eprintln!("BRIDGE_DELIVERY_RUNTIME_FAILURE={error:?}");
                ShippingCompositionError::DeliveryRuntime(error)
            })?;
        let (release_sequence, runtime_id) = active_delivery.observed_release_identity();
        report::record_runtime_identity(release_sequence, runtime_id);
        let owner_setup_started = std::time::Instant::now();
        let pending_health = active_delivery.pending_health_confirmation();
        let runtime_root = active_delivery.runtime_root().to_path_buf();
        let runtime_bundles = active_delivery.into_bundle_selection();

        let binding =
            WindowsDeviceApplicationBinding::open(&config.device_application_binding_path)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        let identity = WindowsDeviceIdentity::new(binding.device_id().clone());
        let device_application =
            WindowsDeviceApplication::from_system(config.control_plane_origin, binding)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        let transport = device_application.transport();
        let save_transport = transport.clone();
        let mut recovery_observation_transport = transport.clone();
        let signed_generation_get = WindowsSignedGenerationObjectGet::from_system()
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let mut generation_downloader =
            VerifiedGenerationObjectStreamDownloader::new(signed_generation_get);
        let mut generation_uploader = WindowsSignedGenerationObjectPut::from_system()
            .map_err(|_| ShippingCompositionError::Configuration)?;

        let enrollment = ControlPlaneEnrollment::new(transport.clone());
        let coordinator =
            SharedControlPlaneCoordinator::new(ControlPlaneCoordinator::new(transport));
        let materialization_root =
            MaterializationRoot::open_or_create(&config.materialization_root)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        let runtime_binding = RuntimeBindingSlot::new();
        let browser_preflight = ShippingBrowserLaunchPreflight::new(runtime_binding.clone());
        let python_executable = runtime_root.join(PACKAGED_PYTHON_EXECUTABLE);
        let camouhost_config = ManagedCamouhostConfig::new(
            python_executable,
            runtime_root,
            RuntimeDisplayMode::Headful,
            None,
            None,
        )
        .map_err(|_| ShippingCompositionError::Configuration)?;
        let (process, camouhost) = ManagedCamouhostProcess::pair(camouhost_config, runtime_binding);
        let mut close_observer = camouhost.close_observer();

        let mut operator = ProfileBridgeOperator::new(
            identity,
            device_application.clone(),
            device_application,
            enrollment,
            coordinator,
            runtime_bundles,
            browser_preflight,
            process,
            camouhost,
        );
        eprintln!(
            "bridge_open_phase name=owner_setup elapsed_ms={}",
            owner_setup_started.elapsed().as_millis()
        );
        let opened_at = now()?;
        let open_started = std::time::Instant::now();
        report::record_current(
            Stage::GenerationSelection,
            Outcome::Started,
            Cloud::Unknown,
            Cause::None,
            0,
        );
        let mut pending_health = pending_health;
        let mut started_health = None;
        let mut prelaunch_keepalive = None;
        let selection_started = std::time::Instant::now();
        let opening = operator
            .open_authoritative_with_runtime_health_and_pending_recovery(
                claim,
                &materialization_root,
                &mut generation_downloader,
                opened_at,
                (|current, lease| {
                    // The same original cursor stays alive during exact
                    // pending N+1 recovery, R2 download and Camoufox startup.
                    prelaunch_keepalive = Some(RetainedSaveHeartbeat::start(
                        current.clone(),
                        lease.clone(),
                    ));
                },
                || {
                    started_health = pending_health
                        .take()
                        .map(|confirmation| {
                            confirmation.start_before_runtime_health().map_err(|_| ())
                        })
                        .transpose()?;
                    Ok(())
                }),
                |live_coordinator, tenant, profile, selected_generation, fresh_lease| {
                    // The pre-launch keepalive above already spans exact
                    // recovery and any long R2 PUT. No duplicate thread.
                    // The existing operator already proved fresh CNG/ACL,
                    // tenant/Profile enrollment and exclusive coordinator
                    // writer claim. Only exact ACTIVE_EXACT may repair an
                    // unchanged N+1 binding; never PUT/commit with old fence.
                    let recovered = match complete_active_exact_after_restart(
                        &materialization_root,
                        tenant,
                        profile,
                        selected_generation,
                        fresh_lease,
                        &mut recovery_observation_transport,
                    ) {
                        Ok(RestartExactLocalCompletion::ActiveExactLocalReady) => {
                            eprintln!(
                                "P3_RECOVERY=ACTIVE_EXACT_LOCAL_READY;NEXT_SAFE_STEP=CONTINUE_OPEN"
                            );
                            PendingSaveRecovery::ReadySelected
                        }
                        Ok(RestartExactLocalCompletion::RetryDecisionConfirmedAwaitingResumption) => {
                            // The fresh lease selected N, not N+1. Even if
                            // replay commits successfully, this launch must
                            // NOT open N. A new claim selects and completes
                            // the authoritative exact N+1 on the next Open.
                            let committed = match resume_exact_successor_under_fresh_lease(
                                &materialization_root,
                                tenant,
                                profile,
                                selected_generation,
                                fresh_lease,
                                &mut recovery_observation_transport,
                                &mut generation_uploader,
                            ) {
                                Ok(true) => {
                                    eprintln!(
                                        "P3_RECOVERY=RETRY_COMMITTED;NEXT_SAFE_STEP=FRESH_OPEN_ACTIVE_EXACT"
                                    );
                                    true
                                }
                                Ok(false) => {
                                    eprintln!(
                                        "P3_RECOVERY=RETRY_BLOCKED;NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES"
                                    );
                                    false
                                }
                                Err(error) => {
                                    eprintln!(
                                        "P3_RECOVERY={};NEXT_SAFE_STEP={}",
                                        error.code(),
                                        recovery_next_safe_step(error)
                                    );
                                    return pending_recovery_failure_disposition(error);
                                }
                            };
                            if committed {
                                PendingSaveRecovery::CommittedNewerRequiresClaim
                            } else {
                                PendingSaveRecovery::Blocked
                            }
                        }
                        Ok(RestartExactLocalCompletion::NativeRescueRequired) => {
                            eprintln!("P3_RECOVERY=NATIVE_RESCUE_REQUESTED;NEXT_SAFE_STEP=FRESH_CLAIM_NATIVE_JOURNAL");
                            PendingSaveRecovery::NativeRescueRequired
                        }
                        Ok(RestartExactLocalCompletion::RetryDecisionBlocked) => {
                            eprintln!("P3_RECOVERY=RETRY_BLOCKED;NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES");
                            PendingSaveRecovery::Blocked
                        }
                        Ok(RestartExactLocalCompletion::NotActiveExact) => {
                            let diagnosis = PendingSaveReadOnlyDiagnosis::from_observation(
                                Ok(RestartExactCommitObservation::NotActiveExact),
                            );
                            eprintln!(
                                "{};NEXT_SAFE_STEP={}",
                                diagnosis.code(),
                                diagnosis.next_safe_step()
                            );
                            PendingSaveRecovery::Blocked
                        }
                        Ok(RestartExactLocalCompletion::LocalUnsyncedAuthorizedAwaitingFirstSeal) => {
                            // The one current coordinator owns the new claimed
                            // fence AND the generation sealing material cursor.
                            // No second coordinator or persisting recovery SM.
                            let committed = match resume_lone_local_unsynced_under_fresh_lease(
                                &materialization_root,
                                tenant,
                                profile,
                                selected_generation,
                                fresh_lease,
                                live_coordinator,
                                &mut recovery_observation_transport,
                                &mut generation_uploader,
                            ) {
                                Ok(true) => {
                                    eprintln!(
                                        "P3_RECOVERY=FIRST_SUCCESSOR_COMMITTED;NEXT_SAFE_STEP=FRESH_OPEN_ACTIVE_EXACT"
                                    );
                                    true
                                }
                                Ok(false) => {
                                    eprintln!(
                                        "P3_RECOVERY=FIRST_SEAL_BLOCKED;NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES"
                                    );
                                    false
                                }
                                Err(RestartExactLocalCompletionError::NativeSnapshotTooLarge) => {
                                    // Not a PUT retry grant. This Open already owns
                                    // a freshly redeemed/current N writer claim;
                                    // the operator must also acquire native OS
                                    // admission, verify the original and publish
                                    // rescue-in-progress before Camoufox launch.
                                    eprintln!("P3_RECOVERY=NATIVE_RESCUE_REQUIRED;NEXT_SAFE_STEP=VERIFY_ORIGINAL_AND_JOURNAL");
                                    return PendingSaveRecovery::NativeRescueRequired;
                                }
                                Err(error) => {
                                    eprintln!(
                                        "P3_RECOVERY={};NEXT_SAFE_STEP={}",
                                        error.code(),
                                        recovery_next_safe_step(error)
                                    );
                                    return pending_recovery_failure_disposition(error);
                                }
                            };
                            // First-sealed LOCAL_UNSYNCED uses the same exact
                            // checkpoint/Firefox-writer proofs as sealed BPAT.
                            // Never open stale N, but don't strand a proved
                            // committed N+1 behind Uncertain coordinator state.
                            if committed {
                                PendingSaveRecovery::CommittedNewerRequiresClaim
                            } else {
                                PendingSaveRecovery::Blocked
                            }
                        }
                        Ok(RestartExactLocalCompletion::NoCanonicalCheckpoint) => {
                            let diagnosis = PendingSaveReadOnlyDiagnosis::from_observation(
                                Ok(RestartExactCommitObservation::NoCanonicalCheckpoint),
                            );
                            eprintln!(
                                "{};NEXT_SAFE_STEP={}",
                                diagnosis.code(),
                                diagnosis.next_safe_step()
                            );
                            PendingSaveRecovery::Blocked
                        }
                        Ok(RestartExactLocalCompletion::ServerSelectedDifferentGeneration) => {
                            eprintln!(
                                "P3_RECOVERY=CONFLICT;NEXT_SAFE_STEP=REQUIRE_CURRENT_SERVER_GENERATION"
                            );
                            PendingSaveRecovery::Blocked
                        }
                        Err(error) => {
                            eprintln!(
                                "P3_RECOVERY={};NEXT_SAFE_STEP={}",
                                error.code(),
                                recovery_next_safe_step(error)
                            );
                            pending_recovery_failure_disposition(error)
                        }
                    };
                    recovered
                },
            );
        eprintln!(
            "bridge_open_phase name=authoritative_open elapsed_ms={} completed={}",
            selection_started.elapsed().as_millis(),
            opening.is_ok(),
        );
        // Always finish the heartbeat thread before observing its final
        // authority result. A slow or failed heartbeat cannot be ignored just
        // because the browser opened successfully in the foreground.
        let health = prelaunch_keepalive
            .as_ref()
            .map(|guard: &RetainedSaveHeartbeat| Arc::clone(&guard.healthy));
        drop(prelaunch_keepalive);
        opening.map_err(ShippingCompositionError::Operator)?;
        if !health
            .as_ref()
            .is_some_and(|confirmed| confirmed.load(Ordering::Acquire))
        {
            let _ = operator.abort(now().unwrap_or(opened_at));
            return Err(ShippingCompositionError::HeartbeatAuthorityUncertain);
        }

        if let Some(confirmation) = started_health
            && confirmation.confirm_after_runtime_ready().is_err()
        {
            let abort_at = now().unwrap_or(opened_at);
            let _ = operator.abort(abort_at);
            return Err(ShippingCompositionError::DeliveryHealth);
        }
        report::record_current(
            Stage::BrowserReady,
            Outcome::Observed,
            Cloud::Unknown,
            Cause::None,
            u64::try_from(open_started.elapsed().as_millis())
                .unwrap_or(u64::MAX)
                .min(86_400_000),
        );

        let mut next_heartbeat_deadline = next_heartbeat_at(
            operator
                .coordinator()
                .runtime_timing()
                .map_err(|_| ShippingCompositionError::ControlPlane)?,
            now()?,
        )?;
        // An unanswered heartbeat does not refresh the coordinator deadline.
        // We allow at most ONE narrow transport-only retry before gracefully
        // quiescing the local browser and preserving its native WAL/session data.
        let mut heartbeat_retry_used = false;
        loop {
            let observed_at = now()?;
            let timing = operator
                .coordinator()
                .runtime_timing()
                .map_err(|_| ShippingCompositionError::ControlPlane)?;
            // Check BEFORE controlled-close observation or any save attempt.
            // A loop delayed past the confirmed lease deadline must not
            // publish under a stale fence even if the browser just closed.
            if observed_at.value() >= timing.idle_expires_at_ms().min(timing.hard_expires_at_ms()) {
                // Unlike a transport timeout, this deadline is known to
                // have expired. Immediately fence the writer; do not spend a
                // graceful shutdown interval writing without coordinator authority.
                return Err(ShippingCompositionError::Operator(
                    operator.stop_after_confirmed_lease_expiry(observed_at),
                ));
            }
            let session_id = operator
                .active_session_id()
                .cloned()
                .ok_or(ShippingCompositionError::ActiveSessionMissing)?;
            let close_disposition =
                close_observation_disposition(close_observer.observe_controlled_close(&session_id))
                    .map_err(ShippingCompositionError::CloseObservation)?;
            if close_disposition == CloseObservationDisposition::RecoverUncleanExit {
                operator
                    .abort(now()?)
                    .map_err(ShippingCompositionError::Operator)?;
                return Err(ShippingCompositionError::CloseObservation(
                    bridge_domain::BridgePortError::UncleanBrowserExit,
                ));
            }
            if close_disposition == CloseObservationDisposition::BeginConfirmedSave {
                operator
                    .close_with_durable_local_intent(&materialization_root, now()?)
                    .map_err(ShippingCompositionError::Operator)?;
                eprintln!(
                    "{}",
                    ReadOnlyProfileDiagnosis::after_durable_local_close(
                        operator.diagnostic_facts()
                    )
                    .redacted_record()
                );
                report::record_current(
                    Stage::LocalUnsynced,
                    Outcome::Unconfirmed,
                    Cloud::NotConfirmed,
                    Cause::None,
                    0,
                );
                // An unknown heartbeat response is not an acknowledged fence.
                // Retain the closed local working profile; do not attempt an
                // upload/commit using a possibly revoked writer session.
                if heartbeat_retry_used {
                    return Err(ShippingCompositionError::HeartbeatAuthorityUncertain);
                }
                // Native sealing makes several full passes; the signed R2 PUT
                // can block for up to 120 seconds. The browser event loop
                // cannot heartbeat during either operation. This ONE scoped
                // keepalive uses the same serialized coordinator cursor.
                let retained = operator
                    .retained_save_lease()
                    .cloned()
                    .ok_or(ShippingCompositionError::ActiveSessionMissing)?;
                let save_heartbeat =
                    RetainedSaveHeartbeat::start(operator.coordinator().clone(), retained);
                report::record_current(
                    Stage::CloudSaveAttempt,
                    Outcome::Started,
                    Cloud::NotConfirmed,
                    Cause::None,
                    0,
                );
                let save_started = std::time::Instant::now();
                let initial = operator.save_retained_successor_streaming(
                    &materialization_root,
                    save_transport.clone(),
                    &mut generation_uploader,
                    now()?,
                );
                let confirmed = initial.as_ref().is_ok_and(|done| done.is_saved());
                report::record_current(
                    if confirmed {
                        Stage::CloudSaveConfirmed
                    } else {
                        Stage::CloudSaveAttempt
                    },
                    if confirmed {
                        Outcome::Succeeded
                    } else {
                        Outcome::Unconfirmed
                    },
                    if confirmed {
                        Cloud::CommittedExact
                    } else {
                        Cloud::NotConfirmed
                    },
                    if confirmed {
                        Cause::None
                    } else {
                        Cause::CloudSaveFailed
                    },
                    u64::try_from(save_started.elapsed().as_millis())
                        .unwrap_or(u64::MAX)
                        .min(86_400_000),
                );
                eprintln!(
                    "{}",
                    ReadOnlyProfileDiagnosis::after_save(operator.diagnostic_facts(), &initial)
                        .redacted_record()
                );
                if !save_heartbeat.is_healthy()
                    && !initial.as_ref().is_ok_and(|done| done.is_saved())
                {
                    return Err(ShippingCompositionError::HeartbeatAuthorityUncertain);
                }
                // Pre-seal transport uncertainty precedes BPAT creation.
                // Reattempt only the original local seal after a confirmed
                // live heartbeat; ordinary D1 retry fencing still guards PUT.
                let mut first_seal_pending = matches!(
                    &initial,
                    Err(OperatorFlowError::GenerationSave { reason })
                        if reason.allows_bounded_preseal_reconnect()
                );
                match initial {
                    Ok(completion) if completion.is_saved() => return Ok(()),
                    Ok(_) => return Err(ShippingCompositionError::CommittedRecoveryRequired),
                    Err(OperatorFlowError::GenerationSave { reason })
                        if !reason.allows_bounded_cloud_reconcile()
                            && !reason.allows_bounded_preseal_reconnect() =>
                    {
                        if reason
                            == crate::operator_flow::GenerationSaveFailureReason::SnapshotTooLarge
                        {
                            eprintln!(
                                "P3_RECOVERY=NATIVE_SNAPSHOT_TOO_LARGE;NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES;NATIVE_RESCUE_REQUIRED"
                            );
                        }
                        return Err(ShippingCompositionError::Operator(
                            OperatorFlowError::GenerationSave { reason },
                        ));
                    }
                    Err(
                        OperatorFlowError::Stage(OperatorFailureStage::GenerationSave)
                        | OperatorFlowError::GenerationSave { .. },
                    ) if operator.has_pending_dirty_close() => {
                        // Browser closed: WAL/native bytes and the original
                        // workspace lock are retained. Do not start Firefox.
                        // Retry ONLY the already-sealed exact checkpoint.
                        // Both a current DO heartbeat AND affirmative D1
                        // descriptor-level nonapplication are mandatory.
                        eprintln!(
                            "P3_RECOVERY=LOCAL_UNSYNCED;NEXT_SAFE_STEP=BOUNDED_LIVE_RECONNECT"
                        );
                        // Do not abandon a recoverable Save after an arbitrary
                        // 30 seconds. The current Coordinator's confirmed idle
                        // and hard deadlines are the real upper bound. Every
                        // network effect below re-proves the same writer and
                        // exact D1 nonapplication before reusing N+1.
                        loop {
                            if !save_heartbeat.is_healthy() {
                                return Err(ShippingCompositionError::HeartbeatAuthorityUncertain);
                            }
                            let timing = operator
                                .coordinator()
                                .runtime_timing()
                                .map_err(|_| ShippingCompositionError::ControlPlane)?;
                            if !safe_reconnect_writer_window(
                                timing.idle_expires_at_ms(),
                                timing.hard_expires_at_ms(),
                                now()?,
                            ) {
                                break;
                            }
                            thread::sleep(Duration::from_millis(SAVE_RECONNECT_PAUSE_MS));
                            let timing = operator
                                .coordinator()
                                .runtime_timing()
                                .map_err(|_| ShippingCompositionError::ControlPlane)?;
                            if !safe_reconnect_writer_window(
                                timing.idle_expires_at_ms(),
                                timing.hard_expires_at_ms(),
                                now()?,
                            ) {
                                break;
                            }
                            match operator.reprove_retained_save_writer() {
                                Ok(()) => {}
                                Err(OperatorFlowError::HeartbeatTransportUncertain) => continue,
                                Err(error) => {
                                    return Err(ShippingCompositionError::Operator(error));
                                }
                            }
                            let timing = operator
                                .coordinator()
                                .runtime_timing()
                                .map_err(|_| ShippingCompositionError::ControlPlane)?;
                            if !safe_reconnect_writer_window(
                                timing.idle_expires_at_ms(),
                                timing.hard_expires_at_ms(),
                                now()?,
                            ) {
                                break;
                            }
                            let original = operator
                                .retained_save_lease()
                                .cloned()
                                .ok_or(ShippingCompositionError::CommittedRecoveryRequired)?;
                            // No exact BPAT exists after a transient first-seal
                            // failure. The retained Save must still prove
                            // the exact descriptor in D1 before any cloud PUT.
                            if !first_seal_pending {
                                match confirm_retained_exact_retry_nonapplication(
                                    &materialization_root,
                                    &original,
                                    &mut recovery_observation_transport,
                                ) {
                                    Ok(true) => {}
                                    Ok(false) => {
                                        return Err(
                                            ShippingCompositionError::CommittedRecoveryRequired,
                                        );
                                    }
                                    Err(error) if retryable_reconnect_observation(error) => {
                                        // No D1-positive decision was observed. Wait
                                        // only within the confirmed writer deadline.
                                        continue;
                                    }
                                    Err(error) => {
                                        // Definitive 401/403/404, malformed response,
                                        // clock or changed local evidence are NOT
                                        // transient network failures. Preserve N+1.
                                        eprintln!(
                                            "P3_RECOVERY={};NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES",
                                            error.code()
                                        );
                                        return Err(
                                            ShippingCompositionError::CommittedRecoveryRequired,
                                        );
                                    }
                                }
                            }
                            let timing = operator
                                .coordinator()
                                .runtime_timing()
                                .map_err(|_| ShippingCompositionError::ControlPlane)?;
                            if !safe_reconnect_writer_window(
                                timing.idle_expires_at_ms(),
                                timing.hard_expires_at_ms(),
                                now()?,
                            ) {
                                break;
                            }
                            report::record_current(
                                Stage::CloudSaveAttempt,
                                Outcome::Started,
                                Cloud::NotConfirmed,
                                Cause::None,
                                0,
                            );
                            let retry_started = std::time::Instant::now();
                            let attempt = operator.save_retained_successor_streaming(
                                &materialization_root,
                                save_transport.clone(),
                                &mut generation_uploader,
                                now()?,
                            );
                            let confirmed = attempt.as_ref().is_ok_and(|done| done.is_saved());
                            report::record_current(
                                if confirmed {
                                    Stage::CloudSaveConfirmed
                                } else {
                                    Stage::CloudSaveAttempt
                                },
                                if confirmed {
                                    Outcome::Succeeded
                                } else {
                                    Outcome::Unconfirmed
                                },
                                if confirmed {
                                    Cloud::CommittedExact
                                } else {
                                    Cloud::NotConfirmed
                                },
                                if confirmed {
                                    Cause::None
                                } else {
                                    Cause::CloudSaveFailed
                                },
                                u64::try_from(retry_started.elapsed().as_millis())
                                    .unwrap_or(u64::MAX)
                                    .min(86_400_000),
                            );
                            eprintln!(
                                "{}",
                                ReadOnlyProfileDiagnosis::after_save(
                                    operator.diagnostic_facts(),
                                    &attempt
                                )
                                .redacted_record()
                            );
                            match attempt {
                                Ok(completion) if completion.is_saved() => return Ok(()),
                                Ok(_) => {
                                    return Err(
                                        ShippingCompositionError::CommittedRecoveryRequired,
                                    );
                                }
                                Err(OperatorFlowError::GenerationSave { reason })
                                    if !reason.allows_bounded_cloud_reconcile()
                                        && !reason.allows_bounded_preseal_reconnect() =>
                                {
                                    if reason == crate::operator_flow::GenerationSaveFailureReason::SnapshotTooLarge {
                                        eprintln!("P3_RECOVERY=NATIVE_SNAPSHOT_TOO_LARGE;NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES;NATIVE_RESCUE_REQUIRED");
                                    }
                                    return Err(ShippingCompositionError::Operator(
                                        OperatorFlowError::GenerationSave { reason },
                                    ));
                                }
                                Err(OperatorFlowError::GenerationSave { reason })
                                    if operator.has_pending_dirty_close() =>
                                {
                                    // A retry after first seal can reach D1/R2.
                                    // Next iteration must gate any exact BPAT.
                                    first_seal_pending = reason.allows_bounded_preseal_reconnect();
                                    continue;
                                }
                                Err(OperatorFlowError::Stage(
                                    OperatorFailureStage::GenerationSave,
                                )) if operator.has_pending_dirty_close() => {
                                    first_seal_pending = false;
                                    continue;
                                }
                                Err(error) => {
                                    return Err(ShippingCompositionError::Operator(error));
                                }
                            }
                        }
                        eprintln!(
                            "P3_RECOVERY=LOCAL_BYTES_RETAINED;NEXT_SAFE_STEP=FRESH_AUTHORIZED_OPEN"
                        );
                        return Err(ShippingCompositionError::CommittedRecoveryRequired);
                    }
                    Err(error) => return Err(ShippingCompositionError::Operator(error)),
                }
            }

            if observed_at >= next_heartbeat_deadline {
                match operator.heartbeat(observed_at) {
                    Ok(()) => {
                        heartbeat_retry_used = false;
                        next_heartbeat_deadline = next_heartbeat_at(
                            operator
                                .coordinator()
                                .runtime_timing()
                                .map_err(|_| ShippingCompositionError::ControlPlane)?,
                            observed_at,
                        )?;
                    }
                    Err(crate::operator_flow::OperatorFlowError::HeartbeatTransportUncertain) => {
                        if heartbeat_retry_used {
                            // Two consecutive unacknowledged transport effects:
                            // stop normal work without falsely marking Clean/Saved.
                            operator
                                .close_with_durable_local_intent(&materialization_root, observed_at)
                                .map_err(ShippingCompositionError::Operator)?;
                            return Err(ShippingCompositionError::HeartbeatAuthorityUncertain);
                        }
                        let Some(retry_at) = bounded_heartbeat_retry_at(
                            timing.idle_expires_at_ms().min(timing.hard_expires_at_ms()),
                            observed_at,
                        ) else {
                            operator
                                .close_with_durable_local_intent(&materialization_root, observed_at)
                                .map_err(ShippingCompositionError::Operator)?;
                            return Err(ShippingCompositionError::HeartbeatAuthorityUncertain);
                        };
                        heartbeat_retry_used = true;
                        next_heartbeat_deadline = retry_at;
                    }
                    Err(error) => return Err(ShippingCompositionError::Operator(error)),
                }
            }

            let until_heartbeat = next_heartbeat_deadline
                .value()
                .saturating_sub(observed_at.value())
                .max(1);
            thread::sleep(Duration::from_millis(
                until_heartbeat.min(CONTROLLED_CLOSE_POLL_MS),
            ));
        }
    }

    pub fn run_delivery_command(
        command: ShippingDeliveryCommand,
    ) -> Result<(), ShippingCompositionError> {
        let map_handoff_error = |error| match &command {
            ShippingDeliveryCommand::InstallStaged(_) => {
                ShippingCompositionError::FirstInstallation(error)
            }
            _ => ShippingCompositionError::DeliveryHandoff,
        };
        let (coordinator, current_executable) =
            delivery_process_context().map_err(map_handoff_error)?;
        let materialization_root = delivery_materialization_root(&coordinator)?;
        let guard = DeliveryActivationGuard::acquire(&materialization_root).map_err(|_| {
            map_handoff_error(
                crate::windows_delivery_handoff::DeliveryHandoffError::ActivationGuardUnavailable,
            )
        })?;
        match command {
            ShippingDeliveryCommand::InstallStaged(origin) => {
                let connection =
                    crate::installed_connection::InstalledConnection::prepare(&coordinator, origin)
                        .map_err(ShippingCompositionError::InstalledConnection)?;
                coordinator
                    .complete_installation(&guard, &current_executable)
                    .map_err(ShippingCompositionError::FirstInstallation)?;
                connection
                    .persist()
                    .map_err(ShippingCompositionError::InstalledConnection)?;
            }
            ShippingDeliveryCommand::ActivateStaged => coordinator
                .start_activation(&guard, std::process::id(), &current_executable)
                .map_err(|_| ShippingCompositionError::DeliveryHandoff)?,
            ShippingDeliveryCommand::HandoffArrived => coordinator
                .complete_arrival(&guard, &current_executable)
                .map_err(|_| ShippingCompositionError::DeliveryHandoff)?,
        }
        hold_activation_guard_until_process_exit(guard);
        Ok(())
    }

    fn recover_delivery_before_claim() -> Result<bool, ShippingCompositionError> {
        let (coordinator, current_executable) =
            delivery_process_context().map_err(|_| ShippingCompositionError::DeliveryHandoff)?;
        if !coordinator
            .restart_recovery_pending()
            .map_err(|_| ShippingCompositionError::DeliveryHandoff)?
        {
            return Ok(false);
        }
        let materialization_root = delivery_materialization_root(&coordinator)?;
        let guard = DeliveryActivationGuard::acquire(&materialization_root)
            .map_err(|_| ShippingCompositionError::DeliveryHandoff)?;
        match coordinator
            .recover_or_resume_started(&guard, std::process::id(), &current_executable)
            .map_err(|_| ShippingCompositionError::DeliveryHandoff)?
        {
            DeliveryHandoffRestartDisposition::None => Ok(false),
            DeliveryHandoffRestartDisposition::TransferScheduled => {
                hold_activation_guard_until_process_exit(guard);
                Ok(true)
            }
            DeliveryHandoffRestartDisposition::RecoveryRequired => {
                Err(ShippingCompositionError::DeliveryRecoveryRequired)
            }
        }
    }

    fn delivery_process_context() -> Result<
        (DeliveryHandoffCoordinator, PathBuf),
        crate::windows_delivery_handoff::DeliveryHandoffError,
    > {
        let current_executable = env::current_exe().map_err(|_| {
            crate::windows_delivery_handoff::DeliveryHandoffError::InvalidCurrentProcess
        })?;
        let coordinator = DeliveryHandoffCoordinator::from_current_executable(&current_executable)?;
        Ok((coordinator, current_executable))
    }

    fn delivery_materialization_root(
        coordinator: &DeliveryHandoffCoordinator,
    ) -> Result<MaterializationRoot, ShippingCompositionError> {
        let path = coordinator
            .installed_profile_root()
            .map_err(|_| ShippingCompositionError::Configuration)?;
        MaterializationRoot::open_or_create(path)
            .map_err(|_| ShippingCompositionError::Configuration)
    }

    fn materialization_root_path() -> Result<PathBuf, ShippingCompositionError> {
        let (coordinator, _) =
            delivery_process_context().map_err(|_| ShippingCompositionError::Configuration)?;
        coordinator
            .installed_profile_root()
            .map_err(|_| ShippingCompositionError::Configuration)
    }

    fn hold_activation_guard_until_process_exit(guard: DeliveryActivationGuard) {
        // The OS releases the exclusive admission handle when this process terminates. Retaining it
        // closes the window between durable handoff evidence and the old process actually exiting.
        std::mem::forget(guard);
    }

    fn next_heartbeat_at(
        timing: ControlPlaneLeaseTiming,
        observed_at: UnixMillis,
    ) -> Result<UnixMillis, ShippingCompositionError> {
        let deadline = timing.idle_expires_at_ms().min(timing.hard_expires_at_ms());
        let remaining = deadline.saturating_sub(observed_at.value());
        if remaining <= 1 {
            return Err(ShippingCompositionError::ControlPlane);
        }
        let delay = (remaining / 2).max(1);
        observed_at
            .value()
            .checked_add(delay)
            .map(UnixMillis::new)
            .ok_or(ShippingCompositionError::Clock)
    }

    // This is only a server-CONFIRMED live writer's authority window.
    // Wall-clock passage NEVER extends the DO fence, and no upload is
    // started too near the hard/idle expiration.
    fn safe_reconnect_writer_window(
        idle_expires_at_ms: u64,
        hard_expires_at_ms: u64,
        observed_at: UnixMillis,
    ) -> bool {
        observed_at
            .value()
            .checked_add(SAVE_RECONNECT_AUTHORITY_RESERVE_MS)
            .is_some_and(|finish_by| finish_by < idle_expires_at_ms.min(hard_expires_at_ms))
    }

    // Retry at most once, and strictly before the last confirmed deadline.
    // No old lease/fence authority is extended by the passage of time.
    fn bounded_heartbeat_retry_at(expiry: u64, observed_at: UnixMillis) -> Option<UnixMillis> {
        let remaining = expiry.checked_sub(observed_at.value())?;
        if remaining <= 1 {
            return None;
        }
        let delay = HEARTBEAT_TRANSPORT_RETRY_MS.min(remaining / 2).max(1);
        observed_at
            .value()
            .checked_add(delay)
            .filter(|next| *next < expiry)
            .map(UnixMillis::new)
    }

    #[cfg(test)]
    mod heartbeat_safety_tests {
        use super::{bounded_heartbeat_retry_at, safe_reconnect_writer_window};
        use profile_platform_primitives::UnixMillis;

        #[test]
        fn live_save_reconnect_requires_positive_deadline_margin() {
            assert!(safe_reconnect_writer_window(
                60_000,
                120_000,
                UnixMillis::new(1_000),
            ));
            assert!(!safe_reconnect_writer_window(
                60_000,
                120_000,
                UnixMillis::new(55_000),
            ));
            assert!(!safe_reconnect_writer_window(
                60_000,
                120_000,
                UnixMillis::new(60_000),
            ));
            assert!(!safe_reconnect_writer_window(
                60_000,
                120_000,
                UnixMillis::new(u64::MAX),
            ));
            assert!(!safe_reconnect_writer_window(
                80_000,
                60_000,
                UnixMillis::new(56_000),
            ));
        }

        #[test]
        fn live_save_reconnect_outlasts_thirty_seconds_only_with_current_authority() {
            // A brief outage that exceeds the previous 30 attempts must not
            // strand the retained exact checkpoint when the writer is valid.
            assert!(safe_reconnect_writer_window(
                120_000,
                180_000,
                UnixMillis::new(35_000),
            ));
            assert!(safe_reconnect_writer_window(
                120_000,
                180_000,
                UnixMillis::new(110_000),
            ));
            // Never continue at/inside the five-second reserve or after
            // either independently enforced deadline.
            assert!(!safe_reconnect_writer_window(
                120_000,
                180_000,
                UnixMillis::new(115_000),
            ));
            assert!(!safe_reconnect_writer_window(
                180_000,
                120_000,
                UnixMillis::new(115_000),
            ));
        }

        #[test]
        fn retry_is_at_most_one_second_and_never_reaches_lease_deadline() {
            assert_eq!(
                bounded_heartbeat_retry_at(60_000, UnixMillis::new(10_000)),
                Some(UnixMillis::new(11_000))
            );
            assert_eq!(
                bounded_heartbeat_retry_at(60_000, UnixMillis::new(59_900)),
                Some(UnixMillis::new(59_950))
            );
            assert_eq!(
                bounded_heartbeat_retry_at(60_000, UnixMillis::new(59_999)),
                None
            );
            assert_eq!(
                bounded_heartbeat_retry_at(60_000, UnixMillis::new(60_000)),
                None
            );
            assert_eq!(
                bounded_heartbeat_retry_at(60_000, UnixMillis::new(60_001)),
                None
            );
        }
    }

    fn now() -> Result<UnixMillis, ShippingCompositionError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ShippingCompositionError::Clock)?
            .as_millis();
        let millis = u64::try_from(millis).map_err(|_| ShippingCompositionError::Clock)?;
        Ok(UnixMillis::new(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CloseObservationDisposition, ShippingCompositionError, close_observation_disposition,
        open_with_one_fresh_claim, pending_recovery_failure_disposition, recovery_next_safe_step,
        retryable_reconnect_observation,
    };
    #[cfg(not(windows))]
    use super::{ShippingDeliveryCommand, run_claim, run_delivery_command};
    use crate::operator_flow::OperatorFlowError;
    use bridge_domain::ClaimUri;
    use std::cell::Cell;

    #[test]
    fn committed_newer_generation_reopens_once_with_independent_claim()
    -> Result<(), Box<dyn std::error::Error>> {
        let old = ClaimUri::parse("profilebridge://claim/claim_original_0123456789abcdef")?;
        let fresh = ClaimUri::parse("profilebridge://claim/claim_current_0123456789abcdef")?;
        let calls = Cell::new(0);
        let issued = Cell::new(0);
        open_with_one_fresh_claim(
            &old,
            |claim| {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    assert_eq!(claim, &old);
                    Err(ShippingCompositionError::Operator(
                        OperatorFlowError::FreshAuthorizedOpenRequired,
                    ))
                } else {
                    assert_eq!(claim, &fresh);
                    Ok(())
                }
            },
            || {
                issued.set(issued.get() + 1);
                Ok(fresh.clone())
            },
        )?;
        assert_eq!(calls.get(), 2);
        assert_eq!(issued.get(), 1);
        Ok(())
    }

    #[test]
    fn reauthorization_is_never_started_for_other_failures_or_replayed_claims()
    -> Result<(), Box<dyn std::error::Error>> {
        let original = ClaimUri::parse("profilebridge://claim/claim_original_0123456789abcdef")?;
        let calls = Cell::new(0);
        let result = open_with_one_fresh_claim(
            &original,
            |_| {
                calls.set(calls.get() + 1);
                Err(ShippingCompositionError::ControlPlane)
            },
            || Err(ShippingCompositionError::ControlPlane),
        );
        assert_eq!(result, Err(ShippingCompositionError::ControlPlane));
        assert_eq!(calls.get(), 1);
        let denied = ShippingCompositionError::Initialization(
            crate::generation_initialization::InitialProfileError::Authorization,
        );
        assert_eq!(
            open_with_one_fresh_claim(
                &original,
                |_| {
                    calls.set(calls.get() + 1);
                    Err(ShippingCompositionError::Operator(
                        OperatorFlowError::FreshAuthorizedOpenRequired,
                    ))
                },
                || Err(denied),
            ),
            Err(denied)
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(
            open_with_one_fresh_claim(
                &original,
                |_| {
                    calls.set(calls.get() + 1);
                    Err(ShippingCompositionError::Operator(
                        OperatorFlowError::FreshAuthorizedOpenRequired,
                    ))
                },
                || Ok(original.clone()),
            ),
            Err(ShippingCompositionError::ControlPlane)
        );
        assert_eq!(calls.get(), 3);
        let fresh = ClaimUri::parse("profilebridge://claim/claim_current_0123456789abcdef")?;
        assert_eq!(
            open_with_one_fresh_claim(
                &original,
                |_| {
                    calls.set(calls.get() + 1);
                    Err(ShippingCompositionError::Operator(
                        OperatorFlowError::FreshAuthorizedOpenRequired,
                    ))
                },
                || Ok(fresh),
            ),
            Err(ShippingCompositionError::Operator(
                OperatorFlowError::FreshAuthorizedOpenRequired
            ))
        );
        assert_eq!(calls.get(), 5);
        Ok(())
    }

    #[test]
    fn native_oversize_recovery_reports_exact_cause_without_open_or_retry_authority() {
        use crate::operator_flow::PendingSaveRecovery;
        use crate::shipping_control_plane::ShippingControlPlaneError as Http;
        use crate::shipping_generation_successor_control::RestartExactLocalCompletionError as Restart;

        assert_eq!(
            pending_recovery_failure_disposition(Restart::NativeSnapshotTooLarge),
            PendingSaveRecovery::NativeSnapshotTooLarge
        );
        for other in [
            Restart::LocalEvidenceChanged,
            Restart::WriterOrLockUnresolved,
            Restart::PreSealTransport,
            Restart::ControlPlane(Http::HttpStatus),
        ] {
            assert_eq!(
                pending_recovery_failure_disposition(other),
                PendingSaveRecovery::Blocked
            );
        }
    }

    #[test]
    fn reconnect_observation_never_spins_on_acl_denial_or_invalid_authority() {
        use crate::shipping_control_plane::ShippingControlPlaneError as Http;
        use crate::shipping_generation_successor_control::RestartExactLocalCompletionError as Restart;

        for retryable in [Http::Transport, Http::HttpServerUncertain] {
            assert!(
                retryable_reconnect_observation(Restart::ControlPlane(retryable)),
                "only temporary network uncertainty can continue the same Save"
            );
        }
        for terminal in [Http::HttpStatus, Http::InvalidResponse, Http::Clock] {
            assert!(
                !retryable_reconnect_observation(Restart::ControlPlane(terminal)),
                "explicit rejection or bad authority is never a retry grant"
            );
        }
        for local in [
            Restart::LocalEvidenceChanged,
            Restart::NativeSnapshotTooLarge,
            Restart::PreSealTransport,
            Restart::PreSealResponseUncertain,
            Restart::PreSealRejected,
            Restart::PreSealUnavailable,
            Restart::WriterOrLockUnresolved,
            Restart::BindingOrRetirementFailed,
            Restart::UploadOrCommitUncertain,
        ] {
            assert!(!retryable_reconnect_observation(local));
        }
    }

    #[test]
    fn failed_recovery_advises_only_safe_actions_without_replay_or_stale_open() {
        use crate::shipping_control_plane::ShippingControlPlaneError as Http;
        use crate::shipping_generation_successor_control::RestartExactLocalCompletionError as Restart;

        for (failure, expected) in [
            (
                Restart::NativeSnapshotTooLarge,
                "PRESERVE_LOCAL_BYTES;NATIVE_RESCUE_REQUIRED",
            ),
            (
                Restart::PreSealTransport,
                "PRESERVE_LOCAL_BYTES;FRESH_AUTHORIZED_OBSERVATION",
            ),
            (
                Restart::PreSealResponseUncertain,
                "PRESERVE_LOCAL_BYTES;FRESH_AUTHORIZED_OBSERVATION",
            ),
            (
                Restart::UploadOrCommitUncertain,
                "PRESERVE_LOCAL_BYTES;FRESH_AUTHORIZED_OBSERVATION",
            ),
            (
                Restart::PreSealRejected,
                "PRESERVE_LOCAL_BYTES;CHECK_SERVER_AUTHORITY",
            ),
            (
                Restart::PreSealUnavailable,
                "PRESERVE_LOCAL_BYTES;CHECK_SEALING_SERVICE",
            ),
            (
                Restart::LocalEvidenceChanged,
                "PRESERVE_LOCAL_BYTES;CHECK_LOCAL_EVIDENCE",
            ),
            (
                Restart::WriterOrLockUnresolved,
                "PRESERVE_LOCAL_BYTES;CHECK_NATIVE_WRITER",
            ),
            (
                Restart::BindingOrRetirementFailed,
                "PRESERVE_LOCAL_BYTES;RECONCILE_ACTIVE_EXACT",
            ),
            (
                Restart::ControlPlane(Http::Transport),
                "PRESERVE_LOCAL_BYTES;FRESH_AUTHORIZED_OBSERVATION",
            ),
            (
                Restart::ControlPlane(Http::HttpServerUncertain),
                "PRESERVE_LOCAL_BYTES;FRESH_AUTHORIZED_OBSERVATION",
            ),
            (
                Restart::ControlPlane(Http::HttpStatus),
                "PRESERVE_LOCAL_BYTES;CHECK_SERVER_AUTHORITY",
            ),
            (
                Restart::ControlPlane(Http::LaunchLeaseConflict),
                "PRESERVE_LOCAL_BYTES;CHECK_SERVER_AUTHORITY",
            ),
            (
                Restart::ControlPlane(Http::InvalidResponse),
                "PRESERVE_LOCAL_BYTES;INSPECT_SERVER_RESPONSE",
            ),
            (
                Restart::ControlPlane(Http::Clock),
                "PRESERVE_LOCAL_BYTES;CHECK_CLOCK",
            ),
        ] {
            let step = recovery_next_safe_step(failure);
            assert_eq!(step, expected);
            assert!(step.starts_with("PRESERVE_LOCAL_BYTES;"));
            assert!(!step.contains("RETRY_OWNED_RECOVERY"));
            assert!(!step.contains("CONTINUE_OPEN"));
        }
    }

    #[test]
    fn only_verified_clean_shutdown_starts_save_and_unclean_exit_recovers() {
        assert_eq!(
            close_observation_disposition(Ok(false)),
            Ok(CloseObservationDisposition::ContinueRuntime)
        );
        assert_eq!(
            close_observation_disposition(Ok(true)),
            Ok(CloseObservationDisposition::BeginConfirmedSave)
        );
        assert_eq!(
            close_observation_disposition(Err(bridge_domain::BridgePortError::UncleanBrowserExit)),
            Ok(CloseObservationDisposition::RecoverUncleanExit)
        );
        assert_eq!(
            close_observation_disposition(Err(bridge_domain::BridgePortError::Unavailable)),
            Err(bridge_domain::BridgePortError::Unavailable)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_shipping_binary_has_no_fallback_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let claim = ClaimUri::parse("profilebridge://claim/claim_01JBRIDGE_FEASIBILITY")?;
        assert_eq!(
            run_claim(&claim),
            Err(ShippingCompositionError::UnsupportedPlatform)
        );
        assert_eq!(
            run_delivery_command(ShippingDeliveryCommand::ActivateStaged),
            Err(ShippingCompositionError::UnsupportedPlatform)
        );
        assert_eq!(
            run_delivery_command(ShippingDeliveryCommand::HandoffArrived),
            Err(ShippingCompositionError::UnsupportedPlatform)
        );
        Ok(())
    }
}
