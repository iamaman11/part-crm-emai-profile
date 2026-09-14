#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::device_pairing::{DevicePairingCompleteUri, DevicePairingStartUri};
use crate::windows_device_application::WindowsDeviceApplicationBinding;
use bridge_domain::BridgePortError;
use control_plane_contract::device_application_api::{
    DEVICE_PAIRING_COLLECTION_PATH_TEMPLATE, DEVICE_PAIRING_COMPLETION_PATH_TEMPLATE,
    DeviceApplicationSessionProjection, DevicePairingCreateProjection, OPAQUE_TOKEN_HEX_LENGTH,
};
use device_domain::device_proof_message_v1;
use profile_platform_primitives::{CorrelationId, DeviceId, TenantId, UnixMillis};
use serde::{Serialize, de::DeserializeOwned};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_device_key::{KeyDisposition, PersistedP256Key};
use zeroize::{Zeroize, Zeroizing};

const DEVICE_APPLICATION_BINDING_PATH_ENV: &str = "PROFILE_BRIDGE_DEVICE_APPLICATION_BINDING_PATH";
const CONTROL_PLANE_ORIGIN_ENV: &str = "PROFILE_BRIDGE_CONTROL_PLANE_ORIGIN";
const MAX_CONTROL_PLANE_RESPONSE_BYTES: usize = 65_536;
const MAX_BINDING_BYTES: u64 = 4_096;
const CURL_CONNECT_TIMEOUT_SECONDS: &str = "10";
const CURL_TOTAL_TIMEOUT_SECONDS: &str = "20";
const HTTP_STATUS_MARKER: &[u8] = b"\nPROFILE_BRIDGE_HTTP_STATUS:";
const KEY_NAME_PREFIX: &str = "part-crm.device.";
const PAIRING_MAX_LIFETIME_MS: u64 = 600_000;
const CHALLENGE_MAX_LIFETIME_MS: u64 = 120_000;
const SESSION_MAX_LIFETIME_MS: u64 = 900_000;
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingCreateRequest<'a> {
    device_id: &'a str,
    public_key_spki_der_hex: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingCompleteRequest<'a> {
    pairing_token: &'a str,
    challenge_token: &'a str,
    signature_p1363_hex: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingDocument<'a> {
    tenant_id: &'a str,
    actor_id: &'a str,
    device_id: &'a str,
}

pub fn run_start(uri: &DevicePairingStartUri) -> Result<(), BridgePortError> {
    let config = BootstrapConfig::from_environment()?;
    require_binding_absent(&config.binding_path)?;

    let key_name = key_name(uri.device_id())?;
    let key =
        PersistedP256Key::open_or_create(&key_name).map_err(|_| BridgePortError::Unavailable)?;
    key.require_non_exportable()
        .map_err(|_| BridgePortError::InvalidResponse)?;
    let created_key = key.disposition() == KeyDisposition::Created;

    let result = start_with_key(uri, &config, &key);
    if result.is_err() && created_key {
        let _ = key.delete();
    }
    result
}

fn start_with_key(
    uri: &DevicePairingStartUri,
    config: &BootstrapConfig,
    key: &PersistedP256Key,
) -> Result<(), BridgePortError> {
    let spki = key
        .public_key_spki_der()
        .map_err(|_| BridgePortError::Unavailable)?;
    let public_key_hex = hex_encode(&spki);
    let request = PairingCreateRequest {
        device_id: uri.device_id().as_str(),
        public_key_spki_der_hex: &public_key_hex,
    };
    let mut body = serde_json::to_vec(&request).map_err(|_| BridgePortError::InvalidResponse)?;
    let path = tenant_path(DEVICE_PAIRING_COLLECTION_PATH_TEMPLATE, uri.tenant_id());
    let response = post_json(&config.origin, &path, &next_correlation_id()?, &body);
    body.zeroize();
    let mut response = response?;
    if response.status != 201 {
        response.body.zeroize();
        return Err(BridgePortError::InvalidResponse);
    }
    let projection = parse_json_response::<DevicePairingCreateProjection>(&mut response)?;
    if !valid_lower_hex(&projection.pairing_token, OPAQUE_TOKEN_HEX_LENGTH) {
        return Err(BridgePortError::InvalidResponse);
    }
    let observed_at = now()?;
    let expires_at = UnixMillis::new(projection.expires_at_ms);
    if expires_at <= observed_at
        || expires_at.value().saturating_sub(observed_at.value()) > PAIRING_MAX_LIFETIME_MS
    {
        return Err(BridgePortError::InvalidResponse);
    }
    let pairing_token = Zeroizing::new(projection.pairing_token);
    let authorization_url = Zeroizing::new(format!(
        "{}/devices?tenant={}#pairing={}&device={}",
        config.origin,
        uri.tenant_id().as_str(),
        pairing_token.as_str(),
        uri.device_id().as_str()
    ));
    open_default_browser(authorization_url.as_str())
}

