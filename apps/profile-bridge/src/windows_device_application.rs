#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::operator_flow::DeviceAuthenticationPort;
use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort, MachineHttpResponse};
use bridge_domain::{BridgePortError, DeviceKeyPort};
use control_plane_contract::device_application_api::{
    DEVICE_APPLICATION_SESSION_HEADER, DEVICE_PROOF_NONCE_HEX_LENGTH,
    DEVICE_REQUEST_PROOF_EXPIRES_HEADER, DEVICE_REQUEST_PROOF_SIGNATURE_HEADER,
    DEVICE_SESSION_CHALLENGE_PATH_TEMPLATE, DEVICE_SESSION_COLLECTION_PATH_TEMPLATE,
    DeviceApplicationSessionProjection, DeviceProofChallengeProjection, OPAQUE_TOKEN_HEX_LENGTH,
};
use device_domain::{
    BRIDGE_REQUEST_PROOF_MAX_LIFETIME_MS, BridgeRequestProofMethod,
    bridge_request_proof_message_v1, device_proof_message_v1,
};
use profile_platform_primitives::{ActorId, CorrelationId, DeviceId, TenantId, UnixMillis};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_device_key::{P1363_SIGNATURE_BYTES, PersistedP256Key};
use zeroize::Zeroizing;

