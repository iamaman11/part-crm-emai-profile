# Windows Profile Bridge Feasibility

**Status:** Repository Step 6 implementation specification  
**Production readiness:** false

## Responsibility

The Profile Bridge is a Windows-native Rust process that owns local enrollment,
workspace writer exclusion, browser-runtime process supervision and typed IPC to
a separately packaged Camouhost runtime. It does not embed browser lifecycle in
the Cloudflare Worker or CRM process.

Repository Step 6 proves bounded architecture and deterministic local behavior.
It does not claim a production installer, trusted signature, real Camoufox
runtime, production device-key protection or physical multi-device evidence.

## Custom URI Enrollment

The only accepted enrollment shape is:

`profilebridge://claim/<opaque-code>`

The parser is case-sensitive and rejects alternate scheme/host/path shapes,
additional path segments, query strings, fragments, percent encoding,
backslashes and traversal-like values. The claim code is bounded opaque ASCII,
does not implement `Display`, and is redacted by `Debug`. CLI success and error
messages never echo the URI or token.

A claim has a strict expiry boundary and is single-use. First redemption binds it
to one typed device ID. Replay by that device and attempted rebinding by another
device fail with distinct deterministic outcomes.

## Device And Key Boundary

The pure Bridge domain defines replaceable device identity and device key-handle
ports. Repository tests use deterministic fake adapters. Production CNG, TPM or
DPAPI protection, key rotation and recovery escrow remain later external and
implementation gates; no fake handle is treated as cryptographic evidence.

## Workspace Writer Exclusion

A local workspace has at most one active writer lease. The lease carries a
monotonic local epoch and opaque redacted lock token. Repeating the same acquire
is idempotent, while another writer is rejected. A release must match device,
epoch and token, so a stale process cannot unlock a newer writer.

Browser runtime lock files such as `parent.lock`, `.parentlock` and
`SingletonLock` are evidence, not garbage. The Bridge must never remove them
blindly. A permanent policy gate rejects code that combines known browser lock
names with deletion APIs.

## Process Supervision

The process state machine distinguishes:

- idle;
- starting with a bounded readiness deadline;
- ready;
- closing with a bounded graceful-close deadline;
- closed cleanly;
- crashed;
- forced timeout.

A successful process exit is clean only after an explicit graceful-close
request. Exit during startup or normal running is a crash even when the OS exit
code is zero. Start or close deadline expiry produces `ForcedTimeout`; it cannot
silently become a clean profile close.

The native composition layer exposes a replaceable process-control port and a
deterministic fake that records spawn, graceful-close and forced-termination
commands. A Windows-only safe adapter uses `std::os::windows` argument encoding
without `unsafe` code. The pinned Windows job compiles and tests this module and
verifies a non-empty release executable.

## Camouhost IPC

Bridge and Camouhost exchange bounded, versioned typed messages. Step 6 defines
version `1` frames for hello/ack, launch/ready and close/closed. Frames with
unsupported versions, extra fields, invalid typed session IDs, newlines, NULs or
invalid booleans fail closed.

A deterministic fake Camouhost requires successful version negotiation before
launch, preserves the exact session ID through ready and close, and rejects
invalid message order.

## Local SQLite And Outbox

The local SQLite schema is rebuildable operational state, not a credential or
key store. It contains:

- one versioned Bridge state row;
- append-only idempotent command evidence;
- strict command sequence and expected-version checks;
- append-only outbox payloads with separately mutable delivery metadata.

A command, resulting state transition and outbox event commit in one SQLite
transaction. Exact duplicate commands are ignored without growing state;
conflicting reuse, stale expected versions and reordered sequences abort. Crash
and timeout results persist `dirty` or `uncertain`, never `idle` or a clean close.

## Evidence Boundary

Permanent CI must prove:

- pure domain and CLI tests on Linux and Windows;
- Bridge domain compilation for `wasm32-unknown-unknown` as a provider-free
  architecture check, even though the executable is Windows-native;
- deterministic SQLite migration, idempotency, ordering, append-only and outbox
  delivery tests;
- deliberate browser-lock deletion fixture rejection;
- Windows-only safe adapter tests;
- release compilation and non-empty `profile-bridge.exe` verification on the
  pinned Windows runner;
- unchanged Cloudflare Worker, D1 and existing repository gates.

Remote enrollment, registry handler installation, physical process/job behavior,
trusted code signing, real Camouhost/Camoufox execution, encrypted profile data,
production credentials and production readiness remain explicitly unproven.
