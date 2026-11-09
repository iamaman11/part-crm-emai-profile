#![forbid(unsafe_code)]

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
    SyncUnconfirmed,
    #[cfg(windows)]
    DeliveryRuntime(crate::windows_delivery_runtime::WindowsDeliveryRuntimeError),
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
            #[cfg(windows)]
            Self::DeliveryRuntime(error) => return core::fmt::Display::fmt(error, formatter),
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
        ShippingCompositionError, ShippingDeliveryCommand,
    };
    use crate::authoritative_generation::{
        ensure_authoritative_current, reconcile_closed_draft_with_confirmed_current,
        reconcile_closed_ready_with_confirmed_current,
    };
    use crate::dirty_generation::{GenerationSealingMaterialPort, closed_current_candidate_id};
    use crate::generation_initialization::{
        InitialProfileStatus, reauthorize_current_ready_generation, sync_closed_local_draft,
    };
    use crate::generation_reopen::{
        GenerationReopenControlPort, VerifiedGenerationObjectStreamDownloader,
    };
    use crate::generation_snapshot::{
        digest_closed_workspace_streaming, seal_closed_workspace_streaming,
    };
    use crate::local_profile::{DeliveryActivationGuard, MaterializationRoot};
    use crate::shipping_control_plane::{
        ControlPlaneCoordinator, ControlPlaneEnrollment,
        SharedControlPlaneCoordinator, ShippingControlPlaneError,
    };
    use crate::shipping_generation_successor_control::publish_closed_streamed_successor;
    use crate::shipping_generation_successor_control::{
        RestartExactCommitObservation, RestartExactLocalCompletion,
        RestartExactLocalCompletionError, complete_active_exact_after_restart,
        confirm_retained_exact_retry_nonapplication, resume_exact_successor_under_fresh_lease,
        resume_lone_local_unsynced_under_fresh_lease,
    };
    use crate::user_diagnostics::report::{self, Cause, Cloud, Outcome, Stage};
    use crate::windows_delivery_handoff::{
        DeliveryHandoffCoordinator, DeliveryHandoffRestartDisposition,
    };
    use crate::windows_delivery_runtime::ActiveWindowsDeliveryRuntime;
    use crate::windows_device_application::{
        WindowsApplicationSessionHttp, WindowsDeviceApplication, WindowsDeviceApplicationBinding,
    };
    use crate::windows_generation_get::WindowsSignedGenerationObjectGet;
    use crate::windows_generation_put::WindowsSignedGenerationObjectPut;
    use application_ports::ProfileCoordinatorRuntimePort;
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
    const HEARTBEAT_TRANSPORT_RETRY_MS: u64 = 1_000;
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
        let lock = BridgeWorkspaceLock::acquire(&workspace, device, now().map_err(|_| ())?.value())
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
            let lock =
                BridgeWorkspaceLock::acquire(&workspace, device, now().map_err(|_| ())?.value())
                    .map_err(|_| ())?;
            let encrypted = (|| {
                if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
                    return Err(());
                }
                seal_closed_workspace_streaming(
                    &workspace,
                    &original_inventory,
                    &candidate,
                    Some(&base),
                    material,
                    plaintext_bytes,
                    digest,
                )
                .map_err(|_| ())
            })();
            let unlock = lock.release().map_err(|_| ());
            let encrypted = encrypted?;
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
            Ok(candidate.clone())
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

    pub fn run_closed_sync(
        intent: &crate::generation_initialization::InitialProfileOpen,
    ) -> Result<(), ShippingCompositionError> {
        // The same one-shot Bridge process retries transient offline failure
        // without requiring the native browser to open again. This has no
        // additional service, local state file, or independent cloud writer.
        const RETRY_DELAYS_SECONDS: [u64; 4] = [2, 8, 20, 45];
        let config = ShippingConfig::from_installation()?;
        let binding =
            WindowsDeviceApplicationBinding::open(&config.device_application_binding_path)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        if binding.tenant_id() != &intent.tenant_id || binding.actor_id() != &intent.actor_id {
            return Err(ShippingCompositionError::Configuration);
        }
        let root = MaterializationRoot::open_or_create(&config.materialization_root)
            .map_err(|_| ShippingCompositionError::Configuration)?;
        for (attempt, pause) in RETRY_DELAYS_SECONDS
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::once(None))
            .enumerate()
        {
            let workspace = root
                .open_profile_current(&intent.tenant_id, &intent.profile_id)
                .map_err(|_| ShippingCompositionError::SyncUnconfirmed)?;
            let dirty = workspace.path().join(".local-unsynced");
            if matches!(
                std::fs::symlink_metadata(&dirty),
                Err(ref error) if error.kind() == std::io::ErrorKind::NotFound
            ) {
                return Ok(());
            }
            // If Camouhost exited before Playwright could deliver its close
            // event, this same one-shot Sync process waits for the native OS
            // file lock to clear. It never guesses from a PID/window and never
            // needs the CRM page or a second service to trigger another try.
            wait_for_native_writer_release(
                || crate::browser_execution::browser_writer_locks_absent(&workspace),
                thread::sleep,
            )?;
            // A completed first DRAFT Commit advances the *same* local marker.
            // Retry the outstanding native edits as READY without creating a
            // second local folder or replaying the first DRAFT generation.
            let mut effective = intent.clone();
            if workspace.base_generation_id().is_some() {
                effective.status = InitialProfileStatus::Ready;
                effective.expected_version = workspace.profile_version();
            }
            let outcome =
                sync_after_native_close(&effective, &binding, &root, &config.control_plane_origin);
            if outcome.is_ok()
                && matches!(
                    std::fs::symlink_metadata(&dirty),
                    Err(ref error) if error.kind() == std::io::ErrorKind::NotFound
                )
            {
                return Ok(());
            }
            if let Some(seconds) = pause {
                eprintln!("BRIDGE_CLOUD_SYNC_RETRY_PENDING attempt={}", attempt + 1);
                thread::sleep(Duration::from_secs(seconds));
            }
        }
        // A non-confirmed cloud effect never changes the local success state.
        // The original dirty marker survives for an explicit independent
        // existing Bridge startup/recovery or an explicit --sync-closed retry;
        // neither requires a browser window.
        Err(ShippingCompositionError::SyncUnconfirmed)
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

    pub fn run_initial(
        intent: &crate::generation_initialization::InitialProfileOpen,
    ) -> Result<(), ShippingCompositionError> {
        // The CRM web page already supplied the selected profile. Opening the
        // browser is now a local filesystem/process operation, not an R2
        // initialization or a Coordinator lease. Sync belongs to a different
        // operation and cannot block a local Firefox window.
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
        match crate::local_browser_open::open(
            &root,
            intent,
            binding.device_id(),
            delivery.runtime_root(),
        ) {
            Ok(()) => {
                start_sync_after_close(intent);
                Ok(())
            }
            Err(error @ crate::local_browser_open::LocalOpenError::BrowserNotClosedCleanly) => {
                // The adapter may have died before reporting the official
                // Playwright close event. Keep .local-unsynced and launch the
                // same file-gated Sync/recovery anyway: it must not snapshot
                // while Firefox still owns the folder. CRM is not the retry
                // trigger, and the browser is never closed by this path.
                start_sync_after_close(intent);
                Err(ShippingCompositionError::LocalOpen(error))
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
                match crate::local_browser_open::open(
                    &root,
                    intent,
                    &device,
                    delivery.runtime_root(),
                ) {
                    Ok(()) => {
                        start_sync_after_close(intent);
                        Ok(())
                    }
                    Err(
                        error @ crate::local_browser_open::LocalOpenError::BrowserNotClosedCleanly,
                    ) => {
                        // Adapter failure is not proof that Firefox released
                        // its files. The existing Sync owner must be started;
                        // its native file-lock gate decides whether snapshot
                        // can proceed and retains dirty evidence otherwise.
                        start_sync_after_close(intent);
                        Err(ShippingCompositionError::LocalOpen(error))
                    }
                    Err(error) => Err(ShippingCompositionError::LocalOpen(error)),
                }
            }
            Err(error) => Err(ShippingCompositionError::LocalOpen(error)),
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
}


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
#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn non_windows_shipping_binary_has_no_browser_or_sync_fallback() {
        use super::{ShippingCompositionError, run_closed_profile_sync};
        use crate::generation_initialization::InitialProfileOpen;
        let _sync_entrypoint = run_closed_profile_sync;
        let _error_type = core::mem::size_of::<ShippingCompositionError>();
        let _intent_type = core::mem::size_of::<InitialProfileOpen>();
    }
}
