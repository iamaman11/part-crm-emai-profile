#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::windows_delivery::DetachedSignatureVerifier;
use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

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
        $chain.ChainPolicy.RevocationFlag = [System.Security.Cryptography.X509Certificates.X509RevocationFlag]::EntireChain
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
static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsCmsSignatureVerifier {
    scratch_root: PathBuf,
    powershell_executable: PathBuf,
}

impl WindowsCmsSignatureVerifier {
    pub fn from_system(scratch_root: impl Into<PathBuf>) -> Result<Self, WindowsCmsVerifierError> {
        let scratch_root = scratch_root.into();
        validate_directory(&scratch_root).map_err(|_| WindowsCmsVerifierError::InvalidScratchRoot)?;
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
            || cms_der.is_empty()
            || !is_lower_hex(expected_certificate_sha256, 64)
        {
            return Ok(false);
        }
        validate_directory(&self.scratch_root)
            .map_err(|_| WindowsCmsVerifierError::InvalidScratchRoot)?;
        validate_regular_file(&self.powershell_executable)
            .map_err(|_| WindowsCmsVerifierError::PlatformUnavailable)?;

        let scratch = VerificationScratch::create(&self.scratch_root)?;
        scratch.write_inputs(manifest_bytes, cms_der)?;
        let status = Command::new(&self.powershell_executable)
            .env_clear()
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
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
        let mut command = status;
        inherit_windows_system_environment(&mut command);
        let status = command
            .status()
            .map_err(|_| WindowsCmsVerifierError::VerificationProcess)?;
        let cleanup = scratch.cleanup();
        let code = status
            .code()
            .ok_or(WindowsCmsVerifierError::VerificationProcess)?;
        cleanup?;
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
        self.verify_detached_cms(
            manifest_bytes,
            cms_der,
            expected_certificate_sha256,
        )
    }
}

struct VerificationScratch {
    directory: PathBuf,
}

impl VerificationScratch {
    fn create(root: &Path) -> Result<Self, WindowsCmsVerifierError> {
        for _ in 0..SCRATCH_ATTEMPTS {
            let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = root.join(format!(
                ".cms-verify-{}-{sequence}",
                std::process::id()
            ));
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
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| WindowsCmsVerifierError::ScratchIo)?;
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
                return Err(WindowsCmsVerifierError::ScratchIo);
            }
            fs::remove_file(path).map_err(|_| WindowsCmsVerifierError::ScratchIo)?;
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
    let system_root = env::var_os("SystemRoot").ok_or(WindowsCmsVerifierError::PlatformUnavailable)?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    validate_regular_file(&executable)
        .map_err(|_| WindowsCmsVerifierError::PlatformUnavailable)?;
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
}

impl fmt::Display for WindowsCmsVerifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidScratchRoot => "Windows release verification scratch root is invalid",
            Self::PlatformUnavailable => "Windows CMS verification platform is unavailable",
            Self::ScratchIo => "Windows CMS verification scratch operation failed",
            Self::VerificationProcess => "Windows CMS verification process failed",
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
    fn relative_or_missing_scratch_root_is_rejected() {
        assert_eq!(
            WindowsCmsSignatureVerifier::from_system(PathBuf::from("relative")),
            Err(WindowsCmsVerifierError::InvalidScratchRoot)
        );
        let missing = std::env::temp_dir().join(format!(
            "profile-bridge-cms-missing-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&missing);
        assert_eq!(
            WindowsCmsSignatureVerifier::from_system(missing),
            Err(WindowsCmsVerifierError::InvalidScratchRoot)
        );
    }
}
