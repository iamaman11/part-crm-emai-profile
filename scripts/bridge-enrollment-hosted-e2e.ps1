param(
    [string]$ReportPath = 'artifacts/bridge-enrollment-e2e/evidence.json'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version Latest

$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$SourceSha = (& git -C $Root rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $SourceSha -notmatch '^[0-9a-f]{40}$') {
    throw 'unable to resolve exact source SHA'
}

foreach ($name in @(
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_OBSERVE_API_TOKEN',
    'CLOUDFLARE_ZERO_TRUST_OBSERVE_API_TOKEN',
    'CLOUDFLARE_ACCOUNT_ID'
)) {
    if (-not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($name))) {
        throw "$name must not be exposed to the local Bridge enrollment E2E harness"
    }
}

$ControlPort = 18787
$DependencyPort = 19080
$IngressPort = 19443
$TenantId = 'tenant_e2e_01'
$ActorId = 'actor_e2e_01'
$IdentityId = 'identity_e2e_01'
$AccessSubject = 'bridge_e2e_subject_01'
$Origin = "https://localhost:$IngressPort"
$Scratch = Join-Path $env:RUNNER_TEMP "bridge-enrollment-e2e-$PID"
$State = Join-Path $Scratch 'state'
$MigrationDir = Join-Path $Scratch 'migrations'
$ControlConfig = Join-Path $Scratch 'control.wrangler.jsonc'
$SignerConfig = Join-Path $Scratch 'signer.wrangler.jsonc'
$TokenFile = Join-Path $Scratch 'access-token.txt'
$TlsPfx = Join-Path $Scratch 'ingress.pfx'
$DependencyStdout = Join-Path $Scratch 'dependency.stdout.log'
$DependencyStderr = Join-Path $Scratch 'dependency.stderr.log'
$WranglerStdout = Join-Path $Scratch 'wrangler.stdout.log'
$WranglerStderr = Join-Path $Scratch 'wrangler.stderr.log'
$DependencyServer = Join-Path $Root 'tests/bridge-enrollment-e2e/dependency-server.mjs'
$SignerWorker = Join-Path $Root 'tests/bridge-enrollment-e2e/signer-worker.mjs'
$SignerScript = Join-Path $Root 'tests/bridge-enrollment-e2e/sign-csr.ps1'
$HostBinary = Join-Path $Root 'tools/bridge-host-ops/target/release/bridge-host-ops.exe'
$WorkerShim = Join-Path $Root 'apps/control-plane-worker/build/worker/shim.mjs'
$Provider = [System.Security.Cryptography.CngProvider]::MicrosoftSoftwareKeyStorageProvider

$DependencyProcess = $null
$WranglerProcess = $null
$TlsCertificate = $null
$TlsRootStore = $null
$PositiveThumbprint = $null
$PositiveDeviceId = $null
$EvidenceWritten = $false

function Convert-BytesToLowerHex([byte[]]$Bytes) {
    return ([Convert]::ToHexString($Bytes)).ToLowerInvariant()
}

function Write-JsonFile([string]$Path, [object]$Value) {
    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrEmpty($parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    ($Value | ConvertTo-Json -Depth 20) + "`n" | Set-Content -Path $Path -Encoding utf8NoBOM
}

function Invoke-Wrangler([string[]]$Arguments) {
    & npx.cmd --yes 'wrangler@4.94.0' @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Wrangler failed: $($Arguments -join ' ')"
    }
}

function Stop-ProcessTree([System.Diagnostics.Process]$Process) {
    if ($null -eq $Process -or $Process.HasExited) { return }
    & taskkill.exe /PID $Process.Id /T /F 2>$null | Out-Null
}

function Wait-HttpReady(
    [string]$Uri,
    [System.Diagnostics.Process]$Process,
    [string]$Label,
    [int]$Attempts = 90
) {
    for ($index = 0; $index -lt $Attempts; $index++) {
        if ($null -ne $Process -and $Process.HasExited) {
            throw "$Label exited before readiness"
        }
        try {
            $response = Invoke-WebRequest -Uri $Uri -Method Get -TimeoutSec 2 -SkipHttpErrorCheck
            if ([int]$response.StatusCode -eq 200) { return }
        } catch {
        }
        Start-Sleep -Seconds 1
    }
    throw "$Label did not become ready"
}

