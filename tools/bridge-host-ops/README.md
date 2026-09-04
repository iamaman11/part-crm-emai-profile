# Bridge host operations

`bridge-host-ops` is the single bounded Windows host-operations owner for CAP-EXEC V2 TX-2 device provisioning. It is not `profile-bridge.exe`, not a browser launcher, not a second device registry, not a CA, and not a Cloudflare/provider mutation client.

Its job is deliberately narrow:

1. import or inspect an externally issued client certificate in `Cert:\LocalMachine\My`;
2. prove that the same installed certificate has a private key, an explicit Client Authentication EKU (`1.3.6.1.5.5.7.3.2`), and is currently valid;
3. derive the Windows SHA-1 selector and canonical lowercase SHA-256 fingerprint from that one certificate object;
4. call the accepted owner-authenticated device-binding API from #590;
5. emit only secret-free reconciliation metadata and the three existing shipping selectors consumed by `profile-bridge.exe`;
6. remove the superseded/revoked local certificate and associated private key only after the authoritative server mutation succeeds.

## Security boundary

The tool never generates a CA, certificate, private key, PFX, Cloudflare policy, Access application, mTLS rule, Worker binding, or D1 trust row directly.

Secret material is accepted only through explicit files:

- `--pfx` points to an externally issued PFX supplied by the certificate/operations owner;
- `--password-file` contains the PFX password;
- `--access-token-file` contains a user-scoped Cloudflare Access application token obtained through the normal Access login flow.

Secret values are never accepted as command-line values, never included in JSON output, and never forwarded in process arguments. The PFX password is written only to the child PowerShell stdin and the Access token is written only to the child `curl.exe` config stdin. In-memory byte buffers are overwritten after handoff.

The PFX import omits `-Exportable`, so Windows imports the private key as non-exportable. Local certificate removal uses the Certificate provider `-DeleteKey` operation so the associated private key is removed with the certificate.

The operator remains responsible for protecting and deleting the temporary PFX/password/token files after the ceremony. Run LocalMachine certificate mutations from an elevated Windows session.

## Cloudflare Access user identity

Do not supply `Cf-Access-Jwt-Assertion` directly. That is the origin-facing assertion verified by the Worker. The external user credential must go through Cloudflare Access.

Use the current Cloudflare CLI user flow, for example:

```powershell
cloudflared access login https://control.example.test
cloudflared access token -app=https://control.example.test |
  Set-Content -NoNewline -Encoding ascii $env:TEMP\bridge-host-access.token
```

The tool sends that value to the Access edge as `cf-access-token` through `curl.exe` stdin. Access remains responsible for authenticating the user and injecting the `Cf-Access-Jwt-Assertion` that the accepted Worker validates against `ACCESS_AUDIENCE`.

This is a human/operator governance credential, not the Bridge machine credential and not a Cloudflare API/provider mutation token.

## Commands

Inspect one already installed certificate:

```powershell
bridge-host-ops inspect --thumbprint <40-hex-sha1>
```

Import one externally issued password-protected PFX into `LocalMachine/My`:

```powershell
bridge-host-ops import `
  --pfx C:\secure\bridge-device.pfx `
  --password-file C:\secure\bridge-device.password
```

A PFX that does not yield exactly one certificate with a private key is rejected. Newly imported material is rolled back on import/validation failure; a certificate that already existed before the operation is never deleted by rollback.

Every API mutation requires both `--correlation-id` and a separate `--idempotency-key`. Both use the accepted 8–96 character opaque-ID wire form (`A-Z`, `a-z`, `0-9`, `_`, `-`). The idempotency key is non-secret and is sent as the exact `Idempotency-Key` header required by #590.

Initial authoritative bind:

```powershell
bridge-host-ops bind `
  --origin https://control.example.test `
  --tenant-id tenant_... `
  --actor-id actor_... `
  --device-id device_... `
  --thumbprint <new-sha1> `
  --access-token-file C:\secure\bridge-host-access.token `
  --correlation-id corr_... `
  --idempotency-key idem_...
