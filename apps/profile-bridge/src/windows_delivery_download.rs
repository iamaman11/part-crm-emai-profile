#![forbid(unsafe_code)]

use crate::windows_delivery::{
    DeliveryIdentity, VerifiedDeliveryCandidate, WindowsDeliveryComponent,
};
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
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

const RELEASE_SET_PREFIX: &str = "release-set-v3-sha256-";
const RUNTIME_BUNDLE_PREFIX: &str = "runtime-bundle-v3-sha256-";
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
const FETCH_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(windows)]
const FETCH_NO_BYTE_TIMEOUT: Duration = Duration::from_secs(60);

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
    # Use Windows' configured user/system proxy. Do not log the proxy or credentials.
    $handler.UseProxy = $true
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
        $null -eq $response.RequestMessage -or
        $response.RequestMessage.RequestUri.Scheme -cne 'https'
    ) {
        $code = 24
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
    # Only a closed numeric category crosses the installer boundary. Never emit
    # the exception message, URL, proxy, stack trace or credentials.
    $exception = $_.Exception
    while ($null -ne $exception.InnerException) {
        $exception = $exception.InnerException
    }
    if ($exception -is [System.Net.Sockets.SocketException]) {
        if ($exception.SocketErrorCode -eq [System.Net.Sockets.SocketError]::HostNotFound -or
            $exception.SocketErrorCode -eq [System.Net.Sockets.SocketError]::NoData -or
            $exception.SocketErrorCode -eq [System.Net.Sockets.SocketError]::TryAgain) {
            $code = 31 # DNS
        } else {
            $code = 32 # CONNECT
        }
    } elseif ($exception -is [System.Security.Authentication.AuthenticationException]) {
        $code = 33 # TLS
    } elseif ($exception -is [System.TimeoutException] -or
              $exception -is [System.Threading.Tasks.TaskCanceledException]) {
        $code = 34 # TIMEOUT
    } elseif ($exception -is [System.IO.IOException] -or
              $exception -is [System.UnauthorizedAccessException]) {
        $code = 36 # DISK (read or write failure indistinguishable at this layer)
    } else {
        $code = 30 # UNCLASSIFIED_NETWORK_OR_IO
    }
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
    active: Option<&DeliveryIdentity>,
    fetcher: &mut F,
) -> Result<DownloadedDeliveryArtifacts, DeliveryDownloadError> {
    validate_directory_path(root.path())?;
    let manifest = candidate.manifest();
    if !prefixed_sha256(&manifest.release_set_id, RELEASE_SET_PREFIX) {
        return Err(DeliveryDownloadError::InvalidCandidate);
    }
    validate_component_bounds(&manifest.components.profile_bridge)?;
    validate_component_bounds(&manifest.components.runtime_bundle)?;
    let runtime = &manifest.components.runtime_bundle;
    if !prefixed_sha256(&runtime.release_id, RUNTIME_BUNDLE_PREFIX) {
        return Err(DeliveryDownloadError::InvalidCandidate);
    }

    let release_directory = root.path().join(&manifest.release_set_id);
    ensure_directory(&release_directory)?;
    let profile_bridge = materialize_asset(
        &release_directory,
        &manifest.release_set_id,
        PROFILE_BRIDGE_ASSET,
        &manifest.components.profile_bridge,
        None,
        fetcher,
    )?;
    // Runtime bytes have their own immutable component identity. A Bridge-only update must not
    // key the same large archive by another whole Release Set and download it again.
    let runtime_directory = root.path().join(&runtime.release_id);
    ensure_directory(&runtime_directory)?;
    let active_archive = active
        .filter(|identity| {
            identity.runtime_bundle_release_id == runtime.release_id
                && prefixed_sha256(&identity.release_set_id, RELEASE_SET_PREFIX)
        })
        .map(|identity| {
            root.path()
                .join(&identity.release_set_id)
                .join(RUNTIME_BUNDLE_ASSET)
        });
    let runtime_bundle = materialize_asset(
        &runtime_directory,
        &runtime.release_id,
        RUNTIME_BUNDLE_ASSET,
        runtime,
        active_archive.as_deref(),
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
    active_archive: Option<&Path>,
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
                #[cfg(windows)]
                emit_install_stage(asset_name, "cached");
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
    let seeded = seed_active_archive(active_archive, &pending_path, component)?;
    #[cfg(windows)]
    if seeded {
        emit_install_stage(asset_name, "cached");
    } else {
        emit_install_stage(asset_name, "download");
    }
    if !seeded
        && fetcher
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
    #[cfg(windows)]
    emit_install_stage(asset_name, "verifying");
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
            #[cfg(windows)]
            emit_install_stage(asset_name, "verified");
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
                #[cfg(windows)]
                emit_install_stage(asset_name, "verified");
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

fn seed_active_archive(
    source: Option<&Path>,
    pending: &Path,
    component: &WindowsDeliveryComponent,
) -> Result<bool, DeliveryDownloadError> {
    let Some(source) = source else {
        return Ok(false);
    };
    let parent = source.parent().ok_or(DeliveryDownloadError::InvalidRoot)?;
    let parent_metadata = match fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(DeliveryDownloadError::Io),
    };
    validate_directory_metadata(&parent_metadata)?;
    match fs::symlink_metadata(source) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(DeliveryDownloadError::Io),
    }
    if !verify_file_identity(
        source,
        component.artifact_size_bytes,
        &component.artifact_sha256,
    )? {
        return Ok(false);
    }
    // Only the named active cache is eligible, never a history scan or a source of authority.
    // Existing pending-file verification and create-only publication still check these bytes.
    fs::hard_link(source, pending).map_err(|_| DeliveryDownloadError::Io)?;
    Ok(true)
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
fn release_asset_url(release_id: &str, asset_name: &str) -> Option<String> {
    let valid_release = match asset_name {
        PROFILE_BRIDGE_ASSET => prefixed_sha256(release_id, RELEASE_SET_PREFIX),
        RUNTIME_BUNDLE_ASSET => prefixed_sha256(release_id, RUNTIME_BUNDLE_PREFIX),
        _ => false,
    };
    if !valid_release {
        return None;
    }
    Some(format!("{RELEASE_BASE_URL}/{release_id}/{asset_name}"))
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
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: setup owns the visible UI.
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
            .and_then(|mut child| {
                wait_for_fetch_process(&mut child, destination, expected_size_bytes, asset_name)
            });
        let cleanup =
            fs::remove_file(&script_path).map_err(|_| WindowsReleaseAssetFetcherError::ScratchIo);
        let outcome = match (status, cleanup) {
            (Ok(status), Ok(())) => fetch_exit_disposition(status.code()),
            (Err(error), _) => Err(error),
            (_, Err(error)) => Err(error),
        };
        if let Err(error) = outcome {
            // Static categories only. Never print the URL, raw PowerShell error,
            // proxy configuration, local path or any profile content.
            emit_install_failure(asset_name, error.category());
            // The canonical download owner knows the typed network failure.
            // Persist a safe observation at the already-owned installation root.
            if let Some(root) = destination
                .parent()
                .and_then(Path::parent)
                .filter(|downloads| {
                    downloads
                        .file_name()
                        .is_some_and(|name| name == "downloads")
                })
                .and_then(Path::parent)
            {
                crate::user_diagnostics::report::record_download_failure(
                    root,
                    error.diagnostic_cause(),
                );
            }
        }
        outcome
    }
}

