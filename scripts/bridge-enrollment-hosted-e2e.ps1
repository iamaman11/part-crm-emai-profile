param(
    [string]$ReportPath = 'artifacts/bridge-enrollment-e2e/evidence.json'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version Latest

$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$SourceSha = (& git -C $Root rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $SourceSha -notmatch '^[0-9a-f]{40}$') { throw 'unable to resolve exact source SHA' }
foreach ($name in @(
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_OBSERVE_API_TOKEN',
    'CLOUDFLARE_ZERO_TRUST_OBSERVE_API_TOKEN',
    'CLOUDFLARE_ACCOUNT_ID',
    'CLOUDFLARE_E2E_TUNNEL_TOKEN',
    'TUNNEL_TOKEN'
)) {
    if (-not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($name))) {
        throw "$name must not be exposed to the Bridge enrollment E2E harness"
    }
}

$ControlPort = 18787
$DependencyPort = 19080
$IngressPort = 19443
$TenantId = 'tenant_e2e_01'
$ActorId = 'actor_e2e_01'
$IdentityId = 'identity_e2e_01'
$AccessSubject = 'bridge_e2e_subject_01'
$TunnelHostname = 'bridge-e2e.alegria.by'
$Origin = "https://$TunnelHostname"
$Scratch = Join-Path $env:RUNNER_TEMP "bridge-enrollment-e2e-$PID"
$State = Join-Path $Scratch 'state'
$MigrationDir = Join-Path $Scratch 'migrations'
$ControlConfig = Join-Path $Scratch 'control.wrangler.jsonc'
$SignerConfig = Join-Path $Scratch 'signer.wrangler.jsonc'
$TokenFile = Join-Path $Scratch 'access-token.txt'
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
$PositiveThumbprint = $null
$PositiveDeviceId = $null

function Write-Phase([string]$Name) {
    Write-Host "bridge-enrollment-e2e phase=$Name"
}

function Convert-BytesToLowerHex([byte[]]$Bytes) {
    ([Convert]::ToHexString($Bytes)).ToLowerInvariant()
}

function Write-JsonFile([string]$Path, [object]$Value) {
    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrEmpty($parent)) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
    (($Value | ConvertTo-Json -Depth 20) + "`n") | Set-Content -Path $Path -Encoding utf8NoBOM
}

function Stop-ProcessTree([System.Diagnostics.Process]$Process, [string]$Label) {
    if ($null -eq $Process -or $Process.HasExited) { return }
    try { $Process.Kill($true) } catch {}
    try {
        if (-not $Process.WaitForExit(10000)) {
            Write-Warning "$Label cleanup did not exit within 10 seconds"
        }
    } catch {}
}

function Invoke-BoundedExecutable(
    [string]$FilePath,
    [string[]]$Arguments,
    [string]$Label,
    [int]$TimeoutSeconds = 60
) {
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.CreateNoWindow = $true
    foreach ($argument in $Arguments) { $startInfo.ArgumentList.Add($argument) }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) { throw "$Label failed to start" }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            $activeEffect = 'none'
            $powershellStage = ''
            try {
                $effects = @(
                    Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId = $($process.Id)" -ErrorAction Stop |
                        Where-Object { [string]$_.Name -in @('curl.exe', 'powershell.exe') }
                )
                $effectNames = @($effects | ForEach-Object { [string]$_.Name } | Sort-Object -Unique)
                if ($effectNames.Count -gt 0) { $activeEffect = $effectNames -join ',' }
                $powershellEffect = @($effects | Where-Object { [string]$_.Name -eq 'powershell.exe' } | Select-Object -First 1)
                if ($powershellEffect.Count -eq 1) {
                    $commandLine = [string]$powershellEffect[0].CommandLine
                    if ($commandLine -like '*CreateSigningRequest*') {
                        $powershellStage = 'csr-create'
                    } elseif ($commandLine -like '*CopyWithPrivateKey*') {
                        $powershellStage = 'certificate-install'
                    } elseif ($commandLine -like "*Write-Output 'removed'*") {
                        $powershellStage = 'key-cleanup'
                    } else {
                        $powershellStage = 'unknown'
                    }
                }
            } catch {
                $activeEffect = 'observation-unavailable'
                $powershellStage = ''
            }
            try { $process.Kill($true) } catch {}
            $stageDetail = if ([string]::IsNullOrEmpty($powershellStage)) { '' } else { "; powershell-stage=$powershellStage" }
            throw "$Label timed out after $TimeoutSeconds seconds; active-effect=$activeEffect$stageDetail"
        }
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) {
            $detail = if ([string]::IsNullOrWhiteSpace($stderr)) { '' } else { ": $($stderr.Trim())" }
            throw "$Label failed with exit $($process.ExitCode)$detail"
        }
        @($stdout -split "`r?`n" | Where-Object { -not [string]::IsNullOrEmpty($_) })
    } finally {
        $process.Dispose()
    }
}

