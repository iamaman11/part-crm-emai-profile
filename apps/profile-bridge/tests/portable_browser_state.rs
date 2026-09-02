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

    struct FixedObservation(browser_execution_domain::NetworkIdentityObservation);

    impl crate::browser_preflight::BrowserRuntimeObservationPort for FixedObservation {
        type Error = bridge_domain::BridgePortError;

        fn observe(
            &mut self,
            _workspace: &GenerationWorkspace,
            _device_id: &DeviceId,
        ) -> Result<crate::browser_preflight::BrowserRuntimeObservation, Self::Error> {
            Ok(crate::browser_preflight::BrowserRuntimeObservation::new(
                self.0.clone(),
                false,
            ))
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

    fn portable_network_policy() -> Result<
        (
            browser_execution_domain::NetworkIdentityPolicy,
            browser_execution_domain::NetworkIdentityObservation,
        ),
        Box<dyn std::error::Error>,
    > {
        Ok((
            browser_execution_domain::NetworkIdentityPolicy::new(
                Some("PL".to_owned()),
                Some("Mazowieckie".to_owned()),
                Some("Europe/Warsaw".to_owned()),
                [browser_execution_domain::NetworkClass::Mobile],
                [5617],
                Some("s0-portable-local-evidence".to_owned()),
            )?,
            browser_execution_domain::NetworkIdentityObservation::new(
                "PL",
                "Mazowieckie",
                "Europe/Warsaw",
                browser_execution_domain::NetworkClass::Mobile,
                5617,
                "s0-portable-local-evidence",
            )?,
        ))
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
        let runtime_generations = Rc::new(RefCell::new(Vec::new()));
        let enrollment = ControlPlaneEnrollment::new(transport.clone());
        let coordinator = ControlPlaneCoordinator::new(transport.clone());
        let (network_policy, network_observation) = portable_network_policy()?;
        let runtime_binding = crate::camouhost_process::RuntimeBindingSlot::new();
        let browser_preflight = crate::shipping_preflight::ShippingBrowserLaunchPreflight::new(
            network_policy,
            FixedObservation(network_observation),
            runtime_binding.clone(),
        );
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
            RecordingBundles {
                bundle,
                generations: Rc::clone(&runtime_generations),
            },
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
        operator.close(UnixMillis::new(20))?;
        assert_eq!(
            operator.pending_dirty_local_state(),
            Some(LocalGenerationState::DirtyLocal)
        );

        let mut put = BackendPut {
            state: Rc::clone(&state),
        };
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
        root.reject_generation_for_rematerialization(&tenant_id, &profile_id, &successor)?;
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
        assert_eq!(
            runtime_generations.borrow().as_slice(),
            [BASE_GENERATION, successor.as_str()]
        );

        operator.abort(UnixMillis::new(50))?;
        eprintln!(
            "S0_REAL_PORTABLE_BROWSER_STATE=PASSED;MANAGED_PROCESS=1;MANAGED_IPC=1;COOKIE=1;LOCAL_STORAGE=1;INDEXED_DB=1;AUTHORITATIVE_RESTORE=1;IDENTITY_REUSED=1"
        );
        Ok(())
    }
}
