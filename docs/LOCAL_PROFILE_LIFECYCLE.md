# Local Profile Lifecycle Boundary

**Status:** current bounded local Profile Bridge lifecycle contract  
**Production authorization:** not granted by this document

## 1. Scope

This boundary governs local materialization, exact generation selection, writer locking, browser/runtime
preflight, dirty-state preservation, clone-only recovery, quota planning and privacy-safe support metadata
for Profile Bridge.

It does not own Profile ACL, server device authorization, coordinator state, cloud generation commit or
Release/Capability admission. Those concerns are composed through their natural owners.

## 2. Shipping Launch Ordering

The shipping Bridge reaches local mutation only after the remote launch authority has been redeemed and
an exact coordinator lease has been claimed:

```text
profilebridge://claim/<opaque-code>
-> strict URI parse
-> native device identity/key handle
-> machine-authenticated Control Plane redemption
-> exact tenant/Profile/device/generation/launch-intent binding
-> coordinator claim -> exact lease epoch/fence
-> open exact materialized generation
-> acquire local writer lock
-> validate materialization + runtime + network preflight
-> start managed Camouhost / real Camoufox
```

The claim URI is a bounded one-time handoff, not a local authorization source. Local code does not choose
a different Profile, generation or trusted device after redemption.

There is one shipping executable/composition path. Synthetic binaries/fakes may exist only behind an
explicit test-only feature and must remain production-unreachable.

## 3. Safe Materialization Root

A local root must be absolute, must be a real directory rather than a symbolic link, and must contain the
owned root marker. A non-empty unmarked directory is rejected rather than silently adopted.

Generation paths are composed only from opaque typed tenant, Profile and generation IDs:

```text
<marked-root>/<tenant-id>/<profile-id>/<generation-id>/
```

Each generation carries the expected owned marker. Existing unexpected targets, symlinked path
components, missing markers and marker mismatch fail closed. Email addresses and user-selected labels
are never path segments.

## 4. Bridge Writer Lock

Profile Bridge owns only its application lock. Acquisition uses atomic create-new semantics and records
the typed device ID plus lease epoch. A second writer receives `LockBusy`.

Release is explicit and verifies exact ownership before deleting the Bridge-owned lock. There is no
automatic `Drop` cleanup that could silently declare a crashed writer clean.

Browser-owned lock files such as `.parentlock`, `parent.lock` and `lock` are ordinary browser state and
are never deleted merely to acquire ownership. Ambiguous writer state fails closed as busy/recovery.

The coordinator lease is acquired before the local writer lock; launch failure after either acquisition
runs bounded cleanup in reverse ownership order. Unresolved cleanup blocks a later session rather than
creating parallel ownership.

## 5. Pinned Materialization And Runtime Preflight

A shipping launch does not approve arbitrary local runtime bytes merely because they can be hashed.
The selected local runtime manifest/inventory must match the authoritative browser materialization
identity for the exact redeemed generation before process start.

The accepted preflight binds at least:

- exact tenant/Profile/generation workspace identity;
- exact device and local writer epoch;
- expected browser identity compatibility version;
- expected runtime version and runtime inventory digest;
- exact runtime entrypoint/runtime-lock bytes;
- expected fingerprint configuration/probe identity;
- current bounded network identity observation under the accepted network policy.

Any missing, stale or mismatched binding fails before real browser launch. Runtime/process configuration
is an outer adapter concern and may not weaken the materialization identity comparison.

## 6. Runtime Supervision And Coordinator Heartbeat

`ProfileBridgeOperator` is the local runtime orchestration owner. The shipping composition creates the
concrete adapters and calls it; `main.rs` does not duplicate its lifecycle algorithm.

While a browser is active:

1. the supervised process must still belong to the exact active session;
2. the Bridge heartbeats the exact coordinator lease/session/epoch/fence;
3. heartbeat success preserves ownership;
4. process loss or heartbeat/fence loss force-terminates the runtime and enters recovery/terminal cleanup;
5. the browser never continues running after distributed ownership is lost.

A failed force termination, local-lock release or coordinator release is retained as explicit cleanup
failure and blocks new ownership until recovered.

## 7. Deterministic Inventory

Inventory recursively accepts regular files and directories only. It rejects symbolic links,
non-UTF-8/unsafe relative paths, special files, excessive path length and excessive file count.

Entries are deterministic and contain relative path, byte length and content digest. Security-critical
runtime/generation identities use their accepted cryptographic/versioned contracts; older local
repository-test identities are not promoted into cryptographic authority merely by reuse.

Bridge control files are excluded from logical browser inventory. Browser-owned lock files remain part
of browser state.

## 8. Local Generation State Machine

The local record uses explicit states:

```text
MATERIALIZED_CLEAN
  -> IN_USE
  -> DIRTY_LOCAL
  -> SYNCED_EVICTABLE
  -> EVICTED

IN_USE
  -> RECOVERY_REQUIRED
  -> MATERIALIZED_CLEAN | QUARANTINED
```

Opening requires exact distributed/local ownership and a successful preflight. Graceful browser close
produces `DIRTY_LOCAL`, not a clean or evictable state. Crash, process loss or coordinator ownership loss
produces `RECOVERY_REQUIRED`.

A controlled dirty close retains the local writer lock and coordinator ownership until the immutable
successor is uploaded, exactly verified and authoritatively committed, or until an explicit recovery
outcome is recorded. The confirmed-save/reopen semantics are owned by the subsequent save/reopen
application transaction, not by launch admission itself.

## 9. Clone-Only Recovery

Recovery never mutates the source generation in place merely to obtain a launchable workspace. A fresh
recovery clone records the source inventory, copies only accepted entries and proves source/clone
integrity under the owned recovery contract.

Browser database semantic repair, when required, executes only on an isolated recovery candidate and
must not silently rewrite the last confirmed generation.

## 10. Forgotten-Window And Quota Policy

Forgotten-window thresholds are strictly ordered and bounded. Clock regression fails closed. An active
session progresses from no action to warning, drain and hard-stop decisions without inventing a second
ownership authority.

Quota planning selects only unlocked `SYNCED_EVICTABLE` generations. `IN_USE`, `DIRTY_LOCAL`,
`RECOVERY_REQUIRED`, `QUARANTINED` and locked generations are never broadened into eviction candidates
when space is insufficient.

## 11. Privacy And Secret Boundary

Support summaries contain aggregate local state/failure metadata only. They do not emit Profile contents,
paths, credentials, proxy secrets, device private keys or launch claim material.

The one-time claim remains redacted in domain/debug formatting and is exposed only through the narrowly
named transport serializer. Native machine transport sends the redemption JSON body through stdin to the
owned HTTPS client process rather than placing raw claim material in process arguments. Application
errors remain generic and must not echo the claim.

## 12. Permanent Evidence

Repository and protected CI prove the boundary at multiple tiers:

- local lifecycle and writer-lock positive/negative tests on Linux/Windows;
- runtime/materialization mismatch tests;
- operator tests for exact launch intent, lease binding, heartbeat loss, crash cleanup and unresolved
  cleanup blocking;
- shipping machine-client tests for redemption -> coordinator claim -> heartbeat -> release and stale
  fence rejection;
- production-boundary checks rejecting claim-only success, direct duplicate process execution, extra
  shipping binaries and production-reachable synthetic runtime;
- real Camoufox cold-launch/managed IPC proof on exact source;
- Windows release Bridge build/regression proof.

These are repository/candidate proofs. Trusted installer/signing/distribution/updater/rollback and exact
environment authorization remain separately owned later release stages; their absence may not be hidden
by claiming repository tests are Production evidence.