const MAX_ADMISSION_STATE_BYTES: u64 = 4_096;
const MAX_CONTROL_PLANE_RESPONSE_BYTES: usize = 65_536;
const HTTP_STATUS_MARKER: &[u8] = b"\nPROFILE_BRIDGE_HTTP_STATUS:";
const CURL_CONNECT_TIMEOUT_SECONDS: &str = "10";
const CURL_TOTAL_TIMEOUT_SECONDS: &str = "20";
const SESSION_RENEWAL_MARGIN_MS: u64 = 60_000;
const SESSION_MAX_LIFETIME_MS: u64 = 900_000;
const KEY_NAME_PREFIX: &str = "part-crm.device.";
const CURL_SESSION_ENV: &str = "PROFILE_BRIDGE_CURL_DEVICE_SESSION";
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsDeviceApplicationBinding {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BindingDocument {
    tenant_id: String,
    actor_id: String,
    device_id: String,
}

impl WindowsDeviceApplicationBinding {
    pub fn open(path: &Path) -> Result<Self, BridgePortError> {
        validate_regular_absolute_file(path, Some(MAX_ADMISSION_STATE_BYTES))?;
        let document = fs::read_to_string(path).map_err(|_| BridgePortError::Unavailable)?;
        if document.len() as u64 > MAX_ADMISSION_STATE_BYTES {
            return Err(BridgePortError::InvalidResponse);
        }
        let document = serde_json::from_str::<BindingDocument>(&document)
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(Self {
            tenant_id: TenantId::parse(document.tenant_id)
                .map_err(|_| BridgePortError::InvalidResponse)?,
            actor_id: ActorId::parse(document.actor_id)
                .map_err(|_| BridgePortError::InvalidResponse)?,
            device_id: DeviceId::parse(document.device_id)
                .map_err(|_| BridgePortError::InvalidResponse)?,
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    fn key_name(&self) -> Result<String, BridgePortError> {
        let value = format!("{KEY_NAME_PREFIX}{}", self.device_id.as_str());
        if value.len() > 128 {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(value)
    }
}

struct InMemoryApplicationSession {
    token: Zeroizing<String>,
    expires_at: UnixMillis,
}

impl Clone for InMemoryApplicationSession {
    fn clone(&self) -> Self {
        Self {
            token: Zeroizing::new(self.token.to_string()),
            expires_at: self.expires_at,
        }
    }
}

type SharedSession = Arc<Mutex<Option<InMemoryApplicationSession>>>;

#[derive(Clone)]
pub struct WindowsDeviceApplication {
    binding: WindowsDeviceApplicationBinding,
    key_name: String,
    transport: WindowsApplicationSessionHttp,
}

impl WindowsDeviceApplication {
    pub fn from_system(
        origin: impl Into<String>,
        binding: WindowsDeviceApplicationBinding,
    ) -> Result<Self, BridgePortError> {
        let key_name = binding.key_name()?;
        let session = Arc::new(Mutex::new(None));
        let transport = WindowsApplicationSessionHttp::from_system(
            origin,
            binding.clone(),
            key_name.clone(),
            session,
        )?;
        Ok(Self {
            binding,
            key_name,
            transport,
        })
    }

    #[must_use]
    pub fn transport(&self) -> WindowsApplicationSessionHttp {
        self.transport.clone()
    }
}

impl DeviceKeyPort for WindowsDeviceApplication {
    fn ensure_key_handle(&mut self, device_id: &DeviceId) -> Result<String, BridgePortError> {
        if device_id != self.binding.device_id() {
            return Err(BridgePortError::InvalidResponse);
        }
        let key = PersistedP256Key::open_or_create(&self.key_name)
            .map_err(|_| BridgePortError::Unavailable)?;
        key.require_non_exportable()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(self.key_name.clone())
    }
}

impl DeviceAuthenticationPort for WindowsDeviceApplication {
    type Error = BridgePortError;

    fn authenticate(&mut self, device_id: &DeviceId, key_handle: &str) -> Result<(), Self::Error> {
        if device_id != self.binding.device_id() || key_handle != self.key_name {
            return Err(BridgePortError::InvalidResponse);
        }
        self.transport.renew_application_session().map(|_| ())
    }
}

#[derive(Clone)]
pub struct WindowsApplicationSessionHttp {
    curl_executable: PathBuf,
    origin: String,
    binding: WindowsDeviceApplicationBinding,
    key_name: String,
    session: SharedSession,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionRenewRequest<'a> {
    challenge_token: &'a str,
    signature_p1363_hex: &'a str,
}

struct RequestProofHeaders<'a> {
    session_token: &'a str,
    expires_at: UnixMillis,
    signature_hex: String,
}

impl WindowsApplicationSessionHttp {
    fn from_system(
        origin: impl Into<String>,
        binding: WindowsDeviceApplicationBinding,
        key_name: String,
        session: SharedSession,
    ) -> Result<Self, BridgePortError> {
        let origin = origin.into();
        validate_https_origin(&origin)?;
        let curl_executable = system_curl_executable()?;
        require_curl_variable_support(&curl_executable)?;
        Ok(Self {
            curl_executable,
            origin,
            binding,
            key_name,
            session,
        })
    }

    fn renew_application_session(&self) -> Result<InMemoryApplicationSession, BridgePortError> {
        let key = PersistedP256Key::open_or_create(&self.key_name)
            .map_err(|_| BridgePortError::Unavailable)?;
        key.require_non_exportable()
            .map_err(|_| BridgePortError::InvalidResponse)?;

        let challenge_correlation = next_correlation_id()?;
        let challenge_path = device_path(
            DEVICE_SESSION_CHALLENGE_PATH_TEMPLATE,
            self.binding.tenant_id(),
            self.binding.device_id(),
        );
        let response = self.request_inner(
            MachineHttpMethod::PostJson,
            &challenge_path,
            &challenge_correlation,
            Some(b"{}"),
            None,
        )?;
        if response.status() != 201 {
            return Err(BridgePortError::InvalidResponse);
        }
        let challenge = serde_json::from_slice::<DeviceProofChallengeProjection>(response.body())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if challenge.device_id != self.binding.device_id().as_str()
            || !valid_lower_hex_exact(&challenge.challenge_token, OPAQUE_TOKEN_HEX_LENGTH)
            || !valid_lower_hex_exact(&challenge.nonce_hex, DEVICE_PROOF_NONCE_HEX_LENGTH)
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let nonce = decode_exact_lower_hex::<32>(&challenge.nonce_hex)
            .ok_or(BridgePortError::InvalidResponse)?;
        let challenge_expires_at = UnixMillis::new(challenge.expires_at_ms);
        let challenge_observed_at = now()?;
        if challenge_expires_at <= challenge_observed_at
            || challenge_expires_at
                .value()
                .saturating_sub(challenge_observed_at.value())
                > 120_000
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let message = device_proof_message_v1(
            self.binding.tenant_id(),
            self.binding.actor_id(),
            self.binding.device_id(),
            &nonce,
            challenge_expires_at,
        )
        .map_err(|_| BridgePortError::InvalidResponse)?;
        let signature = key
            .sign_sha256_message(&message)
            .map_err(|_| BridgePortError::Unavailable)?;
        let signature_hex = hex_encode(&signature);
        let renew = SessionRenewRequest {
            challenge_token: &challenge.challenge_token,
            signature_p1363_hex: &signature_hex,
        };
        let mut body = serde_json::to_vec(&renew).map_err(|_| BridgePortError::InvalidResponse)?;
        let renewal_correlation = next_correlation_id()?;
        let renewal_path = device_path(
            DEVICE_SESSION_COLLECTION_PATH_TEMPLATE,
            self.binding.tenant_id(),
            self.binding.device_id(),
        );
        let response = self.request_inner(
            MachineHttpMethod::PostJson,
            &renewal_path,
            &renewal_correlation,
            Some(&body),
            None,
        );
        body.fill(0);
        let response = response?;
        if response.status() != 201 {
            return Err(BridgePortError::InvalidResponse);
        }
        let projection =
            serde_json::from_slice::<DeviceApplicationSessionProjection>(response.body())
                .map_err(|_| BridgePortError::InvalidResponse)?;
        if projection.actor_id != self.binding.actor_id().as_str()
            || projection.device_id != self.binding.device_id().as_str()
            || !valid_lower_hex_exact(&projection.session_token, OPAQUE_TOKEN_HEX_LENGTH)
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let observed_at = now()?;
        let expires_at = UnixMillis::new(projection.expires_at_ms);
        if expires_at <= observed_at
            || expires_at.value().saturating_sub(observed_at.value()) > SESSION_MAX_LIFETIME_MS
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let session = InMemoryApplicationSession {
            token: Zeroizing::new(projection.session_token),
            expires_at,
        };
        let mut guard = self
            .session
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        *guard = Some(session.clone());
        Ok(session)
    }

    fn current_or_renewed_session(&self) -> Result<InMemoryApplicationSession, BridgePortError> {
        let observed_at = now()?;
        let current = self
            .session
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?
            .as_ref()
            .filter(|session| {
                session
                    .expires_at
                    .value()
                    .saturating_sub(observed_at.value())
                    > SESSION_RENEWAL_MARGIN_MS
            })
            .cloned();
        current.map_or_else(|| self.renew_application_session(), Ok)
    }

    fn request_inner(
        &self,
        method: MachineHttpMethod,
        path: &str,
        correlation_id: &CorrelationId,
        body: Option<&[u8]>,
        proof: Option<&RequestProofHeaders<'_>>,
    ) -> Result<MachineHttpResponse, BridgePortError> {
        if !valid_route_path(path) || matches!(method, MachineHttpMethod::Get) != body.is_none() {
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
            .arg(MAX_CONTROL_PLANE_RESPONSE_BYTES.to_string())
            .arg("--noproxy")
            .arg("*")
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
        if let Some(proof) = proof {
            command
                .env(CURL_SESSION_ENV, proof.session_token)
                .arg("--variable")
                .arg(format!("%{CURL_SESSION_ENV}"))
                .arg("--expand-header")
                .arg(format!(
                    "{DEVICE_APPLICATION_SESSION_HEADER}: {{{{{CURL_SESSION_ENV}}}}}"
                ))
                .arg("--header")
                .arg(format!(
                    "{DEVICE_REQUEST_PROOF_EXPIRES_HEADER}: {}",
                    proof.expires_at.value()
                ))
                .arg("--header")
                .arg(format!(
                    "{DEVICE_REQUEST_PROOF_SIGNATURE_HEADER}: {}",
                    proof.signature_hex
                ));
        }
        inherit_windows_system_root(&mut command);

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
        let (status, body) = decode_curl_http_output(output, MAX_CONTROL_PLANE_RESPONSE_BYTES)?;
        Ok(MachineHttpResponse::new(status, body))
    }
}

impl MachineHttpPort for WindowsApplicationSessionHttp {
    type Error = BridgePortError;

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        let session = self.current_or_renewed_session()?;
        let expires_at = UnixMillis::new(
            now()?
                .value()
                .checked_add(BRIDGE_REQUEST_PROOF_MAX_LIFETIME_MS)
                .ok_or(BridgePortError::InvalidResponse)?,
        );
        let session_digest = Sha256::digest(session.token.as_bytes());
        let body_digest = Sha256::digest(body.unwrap_or_default());
        let mut session_digest_bytes = [0_u8; 32];
        session_digest_bytes.copy_from_slice(&session_digest);
        let mut body_digest_bytes = [0_u8; 32];
        body_digest_bytes.copy_from_slice(&body_digest);
        let proof_method = match method {
            MachineHttpMethod::Get => BridgeRequestProofMethod::Get,
            MachineHttpMethod::PostJson => BridgeRequestProofMethod::PostJson,
        };
        let message = bridge_request_proof_message_v1(
            self.binding.tenant_id(),
            self.binding.actor_id(),
            self.binding.device_id(),
            &session_digest_bytes,
            proof_method,
            path,
            correlation_id,
            &body_digest_bytes,
            expires_at,
        )
        .map_err(|_| BridgePortError::InvalidResponse)?;
        let key = PersistedP256Key::open_or_create(&self.key_name)
            .map_err(|_| BridgePortError::Unavailable)?;
        key.require_non_exportable()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let signature = key
            .sign_sha256_message(&message)
            .map_err(|_| BridgePortError::Unavailable)?;
        if signature.len() != P1363_SIGNATURE_BYTES {
            return Err(BridgePortError::InvalidResponse);
        }
        let proof = RequestProofHeaders {
            session_token: session.token.as_str(),
            expires_at,
            signature_hex: hex_encode(&signature),
        };
        self.request_inner(method, path, correlation_id, body, Some(&proof))
    }
}

fn device_path(template: &str, tenant_id: &TenantId, device_id: &DeviceId) -> String {
    template
        .replace("{tenantId}", tenant_id.as_str())
        .replace("{deviceId}", device_id.as_str())
}

fn next_correlation_id() -> Result<CorrelationId, BridgePortError> {
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let now = now()?.value();
    CorrelationId::parse(format!("corr_bridge_auth_{now}_{sequence}"))
        .map_err(|_| BridgePortError::InvalidResponse)
}

fn now() -> Result<UnixMillis, BridgePortError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BridgePortError::Unavailable)?;
    let millis = u64::try_from(duration.as_millis()).map_err(|_| BridgePortError::Unavailable)?;
    Ok(UnixMillis::new(millis))
}

fn require_curl_variable_support(curl: &Path) -> Result<(), BridgePortError> {
    let output = Command::new(curl)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| BridgePortError::Unavailable)?;
    if !output.status.success() || !curl_version_supports_variables(&output.stdout) {
        return Err(BridgePortError::Unavailable);
    }
    Ok(())
}

fn curl_version_supports_variables(stdout: &[u8]) -> bool {
    let Ok(value) = std::str::from_utf8(stdout) else {
        return false;
    };
    let Some(version) = value
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("curl "))
        .and_then(|line| line.split_ascii_whitespace().next())
    else {
        return false;
    };
    let mut parts = version.split('.');
    let Some(major) = parts.next().and_then(|value| value.parse::<u64>().ok()) else {
        return false;
    };
    let Some(minor) = parts.next().and_then(|value| value.parse::<u64>().ok()) else {
        return false;
    };
    major > 8 || (major == 8 && minor >= 3)
}