function Invoke-Wrangler([string[]]$Arguments, [string]$Label, [int]$TimeoutSeconds = 75) {
    $npx = (Get-Command npx.cmd).Source
    $npxArguments = @('--yes', 'wrangler@4.129.1') + $Arguments
    $null = Invoke-BoundedExecutable -FilePath $npx -Arguments $npxArguments -Label $Label -TimeoutSeconds $TimeoutSeconds
}

function Wait-HttpReady(
    [string]$Uri,
    [System.Diagnostics.Process]$Process,
    [string]$Label,
    [int]$TimeoutSeconds = 30
) {
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    while ($timer.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        if ($null -ne $Process -and $Process.HasExited) { throw "$Label exited before readiness" }
        try {
            $response = Invoke-WebRequest -Uri $Uri -Method Get -TimeoutSec 2 -SkipHttpErrorCheck
            if ([int]$response.StatusCode -eq 200) { return }
        } catch {}
        Start-Sleep -Milliseconds 500
    }
    throw "$Label did not become ready within $TimeoutSeconds seconds"
}

function New-TestCsrHex {
    $rsa = [System.Security.Cryptography.RSA]::Create(2048)
    try {
        $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
            'CN=part-crm-bridge-e2e-negative',
            $rsa,
            [System.Security.Cryptography.HashAlgorithmName]::SHA256,
            [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
        )
        $oids = [System.Security.Cryptography.OidCollection]::new()
        $null = $oids.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.2'))
        $request.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($oids, $false))
        Convert-BytesToLowerHex $request.CreateSigningRequest()
    } finally {
        $rsa.Dispose()
    }
}

function Invoke-EnrollmentHttp(
    [string]$Path,
    [string]$Body,
    [string]$CorrelationId,
    [int[]]$ExpectedStatus,
    [string]$IdempotencyKey = ''
) {
    $headers = @{
        accept = 'application/json'
        'cf-access-token' = (Get-Content -Raw -Path $TokenFile).Trim()
        'X-Correlation-Id' = $CorrelationId
    }
    if (-not [string]::IsNullOrEmpty($IdempotencyKey)) { $headers['Idempotency-Key'] = $IdempotencyKey }
    $request = @{
        Uri = "$Origin$Path"
        Method = 'Post'
        Headers = $headers
        ContentType = 'application/json'
        Body = $Body
        SkipHttpErrorCheck = $true
        TimeoutSec = 15
    }
    $response = Invoke-WebRequest @request
    $status = [int]$response.StatusCode
    if ($ExpectedStatus -notcontains $status) { throw "unexpected enrollment HTTP status $status for $Path" }
    $document = $null
    if (-not [string]::IsNullOrWhiteSpace($response.Content)) { $document = $response.Content | ConvertFrom-Json }
    [pscustomobject]@{ Status = $status; Document = $document }
}

function Issue-Claim([string]$CorrelationId, [string]$IdempotencyKey) {
    $arguments = @{
        Path = "/api/v1/tenants/$TenantId/bridge-enrollment/authorities"
        Body = '{}'
        CorrelationId = $CorrelationId
        ExpectedStatus = @(201)
        IdempotencyKey = $IdempotencyKey
    }
    $response = Invoke-EnrollmentHttp @arguments
    if ($response.Document.claimCode -notmatch '^[0-9a-f]{64}$' -or $response.Document.deviceId -notmatch '^[A-Za-z0-9_-]{8,96}$' -or [uint64]$response.Document.expiresAtMs -eq 0) {
        throw 'issue response identity is malformed'
    }
    $response.Document
}

function Redeem-Claim([object]$Issue, [string]$CsrHex, [string]$CorrelationId, [int[]]$ExpectedStatus) {
    $body = [ordered]@{ claimCode = [string]$Issue.claimCode; csrDerHex = $CsrHex } | ConvertTo-Json -Compress
    $arguments = @{
        Path = "/api/v1/tenants/$TenantId/bridge-enrollment/redemptions"
        Body = $body
        CorrelationId = $CorrelationId
        ExpectedStatus = $ExpectedStatus
    }
    Invoke-EnrollmentHttp @arguments
}

