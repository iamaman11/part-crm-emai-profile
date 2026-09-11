# Bridge host operations

`bridge-host-ops` is the single bounded Windows host-operations owner for Bridge device provisioning. It is not `profile-bridge.exe`, not a browser launcher, not a second device registry, not a CA, and not a Cloudflare/provider mutation client.

The accepted primary product flow is intentionally simple:

```text
Login
-> Connect this computer
-> local non-exportable Windows device key
-> authenticated one-shot CSR/certificate enrollment
-> certificate SHA-256 fingerprint binding
-> shipping mTLS Bridge
```

The user does not handle a CA, PFX, certificate thumbprint or provider payload on this primary path.

## Primary enrollment boundary

The target Windows host owns the device private key. The key is created locally as non-exportable, is referenced through its platform handle, and is never transported to the control plane, GitHub, an Issue, an artifact or a provider payload.

The intended shipping enrollment operation composes these already accepted responsibilities:

1. create one local non-exportable per-device Windows key;
2. create a CSR for that exact local key;
3. redeem one authenticated, one-shot enrollment authorization;
4. accept only the public Client Authentication certificate/chain issued for that CSR;
5. attach/install the certificate to the existing local private key in `Cert:\LocalMachine\My`;
6. prove that the installed certificate has the local private key, explicit Client Authentication EKU (`1.3.6.1.5.5.7.3.2`) and current validity;
7. derive the Windows SHA-1 selector and canonical lowercase SHA-256 certificate fingerprint;
8. use the existing governed device-binding owner to bind the user/device/fingerprint;
9. emit only secret-free shipping/reconciliation metadata.

There is no second device registry. The existing D1 device-binding owner remains authoritative for active device authorization; mTLS is machine transport authentication only.

The final shipping `enroll` command is **not yet exposed** by this source snapshot because the authenticated one-shot server enrollment/issuer path is still being implemented. The repository must not fake that missing server owner by silently using the recovery PFX path as product enrollment.

## Security boundary

`bridge-host-ops` never generates or stores a CA signing key and never mutates Cloudflare policy, an Access application, an mTLS rule, Worker bindings or D1 trust rows directly.

Primary enrollment rules:

- private key is generated on the target Windows host;
- private key export/transport is forbidden;
- CSR/public certificate material is non-secret;
- enrollment authorization is bounded and one-shot;
- certificate issuance belongs to the protected issuer boundary;
- the existing device-binding API remains the only trust-state mutation path;
- provider reconciliation remains outside this tool.

## Admin / recovery commands

The existing PFX path is retained only as an optional admin/recovery fallback. It is **not** the B7 primary product path and must not be presented to ordinary users as `Connect this computer`.

Inspect one already installed certificate:

```powershell
bridge-host-ops inspect --thumbprint <40-hex-sha1>
```

Recovery import of one externally supplied password-protected PFX into `LocalMachine/My`:

```powershell
bridge-host-ops import `
  --pfx C:\secure\bridge-device.pfx `
  --password-file C:\secure\bridge-device.password
```

A recovery PFX that does not yield exactly one certificate with a private key is rejected. Import omits `-Exportable`; newly imported material is rolled back on import/validation failure, while a certificate that pre-existed the operation is never deleted by rollback. The operator is responsible for protecting and deleting temporary recovery PFX/password files.

Every API mutation requires both `--correlation-id` and a separate `--idempotency-key`. Both use the accepted 8–96 character opaque-ID wire form (`A-Z`, `a-z`, `0-9`, `_`, `-`).

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

The binding API remains exactly the accepted resource:

- `PUT /api/v1/tenants/{tenantId}/members/{actorId}/device-binding`
- `DELETE /api/v1/tenants/{tenantId}/members/{actorId}/device-binding`

The tool never writes D1 directly.

## Cloudflare Access user identity

Do not supply `Cf-Access-Jwt-Assertion` directly. That is the origin-facing assertion verified by the Worker. External user authentication goes through the normal Cloudflare Access login flow.

The currently retained admin/recovery commands accept a user-scoped Access token only through `--access-token-file`; the value is not accepted as a command-line value, printed in JSON output or forwarded in process arguments. It is written only to `curl.exe` configuration through stdin, and owned in-memory buffers are overwritten after handoff.

This user credential is not the Bridge machine credential and is not a Cloudflare API/provider mutation token.

## Failure and replay semantics

Every operation fails closed and does not internally retry network or mutation operations.

For `rebind` and `revoke`, the authoritative server mutation happens before old local certificate/private-key deletion. If the server mutation succeeds but local cleanup fails, the process exits nonzero with `local_cleanup_required_after_server_commit` and writes one secret-free recovery receipt.

Recovery is an **exact replay**, not a new mutation: rerun the same command with the same route/payload, `--correlation-id` and `--idempotency-key`. Do not invent a new idempotency key after a server-commit/local-cleanup split.

Provider reconciliation remains outside this tool. Cloudflare Access/mTLS state may be observed by existing external-evidence owners, but this tool does not mutate provider state.

## Output

Successful host operations emit only non-secret evidence/shipping metadata, including when applicable:

- operation and schema version;
- `DeviceId` / target `ActorId`;
- `LocalMachine/My`;
- certificate SHA-1 selector and canonical lowercase SHA-256 fingerprint derived from the same installed certificate;
- authoritative binding result/version;
- shipping values such as `PROFILE_BRIDGE_DEVICE_ID`, `PROFILE_BRIDGE_MACHINE_CERT_SHA1` and `PROFILE_BRIDGE_CONTROL_PLANE_ORIGIN`.

Certificate bytes may be transported only where a certificate-enrollment/install operation explicitly requires the public certificate; private key material, PFX bytes/passwords, Access tokens, provider credentials, cookies and direct D1 material are never emitted as ordinary evidence.

## Development verification

The crate remains deliberately dependency-free and has its own lockfile:

```text
cargo fmt --manifest-path tools/bridge-host-ops/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path tools/bridge-host-ops/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path tools/bridge-host-ops/Cargo.toml
cargo run --locked --manifest-path tools/bridge-host-ops/Cargo.toml -- self-test
```

The dedicated `Bridge Host Operations Gate` is thin orchestration only. Linux proves pure policy/serialization and secret boundaries. `windows-latest` additionally executes the real Windows adapter and the local enrollment-crypto proof against Windows CNG and `Cert:\LocalMachine\My`:

```text
create machine key with export policy NONE
-> require private-key export attempt to fail
-> create CSR from that key
-> attach a CI-only public ClientAuth certificate to the same key
-> install/inspect it through LocalMachine/My
-> clean up certificate + key
```

The CI-only certificate used by that proof is deliberately ephemeral test material. It is not a runtime issuer, staging CA, provider trust root or substitute for B7. Hosted Windows proves application/OS crypto mechanics; physical TPM/hardware-backed protection and real staging Access/mTLS admission remain separately scoped evidence where required.