fn system_curl_executable() -> Result<PathBuf, BridgePortError> {
    let system_root = env::var_os("SystemRoot").ok_or(BridgePortError::Unavailable)?;
    let curl_executable = PathBuf::from(system_root).join("System32").join("curl.exe");
    validate_regular_absolute_file(&curl_executable, None)?;
    Ok(curl_executable)
}

fn inherit_windows_system_root(command: &mut Command) {
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
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

fn valid_route_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 512
        && !path.contains(['?', '#', '\\', '\r', '\n'])
        && !path.contains("//")
        && !path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
}

fn valid_lower_hex_exact(value: &str, expected: usize) -> bool {
    value.len() == expected
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_exact_lower_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if !valid_lower_hex_exact(value, N * 2) {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
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

fn validate_regular_absolute_file(
    path: &Path,
    max_bytes: Option<u64>,
) -> Result<(), BridgePortError> {
    if !path.is_absolute() {
        return Err(BridgePortError::InvalidResponse);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| BridgePortError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BridgePortError::InvalidResponse);
    }
    if max_bytes.is_some_and(|limit| metadata.len() > limit) {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        curl_version_supports_variables, decode_exact_lower_hex, valid_lower_hex_exact,
        validate_https_origin,
    };

    #[test]
    fn curl_secret_expansion_requires_version_that_supports_variables() {
        assert!(curl_version_supports_variables(b"curl 8.3.0 (Windows)\r\n"));
        assert!(curl_version_supports_variables(b"curl 9.0.0 (Windows)\r\n"));
        assert!(!curl_version_supports_variables(
            b"curl 8.2.1 (Windows)\r\n"
        ));
        assert!(!curl_version_supports_variables(b"not-curl 8.3.0\r\n"));
    }

    #[test]
    fn admission_transport_is_strict_about_origin_and_hex() {
        assert!(validate_https_origin("https://control.example.com").is_ok());
        assert!(validate_https_origin("http://control.example.com").is_err());
        assert!(valid_lower_hex_exact(&"ab".repeat(32), 64));
        assert!(!valid_lower_hex_exact(&"AB".repeat(32), 64));
        assert!(decode_exact_lower_hex::<64>(&"ab".repeat(64)).is_some());
        assert!(decode_exact_lower_hex::<64>(&"ab".repeat(63)).is_none());
    }
}
