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
    include!("../src/test_support.rs");
}

mod p3_fixture {
    include!("../src/p3_reopen_test_support.rs");

    #[derive(Clone, Debug, serde::Deserialize)]
    struct RealIdentityReport {
        fingerprint_config_sha256: String,
        fingerprint_policy_version: String,
        profile_stable_probe_sha256: String,
        runtime_lock_sha256: String,
    }

    fn real_runtime_requested() -> bool {
        std::env::var("AR10_REAL_CAMOUFOX").as_deref() == Ok("1")
    }

    fn real_runtime_paths()
    -> Result<(PathBuf, PathBuf, PathBuf, String), Box<dyn std::error::Error>> {
        let python = PathBuf::from(std::env::var("AR10_PYTHON")?);
        let runtime_root = PathBuf::from(std::env::var("AR10_RUNTIME_ROOT")?);
        let camouhost = runtime_root.join("camouhost").join("real.py");
        let runtime_lock = runtime_root.join("camouhost").join("runtime-lock.json");
        let headless = std::env::var("AR10_BROWSER_STATE_HEADLESS")?;
        if !matches!(headless.as_str(), "false" | "virtual") {
            return Err("AR10_BROWSER_STATE_HEADLESS must be false or virtual".into());
        }
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
        Ok((python, camouhost, runtime_lock, headless))
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
    ) -> Result<RealIdentityReport, Box<dyn std::error::Error>> {
        let (python, camouhost, runtime_lock, headless) = real_runtime_paths()?;
        let writer = crate::local_profile::BridgeWorkspaceLock::acquire(workspace, device_id, 99)?;
        let mut command = std::process::Command::new(&python);
        command
            .arg(&camouhost)
            .arg("--materialize-identity")
            .arg(workspace.path())
            .env("CAMOUHOST_RUNTIME_LOCK", &runtime_lock)
            .env("CAMOUHOST_HEADLESS_MODE", &headless)
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
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(format!("{label} digest is not canonical lower-hex SHA-256").into());
            }
        }
        let lock_bytes = fs::read(&runtime_lock)?;
        if lower_hex(&Sha256::digest(&lock_bytes)) != report.runtime_lock_sha256 {
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
            report.fingerprint_config_sha256.clone(),
            stable,
        )?;
        let tenant_id = TenantId::parse(TENANT)?;
        let profile_id = ProfileId::parse(PROFILE)?;
        let generation_id = GenerationId::parse(BASE_GENERATION)?;
        let binding = MaterializationBinding::new(
            tenant_id,
            profile_id,
            generation_id,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity,
        )?;
        persist_materialization_binding(workspace, &binding)?;
        writer.release()?;
        Ok(report)
    }

    fn run_real_browser_state_probe(
        workspace: &GenerationWorkspace,
        report: &RealIdentityReport,
        origin_port: u16,
        mode: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !matches!(mode, "seed" | "verify") {
            return Err("browser-state probe mode must be seed or verify".into());
        }
        if origin_port < 1024 {
            return Err("browser-state origin port is outside the accepted range".into());
        }
        let (python, camouhost, runtime_lock, headless) = real_runtime_paths()?;
        let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        let probe = repository_root.join("scripts/test-s0-portable-browser-state.py");
        if !probe.is_file() || probe.symlink_metadata()?.file_type().is_symlink() {
            return Err("portable browser-state probe source is missing or unsafe".into());
        }
        let mut command = std::process::Command::new(&python);
        command
            .arg(&probe)
            .arg("--python")
            .arg(&python)
            .arg("--camouhost")
            .arg(&camouhost)
            .arg("--runtime-lock")
            .arg(&runtime_lock)
            .arg("--profile-root")
            .arg(workspace.path())
            .arg("--config-sha256")
            .arg(&report.fingerprint_config_sha256)
            .arg("--probe-sha256")
            .arg(&report.profile_stable_probe_sha256)
            .arg("--runtime-lock-sha256")
            .arg(&report.runtime_lock_sha256)
            .arg("--headless")
            .arg(&headless)
            .arg("--port")
            .arg(origin_port.to_string())
            .arg("--mode")
            .arg(mode);
        let output = run_command(command, "real portable browser-state probe")?;
        let observed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let expected = serde_json::json!({
            "cookie": true,
            "indexed_db": true,
            "local_storage": true,
            "mode": mode,
            "origin_port": origin_port,
        });
        if observed != expected {
            return Err(format!("unexpected portable browser-state report: {observed}").into());
        }
        Ok(())
    }

    fn reserve_origin_port() -> Result<u16, Box<dyn std::error::Error>> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        drop(listener);
        if port < 1024 {
            return Err("allocated browser-state origin port is outside the accepted range".into());
        }
        Ok(port)
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
        let bundle = approved_bundle()?;
        let real_identity = materialize_real_identity(&base_workspace, &device_id, &bundle)?;
        let origin_port = reserve_origin_port()?;

        let state = Rc::new(RefCell::new(BackendState::new()?));
        let transport = BackendMachineHttp {
            state: Rc::clone(&state),
        };
        let runtime_generations = Rc::new(RefCell::new(Vec::new()));
        let preflight: PreflightObservations = Rc::new(RefCell::new(Vec::new()));
        let enrollment = ControlPlaneEnrollment::new(transport.clone());
        let coordinator = ControlPlaneCoordinator::new(transport.clone());
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
            RecordingPreflight {
                observations: Rc::clone(&preflight),
            },
            FakeProcessControl::default(),
            FakeCamouhost::default(),
        );
        let mut downloader = VerifiedGenerationObjectDownloader::new(BackendGet {
            state: Rc::clone(&state),
        });

        let first_claim =
            ClaimUri::parse("profilebridge://claim/claim_s0_browser_state_first_000001")?;
        operator.open_authoritative(&first_claim, &root, &mut downloader, UnixMillis::new(10))?;
        let first_workspace = root.open_generation(&tenant_id, &profile_id, &base_generation)?;
        run_real_browser_state_probe(&first_workspace, &real_identity, origin_port, "seed")?;
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

        // Force the second launch to consume the exact committed encrypted object instead of any
        // locally retained successor workspace.
        root.reject_generation_for_rematerialization(&tenant_id, &profile_id, &successor)?;
        assert!(
            root.open_generation(&tenant_id, &profile_id, &successor)
                .is_err()
        );

        let second_claim =
            ClaimUri::parse("profilebridge://claim/claim_s0_browser_state_second_000002")?;
        operator.open_authoritative(&second_claim, &root, &mut downloader, UnixMillis::new(40))?;
        let reopened = root.open_generation(&tenant_id, &profile_id, &successor)?;
        run_real_browser_state_probe(&reopened, &real_identity, origin_port, "verify")?;

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
            "S0_REAL_PORTABLE_BROWSER_STATE=PASSED;COOKIE=1;LOCAL_STORAGE=1;INDEXED_DB=1;AUTHORITATIVE_RESTORE=1"
        );
        Ok(())
    }
}
