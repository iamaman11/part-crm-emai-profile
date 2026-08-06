# Repository Step 6 — Windows Profile Bridge Feasibility Evidence

**Дата:** 2026-08-06  
**Статус:** accepted  
**Baseline:** `aac9f994cd79b5d6534f6ae9ec1669cdfeb8b73c`  
**Accepted source head:** `cceb7e97da980c905739eb02366019015f247d6e`  
**Pull request:** #21  
**Exact-head Quality Gate run:** `31058767330`  
**Squash merge:** `d0e2e0b1d11eb689b57f8ebaaefd591a6a7b6bab`

## 1. Реализованный Bridge Boundary

Step 6 introduces a provider-free Bridge domain and a Windows-native Rust
`profile-bridge` executable. The bounded slice owns custom-URI enrollment,
device-bound claim redemption, local workspace writer exclusion, process
lifecycle decisions, typed Camouhost IPC and rebuildable local command/outbox
state.

The executable remains separate from the Cloudflare Worker and CRM process.
Repository evidence proves feasibility and deterministic boundaries, not a
production installer or real browser runtime.

## 2. Custom URI And Enrollment

The accepted parser recognizes only:

`profilebridge://claim/<opaque-code>`

It rejects alternate case, scheme, host, path, additional segments, query,
fragment, percent encoding, backslash and traversal-like values. Claim tokens are
bounded opaque ASCII. They do not implement `Display`; their `Debug` output is
always redacted. CLI output returns only generic success or error text and never
echoes the URI or secret.

Enrollment claims have strict expiration and single-use semantics. First
redemption binds the claim to one typed device ID. Replay by the same device and
attempted rebinding to a different device are rejected deterministically.

## 3. Workspace And Process Lifecycle

The pure workspace state permits one active writer. A lease contains a monotonic
local epoch and opaque lock token. Repeating the same acquire is idempotent;
another writer is rejected. Release requires the exact device, epoch and token,
preventing a stale process from unlocking a newer writer.

The process supervisor distinguishes startup, ready, graceful closing, clean
close, crash and forced timeout. A successful OS exit is clean only after an
explicit graceful-close request. Exit while starting or ready is a crash even if
the exit code is successful. Start or close deadline expiry produces
`ForcedTimeout` and cannot be represented as a clean profile close.

A deterministic process-control fake records spawn, graceful-close and
force-terminate actions. A Windows-only safe adapter validates native wide-string
argument encoding without `unsafe` code.

## 4. Camouhost IPC

Step 6 defines bounded version-1 frames for hello/ack, launch/ready and
close/closed. Unsupported versions, invalid typed session IDs, extra fields,
newlines, NUL bytes and invalid booleans fail closed.

The deterministic fake Camouhost requires version negotiation before launch,
preserves the exact session ID through ready and close, rejects invalid ordering
and returns a typed close result.

## 5. Local SQLite And Outbox

Migration `migrations/bridge/0001_local_state.sql` defines rebuildable local
operational state:

- one versioned Bridge state row;
- append-only idempotent commands;
- strict command sequence and expected-version checks;
- immutable outbox event payloads with separately mutable delivery metadata.

A successful command, resulting state and outbox event commit in one SQLite
transaction. Exact duplicate commands do not grow storage. Conflicting key reuse,
stale expected versions and reordered sequences abort. Crash evidence persists a
`dirty` state. Command deletion and outbox payload mutation are rejected.

## 6. Permanent CI Result

Exact-head Quality Gate run `31058767330` succeeded on source head
`cceb7e97da980c905739eb02366019015f247d6e`.

### Rust Linux And WASM

- Step 6 architecture and source-hygiene policies passed;
- deliberate browser-runtime lock deletion fixture was rejected;
- deterministic local SQLite and outbox tests passed;
- rustfmt, Clippy with warnings denied and all native Bridge tests passed;
- `bridge-domain` compiled for `wasm32-unknown-unknown`, proving provider-free
  domain composition;
- all accepted Step 0–5 policy, D1, ACL, coordinator, contract and secret-scan
  gates remained green.

### Rust Windows And Profile Bridge Artifact

- all native workspace tests, including the Windows-only adapter test, passed;
- `cargo build --locked --release -p profile-bridge` succeeded;
- a non-empty `target/release/profile-bridge.exe` was verified on the pinned
  Windows runner.

### Existing Platform Regression Gates

- D1 migrations applied and replayed successfully;
- the Cloudflare Worker checked for WASM and produced verified release shim/Wasm
  artifacts.

## 7. Доказанные Свойства

The accepted repository evidence proves:

- malformed or secret-leaking custom URI shapes fail closed;
- claim tokens are redacted from normal formatting and CLI output;
- a claim is strict-expiry, single-use and device-bound;
- a second workspace writer cannot acquire while the first lease is active;
- stale device/epoch/token release cannot unlock a newer writer;
- unexpected exit, crash and forced timeout cannot become a clean close;
- versioned fake Camouhost launch/ready/close flow is deterministic;
- local command replay, stale version, sequence ordering and outbox immutability
  are enforced;
- new Bridge source cannot blindly delete known browser runtime lock files;
- the exact pinned Windows toolchain builds a non-empty release executable.

## 8. Defects Found And Corrected

- the first static browser-lock policy scanned an accepted legacy smoke utility
  outside the new Bridge boundary; the permanent policy was narrowed to Step 6
  Bridge sources while retaining the deliberate negative fixture;
- temporary formatting and lockfile workflows were removed after use and source
  hygiene rejects their return;
- deterministic fake-key tests were corrected to avoid transient borrowed values;
- Windows artifact verification was added as a distinct permanent job rather than
  inferring feasibility from cross-platform unit tests alone.

## 9. Ограничения И Внешние Gates

This evidence does not prove:

- Windows registry installation of the custom URI handler;
- production DPAPI, CNG, TPM, key rotation or recovery escrow;
- real Windows Job Object ownership, process-tree termination or crash behavior;
- trusted code signing, installer, auto-update, rollback or elevation;
- real Camouhost/Camoufox packaging or execution;
- migration or launch of the legacy profile corpus;
- encrypted R2 generations or production Cloudflare enrollment;
- physical multi-device behavior, production credentials, privacy readiness or
  production readiness.

No production credential, remote resource, real user profile, mailbox content or
personal data was used. All claim codes, devices, sessions and local state were
synthetic. `production_ready` remains `false`.
