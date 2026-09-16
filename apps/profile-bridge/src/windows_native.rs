#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::generation_reopen::{SignedGenerationObjectGetPort, SignedGenerationObjectGetResponse};
use bridge_domain::{BridgePortError, DeviceIdentityPort};
use encrypted_generation_domain::MAX_GENERATION_CONTAINER_BYTES;
use profile_platform_primitives::DeviceId;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAX_SIGNED_GENERATION_URL_BYTES: usize = 8_192;
const CURL_CONNECT_TIMEOUT_SECONDS: &str = "10";
const CURL_GENERATION_TOTAL_TIMEOUT_SECONDS: &str = "120";
const HTTP_STATUS_MARKER: &[u8] = b"\nPROFILE_BRIDGE_HTTP_STATUS:";

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
/// one direct GET for the exact signed URL and never enables curl redirect following. Descriptor,
/// size and digest verification remain owned by `VerifiedGenerationObjectDownloader`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsSignedGenerationObjectGet {
    curl_executable: PathBuf,
}

impl WindowsSignedGenerationObjectGet {
    pub fn from_system() -> Result<Self, BridgePortError> {
        Ok(Self {
            curl_executable: system_curl_executable()?,
        })
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
        let mut command = Command::new(&self.curl_executable);
        command
            .env_clear()
            .arg("--silent")
            .arg("--show-error")
            .arg("--proto")
            .arg("=https")
            .arg("--max-redirs")
            .arg("0")
            .arg("--connect-timeout")
            .arg(CURL_CONNECT_TIMEOUT_SECONDS)
            .arg("--max-time")
            .arg(CURL_GENERATION_TOTAL_TIMEOUT_SECONDS)
            .arg("--max-filesize")
            .arg(max_bytes.to_string())
            .arg("--noproxy")
            .arg("*")
            .arg("--request")
            .arg("GET")
            .arg("--url")
            .arg(signed_url)
            .arg("--write-out")
            .arg("\nPROFILE_BRIDGE_HTTP_STATUS:%{http_code}")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        inherit_windows_system_root(&mut command);

        let output = command
            .spawn()
            .and_then(|child| child.wait_with_output())
            .map_err(|_| BridgePortError::Unavailable)?;
        let (status, body) = decode_curl_http_output(output, max_bytes)?;
        Ok(SignedGenerationObjectGetResponse::new(status, body))
    }
}

fn decode_curl_http_output(
    output: Output,
    max_body_bytes: usize,
) -> Result<(u16, Vec<u8>), BridgePortError> {
    let max_output_bytes = max_body_bytes
        .checked_add(HTTP_STATUS_MARKER.len())
        .and_then(|value| value.checked_add(3))
        .ok_or(BridgePortError::InvalidResponse)?;
    if !output.status.success() || output.stdout.len() > max_output_bytes {
        return Err(BridgePortError::Unavailable);
    }
    let marker = output
        .stdout
        .windows(HTTP_STATUS_MARKER.len())
        .rposition(|window| window == HTTP_STATUS_MARKER)
        .ok_or(BridgePortError::InvalidResponse)?;
    if marker > max_body_bytes {
        return Err(BridgePortError::Unavailable);
    }
    let status_bytes = &output.stdout[marker + HTTP_STATUS_MARKER.len()..];
    if status_bytes.len() != 3 || !status_bytes.iter().all(u8::is_ascii_digit) {
        return Err(BridgePortError::InvalidResponse);
    }
    let status = std::str::from_utf8(status_bytes)
        .map_err(|_| BridgePortError::InvalidResponse)?
        .parse::<u16>()
        .map_err(|_| BridgePortError::InvalidResponse)?;
    Ok((status, output.stdout[..marker].to_vec()))
}

fn system_curl_executable() -> Result<PathBuf, BridgePortError> {
    let system_root = env::var_os("SystemRoot").ok_or(BridgePortError::Unavailable)?;
    let curl_executable = PathBuf::from(system_root).join("System32").join("curl.exe");
    validate_regular_absolute_file(&curl_executable)?;
    Ok(curl_executable)
}

fn inherit_windows_system_root(command: &mut Command) {
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
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

fn validate_regular_absolute_file(path: &Path) -> Result<(), BridgePortError> {
    if !path.is_absolute() {
        return Err(BridgePortError::InvalidResponse);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| BridgePortError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{encode_wide_argument, valid_direct_https_url};
    use std::ffi::OsStr;

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
