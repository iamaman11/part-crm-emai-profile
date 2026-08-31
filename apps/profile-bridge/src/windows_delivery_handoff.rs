#![forbid(unsafe_code)]

use crate::windows_delivery::DeliveryIdentity;
use crate::windows_delivery_recovery::VerifiedDeliveryRecoveryTarget;
use crate::windows_delivery_staging::StagedDelivery;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Child;
#[cfg(any(windows, test))]
use std::process::Command;

const PROFILE_BRIDGE_EXECUTABLE: &str = "profile-bridge.exe";
pub const HANDOFF_ARRIVAL_ARGUMENT: &str = "--delivery-handoff-arrived";
#[cfg(any(windows, test))]
const PARENT_PID_ENV: &str = "PART_CRM_DELIVERY_HANDOFF_PARENT_PID";
#[cfg(any(windows, test))]
const TARGET_EXECUTABLE_ENV: &str = "PART_CRM_DELIVERY_HANDOFF_TARGET";
#[cfg(any(windows, test))]
const POWERSHELL_HANDOFF: &str = "$ErrorActionPreference='Stop'; $pidToWait=[uint32]$env:PART_CRM_DELIVERY_HANDOFF_PARENT_PID; Wait-Process -Id $pidToWait -ErrorAction SilentlyContinue; $target=$env:PART_CRM_DELIVERY_HANDOFF_TARGET; if ([string]::IsNullOrWhiteSpace($target)) { exit 21 }; $child=Start-Process -FilePath $target -ArgumentList '--delivery-handoff-arrived' -PassThru; if ($null -eq $child) { exit 22 }";

/// Exact filesystem/process identity already selected by the Windows delivery owner.
///
/// This value never selects a candidate. It can only be built from a stage that has already passed
/// the canonical staging verifier, or from R8's exact verified recovery target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDeliveryProcessTarget {
    identity: DeliveryIdentity,
    release_root: PathBuf,
    profile_bridge_executable: PathBuf,
}

impl VerifiedDeliveryProcessTarget {
    pub fn from_staged(staged: &StagedDelivery) -> Result<Self, DeliveryHandoffError> {
        let release_root =
            fs::canonicalize(staged.path()).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
        let executable = staged.profile_bridge_root().join(PROFILE_BRIDGE_EXECUTABLE);
        let executable = canonical_regular_executable(&executable)?;
        if executable.parent().and_then(Path::parent) != Some(release_root.as_path()) {
            return Err(DeliveryHandoffError::InvalidTarget);
        }
        Ok(Self {
            identity: staged.identity().clone(),
            release_root,
            profile_bridge_executable: executable,
        })
    }

    pub fn from_recovery(
        target: &VerifiedDeliveryRecoveryTarget,
    ) -> Result<Self, DeliveryHandoffError> {
        let release_root = fs::canonicalize(target.release_root())
            .map_err(|_| DeliveryHandoffError::InvalidTarget)?;
        let executable = canonical_regular_executable(target.profile_bridge_executable())?;
        if executable.parent().and_then(Path::parent) != Some(release_root.as_path()) {
            return Err(DeliveryHandoffError::InvalidTarget);
        }
        Ok(Self {
            identity: target.identity().clone(),
            release_root,
            profile_bridge_executable: executable,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> &DeliveryIdentity {
        &self.identity
    }

    #[must_use]
    pub fn release_root(&self) -> &Path {
        &self.release_root
    }

    #[must_use]
    pub fn profile_bridge_executable(&self) -> &Path {
        &self.profile_bridge_executable
    }
}

/// One mechanical same-binary transfer. The request owns no release/trust/lifecycle policy and
/// carries no claim credential. The child helper waits for this exact process to disappear before
/// starting the already-verified target in bounded arrival mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OneShotDeliveryHandoff {
    current_process_id: u32,
    current_executable: PathBuf,
    target: VerifiedDeliveryProcessTarget,
}

impl OneShotDeliveryHandoff {
    pub fn new(
        current_process_id: u32,
        current_executable: impl AsRef<Path>,
        target: VerifiedDeliveryProcessTarget,
    ) -> Result<Self, DeliveryHandoffError> {
        if current_process_id == 0 {
            return Err(DeliveryHandoffError::InvalidCurrentProcess);
        }
        let current_executable = canonical_regular_executable(current_executable.as_ref())?;
        if current_executable.file_name().and_then(OsStr::to_str)
            != Some(PROFILE_BRIDGE_EXECUTABLE)
            || current_executable == target.profile_bridge_executable
        {
            return Err(DeliveryHandoffError::InvalidCurrentProcess);
        }
        Ok(Self {
            current_process_id,
            current_executable,
            target,
        })
    }

    #[must_use]
    pub const fn current_process_id(&self) -> u32 {
        self.current_process_id
    }

    #[must_use]
    pub fn current_executable(&self) -> &Path {
        &self.current_executable
    }

    #[must_use]
    pub const fn target(&self) -> &VerifiedDeliveryProcessTarget {
        &self.target
    }

    #[cfg(windows)]
    pub fn schedule(self) -> Result<Child, DeliveryHandoffError> {
        let mut command = self.windows_helper_command();
        command
            .spawn()
            .map_err(|_| DeliveryHandoffError::HelperLaunchFailed)
    }

    #[cfg(any(windows, test))]
    fn windows_helper_command(&self) -> Command {
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                POWERSHELL_HANDOFF,
            ])
            .env(PARENT_PID_ENV, self.current_process_id.to_string())
            .env(TARGET_EXECUTABLE_ENV, &self.target.profile_bridge_executable);
        command
    }
}

