#![cfg(windows)]
#![forbid(unsafe_code)]

// One signed read-only R2 GET. This adapter does NOT select an object,
// handle a key, or publish Firefox bytes. It streams bytes into an unpublished
// bounded writer owned by generation_reopen, which independently checks
// HTTP 200, exact authoritative length, SHA-256 and fsync before use.
use crate::generation_reopen::SignedGenerationObjectStreamGetPort;
use crate::windows_generation_put::{
    inherit_windows_system_root, system_curl_executable, valid_signed_r2_url,
};
use bridge_domain::BridgePortError;
use encrypted_generation_domain::MAX_STREAM_CONTAINER_BYTES;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CURL_CONNECT_TIMEOUT_SECONDS: &str = "10";
const CURL_GET_TOTAL_TIMEOUT_SECONDS: &str = "240";
const HTTP_STATUS_MARKER: &[u8] = b"PROFILE_BRIDGE_HTTP_STATUS:";

/// Signed GET URLs are short-lived bearer capabilities and must never appear
/// in logs or Debug. The system CURL process receives only the URL and
/// environment needed for Windows root certificates.
#[derive(Clone)]
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

impl SignedGenerationObjectStreamGetPort for WindowsSignedGenerationObjectGet {
    type Error = BridgePortError;

    fn get_exact_stream(
        &mut self,
        signed_url: &str,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<u16, Self::Error> {
        if max_bytes == 0
            || max_bytes > MAX_STREAM_CONTAINER_BYTES
            || !valid_signed_r2_url(signed_url)
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let mut cmd: Command = crate::windows_native::background_command(&self.curl_executable);
        cmd.env_clear()
            .arg("--silent")
            .arg("--proto")
            .arg("=https")
            .arg("--max-redirs")
            .arg("0")
            .arg("--connect-timeout")
            .arg(CURL_CONNECT_TIMEOUT_SECONDS)
            .arg("--max-time")
            .arg(CURL_GET_TOTAL_TIMEOUT_SECONDS)
            .arg("--max-filesize")
            .arg(max_bytes.to_string())
            .arg("--noproxy")
            .arg("*")
            .arg("--request")
            .arg("GET")
            .arg("--url")
            .arg(signed_url)
            .arg("--output")
            .arg("-")
            // Only the exact three-digit status travels on stderr, never
            // mingled with the binary container on stdout.
            .arg("--write-out")
            .arg("%{stderr}PROFILE_BRIDGE_HTTP_STATUS:%{http_code}")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        inherit_windows_system_root(&mut cmd);
        let mut child = cmd.spawn().map_err(|_| BridgePortError::Unavailable)?;
        let result = (|| -> Result<(), BridgePortError> {
            let mut stdout = child.stdout.take().ok_or(BridgePortError::Unavailable)?;
            let mut read_total = 0_u64;
            let mut buffer = [0_u8; 65_536];
            loop {
                let count = stdout
                    .read(&mut buffer)
                    .map_err(|_| BridgePortError::Unavailable)?;
                if count == 0 {
                    break;
                }
                read_total = read_total
                    .checked_add(count as u64)
                    .ok_or(BridgePortError::InvalidResponse)?;
                if read_total > max_bytes {
                    return Err(BridgePortError::InvalidResponse);
                }
                sink.write_all(&buffer[..count])
                    .map_err(|_| BridgePortError::InvalidResponse)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let output = child
            .wait_with_output()
            .map_err(|_| BridgePortError::Unavailable)?;
        if !output.status.success() || !output.stdout.is_empty() {
            return Err(BridgePortError::Unavailable);
        }
        decode_exact_get_status(&output.stderr)
    }
}

fn decode_exact_get_status(stderr: &[u8]) -> Result<u16, BridgePortError> {
    if !stderr.starts_with(HTTP_STATUS_MARKER) || stderr.len() != HTTP_STATUS_MARKER.len() + 3 {
        return Err(BridgePortError::InvalidResponse);
    }
    let digits = &stderr[HTTP_STATUS_MARKER.len()..];
    if !digits.iter().all(u8::is_ascii_digit) {
        return Err(BridgePortError::InvalidResponse);
    }
    let status = std::str::from_utf8(digits)
        .map_err(|_| BridgePortError::InvalidResponse)?
        .parse::<u16>()
        .map_err(|_| BridgePortError::InvalidResponse)?;
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::decode_exact_get_status;

    #[test]
    fn only_one_unambiguous_out_of_band_http_status_is_accepted() {
        assert_eq!(
            decode_exact_get_status(b"PROFILE_BRIDGE_HTTP_STATUS:200"),
            Ok(200),
        );
        for malformed in [
            b"PROFILE_BRIDGE_HTTP_STATUS:20".as_slice(),
            b"PROFILE_BRIDGE_HTTP_STATUS:200\n".as_slice(),
            b"redirect PROFILE_BRIDGE_HTTP_STATUS:200".as_slice(),
            b"PROFILE_BRIDGE_HTTP_STATUS:2xx".as_slice(),
            b"PROFILE_BRIDGE_HTTP_STATUS:200PROFILE_BRIDGE_HTTP_STATUS:200".as_slice(),
        ] {
            assert!(decode_exact_get_status(malformed).is_err());
        }
    }
}
