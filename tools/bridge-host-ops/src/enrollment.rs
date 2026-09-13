use super::{ensure_only, idempotency_header, required, validate_api_opaque_id};
use bridge_host_ops::{
    HostOpsError, HostOpsResult, build_access_config, json_string, normalize_https_origin,
};
use std::collections::BTreeMap;

#[cfg(any(windows, test))]
use bridge_host_ops::{
    CERTIFICATE_STORE, CertificateObservation, SCHEMA_VERSION, SHIPPING_CERT_SHA1_ENV,
    SHIPPING_DEVICE_ID_ENV, SHIPPING_ORIGIN_ENV, validate_identifier, validate_sha256_fingerprint,
};
#[cfg(windows)]
use bridge_host_ops::{parse_certificate_observation, validate_access_token};
#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::io::Write;
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};

const ISSUE_PATH_SUFFIX: &str = "/bridge-enrollment/authorities";
const REDEEM_PATH_SUFFIX: &str = "/bridge-enrollment/redemptions";
const MAX_CSR_DER_HEX_LENGTH: usize = 32 * 1024;
#[cfg(any(windows, test))]
const MAX_CERTIFICATE_DER_HEX_LENGTH: usize = 64 * 1024;
#[cfg(any(windows, test))]
const MAX_CERTIFICATE_CHAIN_COUNT: usize = 8;
const MAX_HTTP_OUTPUT_SIZE: usize = 640 * 1024;

#[cfg(any(windows, test))]
#[derive(Clone, Eq, PartialEq)]
struct EnrollmentIssueProjection {
    claim_code: String,
    device_id: String,
}

#[cfg(any(windows, test))]
#[derive(Clone, Eq, PartialEq)]
struct EnrollmentRedemptionProjection {
    device_id: String,
    certificate_sha256: String,
    leaf_certificate_der_hex: String,
}

#[derive(Clone, Eq, PartialEq)]
struct EnrollmentHttpPlan {
    url: String,
    body: String,
    correlation_id: String,
    idempotency_header: Option<String>,
    success_statuses: &'static [u16],
}

impl EnrollmentHttpPlan {
    fn curl_arguments(&self) -> Vec<String> {
        let mut arguments = vec![
            "--silent".to_owned(),
            "--show-error".to_owned(),
            "--no-progress-meter".to_owned(),
            "--connect-timeout".to_owned(),
            "10".to_owned(),
            "--max-time".to_owned(),
            "30".to_owned(),
            "--proto".to_owned(),
            "=https".to_owned(),
            "--noproxy".to_owned(),
            "*".to_owned(),
            "--request".to_owned(),
            "POST".to_owned(),
            "--header".to_owned(),
            "Accept: application/json".to_owned(),
            "--header".to_owned(),
            "Content-Type: application/json".to_owned(),
            "--header".to_owned(),
            format!("X-Correlation-Id: {}", self.correlation_id),
        ];
        if let Some(header) = &self.idempotency_header {
            arguments.push("--header".to_owned());
            arguments.push(header.clone());
        }
        arguments.extend([
            "--max-filesize".to_owned(),
            MAX_HTTP_OUTPUT_SIZE.to_string(),
            "--write-out".to_owned(),
            "\n%{http_code}".to_owned(),
            "--config".to_owned(),
            "-".to_owned(),
            "--url".to_owned(),
            self.url.clone(),
        ]);
        arguments
    }

    fn stdin_config(&self, access_token: &str) -> HostOpsResult<Vec<u8>> {
        let mut config = build_access_config(access_token)?;
        append_curl_config_value(&mut config, "data-binary", &self.body)?;
        Ok(config)
    }
}

pub(crate) fn self_test() -> HostOpsResult<()> {
    let token = "eyJhbGciOiJSUzI1NiJ9.eyJhdWQiOiJlbnJvbGwtc2VsZi10ZXN0In0.c2lnbmF0dXJl";
    let issue = issue_plan(
        "https://control.example.test",
        "tenant_01",
        "corr_enroll_self_test",
        "idem_enroll_self_test",
    )?;
    if issue
        .curl_arguments()
        .iter()
        .any(|value| value.contains(token))
    {
        return Err(HostOpsError::new(
            "enrollment_self_test_secret_argv_failure",
        ));
    }
    let claim = "ab".repeat(32);
    let csr = "3000";
    let redeem = redeem_plan(
        "https://control.example.test",
        "tenant_01",
        "corr_enroll_self_test",
        &claim,
        csr,
    )?;
    let arguments = redeem.curl_arguments();
    if arguments
        .iter()
        .any(|value| value.contains(token) || value.contains(&claim) || value.contains(csr))
    {
        return Err(HostOpsError::new(
            "enrollment_self_test_secret_argv_failure",
        ));
    }
    let mut config = redeem.stdin_config(token)?;
    let config_text = std::str::from_utf8(&config)
        .map_err(|_| HostOpsError::new("enrollment_self_test_failed"))?;
    if !config_text.contains(token) || !config_text.contains(&claim) || !config_text.contains(csr) {
        config.fill(0);
        return Err(HostOpsError::new("enrollment_self_test_failed"));
    }
    config.fill(0);
    Ok(())
}