function Require-ProblemCode([object]$Response, [string]$Code) {
    if ($null -eq $Response.Document -or [string]$Response.Document.code -ne $Code) { throw "expected problem code $Code" }
}

New-Item -ItemType Directory -Path $Scratch, $State, $MigrationDir -Force | Out-Null

try {
    if (-not (Test-Path $HostBinary)) { throw 'release bridge-host-ops binary is missing' }
    if (-not (Test-Path $WorkerShim)) { throw 'control-plane Worker build output is missing' }

    Write-Phase 'typed-d1-projection'
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
        if ([string]::IsNullOrWhiteSpace($name) -or [string]::IsNullOrWhiteSpace($sourceRoot) -or -not $seenMigrations.Add($name)) { throw 'typed Catalog migration projection is malformed' }
        $source = [System.IO.Path]::GetFullPath((Join-Path (Join-Path $Root $sourceRoot) $name))
        if (-not $source.StartsWith($Root, [System.StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $source -PathType Leaf)) { throw 'typed Catalog migration source escaped the repository or is missing' }
        $sourceItem = Get-Item -LiteralPath $source
        if (-not [string]::IsNullOrEmpty([string]$sourceItem.LinkType)) { throw 'typed Catalog migration source must be a regular file' }
        Copy-Item -LiteralPath $source -Destination (Join-Path $MigrationDir $name)
    }

    $shippingConfig = Get-Content -Raw -Path (Join-Path $Root 'deploy/cloudflare/wrangler.jsonc') | ConvertFrom-Json
    $profileId = [string]$shippingConfig.env.staging.vars.CAPABILITY_PROFILE_ID
    $profileDigest = [string]$shippingConfig.env.staging.vars.CAPABILITY_PROFILE_DIGEST
    if ($profileId -ne 'rehearsal-core-v2' -or $profileDigest -notmatch '^[0-9a-f]{64}$') { throw 'shipping V2 staging capability projection drifted' }
    $sourceIdentity = [System.Text.Encoding]::UTF8.GetBytes("bridge-enrollment-e2e`n$SourceSha")
    $localReleaseDigest = Convert-BytesToLowerHex ([System.Security.Cryptography.SHA256]::HashData($sourceIdentity))
    $targetObservation = "target-v1|staging|$profileId|$profileDigest|release-set-v3-sha256-$localReleaseDigest"
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
        services = @([ordered]@{ binding = 'BRIDGE_CERTIFICATE_SIGNER'; service = 'bridge-enrollment-e2e-signer' })
    })
    Write-JsonFile $SignerConfig ([ordered]@{
        name = 'bridge-enrollment-e2e-signer'
        main = $signerMain
        compatibility_date = '2026-08-05'
        vars = [ordered]@{ TEST_SIGNER_ORIGIN = "http://127.0.0.1:$DependencyPort" }
    })

    Write-Phase 'local-d1-migrations'
    Invoke-Wrangler -Arguments @('d1', 'migrations', 'apply', 'CATALOG_DB', '--local', '--config', $ControlConfig, '--persist-to', $State, '--experimental-provision=false', '--experimental-auto-create=false') -Label 'local D1 migrations apply' -TimeoutSeconds 75
    Write-Phase 'local-d1-migrations-pass'
    $nowMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $seedSql = "INSERT INTO tenants (tenant_id, display_name, status, version, created_at_ms, updated_at_ms) VALUES ('$TenantId', 'Bridge Enrollment E2E', 'ACTIVE', 1, $nowMs, $nowMs); INSERT INTO identities (identity_id, access_subject, verified_contact_hint, created_at_ms) VALUES ('$IdentityId', '$AccessSubject', 'bridge-e2e@example.test', $nowMs); INSERT INTO memberships (tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms) VALUES ('$TenantId', '$ActorId', '$IdentityId', 'TENANT_OWNER', 'ACTIVE', 1, $nowMs, $nowMs);"
    Write-Phase 'local-d1-seed'
    Invoke-Wrangler -Arguments @('d1', 'execute', 'CATALOG_DB', '--local', '--config', $ControlConfig, '--persist-to', $State, '--command', $seedSql, '--experimental-provision=false', '--experimental-auto-create=false') -Label 'local D1 seed execute' -TimeoutSeconds 30
    Write-Phase 'local-d1-seed-pass'

    $env:E2E_CONTROL_PORT = [string]$ControlPort
    $env:E2E_DEPENDENCY_PORT = [string]$DependencyPort
    $env:E2E_INGRESS_PORT = [string]$IngressPort
    $env:E2E_TOKEN_FILE = $TokenFile
    $env:E2E_SIGNER_SCRIPT = $SignerScript
    $dependencyStart = @{
        FilePath = (Get-Command node.exe).Source
        ArgumentList = @($DependencyServer)
        PassThru = $true
        NoNewWindow = $true
        RedirectStandardOutput = $DependencyStdout
        RedirectStandardError = $DependencyStderr
    }
    Write-Phase 'dependency-start'
    $DependencyProcess = Start-Process @dependencyStart
    Wait-HttpReady -Uri "http://127.0.0.1:$DependencyPort/cdn-cgi/access/certs" -Process $DependencyProcess -Label 'local dependency server' -TimeoutSeconds 20
    Write-Phase 'dependency-ready'
    Wait-HttpReady -Uri "http://127.0.0.1:$IngressPort/__e2e/ready" -Process $DependencyProcess -Label 'local HTTP tunnel origin' -TimeoutSeconds 20
    Write-Phase 'local-ingress-ready'
    if (-not (Test-Path $TokenFile -PathType Leaf)) { throw 'local Access token was not created' }

    $wranglerStart = @{
        FilePath = (Get-Command npx.cmd).Source
        ArgumentList = @('--yes', 'wrangler@4.129.1', 'dev', '--local', '-c', $ControlConfig, '-c', $SignerConfig, '--persist-to', $State, '--ip', '127.0.0.1', '--port', [string]$ControlPort)
        PassThru = $true
        NoNewWindow = $true
        RedirectStandardOutput = $WranglerStdout
        RedirectStandardError = $WranglerStderr
    }
    Write-Phase 'wrangler-start'
    $WranglerProcess = Start-Process @wranglerStart
    Wait-HttpReady -Uri "http://127.0.0.1:$ControlPort/api/v1/health" -Process $WranglerProcess -Label 'local shipping control-plane Worker' -TimeoutSeconds 45
    Write-Phase 'wrangler-ready'
    Wait-HttpReady -Uri "$Origin/__e2e/ready" -Process $null -Label 'Cloudflare Tunnel public ingress' -TimeoutSeconds 45
    Write-Phase 'cloudflare-tunnel-ready'
    Write-Phase 'local-services-ready'

    Write-Phase 'positive-host-enroll'
    $hostArgs = @('enroll', '--origin', $Origin, '--tenant-id', $TenantId, '--access-token-file', $TokenFile, '--correlation-id', 'corr_e2e_host_01', '--idempotency-key', 'idem_e2e_host_01')
    $hostOutput = Invoke-BoundedExecutable -FilePath $HostBinary -Arguments $hostArgs -Label 'shipping bridge-host-ops enroll' -TimeoutSeconds 45
    $hostReceipt = ($hostOutput -join "`n") | ConvertFrom-Json
    if ([string]$hostReceipt.schemaVersion -ne 'bridge-host-ops/v1' -or [string]$hostReceipt.operation -ne 'enroll' -or [string]$hostReceipt.controlPlaneOrigin -ne $Origin -or [string]$hostReceipt.certificateStore -ne 'LocalMachine/My' -or [string]$hostReceipt.deviceId -notmatch '^[A-Za-z0-9_-]{8,96}$' -or [string]$hostReceipt.certificateSha1 -notmatch '^[0-9A-F]{40}$' -or [string]$hostReceipt.certificateSha256 -notmatch '^[0-9a-f]{64}$') {
        throw 'shipping enroll receipt is malformed'
    }
    $PositiveThumbprint = [string]$hostReceipt.certificateSha1
    $PositiveDeviceId = [string]$hostReceipt.deviceId
    $inspectOutput = Invoke-BoundedExecutable -FilePath $HostBinary -Arguments @('inspect', '--thumbprint', $PositiveThumbprint) -Label 'shipping certificate inspect' -TimeoutSeconds 20
    $inspectReceipt = ($inspectOutput -join "`n") | ConvertFrom-Json
    if ([string]$inspectReceipt.certificateSha256 -ne [string]$hostReceipt.certificateSha256) { throw 'post-enroll local certificate identity drifted' }
    Write-Phase 'positive-host-enroll-pass'

    Write-Phase 'negative-reservation-replay'
    $issue = Issue-Claim -CorrelationId 'corr_e2e_negative_01' -IdempotencyKey 'idem_e2e_negative_01'
    $csrA = New-TestCsrHex
    $csrB = New-TestCsrHex
    $null = Invoke-WebRequest -Uri "http://127.0.0.1:$DependencyPort/__e2e/fail-next-signer" -Method Post -TimeoutSec 10 -SkipHttpErrorCheck
    $dependencyFailure = Redeem-Claim -Issue $issue -CsrHex $csrA -CorrelationId 'corr_e2e_negative_02' -ExpectedStatus @(503)
    Require-ProblemCode -Response $dependencyFailure -Code 'dependency_unavailable'
    $wrongCsr = Redeem-Claim -Issue $issue -CsrHex $csrB -CorrelationId 'corr_e2e_negative_03' -ExpectedStatus @(409)
    Require-ProblemCode -Response $wrongCsr -Code 'conflict'
    $recovery = Redeem-Claim -Issue $issue -CsrHex $csrA -CorrelationId 'corr_e2e_negative_04' -ExpectedStatus @(200)
    if ([string]$recovery.Document.deviceId -ne [string]$issue.deviceId -or [string]$recovery.Document.certificateSha256 -notmatch '^[0-9a-f]{64}$') { throw 'exact-CSR recovery response is malformed' }
    $replay = Redeem-Claim -Issue $issue -CsrHex $csrA -CorrelationId 'corr_e2e_negative_05' -ExpectedStatus @(409)
    Require-ProblemCode -Response $replay -Code 'replay_rejected'
    Write-Phase 'negative-reservation-replay-pass'

    $expiryIssue = Issue-Claim -CorrelationId 'corr_e2e_expiry_01' -IdempotencyKey 'idem_e2e_expiry_01'
    $expiryCsr = New-TestCsrHex
    $waitMs = ([int64]$expiryIssue.expiresAtMs + 1500) - [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    Write-Phase "real-expiry-wait-ms-$([Math]::Max(0, $waitMs))"
    if ($waitMs -gt 0) { Start-Sleep -Milliseconds ([int][Math]::Min($waitMs, [int]::MaxValue)) }
    $expired = Redeem-Claim -Issue $expiryIssue -CsrHex $expiryCsr -CorrelationId 'corr_e2e_expiry_02' -ExpectedStatus @(409)
    Require-ProblemCode -Response $expired -Code 'replay_rejected'
    Write-Phase 'real-expiry-pass'

    $report = [ordered]@{
        schemaVersion = 1
        kind = 'BRIDGE_ENROLLMENT_HOSTED_E2E'
        status = 'PASS'
        sourceSha = $SourceSha
        runner = 'windows'
        mode = 'HOSTED_CLOUDFLARE_TUNNEL_TEST_ONLY'
        providerMutation = $false
        productionMutation = $false
        remoteProviderCredentialsPresent = $true
        remoteProviderCredentialUsedByHarness = $false
        remoteProviderCredentialUsedByWorkflowConnector = $true
        cloudflareTunnelConnectorOnly = $true
        cloudflareManagedPublicTls = $true
        cloudflareTunnelHostname = $TunnelHostname
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
    $absoluteReport = if ([System.IO.Path]::IsPathRooted($ReportPath)) { $ReportPath } else { Join-Path $Root $ReportPath }
    Write-JsonFile $absoluteReport $report
    Write-Phase 'evidence-pass'
    Write-Output ($report | ConvertTo-Json -Compress -Depth 20)
} finally {
    Write-Phase 'cleanup-start'
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
    Stop-ProcessTree -Process $WranglerProcess -Label 'Wrangler process tree'
    Stop-ProcessTree -Process $DependencyProcess -Label 'dependency process tree'
    foreach ($name in @('E2E_CONTROL_PORT', 'E2E_DEPENDENCY_PORT', 'E2E_INGRESS_PORT', 'E2E_TOKEN_FILE', 'E2E_SIGNER_SCRIPT')) {
        [Environment]::SetEnvironmentVariable($name, $null)
    }
    if (Test-Path $Scratch) { Remove-Item -LiteralPath $Scratch -Recurse -Force -ErrorAction SilentlyContinue }
    Write-Phase 'cleanup-pass'
}