pub fn run_complete(uri: &DevicePairingCompleteUri) -> Result<(), BridgePortError> {
    let config = BootstrapConfig::from_environment()?;
    require_binding_absent(&config.binding_path)?;

    let observed_at = now()?;
    if uri.expires_at() <= observed_at
        || uri.expires_at().value().saturating_sub(observed_at.value()) > CHALLENGE_MAX_LIFETIME_MS
    {
        return Err(BridgePortError::InvalidResponse);
    }

    let key_name = key_name(uri.device_id())?;
    let key =
        PersistedP256Key::open_or_create(&key_name).map_err(|_| BridgePortError::Unavailable)?;
    if key.disposition() == KeyDisposition::Created {
        let _ = key.delete();
        return Err(BridgePortError::InvalidResponse);
    }
    key.require_non_exportable()
        .map_err(|_| BridgePortError::InvalidResponse)?;

    let nonce =
        decode_exact_lower_hex::<32>(uri.nonce_hex()).ok_or(BridgePortError::InvalidResponse)?;
    let message = device_proof_message_v1(
        uri.tenant_id(),
        uri.actor_id(),
        uri.device_id(),
        &nonce,
        uri.expires_at(),
    )
    .map_err(|_| BridgePortError::InvalidResponse)?;
    let signature = key
        .sign_sha256_message(&message)
        .map_err(|_| BridgePortError::Unavailable)?;
    let signature_hex = hex_encode(&signature);
    let request = PairingCompleteRequest {
        pairing_token: uri.pairing_token_for_transport(),
        challenge_token: uri.challenge_token_for_transport(),
        signature_p1363_hex: &signature_hex,
    };
    let mut body = serde_json::to_vec(&request).map_err(|_| BridgePortError::InvalidResponse)?;
    let path = tenant_path(DEVICE_PAIRING_COMPLETION_PATH_TEMPLATE, uri.tenant_id());
    let response = post_json(&config.origin, &path, &next_correlation_id()?, &body);
    body.zeroize();
    let mut response = response?;
    if response.status != 201 {
        response.body.zeroize();
        return Err(BridgePortError::InvalidResponse);
    }
    let projection = parse_json_response::<DeviceApplicationSessionProjection>(&mut response)?;
    if projection.actor_id != uri.actor_id().as_str()
        || projection.device_id != uri.device_id().as_str()
        || !valid_lower_hex(&projection.session_token, OPAQUE_TOKEN_HEX_LENGTH)
    {
        return Err(BridgePortError::InvalidResponse);
    }
    let session_token = Zeroizing::new(projection.session_token);
    let session_expires_at = UnixMillis::new(projection.expires_at_ms);
    let observed_at = now()?;
    if session_expires_at <= observed_at
        || session_expires_at
            .value()
            .saturating_sub(observed_at.value())
            > SESSION_MAX_LIFETIME_MS
    {
        return Err(BridgePortError::InvalidResponse);
    }
    drop(session_token);

    persist_binding(&config.binding_path, uri)?;
    let binding = WindowsDeviceApplicationBinding::open(&config.binding_path)?;
    if binding.tenant_id() != uri.tenant_id()
        || binding.actor_id() != uri.actor_id()
        || binding.device_id() != uri.device_id()
    {
        let _ = fs::remove_file(&config.binding_path);
        return Err(BridgePortError::InvalidResponse);
    }

    let success_url = format!(
        "{}/devices?tenant={}#connected={}",
        config.origin,
        uri.tenant_id().as_str(),
        uri.device_id().as_str()
    );
    let _ = open_default_browser(&success_url);
    Ok(())
}

struct BootstrapConfig {
    binding_path: PathBuf,
    origin: String,
}

