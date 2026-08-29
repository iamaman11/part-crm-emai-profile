#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::operator_flow::DeviceAuthenticationPort;
use crate::shipping_control_plane::{
    MachineHttpMethod, MachineHttpPort, MachineHttpResponse,
};
use bridge_domain::{BridgePortError, DeviceIdentityPort, DeviceKeyPort};
use profile_platform_primitives::{CorrelationId, DeviceId};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_MACHINE_RESPONSE_BYTES: usize = 65_536;
const CURL_CONNECT_TIMEOUT_SECONDS: &str = "10";
const CURL_TOTAL_TIMEOUT_SECONDS: &str = "20";
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsMachineCertificate {
    device_id: DeviceId,
    selector: String,
}

impl WindowsMachineCertificate {
    pub fn local_machine_my(
        device_id: DeviceId,
        sha1_thumbprint: &str,
    ) -> Result<Self, BridgePortError> {
        if sha1_thumbprint.len() != 40
            || !sha1_thumbprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(Self {
            device_id,
            selector: format!("LocalMachine\\MY\\{}", sha1_thumbprint.to_ascii_uppercase()),
        })
    }

    #[must_use]
    pub fn selector(&self) -> &str {
        &self.selector
    }
}

impl DeviceKeyPort for WindowsMachineCertificate {
    fn ensure_key_handle(&mut self, device_id: &DeviceId) -> Result<String, BridgePortError> {
        if device_id != &self.device_id {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(self.selector.clone())
    }
}

impl DeviceAuthenticationPort for WindowsMachineCertificate {
    type Error = BridgePortError;

