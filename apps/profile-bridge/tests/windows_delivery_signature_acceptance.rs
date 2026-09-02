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
const PRODUCTION_SCRIPT_PREFIX: &str = "const VERIFY_SCRIPT: &str = r#\"";
const PRODUCTION_SCRIPT_SUFFIX: &str = "\"#;";
const PARAMETER_ANCHOR: &str =
    "    [Parameter(Mandatory=$true)][string]$ExpectedCertificateSha256\n)";
const PARAMETER_REPLACEMENT: &str = "    [Parameter(Mandatory=$true)][string]$ExpectedCertificateSha256,\n    [Parameter(Mandatory=$true)][string]$CustomRootPath\n)";
const CHAIN_ANCHOR: &str =
    "    $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()\n    try {\n";
const CHAIN_REPLACEMENT: &str = "    $customRoot = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($CustomRootPath)\n    $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()\n    try {\n        [void]$chain.ChainPolicy.ExtraStore.Add($customRoot)\n";
const FLAGS_ANCHOR: &str = "        $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::NoFlag";
const FLAGS_REPLACEMENT: &str = "        $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::AllowUnknownCertificateAuthority";
const BUILD_ANCHOR: &str = "        if (-not $chain.Build($certificate)) { exit 23 }";
const BUILD_REPLACEMENT: &str = r#"        if (-not $chain.Build($certificate)) { exit 23 }
        if ($chain.ChainElements.Count -lt 1) { exit 23 }
        $chainRoot = $chain.ChainElements[$chain.ChainElements.Count - 1].Certificate
        if ([Convert]::ToBase64String($chainRoot.RawData) -cne [Convert]::ToBase64String($customRoot.RawData)) { exit 23 }"#;
const FINALLY_ANCHOR: &str = "    finally {\n        $chain.Dispose()\n    }";
const FINALLY_REPLACEMENT: &str =
    "    finally {\n        $chain.Dispose()\n        $customRoot.Dispose()\n    }";
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
    certificate_path: PathBuf,
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
            certificate_path,
        })
    }
}

fn production_verify_script() -> Result<String, io::Error> {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("windows_delivery_signature.rs");
    let source = fs::read_to_string(source_path)?.replace("\r\n", "\n");
    let start = source
        .find(PRODUCTION_SCRIPT_PREFIX)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "VERIFY_SCRIPT owner missing"))?;
    let body_start = start + PRODUCTION_SCRIPT_PREFIX.len();
    let tail = &source[body_start..];
    let body_end = tail.find(PRODUCTION_SCRIPT_SUFFIX).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "VERIFY_SCRIPT terminator missing",
        )
    })?;
    if tail[body_end + PRODUCTION_SCRIPT_SUFFIX.len()..].contains(PRODUCTION_SCRIPT_PREFIX) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "VERIFY_SCRIPT owner is ambiguous",
        ));
    }
    Ok(tail[..body_end].to_owned())
}

fn isolated_trust_verify_script() -> Result<String, io::Error> {
    let script = production_verify_script()?;
    if !script.contains("X509RevocationMode]::NoCheck")
        || script.contains("X509RevocationMode]::Online")
        || script.contains("UrlRetrievalTimeout")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "production verifier must keep app-owned revocation offline and deterministic",
        ));
    }
    let script = replace_exactly_once(&script, PARAMETER_ANCHOR, PARAMETER_REPLACEMENT)?;
    let script = replace_exactly_once(&script, CHAIN_ANCHOR, CHAIN_REPLACEMENT)?;
    let script = replace_exactly_once(&script, FLAGS_ANCHOR, FLAGS_REPLACEMENT)?;
    let script = replace_exactly_once(&script, BUILD_ANCHOR, BUILD_REPLACEMENT)?;
    replace_exactly_once(&script, FINALLY_ANCHOR, FINALLY_REPLACEMENT)
}

fn replace_exactly_once(input: &str, anchor: &str, replacement: &str) -> Result<String, io::Error> {
    let start = input.find(anchor).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "production verifier anchor missing",
        )
    })?;
    if input[start + anchor.len()..].contains(anchor) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "production verifier anchor is ambiguous",
        ));
    }
    let mut output = String::with_capacity(input.len() - anchor.len() + replacement.len());
    output.push_str(&input[..start]);
    output.push_str(replacement);
    output.push_str(&input[start + anchor.len()..]);
    Ok(output)
}

fn run_isolated_trust_verifier(
    directory: &Path,
    label: &str,
    manifest: &[u8],
    signature: &[u8],
    expected_certificate_sha256: &str,
    custom_root_path: &Path,
) -> Result<i32, Box<dyn std::error::Error>> {
    let script_path = directory.join(format!("verify-{label}.ps1"));
    let manifest_path = directory.join(format!("manifest-{label}.bin"));
    let signature_path = directory.join(format!("signature-{label}.p7s"));
    fs::write(&script_path, isolated_trust_verify_script()?.as_bytes())?;
    fs::write(&manifest_path, manifest)?;
    fs::write(&signature_path, signature)?;

    let executable = powershell_executable()?;
    let mut command = Command::new(executable);
    command
        .env_clear()
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
        .arg(&script_path)
        .arg("-ManifestPath")
        .arg(&manifest_path)
        .arg("-SignaturePath")
        .arg(&signature_path)
        .arg("-ExpectedCertificateSha256")
        .arg(expected_certificate_sha256)
        .arg("-CustomRootPath")
        .arg(custom_root_path);
    inherit_system_environment(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let status = wait_for_powershell_process(&mut child)?;
    let code = status
        .code()
        .ok_or_else(|| io::Error::other("isolated CMS verifier exited without a code"))?;
    Ok(code)
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
fn production_script_accepts_isolated_test_trust_without_system_trust_fallback() -> TestResult {
    let directory = TestDirectory::create()?;
    let manifest = br#"{"kind":"S0_CMS_ACCEPTANCE","sequence":1}"#;
    let fixture = SigningFixture::create(&directory.0, manifest)?;

    let exact_code = run_isolated_trust_verifier(
        &directory.0,
        "exact",
        manifest,
        &fixture.signature,
        &fixture.certificate_sha256,
        &fixture.certificate_path,
    )?;
    let tampered_code = run_isolated_trust_verifier(
        &directory.0,
        "tampered",
        br#"{"kind":"S0_CMS_ACCEPTANCE","sequence":2}"#,
        &fixture.signature,
        &fixture.certificate_sha256,
        &fixture.certificate_path,
    )?;
    let wrong_pin_code = run_isolated_trust_verifier(
        &directory.0,
        "wrong-pin",
        manifest,
        &fixture.signature,
        &"b".repeat(64),
        &fixture.certificate_path,
    )?;

    let mut system_verifier = WindowsCmsSignatureVerifier::from_system(directory.0.clone())?;
    let system_trust_result =
        system_verifier.verify_cms(manifest, &fixture.signature, &fixture.certificate_sha256)?;

    assert_eq!(exact_code, 0);
    assert_eq!(tampered_code, 24);
    assert_eq!(wrong_pin_code, 22);
    assert!(!system_trust_result);
    Ok(())
}