pub(crate) fn run(flags: &BTreeMap<String, String>) -> HostOpsResult<String> {
    ensure_only(
        flags,
        &[
            "--origin",
            "--tenant-id",
            "--access-token-file",
            "--correlation-id",
            "--idempotency-key",
        ],
    )?;
    let origin = normalize_https_origin(required(flags, "--origin")?)?;
    let tenant_id = required(flags, "--tenant-id")?;
    let correlation_id = required(flags, "--correlation-id")?;
    let idempotency_key = required(flags, "--idempotency-key")?;
    validate_api_opaque_id(tenant_id)?;
    validate_api_opaque_id(correlation_id)?;
    validate_api_opaque_id(idempotency_key)?;
    let access_token_file = required(flags, "--access-token-file")?;
    if access_token_file.is_empty() {
        return Err(HostOpsError::new("secret_input_unavailable"));
    }

    #[cfg(not(windows))]
    {
        let _ = (
            origin,
            tenant_id,
            correlation_id,
            idempotency_key,
            access_token_file,
        );
        Err(HostOpsError::new("windows_required"))
    }

    #[cfg(windows)]
    {
        run_windows(
            &origin,
            tenant_id,
            Path::new(access_token_file),
            correlation_id,
            idempotency_key,
        )
    }
}

#[cfg(windows)]
fn run_windows(
    origin: &str,
    tenant_id: &str,
    access_token_file: &Path,
    correlation_id: &str,
    idempotency_key: &str,
) -> HostOpsResult<String> {
    let issue = execute_issue(
        &issue_plan(origin, tenant_id, correlation_id, idempotency_key)?,
        access_token_file,
    )?;
    let csr_der_hex = create_or_reuse_machine_csr(&issue.device_id)?;
    let redeem = match execute_redeem(
        &redeem_plan(
            origin,
            tenant_id,
            correlation_id,
            &issue.claim_code,
            &csr_der_hex,
        )?,
        access_token_file,
    ) {
        Ok(value) => value,
        Err(error) => {
            cleanup_machine_key(&issue.device_id)?;
            return Err(error);
        }
    };
    if redeem.device_id != issue.device_id {
        cleanup_machine_key(&issue.device_id)?;
        return Err(HostOpsError::new("enrollment_device_identity_mismatch"));
    }
    let certificate = match install_enrollment_certificate(
        &issue.device_id,
        &redeem.certificate_sha256,
        &redeem.leaf_certificate_der_hex,
    ) {
        Ok(value) => value,
        Err(error) => {
            cleanup_machine_key(&issue.device_id)?;
            return Err(error);
        }
    };
    if certificate.sha256_fingerprint() != redeem.certificate_sha256 {
        if super::windows::remove_certificate(certificate.sha1_thumbprint()).is_err() {
            return Err(HostOpsError::new("enrollment_certificate_cleanup_failed"));
        }
        return Err(HostOpsError::new(
            "enrollment_certificate_identity_mismatch",
        ));
    }
    Ok(render_enrollment_receipt(
        origin,
        &issue.device_id,
        &certificate,
    ))
}

fn issue_plan(
    origin: &str,
    tenant_id: &str,
    correlation_id: &str,
    idempotency_key: &str,
) -> HostOpsResult<EnrollmentHttpPlan> {
    let origin = normalize_https_origin(origin)?;
    validate_api_opaque_id(tenant_id)?;
    validate_api_opaque_id(correlation_id)?;
    Ok(EnrollmentHttpPlan {
        url: format!("{origin}/api/v1/tenants/{tenant_id}{ISSUE_PATH_SUFFIX}"),
        body: "{}".to_owned(),
        correlation_id: correlation_id.to_owned(),
        idempotency_header: Some(idempotency_header(idempotency_key)?),
        success_statuses: &[200, 201],
    })
}

fn redeem_plan(
    origin: &str,
    tenant_id: &str,
    correlation_id: &str,
    claim_code: &str,
    csr_der_hex: &str,
) -> HostOpsResult<EnrollmentHttpPlan> {
    let origin = normalize_https_origin(origin)?;
    validate_api_opaque_id(tenant_id)?;
    validate_api_opaque_id(correlation_id)?;
    validate_lower_hex_exact(claim_code, 64, "invalid_enrollment_claim")?;
    validate_der_hex(
        csr_der_hex,
        MAX_CSR_DER_HEX_LENGTH,
        "invalid_enrollment_csr",
    )?;
    Ok(EnrollmentHttpPlan {
        url: format!("{origin}/api/v1/tenants/{tenant_id}{REDEEM_PATH_SUFFIX}"),
        body: format!(
            "{{\"claimCode\":{},\"csrDerHex\":{}}}",
            json_string(claim_code),
            json_string(csr_der_hex)
        ),
        correlation_id: correlation_id.to_owned(),
        idempotency_header: None,
        success_statuses: &[200],
    })
}

fn append_curl_config_value(config: &mut Vec<u8>, name: &str, value: &str) -> HostOpsResult<()> {
    if value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0)) {
        return Err(HostOpsError::new("invalid_enrollment_http_body"));
    }
    config.extend_from_slice(name.as_bytes());
    config.extend_from_slice(b" = \"");
    for byte in value.bytes() {
        match byte {
            b'\\' => config.extend_from_slice(b"\\\\"),
            b'\"' => config.extend_from_slice(b"\\\""),
            _ => config.push(byte),
        }
    }
    config.extend_from_slice(b"\"\n");
    Ok(())
}

