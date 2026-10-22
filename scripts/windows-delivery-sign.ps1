# CI-only cryptographic effect adapter for the existing Windows delivery contract.
# Protected Release Set Build owns publication. This script cannot select/adopt a candidate.
# The private key is supplied only through the step environment, used in memory, never logged,
# written to disk or installed in a Windows certificate store. This is NOT OS publisher signing.
param(
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [Parameter(Mandatory=$true)][string]$OutputPath,
    [Parameter(Mandatory=$true)][string]$PublicMetadataPath
)
$ErrorActionPreference = 'Stop'
$certificate = $null
$pfx = $null
try {
    Add-Type -AssemblyName System.Security
    if ([string]::IsNullOrWhiteSpace($env:PROFILE_BRIDGE_DELIVERY_PFX_BASE64)) { throw 'missing key' }
    if ((Test-Path -LiteralPath $OutputPath) -or (Test-Path -LiteralPath $PublicMetadataPath)) { throw 'output exists' }
    $pfx = [Convert]::FromBase64String($env:PROFILE_BRIDGE_DELIVERY_PFX_BASE64)
    Remove-Item Env:PROFILE_BRIDGE_DELIVERY_PFX_BASE64
    $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
        $pfx, '', [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
    )
    if (-not $certificate.HasPrivateKey -or $certificate.NotBefore.ToUniversalTime() -gt [DateTime]::UtcNow -or
        $certificate.NotAfter.ToUniversalTime() -le [DateTime]::UtcNow -or
        $certificate.NotAfter.ToUniversalTime() -gt $certificate.NotBefore.ToUniversalTime().AddMonths(6)) { throw 'unavailable key' }
    $rsa = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($certificate)
    if ($null -eq $rsa -or $rsa.KeySize -lt 3072) { throw 'invalid key' }
    $rsa.Dispose()
    $manifest = [System.IO.File]::ReadAllBytes($ManifestPath)
    if ($manifest.Length -eq 0 -or $manifest.Length -gt 1048576) { throw 'invalid manifest' }
    $cms = [System.Security.Cryptography.Pkcs.SignedCms]::new(
        [System.Security.Cryptography.Pkcs.ContentInfo]::new($manifest), $true
    )
    $signer = [System.Security.Cryptography.Pkcs.CmsSigner]::new($certificate)
    $signer.IncludeOption = [System.Security.Cryptography.X509Certificates.X509IncludeOption]::EndCertOnly
    $signer.DigestAlgorithm = [System.Security.Cryptography.Oid]::new('2.16.840.1.101.3.4.2.1')
    # RSA PKCS#1 without SigningTime attributes makes the exact delivery signature replayable.
    $cms.ComputeSignature($signer)
    $cms.CheckSignature($true)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $pin = [BitConverter]::ToString($sha.ComputeHash($certificate.RawData)).Replace('-', '').ToLowerInvariant()
    } finally { $sha.Dispose() }
    $keyId = $pin
    $utf8 = [System.Text.UTF8Encoding]::new($false)
    $signature = [ordered]@{
        schema_version = 1; kind = 'WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS'; key_id = $keyId
        cms_der_hex = [BitConverter]::ToString($cms.Encode()).Replace('-', '').ToLowerInvariant()
    } | ConvertTo-Json -Compress
    [System.IO.File]::WriteAllText($OutputPath, $signature, $utf8)
    [System.IO.File]::WriteAllText($PublicMetadataPath, (
        [ordered]@{ key_id = $keyId; certificate_sha256 = $pin } | ConvertTo-Json -Compress
    ), $utf8)
} catch {
    # Never propagate provider/.NET exceptions that may include private input or environment data.
    [Console]::Error.WriteLine('WINDOWS_DELIVERY_SIGNING_FAILED')
    exit 1
} finally {
    if ($null -ne $pfx) { [Array]::Clear($pfx, 0, $pfx.Length) }
    if ($null -ne $certificate) { $certificate.Dispose() }
    Remove-Item Env:PROFILE_BRIDGE_DELIVERY_PFX_BASE64 -ErrorAction SilentlyContinue
}