impl BootstrapConfig {
    fn from_environment() -> Result<Self, BridgePortError> {
        let binding_path = env::var_os(DEVICE_APPLICATION_BINDING_PATH_ENV)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or(BridgePortError::InvalidResponse)?;
        let origin =
            env::var(CONTROL_PLANE_ORIGIN_ENV).map_err(|_| BridgePortError::Unavailable)?;
        validate_https_origin(&origin)?;
        Ok(Self {
            binding_path,
            origin,
        })
    }
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

fn parse_json_response<T: DeserializeOwned>(
    response: &mut HttpResponse,
) -> Result<T, BridgePortError> {
    let projection = serde_json::from_slice::<T>(&response.body);
    response.body.zeroize();
    projection.map_err(|_| BridgePortError::InvalidResponse)
}

fn post_json(
    origin: &str,
    path: &str,
    correlation_id: &CorrelationId,
    body: &[u8],
) -> Result<HttpResponse, BridgePortError> {
    if !valid_route_path(path) {
        return Err(BridgePortError::InvalidResponse);
    }
    let curl = system_executable("curl.exe")?;
    let mut command = Command::new(curl);
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
        .arg("Content-Type: application/json")
        .arg("--header")
        .arg(format!("X-Correlation-Id: {}", correlation_id.as_str()))
        .arg("--request")
        .arg("POST")
        .arg("--url")
        .arg(format!("{origin}{path}"))
        .arg("--data-binary")
        .arg("@-")
        .arg("--write-out")
        .arg("\nPROFILE_BRIDGE_HTTP_STATUS:%{http_code}")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    inherit_windows_system_root(&mut command);

    let mut child = command.spawn().map_err(|_| BridgePortError::Unavailable)?;
    child
        .stdin
        .take()
        .ok_or(BridgePortError::Unavailable)?
        .write_all(body)
        .map_err(|_| BridgePortError::Unavailable)?;
    let output = child
        .wait_with_output()
        .map_err(|_| BridgePortError::Unavailable)?;
    let (status, body) = decode_curl_http_output(output, MAX_CONTROL_PLANE_RESPONSE_BYTES)?;
    Ok(HttpResponse { status, body })
}

fn open_default_browser(url: &str) -> Result<(), BridgePortError> {
    if !valid_direct_https_url(url) {
        return Err(BridgePortError::InvalidResponse);
    }
    let rundll32 = system_executable("rundll32.exe")?;
    let mut command = Command::new(rundll32);
    command
        .env_clear()
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    inherit_windows_system_root(&mut command);
    let status = command.status().map_err(|_| BridgePortError::Unavailable)?;
    if status.success() {
        Ok(())
    } else {
        Err(BridgePortError::Unavailable)
    }
}

fn persist_binding(path: &Path, uri: &DevicePairingCompleteUri) -> Result<(), BridgePortError> {
    require_binding_absent(path)?;
    let document = BindingDocument {
        tenant_id: uri.tenant_id().as_str(),
        actor_id: uri.actor_id().as_str(),
        device_id: uri.device_id().as_str(),
    };
    let mut body = serde_json::to_vec(&document).map_err(|_| BridgePortError::InvalidResponse)?;
    if body.len() as u64 > MAX_BINDING_BYTES {
        body.zeroize();
        return Err(BridgePortError::InvalidResponse);
    }
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(_) => {
            body.zeroize();
            return Err(BridgePortError::Unavailable);
        }
    };
    let result = file
        .write_all(&body)
        .and_then(|()| file.sync_all())
        .map_err(|_| BridgePortError::Unavailable);
    body.zeroize();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn require_binding_absent(path: &Path) -> Result<(), BridgePortError> {
    if !path.is_absolute() {
        return Err(BridgePortError::InvalidResponse);
    }
    match fs::symlink_metadata(path) {
        Ok(_) => return Err(BridgePortError::InvalidResponse),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(BridgePortError::Unavailable),
    }
    let parent = path.parent().ok_or(BridgePortError::InvalidResponse)?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| BridgePortError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(())
}

fn key_name(device_id: &DeviceId) -> Result<String, BridgePortError> {
    let value = format!("{KEY_NAME_PREFIX}{}", device_id.as_str());
    if value.len() > 128 {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(value)
}

fn tenant_path(template: &str, tenant_id: &TenantId) -> String {
    template.replace("{tenantId}", tenant_id.as_str())
}

fn next_correlation_id() -> Result<CorrelationId, BridgePortError> {
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    CorrelationId::parse(format!("corr_bridge_pair_{}_{}", now()?.value(), sequence))
        .map_err(|_| BridgePortError::InvalidResponse)
}

fn now() -> Result<UnixMillis, BridgePortError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BridgePortError::Unavailable)?;
    let millis = u64::try_from(duration.as_millis()).map_err(|_| BridgePortError::Unavailable)?;
    Ok(UnixMillis::new(millis))
}