```

An optional `--expected-previous-version <n>` may be supplied when the caller has an explicit existing version expectation.

Atomic authoritative rebind followed by old local certificate cleanup:

```powershell
bridge-host-ops rebind `
  --origin https://control.example.test `
  --tenant-id tenant_... `
  --actor-id actor_... `
  --device-id device_... `
  --thumbprint <new-sha1> `
  --old-thumbprint <old-sha1> `
  --expected-previous-version <n> `
  --access-token-file C:\secure\bridge-host-access.token `
  --correlation-id corr_... `
  --idempotency-key idem_...
```

The old and new SHA-1 selectors are validated as distinct before any server mutation.

Authoritative revoke followed by local certificate/private-key cleanup:

```powershell
bridge-host-ops revoke `
  --origin https://control.example.test `
  --tenant-id tenant_... `
  --actor-id actor_... `
  --thumbprint <current-sha1> `
  --expected-version <n> `
  --access-token-file C:\secure\bridge-host-access.token `
  --correlation-id corr_... `
  --idempotency-key idem_...
```

The binding API is exactly the accepted #590 resource:

- `PUT /api/v1/tenants/{tenantId}/members/{actorId}/device-binding`
- `DELETE /api/v1/tenants/{tenantId}/members/{actorId}/device-binding`

The tool never writes D1 directly.

## Failure and replay semantics

Every operation fails closed. It does not retry network or mutation operations internally.

For `rebind` and `revoke`, the authoritative server mutation happens before local certificate/private-key deletion. If the server mutation succeeds but local cleanup fails, the process exits nonzero with `local_cleanup_required_after_server_commit` and writes one secret-free recovery receipt to stdout. That receipt includes the accepted aggregate version plus the correlation/idempotency identifiers and explicitly marks the local certificate as still present.

Recovery is an **exact replay**, not a new mutation: rerun the same command with the same route/payload, `--correlation-id`, and especially the same `--idempotency-key`. The accepted #590 command-evidence owner can then replay the already committed mutation while the host tool retries the idempotent local cleanup. Do not invent a new idempotency key after a server-commit/local-cleanup split.

For `bind`, no local cleanup follows the server mutation.

Provider reconciliation remains outside this tool. Cloudflare Access/mTLS state may be observed by the existing external-evidence owners, but this source transaction does not mutate provider state.

## Output

Successful commands write one compact JSON object containing only non-secret evidence such as:

- operation and schema version;
- `DeviceId` / target `ActorId` when applicable;
- `LocalMachine/My`;
- certificate SHA-1 selector and lowercase SHA-256 fingerprint derived from the same certificate;
- authoritative binding result/version;
- existing shipping values for `PROFILE_BRIDGE_DEVICE_ID`, `PROFILE_BRIDGE_MACHINE_CERT_SHA1`, and `PROFILE_BRIDGE_CONTROL_PLANE_ORIGIN`.

A post-server cleanup failure writes the same class of secret-free reconciliation evidence before the nonzero exit so recovery does not lose the authoritative version. No certificate bytes, PFX bytes, private key, password, Access token, provider credential, cookie, or direct D1 material is emitted.

## Development verification

The crate is deliberately dependency-free and has its own lockfile. Run:

```text
cargo fmt --manifest-path tools/bridge-host-ops/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path tools/bridge-host-ops/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path tools/bridge-host-ops/Cargo.toml
cargo run --locked --manifest-path tools/bridge-host-ops/Cargo.toml -- self-test
```

The dedicated `Bridge Host Operations Gate` is thin orchestration only: Linux proves the pure policy/serialization and secret boundary; Windows additionally compiles/tests the real `LocalMachine/My`, PowerShell and `curl.exe` adapter. It owns no certificate issuance, provider mutation or trust state. Real certificate/provider/effect-path evidence remains candidate-bound B7/B8 work and is not fabricated by unit tests.
