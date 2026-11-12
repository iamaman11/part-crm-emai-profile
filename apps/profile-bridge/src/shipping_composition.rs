#![forbid(unsafe_code)]

/// Existing one-shot installation and activation commands; not browser Open or Sync.

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
    SyncUnconfirmed,
    LocalDelete(crate::local_profile_delete::TerminalDeleteError),
    #[cfg(windows)]
    DeliveryRuntime(crate::windows_delivery_runtime::WindowsDeliveryRuntimeError),
    // Diagnostic-only historical outcomes; the v2 Open/Sync path cannot construct them.
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
            Self::SyncUnconfirmed => "Native Close succeeded; cloud Sync not confirmed",
            Self::LocalDelete(error) => return core::fmt::Display::fmt(error, formatter),
            #[cfg(windows)]
            Self::DeliveryRuntime(error) => return core::fmt::Display::fmt(error, formatter),
            Self::Operator(_) => "retired Profile Bridge operator outcome",
            Self::ActiveSessionMissing => "retired Profile Bridge active session outcome",
            Self::CloseObservation(_) => "retired Profile Bridge close observation outcome",
            Self::CommittedRecoveryRequired => "retired Profile Bridge committed recovery outcome",
            Self::HeartbeatAuthorityUncertain => "retired Profile Bridge heartbeat outcome",
            Self::Initialization(_) => "profile first-open preparation failed",
            #[cfg(windows)]
            Self::LocalOpen(error) => return core::fmt::Display::fmt(error, formatter),
        })
    }
}

impl std::error::Error for ShippingCompositionError {}

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

/* Internal one-shot invocation of the SAME installed Bridge executable.
 * Only the cloud Sync owner runs here; native Camoufox is never relaunched. */
pub fn run_closed_profile_sync(
    intent: &crate::generation_initialization::InitialProfileOpen,
) -> Result<(), ShippingCompositionError> {
    #[cfg(windows)]
    {
        windows::run_closed_sync(intent)
    }
    #[cfg(not(windows))]
    {
        let _ = intent;
        Err(ShippingCompositionError::UnsupportedPlatform)
    }
}

