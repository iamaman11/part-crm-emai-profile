#![forbid(unsafe_code)]

use crate::windows_delivery::{VerifiedDeliveryCandidate, WindowsDeliveryComponent};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File, Metadata, OpenOptions};
#[cfg(windows)]
use std::io::Write;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

const RELEASE_SET_PREFIX: &str = "release-set-v3-sha256-";
const PROFILE_BRIDGE_ASSET: &str = "profile-bridge.zip";
const RUNTIME_BUNDLE_ASSET: &str = "runtime-bundle.tar";
#[cfg(any(test, windows))]
const RELEASE_BASE_URL: &str =
    "https://github.com/iamaman11/part-crm-emai-profile/releases/download";
const MAX_DOWNLOAD_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
static PENDING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(windows)]
const FETCH_PROCESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);
#[cfg(windows)]
const FETCH_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[cfg(windows)]
const FETCH_SCRIPT: &str = r#"param(
    [Parameter(Mandatory=$true)][string]$Uri,
    [Parameter(Mandatory=$true)][string]$Destination,
    [Parameter(Mandatory=$true)][UInt64]$ExpectedSize
)
$ErrorActionPreference = 'Stop'
$handler = $null
$client = $null
$response = $null
$stream = $null
$file = $null
$code = 30
try {
    Add-Type -AssemblyName System.Net.Http
    $handler = [System.Net.Http.HttpClientHandler]::new()
    $handler.AllowAutoRedirect = $true
    $handler.MaxAutomaticRedirections = 5
    $client = [System.Net.Http.HttpClient]::new($handler)
    $client.Timeout = [System.TimeSpan]::FromMinutes(10)
    [void]$client.DefaultRequestHeaders.UserAgent.ParseAdd('profile-bridge-windows-delivery/1')
    $response = $client.GetAsync(
        $Uri,
        [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
    ).GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) {
        $code = 20
    }
    elseif (
        $null -ne $response.Content.Headers.ContentLength -and
        [UInt64]$response.Content.Headers.ContentLength -ne $ExpectedSize
    ) {
        $code = 21
    }
    else {
        $stream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
        $file = [System.IO.File]::Open(
            $Destination,
            [System.IO.FileMode]::CreateNew,
            [System.IO.FileAccess]::Write,
            [System.IO.FileShare]::None
        )
        $buffer = New-Object byte[] 65536
        [UInt64]$total = 0
        $code = 0
        while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $next = $total + [UInt64]$read
            if ($next -gt $ExpectedSize) {
                $code = 22
                break
            }
            $file.Write($buffer, 0, $read)
            $total = $next
        }
        if ($code -eq 0 -and $total -ne $ExpectedSize) {
            $code = 23
        }
        if ($code -eq 0) {
            $file.Flush($true)
        }
    }
}
catch {
    $code = 30
}
finally {
    if ($null -ne $file) { $file.Dispose() }
    if ($null -ne $stream) { $stream.Dispose() }
    if ($null -ne $response) { $response.Dispose() }
    if ($null -ne $client) { $client.Dispose() }
    elseif ($null -ne $handler) { $handler.Dispose() }
}
exit $code
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryDownloadRoot {
    path: PathBuf,
}

