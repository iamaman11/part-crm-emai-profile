#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::generation_reopen::{SignedGenerationObjectGetPort, SignedGenerationObjectGetResponse};
use bridge_domain::{BridgePortError, DeviceIdentityPort};
use encrypted_generation_domain::MAX_GENERATION_CONTAINER_BYTES;
use profile_platform_primitives::DeviceId;
use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::process::Command;

const MAX_SIGNED_GENERATION_URL_BYTES: usize = 8_192;

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

/// Native effect for the server-issued immutable R2 generation capability.
///
/// This adapter deliberately has no machine certificate and no R2 credential fields. It performs
/// one direct GET for the exact signed URL and never enables redirect following. Descriptor,
/// size and digest verification remain owned by `VerifiedGenerationObjectDownloader`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsSignedGenerationObjectGet {}

impl WindowsSignedGenerationObjectGet {
    pub fn from_system() -> Result<Self, BridgePortError> {
        Ok(Self {})
    }
}

impl SignedGenerationObjectGetPort for WindowsSignedGenerationObjectGet {
    type Error = BridgePortError;
    fn get_exact(
        &mut self,
        signed_url: &str,
        max_bytes: usize,
    ) -> Result<SignedGenerationObjectGetResponse, Self::Error> {
        if !valid_direct_https_url(signed_url)
            || max_bytes == 0
            || max_bytes > MAX_GENERATION_CONTAINER_BYTES
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let request = crate::native_transport::shared()?
            .client
            .get(signed_url)
            .timeout(std::time::Duration::from_secs(120));
        let (status, body) = crate::native_transport::buffered(
            request,
            max_bytes,
            crate::native_transport::HttpOperation::GenerationGet,
            None,
        )?;
        Ok(SignedGenerationObjectGetResponse::new(status, body))
    }
}

fn valid_direct_https_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    !rest.is_empty()
        && value.len() <= MAX_SIGNED_GENERATION_URL_BYTES
        && !value.contains('#')
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::{encode_wide_argument, valid_direct_https_url};
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

    #[test]
    fn signed_generation_get_accepts_only_direct_https_url_shape() {
        let good = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/object?X-Amz-Signature=abc";
        assert!(valid_direct_https_url(good));
        assert!(!valid_direct_https_url(
            &good.replacen("https://", "http://", 1)
        ));
        assert!(!valid_direct_https_url(&format!("{good}#fragment")));
        assert!(!valid_direct_https_url("https://"));
        assert!(!valid_direct_https_url(
            "https://example.com/object\r\nheader:value"
        ));
    }
}
