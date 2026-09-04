#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::{idempotency_header, validate_api_opaque_id};
use bridge_host_ops::{
    BindingHttpPlan, CertificateObservation, HostOpsError, HostOpsResult, ImportObservation,
    MutationReceipt, build_access_config, normalize_sha1_thumbprint, parse_certificate_observation,
    parse_control_plane_output, parse_import_observation, validate_access_token,
};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const INSPECT_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$thumb = $env:BRIDGE_HOST_OPS_CERT_SHA1
$path = "Cert:\LocalMachine\My\$thumb"
$cert = Get-Item -Path $path -ErrorAction Stop
$clientAuth = $false
$ekuExtension = $cert.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' } | Select-Object -First 1
if ($null -ne $ekuExtension) {
    $enhanced = New-Object System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension($ekuExtension, $ekuExtension.Critical)
    foreach ($usage in $enhanced.EnhancedKeyUsages) {
        if ($usage.Value -eq '1.3.6.1.5.5.7.3.2') { $clientAuth = $true }
    }
}
$sha256 = [System.Security.Cryptography.SHA256]::Create()
try {
    $hash = $sha256.ComputeHash($cert.RawData)
} finally {
    $sha256.Dispose()
}
$fingerprint = ([BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
$now = [DateTime]::UtcNow
$current = ($cert.NotBefore.ToUniversalTime() -le $now) -and ($cert.NotAfter.ToUniversalTime() -gt $now)
$privateFlag = $(if ($cert.HasPrivateKey) { '1' } else { '0' })
$clientFlag = $(if ($clientAuth) { '1' } else { '0' })
$currentFlag = $(if ($current) { '1' } else { '0' })
Write-Output ([string]::Join("`t", @($cert.Thumbprint.ToUpperInvariant(), $fingerprint, $privateFlag, $clientFlag, $currentFlag)))
"#;

const IMPORT_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$before = @{}
Get-ChildItem -Path 'Cert:\LocalMachine\My' | ForEach-Object { $before[$_.Thumbprint.ToUpperInvariant()] = $true }
$passwordText = [Console]::In.ReadToEnd().TrimEnd("`r", "`n")
if ([string]::IsNullOrEmpty($passwordText)) { throw 'empty PFX password is forbidden' }
$securePassword = ConvertTo-SecureString -String $passwordText -AsPlainText -Force
try {
    $imported = @(Import-PfxCertificate -FilePath $env:BRIDGE_HOST_OPS_PFX_PATH -CertStoreLocation 'Cert:\LocalMachine\My' -Password $securePassword -ErrorAction Stop)
} finally {
    $passwordText = $null
    $securePassword = $null
}
$private = @($imported | Where-Object { $_.HasPrivateKey })
if ($private.Count -ne 1) {
    foreach ($item in $imported) {
        $candidate = $item.Thumbprint.ToUpperInvariant()
        if (-not $before.ContainsKey($candidate) -and (Test-Path "Cert:\LocalMachine\My\$candidate")) {
            Remove-Item -Path "Cert:\LocalMachine\My\$candidate" -DeleteKey -Confirm:$false
        }
    }
    throw 'PFX must import exactly one certificate with a private key'
}
$leaf = $private[0]
$leafThumb = $leaf.Thumbprint.ToUpperInvariant()
foreach ($item in $imported) {
    $candidate = $item.Thumbprint.ToUpperInvariant()
    if ($candidate -ne $leafThumb -and -not $before.ContainsKey($candidate) -and (Test-Path "Cert:\LocalMachine\My\$candidate")) {
        Remove-Item -Path "Cert:\LocalMachine\My\$candidate" -DeleteKey -Confirm:$false
    }
}
$preexisting = $(if ($before.ContainsKey($leafThumb)) { '1' } else { '0' })
Write-Output ([string]::Join("`t", @($leafThumb, $preexisting)))
"#;

const REMOVE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$thumb = $env:BRIDGE_HOST_OPS_CERT_SHA1
$path = "Cert:\LocalMachine\My\$thumb"
if (Test-Path $path) {
    Remove-Item -Path $path -DeleteKey -Confirm:$false
    Write-Output 'removed'
} else {
    Write-Output 'absent'
}
"#;

pub fn inspect_certificate(thumbprint: &str) -> HostOpsResult<CertificateObservation> {
    let thumbprint = normalize_sha1_thumbprint(thumbprint)?;
    let mut command = powershell_command(INSPECT_SCRIPT)?;
    command.env("BRIDGE_HOST_OPS_CERT_SHA1", &thumbprint);
    let output = run_command(command, None, 8_192)?;
    parse_certificate_observation(&output)
}

pub fn import_certificate(
    pfx_path: &Path,
    password_file: &Path,
) -> HostOpsResult<CertificateObservation> {
    let canonical_pfx = fs::canonicalize(pfx_path)
        .map_err(|_| HostOpsError::new("pfx_input_unavailable"))?;
    if !canonical_pfx.is_file() {
        return Err(HostOpsError::new("pfx_input_unavailable"));
    }
    let mut password = read_secret_file(password_file, 4_096)?;
    if password.is_empty() || password.contains(&0) || std::str::from_utf8(&password).is_err() {
        password.fill(0);
        return Err(HostOpsError::new("invalid_pfx_password_input"));
    }

    let mut command = powershell_command(IMPORT_SCRIPT)?;
    command.env("BRIDGE_HOST_OPS_PFX_PATH", canonical_pfx.as_os_str());
    let output = run_command(command, Some(password), 8_192)?;
    let imported: ImportObservation = parse_import_observation(&output)?;
    match inspect_certificate(imported.sha1_thumbprint()) {
        Ok(certificate) => Ok(certificate),
        Err(error) => {
            if !imported.preexisting() && remove_certificate(imported.sha1_thumbprint()).is_err() {
                return Err(HostOpsError::new("import_validation_cleanup_failed"));
            }
            Err(error)
        }
    }
}

pub fn remove_certificate(thumbprint: &str) -> HostOpsResult<()> {
    let thumbprint = normalize_sha1_thumbprint(thumbprint)?;
    let mut command = powershell_command(REMOVE_SCRIPT)?;
    command.env("BRIDGE_HOST_OPS_CERT_SHA1", &thumbprint);
    let output = run_command(command, None, 1_024)?;
    match output.trim() {
        "removed" | "absent" => Ok(()),
        _ => Err(HostOpsError::new("invalid_certificate_removal_observation")),
    }
}

pub fn execute_binding(
    plan: &BindingHttpPlan,
    access_token_file: &Path,
    idempotency_key: &str,
) -> HostOpsResult<MutationReceipt> {
    validate_api_opaque_id(idempotency_key)?;
    let idempotency = idempotency_header(idempotency_key)?;
    let mut token = read_secret_file(access_token_file, 32_768)?;
    while matches!(token.last(), Some(b'\r' | b'\n')) {
        token.pop();
    }
    let token_text = match std::str::from_utf8(&token) {
        Ok(value) => value,
        Err(_) => {
            token.fill(0);
            return Err(HostOpsError::new("invalid_access_token"));
        }
    };
    if let Err(error) = validate_access_token(token_text) {
        token.fill(0);
        return Err(error);
    }
    let mut access_config = match build_access_config(token_text) {
        Ok(value) => value,
        Err(error) => {
            token.fill(0);
            return Err(error);
        }
    };
    token.fill(0);

    let mut command = curl_command()?;
    command
        .args(plan.curl_arguments())
        .arg("--header")
        .arg(idempotency)
        .arg("--max-filesize")
        .arg("65536");
    let output = run_command(command, Some(std::mem::take(&mut access_config)), 65_536)?;
    access_config.fill(0);
    let receipt = parse_control_plane_output(&output)?;
    if receipt.resource_id() != plan.target_actor_id() {
        return Err(HostOpsError::new("control_plane_resource_mismatch"));
    }
    Ok(receipt)
}

fn powershell_command(script: &str) -> HostOpsResult<Command> {
    let root = system_root()?;
    let root_path = PathBuf::from(&root);
    let program = root_path.join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if !program.is_file() {
        return Err(HostOpsError::new("windows_powershell_unavailable"));
    }
    let modules = root_path.join(r"System32\WindowsPowerShell\v1.0\Modules");
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("SystemRoot", &root)
        .env("WINDIR", &root)
        .env("PSModulePath", modules)
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script);
    Ok(command)
}

fn curl_command() -> HostOpsResult<Command> {
    let root = system_root()?;
    let program = PathBuf::from(&root).join(r"System32\curl.exe");
    if !program.is_file() {
        return Err(HostOpsError::new("windows_curl_unavailable"));
    }
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("SystemRoot", &root)
        .env("WINDIR", &root);
    Ok(command)
}

fn system_root() -> HostOpsResult<OsString> {
    env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HostOpsError::new("windows_system_root_unavailable"))
}

