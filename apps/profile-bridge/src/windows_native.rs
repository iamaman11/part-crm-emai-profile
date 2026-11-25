#![cfg(windows)]
#![forbid(unsafe_code)]

use bridge_domain::{BridgePortError, DeviceIdentityPort};
use profile_platform_primitives::DeviceId;
use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::process::Command;

pub(crate) fn background_command(executable: impl AsRef<OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(executable);
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW; helpers have no user interface.
    command
}

#[must_use]
pub fn encode_wide_argument(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(iter::once(0)).collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsDeviceIdentity {
    device_id: DeviceId,
}

impl WindowsDeviceIdentity {
    #[must_use]
    pub const fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }
}

impl DeviceIdentityPort for WindowsDeviceIdentity {
    fn device_id(&self) -> Result<DeviceId, BridgePortError> {
        Ok(self.device_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::encode_wide_argument;
    use std::ffi::OsStr;

    #[test]
    fn background_helpers_have_no_windows_console() -> Result<(), Box<dyn std::error::Error>> {
        let powershell =
            std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot")?)
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let output = super::background_command(powershell)
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
                "Add-Type -Name Native -Namespace BridgeConsoleTest -MemberDefinition '[System.Runtime.InteropServices.DllImport(\"kernel32.dll\")] public static extern System.IntPtr GetConsoleWindow();'; [Console]::Write([BridgeConsoleTest.Native]::GetConsoleWindow().ToInt64())"])
            .output()?;
        assert!(output.status.success());
        assert_eq!(output.stdout, b"0");
        Ok(())
    }

    #[test]
    fn windows_argument_encoding_is_nul_terminated_without_unsafe_code() {
        let encoded = encode_wide_argument(OsStr::new("profile-bridge"));
        assert_eq!(encoded.last(), Some(&0));
        assert!(encoded.len() > 1);
    }
}
