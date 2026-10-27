#![forbid(unsafe_code)]

// Re-export the normal Profile Bridge modules so the shared P3 acceptance fixture can exercise
// the same production owners without changing their production visibility solely for acceptance.
pub use profile_bridge::*;

#[cfg(not(feature = "synthetic-test-bin"))]
mod shared_fakes {
    include!("../src/test_fakes.rs");
}
#[cfg(not(feature = "synthetic-test-bin"))]
pub use shared_fakes::{
    FakeCamouhost, FakeDeviceIdentity, FakeDeviceKeyStore, FakeProcessControl, ProcessAction,
};

mod test_support {
    include!("../src/browser_identity_test_fixture.rs");
}

mod p3_fixture {
    include!("../src/p3_reopen_test_support.rs");

    #[derive(Clone, Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RealIdentityReport {
        fingerprint_config_sha256: String,
        fingerprint_policy_version: String,
        profile_stable_probe_sha256: String,
        runtime_lock_sha256: String,
    }

    #[derive(Clone)]
    struct RealRuntimePaths {
        python: std::path::PathBuf,
        runtime_root: std::path::PathBuf,
        camouhost: std::path::PathBuf,
        runtime_lock: std::path::PathBuf,
        display_mode: crate::camouhost_process::RuntimeDisplayMode,
        headless: String,
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct OriginReady {
        origin_port: u16,
        ready: bool,
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct OriginObservation {
        phase: String,
        visit: u32,
        cookie: bool,
        local_storage: bool,
        indexed_db: bool,
    }

    struct PortableStateOrigin {
        child: std::process::Child,
        reports: std::sync::mpsc::Receiver<String>,
        origin_port: u16,
    }

    impl PortableStateOrigin {
        fn start(python: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
            use std::io::BufRead as _;

            let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()?;
            let fixture = repository_root.join("scripts/test-s0-portable-browser-state.py");
            if !fixture.is_file() || fixture.symlink_metadata()?.file_type().is_symlink() {
                return Err(
                    "portable browser-state stable-origin fixture is missing or unsafe".into(),
                );
            }

            let mut child = std::process::Command::new(python)
                .arg(&fixture)
                .arg("--port")
                .arg("0")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit())
                .spawn()?;
            let stdout = child
                .stdout
                .take()
                .ok_or("portable browser-state fixture stdout is unavailable")?;
            let (sender, reports) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });

            let ready_line = reports
                .recv_timeout(std::time::Duration::from_secs(10))
                .map_err(|error| {
                    std::io::Error::other(format!(
                        "portable browser-state fixture did not become ready: {error}"
                    ))
                })?;
            let ready: OriginReady = serde_json::from_str(&ready_line)?;
            if !ready.ready || ready.origin_port < 1024 {
                return Err("portable browser-state fixture returned invalid readiness".into());
            }
            Ok(Self {
                child,
                reports,
                origin_port: ready.origin_port,
            })
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}/", self.origin_port)
        }

        fn expect(
            &self,
            expected_phase: &str,
            expected_visit: u32,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let line = self
                .reports
                .recv_timeout(std::time::Duration::from_secs(30))
                .map_err(|error| {
                    std::io::Error::other(format!(
                        "portable browser-state observation timed out: {error}"
                    ))
                })?;
            let observed: OriginObservation = serde_json::from_str(&line)?;
            if observed.phase != expected_phase
                || observed.visit != expected_visit
                || !observed.cookie
                || !observed.local_storage
                || !observed.indexed_db
            {
                return Err(format!(
                    "portable browser-state observation failed: phase={} visit={} cookie={} local_storage={} indexed_db={}",
                    observed.phase,
                    observed.visit,
                    observed.cookie,
                    observed.local_storage,
                    observed.indexed_db
                )
                .into());
            }
            Ok(())
        }
    }

    impl Drop for PortableStateOrigin {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
    }

    fn real_runtime_requested() -> bool {
        std::env::var("AR10_REAL_CAMOUFOX").as_deref() == Ok("1")
    }

    fn real_runtime_paths() -> Result<RealRuntimePaths, Box<dyn std::error::Error>> {
        let python = PathBuf::from(std::env::var("AR10_PYTHON")?).canonicalize()?;
        let runtime_root = PathBuf::from(std::env::var("AR10_RUNTIME_ROOT")?).canonicalize()?;
        let camouhost = runtime_root.join("camouhost").join("real.py");
        let runtime_lock = runtime_root.join("camouhost").join("runtime-lock.json");
        let headless = std::env::var("AR10_BROWSER_STATE_HEADLESS")?;
        let display_mode = match headless.as_str() {
            "false" => crate::camouhost_process::RuntimeDisplayMode::Headful,
            "virtual" => crate::camouhost_process::RuntimeDisplayMode::VirtualHeadful,
            _ => {
                return Err("AR10_BROWSER_STATE_HEADLESS must be exactly false or virtual".into());
            }
        };
        for (path, label) in [
            (&python, "runtime Python"),
            (&camouhost, "Camouhost entrypoint"),
            (&runtime_lock, "runtime lock"),
        ] {
            if !path.is_file() || path.symlink_metadata()?.file_type().is_symlink() {
                return Err(format!(
                    "{label} is not a regular non-symlink file: {}",
                    path.display()
                )
                .into());
            }
        }
        #[cfg(windows)]
        for (path, label) in [
            (
                runtime_root.join("browser/camoufox.exe"),
                "packaged Camoufox",
            ),
            (runtime_root.join("python/python.exe"), "packaged Python"),
        ] {
            if !path.is_file() || path.symlink_metadata()?.file_type().is_symlink() {
                return Err(format!(
                    "{label} is not a regular non-symlink file: {}",
                    path.display()
                )
                .into());
            }
        }
        Ok(RealRuntimePaths {
            python,
            runtime_root,
            camouhost,
            runtime_lock,
            display_mode,
            headless,
        })
    }

    fn digest_runtime_file(
        runtime_root: &std::path::Path,
        relative: &str,
    ) -> Result<(String, u64, String), Box<dyn std::error::Error>> {
        let path = runtime_root.join(relative);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("runtime evidence is not a regular file: {relative}").into());
        }
        let bytes = fs::read(path)?;
        Ok((
            relative.to_owned(),
            metadata.len(),
            lower_hex(&Sha256::digest(bytes)),
        ))
    }

    fn approved_real_bundle(
        paths: &RealRuntimePaths,
    ) -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
        let mut files = vec![
            digest_runtime_file(&paths.runtime_root, "camouhost/real.py")?,
            digest_runtime_file(&paths.runtime_root, "camouhost/runtime-lock.json")?,
        ];
        #[cfg(windows)]
        {
            files.push(digest_runtime_file(
                &paths.runtime_root,
                "browser/camoufox.exe",
            )?);
            files.push(digest_runtime_file(
                &paths.runtime_root,
                "python/python.exe",
            )?);
        }
        files.sort_by(|left, right| left.0.cmp(&right.0));

        let mut canonical = String::from("[");
        for (index, (path, length, sha256)) in files.iter().enumerate() {
            if index > 0 {
                canonical.push(',');
            }
            canonical.push_str(&format!(
                "{{\"length\":{length},\"path\":\"{path}\",\"sha256\":\"{sha256}\"}}"
            ));
        }
        canonical.push_str("]\n");
        let inventory_sha256 = lower_hex(&Sha256::digest(canonical.as_bytes()));
        let manifest = RuntimeManifest::new(
            "2.0.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse("camouhost/real.py")?,
            Sha256Digest::parse(inventory_sha256.clone())?,
        )?;
        let mut entries = Vec::with_capacity(files.len());
        for (path, length, sha256) in files {
            entries.push(InventoryEntry::new(
                BundleRelativePath::parse(path)?,
                length,
                Sha256Digest::parse(sha256)?,
            ));
        }
        let inventory = RuntimeInventory::new(entries)?;
        Ok(ApprovedRuntimeBundle::validate(
            manifest,
            inventory,
            &Sha256Digest::parse(inventory_sha256)?,
        )?)
    }

    fn run_command(
        mut command: std::process::Command,
        label: &str,
    ) -> Result<std::process::Output, Box<dyn std::error::Error>> {
        let output = command.output()?;
        if !output.status.success() {
            return Err(format!(
                "{label} failed: status={} stdout={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(output)
    }

    fn materialize_real_identity(
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
        bundle: &ApprovedRuntimeBundle,
        paths: &RealRuntimePaths,
    ) -> Result<MaterializationBinding, Box<dyn std::error::Error>> {
        let writer = crate::local_profile::BridgeWorkspaceLock::acquire(workspace, device_id, 99)?;
        let mut command = std::process::Command::new(&paths.python);
        command
            .arg(&paths.camouhost)
            .arg("--materialize-identity")
            .arg(workspace.path())
            .env("CAMOUHOST_RUNTIME_LOCK", &paths.runtime_lock)
            .env("CAMOUHOST_HEADLESS_MODE", &paths.headless)
            .env_remove("CAMOUHOST_PROXY_CONFIG_PATH");
        let output = run_command(command, "real candidate identity materialization")?;
        let report: RealIdentityReport = serde_json::from_slice(&output.stdout)?;
        for (value, label) in [
            (&report.fingerprint_config_sha256, "fingerprint config"),
            (&report.profile_stable_probe_sha256, "profile-stable probe"),
            (&report.runtime_lock_sha256, "runtime lock"),
        ] {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            {
                return Err(format!("{label} digest is not canonical lower-hex SHA-256").into());
            }
        }
        let lock_bytes = fs::read(&paths.runtime_lock)?;
        if lower_hex(&Sha256::digest(lock_bytes)) != report.runtime_lock_sha256 {
            return Err("materialization report runtime-lock digest mismatch".into());
        }

        fs::write(workspace.path().join("prefs.js"), b"portable-browser-state")?;
        let config_bytes = fs::read(workspace.path().join("camoufox-config.json"))?;
        if lower_hex(&Sha256::digest(&config_bytes)) != report.fingerprint_config_sha256 {
            return Err("materialization report config digest mismatch".into());
        }
        let stable =
            crate::shipping_preflight::profile_stable_identity_from_config_bytes(&config_bytes)?;
        let identity = browser_execution_domain::BrowserIdentityManifest::new(
            2,
            report.fingerprint_policy_version.clone(),
            bundle.manifest().runtime_version(),
            bundle.manifest().inventory_sha256().as_str(),
            format!(
                "{}-probe-{}",
                report.fingerprint_policy_version, report.profile_stable_probe_sha256
            ),
            report.fingerprint_config_sha256,
            stable,
        )?;
        let binding = MaterializationBinding::new(
            TenantId::parse(TENANT)?,
            ProfileId::parse(PROFILE)?,
            GenerationId::parse(BASE_GENERATION)?,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity,
        )?;
        persist_materialization_binding(workspace, &binding)?;
        writer.release()?;
        Ok(binding)
    }

    #[test]
    fn real_cookie_local_storage_and_indexed_db_survive_authoritative_p3_reopen()
    -> Result<(), Box<dyn std::error::Error>> {
        if !real_runtime_requested() {
            eprintln!(
                "S0_REAL_PORTABLE_BROWSER_STATE=SKIPPED;REASON=AR10_REAL_CAMOUFOX_NOT_REQUESTED"
            );
            return Ok(());
        }

        let paths = real_runtime_paths()?;
        let bundle = approved_real_bundle(&paths)?;
        let origin = PortableStateOrigin::start(&paths.python)?;

        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root_path = std::env::temp_dir().join(format!(
            "profile-bridge-s0-portable-browser-state-{}-{counter}",
            std::process::id()
        ));
        let _cleanup = CleanupRoot(root_path.clone());
        let root = MaterializationRoot::open_or_create(root_path)?;
        let tenant_id = TenantId::parse(TENANT)?;
        let profile_id = ProfileId::parse(PROFILE)?;
        let base_generation = GenerationId::parse(BASE_GENERATION)?;
        let device_id = DeviceId::parse(DEVICE)?;
        let base_workspace = root.create_generation(&tenant_id, &profile_id, &base_generation)?;
        let base_binding = materialize_real_identity(&base_workspace, &device_id, &bundle, &paths)?;

        let state = Rc::new(RefCell::new(BackendState::new()?));
        let transport = BackendMachineHttp {
            state: Rc::clone(&state),
        };
        let enrollment = ControlPlaneEnrollment::new(transport.clone());
        let coordinator = ControlPlaneCoordinator::new(transport.clone());
        let runtime_binding = crate::camouhost_process::RuntimeBindingSlot::new();
        let browser_preflight =
            crate::shipping_preflight::ShippingBrowserLaunchPreflight::new(runtime_binding.clone());
        let managed_config = crate::camouhost_process::ManagedCamouhostConfig::new(
            paths.python.clone(),
            paths.runtime_root.clone(),
            paths.display_mode,
            Some(origin.url()),
            None,
        )?;
        let (process, camouhost) = crate::camouhost_process::ManagedCamouhostProcess::pair(
            managed_config,
            runtime_binding,
        );
        let mut operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(device_id),
            FakeDeviceKeyStore::default(),
            DeviceAuthentication,
            enrollment,
            coordinator,
            TestRuntimeBundle { bundle },
            browser_preflight,
            process,
            camouhost,
        );
        let mut downloader = VerifiedGenerationObjectDownloader::new(BackendGet {
            state: Rc::clone(&state),
        });

        let first_claim =
            ClaimUri::parse("profilebridge://claim/claim_s0_browser_state_first_000001")?;
        operator.open_authoritative(&first_claim, &root, &mut downloader, UnixMillis::new(10))?;
        origin.expect("seed", 1)?;
        assert_eq!(
            operator.active_local_state(),
            Some(LocalGenerationState::InUse)
        );
        // The installed controlled Close must durably pin the original
        // LOCAL_UNSYNCED N+1 before the streaming upload starts. A plain
        // synthetic close is not the Windows release Save entrypoint.
        #[cfg(windows)]
        operator.close_with_durable_local_intent(&root, UnixMillis::new(20))?;
        #[cfg(not(windows))]
        operator.close(UnixMillis::new(20))?;
        assert_eq!(
            operator.pending_dirty_local_state(),
            Some(LocalGenerationState::DirtyLocal)
        );

        let mut put = BackendPut {
            state: Rc::clone(&state),
        };
        // Real installed Windows builds prove the shipping stream path. Linux
        // integration compilation remains on the test-only predecessor: the
        // production streaming operator method is intentionally Windows-only.
        #[cfg(windows)]
        let completion = operator.save_retained_successor_streaming(
            &root,
            transport.clone(),
            &mut put,
            UnixMillis::new(30),
        )?;
        #[cfg(not(windows))]
        let completion = operator.save_retained_successor(
            &root,
            transport.clone(),
            &mut put,
            UnixMillis::new(30),
        )?;
        assert!(completion.is_saved());
        let successor = completion.committed().generation_id().clone();
        assert_ne!(successor, base_generation);

        // Force the second launch to consume the exact committed encrypted object rather than any
        // locally retained successor workspace.
        // This test exclusively owns a disposable local fixture. Shipping never
        // discards an unresolved successor merely to force a fresh cloud read.
        let disposable_successor = root.open_generation(&tenant_id, &profile_id, &successor)?;
        std::fs::remove_dir_all(disposable_successor.path())?;
        assert!(
            root.open_generation(&tenant_id, &profile_id, &successor)
                .is_err()
        );

        let second_claim =
            ClaimUri::parse("profilebridge://claim/claim_s0_browser_state_second_000002")?;
        operator.open_authoritative(&second_claim, &root, &mut downloader, UnixMillis::new(40))?;
        origin.expect("verify", 2)?;
        let reopened = root.open_generation(&tenant_id, &profile_id, &successor)?;
        let restored_binding = crate::browser_execution::load_materialization_binding(
            &reopened,
            &tenant_id,
            &profile_id,
            &successor,
        )?;
        assert_eq!(
            base_binding.browser_identity(),
            restored_binding.browser_identity()
        );

        let events = state.borrow().events.clone();
        #[cfg(windows)]
        {
            let retry = event_index(&events, "retry:nonapplied:")?;
            let put = event_index(&events, "put:")?;
            let commit = event_index(&events, "commit:")?;
            assert!(retry < put && put < commit);
        }
        let commit = event_index(&events, "commit:")?;
        let clean_release = event_index(&events, "coordinator:release:Clean")?;
        let capability = event_index(&events, "download-capability:")?;
        let get = event_index(&events, "get:")?;
        let opening = event_index(&events, "opening-material:")?;
        assert!(commit < clean_release);
        assert!(clean_release < capability);
        assert!(capability < get);
        assert!(get < opening);
        assert!(events.iter().all(|event| !event.contains("device-job")));

        // K2 integration fixture: move CURRENT to an actual second committed
        // generation, retaining the previous immutable encrypted object.
        // Never select the historical object from the live CURRENT pointer.
        #[cfg(windows)]
        operator.close_with_durable_local_intent(&root, UnixMillis::new(50))?;
        #[cfg(not(windows))]
        operator.close(UnixMillis::new(50))?;
        #[cfg(windows)]
        let next = operator.save_retained_successor_streaming(
            &root,
            transport.clone(),
            &mut put,
            UnixMillis::new(60),
        )?;
        #[cfg(not(windows))]
        let next = operator.save_retained_successor(
            &root,
            transport.clone(),
            &mut put,
            UnixMillis::new(60),
        )?;
        assert!(next.is_saved());
        let current = next.committed().generation_id().clone();
        assert_ne!(current, successor);
        // CURRENT is encrypted with the rotated key. The previous version
        // must still decrypt the historical object and launch Camoufox.
        {
            let backend = state.borrow();
            let latest = backend
                .object
                .as_ref()
                .ok_or("rotated current encrypted object missing")?;
            let metadata = inspect_generation_metadata_prelude(latest)?;
            assert_eq!(metadata.metadata().key_id().as_str(), ROTATED_ROOT_KEY_ID);
        }
        let (historical_request, historical_container) = {
            let backend = state.borrow();
            assert_eq!(backend.active_generation, current);
            let (request, bytes) = backend
                .retained_object
                .as_ref()
                .ok_or("the previous committed immutable object was not retained")?;
            assert_eq!(request.generation_id(), successor.as_str());
            (request.clone(), bytes.clone())
        };
        assert_eq!(
            historical_request.object_key(),
            canonical_generation_object_key(&tenant_id, &profile_id, &successor)
        );
        let historical_metadata = inspect_generation_metadata_prelude(&historical_container)?;
        assert_eq!(
            historical_metadata.metadata().key_id().as_str(),
            ROOT_KEY_ID
        );
        // Fresh CURRENT coordinator authority, immutable old R2 bytes and a
        // physically empty independent workspace. Unlike the old synthetic
        // acceptance this NEVER promotes the historical generation to CURRENT.
        let historical_path = std::env::temp_dir().join(format!(
            "profile-bridge-historical-camoufox-{}-{counter}",
            std::process::id()
        ));
        let _historical_cleanup = CleanupRoot(historical_path.clone());
        let historical_root = MaterializationRoot::open_or_create(historical_path)?;
        assert!(
            historical_root
                .open_generation(&tenant_id, &profile_id, &successor)
                .is_err()
        );
        let fresh_claim =
            ClaimUri::parse("profilebridge://claim/claim_s0_historical_lkg_cold_000003")?;
        let historical_slot = crate::camouhost_process::RuntimeBindingSlot::new();
        let historical_preflight =
            crate::shipping_preflight::ShippingBrowserLaunchPreflight::new(historical_slot.clone());
        let historical_config = crate::camouhost_process::ManagedCamouhostConfig::new(
            paths.python.clone(),
            paths.runtime_root.clone(),
            paths.display_mode,
            Some(origin.url()),
            None,
        )?;
        let (historical_process, historical_camouhost) =
            crate::camouhost_process::ManagedCamouhostProcess::pair(
                historical_config,
                historical_slot,
            );
        let mut historical_operator = ProfileBridgeOperator::new(
            FakeDeviceIdentity::new(DeviceId::parse(DEVICE)?),
            FakeDeviceKeyStore::default(),
            DeviceAuthentication,
            ControlPlaneEnrollment::new(transport.clone()),
            ControlPlaneCoordinator::new(transport.clone()),
            TestRuntimeBundle {
                bundle: approved_real_bundle(&paths)?,
            },
            historical_preflight,
            historical_process,
            historical_camouhost,
        );
        let mut historical_download = VerifiedGenerationObjectDownloader::new(BackendGet {
            state: Rc::clone(&state),
        });
        let restored = historical_operator.probe_verified_predecessor_once(
            &fresh_claim,
            &root,
            &historical_root,
            &mut historical_download,
            UnixMillis::new(70),
            &mut || origin.expect("verify", 3).map_err(|_| ()),
        )?;
        assert_eq!(restored.predecessor_generation_id(), &successor);
        assert_eq!(restored.current_generation_id(), &current);
        assert_eq!(restored.tenant_id(), &tenant_id);
        assert_eq!(restored.profile_id(), &profile_id);
        assert_eq!(
            restored.runtime_inventory_sha256(),
            approved_real_bundle(&paths)?
                .manifest()
                .inventory_sha256()
                .as_str()
        );
        assert_eq!(state.borrow().active_generation, current);
        let restored_workspace = historical_root.open_generation(
            &tenant_id,
            &profile_id,
            restored.predecessor_generation_id(),
        )?;
        let historical_binding = crate::browser_execution::load_materialization_binding(
            &restored_workspace,
            &tenant_id,
            &profile_id,
            restored.predecessor_generation_id(),
        )?;
        assert_eq!(
            restored.source_container_sha256(),
            historical_binding.source_container_sha256()
        );
        let redacted = format!("{restored:?}");
        for private in [
            tenant_id.as_str(),
            profile_id.as_str(),
            current.as_str(),
            successor.as_str(),
            restored.source_container_sha256(),
            restored.runtime_inventory_sha256(),
        ] {
            assert!(!redacted.contains(private));
        }
        assert!(redacted.contains("authorization: \"NONE\""));
        assert_eq!(
            restored.materialized_inventory_digest(),
            historical_binding.materialized_inventory_digest()
        );
        assert_eq!(
            historical_binding.browser_identity(),
            base_binding.browser_identity()
        );
        let original_current = root.open_generation(&tenant_id, &profile_id, &current)?;
        assert!(
            !original_current
                .path()
                .join(".profile-platform.lock")
                .exists()
        );
        {
            let state_ref = state.borrow();
            let events = &state_ref.events;
            assert!(
                events
                    .iter()
                    .any(|e| e.starts_with("historical-download-capability:"))
            );
            assert!(events.iter().any(|e| e.starts_with("historical-get:")));
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.starts_with("historical-download-capability:"))
                    .count(),
                2,
                "historical R2 permission must be revalidated after the real browser has stopped"
            );
            assert!(
                events
                    .iter()
                    .any(|e| e.starts_with("historical-opening-material:"))
            );
            assert!(!events.iter().any(|e| e.contains("historical-commit")));
            assert!(
                events.iter().any(|e| e == "coordinator:release:Clean"),
                "the existing coordinator must release CURRENT cleanly only after verified historical stop"
            );
        }

        // Subsequent negative probes hold a NEW genuine CURRENT lease.
        // The completed historical success never keeps a second writer.
        use crate::operator_flow::EnrollmentPort as _;
        use application_ports::ProfileCoordinatorPort as _;
        let negative_claim =
            ClaimUri::parse("profilebridge://claim/claim_s0_historical_failure_000004")?;
        let probe_device_id = DeviceId::parse(DEVICE)?;
        let mut enrollment = ControlPlaneEnrollment::new(transport.clone());
        let admitted =
            enrollment.redeem_claim(&negative_claim, &probe_device_id, UnixMillis::new(81))?;
        assert_eq!(admitted.generation_id(), &current);
        let mut historical_control = ControlPlaneCoordinator::new(transport.clone());
        let lease = historical_control.claim_launch_intent(
            admitted.actor(),
            admitted.profile_id(),
            &probe_device_id,
            admitted.launch_intent_id(),
        )?;
        assert_eq!(lease.profile_id(), &profile_id);
        assert_eq!(lease.device_id(), &probe_device_id);
        let runtime = approved_real_bundle(&paths)?;

        // A *real* historical Camoufox launch cannot certify WORKING if
        // the authoritative D1 predecessor/current fence changes while the
        // browser is running. Reject the second, post-close server read.
        state.borrow_mut().historical_capability_reads = 0;
        state.borrow_mut().historical_recheck_rejected = true;
        let stale_path = std::env::temp_dir().join(format!(
            "profile-bridge-stale-historical-recheck-{}-{counter}",
            std::process::id()
        ));
        let _stale_cleanup = CleanupRoot(stale_path.clone());
        let stale_root = MaterializationRoot::open_or_create(stale_path)?;
        let stale_slot = crate::camouhost_process::RuntimeBindingSlot::new();
        let mut stale_preflight =
            crate::shipping_preflight::ShippingBrowserLaunchPreflight::new(stale_slot.clone());
        let stale_config = crate::camouhost_process::ManagedCamouhostConfig::new(
            paths.python.clone(),
            paths.runtime_root.clone(),
            paths.display_mode,
            Some(origin.url()),
            None,
        )?;
        let (mut stale_process, mut stale_camouhost) =
            crate::camouhost_process::ManagedCamouhostProcess::pair(stale_config, stale_slot);
        let mut stale_downloader = VerifiedGenerationObjectDownloader::new(BackendGet {
            state: Rc::clone(&state),
        });
        let stale = crate::authoritative_generation::cold_launch_verified_predecessor_once(
            &root,
            &stale_root,
            &current,
            &lease,
            &runtime,
            &mut historical_control,
            &mut stale_downloader,
            &mut stale_preflight,
            &mut stale_process,
            &mut stale_camouhost,
            &mut || origin.expect("verify", 4).map_err(|_| ()),
        );
        assert!(matches!(
            stale,
            Err(
                crate::authoritative_generation::HistoricalColdProbeError::PostLaunchAuthorityUnconfirmed
            )
        ));
        assert_eq!(state.borrow().historical_capability_reads, 2);
        assert_eq!(state.borrow().active_generation, current);
        assert!(
            !original_current
                .path()
                .join(".profile-platform.lock")
                .exists()
        );
        state.borrow_mut().historical_capability_reads = 0;
        state.borrow_mut().historical_recheck_rejected = false;

        // Negative admission against the SAME unchanged CURRENT pointer and
        // coordinator: missing original root key must block before any browser
        // process is spawned, and cannot overwrite the proven prior cold copy.
        state.borrow_mut().historical_root_available = false;
        let missing_key_path = std::env::temp_dir().join(format!(
            "profile-bridge-missing-old-root-{}-{counter}",
            std::process::id()
        ));
        let _missing_key_cleanup = CleanupRoot(missing_key_path.clone());
        let missing_key_root = MaterializationRoot::open_or_create(missing_key_path)?;
        let missing_slot = crate::camouhost_process::RuntimeBindingSlot::new();
        let mut missing_preflight =
            crate::shipping_preflight::ShippingBrowserLaunchPreflight::new(missing_slot.clone());
        let missing_config = crate::camouhost_process::ManagedCamouhostConfig::new(
            paths.python.clone(),
            paths.runtime_root.clone(),
            paths.display_mode,
            Some(origin.url()),
            None,
        )?;
        let (mut missing_process, mut missing_camouhost) =
            crate::camouhost_process::ManagedCamouhostProcess::pair(missing_config, missing_slot);
        let mut missing_download = VerifiedGenerationObjectDownloader::new(BackendGet {
            state: Rc::clone(&state),
        });
        let failure = crate::authoritative_generation::cold_launch_verified_predecessor_once(
            &root,
            &missing_key_root,
            &current,
            &lease,
            &runtime,
            &mut historical_control,
            &mut missing_download,
            &mut missing_preflight,
            &mut missing_process,
            &mut missing_camouhost,
            &mut || Err(()),
        );
        assert!(matches!(
            failure,
            Err(crate::authoritative_generation::HistoricalColdProbeError::Materialization(_))
        ));
        assert!(
            missing_key_root
                .open_generation(&tenant_id, &profile_id, &successor)
                .is_err()
        );
        assert_eq!(state.borrow().active_generation, current);
        assert!(
            !original_current
                .path()
                .join(".profile-platform.lock")
                .exists()
        );
        assert_eq!(
            fs::read(restored_workspace.path().join("prefs.js"))?,
            b"portable-browser-state"
        );
        state.borrow_mut().historical_root_available = true;
        // No fake successor lease was minted. The original CURRENT lease is
        // released through its canonical coordinator owner.
        historical_control.close_lease(&lease)?;
        eprintln!(
            "S0_REAL_PORTABLE_BROWSER_STATE=PASSED;STREAMING_BPAT=1;MANAGED_PROCESS=1;MANAGED_IPC=1;COOKIE=1;LOCAL_STORAGE=1;INDEXED_DB=1;AUTHORITATIVE_RESTORE=1;IDENTITY_REUSED=1;HISTORICAL_NONCURRENT_COLD_CAMOUFOX=1;HISTORICAL_OLD_KEY=1;HISTORICAL_MISSING_OLD_KEY_BLOCKED=1;HISTORICAL_CURRENT_POINTER_UNCHANGED=1;HISTORICAL_POST_LAUNCH_FENCE_RECHECK_BLOCKED=1;LIVE_LKG_CERTIFICATE=0"
        );
        Ok(())
    }
}