fn canonical_regular_executable(path: &Path) -> Result<PathBuf, DeliveryHandoffError> {
    if !path.is_absolute() {
        return Err(DeliveryHandoffError::InvalidTarget);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DeliveryHandoffError::InvalidTarget);
    }
    let canonical = fs::canonicalize(path).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
    let canonical_metadata =
        fs::symlink_metadata(&canonical).map_err(|_| DeliveryHandoffError::InvalidTarget)?;
    if canonical_metadata.file_type().is_symlink() || !canonical_metadata.is_file() {
        return Err(DeliveryHandoffError::InvalidTarget);
    }
    Ok(canonical)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryHandoffError {
    InvalidTarget,
    InvalidCurrentProcess,
    HelperLaunchFailed,
}

impl fmt::Display for DeliveryHandoffError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTarget => "Windows delivery handoff target is invalid",
            Self::InvalidCurrentProcess => "Windows delivery handoff current process is invalid",
            Self::HelperLaunchFailed => "Windows delivery one-shot helper could not be started",
        })
    }
}

impl std::error::Error for DeliveryHandoffError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_delivery::DeliveryIdentity;
    use std::io;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-handoff-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn identity() -> DeliveryIdentity {
        DeliveryIdentity {
            sequence: 2,
            release_set_id: format!("release-set-v3-sha256-{}", "1".repeat(64)),
            manifest_sha256: "2".repeat(64),
            profile_bridge_release_id: format!("profile-bridge-v2-sha256-{}", "3".repeat(64)),
            runtime_bundle_release_id: format!("runtime-bundle-v2-sha256-{}", "4".repeat(64)),
        }
    }

    fn fixture_target(
        directory: &TestDirectory,
    ) -> Result<VerifiedDeliveryProcessTarget, io::Error> {
        let release = directory.0.join("releases").join("candidate");
        let bridge = release.join("profile-bridge");
        fs::create_dir_all(&bridge)?;
        let executable = bridge.join(PROFILE_BRIDGE_EXECUTABLE);
        fs::write(&executable, b"candidate")?;
        Ok(VerifiedDeliveryProcessTarget {
            identity: identity(),
            release_root: fs::canonicalize(release)?,
            profile_bridge_executable: fs::canonicalize(executable)?,
        })
    }

    #[test]
    fn one_shot_plan_binds_exact_old_pid_and_target_without_claim(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::create("plan")?;
        let current_root = directory
            .0
            .join("releases")
            .join("current")
            .join("profile-bridge");
        fs::create_dir_all(&current_root)?;
        let current = current_root.join(PROFILE_BRIDGE_EXECUTABLE);
        fs::write(&current, b"current")?;
        let handoff = OneShotDeliveryHandoff::new(42, &current, fixture_target(&directory)?)?;
        let command = handoff.windows_helper_command();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(command.get_program(), "powershell.exe");
        assert!(args.iter().any(|arg| arg == POWERSHELL_HANDOFF));
        assert!(
            args.iter()
                .all(|arg| !arg.contains("part-crm-bridge://claim"))
        );
        assert_eq!(handoff.current_process_id(), 42);
        assert_eq!(handoff.target().identity(), &identity());
        Ok(())
    }

    #[test]
    fn target_substitution_or_same_executable_fails_closed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::create("reject")?;
        let target = fixture_target(&directory)?;
        assert_eq!(
            OneShotDeliveryHandoff::new(0, target.profile_bridge_executable(), target.clone()),
            Err(DeliveryHandoffError::InvalidCurrentProcess)
        );
        assert_eq!(
            OneShotDeliveryHandoff::new(7, target.profile_bridge_executable(), target),
            Err(DeliveryHandoffError::InvalidCurrentProcess)
        );
        Ok(())
    }
}
