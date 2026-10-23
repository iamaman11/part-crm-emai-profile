//! First identity materialization belongs to the existing pinned Camouhost process owner.
use super::*;
use crate::shipping_preflight::profile_stable_identity_from_config_bytes;
use browser_execution_domain::BrowserIdentityManifest;
use serde::Deserialize;
use std::io::{BufRead, BufReader, Read};

fn read_initial_report(reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    BufReader::new(reader.take(8193)).read_until(b'\n', &mut bytes)?;
    if bytes.len() > 8192 || bytes.last() != Some(&b'\n') {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitialReport {
    fingerprint_config_sha256: String,
    fingerprint_policy_version: String,
    profile_stable_probe_sha256: String,
    runtime_lock_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitialFailureReport {
    failure: InitialFailure,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitialFailure {
    category: FailureCategory,
    line: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FailureCategory {
    Contract,
    Timeout,
    Io,
    BrowserClosed,
    Unexpected,
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
            .arg("-B")
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
        let mut child = InitialChild(command.spawn().map_err(|error| {
            eprintln!("INITIAL_IDENTITY_FAILURE=spawn;category={:?}", error.kind());
            BridgePortError::Unavailable
        })?);
        let stdout = child.0.stdout.take().ok_or(BridgePortError::Unavailable)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let _ = sender.send(read_initial_report(stdout));
        });
        let started = Instant::now();
        let status = loop {
            match child.0.try_wait().map_err(|error| {
                eprintln!(
                    "INITIAL_IDENTITY_FAILURE=child_wait;category={:?}",
                    error.kind()
                );
                BridgePortError::Unavailable
            })? {
                Some(status) => {
                    if !status.success() {
                        eprintln!(
                            "INITIAL_IDENTITY_FAILURE=child_exit;code={:?}",
                            status.code()
                        );
                    }
                    break status;
                }
                None if started.elapsed() >= Duration::from_secs(180) => {
                    eprintln!("INITIAL_IDENTITY_FAILURE=child_timeout");
                    return Err(BridgePortError::Unavailable);
                }
                None => thread::sleep(PROCESS_POLL_INTERVAL),
            }
        };
        let bytes = receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| {
                eprintln!("INITIAL_IDENTITY_FAILURE=report_receive;category={error:?}");
                BridgePortError::Unavailable
            })?
            .map_err(|error| {
                eprintln!(
                    "INITIAL_IDENTITY_FAILURE=report_read;category={:?}",
                    error.kind()
                );
                if error.kind() == std::io::ErrorKind::InvalidData {
                    BridgePortError::InvalidResponse
                } else {
                    BridgePortError::Unavailable
                }
            })?;
        if !status.success() {
            if let Ok(report) = serde_json::from_slice::<InitialFailureReport>(&bytes) {
                eprintln!(
                    "INITIAL_IDENTITY_FAILURE=child_report;category={:?};runtime_source_line={}",
                    report.failure.category, report.failure.line
                );
            }
            return Err(BridgePortError::InvalidResponse);
        }
        let report: InitialReport = serde_json::from_slice(&bytes).map_err(|error| {
            eprintln!(
                "INITIAL_IDENTITY_FAILURE=report_json;category={:?}",
                error.classify()
            );
            BridgePortError::InvalidResponse
        })?;
        if report.runtime_lock_sha256 != lock_digest
            || report.fingerprint_policy_version != "profile-stability-v1"
        {
            eprintln!("INITIAL_IDENTITY_FAILURE=report_identity");
            return Err(BridgePortError::InvalidResponse);
        }
        verify_file_digest(
            &workspace.path().join(CONFIG_NAME),
            &report.fingerprint_config_sha256,
        )
        .inspect_err(|error| {
            eprintln!("INITIAL_IDENTITY_FAILURE=config_integrity;category={error:?}");
        })?;
        let config = fs::read(workspace.path().join(CONFIG_NAME)).map_err(|error| {
            eprintln!(
                "INITIAL_IDENTITY_FAILURE=config_read;category={:?}",
                error.kind()
            );
            BridgePortError::Unavailable
        })?;
        let stable = profile_stable_identity_from_config_bytes(&config).map_err(|error| {
            eprintln!("INITIAL_IDENTITY_FAILURE=config_projection;category={error:?}");
            BridgePortError::InvalidResponse
        })?;
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
        .map_err(|error| {
            eprintln!("INITIAL_IDENTITY_FAILURE=identity_manifest;category={error:?}");
            BridgePortError::InvalidResponse
        })
    }
}

#[cfg(test)]
mod tests {
    use super::read_initial_report;
    use std::io::{Cursor, ErrorKind, Read};

    struct OpenStream(Cursor<&'static [u8]>);
    impl Read for OpenStream {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let count = self.0.read(buffer)?;
            if count == 0 {
                return Err(ErrorKind::WouldBlock.into());
            }
            Ok(count)
        }
    }

    #[test]
    fn complete_report_does_not_require_descendant_pipe_eof() -> std::io::Result<()> {
        let frame = b"{\"report\":true}\n";
        assert_eq!(
            read_initial_report(OpenStream(Cursor::new(frame.as_slice())))?,
            frame
        );
        Ok(())
    }

    #[test]
    fn incomplete_and_oversized_report_frames_are_rejected() {
        for frame in [b"{}".to_vec(), vec![b'x'; 8193]] {
            assert!(matches!(
                read_initial_report(Cursor::new(frame)),
                Err(error) if error.kind() == ErrorKind::InvalidData
            ));
        }
    }
}