#[cfg(any(windows, test))]
fn parse_issue_output(value: &str, statuses: &[u16]) -> HostOpsResult<EnrollmentIssueProjection> {
    let body = success_body(value, statuses)?;
    let mut cursor = JsonCursor::new(body);
    cursor.expect(b'{')?;
    let mut claim_code = None;
    let mut expires_at_ms = None;
    let mut device_id = None;
    loop {
        cursor.skip_ws();
        if cursor.consume(b'}') {
            break;
        }
        let key = cursor.string()?;
        cursor.expect(b':')?;
        match key.as_str() {
            "claimCode" if claim_code.is_none() => claim_code = Some(cursor.string()?),
            "expiresAtMs" if expires_at_ms.is_none() => expires_at_ms = Some(cursor.u64()?),
            "deviceId" if device_id.is_none() => device_id = Some(cursor.string()?),
            _ => return Err(HostOpsError::new("invalid_enrollment_issue_response")),
        }
        cursor.skip_ws();
        if cursor.consume(b',') {
            continue;
        }
        cursor.expect(b'}')?;
        break;
    }
    cursor.finish()?;
    let claim_code =
        claim_code.ok_or_else(|| HostOpsError::new("invalid_enrollment_issue_response"))?;
    validate_lower_hex_exact(&claim_code, 64, "invalid_enrollment_issue_response")?;
    if expires_at_ms.ok_or_else(|| HostOpsError::new("invalid_enrollment_issue_response"))? == 0 {
        return Err(HostOpsError::new("invalid_enrollment_issue_response"));
    }
    let device_id =
        device_id.ok_or_else(|| HostOpsError::new("invalid_enrollment_issue_response"))?;
    validate_identifier(&device_id)
        .map_err(|_| HostOpsError::new("invalid_enrollment_issue_response"))?;
    Ok(EnrollmentIssueProjection {
        claim_code,
        device_id,
    })
}

#[cfg(any(windows, test))]
fn parse_redeem_output(
    value: &str,
    statuses: &[u16],
) -> HostOpsResult<EnrollmentRedemptionProjection> {
    let body = success_body(value, statuses)?;
    let mut cursor = JsonCursor::new(body);
    cursor.expect(b'{')?;
    let mut device_id = None;
    let mut certificate_sha256 = None;
    let mut leaf_certificate_der_hex = None;
    let mut chain_seen = false;
    loop {
        cursor.skip_ws();
        if cursor.consume(b'}') {
            break;
        }
        let key = cursor.string()?;
        cursor.expect(b':')?;
        match key.as_str() {
            "deviceId" if device_id.is_none() => device_id = Some(cursor.string()?),
            "certificateSha256" if certificate_sha256.is_none() => {
                certificate_sha256 = Some(cursor.string()?)
            }
            "leafCertificateDerHex" if leaf_certificate_der_hex.is_none() => {
                leaf_certificate_der_hex = Some(cursor.string()?)
            }
            "certificateChainDerHex" if !chain_seen => {
                parse_certificate_chain(&mut cursor)?;
                chain_seen = true;
            }
            _ => return Err(HostOpsError::new("invalid_enrollment_redeem_response")),
        }
        cursor.skip_ws();
        if cursor.consume(b',') {
            continue;
        }
        cursor.expect(b'}')?;
        break;
    }
    cursor.finish()?;
    let device_id =
        device_id.ok_or_else(|| HostOpsError::new("invalid_enrollment_redeem_response"))?;
    validate_identifier(&device_id)
        .map_err(|_| HostOpsError::new("invalid_enrollment_redeem_response"))?;
    let certificate_sha256 = certificate_sha256
        .ok_or_else(|| HostOpsError::new("invalid_enrollment_redeem_response"))?;
    validate_sha256_fingerprint(&certificate_sha256)
        .map_err(|_| HostOpsError::new("invalid_enrollment_redeem_response"))?;
    let leaf_certificate_der_hex = leaf_certificate_der_hex
        .ok_or_else(|| HostOpsError::new("invalid_enrollment_redeem_response"))?;
    validate_der_hex(
        &leaf_certificate_der_hex,
        MAX_CERTIFICATE_DER_HEX_LENGTH,
        "invalid_enrollment_redeem_response",
    )?;
    if !chain_seen {
        return Err(HostOpsError::new("invalid_enrollment_redeem_response"));
    }
    Ok(EnrollmentRedemptionProjection {
        device_id,
        certificate_sha256,
        leaf_certificate_der_hex,
    })
}

#[cfg(any(windows, test))]
fn parse_certificate_chain(cursor: &mut JsonCursor<'_>) -> HostOpsResult<()> {
    cursor.expect(b'[')?;
    let mut count = 0usize;
    loop {
        cursor.skip_ws();
        if cursor.consume(b']') {
            return Ok(());
        }
        if count >= MAX_CERTIFICATE_CHAIN_COUNT {
            return Err(HostOpsError::new("invalid_enrollment_redeem_response"));
        }
        let certificate = cursor.string()?;
        validate_der_hex(
            &certificate,
            MAX_CERTIFICATE_DER_HEX_LENGTH,
            "invalid_enrollment_redeem_response",
        )?;
        count += 1;
        cursor.skip_ws();
        if cursor.consume(b',') {
            continue;
        }
        cursor.expect(b']')?;
        return Ok(());
    }
}

#[cfg(any(windows, test))]
fn success_body<'a>(value: &'a str, statuses: &[u16]) -> HostOpsResult<&'a str> {
    if value.len() > MAX_HTTP_OUTPUT_SIZE {
        return Err(HostOpsError::new("enrollment_http_output_too_large"));
    }
    let Some((body, raw_status)) = value.rsplit_once('\n') else {
        return Err(HostOpsError::new("invalid_enrollment_http_response"));
    };
    let status = raw_status
        .trim_end_matches('\r')
        .parse::<u16>()
        .map_err(|_| HostOpsError::new("invalid_enrollment_http_response"))?;
    if !statuses.contains(&status) {
        return Err(match status {
            400 => HostOpsError::new("enrollment_invalid_request"),
            404 => HostOpsError::new("enrollment_not_found"),
            409 => HostOpsError::new("enrollment_replay_or_conflict"),
            500 => HostOpsError::new("enrollment_integrity_failure"),
            503 => HostOpsError::new("enrollment_dependency_unavailable"),
            _ => HostOpsError::new("enrollment_control_plane_rejected"),
        });
    }
    Ok(body.trim())
}