fn read_secret_file(path: &Path, maximum_size: usize) -> HostOpsResult<Vec<u8>> {
    let mut bytes = fs::read(path).map_err(|_| HostOpsError::new("secret_input_unavailable"))?;
    if bytes.len() > maximum_size {
        bytes.fill(0);
        return Err(HostOpsError::new("secret_input_too_large"));
    }
    Ok(bytes)
}

fn run_command(
    mut command: Command,
    mut secret_input: Option<Vec<u8>>,
    maximum_output_size: usize,
) -> HostOpsResult<String> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    if secret_input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = match command.spawn() {
        Ok(value) => value,
        Err(_) => {
            if let Some(secret) = secret_input.as_mut() {
                secret.fill(0);
            }
            return Err(HostOpsError::new("host_effect_spawn_failed"));
        }
    };
    let mut write_failed = false;
    if let Some(secret) = secret_input.as_mut() {
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(secret).is_err() {
                write_failed = true;
            }
        } else {
            write_failed = true;
        }
        secret.fill(0);
    }
    let output = child
        .wait_with_output()
        .map_err(|_| HostOpsError::new("host_effect_wait_failed"))?;
    if write_failed {
        return Err(HostOpsError::new("host_effect_stdin_failed"));
    }
    if !output.status.success() {
        return Err(HostOpsError::new("host_effect_failed"));
    }
    if output.stdout.len() > maximum_output_size {
        return Err(HostOpsError::new("host_effect_output_too_large"));
    }
    String::from_utf8(output.stdout).map_err(|_| HostOpsError::new("host_effect_output_invalid"))
}

#[allow(dead_code)]
fn _assert_os_str_is_not_logged(_: &OsStr) {}
