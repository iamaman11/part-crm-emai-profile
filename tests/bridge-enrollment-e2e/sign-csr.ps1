$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Convert-HexToBytes([string]$Hex) {
    if ([string]::IsNullOrWhiteSpace($Hex) -or ($Hex.Length % 2) -ne 0 -or $Hex -notmatch '^[0-9a-f]+$') {
        throw 'invalid lowercase DER hex'
    }
    return [Convert]::FromHexString($Hex)
}

function Convert-BytesToLowerHex([byte[]]$Bytes) {
    return ([Convert]::ToHexString($Bytes)).ToLowerInvariant()
}

$raw = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($raw) -or $raw.Length -gt (512 * 1024)) {
    throw 'invalid signer request'
}
$payload = $raw | ConvertFrom-Json -AsHashtable
$required = @('tenantId', 'actorId', 'deviceId', 'csrSha256', 'csrDerHex', 'profile')
if ($payload.Keys.Count -ne $required.Count) {
    throw 'unexpected signer request fields'
}
foreach ($field in $required) {
    if (-not $payload.ContainsKey($field) -or $payload[$field] -isnot [string] -or [string]::IsNullOrWhiteSpace($payload[$field])) {
        throw "missing signer field: $field"
    }
}
if ($payload.profile -ne 'windows_rsa_sha256_client_auth_v1') {
    throw 'unsupported certificate profile'
}
if ($payload.csrSha256 -notmatch '^[0-9a-f]{64}$') {
    throw 'invalid CSR fingerprint'
}

$csrBytes = Convert-HexToBytes $payload.csrDerHex
$sha256 = [System.Security.Cryptography.SHA256]::Create()
$csrFingerprint = $null
try {
    $csrFingerprint = Convert-BytesToLowerHex ($sha256.ComputeHash($csrBytes))
} finally {
    $sha256.Dispose()
}
if ($csrFingerprint -ne $payload.csrSha256) {
    throw 'CSR fingerprint mismatch'
}

$request = $null
$rsaPublic = $null
$caKey = $null
$caRequest = $null
$caCertificate = $null
$leafCertificate = $null
try {
    $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::LoadSigningRequest(
        $csrBytes,
        [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.X509Certificates.CertificateRequestLoadOptions]::Default,
        [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
    )
    if ($request.HashAlgorithm.Name -ne [System.Security.Cryptography.HashAlgorithmName]::SHA256.Name) {
        throw 'CSR signature hash is not SHA-256'
    }
    if ($request.PublicKey.Oid.Value -ne '1.2.840.113549.1.1.1') {
        throw 'CSR public key is not RSA'
    }
    $rsaPublic = [System.Security.Cryptography.RSA]::Create()
    $bytesRead = 0
    $rsaPublic.ImportSubjectPublicKeyInfo($request.PublicKey.ExportSubjectPublicKeyInfo(), [ref]$bytesRead)
    if ($rsaPublic.KeySize -lt 2048) {
        throw 'CSR RSA key is too small'
    }

    $ekuExtensions = @($request.CertificateExtensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' })
    if ($ekuExtensions.Count -ne 1) {
        throw 'CSR must contain exactly one EKU extension'
    }
    $eku = [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new(
        $ekuExtensions[0],
        $ekuExtensions[0].Critical
    )
    $ekuValues = @($eku.EnhancedKeyUsages | ForEach-Object { $_.Value })
    if ($ekuValues.Count -ne 1 -or $ekuValues[0] -ne '1.3.6.1.5.5.7.3.2') {
        throw 'CSR is not ClientAuth-only'
    }

    $caKey = [System.Security.Cryptography.RSA]::Create(2048)
    $caRequest = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
        'CN=part-crm-bridge-e2e-test-ca',
        $caKey,
        [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
    )
    $caRequest.CertificateExtensions.Add(
        [System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($true, $false, 0, $true)
    )
    $caRequest.CertificateExtensions.Add(
        [System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new(
            [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::KeyCertSign -bor [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::CrlSign,
            $true
        )
    )
    $caCertificate = $caRequest.CreateSelfSigned(
        [DateTimeOffset]::UtcNow.AddMinutes(-5),
        [DateTimeOffset]::UtcNow.AddHours(2)
    )
    $serial = [System.Security.Cryptography.RandomNumberGenerator]::GetBytes(16)
    $leafCertificate = $request.Create(
        $caCertificate,
        [DateTimeOffset]::UtcNow.AddMinutes(-1),
        [DateTimeOffset]::UtcNow.AddHours(1),
        $serial
    )

    $leafDer = $leafCertificate.RawData
    $leafSha = [System.Security.Cryptography.SHA256]::HashData($leafDer)
    $result = [ordered]@{
        csrSha256 = $payload.csrSha256
        certificateSha256 = Convert-BytesToLowerHex $leafSha
        leafCertificateDerHex = Convert-BytesToLowerHex $leafDer
        certificateChainDerHex = @((Convert-BytesToLowerHex $caCertificate.RawData))
    }
    [Console]::Out.Write(($result | ConvertTo-Json -Compress -Depth 4))
} finally {
    if ($null -ne $leafCertificate) { $leafCertificate.Dispose() }
    if ($null -ne $caCertificate) { $caCertificate.Dispose() }
    if ($null -ne $caRequest) { $caRequest = $null }
    if ($null -ne $caKey) { $caKey.Dispose() }
    if ($null -ne $rsaPublic) { $rsaPublic.Dispose() }
    if ($null -ne $request) { $request = $null }
}