fn validate_lower_hex_exact(value: &str, length: usize, code: &'static str) -> HostOpsResult<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(HostOpsError::new(code));
    }
    Ok(())
}

fn validate_der_hex(value: &str, maximum_length: usize, code: &'static str) -> HostOpsResult<()> {
    if value.len() < 4
        || value.len() > maximum_length
        || !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(HostOpsError::new(code));
    }
    Ok(())
}

#[cfg(any(windows, test))]
struct JsonCursor<'a> {
    input: &'a [u8],
    position: usize,
}

#[cfg(any(windows, test))]
impl<'a> JsonCursor<'a> {
    fn new(value: &'a str) -> Self {
        Self {
            input: value.as_bytes(),
            position: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        self.skip_ws();
        if self.input.get(self.position) == Some(&expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: u8) -> HostOpsResult<()> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(HostOpsError::new("invalid_enrollment_json"))
        }
    }

    fn string(&mut self) -> HostOpsResult<String> {
        self.skip_ws();
        if self.input.get(self.position) != Some(&b'\"') {
            return Err(HostOpsError::new("invalid_enrollment_json"));
        }
        self.position += 1;
        let start = self.position;
        while let Some(byte) = self.input.get(self.position).copied() {
            if byte == b'\"' {
                let slice = &self.input[start..self.position];
                if slice.iter().any(|byte| *byte == b'\\' || *byte < 0x20) {
                    return Err(HostOpsError::new("invalid_enrollment_json"));
                }
                self.position += 1;
                return String::from_utf8(slice.to_vec())
                    .map_err(|_| HostOpsError::new("invalid_enrollment_json"));
            }
            self.position += 1;
        }
        Err(HostOpsError::new("invalid_enrollment_json"))
    }

    fn u64(&mut self) -> HostOpsResult<u64> {
        self.skip_ws();
        let start = self.position;
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        if start == self.position {
            return Err(HostOpsError::new("invalid_enrollment_json"));
        }
        std::str::from_utf8(&self.input[start..self.position])
            .map_err(|_| HostOpsError::new("invalid_enrollment_json"))?
            .parse::<u64>()
            .map_err(|_| HostOpsError::new("invalid_enrollment_json"))
    }

    fn finish(&mut self) -> HostOpsResult<()> {
        self.skip_ws();
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(HostOpsError::new("invalid_enrollment_json"))
        }
    }
}

#[cfg(any(windows, test))]
fn render_enrollment_receipt(
    origin: &str,
    device_id: &str,
    certificate: &CertificateObservation,
) -> String {
    format!(
        "{{\"schemaVersion\":{},\"operation\":\"enroll\",\"deviceId\":{},\"controlPlaneOrigin\":{},\"certificateStore\":{},\"certificateSha1\":{},\"certificateSha256\":{},\"certificateSelector\":{},\"shippingEnvironment\":{{{}:{},{}:{},{}:{}}}}}",
        json_string(SCHEMA_VERSION),
        json_string(device_id),
        json_string(origin),
        json_string(CERTIFICATE_STORE),
        json_string(certificate.sha1_thumbprint()),
        json_string(certificate.sha256_fingerprint()),
        json_string(&certificate.selector()),
        json_string(SHIPPING_DEVICE_ID_ENV),
        json_string(device_id),
        json_string(SHIPPING_CERT_SHA1_ENV),
        json_string(certificate.sha1_thumbprint()),
        json_string(SHIPPING_ORIGIN_ENV),
        json_string(origin),
    )
}