fn system_executable(name: &str) -> Result<PathBuf, BridgePortError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(BridgePortError::InvalidResponse);
    }
    let system_root = env::var_os("SystemRoot").ok_or(BridgePortError::Unavailable)?;
    let path = PathBuf::from(system_root).join("System32").join(name);
    let metadata = fs::symlink_metadata(&path).map_err(|_| BridgePortError::Unavailable)?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok(path)
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

fn valid_direct_https_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    !rest.is_empty()
        && value.len() <= 2_048
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

fn valid_lower_hex(value: &str, expected: usize) -> bool {
    value.len() == expected
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_exact_lower_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if !valid_lower_hex(value, N * 2) {
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
    mut output: Output,
    max_body_bytes: usize,
) -> Result<(u16, Vec<u8>), BridgePortError> {
    let decoded = (|| {
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
    })();
    output.stdout.zeroize();
    decoded
}

#[cfg(test)]
mod tests {
    use super::{
        DevicePairingCreateProjection, HttpResponse, WindowsDeviceApplicationBinding,
        parse_json_response, persist_binding, require_binding_absent, valid_direct_https_url,
        validate_https_origin,
    };
    use crate::device_pairing::DevicePairingCompleteUri;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn temp_root(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "profile-bridge-device-pairing-{label}-{}-{sequence}",
            std::process::id()
        ));
        match fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::create_dir(&root)?;
        Ok(root)
    }

    fn completion_uri() -> Result<DevicePairingCompleteUri, Box<dyn std::error::Error>> {
        let pairing = "a".repeat(64);
        let challenge = "b".repeat(64);
        let nonce = "c".repeat(64);
        Ok(DevicePairingCompleteUri::parse(&format!(
            "profilebridge://pair/complete/tenant_01JPAIR/actor_01JPAIR/device_01JPAIR/{pairing}/{challenge}/{nonce}/123456789"
        ))?)
    }

    #[test]
    fn pairing_browser_urls_require_direct_https() {
        assert!(validate_https_origin("https://control.example.com").is_ok());
        assert!(validate_https_origin("http://control.example.com").is_err());
        assert!(valid_direct_https_url(
            "https://control.example.com/devices?tenant=tenant_01JPAIR#connected=device_01JPAIR"
        ));
        assert!(!valid_direct_https_url(
            "https://control.example.com/devices\nmalformed"
        ));
    }

    #[test]
    fn pairing_response_body_is_zeroized_after_success_and_parse_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let token = "a".repeat(64);
        let mut valid = HttpResponse {
            status: 201,
            body: format!(r#"{{"pairingToken":"{token}","expiresAtMs":123}}"#).into_bytes(),
        };
        let projection = parse_json_response::<DevicePairingCreateProjection>(&mut valid)?;
        assert_eq!(projection.pairing_token, token);
        assert!(valid.body.iter().all(|byte| *byte == 0));

        let mut malformed = HttpResponse {
            status: 201,
            body: b"{malformed-json".to_vec(),
        };
        assert!(parse_json_response::<DevicePairingCreateProjection>(&mut malformed).is_err());
        assert!(malformed.body.iter().all(|byte| *byte == 0));
        Ok(())
    }

    #[test]
    fn pairing_completion_persists_only_nonsecret_binding_and_reopens_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root("persist")?;
        let path = root.join("device-application-binding.json");
        let uri = completion_uri()?;

        assert!(require_binding_absent(&path).is_ok());
        persist_binding(&path, &uri)?;
        let binding = WindowsDeviceApplicationBinding::open(&path)?;
        assert_eq!(binding.tenant_id(), uri.tenant_id());
        assert_eq!(binding.actor_id(), uri.actor_id());
        assert_eq!(binding.device_id(), uri.device_id());
        let persisted = fs::read_to_string(&path)?;
        assert!(!persisted.contains(uri.pairing_token_for_transport()));
        assert!(!persisted.contains(uri.challenge_token_for_transport()));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn pairing_completion_never_overwrites_or_deletes_preexisting_binding()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root("preexisting")?;
        let path = root.join("device-application-binding.json");
        let uri = completion_uri()?;
        let sentinel = b"preexisting-binding-owned-elsewhere";
        fs::write(&path, sentinel)?;

        assert!(require_binding_absent(&path).is_err());
        assert!(persist_binding(&path, &uri).is_err());
        assert_eq!(fs::read(&path)?, sentinel);

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
