#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::shipping_generation_save::{
    SignedGenerationObjectPutPort, SignedGenerationUploadCapability,
};
use bridge_domain::BridgePortError;
use encrypted_generation_domain::MAX_GENERATION_CONTAINER_BYTES;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAX_SIGNED_GENERATION_URL_BYTES: usize = 8_192;
const MAX_SIGNED_UPLOAD_HEADERS: usize = 16;
const MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES: usize = 128;
const MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES: usize = 1_024;
const MAX_PUT_RESPONSE_BYTES: usize = 1_024;
const CURL_CONNECT_TIMEOUT_SECONDS: &str = "10";
const CURL_GENERATION_TOTAL_TIMEOUT_SECONDS: &str = "120";
const HTTP_STATUS_MARKER: &[u8] = b"\nPROFILE_BRIDGE_HTTP_STATUS:";

/// Effect-only Windows adapter for one server-issued immutable generation PUT capability.
///
/// This adapter owns no R2 credentials, machine certificate, descriptor semantics or verification.
/// It sends exactly the backend-issued signed headers and container bytes only to the canonical
/// direct R2 HTTPS authority, never follows redirects, and treats every non-2xx response as a
/// pre-commit failure. Exact object verification remains server-owned and is required again before
/// successor commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsSignedGenerationObjectPut {
    curl_executable: PathBuf,
}

impl WindowsSignedGenerationObjectPut {
    pub fn from_system() -> Result<Self, BridgePortError> {
        Ok(Self {
            curl_executable: system_curl_executable()?,
        })
    }
}

impl SignedGenerationObjectPutPort for WindowsSignedGenerationObjectPut {
    type Error = BridgePortError;

    fn put_exact(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        container: &[u8],
    ) -> Result<(), Self::Error> {
        if container.is_empty() || container.len() > MAX_GENERATION_CONTAINER_BYTES {
            return Err(BridgePortError::InvalidResponse);
        }
        let signed_url = capability
            .url()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if !valid_signed_r2_put_url(signed_url) {
            return Err(BridgePortError::InvalidResponse);
        }
        let headers = capability
            .headers()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if headers.is_empty() || headers.len() > MAX_SIGNED_UPLOAD_HEADERS {
            return Err(BridgePortError::InvalidResponse);
        }
        let mut names = BTreeSet::new();
        for (name, value) in &headers {
            if !valid_header_name(name)
                || !valid_header_value(value)
                || !names.insert((*name).to_owned())
            {
                return Err(BridgePortError::InvalidResponse);
            }
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
            .arg(MAX_PUT_RESPONSE_BYTES.to_string())
            .arg("--noproxy")
            .arg("*")
            .arg("--request")
            .arg("PUT")
            .arg("--url")
            .arg(signed_url)
            .arg("--data-binary")
            .arg("@-")
            .arg("--write-out")
            .arg("\nPROFILE_BRIDGE_HTTP_STATUS:%{http_code}")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (name, value) in headers {
            command.arg("--header").arg(format!("{name}: {value}"));
        }
        inherit_windows_system_root(&mut command);

        let mut child = command.spawn().map_err(|_| BridgePortError::Unavailable)?;
        child
            .stdin
            .take()
            .ok_or(BridgePortError::Unavailable)?
            .write_all(container)
            .map_err(|_| BridgePortError::Unavailable)?;
        let output = child
            .wait_with_output()
            .map_err(|_| BridgePortError::Unavailable)?;
        let (status, body) = decode_curl_http_output(output)?;
        if !(200..300).contains(&status) || !body.is_empty() {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }
}

fn decode_curl_http_output(output: Output) -> Result<(u16, Vec<u8>), BridgePortError> {
    let max_output_bytes = MAX_PUT_RESPONSE_BYTES
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
    if marker > MAX_PUT_RESPONSE_BYTES {
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

fn valid_signed_r2_put_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_SIGNED_GENERATION_URL_BYTES
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || value.contains('#')
    {
        return false;
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, path_and_query)) = rest.split_once('/') else {
        return false;
    };
    const R2_SUFFIX: &str = ".r2.cloudflarestorage.com";
    let Some(account_id) = authority.strip_suffix(R2_SUFFIX) else {
        return false;
    };
    if account_id.len() != 32
        || !account_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || authority.contains('@')
        || authority.contains(':')
    {
        return false;
    }
    let Some((path, query)) = path_and_query.split_once('?') else {
        return false;
    };
    if path.is_empty() || query.is_empty() {
        return false;
    }
    let required = [
        "X-Amz-Algorithm=AWS4-HMAC-SHA256",
        "X-Amz-Credential=",
        "X-Amz-Date=",
        "X-Amz-Expires=",
        "X-Amz-SignedHeaders=",
        "X-Amz-Signature=",
    ];
    required.iter().all(|needle| query.contains(needle))
}

fn valid_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES
        && !value
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0)
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
    use super::{valid_header_name, valid_header_value, valid_signed_r2_put_url};

    #[test]
    fn signed_put_url_requires_direct_canonical_r2_authority() {
        let good = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/profile-generations/tenants/tenant_01/profiles/profile_01/generations/generation_01.bpgc?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=access%2F20260830%2Fauto%2Fs3%2Faws4_request&X-Amz-Date=20260830T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=content-type%3Bhost&X-Amz-Signature=abc";
        assert!(valid_signed_r2_put_url(good));
        for bad in [
            good.replacen("https://", "http://", 1),
            good.replacen(".r2.cloudflarestorage.com", ".example.com", 1),
            format!("{good}#fragment"),
            good.replacen("X-Amz-Signature=abc", "signature=abc", 1),
            good.replacen(
                "0123456789abcdef0123456789abcdef",
                "ABCDEF0123456789ABCDEF0123456789",
                1,
            ),
        ] {
            assert!(!valid_signed_r2_put_url(&bad));
        }
    }

    #[test]
    fn signed_put_headers_reject_injection_and_duplicates_are_checked_by_adapter() {
        assert!(valid_header_name("x-amz-checksum-sha256"));
        assert!(!valid_header_name("X-Amz-Checksum-Sha256"));
        assert!(valid_header_value("abc+/=="));
        assert!(!valid_header_value("value\r\ninjected: true"));
    }
}