#[cfg(windows)]
const CREATE_CSR_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function Test-KeyInUse([string]$keyName) {
    foreach ($candidateCertificate in Get-ChildItem -Path 'Cert:\LocalMachine\My') {
        $candidateRsa = $null
        try {
            if (-not $candidateCertificate.HasPrivateKey) { continue }
            $candidateRsa = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($candidateCertificate)
            if ($null -ne $candidateRsa -and $candidateRsa.GetType().FullName -eq 'System.Security.Cryptography.RSACng' -and $candidateRsa.Key.KeyName -eq $keyName) {
                return $true
            }
        } catch {
        } finally {
            if ($null -ne $candidateRsa) { $candidateRsa.Dispose() }
        }
    }
    return $false
}
$keyName = 'part-crm-bridge-' + $env:BRIDGE_HOST_OPS_DEVICE_ID
$provider = [System.Security.Cryptography.CngProvider]::MicrosoftSoftwareKeyStorageProvider
$keyExists = [System.Security.Cryptography.CngKey]::Exists($keyName, $provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)
if ($keyExists -and (Test-KeyInUse $keyName)) {
    Write-Output 'in_use'
    exit 0
}
$key = $null
$rsa = $null
$created = $false
try {
    if ($keyExists) {
        $key = [System.Security.Cryptography.CngKey]::Open($keyName, $provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)
    } else {
        $parameters = New-Object System.Security.Cryptography.CngKeyCreationParameters
        $parameters.Provider = $provider
        $parameters.KeyCreationOptions = [System.Security.Cryptography.CngKeyCreationOptions]::MachineKey
        $parameters.ExportPolicy = [System.Security.Cryptography.CngExportPolicies]::None
        $parameters.KeyUsage = [System.Security.Cryptography.CngKeyUsages]::Signing
        $key = [System.Security.Cryptography.CngKey]::Create([System.Security.Cryptography.CngAlgorithm]::Rsa, $keyName, $parameters)
        $created = $true
    }
    if ($key.ExportPolicy -ne [System.Security.Cryptography.CngExportPolicies]::None) { throw 'device key is exportable' }
    if (($key.KeyUsage -band [System.Security.Cryptography.CngKeyUsages]::Signing) -eq 0) { throw 'device key cannot sign' }
    $rsa = New-Object System.Security.Cryptography.RSACng($key)
    if ($rsa.KeySize -lt 2048) { throw 'device key is too small' }
    $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
        'CN=part-crm-bridge-device',
        $rsa,
        [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
    )
    $oids = New-Object System.Security.Cryptography.OidCollection
    $null = $oids.Add((New-Object System.Security.Cryptography.Oid('1.3.6.1.5.5.7.3.2')))
    $request.CertificateExtensions.Add((New-Object System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension($oids, $false)))
    $csr = $request.CreateSigningRequest()
    if ($null -eq $csr -or $csr.Length -lt 256) { throw 'CSR generation failed' }
    Write-Output (([BitConverter]::ToString($csr)).Replace('-', '').ToLowerInvariant())
} catch {
    if ($created -and $null -ne $key) { try { $key.Delete() } catch {} }
    throw
} finally {
    if ($null -ne $rsa) { $rsa.Dispose() }
    if ($null -ne $key) { $key.Dispose() }
}
"#;