impl DeliveryDownloadRoot {
    pub fn open_or_create(path: impl AsRef<Path>) -> Result<Self, DeliveryDownloadError> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(DeliveryDownloadError::InvalidRoot);
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) => validate_directory_metadata(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let parent = path.parent().ok_or(DeliveryDownloadError::InvalidRoot)?;
                let parent_metadata =
                    fs::symlink_metadata(parent).map_err(|_| DeliveryDownloadError::Io)?;
                validate_directory_metadata(&parent_metadata)?;
                match fs::create_dir(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(DeliveryDownloadError::Io),
                }
                let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryDownloadError::Io)?;
                validate_directory_metadata(&metadata)?;
            }
            Err(_) => return Err(DeliveryDownloadError::Io),
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub trait DeliveryAssetFetcher {
    type Error;

    fn fetch_release_asset(
        &mut self,
        release_set_id: &str,
        asset_name: &str,
        destination: &Path,
        expected_size_bytes: u64,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadedDeliveryArtifacts {
    profile_bridge: PathBuf,
    runtime_bundle: PathBuf,
}

impl DownloadedDeliveryArtifacts {
    #[must_use]
    pub fn profile_bridge(&self) -> &Path {
        &self.profile_bridge
    }

    #[must_use]
    pub fn runtime_bundle(&self) -> &Path {
        &self.runtime_bundle
    }
}

pub fn download_verified_delivery<F: DeliveryAssetFetcher>(
    root: &DeliveryDownloadRoot,
    candidate: &VerifiedDeliveryCandidate,
    fetcher: &mut F,
) -> Result<DownloadedDeliveryArtifacts, DeliveryDownloadError> {
    validate_directory_path(root.path())?;
    let manifest = candidate.manifest();
    if !prefixed_sha256(&manifest.release_set_id, RELEASE_SET_PREFIX) {
        return Err(DeliveryDownloadError::InvalidCandidate);
    }
    validate_component_bounds(&manifest.components.profile_bridge)?;
    validate_component_bounds(&manifest.components.runtime_bundle)?;

    let release_directory = root.path().join(&manifest.release_set_id);
    ensure_directory(&release_directory)?;
    let profile_bridge = materialize_asset(
        &release_directory,
        &manifest.release_set_id,
        PROFILE_BRIDGE_ASSET,
        &manifest.components.profile_bridge,
        fetcher,
    )?;
    let runtime_bundle = materialize_asset(
        &release_directory,
        &manifest.release_set_id,
        RUNTIME_BUNDLE_ASSET,
        &manifest.components.runtime_bundle,
        fetcher,
    )?;
    Ok(DownloadedDeliveryArtifacts {
        profile_bridge,
        runtime_bundle,
    })
}

fn validate_component_bounds(
    component: &WindowsDeliveryComponent,
) -> Result<(), DeliveryDownloadError> {
    if component.artifact_size_bytes == 0
        || component.artifact_size_bytes > MAX_DOWNLOAD_BYTES
        || !is_lower_hex(&component.artifact_sha256, 64)
    {
        return Err(DeliveryDownloadError::InvalidCandidate);
    }
    Ok(())
}

fn materialize_asset<F: DeliveryAssetFetcher>(
    release_directory: &Path,
    release_set_id: &str,
    asset_name: &'static str,
    component: &WindowsDeliveryComponent,
    fetcher: &mut F,
) -> Result<PathBuf, DeliveryDownloadError> {
    let final_path = release_directory.join(asset_name);
    match fs::symlink_metadata(&final_path) {
        Ok(_) => {
            if verify_file_identity(
                &final_path,
                component.artifact_size_bytes,
                &component.artifact_sha256,
            )? {
                return Ok(final_path);
            }
            return Err(DeliveryDownloadError::ExistingArtifactMismatch);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(DeliveryDownloadError::Io),
    }

    let pending_path = pending_path(release_directory, asset_name);
    match fs::symlink_metadata(&pending_path) {
        Ok(_) => return Err(DeliveryDownloadError::Io),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(DeliveryDownloadError::Io),
    }
    if fetcher
        .fetch_release_asset(
            release_set_id,
            asset_name,
            &pending_path,
            component.artifact_size_bytes,
        )
        .is_err()
    {
        cleanup_pending(&pending_path)?;
        return Err(DeliveryDownloadError::FetchFailed);
    }
    let exact = match verify_file_identity(
        &pending_path,
        component.artifact_size_bytes,
        &component.artifact_sha256,
    ) {
        Ok(exact) => exact,
        Err(error) => {
            cleanup_pending(&pending_path)?;
            return Err(error);
        }
    };
    if !exact {
        cleanup_pending(&pending_path)?;
        return Err(DeliveryDownloadError::ArtifactIdentityMismatch);
    }
    if let Err(error) = sync_regular_file(&pending_path) {
        cleanup_pending(&pending_path)?;
        return Err(error);
    }

    match fs::hard_link(&pending_path, &final_path) {
        Ok(()) => {
            fs::remove_file(&pending_path).map_err(|_| DeliveryDownloadError::Io)?;
            Ok(final_path)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let existing_exact = verify_file_identity(
                &final_path,
                component.artifact_size_bytes,
                &component.artifact_sha256,
            )?;
            cleanup_pending(&pending_path)?;
            if existing_exact {
                Ok(final_path)
            } else {
                Err(DeliveryDownloadError::ExistingArtifactMismatch)
            }
        }
        Err(_) => {
            cleanup_pending(&pending_path)?;
            Err(DeliveryDownloadError::Io)
        }
    }
}

fn pending_path(directory: &Path, asset_name: &str) -> PathBuf {
    let sequence = PENDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(
        ".{asset_name}.pending-{}-{sequence}",
        std::process::id()
    ))
}

fn cleanup_pending(path: &Path) -> Result<(), DeliveryDownloadError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
                return Err(DeliveryDownloadError::Io);
            }
            fs::remove_file(path).map_err(|_| DeliveryDownloadError::Io)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(DeliveryDownloadError::Io),
    }
}

