#![cfg(windows)]
#![forbid(unsafe_code)]

use profile_bridge::windows_delivery::DetachedSignatureVerifier;
use profile_bridge::windows_delivery_signature::WindowsCmsSignatureVerifier;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const CREATE_FIXTURE_SCRIPT: &str = r#"param(
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [Parameter(Mandatory=$true)][string]$SignaturePath,
    [Parameter(Mandatory=$true)][string]$PinPath,
    [Parameter(Mandatory=$true)][string]$CertificatePath
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Security
$rsa = [System.Security.Cryptography.RSA]::Create(2048)
try {
    $subject = [System.Security.Cryptography.X509Certificates.X500DistinguishedName]::new(
        "CN=Profile Bridge S0 CI " + [Guid]::NewGuid().ToString()
    )
    $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
        $subject,
        $rsa,
        [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
    )
    $oids = [System.Security.Cryptography.OidCollection]::new()
    [void]$oids.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.3'))
    $eku = [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new(
        $oids,
        $false
    )
    $request.CertificateExtensions.Add($eku)
    $certificate = $request.CreateSelfSigned(
        [DateTimeOffset]::UtcNow.AddMinutes(-1),
        [DateTimeOffset]::UtcNow.AddHours(1)
    )
    try {
        [System.IO.File]::WriteAllBytes(
            $CertificatePath,
            $certificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
        )

        $manifest = [System.IO.File]::ReadAllBytes($ManifestPath)
        $contentInfo = [System.Security.Cryptography.Pkcs.ContentInfo]::new($manifest)
        $cms = [System.Security.Cryptography.Pkcs.SignedCms]::new($contentInfo, $true)
        $signer = [System.Security.Cryptography.Pkcs.CmsSigner]::new($certificate)
        $signer.IncludeOption = [System.Security.Cryptography.X509Certificates.X509IncludeOption]::EndCertOnly
        $signer.DigestAlgorithm = [System.Security.Cryptography.Oid]::new('2.16.840.1.101.3.4.2.1')
        $cms.ComputeSignature($signer)
        [System.IO.File]::WriteAllBytes($SignaturePath, $cms.Encode())

        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            $pin = [System.BitConverter]::ToString(
                $sha256.ComputeHash($certificate.RawData)
            ).Replace('-', '').ToLowerInvariant()
        }
        finally {
            $sha256.Dispose()
        }
        [System.IO.File]::WriteAllText($PinPath, $pin)
    }
    finally {
        $certificate.Dispose()
    }
}
finally {
    $rsa.Dispose()
}
"#;

const ACCEPTANCE_PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
const ACCEPTANCE_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Result<Self, io::Error> {
        let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "profile-bridge-cms-acceptance-{}-{sequence}",
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

struct SigningFixture {
    signature: Vec<u8>,
    certificate_sha256: String,
}

impl SigningFixture {
    fn create(directory: &Path, manifest: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let manifest_path = directory.join("fixture-manifest.bin");
        let signature_path = directory.join("fixture-signature.p7s");
        let pin_path = directory.join("fixture-pin.txt");
        let certificate_path = directory.join("fixture-certificate.cer");
        let create_script = directory.join("create-fixture.ps1");
        fs::write(&manifest_path, manifest)?;
        fs::write(&create_script, CREATE_FIXTURE_SCRIPT.as_bytes())?;

        run_powershell(
            &create_script,
            &[
                ("-ManifestPath", manifest_path.as_path()),
                ("-SignaturePath", signature_path.as_path()),
                ("-PinPath", pin_path.as_path()),
                ("-CertificatePath", certificate_path.as_path()),
            ],
        )?;

        let signature = fs::read(&signature_path)?;
        let certificate_sha256 = fs::read_to_string(&pin_path)?;
        if signature.is_empty()
            || certificate_sha256.len() != 64
            || !certificate_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            || !certificate_path.is_file()
        {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "invalid CMS test fixture").into(),
            );
        }
        Ok(Self {
            signature,
            certificate_sha256,
        })
    }
}

fn run_powershell(
    script: &Path,
    path_arguments: &[(&str, &Path)],
) -> Result<(), Box<dyn std::error::Error>> {
    let executable = powershell_executable()?;
    let mut command = Command::new(executable);
    command
        .env_clear()
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
        .arg(script);
    for (name, value) in path_arguments {
        command.arg(name).arg(value);
    }
    inherit_system_environment(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let status = wait_for_powershell_process(&mut child)?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("PowerShell CMS fixture creation failed").into())
    }
}

fn wait_for_powershell_process(child: &mut Child) -> Result<ExitStatus, io::Error> {
    let started_at = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None => {}
        }
        if started_at.elapsed() >= ACCEPTANCE_PROCESS_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "PowerShell CMS acceptance process timed out",
            ));
        }
        thread::sleep(ACCEPTANCE_PROCESS_POLL_INTERVAL);
    }
}

fn powershell_executable() -> Result<PathBuf, io::Error> {
    let system_root = env::var_os("SystemRoot")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "SystemRoot unavailable"))?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if executable.is_absolute() && executable.is_file() {
        Ok(executable)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "PowerShell unavailable",
        ))
    }
}

fn inherit_system_environment(command: &mut Command) {
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
}

#[test]
fn production_verifier_accepts_only_exact_pinned_application_signature() -> TestResult {
    let directory = TestDirectory::create()?;
    let manifest = br#"{"kind":"CMS_ACCEPTANCE","sequence":1}"#;
    let fixture = SigningFixture::create(&directory.0, manifest)?;
    let mut verifier = WindowsCmsSignatureVerifier::from_system(directory.0.clone())?;
    assert!(verifier.verify_cms(manifest, &fixture.signature, &fixture.certificate_sha256)?);
    assert!(!verifier.verify_cms(
        br#"{"kind":"CMS_ACCEPTANCE","sequence":2}"#,
        &fixture.signature,
        &fixture.certificate_sha256,
    )?);
    assert!(!verifier.verify_cms(manifest, &fixture.signature, &"b".repeat(64))?);
    let mut tampered = fixture.signature.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert!(!verifier.verify_cms(manifest, &tampered, &fixture.certificate_sha256)?);
    assert!(!verifier.verify_cms(manifest, b"unsigned", &fixture.certificate_sha256)?);
    Ok(())
}
