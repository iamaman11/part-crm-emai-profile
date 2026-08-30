#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::windows_delivery::DetachedSignatureVerifier;
use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const VERIFY_SCRIPT: &str = r#"param(
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [Parameter(Mandatory=$true)][string]$SignaturePath,
    [Parameter(Mandatory=$true)][string]$ExpectedCertificateSha256
)
$ErrorActionPreference = 'Stop'
try {
    Add-Type -AssemblyName System.Security
    $manifest = [System.IO.File]::ReadAllBytes($ManifestPath)
    $signature = [System.IO.File]::ReadAllBytes($SignaturePath)
    $contentInfo = [System.Security.Cryptography.Pkcs.ContentInfo]::new($manifest)
    $cms = [System.Security.Cryptography.Pkcs.SignedCms]::new($contentInfo, $true)
    $cms.Decode($signature)
    if ($cms.SignerInfos.Count -ne 1) { exit 20 }
    $signer = $cms.SignerInfos[0]
    $signer.CheckSignature($true)
    $certificate = $signer.Certificate
    if ($null -eq $certificate) { exit 21 }

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $certificateSha256 = [System.BitConverter]::ToString(
            $sha256.ComputeHash($certificate.RawData)
        ).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
    if ($certificateSha256 -cne $ExpectedCertificateSha256) { exit 22 }

    $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
    try {
        $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::Online
        $chain.ChainPolicy.RevocationFlag = [System.Security.Cryptography.X509Certificates.X509RevocationFlag]::ExcludeRoot
        $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::NoFlag
        $chain.ChainPolicy.UrlRetrievalTimeout = [System.TimeSpan]::FromSeconds(10)
        [void]$chain.ChainPolicy.ApplicationPolicy.Add(
            [System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.3')
        )
        if (-not $chain.Build($certificate)) { exit 23 }
    }
    finally {
        $chain.Dispose()
    }
    exit 0
}
catch [System.Security.Cryptography.CryptographicException] {
    exit 24
}
catch {
    exit 30
}
"#;

const SCRIPT_NAME: &str = "verify-delivery-cms.ps1";
const MANIFEST_NAME: &str = "delivery-manifest.json";
const SIGNATURE_NAME: &str = "delivery-signature.p7s";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const SCRATCH_ATTEMPTS: usize = 32;
const VERIFICATION_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const VERIFICATION_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsCmsSignatureVerifier {
    scratch_root: PathBuf,
    powershell_executable: PathBuf,
}

impl WindowsCmsSignatureVerifier {
    pub fn from_system(scratch_root: impl Into<PathBuf>) -> Result<Self, WindowsCmsVerifierError> {
        let scratch_root = scratch_root.into();
        validate_directory(&scratch_root)
            .map_err(|_| WindowsCmsVerifierError::InvalidScratchRoot)?;
        let powershell_executable = system_powershell_executable()?;
        Ok(Self {
            scratch_root,
            powershell_executable,
        })
    }

    #[must_use]
    pub fn scratch_root(&self) -> &Path {
        &self.scratch_root
    }

    #[must_use]
    pub fn powershell_executable(&self) -> &Path {
        &self.powershell_executable
    }

    fn verify_detached_cms(
        &self,
        manifest_bytes: &[u8],
        cms_der: &[u8],
        expected_certificate_sha256: &str,
    ) -> Result<bool, WindowsCmsVerifierError> {
        if manifest_bytes.is_empty()
            || !has_exact_der_sequence_envelope(cms_der)
            || !is_lower_hex(expected_certificate_sha256, 64)
        {
            return Ok(false);
        }
        validate_directory(&self.scratch_root)
            .map_err(|_| WindowsCmsVerifierError::InvalidScratchRoot)?;
        validate_regular_file(&self.powershell_executable)
            .map_err(|_| WindowsCmsVerifierError::PlatformUnavailable)?;

        let scratch = VerificationScratch::create(&self.scratch_root)?;
        if let Err(error) = scratch.write_inputs(manifest_bytes, cms_der) {
            let cleanup = scratch.cleanup();
            cleanup?;
            return Err(error);
        }

        let mut command = Command::new(&self.powershell_executable);
        command
            .env_clear()
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
            .arg(scratch.script_path())
            .arg("-ManifestPath")
            .arg(scratch.manifest_path())
            .arg("-SignaturePath")
            .arg(scratch.signature_path())
            .arg("-ExpectedCertificateSha256")
            .arg(expected_certificate_sha256)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        inherit_windows_system_environment(&mut command);
        let status = command
            .spawn()
            .map_err(|_| WindowsCmsVerifierError::VerificationProcess)
            .and_then(|mut child| wait_for_verification_process(&mut child));
        let cleanup = scratch.cleanup();
        let status = status?;
        cleanup?;
        let code = status
            .code()
            .ok_or(WindowsCmsVerifierError::VerificationProcess)?;
        match code {
            0 => Ok(true),
            20..=29 => Ok(false),
            _ => Err(WindowsCmsVerifierError::VerificationProcess),
        }
    }
}

impl DetachedSignatureVerifier for WindowsCmsSignatureVerifier {
    type Error = WindowsCmsVerifierError;

    fn verify_cms(
        &mut self,
        manifest_bytes: &[u8],
        cms_der: &[u8],
        expected_certificate_sha256: &str,
    ) -> Result<bool, Self::Error> {
        self.verify_detached_cms(manifest_bytes, cms_der, expected_certificate_sha256)
    }
}

fn wait_for_verification_process(child: &mut Child) -> Result<ExitStatus, WindowsCmsVerifierError> {
    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(_) => return Err(WindowsCmsVerifierError::VerificationProcess),
        }
        if started_at.elapsed() >= VERIFICATION_PROCESS_TIMEOUT {
            let kill_result = child.kill();
            let wait_result = child.wait();
            if kill_result.is_err() && wait_result.is_err() {
                return Err(WindowsCmsVerifierError::VerificationProcess);
            }
            return Err(WindowsCmsVerifierError::VerificationTimeout);
        }
        thread::sleep(VERIFICATION_PROCESS_POLL_INTERVAL);
    }
}