fn ensure_directory(path: &Path) -> Result<(), DeliveryDownloadError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_directory_metadata(&metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(DeliveryDownloadError::Io),
            }
            let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryDownloadError::Io)?;
            validate_directory_metadata(&metadata)
        }
        Err(_) => Err(DeliveryDownloadError::Io),
    }
}

fn validate_directory_path(path: &Path) -> Result<(), DeliveryDownloadError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryDownloadError::Io)?;
    validate_directory_metadata(&metadata)
}

fn validate_directory_metadata(metadata: &Metadata) -> Result<(), DeliveryDownloadError> {
    if metadata_is_link_or_reparse(metadata) || !metadata.is_dir() {
        return Err(DeliveryDownloadError::InvalidRoot);
    }
    Ok(())
}

fn verify_file_identity(
    path: &Path,
    expected_size_bytes: u64,
    expected_sha256: &str,
) -> Result<bool, DeliveryDownloadError> {
    if !path.is_absolute() {
        return Err(DeliveryDownloadError::Io);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryDownloadError::Io)?;
    if metadata_is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() != expected_size_bytes
        || metadata.len() > MAX_DOWNLOAD_BYTES
    {
        return Ok(false);
    }
    let mut file = File::open(path).map_err(|_| DeliveryDownloadError::Io)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| DeliveryDownloadError::Io)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(encode_lower_hex(digest.finalize().as_slice()) == expected_sha256)
}

fn sync_regular_file(path: &Path) -> Result<(), DeliveryDownloadError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| DeliveryDownloadError::Io)?;
    file.sync_all().map_err(|_| DeliveryDownloadError::Io)
}

fn prefixed_sha256(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|digest| is_lower_hex(digest, 64))
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        output.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(any(test, windows))]
fn release_asset_url(release_set_id: &str, asset_name: &str) -> Option<String> {
    if !prefixed_sha256(release_set_id, RELEASE_SET_PREFIX)
        || !matches!(asset_name, PROFILE_BRIDGE_ASSET | RUNTIME_BUNDLE_ASSET)
    {
        return None;
    }
    Some(format!("{RELEASE_BASE_URL}/{release_set_id}/{asset_name}"))
}

fn metadata_is_link_or_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsReleaseAssetFetcher {
    powershell_executable: PathBuf,
}

#[cfg(windows)]
impl WindowsReleaseAssetFetcher {
    pub fn from_system() -> Result<Self, WindowsReleaseAssetFetcherError> {
        let powershell_executable = system_powershell_executable()?;
        Ok(Self {
            powershell_executable,
        })
    }

    #[must_use]
    pub fn powershell_executable(&self) -> &Path {
        &self.powershell_executable
    }
}

#[cfg(windows)]
impl DeliveryAssetFetcher for WindowsReleaseAssetFetcher {
    type Error = WindowsReleaseAssetFetcherError;