#[cfg(windows)]
const INSTALL_CERTIFICATE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function Convert-HexToBytes([string]$hex) {
    if ([string]::IsNullOrEmpty($hex) -or ($hex.Length % 2) -ne 0 -or $hex -notmatch '^[0-9a-f]+$') { throw 'invalid certificate hex' }
    $bytes = New-Object byte[] ($hex.Length / 2)
    for ($index = 0; $index -lt $bytes.Length; $index++) { $bytes[$index] = [Convert]::ToByte($hex.Substring($index * 2, 2), 16) }
    return $bytes
}
$keyName = 'part-crm-bridge-' + $env:BRIDGE_HOST_OPS_DEVICE_ID
$provider = [System.Security.Cryptography.CngProvider]::MicrosoftSoftwareKeyStorageProvider
$inputLines = [Console]::In.ReadToEnd() -split "`n"
if ($inputLines.Count -ne 2) { throw 'invalid certificate install input' }
$expectedSha256 = $inputLines[0].TrimEnd("`r")
$leafHex = $inputLines[1].TrimEnd("`r")
if ($expectedSha256 -notmatch '^[0-9a-f]{64}$') { throw 'invalid expected certificate fingerprint' }
$key = $null
$rsa = $null
$publicRsa = $null
$publicCertificate = $null
$certificateWithKey = $null
$installedRsa = $null
$store = $null
$thumbprint = $null
$added = $false
$preexisting = $false
$success = $false
try {
    if (-not [System.Security.Cryptography.CngKey]::Exists($keyName, $provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)) { throw 'device key is missing' }
    $key = [System.Security.Cryptography.CngKey]::Open($keyName, $provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)
    if ($key.ExportPolicy -ne [System.Security.Cryptography.CngExportPolicies]::None) { throw 'device key is exportable' }
    $rsa = New-Object System.Security.Cryptography.RSACng($key)
    $leafBytes = Convert-HexToBytes $leafHex
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try { $fingerprint = ([BitConverter]::ToString($sha256.ComputeHash($leafBytes))).Replace('-', '').ToLowerInvariant() } finally { $sha256.Dispose() }
    if ($fingerprint -ne $expectedSha256) { throw 'certificate fingerprint mismatch' }
    $publicCertificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($leafBytes)
    if ($publicCertificate.HasPrivateKey) { throw 'server certificate unexpectedly carries a private key' }
    $clientAuth = $false
    foreach ($extension in $publicCertificate.Extensions) {
        if ($extension.Oid.Value -eq '2.5.29.37') {
            $enhanced = New-Object System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension($extension, $extension.Critical)
            foreach ($usage in $enhanced.EnhancedKeyUsages) { if ($usage.Value -eq '1.3.6.1.5.5.7.3.2') { $clientAuth = $true } }
        }
    }
    if (-not $clientAuth) { throw 'certificate lacks ClientAuth EKU' }
    $now = [DateTime]::UtcNow
    if ($publicCertificate.NotBefore.ToUniversalTime() -gt $now -or $publicCertificate.NotAfter.ToUniversalTime() -le $now) { throw 'certificate is not currently valid' }
    $publicRsa = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPublicKey($publicCertificate)
    if ($null -eq $publicRsa) { throw 'certificate public key is not RSA' }
    $expectedPublic = $rsa.ExportParameters($false)
    $actualPublic = $publicRsa.ExportParameters($false)
    if ([Convert]::ToBase64String($expectedPublic.Modulus) -ne [Convert]::ToBase64String($actualPublic.Modulus) -or [Convert]::ToBase64String($expectedPublic.Exponent) -ne [Convert]::ToBase64String($actualPublic.Exponent)) { throw 'certificate public key does not match device key' }
    $certificateWithKey = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::CopyWithPrivateKey($publicCertificate, $rsa)
    $thumbprint = $certificateWithKey.Thumbprint.ToUpperInvariant()
    $certificatePath = "Cert:\LocalMachine\My\$thumbprint"
    $preexisting = Test-Path $certificatePath
    if ($preexisting) { throw 'certificate already exists' }
    $store = New-Object System.Security.Cryptography.X509Certificates.X509Store('My', [System.Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine)
    $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    $store.Add($certificateWithKey)
    $added = $true
    $store.Close()
    $store = $null
    $installed = Get-Item -Path $certificatePath -ErrorAction Stop
    if (-not $installed.HasPrivateKey) { throw 'installed certificate has no private key' }
    $installedRsa = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($installed)
    if ($null -eq $installedRsa -or $installedRsa.GetType().FullName -ne 'System.Security.Cryptography.RSACng') { throw 'installed certificate private key is not CNG RSA' }
    if ($installedRsa.Key.KeyName -ne $keyName -or $installedRsa.Key.ExportPolicy -ne [System.Security.Cryptography.CngExportPolicies]::None) { throw 'installed certificate is not bound to the exact non-exportable device key' }
    $success = $true
    Write-Output ([string]::Join("`t", @($installed.Thumbprint.ToUpperInvariant(), $fingerprint, '1', '1', '1')))
} finally {
    if ($null -ne $store) { try { $store.Close() } catch {} }
    if (-not $success) {
        if ($added -and $null -ne $thumbprint) {
            $path = "Cert:\LocalMachine\My\$thumbprint"
            if (Test-Path $path) { Remove-Item -Path $path -DeleteKey -Confirm:$false -ErrorAction SilentlyContinue }
        } elseif (-not $preexisting -and $null -ne $key) {
            try { $key.Delete() } catch {}
        }
    }
    if ($null -ne $installedRsa) { $installedRsa.Dispose() }
    if ($null -ne $certificateWithKey) { $certificateWithKey.Dispose() }
    if ($null -ne $publicRsa) { $publicRsa.Dispose() }
    if ($null -ne $publicCertificate) { $publicCertificate.Dispose() }
    if ($null -ne $rsa) { $rsa.Dispose() }
    if ($null -ne $key) { $key.Dispose() }
}
"#;

#[cfg(windows)]
const CLEANUP_KEY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
function Test-KeyInUse([string]$keyName) {
    foreach ($candidateCertificate in Get-ChildItem -Path 'Cert:\LocalMachine\My') {
        if (-not $candidateCertificate.HasPrivateKey) { continue }
        $candidateRsa = $null
        try {
            $candidateRsa = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($candidateCertificate)
            if ($null -ne $candidateRsa -and $candidateRsa.GetType().FullName -eq 'System.Security.Cryptography.RSACng' -and $candidateRsa.Key.KeyName -eq $keyName) {
                return $true
            }
        } catch {
        } finally {
            if ($null -ne $candidateRsa) { $candidateRsa.Dispose() }
        }
    }
    return $false
}
$keyName = 'part-crm-bridge-' + $env:BRIDGE_HOST_OPS_DEVICE_ID
$provider = [System.Security.Cryptography.CngProvider]::MicrosoftSoftwareKeyStorageProvider
if (Test-KeyInUse $keyName) {
    Write-Output 'in_use'
    exit 0
}
if ([System.Security.Cryptography.CngKey]::Exists($keyName, $provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)) {
    $key = [System.Security.Cryptography.CngKey]::Open($keyName, $provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)
    try { $key.Delete() } finally { $key.Dispose() }
    Write-Output 'removed'
} else {
    Write-Output 'absent'
}
"#;

#[cfg(all(test, windows))]
const TEST_PUBLIC_CERTIFICATE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$keyName = 'part-crm-bridge-' + $env:BRIDGE_HOST_OPS_DEVICE_ID
$provider = [System.Security.Cryptography.CngProvider]::MicrosoftSoftwareKeyStorageProvider
$key = [System.Security.Cryptography.CngKey]::Open($keyName, $provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)
$rsa = $null
$certificate = $null
try {
    $exportRejected = $false
    try { $null = $key.Export([System.Security.Cryptography.CngKeyBlobFormat]::Pkcs8PrivateBlob) } catch [System.Security.Cryptography.CryptographicException] { $exportRejected = $true }
    if (-not $exportRejected) { throw 'private-key export unexpectedly succeeded' }
    $rsa = New-Object System.Security.Cryptography.RSACng($key)
    $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new('CN=part-crm-bridge-device', $rsa, [System.Security.Cryptography.HashAlgorithmName]::SHA256, [System.Security.Cryptography.RSASignaturePadding]::Pkcs1)
    $oids = New-Object System.Security.Cryptography.OidCollection
    $null = $oids.Add((New-Object System.Security.Cryptography.Oid('1.3.6.1.5.5.7.3.2')))
    $request.CertificateExtensions.Add((New-Object System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension($oids, $false)))
    $certificate = $request.CreateSelfSigned([DateTimeOffset]::UtcNow.AddMinutes(-1), [DateTimeOffset]::UtcNow.AddHours(1))
    $leaf = $certificate.RawData
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try { $fingerprint = ([BitConverter]::ToString($sha256.ComputeHash($leaf))).Replace('-', '').ToLowerInvariant() } finally { $sha256.Dispose() }
    $hex = ([BitConverter]::ToString($leaf)).Replace('-', '').ToLowerInvariant()
    Write-Output ([string]::Join("`t", @($fingerprint, $hex)))
} finally {
    if ($null -ne $certificate) { $certificate.Dispose() }
    if ($null -ne $rsa) { $rsa.Dispose() }
    $key.Dispose()
}
"#;

#[cfg(windows)]
fn execute_issue(
    plan: &EnrollmentHttpPlan,
    token_file: &Path,
) -> HostOpsResult<EnrollmentIssueProjection> {
    let output = execute_http(plan, token_file, "enrollment_issue_effect_failed")?;
    parse_issue_output(&output, plan.success_statuses)
}

#[cfg(windows)]
fn execute_redeem(
    plan: &EnrollmentHttpPlan,
    token_file: &Path,
) -> HostOpsResult<EnrollmentRedemptionProjection> {
    let output = execute_http(plan, token_file, "enrollment_redeem_effect_failed")?;
    parse_redeem_output(&output, plan.success_statuses)
}

#[cfg(windows)]
fn execute_http(
    plan: &EnrollmentHttpPlan,
    token_file: &Path,
    effect_failure_code: &'static str,
) -> HostOpsResult<String> {
    let mut token = read_secret_file(token_file, 32_768)?;
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
    let mut config = match plan.stdin_config(token_text) {
        Ok(value) => value,
        Err(error) => {
            token.fill(0);
            return Err(error);
        }
    };
    token.fill(0);
    let mut command = curl_command()?;
    command.args(plan.curl_arguments());
    let output = run_command(
        command,
        Some(std::mem::take(&mut config)),
        MAX_HTTP_OUTPUT_SIZE,
        effect_failure_code,
    )?;
    config.fill(0);
    Ok(output)
}

#[cfg(windows)]
fn create_or_reuse_machine_csr(device_id: &str) -> HostOpsResult<String> {
    validate_identifier(device_id).map_err(|_| HostOpsError::new("invalid_server_device_id"))?;
    let mut command = powershell_command(CREATE_CSR_SCRIPT)?;
    command.env("BRIDGE_HOST_OPS_DEVICE_ID", device_id);
    let output = run_command(
        command,
        None,
        MAX_CSR_DER_HEX_LENGTH + 1_024,
        "enrollment_csr_effect_failed",
    )?;
    let csr = output.trim().to_owned();
    if csr == "in_use" {
        return Err(HostOpsError::new("enrollment_device_already_installed"));
    }
    validate_der_hex(&csr, MAX_CSR_DER_HEX_LENGTH, "invalid_local_csr")?;
    Ok(csr)
}

#[cfg(windows)]
fn install_enrollment_certificate(
    device_id: &str,
    certificate_sha256: &str,
    leaf_certificate_der_hex: &str,
) -> HostOpsResult<CertificateObservation> {
    validate_identifier(device_id).map_err(|_| HostOpsError::new("invalid_server_device_id"))?;
    validate_sha256_fingerprint(certificate_sha256)?;
    validate_der_hex(
        leaf_certificate_der_hex,
        MAX_CERTIFICATE_DER_HEX_LENGTH,
        "invalid_enrollment_certificate",
    )?;
    let mut command = powershell_command(INSTALL_CERTIFICATE_SCRIPT)?;
    command.env("BRIDGE_HOST_OPS_DEVICE_ID", device_id);
    let input = format!("{certificate_sha256}\n{leaf_certificate_der_hex}").into_bytes();
    let output = run_command(
        command,
        Some(input),
        8_192,
        "enrollment_certificate_install_effect_failed",
    )?;
    parse_certificate_observation(&output)
}

#[cfg(windows)]
fn cleanup_machine_key(device_id: &str) -> HostOpsResult<()> {
    validate_identifier(device_id).map_err(|_| HostOpsError::new("invalid_server_device_id"))?;
    let mut command = powershell_command(CLEANUP_KEY_SCRIPT)?;
    command.env("BRIDGE_HOST_OPS_DEVICE_ID", device_id);
    let output = run_command(command, None, 1_024, "enrollment_key_cleanup_effect_failed")?;
    match output.trim() {
        "removed" | "absent" => Ok(()),
        "in_use" => Err(HostOpsError::new("enrollment_key_in_use")),
        _ => Err(HostOpsError::new("enrollment_key_cleanup_failed")),
    }
}

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(windows)]
fn system_root() -> HostOpsResult<OsString> {
    env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HostOpsError::new("windows_system_root_unavailable"))
}