#[cfg(windows)]
fn validate_download_destination(path: &Path) -> Result<(), WindowsReleaseAssetFetcherError> {
    if !path.is_absolute() {
        return Err(WindowsReleaseAssetFetcherError::InvalidRequest);
    }
    match fs::symlink_metadata(path) {
        Ok(_) => return Err(WindowsReleaseAssetFetcherError::InvalidRequest),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(WindowsReleaseAssetFetcherError::InvalidRequest),
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
    destination: &Path,
    expected_size: u64,
    asset_name: &str,
) -> Result<ExitStatus, WindowsReleaseAssetFetcherError> {
    wait_for_fetch_process_with_limits(
        child,
        destination,
        expected_size,
        asset_name,
        FETCH_NO_BYTE_TIMEOUT,
        FETCH_PROCESS_TIMEOUT,
    )
}

#[cfg(windows)]
fn wait_for_fetch_process_with_limits(
    child: &mut Child,
    destination: &Path,
    expected_size: u64,
    asset_name: &str,
    idle_limit: Duration,
    total_limit: Duration,
) -> Result<ExitStatus, WindowsReleaseAssetFetcherError> {
    let started = Instant::now();
    let mut last_byte_at = started;
    let mut last_bytes = 0_u64;
    let mut last_report_second = u64::MAX;
    loop {
        // Poll the actual destination file. No child output or remote headers are
        // trusted to claim progress; the final size + SHA256 is still authoritative.
        match fs::symlink_metadata(destination) {
            Ok(metadata) if metadata.is_file() && !metadata_is_link_or_reparse(&metadata) => {
                let bytes = metadata.len();
                if bytes < last_bytes || bytes > expected_size {
                    terminate_fetch(child)?;
                    return Err(WindowsReleaseAssetFetcherError::Size);
                }
                if bytes != last_bytes {
                    last_bytes = bytes;
                    last_byte_at = Instant::now();
                    last_report_second = u64::MAX;
                }
            }
            Ok(_) => {
                terminate_fetch(child)?;
                return Err(WindowsReleaseAssetFetcherError::ScratchIo);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                terminate_fetch(child)?;
                return Err(WindowsReleaseAssetFetcherError::ScratchIo);
            }
        }
        let idle = last_byte_at.elapsed();
        if last_report_second != idle.as_secs() {
            emit_install_progress(asset_name, last_bytes, expected_size, idle.as_secs());
            last_report_second = idle.as_secs();
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(_) => return Err(WindowsReleaseAssetFetcherError::Process),
        }
        if started.elapsed() >= total_limit {
            terminate_fetch(child)?;
            return Err(WindowsReleaseAssetFetcherError::Timeout);
        }
        if idle >= idle_limit {
            terminate_fetch(child)?;
            return Err(WindowsReleaseAssetFetcherError::Stalled);
        }
        thread::sleep(FETCH_PROCESS_POLL_INTERVAL);
    }
}

#[cfg(windows)]
fn terminate_fetch(child: &mut Child) -> Result<(), WindowsReleaseAssetFetcherError> {
    // On a stalled socket, the only permitted termination is the installer-owned
    // network helper, never Firefox, Bridge user sessions or profile processes.
    child
        .kill()
        .map_err(|_| WindowsReleaseAssetFetcherError::Process)?;
    child
        .wait()
        .map_err(|_| WindowsReleaseAssetFetcherError::Process)?;
    Ok(())
}

#[cfg(windows)]
fn emit_install_progress(asset: &str, bytes: u64, total: u64, idle_seconds: u64) {
    println!("PROFILE_BRIDGE_DOWNLOAD_PROGRESS_V1={asset}|{bytes}|{total}|{idle_seconds}");
    let _ = io::stdout().flush();
}

#[cfg(windows)]
pub(crate) fn emit_install_stage(asset: &str, stage: &str) {
    println!("PROFILE_BRIDGE_DOWNLOAD_STAGE_V1={asset}|{stage}");
    let _ = io::stdout().flush();
}

#[cfg(windows)]
fn emit_install_failure(asset: &str, category: &str) {
    println!("PROFILE_BRIDGE_DOWNLOAD_FAILURE_V1={asset}|{category}");
    let _ = io::stdout().flush();
}

#[cfg(windows)]
fn fetch_exit_disposition(code: Option<i32>) -> Result<(), WindowsReleaseAssetFetcherError> {
    match code {
        Some(0) => Ok(()),
        Some(20) => Err(WindowsReleaseAssetFetcherError::Http),
        Some(21..=23) => Err(WindowsReleaseAssetFetcherError::Size),
        Some(24) => Err(WindowsReleaseAssetFetcherError::InsecureRedirect),
        Some(31) => Err(WindowsReleaseAssetFetcherError::Dns),
        Some(32) => Err(WindowsReleaseAssetFetcherError::Connect),
        Some(33) => Err(WindowsReleaseAssetFetcherError::Tls),
        Some(34) => Err(WindowsReleaseAssetFetcherError::Timeout),
        Some(36) => Err(WindowsReleaseAssetFetcherError::Disk),
        _ => Err(WindowsReleaseAssetFetcherError::Process),
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsReleaseAssetFetcherError {
    InvalidRequest,
    PlatformUnavailable,
    ScratchIo,
    Http,
    Size,
    InsecureRedirect,
    Dns,
    Connect,
    Tls,
    Disk,
    Process,
    Timeout,
    Stalled,
}

#[cfg(windows)]
impl WindowsReleaseAssetFetcherError {
    const fn diagnostic_cause(self) -> crate::user_diagnostics::report::Cause {
        use crate::user_diagnostics::report::Cause;
        match self {
            Self::Dns => Cause::DownloadDns,
            Self::Connect => Cause::DownloadConnect,
            Self::Tls => Cause::DownloadTls,
            Self::Http => Cause::DownloadHttp,
            Self::Timeout => Cause::DownloadTimeout,
            Self::Stalled => Cause::DownloadStalled,
            Self::Size => Cause::DownloadSize,
            Self::Disk | Self::ScratchIo => Cause::DownloadDisk,
            Self::InsecureRedirect => Cause::DownloadRedirect,
            Self::Process | Self::PlatformUnavailable | Self::InvalidRequest => {
                Cause::DownloadProcess
            }
        }
    }

    const fn category(self) -> &'static str {
        match self {
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::PlatformUnavailable => "PLATFORM",
            Self::ScratchIo | Self::Disk => "DISK",
            Self::Http => "HTTP",
            Self::Size => "SIZE",
            Self::InsecureRedirect => "INSECURE_REDIRECT",
            Self::Dns => "DNS",
            Self::Connect => "CONNECT",
            Self::Tls => "TLS",
            Self::Process => "NETWORK_OR_PROCESS",
            Self::Timeout => "TIMEOUT",
            Self::Stalled => "READ_STALLED",
        }
    }
}

#[cfg(windows)]
impl fmt::Display for WindowsReleaseAssetFetcherError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "Windows release asset request is invalid",
            Self::PlatformUnavailable => "Windows release asset platform is unavailable",
            Self::ScratchIo => "Windows release asset scratch operation failed",
            Self::Http => "Windows release asset request was rejected by the server",
            Self::Size => "Windows release asset size did not match the signed manifest",
            Self::InsecureRedirect => "Windows release asset redirect did not remain HTTPS",
            Self::Dns => "Windows release asset DNS resolution failed",
            Self::Connect => "Windows release asset connection failed",
            Self::Tls => "Windows release asset secure connection failed",
            Self::Disk => "Windows release asset read or write failed",
            Self::Process => "Windows release asset fetch process failed",
            Self::Timeout => "Windows release asset fetch exceeded its deadline",
            Self::Stalled => "Windows release asset download made no progress",
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
        calls: Vec<(String, String)>,
    }

    impl DeliveryAssetFetcher for FakeFetcher {
        type Error = ();

        fn fetch_release_asset(
            &mut self,
            release_set_id: &str,
            asset_name: &str,
            destination: &Path,
            _expected_size_bytes: u64,
        ) -> Result<(), Self::Error> {
            self.calls
                .push((release_set_id.to_owned(), asset_name.to_owned()));
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

    #[cfg(windows)]
    #[test]
    fn native_silent_fetch_exits_with_bounded_stall_and_cleans_child() -> TestResult {
        let directory = TestDirectory::create("native-stall")?;
        // No destination appears: this simulates the observed zero-byte
        // response-header/connect hang without using a network endpoint.
        let mut child = Command::new(system_powershell_executable()?)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let result = wait_for_fetch_process_with_limits(
            &mut child,
            &directory.0.join("still-missing.pending"),
            1_188_812_800,
            RUNTIME_BUNDLE_ASSET,
            Duration::from_secs(2),
            Duration::from_secs(10),
        );
        assert_eq!(result, Err(WindowsReleaseAssetFetcherError::Stalled));
        assert!(child.try_wait()?.is_some(), "installer fetch child leaked");
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn network_categories_never_embed_urls_or_credentials() {
        for (code, category) in [
            (20, "HTTP"),
            (21, "SIZE"),
            (22, "SIZE"),
            (23, "SIZE"),
            (24, "INSECURE_REDIRECT"),
            (31, "DNS"),
            (32, "CONNECT"),
            (33, "TLS"),
            (34, "TIMEOUT"),
            (36, "DISK"),
        ] {
            assert_eq!(
                fetch_exit_disposition(Some(code)).unwrap_err().category(),
                category
            );
        }
        assert_eq!(fetch_exit_disposition(Some(0)), Ok(()));
        assert_eq!(
            fetch_exit_disposition(Some(30)).unwrap_err().category(),
            "NETWORK_OR_PROCESS"
        );
        assert!(FETCH_SCRIPT.contains("$handler.UseProxy = $true"));
        assert!(!FETCH_SCRIPT.contains("Write-Host"));
        assert!(!FETCH_SCRIPT.contains("Write-Error"));
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
        release_hex: char,
    ) -> Result<VerifiedDeliveryCandidate, Box<dyn std::error::Error>> {
        let bridge_sha = sha256_hex(bridge);
        let runtime_sha = sha256_hex(runtime);
        let manifest = WindowsDeliveryManifest {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY".to_owned(),
            release_set_id: format!("{RELEASE_SET_PREFIX}{}", release_hex.to_string().repeat(64)),
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
                    release_id: format!("{RUNTIME_BUNDLE_PREFIX}{runtime_sha}"),
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
                runtime_bundle_version: "3.0.0".to_owned(),
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
        let candidate = candidate(bridge, runtime, 'a')?;
        let mut fetcher = FakeFetcher {
            bridge: bridge.to_vec(),
            runtime: runtime.to_vec(),
            calls: Vec::new(),
        };
        let first = download_verified_delivery(&root, &candidate, None, &mut fetcher)?;
        assert_eq!(fs::read(first.profile_bridge())?, bridge);
        assert_eq!(fs::read(first.runtime_bundle())?, runtime);
        assert_eq!(
            fetcher.calls,
            [
                (
                    candidate.manifest().release_set_id.clone(),
                    PROFILE_BRIDGE_ASSET.to_owned()
                ),
                (
                    candidate
                        .manifest()
                        .components
                        .runtime_bundle
                        .release_id
                        .clone(),
                    RUNTIME_BUNDLE_ASSET.to_owned()
                )
            ]
        );

        let second = download_verified_delivery(&root, &candidate, None, &mut RejectingFetcher)?;
        assert_eq!(second, first);
        Ok(())
    }

    #[test]
    fn different_release_sets_share_runtime_and_fetch_only_new_bridge() -> TestResult {
        let directory = TestDirectory::create("component-reuse")?;
        let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
        let original = candidate(b"old-bridge", b"same-runtime", 'a')?;
        let update = candidate(b"new-bridge", b"same-runtime", 'b')?;
        let mut fetcher = FakeFetcher {
            bridge: b"old-bridge".to_vec(),
            runtime: b"same-runtime".to_vec(),
            calls: Vec::new(),
        };
        let first = download_verified_delivery(&root, &original, None, &mut fetcher)?;
        fetcher.bridge = b"new-bridge".to_vec();
        let second =
            download_verified_delivery(&root, &update, Some(&original.identity()), &mut fetcher)?;
        assert_ne!(
            original.identity().release_set_id,
            update.identity().release_set_id
        );
        assert_eq!(first.runtime_bundle(), second.runtime_bundle());
        assert_eq!(fs::read(second.profile_bridge())?, b"new-bridge");
        assert_eq!(fetcher.calls.len(), 3);
        assert_eq!(
            fetcher.calls[2],
            (
                update.manifest().release_set_id.clone(),
                PROFILE_BRIDGE_ASSET.to_owned()
            )
        );
        Ok(())
    }

    #[test]
    fn named_active_archive_seeds_component_cache_without_network() -> TestResult {
        let directory = TestDirectory::create("active-reuse")?;
        let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
        let active = candidate(b"old-bridge", b"same-runtime", 'a')?;
        let update = candidate(b"new-bridge", b"same-runtime", 'b')?;
        let old = root.path().join(&active.manifest().release_set_id);
        fs::create_dir(&old)?;
        fs::write(old.join(RUNTIME_BUNDLE_ASSET), b"same-runtime")?;
        let new = root.path().join(&update.manifest().release_set_id);
        fs::create_dir(&new)?;
        fs::write(new.join(PROFILE_BRIDGE_ASSET), b"new-bridge")?;
        let artifacts = download_verified_delivery(
            &root,
            &update,
            Some(&active.identity()),
            &mut RejectingFetcher,
        )?;
        assert_eq!(
            artifacts.runtime_bundle().parent(),
            Some(
                root.path()
                    .join(&update.manifest().components.runtime_bundle.release_id)
                    .as_path()
            )
        );
        assert_eq!(fs::read(artifacts.runtime_bundle())?, b"same-runtime");
        assert_eq!(fs::read(old.join(RUNTIME_BUNDLE_ASSET))?, b"same-runtime");
        Ok(())
    }

    #[test]
    fn missing_corrupt_or_different_active_runtime_requires_exact_fetch() -> TestResult {
        for case in ["missing", "corrupt", "different"] {
            let directory = TestDirectory::create(case)?;
            let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
            let active = candidate(b"old-bridge", b"old-runtime", 'a')?;
            let update = candidate(
                b"new-bridge",
                if case == "different" {
                    b"new-runtime"
                } else {
                    b"old-runtime"
                },
                'b',
            )?;
            let old = root.path().join(&active.manifest().release_set_id);
            fs::create_dir(&old)?;
            if case != "missing" {
                fs::write(
                    old.join(RUNTIME_BUNDLE_ASSET),
                    if case == "corrupt" {
                        b"bad-runtime"
                    } else {
                        b"old-runtime"
                    },
                )?;
            }
            let new = root.path().join(&update.manifest().release_set_id);
            fs::create_dir(&new)?;
            fs::write(new.join(PROFILE_BRIDGE_ASSET), b"new-bridge")?;
            assert_eq!(
                download_verified_delivery(
                    &root,
                    &update,
                    Some(&active.identity()),
                    &mut RejectingFetcher
                ),
                Err(DeliveryDownloadError::FetchFailed)
            );
            let expected = if case == "different" {
                b"new-runtime"
            } else {
                b"old-runtime"
            };
            let mut fetcher = FakeFetcher {
                bridge: b"new-bridge".to_vec(),
                runtime: expected.to_vec(),
                calls: Vec::new(),
            };
            let artifacts =
                download_verified_delivery(&root, &update, Some(&active.identity()), &mut fetcher)?;
            assert_eq!(fs::read(artifacts.runtime_bundle())?, expected);
            assert_eq!(
                fetcher.calls,
                [(
                    update
                        .manifest()
                        .components
                        .runtime_bundle
                        .release_id
                        .clone(),
                    RUNTIME_BUNDLE_ASSET.to_owned()
                )]
            );
        }
        Ok(())
    }

    #[test]
    fn conflicting_component_cache_is_preserved_and_rejected() -> TestResult {
        let directory = TestDirectory::create("component-conflict")?;
        let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
        let update = candidate(b"new-bridge", b"same-runtime", 'b')?;
        let new = root.path().join(&update.manifest().release_set_id);
        fs::create_dir(&new)?;
        fs::write(new.join(PROFILE_BRIDGE_ASSET), b"new-bridge")?;
        let runtime = root
            .path()
            .join(&update.manifest().components.runtime_bundle.release_id);
        fs::create_dir(&runtime)?;
        fs::write(runtime.join(RUNTIME_BUNDLE_ASSET), b"corrupt")?;
        assert_eq!(
            download_verified_delivery(&root, &update, None, &mut RejectingFetcher),
            Err(DeliveryDownloadError::ExistingArtifactMismatch)
        );
        assert_eq!(fs::read(runtime.join(RUNTIME_BUNDLE_ASSET))?, b"corrupt");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn linked_active_cache_cannot_seed_admitted_bytes() -> TestResult {
        use std::os::unix::fs::symlink;
        for linked_parent in [false, true] {
            let directory = TestDirectory::create("active-link")?;
            let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
            let active = candidate(b"old-bridge", b"same-runtime", 'a')?;
            let update = candidate(b"new-bridge", b"same-runtime", 'b')?;
            let outside = directory.0.join("outside-cache");
            fs::create_dir(&outside)?;
            fs::write(outside.join(RUNTIME_BUNDLE_ASSET), b"same-runtime")?;
            let old = root.path().join(&active.manifest().release_set_id);
            if linked_parent {
                symlink(&outside, &old)?;
            } else {
                fs::create_dir(&old)?;
                symlink(
                    outside.join(RUNTIME_BUNDLE_ASSET),
                    old.join(RUNTIME_BUNDLE_ASSET),
                )?;
            }
            let new = root.path().join(&update.manifest().release_set_id);
            fs::create_dir(&new)?;
            fs::write(new.join(PROFILE_BRIDGE_ASSET), b"new-bridge")?;
            let expected = if linked_parent {
                DeliveryDownloadError::InvalidRoot
            } else {
                DeliveryDownloadError::FetchFailed
            };
            assert_eq!(
                download_verified_delivery(
                    &root,
                    &update,
                    Some(&active.identity()),
                    &mut RejectingFetcher
                ),
                Err(expected)
            );
        }
        Ok(())
    }

    #[test]
    fn downloaded_identity_mismatch_is_removed_and_fails_closed() -> TestResult {
        let directory = TestDirectory::create("mismatch")?;
        let root = DeliveryDownloadRoot::open_or_create(directory.0.join("cache"))?;
        let candidate = candidate(b"expected-bridge", b"expected-runtime", 'a')?;
        let mut fetcher = FakeFetcher {
            bridge: b"wrong-bridge".to_vec(),
            runtime: b"expected-runtime".to_vec(),
            calls: Vec::new(),
        };
        assert_eq!(
            download_verified_delivery(&root, &candidate, None, &mut fetcher),
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
        let candidate = candidate(b"expected-bridge", b"expected-runtime", 'a')?;
        let release_directory = root.path().join(&candidate.manifest().release_set_id);
        fs::create_dir(&release_directory)?;
        fs::write(release_directory.join(PROFILE_BRIDGE_ASSET), b"corrupt")?;
        assert_eq!(
            download_verified_delivery(&root, &candidate, None, &mut RejectingFetcher),
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
        let runtime_release_id = format!("{RUNTIME_BUNDLE_PREFIX}{}", "b".repeat(64));
        assert_eq!(
            release_asset_url(&release_set_id, PROFILE_BRIDGE_ASSET),
            Some(format!(
                "{RELEASE_BASE_URL}/{release_set_id}/{PROFILE_BRIDGE_ASSET}"
            ))
        );
        assert_eq!(
            release_asset_url(&runtime_release_id, RUNTIME_BUNDLE_ASSET),
            Some(format!(
                "{RELEASE_BASE_URL}/{runtime_release_id}/{RUNTIME_BUNDLE_ASSET}"
            ))
        );
        assert!(release_asset_url(&release_set_id, RUNTIME_BUNDLE_ASSET).is_none());
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

    #[cfg(windows)]
    #[test]
    fn system_fetcher_rejects_preexisting_destination_without_network() -> TestResult {
        let directory = TestDirectory::create("existing-destination")?;
        let destination = directory.0.join(PROFILE_BRIDGE_ASSET);
        fs::write(&destination, b"occupied")?;
        let mut fetcher = WindowsReleaseAssetFetcher::from_system()?;
        let release_set_id = format!("{RELEASE_SET_PREFIX}{}", "a".repeat(64));
        assert_eq!(
            fetcher.fetch_release_asset(&release_set_id, PROFILE_BRIDGE_ASSET, &destination, 1,),
            Err(WindowsReleaseAssetFetcherError::InvalidRequest)
        );
        assert_eq!(fs::read(destination)?, b"occupied");
        Ok(())
    }
}