    fn fetch_release_asset(
        &mut self,
        release_set_id: &str,
        asset_name: &str,
        destination: &Path,
        expected_size_bytes: u64,
    ) -> Result<(), Self::Error> {
        if expected_size_bytes == 0 || expected_size_bytes > MAX_DOWNLOAD_BYTES {
            return Err(WindowsReleaseAssetFetcherError::InvalidRequest);
        }
        let url = release_asset_url(release_set_id, asset_name)
            .ok_or(WindowsReleaseAssetFetcherError::InvalidRequest)?;
        validate_download_destination(destination)?;
        let script_path = fetch_script_path(destination);
        write_new_synced(&script_path, FETCH_SCRIPT.as_bytes())?;

        let mut command = Command::new(&self.powershell_executable);
        command
            .env_clear()
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
            .arg(&script_path)
            .arg("-Uri")
            .arg(url)
            .arg("-Destination")
            .arg(destination)
            .arg("-ExpectedSize")
            .arg(expected_size_bytes.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        inherit_windows_system_environment(&mut command);
        let status = command
            .spawn()
            .map_err(|_| WindowsReleaseAssetFetcherError::Process)
            .and_then(|mut child| wait_for_fetch_process(&mut child));
        let cleanup =
            fs::remove_file(&script_path).map_err(|_| WindowsReleaseAssetFetcherError::ScratchIo);
        let status = status?;
        cleanup?;
        match status.code() {
            Some(0) => Ok(()),
            Some(20..=29) => Err(WindowsReleaseAssetFetcherError::RemoteRejected),
            _ => Err(WindowsReleaseAssetFetcherError::Process),
        }
    }
}

#[cfg(windows)]
fn validate_download_destination(path: &Path) -> Result<(), WindowsReleaseAssetFetcherError> {
    if !path.is_absolute() || fs::symlink_metadata(path).is_ok() {
        return Err(WindowsReleaseAssetFetcherError::InvalidRequest);
    }
    let parent = path
        .parent()
        .ok_or(WindowsReleaseAssetFetcherError::InvalidRequest)?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|_| WindowsReleaseAssetFetcherError::InvalidRequest)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(WindowsReleaseAssetFetcherError::InvalidRequest);
    }
    Ok(())
}

#[cfg(windows)]
fn fetch_script_path(destination: &Path) -> PathBuf {
    let sequence = PENDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    destination.with_file_name(format!(
        ".delivery-fetch-{}-{sequence}.ps1",
        std::process::id()
    ))
}

#[cfg(windows)]
fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), WindowsReleaseAssetFetcherError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| WindowsReleaseAssetFetcherError::ScratchIo)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| WindowsReleaseAssetFetcherError::ScratchIo)
}

#[cfg(windows)]
fn system_powershell_executable() -> Result<PathBuf, WindowsReleaseAssetFetcherError> {
    let system_root =
        env::var_os("SystemRoot").ok_or(WindowsReleaseAssetFetcherError::PlatformUnavailable)?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let metadata = fs::symlink_metadata(&executable)
        .map_err(|_| WindowsReleaseAssetFetcherError::PlatformUnavailable)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(WindowsReleaseAssetFetcherError::PlatformUnavailable);
    }
    Ok(executable)
}

#[cfg(windows)]
fn inherit_windows_system_environment(command: &mut Command) {
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
}

#[cfg(windows)]
fn wait_for_fetch_process(
    child: &mut Child,
) -> Result<ExitStatus, WindowsReleaseAssetFetcherError> {
    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(_) => return Err(WindowsReleaseAssetFetcherError::Process),
        }
        if started_at.elapsed() >= FETCH_PROCESS_TIMEOUT {
            let kill_result = child.kill();
            let wait_result = child.wait();
            if kill_result.is_err() && wait_result.is_err() {
                return Err(WindowsReleaseAssetFetcherError::Process);
            }
            return Err(WindowsReleaseAssetFetcherError::Timeout);
        }
        thread::sleep(FETCH_PROCESS_POLL_INTERVAL);
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsReleaseAssetFetcherError {
    InvalidRequest,
    PlatformUnavailable,
    ScratchIo,
    RemoteRejected,
    Process,
    Timeout,
}

#[cfg(windows)]
impl fmt::Display for WindowsReleaseAssetFetcherError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "Windows release asset request is invalid",
            Self::PlatformUnavailable => "Windows release asset platform is unavailable",
            Self::ScratchIo => "Windows release asset scratch operation failed",
            Self::RemoteRejected => "Windows release asset source rejected the exact request",
            Self::Process => "Windows release asset fetch process failed",
            Self::Timeout => "Windows release asset fetch exceeded its deadline",
        })
    }
}

#[cfg(windows)]
impl std::error::Error for WindowsReleaseAssetFetcherError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryDownloadError {
    InvalidRoot,
    InvalidCandidate,
    Io,
    FetchFailed,
    ArtifactIdentityMismatch,
    ExistingArtifactMismatch,
}

impl fmt::Display for DeliveryDownloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRoot => "Windows delivery download root is invalid",
            Self::InvalidCandidate => "Windows delivery candidate is invalid for download",
            Self::Io => "Windows delivery download filesystem operation failed",
            Self::FetchFailed => "Windows delivery exact asset fetch failed",
            Self::ArtifactIdentityMismatch => {
                "Windows delivery downloaded asset failed exact identity verification"
            }
            Self::ExistingArtifactMismatch => {
                "Windows delivery immutable cached asset conflicts with expected identity"
            }
        })
    }
}