struct VerificationScratch {
    directory: PathBuf,
}

impl VerificationScratch {
    fn create(root: &Path) -> Result<Self, WindowsCmsVerifierError> {
        for _ in 0..SCRATCH_ATTEMPTS {
            let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = root.join(format!(".cms-verify-{}-{sequence}", std::process::id()));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    validate_directory(&directory)
                        .map_err(|_| WindowsCmsVerifierError::ScratchIo)?;
                    return Ok(Self { directory });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(WindowsCmsVerifierError::ScratchIo),
            }
        }
        Err(WindowsCmsVerifierError::ScratchIo)
    }

    fn write_inputs(
        &self,
        manifest_bytes: &[u8],
        cms_der: &[u8],
    ) -> Result<(), WindowsCmsVerifierError> {
        write_new_synced(&self.script_path(), VERIFY_SCRIPT.as_bytes())?;
        write_new_synced(&self.manifest_path(), manifest_bytes)?;
        write_new_synced(&self.signature_path(), cms_der)?;
        Ok(())
    }

    fn script_path(&self) -> PathBuf {
        self.directory.join(SCRIPT_NAME)
    }

    fn manifest_path(&self) -> PathBuf {
        self.directory.join(MANIFEST_NAME)
    }

    fn signature_path(&self) -> PathBuf {
        self.directory.join(SIGNATURE_NAME)
    }

    fn cleanup(self) -> Result<(), WindowsCmsVerifierError> {
        validate_directory(&self.directory).map_err(|_| WindowsCmsVerifierError::ScratchIo)?;
        for path in [
            self.script_path(),
            self.manifest_path(),
            self.signature_path(),
        ] {
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
                        return Err(WindowsCmsVerifierError::ScratchIo);
                    }
                    fs::remove_file(path).map_err(|_| WindowsCmsVerifierError::ScratchIo)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(WindowsCmsVerifierError::ScratchIo),
            }
        }
        if fs::read_dir(&self.directory)
            .map_err(|_| WindowsCmsVerifierError::ScratchIo)?
            .next()
            .is_some()
        {
            return Err(WindowsCmsVerifierError::ScratchIo);
        }
        fs::remove_dir(&self.directory).map_err(|_| WindowsCmsVerifierError::ScratchIo)
    }
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), WindowsCmsVerifierError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| WindowsCmsVerifierError::ScratchIo)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| WindowsCmsVerifierError::ScratchIo)
}

fn system_powershell_executable() -> Result<PathBuf, WindowsCmsVerifierError> {
    let system_root =
        env::var_os("SystemRoot").ok_or(WindowsCmsVerifierError::PlatformUnavailable)?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    validate_regular_file(&executable).map_err(|_| WindowsCmsVerifierError::PlatformUnavailable)?;
    Ok(executable)
}