function New-TestCsrHex {
    $rsa = [System.Security.Cryptography.RSA]::Create(2048)
    $request = $null
    try {
        $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
            'CN=part-crm-bridge-e2e-negative',
            $rsa,
            [System.Security.Cryptography.HashAlgorithmName]::SHA256,
            [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
        )
        $oids = [System.Security.Cryptography.OidCollection]::new()
        $null = $oids.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.2'))
        $request.CertificateExtensions.Add(
            [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($oids, $false)
        )
        return Convert-BytesToLowerHex $request.CreateSigningRequest()
    } finally {
        if ($null -ne $rsa) { $rsa.Dispose() }
    }
}

function Invoke-EnrollmentHttp(
    [string]$Path,
    [string]$Body,
    [string]$CorrelationId,
    [int[]]$ExpectedStatus,
    [string]$IdempotencyKey = ''
) {
    $token = (Get-Content -Raw -Path $TokenFile).Trim()
    $headers = @{
        'accept' = 'application/json'
        'cf-access-token' = $token
        'X-Correlation-Id' = $CorrelationId
    }
    if (-not [string]::IsNullOrEmpty($IdempotencyKey)) {
        $headers['Idempotency-Key'] = $IdempotencyKey
    }
    $response = Invoke-WebRequest \
        -Uri "$Origin$Path" \
        -Method Post \
        -Headers $headers \
        -ContentType 'application/json' \
        -Body $Body \
        -SkipHttpErrorCheck
    $status = [int]$response.StatusCode
    if ($ExpectedStatus -notcontains $status) {
        throw "unexpected enrollment HTTP status $status for $Path"
    }
    $document = $null
    if (-not [string]::IsNullOrWhiteSpace($response.Content)) {
        $document = $response.Content | ConvertFrom-Json
    }
    return [pscustomobject]@{
        Status = $status
        Document = $document
    }
}

function Issue-Claim([string]$CorrelationId, [string]$IdempotencyKey) {
    $response = Invoke-EnrollmentHttp \
        -Path "/api/v1/tenants/$TenantId/bridge-enrollment/authorities" \
        -Body '{}' \
        -CorrelationId $CorrelationId \
        -ExpectedStatus @(201) \
        -IdempotencyKey $IdempotencyKey
    if ($response.Document.claimCode -notmatch '^[0-9a-f]{64}$' -or
        $response.Document.deviceId -notmatch '^[A-Za-z0-9_-]{8,96}$' -or
        [uint64]$response.Document.expiresAtMs -eq 0) {
        throw 'issue response identity is malformed'
    }
    return $response.Document
}

function Redeem-Claim(
    [object]$Issue,
    [string]$CsrHex,
    [string]$CorrelationId,
    [int[]]$ExpectedStatus
) {
    $body = [ordered]@{
        claimCode = [string]$Issue.claimCode
        csrDerHex = $CsrHex
    } | ConvertTo-Json -Compress
    return Invoke-EnrollmentHttp \
        -Path "/api/v1/tenants/$TenantId/bridge-enrollment/redemptions" \
        -Body $body \
        -CorrelationId $CorrelationId \
        -ExpectedStatus $ExpectedStatus
}

function Require-ProblemCode([object]$Response, [string]$Code) {
    if ($null -eq $Response.Document -or [string]$Response.Document.code -ne $Code) {
        throw "expected problem code $Code"
    }
}

New-Item -ItemType Directory -Path $Scratch, $State, $MigrationDir -Force | Out-Null

try {
    if (-not (Test-Path $HostBinary)) { throw 'release bridge-host-ops binary is missing' }
    if (-not (Test-Path $WorkerShim)) { throw 'control-plane Worker build output is missing' }

    $projectionText = & cargo run --locked --quiet --manifest-path (Join-Path $Root 'tools/opsctl/Cargo.toml') -- --root $Root d1 repository
    if ($LASTEXITCODE -ne 0) { throw 'typed D1 repository projection failed' }
    $projection = ($projectionText -join "`n") | ConvertFrom-Json
    $catalogs = @($projection.components | Where-Object { $_.component_id -eq 'catalog' })
    if ($catalogs.Count -ne 1) { throw 'typed Catalog projection is missing or ambiguous' }
    $sources = @($catalogs[0].executable_migration_sources)
    if ($sources.Count -eq 0) { throw 'typed Catalog migration projection is empty' }
    $seenMigrations = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($item in $sources) {
        $name = [string]$item.migration_file
        $sourceRoot = [string]$item.source_root
        if ([string]::IsNullOrWhiteSpace($name) -or [string]::IsNullOrWhiteSpace($sourceRoot) -or -not $seenMigrations.Add($name)) {
            throw 'typed Catalog migration projection is malformed'
        }
        $source = [System.IO.Path]::GetFullPath((Join-Path (Join-Path $Root $sourceRoot) $name))
        if (-not $source.StartsWith($Root, [System.StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw 'typed Catalog migration source escaped the repository or is missing'
        }
        $sourceItem = Get-Item -LiteralPath $source
        if (-not [string]::IsNullOrEmpty([string]$sourceItem.LinkType)) {
            throw 'typed Catalog migration source must be a regular file'
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $MigrationDir $name)
    }

    $shippingConfig = Get-Content -Raw -Path (Join-Path $Root 'deploy/cloudflare/wrangler.jsonc') | ConvertFrom-Json
    $profileId = [string]$shippingConfig.env.staging.vars.CAPABILITY_PROFILE_ID
    $profileDigest = [string]$shippingConfig.env.staging.vars.CAPABILITY_PROFILE_DIGEST
    if ($profileId -ne 'rehearsal-core-v2' -or $profileDigest -notmatch '^[0-9a-f]{64}$') {
        throw 'shipping V2 staging capability projection drifted'
    }
    $sourceIdentityBytes = [System.Text.Encoding]::UTF8.GetBytes("bridge-enrollment-e2e`n$SourceSha")
    $localReleaseDigest = Convert-BytesToLowerHex ([System.Security.Cryptography.SHA256]::HashData($sourceIdentityBytes))
    $localAdmissionReleaseSet = "release-set-v3-sha256-$localReleaseDigest"
    $targetObservation = "target-v1|staging|$profileId|$profileDigest|$localAdmissionReleaseSet"
    $derivationKey = Convert-BytesToLowerHex ([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32))

    $controlMain = [System.IO.Path]::GetRelativePath($Scratch, $WorkerShim).Replace('\', '/')
    $migrationsRelative = [System.IO.Path]::GetRelativePath($Scratch, $MigrationDir).Replace('\', '/')
    $signerMain = [System.IO.Path]::GetRelativePath($Scratch, $SignerWorker).Replace('\', '/')
    Write-JsonFile $ControlConfig ([ordered]@{
        name = 'bridge-enrollment-e2e-control'
        main = $controlMain
        compatibility_date = '2026-08-05'
        vars = [ordered]@{
            ACCESS_ISSUER = "http://127.0.0.1:$DependencyPort"
            ACCESS_AUDIENCE = 'bridge-enrollment-e2e-audience'
            CANONICAL_ENVIRONMENT = 'staging'
            CAPABILITY_PROFILE_ID = $profileId
            CAPABILITY_PROFILE_DIGEST = $profileDigest
            TARGET_AUTHORIZATION_OBSERVATION = $targetObservation
            BRIDGE_ENROLLMENT_DERIVATION_KEY = $derivationKey
        }
        d1_databases = @([ordered]@{
            binding = 'CATALOG_DB'
            database_name = 'bridge-enrollment-e2e-catalog'
            database_id = '00000000-0000-0000-0000-000000000081'
            migrations_dir = $migrationsRelative
        })
        services = @([ordered]@{
            binding = 'BRIDGE_CERTIFICATE_SIGNER'
            service = 'bridge-enrollment-e2e-signer'
        })
    })
    Write-JsonFile $SignerConfig ([ordered]@{
        name = 'bridge-enrollment-e2e-signer'
        main = $signerMain
        compatibility_date = '2026-08-05'
        vars = [ordered]@{
            TEST_SIGNER_ORIGIN = "http://127.0.0.1:$DependencyPort"
        }
    })

    Invoke-Wrangler @(
        'd1', 'migrations', 'apply', 'CATALOG_DB', '--local',
        '--config', $ControlConfig,
        '--persist-to', $State,
        '--experimental-provision=false',
        '--experimental-auto-create=false'
    )
    $nowMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $seedSql = @"
INSERT INTO tenants (tenant_id, display_name, status, version, created_at_ms, updated_at_ms)
VALUES ('$TenantId', 'Bridge Enrollment E2E', 'ACTIVE', 1, $nowMs, $nowMs);
INSERT INTO identities (identity_id, access_subject, verified_contact_hint, created_at_ms)
VALUES ('$IdentityId', '$AccessSubject', 'bridge-e2e@example.test', $nowMs);
INSERT INTO memberships (tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms)
VALUES ('$TenantId', '$ActorId', '$IdentityId', 'TENANT_OWNER', 'ACTIVE', 1, $nowMs, $nowMs);
"@
    Invoke-Wrangler @(
        'd1', 'execute', 'CATALOG_DB', '--local',
        '--config', $ControlConfig,
        '--persist-to', $State,
        '--command', $seedSql,
        '--experimental-provision=false',
        '--experimental-auto-create=false'
    )

    $TlsCertificate = New-SelfSignedCertificate \
        -Subject 'CN=localhost' \
        -DnsName 'localhost' \
        -CertStoreLocation 'Cert:\CurrentUser\My' \
        -KeyAlgorithm RSA \
        -KeyLength 2048 \
        -HashAlgorithm SHA256 \
        -KeyExportPolicy Exportable \
        -NotAfter ([DateTimeOffset]::UtcNow.AddHours(2).DateTime)
    $TlsRootStore = [System.Security.Cryptography.X509Certificates.X509Store]::new(
        'Root',
        [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
    )
    $TlsRootStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    $TlsRootStore.Add($TlsCertificate)
    $TlsRootStore.Close()
    $TlsRootStore = $null
    $TlsPassword = Convert-BytesToLowerHex ([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(18))
    $TlsSecurePassword = ConvertTo-SecureString -String $TlsPassword -AsPlainText -Force
    Export-PfxCertificate -Cert $TlsCertificate -FilePath $TlsPfx -Password $TlsSecurePassword | Out-Null

    $env:E2E_CONTROL_PORT = [string]$ControlPort
    $env:E2E_DEPENDENCY_PORT = [string]$DependencyPort
    $env:E2E_INGRESS_PORT = [string]$IngressPort
    $env:E2E_TLS_PFX = $TlsPfx
    $env:E2E_TLS_PFX_PASSWORD = $TlsPassword
    $env:E2E_TOKEN_FILE = $TokenFile
    $env:E2E_SIGNER_SCRIPT = $SignerScript
    $DependencyProcess = Start-Process \
        -FilePath (Get-Command node.exe).Source \
        -ArgumentList @($DependencyServer) \
        -PassThru \
        -NoNewWindow \
        -RedirectStandardOutput $DependencyStdout \
        -RedirectStandardError $DependencyStderr
    Wait-HttpReady -Uri "http://127.0.0.1:$DependencyPort/cdn-cgi/access/certs" -Process $DependencyProcess -Label 'local dependency server'
    if (-not (Test-Path $TokenFile -PathType Leaf)) { throw 'local Access token was not created' }

    $wranglerArguments = @(
        '--yes', 'wrangler@4.94.0', 'dev', '--local',
        '-c', $ControlConfig,
        '-c', $SignerConfig,
        '--persist-to', $State,
        '--ip', '127.0.0.1',
        '--port', [string]$ControlPort
    )
    $WranglerProcess = Start-Process \
        -FilePath (Get-Command npx.cmd).Source \
        -ArgumentList $wranglerArguments \
        -PassThru \
        -NoNewWindow \
        -RedirectStandardOutput $WranglerStdout \
        -RedirectStandardError $WranglerStderr
    Wait-HttpReady -Uri "http://127.0.0.1:$ControlPort/api/v1/health" -Process $WranglerProcess -Label 'local shipping control-plane Worker' -Attempts 120
    Wait-HttpReady -Uri "$Origin/__e2e/ready" -Process $DependencyProcess -Label 'local HTTPS Access edge'

    $hostOutput = & $HostBinary \
        enroll \
        --origin $Origin \
        --tenant-id $TenantId \
        --access-token-file $TokenFile \
        --correlation-id 'corr_e2e_host_01' \
        --idempotency-key 'idem_e2e_host_01'
    if ($LASTEXITCODE -ne 0) { throw 'shipping bridge-host-ops enroll failed' }
    $hostReceipt = ($hostOutput -join "`n") | ConvertFrom-Json
    if ([string]$hostReceipt.schemaVersion -ne 'bridge-host-ops/v1' -or
        [string]$hostReceipt.operation -ne 'enroll' -or
        [string]$hostReceipt.controlPlaneOrigin -ne $Origin -or
        [string]$hostReceipt.certificateStore -ne 'LocalMachine/My' -or
        [string]$hostReceipt.deviceId -notmatch '^[A-Za-z0-9_-]{8,96}$' -or
        [string]$hostReceipt.certificateSha1 -notmatch '^[0-9A-F]{40}$' -or
        [string]$hostReceipt.certificateSha256 -notmatch '^[0-9a-f]{64}$') {
        throw 'shipping enroll receipt is malformed'
    }
    $PositiveThumbprint = [string]$hostReceipt.certificateSha1
    $PositiveDeviceId = [string]$hostReceipt.deviceId
    $inspectOutput = & $HostBinary inspect --thumbprint $PositiveThumbprint
    if ($LASTEXITCODE -ne 0) { throw 'shipping certificate inspect failed after enroll' }
    $inspectReceipt = ($inspectOutput -join "`n") | ConvertFrom-Json
    if ([string]$inspectReceipt.certificateSha256 -ne [string]$hostReceipt.certificateSha256) {
        throw 'post-enroll local certificate identity drifted'
    }

    $issue = Issue-Claim -CorrelationId 'corr_e2e_negative_01' -IdempotencyKey 'idem_e2e_negative_01'
    $csrA = New-TestCsrHex
    $csrB = New-TestCsrHex
    $null = Invoke-WebRequest \
        -Uri "http://127.0.0.1:$DependencyPort/__e2e/fail-next-signer" \
        -Method Post \
        -SkipHttpErrorCheck
    $dependencyFailure = Redeem-Claim -Issue $issue -CsrHex $csrA -CorrelationId 'corr_e2e_negative_02' -ExpectedStatus @(503)
    Require-ProblemCode -Response $dependencyFailure -Code 'dependency_unavailable'
    $wrongCsr = Redeem-Claim -Issue $issue -CsrHex $csrB -CorrelationId 'corr_e2e_negative_03' -ExpectedStatus @(409)
    Require-ProblemCode -Response $wrongCsr -Code 'conflict'
    $recovery = Redeem-Claim -Issue $issue -CsrHex $csrA -CorrelationId 'corr_e2e_negative_04' -ExpectedStatus @(200)
    if ([string]$recovery.Document.deviceId -ne [string]$issue.deviceId -or [string]$recovery.Document.certificateSha256 -notmatch '^[0-9a-f]{64}$') {
        throw 'exact-CSR recovery response is malformed'
    }
    $replay = Redeem-Claim -Issue $issue -CsrHex $csrA -CorrelationId 'corr_e2e_negative_05' -ExpectedStatus @(409)
    Require-ProblemCode -Response $replay -Code 'replay_rejected'

    $expiryIssue = Issue-Claim -CorrelationId 'corr_e2e_expiry_01' -IdempotencyKey 'idem_e2e_expiry_01'
    $expiryCsr = New-TestCsrHex
    $currentMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $waitMs = ([int64]$expiryIssue.expiresAtMs + 1500) - $currentMs
    if ($waitMs -gt 0) {
        Start-Sleep -Milliseconds ([int][Math]::Min($waitMs, [int]::MaxValue))
    }
    $expired = Redeem-Claim -Issue $expiryIssue -CsrHex $expiryCsr -CorrelationId 'corr_e2e_expiry_02' -ExpectedStatus @(409)
    Require-ProblemCode -Response $expired -Code 'replay_rejected'

    $report = [ordered]@{
        schemaVersion = 1
        kind = 'BRIDGE_ENROLLMENT_HOSTED_E2E'
        status = 'PASS'
        sourceSha = $SourceSha
        runner = 'windows'
        mode = 'LOCAL_TEST_ONLY'
        providerMutation = $false
        productionMutation = $false
        remoteProviderCredentialsPresent = $false
        shippingControlPlaneWorker = $true
        shippingHostEnroll = $true
        localD1FromTypedRepositoryProjection = $true
        localAccessEdgeProjectionOnly = $true
        localProtectedSignerDependency = $true
        positive = [ordered]@{
            status = 'PASS'
            deviceId = $PositiveDeviceId
            certificateSha1 = $PositiveThumbprint
            certificateSha256 = [string]$hostReceipt.certificateSha256
            exactPostInstallInspect = $true
        }
        negative = [ordered]@{
            signerFailureReservationPersistence = 'PASS'
            wrongCsrConflict = 'PASS'
            exactCsrRecovery = 'PASS'
            consumedReplayRejected = 'PASS'
            expiryReplayRejected = 'PASS'
        }
    }
    $absoluteReport = if ([System.IO.Path]::IsPathRooted($ReportPath)) {
        $ReportPath
    } else {
        Join-Path $Root $ReportPath
    }
    Write-JsonFile $absoluteReport $report
    $EvidenceWritten = $true
    Write-Output ($report | ConvertTo-Json -Compress -Depth 20)
} finally {
    if ($null -ne $PositiveThumbprint -and (Test-Path "Cert:\LocalMachine\My\$PositiveThumbprint")) {
        Remove-Item -Path "Cert:\LocalMachine\My\$PositiveThumbprint" -DeleteKey -Confirm:$false -ErrorAction SilentlyContinue
    }
    if ($null -ne $PositiveDeviceId) {
        $keyName = "part-crm-bridge-$PositiveDeviceId"
        if ([System.Security.Cryptography.CngKey]::Exists($keyName, $Provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)) {
            $key = [System.Security.Cryptography.CngKey]::Open($keyName, $Provider, [System.Security.Cryptography.CngKeyOpenOptions]::MachineKey)
            try { $key.Delete() } finally { $key.Dispose() }
        }
    }
    Stop-ProcessTree $WranglerProcess
    Stop-ProcessTree $DependencyProcess
    if ($null -ne $TlsRootStore) {
        try { $TlsRootStore.Close() } catch {}
    }
    if ($null -ne $TlsCertificate) {
        try {
            $rootStore = [System.Security.Cryptography.X509Certificates.X509Store]::new(
                'Root',
                [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
            )
            $rootStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
            $matches = @($rootStore.Certificates | Where-Object { $_.Thumbprint -eq $TlsCertificate.Thumbprint })
            foreach ($match in $matches) { $rootStore.Remove($match) }
            $rootStore.Close()
        } catch {}
        Remove-Item -Path "Cert:\CurrentUser\My\$($TlsCertificate.Thumbprint)" -DeleteKey -Confirm:$false -ErrorAction SilentlyContinue
        $TlsCertificate.Dispose()
    }
    foreach ($name in @(
        'E2E_CONTROL_PORT', 'E2E_DEPENDENCY_PORT', 'E2E_INGRESS_PORT', 'E2E_TLS_PFX',
        'E2E_TLS_PFX_PASSWORD', 'E2E_TOKEN_FILE', 'E2E_SIGNER_SCRIPT'
    )) {
        [Environment]::SetEnvironmentVariable($name, $null)
    }
    if (Test-Path $Scratch) { Remove-Item -LiteralPath $Scratch -Recurse -Force -ErrorAction SilentlyContinue }
    if (-not $EvidenceWritten) {
        Write-Error 'Bridge enrollment hosted E2E did not produce PASS evidence'
    }
}