    fn authenticate(&mut self, device_id: &DeviceId, key_handle: &str) -> Result<(), Self::Error> {
        if device_id != &self.device_id || key_handle != self.selector {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsSchannelMachineHttp {
    curl_executable: PathBuf,
    origin: String,
    certificate_selector: String,
}

impl WindowsSchannelMachineHttp {
    pub fn from_system(
        origin: impl Into<String>,
        certificate_selector: impl Into<String>,
    ) -> Result<Self, BridgePortError> {
        let origin = origin.into();
        validate_https_origin(&origin)?;
        let certificate_selector = certificate_selector.into();
        if !valid_certificate_selector(&certificate_selector) {
            return Err(BridgePortError::InvalidResponse);
        }
        let system_root = env::var_os("SystemRoot").ok_or(BridgePortError::Unavailable)?;
        let curl_executable = PathBuf::from(system_root).join("System32").join("curl.exe");
        validate_regular_absolute_file(&curl_executable)?;
        Ok(Self {
            curl_executable,
            origin,
            certificate_selector,
        })
    }

    fn request_inner(
        &self,
        method: MachineHttpMethod,
        path: &str,
        correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, BridgePortError> {
        if !valid_route_path(path)
            || matches!(method, MachineHttpMethod::Get) != body.is_none()
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
            .arg("--connect-timeout")
            .arg(CURL_CONNECT_TIMEOUT_SECONDS)
            .arg("--max-time")
            .arg(CURL_TOTAL_TIMEOUT_SECONDS)
            .arg("--max-filesize")
            .arg(MAX_MACHINE_RESPONSE_BYTES.to_string())
            .arg("--noproxy")
            .arg("*")
            .arg("--cert")
            .arg(&self.certificate_selector)
            .arg("--header")
            .arg("Accept: application/json")
            .arg("--header")
            .arg(format!("X-Correlation-Id: {}", correlation_id.as_str()))
            .arg("--request")
            .arg(match method {
                MachineHttpMethod::Get => "GET",
                MachineHttpMethod::PostJson => "POST",
            })
            .arg("--url")
            .arg(format!("{}{path}", self.origin))
            .arg("--write-out")
            .arg("\nPROFILE_BRIDGE_HTTP_STATUS:%{http_code}")
            .stdin(if body.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if body.is_some() {
            command
                .arg("--header")
                .arg("Content-Type: application/json")
                .arg("--data-binary")
                .arg("@-");
        }
        for key in ["SystemRoot", "WINDIR"] {
            if let Some(value) = env::var_os(key) {
                command.env(key, value);
            }
        }

        let mut child = command.spawn().map_err(|_| BridgePortError::Unavailable)?;
        if let Some(payload) = body {
            child
                .stdin
                .take()
                .ok_or(BridgePortError::Unavailable)?
                .write_all(payload)
                .map_err(|_| BridgePortError::Unavailable)?;
        }
        let output = child
            .wait_with_output()
            .map_err(|_| BridgePortError::Unavailable)?;
        if !output.status.success()
            || output.stdout.len() > MAX_MACHINE_RESPONSE_BYTES + HTTP_STATUS_MARKER.len() + 3
        {
            return Err(BridgePortError::Unavailable);
        }
        let marker = output
            .stdout
            .windows(HTTP_STATUS_MARKER.len())
            .rposition(|window| window == HTTP_STATUS_MARKER)
            .ok_or(BridgePortError::InvalidResponse)?;
        let body = output.stdout[..marker].to_vec();
        let status_bytes = &output.stdout[marker + HTTP_STATUS_MARKER.len()..];
        if status_bytes.len() != 3 || !status_bytes.iter().all(u8::is_ascii_digit) {
            return Err(BridgePortError::InvalidResponse);
        }
        let status = std::str::from_utf8(status_bytes)
            .map_err(|_| BridgePortError::InvalidResponse)?
            .parse::<u16>()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(MachineHttpResponse::new(status, body))
    }
}

impl MachineHttpPort for WindowsSchannelMachineHttp {
    type Error = BridgePortError;

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        self.request_inner(method, path, correlation_id, body)
    }
}

fn validate_https_origin(origin: &str) -> Result<(), BridgePortError> {
    let host = origin
        .strip_prefix("https://")
        .ok_or(BridgePortError::InvalidResponse)?;
    if host.is_empty()
        || host.len() > 253
        || host.contains(['/', '?', '#', '@', '\\'])
        || host.chars().any(char::is_whitespace)
        || !host.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        })
    {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(())
}

fn valid_certificate_selector(value: &str) -> bool {
    value
        .strip_prefix("LocalMachine\\MY\\")
        .is_some_and(|thumbprint| {
            thumbprint.len() == 40 && thumbprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn valid_route_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 512
        && !path.contains(['?', '#', '\\', '\r', '\n'])
        && !path.contains("//")
        && !path.split('/').any(|segment| segment == "." || segment == "..")
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
    use super::{
        WindowsMachineCertificate, encode_wide_argument, valid_certificate_selector,
        validate_https_origin,
    };
    use bridge_domain::{DeviceIdentityPort, DeviceKeyPort};
    use profile_platform_primitives::DeviceId;
    use std::ffi::OsStr;

    #[test]
    fn windows_argument_encoding_is_nul_terminated_without_unsafe_code() {
        let encoded = encode_wide_argument(OsStr::new("profile-bridge"));
        assert_eq!(encoded.last(), Some(&0));
        assert!(encoded.len() > 1);
    }

    #[test]
    fn machine_certificate_is_local_machine_store_handle()
    -> Result<(), Box<dyn std::error::Error>> {
        let device = DeviceId::parse("device_01JBRIDGE")?;
        let mut certificate = WindowsMachineCertificate::local_machine_my(
            device.clone(),
            "0123456789abcdef0123456789abcdef01234567",
        )?;
        let handle = certificate.ensure_key_handle(&device)?;
        assert!(valid_certificate_selector(&handle));
        assert!(handle.starts_with("LocalMachine\\MY\\"));
        Ok(())
    }

    #[test]
    fn machine_origin_is_https_origin_only() {
        assert!(validate_https_origin("https://control.example.com").is_ok());
        assert!(validate_https_origin("http://control.example.com").is_err());
        assert!(validate_https_origin("https://user@control.example.com").is_err());
        assert!(validate_https_origin("https://control.example.com/path").is_err());
    }
}