#[cfg(windows)]
fn read_secret_file(path: &Path, maximum_size: usize) -> HostOpsResult<Vec<u8>> {
    let mut bytes = fs::read(path).map_err(|_| HostOpsError::new("secret_input_unavailable"))?;
    if bytes.len() > maximum_size {
        bytes.fill(0);
        return Err(HostOpsError::new("secret_input_too_large"));
    }
    Ok(bytes)
}

#[cfg(windows)]
fn run_command(
    mut command: Command,
    mut input: Option<Vec<u8>>,
    maximum_output_size: usize,
    effect_failure_code: &'static str,
) -> HostOpsResult<String> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = match command.spawn() {
        Ok(value) => value,
        Err(_) => {
            if let Some(value) = input.as_mut() {
                value.fill(0);
            }
            return Err(HostOpsError::new("host_effect_spawn_failed"));
        }
    };
    let mut write_failed = false;
    if let Some(value) = input.as_mut() {
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(value).is_err() {
                write_failed = true;
            }
        } else {
            write_failed = true;
        }
        value.fill(0);
    }
    let output = child
        .wait_with_output()
        .map_err(|_| HostOpsError::new("host_effect_wait_failed"))?;
    if write_failed {
        return Err(HostOpsError::new("host_effect_stdin_failed"));
    }
    if !output.status.success() {
        return Err(HostOpsError::new(effect_failure_code));
    }
    if output.stdout.len() > maximum_output_size {
        return Err(HostOpsError::new("host_effect_output_too_large"));
    }
    String::from_utf8(output.stdout).map_err(|_| HostOpsError::new("host_effect_output_invalid"))
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_CERTIFICATE_DER_HEX_LENGTH, parse_issue_output, parse_redeem_output, redeem_plan,
        render_enrollment_receipt, self_test,
    };
    use bridge_host_ops::parse_certificate_observation;

    #[test]
    fn enrollment_contract_has_no_caller_identity_or_digest_authority()
    -> Result<(), Box<dyn std::error::Error>> {
        self_test()?;
        let claim = "ab".repeat(32);
        let plan = redeem_plan(
            "https://control.example.test",
            "tenant_01",
            "corr_enroll_test",
            &claim,
            "3000",
        )?;
        assert_eq!(
            plan.body,
            format!("{{\"claimCode\":\"{claim}\",\"csrDerHex\":\"3000\"}}")
        );
        for forbidden in ["tenantId", "actorId", "deviceId", "csrSha256"] {
            assert!(!plan.body.contains(forbidden));
        }
        Ok(())
    }

    #[test]
    fn issue_and_redeem_responses_are_strict_and_bounded() -> Result<(), Box<dyn std::error::Error>>
    {
        let claim = "ab".repeat(32);
        let issue = parse_issue_output(
            &format!(
                "{{\"deviceId\":\"device_01\",\"claimCode\":\"{claim}\",\"expiresAtMs\":1}}\n201"
            ),
            &[200, 201],
        )?;
        assert_eq!(issue.device_id, "device_01");
        assert_eq!(issue.claim_code, claim);
        assert!(parse_issue_output(
            &format!("{{\"deviceId\":\"device_01\",\"claimCode\":\"{}\",\"expiresAtMs\":1,\"actorId\":\"actor_01\"}}\n201", "ab".repeat(32)),
            &[200, 201],
        ).is_err());
        let redeem = parse_redeem_output(
            &format!(
                "{{\"certificateChainDerHex\":[],\"leafCertificateDerHex\":\"3000\",\"certificateSha256\":\"{}\",\"deviceId\":\"device_01\"}}\n200",
                "cd".repeat(32)
            ),
            &[200],
        )?;
        assert_eq!(redeem.device_id, "device_01");
        assert_eq!(redeem.certificate_sha256, "cd".repeat(32));
        assert!(parse_redeem_output(
            &format!("{{\"certificateChainDerHex\":[],\"leafCertificateDerHex\":\"{}\",\"certificateSha256\":\"{}\",\"deviceId\":\"device_01\"}}\n200", "aa".repeat(MAX_CERTIFICATE_DER_HEX_LENGTH / 2 + 1), "cd".repeat(32)),
            &[200],
        ).is_err());
        Ok(())
    }

    #[test]
    fn enrollment_receipt_contains_no_claim_csr_or_certificate_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let certificate = parse_certificate_observation(&format!(
            "{}\t{}\t1\t1\t1",
            "AB".repeat(20),
            "cd".repeat(32)
        ))?;
        let receipt =
            render_enrollment_receipt("https://control.example.test", "device_01", &certificate);
        assert!(receipt.contains("\"operation\":\"enroll\""));
        assert!(receipt.contains("\"deviceId\":\"device_01\""));
        for forbidden in [
            "claimCode",
            "csrDerHex",
            "leafCertificateDerHex",
            "certificateChainDerHex",
        ] {
            assert!(!receipt.contains(forbidden));
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn shipping_enrollment_crypto_path_reuses_exact_non_exportable_machine_key()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{
            TEST_PUBLIC_CERTIFICATE_SCRIPT, cleanup_machine_key, create_or_reuse_machine_csr,
            install_enrollment_certificate, powershell_command, run_command,
        };
        let device_id = format!("device_{}", "a".repeat(32));
        let csr = create_or_reuse_machine_csr(&device_id)?;
        assert!(csr.starts_with("30"));
        let mut command = powershell_command(TEST_PUBLIC_CERTIFICATE_SCRIPT)?;
        command.env("BRIDGE_HOST_OPS_DEVICE_ID", &device_id);
        let public = run_command(
            command,
            None,
            MAX_CERTIFICATE_DER_HEX_LENGTH + 1_024,
            "enrollment_test_certificate_effect_failed",
        )?;
        let Some((fingerprint, leaf)) = public.trim().split_once('\t') else {
            cleanup_machine_key(&device_id)?;
            return Err("invalid test certificate output".into());
        };
        let certificate = install_enrollment_certificate(&device_id, fingerprint, leaf)?;
        assert_eq!(certificate.sha256_fingerprint(), fingerprint);
        let reuse_error = match create_or_reuse_machine_csr(&device_id) {
            Ok(_) => return Err("installed enrollment key unexpectedly reusable".into()),
            Err(error) => error,
        };
        assert_eq!(reuse_error.code(), "enrollment_device_already_installed");
        let cleanup_error = match cleanup_machine_key(&device_id) {
            Ok(()) => return Err("installed enrollment key unexpectedly removable".into()),
            Err(error) => error,
        };
        assert_eq!(cleanup_error.code(), "enrollment_key_in_use");
        super::super::windows::remove_certificate(certificate.sha1_thumbprint())?;
        Ok(())
    }
}