fn inherit_windows_system_environment(command: &mut Command) {
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
}

fn validate_directory(path: &Path) -> Result<(), ()> {
    if !path.is_absolute() {
        return Err(());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(());
    }
    Ok(())
}

fn validate_regular_file(path: &Path) -> Result<(), ()> {
    if !path.is_absolute() {
        return Err(());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(());
    }
    Ok(())
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn has_exact_der_sequence_envelope(value: &[u8]) -> bool {
    if value.len() < 2 || value[0] != 0x30 {
        return false;
    }

    let first_length = value[1];
    let (header_length, content_length) = if first_length & 0x80 == 0 {
        (2usize, usize::from(first_length))
    } else {
        let length_bytes = usize::from(first_length & 0x7f);
        if length_bytes == 0
            || length_bytes > std::mem::size_of::<usize>()
            || value.len() < 2 + length_bytes
            || value[2] == 0
        {
            return false;
        }
        let mut content_length = 0usize;
        for byte in &value[2..2 + length_bytes] {
            let Some(next) = content_length
                .checked_mul(256)
                .and_then(|current| current.checked_add(usize::from(*byte)))
            else {
                return false;
            };
            content_length = next;
        }
        if content_length < 128 {
            return false;
        }
        (2 + length_bytes, content_length)
    };

    header_length.checked_add(content_length) == Some(value.len())
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsCmsVerifierError {
    InvalidScratchRoot,
    PlatformUnavailable,
    ScratchIo,
    VerificationProcess,
    VerificationTimeout,
}

impl fmt::Display for WindowsCmsVerifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidScratchRoot => "Windows release verification scratch root is invalid",
            Self::PlatformUnavailable => "Windows CMS verification platform is unavailable",
            Self::ScratchIo => "Windows CMS verification scratch operation failed",
            Self::VerificationProcess => "Windows CMS verification process failed",
            Self::VerificationTimeout => "Windows CMS verification process exceeded its deadline",
        })
    }
}

impl std::error::Error for WindowsCmsVerifierError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, std::io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-cms-{label}-{}-{sequence}",
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

    #[test]
    fn system_adapter_uses_absolute_windows_powershell_and_owned_scratch() -> TestResult {
        let directory = TestDirectory::create("system")?;
        let verifier = WindowsCmsSignatureVerifier::from_system(directory.0.clone())?;
        assert!(verifier.powershell_executable().is_absolute());
        assert!(verifier.powershell_executable().is_file());
        assert_eq!(verifier.scratch_root(), directory.0.as_path());
        Ok(())
    }

    #[test]
    fn malformed_cms_and_invalid_certificate_pin_fail_closed() -> TestResult {
        let directory = TestDirectory::create("negative")?;
        let mut verifier = WindowsCmsSignatureVerifier::from_system(directory.0.clone())?;
        assert!(!verifier.verify_cms(b"manifest", b"not-cms", &"a".repeat(64))?);
        assert!(!verifier.verify_cms(b"manifest", b"not-cms", "ABC")?);
        assert!(fs::read_dir(&directory.0)?.next().is_none());
        Ok(())
    }

    #[test]
    fn der_envelope_precheck_is_strict_and_bounded() {
        assert!(!has_exact_der_sequence_envelope(b"not-cms"));
        assert!(!has_exact_der_sequence_envelope(&[0x30, 0x80, 0x00, 0x00]));
        assert!(!has_exact_der_sequence_envelope(&[0x30, 0x81, 0x01, 0x00]));
        assert!(has_exact_der_sequence_envelope(&[0x30, 0x01, 0x00]));
        let mut long_form = vec![0x30, 0x81, 0x80];
        long_form.extend(std::iter::repeat_n(0, 128));
        assert!(has_exact_der_sequence_envelope(&long_form));
    }

    #[test]
    fn relative_or_missing_scratch_root_is_rejected() {
        assert_eq!(
            WindowsCmsSignatureVerifier::from_system(PathBuf::from("relative")),
            Err(WindowsCmsVerifierError::InvalidScratchRoot)
        );
        let missing =
            std::env::temp_dir().join(format!("profile-bridge-cms-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&missing);
        assert_eq!(
            WindowsCmsSignatureVerifier::from_system(missing),
            Err(WindowsCmsVerifierError::InvalidScratchRoot)
        );
    }
}
