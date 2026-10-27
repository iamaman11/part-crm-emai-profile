#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;

/// Internal one-shot maintenance command, never a normal Profile Open.
/// It requires a fresh ordinary device-bound one-time launch claim.
pub const VERIFY_HISTORICAL_CANDIDATE_ARGUMENT: &str = "--verify-historical-candidate";

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
    Operator(crate::operator_flow::OperatorFlowError),
    ActiveSessionMissing,
    CloseObservation(bridge_domain::BridgePortError),
    CommittedRecoveryRequired,
    HeartbeatAuthorityUncertain,
    Initialization(crate::generation_initialization::InitialProfileError),
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
            Self::Operator(_) => "shipping Profile Bridge operator flow failed",
            Self::ActiveSessionMissing => "shipping Profile Bridge active session is missing",
            Self::CloseObservation(_) => "shipping Profile Bridge close observation failed",
            Self::Initialization(_) => "profile first-open preparation failed",
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
enum ConfirmedSaveTrigger {
    ContinueRuntime,
    BeginConfirmedSave,
}

#[cfg(any(windows, test))]
fn confirmed_save_trigger<E>(observation: Result<bool, E>) -> Result<ConfirmedSaveTrigger, E> {
    observation.map(|controlled| {
        if controlled {
            ConfirmedSaveTrigger::BeginConfirmedSave
        } else {
            ConfirmedSaveTrigger::ContinueRuntime
        }
    })
}

/// One user-triggered Open may continue once after its local N->N+1
/// recovery was proved committed and its unused Coordinator lease was
/// released cleanly. The existing machine authority issues a fresh claim;
/// no stale claim replay, hidden local grant, background daemon or loop.
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

