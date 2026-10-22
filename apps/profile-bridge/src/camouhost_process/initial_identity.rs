//! First identity materialization belongs to the existing pinned Camouhost process owner.
use super::*;
use crate::shipping_preflight::profile_stable_identity_from_config_bytes;
use browser_execution_domain::BrowserIdentityManifest;
use serde::Deserialize;
use std::io::Read;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitialReport {
    fingerprint_config_sha256: String,
    fingerprint_policy_version: String,
    profile_stable_probe_sha256: String,
    runtime_lock_sha256: String,
}

struct InitialChild(Child);
impl Drop for InitialChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl ManagedCamouhostConfig {
    pub fn materialize_initial_identity(
        &self,
        workspace: &GenerationWorkspace,
        bundle: &ApprovedRuntimeBundle,
    ) -> Result<BrowserIdentityManifest, BridgePortError> {
        let entrypoint = self.runtime_root.join(REAL_ENTRYPOINT);
        let runtime_lock = self.runtime_root.join(RUNTIME_LOCK_PATH);
        verify_file_digest(
            &entrypoint,
            inventory_digest(bundle, REAL_ENTRYPOINT)
                .map_err(|_| BridgePortError::InvalidResponse)?,
        )?;
        let lock_digest = inventory_digest(bundle, RUNTIME_LOCK_PATH)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        verify_file_digest(&runtime_lock, lock_digest)?;
        #[cfg(windows)]
        {
            verify_file_digest(
                &self.python_executable,
                inventory_digest(bundle, WINDOWS_PYTHON_EXECUTABLE)
                    .map_err(|_| BridgePortError::InvalidResponse)?,
            )?;
            verify_file_digest(
                &self.runtime_root.join(WINDOWS_BROWSER_EXECUTABLE),
                inventory_digest(bundle, WINDOWS_BROWSER_EXECUTABLE)
                    .map_err(|_| BridgePortError::InvalidResponse)?,
            )?;
        }
        let mut command = Command::new(&self.python_executable);
        command
            .arg(&entrypoint)
            .arg("--materialize-identity")
            .arg(workspace.path())
            .current_dir(&self.runtime_root)
            .env_clear()
            .env("CAMOUHOST_RUNTIME_LOCK", &runtime_lock)
            .env(
                "CAMOUHOST_HEADLESS_MODE",
                match self.display_mode {
                    RuntimeDisplayMode::Headful => "false",
                    RuntimeDisplayMode::VirtualHeadful => "virtual",
                },
            )
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        inherit_runtime_environment(&mut command);
        let mut child = InitialChild(command.spawn().map_err(|_| BridgePortError::Unavailable)?);
        let stdout = child.0.stdout.take().ok_or(BridgePortError::Unavailable)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take(8193).read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        });
        let started = Instant::now();
        loop {
            match child
                .0
                .try_wait()
                .map_err(|_| BridgePortError::Unavailable)?
            {
                Some(status) if status.success() => break,
                Some(_) => return Err(BridgePortError::InvalidResponse),
                None if started.elapsed() >= Duration::from_secs(180) => {
                    return Err(BridgePortError::Unavailable);
                }
                None => thread::sleep(PROCESS_POLL_INTERVAL),
            }
        }
        let bytes = receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| BridgePortError::Unavailable)?
            .map_err(|_| BridgePortError::Unavailable)?;
        if bytes.len() > 8192 {
            return Err(BridgePortError::InvalidResponse);
        }
        let report: InitialReport =
            serde_json::from_slice(&bytes).map_err(|_| BridgePortError::InvalidResponse)?;
        if report.runtime_lock_sha256 != lock_digest
            || report.fingerprint_policy_version != "profile-stability-v1"
        {
            return Err(BridgePortError::InvalidResponse);
        }
        verify_file_digest(
            &workspace.path().join(CONFIG_NAME),
            &report.fingerprint_config_sha256,
        )?;
        let config = fs::read(workspace.path().join(CONFIG_NAME))
            .map_err(|_| BridgePortError::Unavailable)?;
        let stable = profile_stable_identity_from_config_bytes(&config)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        BrowserIdentityManifest::new(
            IDENTITY_COMPATIBILITY_VERSION,
            report.fingerprint_policy_version,
            bundle.manifest().runtime_version(),
            bundle.manifest().inventory_sha256().as_str(),
            format!(
                "{FINGERPRINT_SOURCE_PREFIX}{}",
                report.profile_stable_probe_sha256
            ),
            report.fingerprint_config_sha256,
            stable,
        )
        .map_err(|_| BridgePortError::InvalidResponse)
    }
}
