# Synthetic CI proof only: no vault, provider credentials or Windows certificate-store writes.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = Join-Path ([IO.Path]::GetTempPath()) ('bridge-delivery-signing-' + [Guid]::NewGuid())
[void](New-Item -ItemType Directory -Path $root)
$rsa = [System.Security.Cryptography.RSA]::Create(3072)
$cert = $null
try {
    Add-Type -AssemblyName System.Security
    $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
        'CN=Ephemeral CI delivery integrity', $rsa,
        [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
    )
    $cert = $request.CreateSelfSigned([DateTimeOffset]::UtcNow.AddMinutes(-1), [DateTimeOffset]::UtcNow.AddHours(1))
    $manifest = Join-Path $root 'manifest.json'
    [IO.File]::WriteAllText($manifest, '{"sequence":1}')
    foreach ($index in 1, 2) {
        $env:PROFILE_BRIDGE_DELIVERY_PFX_BASE64 = [Convert]::ToBase64String(
            $cert.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Pfx, '')
        )
        & "$PSScriptRoot/../scripts/windows-delivery-sign.ps1" -ManifestPath $manifest -OutputPath "$root/signature-$index.json" -PublicMetadataPath "$root/public-$index.json"
    }
    $first = [IO.File]::ReadAllText("$root/signature-1.json")
    if ($first -cne [IO.File]::ReadAllText("$root/signature-2.json")) { throw 'CMS replay differs' }
    if (Test-Path Env:PROFILE_BRIDGE_DELIVERY_PFX_BASE64) { throw 'Signing key environment retained' }
    $wire = $first | ConvertFrom-Json
    $public = Get-Content -Raw "$root/public-1.json" | ConvertFrom-Json
    if ($wire.key_id -cne $public.key_id -or $public.certificate_sha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw 'Public pin metadata mismatch'
    }
    $der = [byte[]]::new($wire.cms_der_hex.Length / 2)
    for ($index = 0; $index -lt $der.Length; $index++) {
        $der[$index] = [Convert]::ToByte($wire.cms_der_hex.Substring(2 * $index, 2), 16)
    }
    $cms = [System.Security.Cryptography.Pkcs.SignedCms]::new(
        [System.Security.Cryptography.Pkcs.ContentInfo]::new([IO.File]::ReadAllBytes($manifest)), $true
    )
    $cms.Decode($der)
    $cms.CheckSignature($true)
    $wrong = [System.Security.Cryptography.Pkcs.SignedCms]::new(
        [System.Security.Cryptography.Pkcs.ContentInfo]::new([Text.Encoding]::UTF8.GetBytes('{"sequence":2}')), $true
    )
    $wrong.Decode($der)
    $rejected = $false
    try { $wrong.CheckSignature($true) } catch [System.Security.Cryptography.CryptographicException] { $rejected = $true }
    if (-not $rejected) { throw 'Tampered manifest accepted' }
    # Exercise the real signing adapter in a child so its fail-closed exit cannot terminate this proof.
    $process = Start-Process -FilePath (Get-Process -Id $PID).Path -WindowStyle Hidden -Wait -PassThru -ArgumentList @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-File',
        ('"' + "$PSScriptRoot/../scripts/windows-delivery-sign.ps1" + '"'),
        '-ManifestPath', ('"' + $manifest + '"'), '-OutputPath', ('"' + "$root/missing.json" + '"'),
        '-PublicMetadataPath', ('"' + "$root/missing-public.json" + '"')
    )
    if ($process.ExitCode -eq 0 -or (Test-Path "$root/missing.json")) { throw 'Missing key accepted' }
    Write-Host 'Exact CMS signing, deterministic replay, tamper rejection and missing-key rejection proved.'
} finally {
    Remove-Item Env:PROFILE_BRIDGE_DELIVERY_PFX_BASE64 -ErrorAction SilentlyContinue
    if ($null -ne $cert) { $cert.Dispose() }
    $rsa.Dispose()
    # Only this proof's freshly created absolute directory is removed.
    if ($root.StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}