/// Only an authenticated terminal D1 readback permits the existing file owner
/// to remove exact closed local CURRENT. No background GC or second process.
pub fn run_terminal_profile_delete(
    intent: &crate::local_profile_delete::TerminalProfileDelete,
) -> Result<(), ShippingCompositionError> {
    #[cfg(windows)]
    {
        windows::run_terminal_delete(intent)
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
    use super::{ShippingCompositionError, ShippingDeliveryCommand};
    use crate::authoritative_generation::{
        ensure_authoritative_current, prove_cold_current_restore,
        reconcile_closed_draft_with_confirmed_current,
        reconcile_closed_ready_with_confirmed_current,
    };
    use crate::dirty_generation::{GenerationSealingMaterialPort, closed_current_candidate_id};
    use crate::generation_initialization::{
        InitialProfileStatus, finish_pending_sync_under_writer,
        reauthorize_current_ready_generation, sync_closed_local_draft,
    };
    use crate::generation_reopen::{
        GenerationReopenControlPort, VerifiedGenerationObjectStreamDownloader,
    };
    use crate::generation_snapshot::{
        digest_closed_workspace_streaming, seal_closed_workspace_streaming,
    };
    use crate::local_profile::{
        DeliveryActivationGuard, MaterializationRoot, PendingSyncDescriptor,
    };
    use crate::shipping_control_plane::{
        ControlPlaneCoordinator, ControlPlaneEnrollment, SharedControlPlaneCoordinator,
        ShippingControlPlaneError,
    };
    use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort, SyncEnrollmentPort};
    use crate::shipping_generation_successor_control::publish_closed_streamed_successor;
    use crate::windows_delivery_handoff::{
        DeliveryHandoffCoordinator, DeliveryHandoffRestartDisposition,
    };
    use crate::windows_delivery_runtime::ActiveWindowsDeliveryRuntime;
    use crate::windows_device_application::{
        WindowsApplicationSessionHttp, WindowsDeviceApplication, WindowsDeviceApplicationBinding,
    };
    use crate::windows_generation_get::WindowsSignedGenerationObjectGet;
    use crate::windows_generation_put::WindowsSignedGenerationObjectPut;
    use application_ports::{ProfileCoordinatorPort, ProfileCoordinatorRuntimePort};
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
    const HEARTBEAT_TRANSPORT_RETRY_MS: u64 = 1_000;
    const SAVE_RECONNECT_AUTHORITY_RESERVE_MS: u64 = 5_000;
    const SAVE_HEARTBEAT_PERIOD_MS: u64 = 5_000;

    /// A scoped helper for the existing one-shot Sync's long native-seal/R2-PUT path.
    /// It shares that Sync's coordinator cursor; it creates no second writer or
    /// persisted state. Dropping the scope joins the heartbeat before exit.
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
    fn pending_descriptor_matches_current(
        pending: &PendingSyncDescriptor,
        current: &crate::generation_reopen::GenerationDownloadCapability,
    ) -> bool {
        current.generation_id() == pending.candidate_generation_id()
            && current.metadata_digest() == *pending.metadata_digest()
            && current.container_digest() == *pending.container_digest()
            && current.container_bytes() == pending.container_bytes()
    }

    fn sync_closed_ready(
        intent: &crate::generation_initialization::InitialProfileOpen,
        binding: &WindowsDeviceApplicationBinding,
        root: &MaterializationRoot,
        origin: &str,
    ) -> Result<(), ()> {
        use crate::browser_execution::browser_writer_locks_absent;
        use crate::local_profile::BridgeWorkspaceLock;
        use std::fs;

        let mut workspace = root
            .open_profile_current(&intent.tenant_id, &intent.profile_id)
            .map_err(|_| ())?;
        if let Some(pending) = workspace.pending_sync().cloned() {
            let reader_app =
                WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
                    .map_err(|_| ())?;
            let mut read_only =
                ControlPlaneCoordinator::for_local_current_restore(reader_app.transport());
            let current = read_only
                .download_capability(&intent.tenant_id, &intent.profile_id)
                .map_err(|_| ())?;
            if pending_descriptor_matches_current(&pending, &current) {
                let guard = BridgeWorkspaceLock::acquire_or_recover_interrupted(
                    &workspace,
                    binding.device_id(),
                    now().map_err(|_| ())?.value(),
                )
                .map_err(|_| ())?;
                let active = root
                    .open_profile_current(&intent.tenant_id, &intent.profile_id)
                    .map_err(|_| ())?;
                let applied = (|| {
                    if active.pending_sync() != Some(&pending) {
                        return Err(());
                    }
                    finish_pending_sync_under_writer(root, &active, &pending).map_err(|_| ())
                })();
                let unlocked = guard.release().map_err(|_| ());
                let unchanged = applied?;
                unlocked?;
                return if unchanged { Ok(()) } else { Err(()) };
            }
            if !pending.is_committed()
                && workspace
                    .base_generation_id()
                    .is_some_and(|base| current.generation_id() == base)
            {
                let guard = BridgeWorkspaceLock::acquire_or_recover_interrupted(
                    &workspace,
                    binding.device_id(),
                    now().map_err(|_| ())?.value(),
                )
                .map_err(|_| ())?;
                let active = root
                    .open_profile_current(&intent.tenant_id, &intent.profile_id)
                    .map_err(|_| ())?;
                let cleared = (|| {
                    if active.pending_sync() != Some(&pending)
                        || !matches!(browser_writer_locks_absent(&active), Ok(true))
                    {
                        return Err(());
                    }
                    root.clear_profile_current_pending_sync(&active)
                        .map_err(|_| ())
                })();
                let unlocked = guard.release().map_err(|_| ());
                cleared?;
                unlocked?;
                workspace = root
                    .open_profile_current(&intent.tenant_id, &intent.profile_id)
                    .map_err(|_| ())?;
            } else {
                return Err(());
            }
        }
        let Some(base) = workspace.base_generation_id().cloned() else {
            // First DRAFT -> READY committed in D1, but our HTTP response or
            // local receipt update was lost. Admit only the expected single
            // Profile-version increment and exact native-byte equality with
            // the verified, server-selected CURRENT. No overwrite or N+2.
            if workspace.profile_version().checked_add(1) != Some(intent.expected_version)
                || fs::read(workspace.path().join(".local-unsynced")).map_err(|_| ())?
                    != b"local-unsynced-v2"
            {
                return Err(());
            }
            let application =
                WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
                    .map_err(|_| ())?;
            let mut control =
                ControlPlaneCoordinator::for_local_current_restore(application.transport());
            let confirmed = control
                .download_capability(&intent.tenant_id, &intent.profile_id)
                .map_err(|_| ())?;
            if confirmed.generation_id() != workspace.generation_id() {
                return Err(());
            }
            let reader = WindowsSignedGenerationObjectGet::from_system().map_err(|_| ())?;
            let mut downloader = VerifiedGenerationObjectStreamDownloader::new(reader);
            return reconcile_closed_draft_with_confirmed_current(
                root,
                &intent.tenant_id,
                &intent.profile_id,
                confirmed.generation_id(),
                intent.expected_version,
                binding.device_id(),
                now().map_err(|_| ())?.value(),
                &mut control,
                &mut downloader,
            )
            .map_err(|_| ());
        };
        if workspace.profile_version() != intent.expected_version
            || fs::read(workspace.path().join(".local-unsynced")).map_err(|_| ())?
                != b"local-unsynced-v2"
        {
            return Err(());
        }
        let device = binding.device_id();
        let lock = BridgeWorkspaceLock::acquire_or_recover_interrupted(
            &workspace,
            device,
            now().map_err(|_| ())?.value(),
        )
        .map_err(|_| ())?;
        let captured = (|| {
            if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
                return Err(());
            }
            let inventory = workspace.inventory().map_err(|_| ())?;
            let (bytes, digest) =
                digest_closed_workspace_streaming(&workspace, &inventory).map_err(|_| ())?;
            Ok((inventory, bytes, digest))
        })();
        let unlock = lock.release().map_err(|_| ());
        let (original_inventory, plaintext_bytes, digest) = captured?;
        unlock?;
        let candidate =
            closed_current_candidate_id(&intent.tenant_id, &intent.profile_id, &base, digest)
                .map_err(|_| ())?;

        // A lost READY N+1 Commit reply cannot authorize a fresh N+2.
        // Read exact server-selected CURRENT first; a newer candidate must
        // match the ORIGINAL closed local bytes before the local marker moves.
        let reader_app = WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
            .map_err(|_| ())?;
        let mut read_only =
            ControlPlaneCoordinator::for_local_current_restore(reader_app.transport());
        let current = read_only
            .download_capability(&intent.tenant_id, &intent.profile_id)
            .map_err(|_| ())?;
        if current.generation_id() == &candidate {
            let reader = WindowsSignedGenerationObjectGet::from_system().map_err(|_| ())?;
            let mut downloader = VerifiedGenerationObjectStreamDownloader::new(reader);
            return reconcile_closed_ready_with_confirmed_current(
                root,
                &intent.tenant_id,
                &intent.profile_id,
                &candidate,
                &base,
                intent.expected_version.checked_add(1).ok_or(())?,
                binding.device_id(),
                now().map_err(|_| ())?.value(),
                &mut read_only,
                &mut downloader,
            )
            .map_err(|_| ());
        }
        if current.generation_id() != &base {
            return Err(());
        }

        let application = WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
            .map_err(|_| ())?;
        let mut transport = application.transport();
        let actor = profile_platform_primitives::ActorContext::new(
            profile_platform_primitives::TenantScope::new(intent.tenant_id.clone()),
            intent.actor_id.clone(),
            profile_platform_primitives::CorrelationId::parse("corr_closed_ready_sync")
                .map_err(|_| ())?,
        );
        // Sync-only fresh Coordinator witness. Native Open uses no lease.
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
        let heartbeat = RetainedSaveHeartbeat::start(coordinator.clone(), lease.clone());
        let outcome = (|| {
            let mut sealer = coordinator.clone();
            let material = sealer
                .material_for(
                    &intent.tenant_id,
                    &intent.profile_id,
                    &base,
                    &candidate,
                    digest,
                )
                .map_err(|_| ())?;
            // Network/key service does not hold the local admission. Only the
            // ORIGINAL closed folder is read for the streaming BPGC seal.
            let lock = BridgeWorkspaceLock::acquire_or_recover_interrupted(
                &workspace,
                device,
                now().map_err(|_| ())?.value(),
            )
            .map_err(|_| ())?;
            let encrypted = (|| {
                if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
                    return Err(());
                }
                let encrypted = seal_closed_workspace_streaming(
                    &workspace,
                    &original_inventory,
                    &candidate,
                    Some(&base),
                    material,
                    plaintext_bytes,
                    digest,
                )
                .map_err(|_| ())?;
                let pending = PendingSyncDescriptor::new(
                    candidate.clone(),
                    encrypted.metadata_digest(),
                    encrypted.container_digest(),
                    encrypted.container_bytes(),
                    digest,
                    plaintext_bytes,
                )
                .map_err(|_| ())?;
                root.set_profile_current_pending_sync(&workspace, pending.clone())
                    .map_err(|_| ())?;
                Ok((encrypted, pending))
            })();
            let unlock = lock.release().map_err(|_| ());
            let (encrypted, pending) = encrypted?;
            unlock?;
            if !heartbeat.is_healthy() {
                return Err(());
            }
            let mut upload = WindowsSignedGenerationObjectPut::from_system().map_err(|_| ())?;
            publish_closed_streamed_successor(
                &intent.tenant_id,
                &intent.profile_id,
                &base,
                &candidate,
                &encrypted,
                &lease,
                &mut transport,
                &mut upload,
            )
            .map_err(|_| ())?;
            if !heartbeat.is_healthy() {
                return Err(());
            }
            Ok((candidate.clone(), pending))
        })();
        drop(heartbeat);
        if outcome.is_ok() {
            coordinator.close_confirmed_save(&lease).map_err(|_| ())?;
        } else {
            let _ = coordinator.close_lease(&lease);
        }
        let (committed, pending) = outcome?;
        if &committed != pending.candidate_generation_id() {
            return Err(());
        }

        // The exact encrypted descriptor was durably checkpointed before
        // upload/Commit. Reconcile it without replacing a reopened profile.
        let active = root
            .open_profile_current(&intent.tenant_id, &intent.profile_id)
            .map_err(|_| ())?;
        let guard = BridgeWorkspaceLock::acquire_or_recover_interrupted(
            &active,
            device,
            now().map_err(|_| ())?.value(),
        )
        .map_err(|_| ())?;
        let applied = (|| {
            if active.base_generation_id() != Some(&base)
                || active.profile_version() != intent.expected_version
                || active.pending_sync() != Some(&pending)
                || !matches!(browser_writer_locks_absent(&active), Ok(true))
            {
                return Err(());
            }
            finish_pending_sync_under_writer(root, &active, &pending).map_err(|_| ())
        })();
        let unlocked = guard.release().map_err(|_| ());
        let unchanged = applied?;
        unlocked?;
        if !unchanged {
            return Err(());
        }
        Ok(())
    }

    /// The SAME post-close handler runs after normal Open and READY restore.
    /// Neither Firefox closure nor its local bytes depend on the cloud result.
    fn sync_after_native_close(
        intent: &crate::generation_initialization::InitialProfileOpen,
        binding: &WindowsDeviceApplicationBinding,
        root: &MaterializationRoot,
        origin: &str,
    ) -> Result<(), ShippingCompositionError> {
        if intent.status == InitialProfileStatus::Ready {
            sync_closed_ready(intent, binding, root, origin)
                .map_err(|_| ShippingCompositionError::SyncUnconfirmed)
        } else {
            let application =
                WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
                    .map_err(|_| ShippingCompositionError::SyncUnconfirmed)?;
            let mut transport = application.transport();
            let mut uploader = WindowsSignedGenerationObjectPut::from_system()
                .map_err(|_| ShippingCompositionError::SyncUnconfirmed)?;
            let actor = profile_platform_primitives::ActorContext::new(
                profile_platform_primitives::TenantScope::new(intent.tenant_id.clone()),
                intent.actor_id.clone(),
                profile_platform_primitives::CorrelationId::parse("corr_local_draft_sync")
                    .map_err(|_| ShippingCompositionError::SyncUnconfirmed)?,
            );
            match sync_closed_local_draft(
                intent,
                &actor,
                binding.device_id(),
                root,
                &mut transport,
                &mut uploader,
            ) {
                Ok(()) => Ok(()),
                Err(error) => {
                    // The first exact D1 Commit may have succeeded while its
                    // HTTP reply was lost. ONLY a verified server-selected
                    // CURRENT matching the original local native bytes may
                    // reconcile it; no blind second PUT or generation is made.
                    let mut proof = intent.clone();
                    proof.status = InitialProfileStatus::Ready;
                    proof.expected_version = proof
                        .expected_version
                        .checked_add(1)
                        .ok_or(ShippingCompositionError::Initialization(error))?;
                    sync_closed_ready(&proof, binding, root, origin)
                        .map_err(|_| ShippingCompositionError::Initialization(error))
                }
            }
        }
    }

    fn wait_for_native_writer_release<E>(
        mut probe: impl FnMut() -> Result<bool, E>,
        mut pause: impl FnMut(Duration),
    ) -> Result<(), ShippingCompositionError> {
        let mut delay_seconds = 1_u64;
        loop {
            match probe() {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    eprintln!("BRIDGE_SYNC_WAITING_FOR_NATIVE_FILE_RELEASE");
                    pause(Duration::from_secs(delay_seconds));
                    delay_seconds = delay_seconds.saturating_mul(2).min(15);
                }
                Err(_) => return Err(ShippingCompositionError::SyncUnconfirmed),
            }
        }
    }

    /// Existing post-close Sync owner performs the only cold Restore proof.
    /// No new local state, worker, browser controller or independent RETIRE API.
    /// This operation can be retried against the same already-confirmed CURRENT.
    fn try_admit_confirmed_current_retirement(
        intent: &crate::generation_initialization::InitialProfileOpen,
        binding: &WindowsDeviceApplicationBinding,
        root: &MaterializationRoot,
        latest: &crate::local_profile::GenerationWorkspace,
        origin: &str,
    ) -> Result<bool, ()> {
        let Some(current) = latest.base_generation_id() else {
            return Ok(false);
        };
        let cold = (|| {
            let reader_app =
                WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
                    .map_err(|_| ())?;
            let mut remote =
                ControlPlaneCoordinator::for_local_current_restore(reader_app.transport());
            let reader = WindowsSignedGenerationObjectGet::from_system().map_err(|_| ())?;
            let mut downloader = VerifiedGenerationObjectStreamDownloader::new(reader);
            let (restored_digest, prelude) = prove_cold_current_restore(
                &root,
                &intent.tenant_id,
                &intent.profile_id,
                current,
                latest.profile_version(),
                binding.device_id(),
                now().map_err(|_| ())?.value(),
                &mut remote,
                &mut downloader,
            )
            .map_err(|_| ())?;
            let restored_native_sha256 = restored_digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let metadata_prelude_hex = prelude
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let body =
                control_plane_contract::profile_generation_api::BridgeCurrentColdRetirementRequest {
                    current_generation_id: current.as_str().to_owned(),
                    expected_profile_version: latest.profile_version(),
                    metadata_prelude_hex,
                    restored_native_sha256,
                };
            let encoded = serde_json::to_vec(&body).map_err(|_| ())?;
            let path = control_plane_contract::profile_generation_api::BRIDGE_PROFILE_CURRENT_COLD_RETIREMENT_PATH_TEMPLATE
                .replace("{tenantId}", intent.tenant_id.as_str())
                .replace("{profileId}", intent.profile_id.as_str());
            let correlation =
                profile_platform_primitives::CorrelationId::parse("corr_cold_current_retire")
                    .map_err(|_| ())?;
            let application =
                WindowsDeviceApplication::from_system(origin.to_owned(), binding.clone())
                    .map_err(|_| ())?;
            let mut transport = application.transport();
            let response = transport
                .request(
                    MachineHttpMethod::PostJson,
                    path.as_str(),
                    &correlation,
                    Some(&encoded),
                )
                .map_err(|_| ())?;
            if !(200..300).contains(&response.status()) {
                return Err(());
            }
            let admitted = serde_json::from_slice::<serde_json::Value>(response.body())
                .map_err(|_| ())?
                .get("admitted")
                .and_then(serde_json::Value::as_bool);
            Ok(admitted)
        })();
        cold.and_then(|admitted| admitted.ok_or(()))
    }

    pub fn run_terminal_delete(
        intent: &crate::local_profile_delete::TerminalProfileDelete,
    ) -> Result<(), ShippingCompositionError> {
        let config = ShippingConfig::from_installation()?;
        let binding = WindowsDeviceApplicationBinding::open(
            &config.device_application_binding_path,
        ).map_err(|_| ShippingCompositionError::Configuration)?;
        if binding.tenant_id() != &intent.tenant_id || binding.actor_id() != &intent.actor_id {
            return Err(ShippingCompositionError::LocalDelete(
                crate::local_profile_delete::TerminalDeleteError::AuthorizationUnconfirmed,
            ));
        }
        let application = WindowsDeviceApplication::from_system(
            config.control_plane_origin,
            binding,
        ).map_err(|_| ShippingCompositionError::LocalDelete(
            crate::local_profile_delete::TerminalDeleteError::AuthorizationUnconfirmed,
        ))?;
        let mut transport = application.transport();
        crate::local_profile_delete::confirm_terminal_delete(intent, &mut transport)
            .map_err(ShippingCompositionError::LocalDelete)?;
        let root = MaterializationRoot::open_or_create(&config.materialization_root)
            .map_err(|_| ShippingCompositionError::LocalDelete(
                crate::local_profile_delete::TerminalDeleteError::LocalFilesystem,
            ))?;
        crate::local_profile_delete::cleanup_confirmed_current(&root, intent)
            .map_err(ShippingCompositionError::LocalDelete)
    }

    pub fn run_closed_sync(
        intent: &crate::generation_initialization::InitialProfileOpen,
    ) -> Result<(), ShippingCompositionError> {
        // This same one-shot Bridge process remains alive until the existing
        // dirty marker is cleared. Backoff is bounded; attempts are not.
        // No daemon, local retry file, or independent cloud writer is added.
        let config = ShippingConfig::from_installation()?;
        let binding =
            WindowsDeviceApplicationBinding::open(&config.device_application_binding_path)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        if binding.tenant_id() != &intent.tenant_id || binding.actor_id() != &intent.actor_id {
            return Err(ShippingCompositionError::Configuration);
        }
        let root = MaterializationRoot::open_or_create(&config.materialization_root)
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let mut retry_delay_seconds = 2_u64;
        let mut attempt = 0_u64;
        let mut retirement_attempts = 0_u8;
        loop {
            let workspace = root
                .open_profile_current(&intent.tenant_id, &intent.profile_id)
                .map_err(|_| ShippingCompositionError::SyncUnconfirmed)?;
            let dirty = workspace.path().join(".local-unsynced");
            if matches!(
                std::fs::symlink_metadata(&dirty),
                Err(ref error) if error.kind() == std::io::ErrorKind::NotFound
            ) && workspace.pending_sync().is_none()
            {
                match try_admit_confirmed_current_retirement(
                    intent,
                    &binding,
                    &root,
                    &workspace,
                    &config.control_plane_origin,
                ) {
                    Ok(true) => {
                        eprintln!("BRIDGE_COLD_CURRENT_RETIREMENT=D1_ADMITTED");
                        return Ok(());
                    }
                    Ok(false) => {
                        eprintln!("BRIDGE_COLD_CURRENT_RESTORE=VERIFIED_NO_PREDECESSOR");
                        return Ok(());
                    }
                    Err(_) if retirement_attempts < 2 => {
                        // Retry a failed exact GET or lost D1 reply WITHOUT a
                        // new R2 PUT or a new cloud generation.
                        retirement_attempts += 1;
                        thread::sleep(Duration::from_secs(retry_delay_seconds));
                        retry_delay_seconds = next_sync_retry_delay(retry_delay_seconds);
                        continue;
                    }
                    Err(_) => {
                        // Sync is confirmed; never mark it failed or create
                        // another generation to compensate for R2 retention.
                        // A later normal Close retries this same clean path.
                        eprintln!("BRIDGE_COLD_CURRENT_RETIREMENT=UNCONFIRMED_R2_RETAINED");
                        return Ok(());
                    }
                }
            }
            // If Camouhost exited before Playwright could deliver its close
            // event, this same process waits for the native OS file lock to
            // clear. It never guesses from a PID/window or needs CRM to retry.
            wait_for_native_writer_release(
                || crate::browser_execution::browser_writer_locks_absent(&workspace),
                thread::sleep,
            )?;
            // A completed first DRAFT Commit advances the same local marker.
            // Retry outstanding native edits as READY without creating a
            // second local folder or replaying the first DRAFT generation.
            let mut effective = intent.clone();
            if workspace.base_generation_id().is_some() {
                effective.status = InitialProfileStatus::Ready;
                effective.expected_version = workspace.profile_version();
            }
            let outcome =
                sync_after_native_close(&effective, &binding, &root, &config.control_plane_origin);
            if outcome.is_ok() {
                let latest = root
                    .open_profile_current(&intent.tenant_id, &intent.profile_id)
                    .map_err(|_| ShippingCompositionError::SyncUnconfirmed)?;
                if matches!(
                    std::fs::symlink_metadata(&dirty),
                    Err(ref error) if error.kind() == std::io::ErrorKind::NotFound
                ) && latest.pending_sync().is_none()
                {
                    // The cloud Save is complete. The same loop re-enters its
                    // clean branch for the read-only CURRENT Restore proof;
                    // never seal or upload again merely to retry retirement.
                    continue;
                }
            }
            eprintln!(
                "BRIDGE_CLOUD_SYNC_RETRY_PENDING attempt={}",
                attempt.saturating_add(1)
            );
            thread::sleep(Duration::from_secs(retry_delay_seconds));
            retry_delay_seconds = next_sync_retry_delay(retry_delay_seconds);
            attempt = attempt.saturating_add(1);
        }
    }

    fn next_sync_retry_delay(current_seconds: u64) -> u64 {
        current_seconds.saturating_mul(2).min(300)
    }

    /// The same installed executable handles one ephemeral cloud Sync
    /// invocation. No daemon, extra binary or new browser writer is created.
    /// A failed child start falls back to the same Sync inline.
    fn start_sync_after_close(intent: &crate::generation_initialization::InitialProfileOpen) {
        use std::process::{Command, Stdio};
        let status = match intent.status {
            InitialProfileStatus::Draft => "DRAFT",
            InitialProfileStatus::Ready => "READY",
        };
        let uri = format!(
            "profilebridge://open/v2/{}/{}/{}/{}/{}",
            intent.tenant_id.as_str(),
            intent.actor_id.as_str(),
            intent.profile_id.as_str(),
            intent.expected_version,
            status,
        );
        let result = env::current_exe().and_then(|exe| {
            Command::new(exe)
                .arg("--sync-closed")
                .arg(uri)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        });
        if result.is_err() {
            eprintln!("BRIDGE_SYNC_NOT_STARTED;FALLBACK=RUN_EXISTING_SYNC_INLINE");
            if let Err(error) = run_closed_sync(intent) {
                eprintln!(
                    "BRIDGE_SYNC_INLINE_FALLBACK_PENDING;error={error:?};NEXT_SAFE_STEP=PRESERVE_LOCAL_BYTES"
                );
            }
        }
    }

    #[cfg(test)]
    mod native_writer_wait_tests {
        use super::{ShippingCompositionError, wait_for_native_writer_release};
        use std::time::Duration;

        #[test]
        fn same_sync_waits_for_native_file_release_with_backoff() {
            let mut probes = [Ok::<bool, ()>(false), Ok(false), Ok(true)].into_iter();
            let mut pauses = Vec::new();
            wait_for_native_writer_release(
                || probes.next().expect("three native lock probes"),
                |delay| pauses.push(delay),
            )
            .expect("released native file lock");
            assert_eq!(pauses, vec![Duration::from_secs(1), Duration::from_secs(2)]);
        }

        #[test]
        fn unknown_native_file_state_fails_closed_without_waiting() {
            let mut pauses = Vec::new();
            let result = wait_for_native_writer_release(
                || Err("native file state unavailable"),
                |delay| pauses.push(delay),
            );
            assert!(matches!(
                result,
                Err(ShippingCompositionError::SyncUnconfirmed)
            ));
            assert!(pauses.is_empty());
        }
    }

    #[cfg(test)]
    mod pending_sync_descriptor_tests {
        use super::pending_descriptor_matches_current;
        use crate::generation_reopen::GenerationDownloadCapability;
        use crate::local_profile::PendingSyncDescriptor;
        use profile_platform_primitives::GenerationId;

        #[test]
        fn pending_checkpoint_matches_only_the_exact_worker_current_descriptor()
        -> Result<(), Box<dyn std::error::Error>> {
            let candidate = GenerationId::parse("generation_pending_descriptor")?;
            let pending = PendingSyncDescriptor::new(
                candidate.clone(),
                [0x11; 32],
                [0x22; 32],
                4_096,
                [0x33; 32],
                2_048,
            )?;
            let exact = GenerationDownloadCapability::new(
                candidate.clone(),
                "tenant/profile/generation_pending_descriptor.bpgc".to_owned(),
                [0x11; 32],
                [0x22; 32],
                4_096,
                "https://example.invalid/signed".to_owned(),
                60,
            );
            assert!(pending_descriptor_matches_current(&pending, &exact));

            let wrong_digest = GenerationDownloadCapability::new(
                candidate.clone(),
                "tenant/profile/generation_pending_descriptor.bpgc".to_owned(),
                [0x12; 32],
                [0x22; 32],
                4_096,
                "https://example.invalid/signed".to_owned(),
                60,
            );
            assert!(!pending_descriptor_matches_current(&pending, &wrong_digest));

            let wrong_size = GenerationDownloadCapability::new(
                candidate,
                "tenant/profile/generation_pending_descriptor.bpgc".to_owned(),
                [0x11; 32],
                [0x22; 32],
                4_097,
                "https://example.invalid/signed".to_owned(),
                60,
            );
            assert!(!pending_descriptor_matches_current(&pending, &wrong_size));
            Ok(())
        }
    }

    fn delivery_health_before_launch(
        pending: Option<crate::windows_delivery_runtime::PendingWindowsDeliveryHealthConfirmation>,
    ) -> Option<crate::local_browser_open::BeforeLaunch> {
        pending.map(|pending| {
            Box::new(move || {
                let started = pending.start_before_runtime_health().map_err(|_| ())?;
                let ready: crate::local_browser_open::ReadyConfirmation =
                    Box::new(move || started.confirm_after_runtime_ready().is_ok());
                Ok(Some(ready))
            }) as crate::local_browser_open::BeforeLaunch
        })
    }

    pub fn run_initial(
        intent: &crate::generation_initialization::InitialProfileOpen,
    ) -> Result<(), ShippingCompositionError> {
        // The CRM web page already supplied the selected profile. Opening the
        // browser is a local filesystem/process operation; Sync remains separate.
        if recover_delivery_before_open()? {
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

        let open_local = || {
            crate::local_browser_open::open_with_before_launch(
                &root,
                intent,
                binding.device_id(),
                delivery.runtime_root(),
                delivery_health_before_launch(delivery.pending_health_confirmation()),
            )
        };
        let result = match open_local() {
            Err(crate::local_browser_open::LocalOpenError::RestoreRequired) => {
                // Only a missing READY local folder crosses the cloud boundary.
                let device = binding.device_id().clone();
                let application = WindowsDeviceApplication::from_system(
                    config.control_plane_origin.clone(),
                    binding.clone(),
                )
                .map_err(|_| ShippingCompositionError::RestoreUnavailable)?;
                let mut remote =
                    ControlPlaneCoordinator::for_local_current_restore(application.transport());
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
                // Re-enter the same local Open with the same runtime and owners.
                open_local()
            }
            result => result,
        };

        match result {
            Ok(()) => {
                start_sync_after_close(intent);
                Ok(())
            }
            Err(error) => {
                // One error path owns the existing Sync handoff. Stale CRM
                // intents alone must not publish conflicting local bytes.
                if error != crate::local_browser_open::LocalOpenError::StaleLocalWorkspace {
                    start_sync_after_close(intent);
                }
                Err(ShippingCompositionError::LocalOpen(error))
            }
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

    fn recover_delivery_before_open() -> Result<bool, ShippingCompositionError> {
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

    // Keep the existing coordinator heartbeat alive only while the
    // same confirmed lease has a safe native-save completion window.
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

    fn now() -> Result<UnixMillis, ShippingCompositionError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ShippingCompositionError::Clock)?
            .as_millis();
        let millis = u64::try_from(millis).map_err(|_| ShippingCompositionError::Clock)?;
        Ok(UnixMillis::new(millis))
    }

    #[cfg(test)]
    mod closed_sync_retry_tests {
        use super::next_sync_retry_delay;

        #[test]
        fn closed_sync_retry_backoff_is_capped_without_limiting_attempts() {
            let mut delay = 2_u64;
            let mut observed = Vec::new();
            for _ in 0..10 {
                observed.push(delay);
                delay = next_sync_retry_delay(delay);
            }
            assert_eq!(observed, vec![2, 4, 8, 16, 32, 64, 128, 256, 300, 300]);
            assert_eq!(next_sync_retry_delay(300), 300);
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn non_windows_shipping_binary_has_no_browser_or_sync_fallback()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{ShippingCompositionError, run_closed_profile_sync, run_initial_profile};
        use crate::generation_initialization::{InitialProfileOpen, InitialProfileStatus};
        use profile_platform_primitives::{ActorId, ProfileId, TenantId};

        let intent = InitialProfileOpen {
            tenant_id: TenantId::parse("tenant_01JLOCAL")?,
            actor_id: ActorId::parse("actor_01JLOCAL")?,
            profile_id: ProfileId::parse("profile_01JLOCAL")?,
            expected_version: 1,
            status: InitialProfileStatus::Draft,
        };
        assert!(matches!(
            run_initial_profile(&intent),
            Err(ShippingCompositionError::UnsupportedPlatform)
        ));
        assert!(matches!(
            run_closed_profile_sync(&intent),
            Err(ShippingCompositionError::UnsupportedPlatform)
        ));
        Ok(())
    }
}