impl std::error::Error for DeliveryDownloadError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_delivery::{
        DetachedSignatureEnvelope, DetachedSignatureVerifier, TrustedSigner, TrustedSignerSet,
        TrustedSignerStatus, WindowsDeliveryCompatibility, WindowsDeliveryComponents,
        WindowsDeliveryEvidence, WindowsDeliveryManifest, verify_delivery_candidate,
    };
    use bridge_domain::CAMOUHOST_IPC_VERSION;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-download-{label}-{}-{sequence}",
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

    struct AcceptVerifier;

    impl DetachedSignatureVerifier for AcceptVerifier {
        type Error = ();

        fn verify_cms(
            &mut self,
            _manifest_bytes: &[u8],
            _cms_der: &[u8],
            _expected_certificate_sha256: &str,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct FakeFetcher {
        bridge: Vec<u8>,
        runtime: Vec<u8>,
        calls: Vec<String>,
    }

    impl DeliveryAssetFetcher for FakeFetcher {
        type Error = ();

        fn fetch_release_asset(
            &mut self,
            _release_set_id: &str,
            asset_name: &str,
            destination: &Path,
            _expected_size_bytes: u64,
        ) -> Result<(), Self::Error> {
            self.calls.push(asset_name.to_owned());
            let bytes = match asset_name {
                PROFILE_BRIDGE_ASSET => &self.bridge,
                RUNTIME_BUNDLE_ASSET => &self.runtime,
                _ => return Err(()),
            };
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .map_err(|_| ())?;
            file.write_all(bytes).map_err(|_| ())?;
            file.sync_all().map_err(|_| ())
        }
    }

    struct RejectingFetcher;

    impl DeliveryAssetFetcher for RejectingFetcher {
        type Error = ();

        fn fetch_release_asset(
            &mut self,
            _release_set_id: &str,
            _asset_name: &str,
            _destination: &Path,
            _expected_size_bytes: u64,
        ) -> Result<(), Self::Error> {
            Err(())
        }
    }

    fn candidate(
        bridge: &[u8],
        runtime: &[u8],
    ) -> Result<VerifiedDeliveryCandidate, Box<dyn std::error::Error>> {
        let bridge_sha = sha256_hex(bridge);
        let runtime_sha = sha256_hex(runtime);
        let manifest = WindowsDeliveryManifest {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY".to_owned(),
            release_set_id: format!("{RELEASE_SET_PREFIX}{}", "a".repeat(64)),
            sequence: 1,
            source_commit_sha: "1".repeat(40),
            components: WindowsDeliveryComponents {
                profile_bridge: WindowsDeliveryComponent {
                    release_id: format!("profile-bridge-v2-sha256-{}", "b".repeat(64)),
                    artifact_sha256: bridge_sha,
                    artifact_size_bytes: u64::try_from(bridge.len())?,
                    component_manifest_sha256: "c".repeat(64),
                },
                runtime_bundle: WindowsDeliveryComponent {
                    release_id: format!("runtime-bundle-v2-sha256-{}", "d".repeat(64)),
                    artifact_sha256: runtime_sha,
                    artifact_size_bytes: u64::try_from(runtime.len())?,
                    component_manifest_sha256: "e".repeat(64),
                },
            },
            evidence: WindowsDeliveryEvidence {
                sbom_sha256: "f".repeat(64),
                provenance_sha256: "1".repeat(64),
            },
            compatibility: WindowsDeliveryCompatibility {
                profile_bridge_protocol_version: 1,
                camouhost_ipc_version: CAMOUHOST_IPC_VERSION,
                runtime_bundle_version: "2.0.0".to_owned(),
            },
        };
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        let signature = serde_json::to_vec(&DetachedSignatureEnvelope {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS".to_owned(),
            key_id: "test-release".to_owned(),
            cms_der_hex: "00".to_owned(),
        })?;
        let signer =
            TrustedSigner::new("test-release", "a".repeat(64), TrustedSignerStatus::Active)?;
        let trust = TrustedSignerSet::new([signer])?;
        Ok(verify_delivery_candidate(
            &manifest_bytes,
            &signature,
            &trust,
            None,
            &mut AcceptVerifier,
        )?)
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        encode_lower_hex(Sha256::digest(bytes).as_slice())
    }

    #[test]
    fn exact_assets_are_create_only_cached_and_reused_without_network() -> TestResult {
        let directory = TestDirectory::create("exact")?;
        let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
        let bridge = b"bridge-archive";
        let runtime = b"runtime-archive";
        let candidate = candidate(bridge, runtime)?;
        let mut fetcher = FakeFetcher {
            bridge: bridge.to_vec(),
            runtime: runtime.to_vec(),
            calls: Vec::new(),
        };
        let first = download_verified_delivery(&root, &candidate, &mut fetcher)?;
        assert_eq!(fs::read(first.profile_bridge())?, bridge);
        assert_eq!(fs::read(first.runtime_bundle())?, runtime);
        assert_eq!(
            fetcher.calls,
            [
                PROFILE_BRIDGE_ASSET.to_owned(),
                RUNTIME_BUNDLE_ASSET.to_owned()
            ]
        );

        let second = download_verified_delivery(&root, &candidate, &mut RejectingFetcher)?;
        assert_eq!(second, first);
        Ok(())
    }

    #[test]
    fn downloaded_identity_mismatch_is_removed_and_fails_closed() -> TestResult {
        let directory = TestDirectory::create("mismatch")?;
        let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
        let candidate = candidate(b"expected-bridge", b"expected-runtime")?;
        let mut fetcher = FakeFetcher {
            bridge: b"wrong-bridge".to_vec(),
            runtime: b"expected-runtime".to_vec(),
            calls: Vec::new(),
        };
        assert_eq!(
            download_verified_delivery(&root, &candidate, &mut fetcher),
            Err(DeliveryDownloadError::ArtifactIdentityMismatch)
        );
        let release_directory = root.path().join(&candidate.manifest().release_set_id);
        assert!(!release_directory.join(PROFILE_BRIDGE_ASSET).exists());
        assert!(fs::read_dir(release_directory)?.next().is_none());
        Ok(())
    }

    #[test]
    fn conflicting_existing_cache_is_never_overwritten() -> TestResult {
        let directory = TestDirectory::create("conflict")?;
        let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
        let candidate = candidate(b"expected-bridge", b"expected-runtime")?;
        let release_directory = root.path().join(&candidate.manifest().release_set_id);
        fs::create_dir(&release_directory)?;
        fs::write(release_directory.join(PROFILE_BRIDGE_ASSET), b"corrupt")?;
        assert_eq!(
            download_verified_delivery(&root, &candidate, &mut RejectingFetcher),
            Err(DeliveryDownloadError::ExistingArtifactMismatch)
        );
        assert_eq!(
            fs::read(release_directory.join(PROFILE_BRIDGE_ASSET))?,
            b"corrupt"
        );
        Ok(())
    }

    #[test]
    fn release_url_is_exact_and_never_discovers_latest() {
        let release_set_id = format!("{RELEASE_SET_PREFIX}{}", "a".repeat(64));
        assert_eq!(
            release_asset_url(&release_set_id, PROFILE_BRIDGE_ASSET),
            Some(format!(
                "{RELEASE_BASE_URL}/{release_set_id}/{PROFILE_BRIDGE_ASSET}"
            ))
        );
        assert!(release_asset_url("latest", PROFILE_BRIDGE_ASSET).is_none());
        assert!(release_asset_url(&release_set_id, "arbitrary.zip").is_none());
    }

    #[test]
    fn relative_or_linked_download_root_is_rejected() -> TestResult {
        assert_eq!(
            DeliveryDownloadRoot::open_or_create("relative"),
            Err(DeliveryDownloadError::InvalidRoot)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let directory = TestDirectory::create("linked-root")?;
            let target = directory.0.join("target");
            let linked = directory.0.join("linked");
            fs::create_dir(&target)?;
            symlink(&target, &linked)?;
            assert_eq!(
                DeliveryDownloadRoot::open_or_create(linked),
                Err(DeliveryDownloadError::InvalidRoot)
            );
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn system_fetcher_uses_absolute_system_powershell() -> TestResult {
        let fetcher = WindowsReleaseAssetFetcher::from_system()?;
        assert!(fetcher.powershell_executable().is_absolute());
        assert!(fetcher.powershell_executable().is_file());
        Ok(())
    }
}