/// A disposable cold-probe workspace can be retired only after an
/// independently positive managed-browser observation. In particular,
/// cleanup is NEVER attempted on unknown/failed probe outcomes even when
/// the generic operator cleanup flag happens to be clear. Caller-owned
/// native-writer, immutable path and lease checks remain mandatory in retire.
#[cfg(any(windows, test))]
fn retire_historical_probe_only_after_proof<T>(
    proof: Result<T, ShippingCompositionError>,
    retire: impl FnOnce(&T) -> Result<(), ShippingCompositionError>,
) -> Result<T, ShippingCompositionError> {
    let observation = proof?;
    retire(&observation)?;
    Ok(observation)
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

/// One-shot read-only cloud recovery probe using the existing Windows
/// device/Coordinator/managed browser owners. An observed browser identity
/// is NOT a durable WORKING/LKG certificate or R2 prune grant; the normal
/// user Open/Close path is never changed by this maintenance operation.
pub fn run_historical_candidate_probe(claim: &ClaimUri) -> Result<(), ShippingCompositionError> {
    #[cfg(windows)]
    {
        return windows::run_historical_candidate_probe(claim);
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
        ConfirmedSaveTrigger, ShippingCompositionError, ShippingDeliveryCommand,
        confirmed_save_trigger, open_with_one_fresh_claim,
        retire_historical_probe_only_after_proof,
    };
    use crate::camouhost_process::{
        ManagedCamouhostConfig, ManagedCamouhostProcess, RuntimeBindingSlot, RuntimeDisplayMode,
    };
    use crate::generation_reopen::VerifiedGenerationObjectStreamDownloader;
    use crate::local_profile::{DeliveryActivationGuard, MaterializationRoot};
    use crate::operator_flow::{
        OperatorFailureStage, OperatorFlowError, PendingSaveRecovery, ProfileBridgeOperator,
    };
    use crate::shipping_control_plane::{
        ControlPlaneCoordinator, ControlPlaneEnrollment, ControlPlaneLeaseTiming,
    };
    use crate::shipping_generation_successor_control::{
        RestartExactCommitObservation, RestartExactLocalCompletion,
        complete_active_exact_after_restart, confirm_retained_exact_retry_nonapplication,
        resume_exact_successor_under_fresh_lease, resume_lone_local_unsynced_under_fresh_lease,
    };
    use crate::shipping_preflight::ShippingBrowserLaunchPreflight;
    use crate::user_diagnostics::{PendingSaveReadOnlyDiagnosis, ReadOnlyProfileDiagnosis};
    use crate::windows_delivery_handoff::{
        DeliveryHandoffCoordinator, DeliveryHandoffRestartDisposition,
    };
    use crate::windows_delivery_runtime::ActiveWindowsDeliveryRuntime;
    use crate::windows_device_application::{
        WindowsDeviceApplication, WindowsDeviceApplicationBinding,
    };
    use crate::windows_generation_get::WindowsSignedGenerationObjectGet;
    use crate::windows_generation_put::WindowsSignedGenerationObjectPut;
    use crate::windows_native::WindowsDeviceIdentity;
    use bridge_domain::ClaimUri;
    use profile_platform_primitives::UnixMillis;
    use std::env;
    use std::fs;
    use std::os::windows::fs::MetadataExt;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const NETWORK_POLICY_PATH_ENV: &str = "PROFILE_BRIDGE_NETWORK_POLICY_PATH";
    const PROXY_CONFIG_PATH_ENV: &str = "PROFILE_BRIDGE_PROXY_CONFIG_PATH";
    const PACKAGED_PYTHON_EXECUTABLE: &str = "python/python.exe";
    // Exactly one protected staging slot. A crashed/uncertain cold probe
    // leaves this slot occupied rather than silently deleting a potentially
    // still-running Firefox workspace or accumulating unlimited full clones.
    const HISTORICAL_PROBE_SLOT: &str = ".profile-bridge-cold-candidate";
    const CONTROLLED_CLOSE_POLL_MS: u64 = 250;
    const HEARTBEAT_TRANSPORT_RETRY_MS: u64 = 1_000;
    const SAVE_RECONNECT_PAUSE_MS: u64 = 1_000;
    const SAVE_RECONNECT_AUTHORITY_RESERVE_MS: u64 = 5_000;

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

    pub fn run_initial(
        intent: &crate::generation_initialization::InitialProfileOpen,
    ) -> Result<(), ShippingCompositionError> {
        use crate::operator_flow::DeviceAuthenticationPort;
        use bridge_domain::DeviceKeyPort;
        use profile_platform_primitives::{ActorContext, CorrelationId, TenantScope};
        if recover_delivery_before_claim()? {
            return Ok(());
        }
        let config = ShippingConfig::from_installation()?;
        let active = ActiveWindowsDeliveryRuntime::resolve_current().map_err(|error| {
            eprintln!("BRIDGE_DELIVERY_RUNTIME_FAILURE={error:?}");
            ShippingCompositionError::DeliveryRecoveryRequired
        })?;
        let pending_health = active.pending_health_confirmation();
        let runtime_root = active.runtime_root().to_path_buf();
        let mut bundles = active.into_bundle_selection();
        let binding =
            WindowsDeviceApplicationBinding::open(&config.device_application_binding_path)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        if binding.tenant_id() != &intent.tenant_id || binding.actor_id() != &intent.actor_id {
            return Err(ShippingCompositionError::ControlPlane);
        }
        let device = binding.device_id().clone();
        let actor = ActorContext::new(
            TenantScope::new(binding.tenant_id().clone()),
            binding.actor_id().clone(),
            CorrelationId::parse("corr_initial_profile")
                .map_err(|_| ShippingCompositionError::Configuration)?,
        );
        let mut application =
            WindowsDeviceApplication::from_system(config.control_plane_origin, binding)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        let key = application
            .ensure_key_handle(&device)
            .map_err(|_| ShippingCompositionError::ControlPlane)?;
        application
            .authenticate(&device, &key)
            .map_err(|_| ShippingCompositionError::ControlPlane)?;
        let mut transport = application.transport();
        let root = MaterializationRoot::open_or_create(&config.materialization_root)
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let runtime = ManagedCamouhostConfig::new(
            runtime_root.join(PACKAGED_PYTHON_EXECUTABLE),
            runtime_root,
            RuntimeDisplayMode::Headful,
            None,
            None,
        )
        .map_err(|_| ShippingCompositionError::Configuration)?;
        let mut uploader = WindowsSignedGenerationObjectPut::from_system()
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let materialize =
            |workspace: &crate::local_profile::GenerationWorkspace,
             bundle: &crate::runtime_bundle::ApprovedRuntimeBundle| {
                use crate::generation_initialization::InitialProfileError;
                let started = pending_health
                    .map(|pending| {
                        pending
                            .start_before_runtime_health()
                            .map_err(|_| InitialProfileError::Runtime)
                    })
                    .transpose()?;
                let identity = runtime
                    .materialize_initial_identity(workspace, bundle)
                    .map_err(|_| InitialProfileError::Runtime)?;
                if let Some(started) = started {
                    started
                        .confirm_after_runtime_ready()
                        .map_err(|_| InitialProfileError::Runtime)?;
                }
                Ok(identity)
            };
        let claim = crate::generation_initialization::initialize(
            intent,
            &actor,
            &device,
            &root,
            materialize,
            &mut bundles,
            &mut transport,
            &mut uploader,
        )
        .map_err(ShippingCompositionError::Initialization)?;
        open_with_one_fresh_claim(&claim, run, || {
            // The browser may have remained open for hours. Never reuse the
            // old machine session; reauthenticate the existing P-256 key.
            application
                .authenticate(&device, &key)
                .map_err(|_| ShippingCompositionError::ControlPlane)?;
            let mut fresh_transport = application.transport();
            crate::generation_initialization::reauthorize_current_ready_generation(
                intent,
                &actor,
                &device,
                &mut fresh_transport,
            )
            .map_err(ShippingCompositionError::Initialization)
        })
    }

    /// Maintenance-only, one real managed Camoufox in a fixed isolated slot.
    /// The normal Open is never delayed or contingent on R2 availability.
    /// A missing proof, stale lease, native writer, old key or probe-slot
    /// collision fails closed without touching the authoritative CURRENT.
    pub fn run_historical_candidate_probe(
        claim: &ClaimUri,
    ) -> Result<(), ShippingCompositionError> {
        if recover_delivery_before_claim()? {
            return Err(ShippingCompositionError::DeliveryRecoveryRequired);
        }
        let config = ShippingConfig::from_installation()?;
        let delivery = ActiveWindowsDeliveryRuntime::resolve_current()
            .map_err(|_| ShippingCompositionError::DeliveryRecoveryRequired)?;
        if delivery.pending_health_confirmation().is_some() {
            // Installing a new runtime is not a safe historical compatibility
            // witness until its own delivery-health owner has confirmed it.
            return Err(ShippingCompositionError::DeliveryRecoveryRequired);
        }
        let runtime_root = delivery.runtime_root().to_path_buf();
        let bundles = delivery.into_bundle_selection();
        let binding =
            WindowsDeviceApplicationBinding::open(&config.device_application_binding_path)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        let identity = WindowsDeviceIdentity::new(binding.device_id().clone());
        let application =
            WindowsDeviceApplication::from_system(config.control_plane_origin, binding)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        let transport = application.transport();
        let downloader = WindowsSignedGenerationObjectGet::from_system()
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let mut downloader = VerifiedGenerationObjectStreamDownloader::new(downloader);
        let primary = MaterializationRoot::open_or_create(&config.materialization_root)
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let parent = primary
            .path()
            .parent()
            .ok_or(ShippingCompositionError::Configuration)?;
        let slot_path = parent.join(HISTORICAL_PROBE_SLOT);
        let slot = RuntimeBindingSlot::new();
        let preflight = ShippingBrowserLaunchPreflight::new(slot.clone());
        let config = ManagedCamouhostConfig::new(
            runtime_root.join(PACKAGED_PYTHON_EXECUTABLE),
            runtime_root,
            RuntimeDisplayMode::Headful,
            None,
            None,
        )
        .map_err(|_| ShippingCompositionError::Configuration)?;
        let (process, camouhost) = ManagedCamouhostProcess::pair(config, slot);
        let mut witness = camouhost.cold_probe_observer();
        let mut operator = ProfileBridgeOperator::new(
            identity,
            application.clone(),
            application,
            ControlPlaneEnrollment::new(transport.clone()),
            ControlPlaneCoordinator::new(transport),
            bundles,
            preflight,
            process,
            camouhost,
        );
        // Create the one protected slot only AFTER all fallible installed
        // runtime/device/bootstrap construction. This avoids leaving a
        // permanent blocker when configuration is rejected before any probe.
        // Never remove/reuse unknown bytes after a prior crash.
        fs::create_dir(&slot_path).map_err(|_| ShippingCompositionError::Configuration)?;
        let isolated = match MaterializationRoot::open_or_create(&slot_path) {
            Ok(root) => root,
            Err(_) => {
                // Only remove our still-empty directory; do not recursively
                // delete an unverified profile or follow an injected link.
                let _ = fs::remove_dir(&slot_path);
                return Err(ShippingCompositionError::Configuration);
            }
        };
        let result = operator.probe_verified_predecessor_once(
            claim,
            &primary,
            &isolated,
            &mut downloader,
            now()?,
            &mut || witness.observe_live().map_err(|_| ()),
        );
        retire_historical_probe_only_after_proof(
            result.map_err(ShippingCompositionError::Operator),
            |observation| {
                // Called ONLY after a proved clean historical Camoufox run. Any
                // uncertain/failing probe leaves all bytes and writer evidence in
                // this slot even when the operator cleanup flag is false.
                if operator.cleanup_blocked() {
                    return Err(ShippingCompositionError::CommittedRecoveryRequired);
                }
                let restored = isolated
                    .open_generation(
                        observation.tenant_id(),
                        observation.profile_id(),
                        observation.predecessor_generation_id(),
                    )
                    .map_err(|_| ShippingCompositionError::CommittedRecoveryRequired)?;
                // The closed historical browser must leave no live native writer.
                // Unknown/unreadable locks are never a deletion authorization.
                if !matches!(
                    crate::browser_execution::browser_writer_locks_absent(&restored),
                    Ok(true)
                ) {
                    return Err(ShippingCompositionError::CommittedRecoveryRequired);
                }
                // The probe slot was atomically created by THIS invocation. Before
                // deleting even that disposable restore, reject a substituted Windows
                // junction/reparse point or a changed canonical destination. Never
                // traverse a foreign link into the real user-data directory.
                const REPARSE_POINT: u32 = 0x0400;
                let slot_metadata = fs::symlink_metadata(&slot_path)
                    .map_err(|_| ShippingCompositionError::Configuration)?;
                // On Windows, canonicalize may add the extended-length prefix
                // (\\\\?\\) to an ordinary absolute path. Compare two canonical
                // paths, not a canonical path against the user's original spelling.
                // A substituted junction to another directory still cannot match
                // the exact canonical child of the original parent.
                let physical_parent = fs::canonicalize(parent)
                    .map_err(|_| ShippingCompositionError::Configuration)?;
                let physical_slot = fs::canonicalize(&slot_path)
                    .map_err(|_| ShippingCompositionError::Configuration)?;
                if !slot_metadata.is_dir()
                    || slot_metadata.file_attributes() & REPARSE_POINT != 0
                    || physical_slot != physical_parent.join(HISTORICAL_PROBE_SLOT)
                {
                    return Err(ShippingCompositionError::Configuration);
                }
                fs::remove_dir_all(&slot_path)
                    .map_err(|_| ShippingCompositionError::Configuration)?;
                Ok(())
            },
        )?;
        eprintln!(
            "HISTORICAL_COLD_PROBE=BROWSER_IDENTITY_VERIFIED;WORKING_LKG_CERTIFICATE=0;R2_PRUNE_AUTHORITY=0"
        );
        Ok(())
    }

    pub fn run(claim: &ClaimUri) -> Result<(), ShippingCompositionError> {
        if recover_delivery_before_claim()? {
            return Ok(());
        }

        let config = ShippingConfig::from_installation()?;
        let active_delivery = ActiveWindowsDeliveryRuntime::resolve_current().map_err(|error| {
            eprintln!("BRIDGE_DELIVERY_RUNTIME_FAILURE={error:?}");
            ShippingCompositionError::DeliveryRecoveryRequired
        })?;
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
        let coordinator = ControlPlaneCoordinator::new(transport);
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
        let opened_at = now()?;
        let mut pending_health = pending_health;
        let mut started_health = None;
        operator
            .open_authoritative_with_runtime_health_and_pending_recovery(
                claim,
                &materialization_root,
                &mut generation_downloader,
                opened_at,
                || {
                    started_health = pending_health
                        .take()
                        .map(|confirmation| {
                            confirmation.start_before_runtime_health().map_err(|_| ())
                        })
                        .transpose()?;
                    Ok(())
                },
                |live_coordinator, tenant, profile, selected_generation, fresh_lease| {
                    // The existing operator already proved fresh CNG/ACL,
                    // tenant/Profile enrollment and exclusive coordinator
                    // writer claim. Only exact ACTIVE_EXACT may repair an
                    // unchanged N+1 binding; never PUT/commit with old fence.
                    match complete_active_exact_after_restart(
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
                                        "P3_RECOVERY={};NEXT_SAFE_STEP=RETRY_OWNED_RECOVERY",
                                        error.code()
                                    );
                                    false
                                }
                            };
                            if committed {
                                PendingSaveRecovery::CommittedNewerRequiresClaim
                            } else {
                                PendingSaveRecovery::Blocked
                            }
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
                                Err(error) => {
                                    eprintln!(
                                        "P3_RECOVERY={};NEXT_SAFE_STEP=RETRY_OWNED_RECOVERY",
                                        error.code()
                                    );
                                    false
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
                                "P3_RECOVERY={};NEXT_SAFE_STEP=RETRY_OWNED_RECOVERY",
                                error.code()
                            );
                            PendingSaveRecovery::Blocked
                        }
                    }
                },
            )
            .map_err(ShippingCompositionError::Operator)?;

        if let Some(confirmation) = started_health
            && confirmation.confirm_after_runtime_ready().is_err()
        {
            let abort_at = now().unwrap_or(opened_at);
            let _ = operator.abort(abort_at);
            return Err(ShippingCompositionError::DeliveryHealth);
        }

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
            let save_trigger = confirmed_save_trigger(
                close_observer
                    .observe_controlled_close(&session_id)
                    .map_err(ShippingCompositionError::CloseObservation),
            )?;
            if save_trigger == ConfirmedSaveTrigger::BeginConfirmedSave {
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
                // An unknown heartbeat response is not an acknowledged fence.
                // Retain the closed local working profile; do not attempt an
                // upload/commit using a possibly revoked writer session.
                if heartbeat_retry_used {
                    return Err(ShippingCompositionError::HeartbeatAuthorityUncertain);
                }
                let initial = operator.save_retained_successor_streaming(
                    &materialization_root,
                    save_transport.clone(),
                    &mut generation_uploader,
                    now()?,
                );
                eprintln!(
                    "{}",
                    ReadOnlyProfileDiagnosis::after_save(operator.diagnostic_facts(), &initial)
                        .redacted_record()
                );
                match initial {
                    Ok(completion) if completion.is_saved() => return Ok(()),
                    Ok(_) => return Err(ShippingCompositionError::CommittedRecoveryRequired),
                    Err(OperatorFlowError::GenerationSave { reason })
                        if !reason.allows_bounded_cloud_reconcile() =>
                    {
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
                            match confirm_retained_exact_retry_nonapplication(
                                &materialization_root,
                                &original,
                                &mut recovery_observation_transport,
                            ) {
                                Ok(true) => {}
                                Ok(false) => {
                                    return Err(ShippingCompositionError::CommittedRecoveryRequired);
                                }
                                Err(crate::shipping_generation_successor_control::RestartExactLocalCompletionError::ControlPlane(_)) => continue,
                                Err(_) => {
                                    return Err(ShippingCompositionError::CommittedRecoveryRequired);
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
                            let attempt = operator.save_retained_successor_streaming(
                                &materialization_root,
                                save_transport.clone(),
                                &mut generation_uploader,
                                now()?,
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
                                    if !reason.allows_bounded_cloud_reconcile() =>
                                {
                                    return Err(ShippingCompositionError::Operator(
                                        OperatorFlowError::GenerationSave { reason },
                                    ));
                                }
                                Err(
                                    OperatorFlowError::Stage(OperatorFailureStage::GenerationSave)
                                    | OperatorFlowError::GenerationSave { .. },
                                ) if operator.has_pending_dirty_close() => {
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
        ConfirmedSaveTrigger, ShippingCompositionError, confirmed_save_trigger,
        open_with_one_fresh_claim, retire_historical_probe_only_after_proof,
    };
    #[cfg(not(windows))]
    use super::{ShippingDeliveryCommand, run_claim, run_delivery_command};
    use crate::operator_flow::OperatorFlowError;
    use bridge_domain::ClaimUri;
    use std::cell::Cell;

    #[test]
    fn failed_or_unknown_historical_probe_never_deletes_its_recovery_slot()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(1);
        let slot = std::env::temp_dir().join(format!(
            "historical-probe-retention-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&slot)?;
        let native_evidence = slot.join("retained-user-data-marker");
        std::fs::write(&native_evidence, b"dirty-firefox-evidence")?;
        let calls = Cell::new(0);
        for failed in [
            ShippingCompositionError::CommittedRecoveryRequired,
            ShippingCompositionError::ControlPlane,
            ShippingCompositionError::Configuration,
        ] {
            let result = retire_historical_probe_only_after_proof::<()>(Err(failed), |_| {
                calls.set(calls.get() + 1);
                std::fs::remove_dir_all(&slot).map_err(|_| ShippingCompositionError::Configuration)
            });
            assert_eq!(result, Err(failed));
            assert_eq!(calls.get(), 0);
            assert_eq!(std::fs::read(&native_evidence)?, b"dirty-firefox-evidence");
        }
        // A positive probe alone is not deletion authority: the retire
        // closure must additionally prove writer, lease and exact path.
        let denied = retire_historical_probe_only_after_proof(Ok(()), |_| {
            calls.set(calls.get() + 1);
            Err(ShippingCompositionError::CommittedRecoveryRequired)
        });
        assert_eq!(
            denied,
            Err(ShippingCompositionError::CommittedRecoveryRequired)
        );
        assert_eq!(std::fs::read(&native_evidence)?, b"dirty-firefox-evidence");
        assert_eq!(calls.get(), 1);
        retire_historical_probe_only_after_proof(Ok(()), |_| {
            calls.set(calls.get() + 1);
            std::fs::remove_dir_all(&slot).map_err(|_| ShippingCompositionError::Configuration)
        })?;
        assert_eq!(calls.get(), 2);
        assert!(!slot.exists());
        Ok(())
    }

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
    fn confirmed_save_requires_positive_controlled_close_witness() {
        assert_eq!(
            confirmed_save_trigger::<ShippingCompositionError>(Ok(false)),
            Ok(ConfirmedSaveTrigger::ContinueRuntime)
        );
        assert_eq!(
            confirmed_save_trigger::<ShippingCompositionError>(Ok(true)),
            Ok(ConfirmedSaveTrigger::BeginConfirmedSave)
        );
        assert_eq!(
            confirmed_save_trigger(Err(ShippingCompositionError::CloseObservation(
                bridge_domain::BridgePortError::Unavailable,
            ))),
            Err(ShippingCompositionError::CloseObservation(
                bridge_domain::BridgePortError::Unavailable,
            ))
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
