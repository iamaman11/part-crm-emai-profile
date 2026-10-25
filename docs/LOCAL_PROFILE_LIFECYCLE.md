# Local Profile Lifecycle Boundary

**Status:** current bounded local Profile Bridge lifecycle contract  
**Production authorization:** not granted by this document

## 1. Scope

This boundary governs local materialization, exact generation selection, writer locking, browser/runtime
preflight, dirty-state preservation, clone-only recovery, quota planning and privacy-safe support metadata
for Profile Bridge.

It does not own Profile ACL, server device authorization, coordinator state, cloud generation commit or
Release/Capability admission. Those concerns are composed through their natural owners.

## 1.1. Permanent local-first crash-resilience invariant (accepted 2026-10-25)

**Local Firefox/Camoufox crash resilience is the baseline product property; encrypted cloud
synchronization is an additive, independently recoverable layer.** A managed browser Profile
must not become less durable or ordinarily less recoverable on the same Windows device solely
because Bridge, the coordinator, the network, or the cloud-save transport failed.

This is a **mandatory prospective invariant**, not a statement that the current shipped
Bridge already conforms. Native Firefox/SQLite session/WAL recovery is the first owned,
safe recovery mechanism for an interrupted local writer. The Bridge MUST:

- Retain the same working `user_data` bytes, including browser-managed SQLite/WAL,
  IndexedDB, extension state, session files and original runtime locks, across OS shutdown,
  Bridge crash, browser crash, and uncertain network/cloud save. No startup, updater, cache
  sweeper, re-materializer or retry may silently delete, overwrite, reset or declare that
  working state clean. Fresh on-disk evidence belongs to the local profile owner, not
  the R2 object or a hash alone.
- On next launch on the **same Windows device**, distinguish *crashed process* from
  *active writer* using native OS handle/process evidence; obtain fresh, currently
  authorized Profile/tenant/device access and current coordinator ownership before
  resuming normal browser execution. Reuse the original local working state **only**
  after checking that there is no incompatible newer cloud generation or competing
  writer and that runtime/profile identity and policy are compatible. Do not simply
  re-download the old remote generation onto unsynced local data.
- Reuse the browser's native crash recovery against that retained working directory
  whenever an authorized restart is safe. Clone/isolate only for destructive repair,
  forensic investigation or migration; clone-only is **not** a mandate to replace the
  everyday working profile with a fresh copy after every crash.
- Treat a transient heartbeat timeout/dependency outage as `AUTHORITY_UNKNOWN`, **not**
  affirmative lease revocation or safe `CLEAN`. Preserve process/user-state
  evidence. If the coordinator cannot prove exclusive authorization, do not issue
  remote commits or falsely grant a new writer; use an explicitly tested
  bounded authority-loss response that maximizes local state preservation.
  A positively revoked/stale/expired fence forbids further privileged writes;
  graceful quiescence should be preferred when safe, with forced termination only
  for evidenced security/process-safety necessity. No unchecked offline lease
  extension, duplicate writers, hidden reconciliation, or stale token reuse.
- Separate `LOCAL_WORKING` (local browser data remains recoverable),
  `LOCAL_UNSYNCED` (local changes not yet cloud-confirmed),
  `CLOUD_VERIFIED` (authenticated immutable generation committed) and
  `RECOVERY_OR_CONFLICT` (cannot yet safely reopen/publish). These are distinct
  facts, not automatic state aliases. `Saved` requires the accepted exact
  authoritative cloud commit; a cloud hash or local snapshot alone cannot prove
  Firefox browser usability (`WORKING`).
- Make the cloud operation bounded and independent of browsing: after a verified
  quiescent snapshot boundary, stream the profile in bounded memory into an immutable
  encrypted generation, exact-verify its storage object, CAS the active cloud
  pointer and retain recoverable N-1 with separately restorable keys. Large
  256/512 MiB profiles, quotas, 100 sequential saves and 100 profiles are
  acceptance requirements; the current 64 MiB full-RAM snapshot is not a
  compliant design.
- Retain original/uncertain local bytes until a safe, user-visible recovery or
  conflict decision is established; clean-cache/quota reclamation may touch only
  *proven* expendable copies, never a sole local working copy or unverified attempt.

**Conflict/authority boundary:** local availability is not blanket permission to start
a revoked user/device or an indefinitely offline profile. Availability can be
`RECOVERY_OR_CONFLICT` with data retained if a fresh authorization or single-writer
proof cannot be obtained. Cross-device conflict must be preserved as two durable
branches, not automatically merged SQLite or silently overwritten. This invariant
changes the *reliability target*, not the accepted ACL/CNG, fencing, CAS, identity,
provider-effect, or Production authorization owner.

**Required executable parity:** actual installed Windows, real pinned Camoufox, with
stable profile identity and fixture hashes: normal GUI close, Windows shutdown with
browser still open, unexpected power loss, abrupt browser/Bridge termination,
network outage/timeout and restoration, ambiguous PUT/commit response, exact
same-device recovery, cross-device concurrency, cold N-1/key restore, 256/512 MiB
SQLite/WAL/IndexedDB profile, 100 saves and 100 profiles. Record whether original
working bytes survive, whether normal browser recovery reopens correctly, cloud
truth, recovery steps, memory/disk overhead and any contradictory writer authority.
Green repository CI or a verified cloud object does not replace this proof.

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

New generation paths use full SHA-256 digests: a domain-separated, NUL-delimited tenant/Profile
scope and a domain-separated generation ID. The scope is split into two directories:

```text
<marked-root>/<scope[0..32]>/<scope[32..64]>/<generation-digest>/
```

This keeps browser paths short regardless of ID lengths; full IDs still own server, snapshot and
authorization identity. The validated workspace carries those typed IDs from its root owner;
preflight, network and save consumers never reconstruct identity from directory names.
Snapshot publication uses the same path owner and preserves the profile parent of existing sidecars.
There are no junctions,
short-name aliases or truncated identity hashes. The three-level depth preserves root-scoped writer
admission and update quiescence. Existing ID-shaped directories contain persisted user/recovery data:
lookup may read them only when no compact target exists; new creation rejects an existing legacy
target instead of shadowing it. No automatic move/delete occurs. This read-only predecessor is retired
only after those directories have been explicitly recovered/migrated or removed by their data owner.

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

The installed runtime inventory is fully verified before device authentication and launch-claim
redemption, so this filesystem work does not consume the short coordinator launch-intent lifetime.
Runtime selection is independent of actor/Profile/generation; their exact compatibility binding is
still enforced by the preflight below. A rejected runtime cannot consume the claim, acquire a lease
or start the browser. Claim/session lifetimes and replay protection are unchanged.

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

Exact saved configuration and runtime integrity are distinct from complete observed fingerprint
equality. Before navigation, the existing typed domain owner rejects a wrong browser/OS identity.
Differences in hardware, display/DPR, WebGL capabilities, fonts, locale or speech voices are diagnostic
for ordinary profile launch; they neither rewrite saved identity nor certify fingerprint equality.
An explicitly observed lack of WebGL is permitted; missing or malformed observation is not evidence
of a compatible runtime. The supported headful platform, usable display/process/filesystem and clock
requirements remain enforced. Strict fingerprint comparison remains available for certification.

## 6. Runtime Supervision And Coordinator Heartbeat

`ProfileBridgeOperator` is the local runtime orchestration owner. The shipping composition creates the
concrete adapters and calls it; `main.rs` does not duplicate its lifecycle algorithm.

Shipping composition preserves the typed operator failure through the CLI. Safe user diagnostic
codes identify the failed phase; closed owner outcomes preserve cleanup/recovery details without raw
claims, profile data or local paths. Close-observation failure never fabricates a controlled close.


**Accepted separation of responsibilities — lease != save trigger (2026-10-25):**

1. Coordinator heartbeat is a minimal, bounded, server-authoritative renewal of
   the **currently active** device/session/epoch/fence and single-writer lease.
   Its only outcomes are confirmed renewal, confirmed refusal/loss, or
   transport/dependency uncertainty. It is neither a browser-close observer,
   a profile-dirty signal, nor permission to publish an otherwise unverified
   generation. Do not introduce per-file/polling cloud sync or duplicate leases.
2. Bridge independently supervises the controlled native Firefox process. A
   witnessed normal close is the **automatic** save trigger: quiesce and
   confirm all Firefox writers/locks are gone, retain the same working
   `user_data` and durable unsynced intent, then perform encrypted generation
   upload, storage verification, authoritative CAS/commit, and truthful status.
   The entire save must still have **fresh** allowed ownership/effect authority.
   Do not wait for a heartbeat tick to notice close, and do not equate a
   completed close with `Saved`.
3. Unexpected browser/Bridge exit, Windows shutdown or sudden power loss is
   **not** a controlled-close witness and cannot guarantee immediate cloud
   upload. The original local working bytes, Firefox WAL/session state and
   unsynced/recovery evidence survive; on restart, fresh process/lock, device,
   ACL, coordinator, active-generation and conflict checks decide whether
   same-directory native Firefox recovery or an authorized deferred save can
   proceed. No fresh cloud download may overwrite unsynced local state.
4. During transient coordinator timeouts or HTTP 5xx, do not assert a revoke,
   invent a successful renewal, change the expiry, or discard local bytes.
   Permit at most a **bounded** same-idempotency retry before an
   `AUTHORITY_UNKNOWN` quiescence/recovery boundary; no unproven remote
   publication or second writer. An explicit 401/403/409, confirmed fence
   replacement/revoke, or passed authoritative expiry requires stop/fencing,
   with safe graceful close where possible. A browser close during uncertain
   authority retains unsynced files rather than performing a speculative save.
5. The UI must distinguish browser closed, locally recoverable/unsynced,
   cloud-verified saved, and recovery/conflict; heartbeat acknowledgements,
   HTTP request completion and local checkpoint existence are **not** `Saved`
   proof. Remote N-1 and independently recoverable keys remain required.

**Required regression proof:** independently simulate normal close with
healthy lease (single upload/commit and truthful `Saved`), long live browsing
with many heartbeats but no close (no save), close racing with uncertain
heartbeat (no stale publish; local bytes retained), short transport flap and
5xx (same command/idempotency retry, no fabricated renewal), explicit
401/403/409/stale fence/two-device contest (no unauthorized writes), Windows
shutdown/kill/crash (native same-directory recovery after fresh authorization,
no deleted WAL/SQLite), and a lost upload/commit response (exact authorized
reconciliation, no duplicate generation and no false `Saved`). Deterministic
unit/integration tests are necessary; installed Windows and current authorized
staging proof remain mandatory to close #802.

While a browser is active:

1. the supervised process must still belong to the exact active session;
2. the Bridge heartbeats the exact coordinator lease/session/epoch/fence;
3. heartbeat success preserves ownership;
4. an independently evidenced local process crash preserves working Profile files and
   records recovery, without deleting native Firefox WAL/session data;
5. transport timeout/dependency failure alone does not prove fence revocation: it
   enters explicit uncertain-authority handling, preserves local bytes, forbids
   unproven cloud commit and does not claim a Clean release;
6. proven lease revocation/expiry/loss of exclusive writer authority requires a bounded,
   state-preserving stop/quiescence with no further unauthorized writes and no
   second writer; the current blanket force-terminate-on-heartbeat-error path must
   be replaced and validated before this contract is accepted in shipping.

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

## 9. Local crash recovery and clone-only destructive repair

An interrupted Firefox writer does not automatically mean its local working profile is
corrupt. Before any repair or replacement, preserve its original bytes and prove the
old process has exited, current device/user/tenant rights are valid, there is no
competing writer or unhandled remote successor, and the pinned runtime can safely
attempt Firefox's normal SQLite/WAL/session recovery on the same working directory.

**Clone-only applies to destructive repair/migration/forensics**, not to every ordinary
same-device crash restart. If repair is needed, a fresh recovery clone records
source inventory, copies accepted entries and proves source/clone integrity. Browser
database semantic repair only runs on the isolated candidate, never silently changes
the last confirmed cloud generation or destroys unsynced local work.

When authorization, cloud predecessor/successor or ownership is unresolved, the local
bytes stay available for controlled recovery but a normal browser writer is **blocked**
until the correct authority can be established. This is a known usability gap in
current shipping code, not permission to bypass existing fences.

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

## Windows first installation and delivery integrity

The existing Release Set Build is the sole publication owner. Assembly emits the exact eight
aggregate inputs; its final Windows job adds detached CMS and `ProfileBridgeSetup.exe` **before**
publishing the complete ten-asset immutable release. Heavy runtime components are reused by exact
content identity. Partial installer/signature pairs fail materialization; historical deployed
aggregates remain readable for known-good evidence without being upgraded to installer proof.

For initial staging installations the OS installer is intentionally unsigned. The user accepted
possible Windows warning/blocking; no paid subscription, certificate/root-store import, certificate
user enrollment or security-setting override is part of the path. Initial bootstrap authenticity
comes from the canonical HTTPS application and protected GitHub immutable publication. This does
not provide Windows verified-publisher identity or promise installation on every Windows policy.

Application delivery integrity remains mandatory: exact manifest bytes, one CMS SHA-256 signature,
exact pinned certificate and Rust `TrustedSignerSet` admission/rotation/revocation, sequence/replay
floors, checked component digests, archive admission and guarded activation. This application key
is separate from the user's non-exportable CNG device key and Cloudflare human identity. The CMS
adapter does not consult Windows CA trust, mutate certificate stores or enroll a machine certificate.
Verification calls Windows Crypt32 directly through the isolated native platform adapter. It starts
no PowerShell process and creates no signature scratch files. Exactly one signer, SHA-256 and the
application-admitted certificate pin remain mandatory; malformed, tampered, weak-digest and multiple-
signer inputs fail closed. PowerShell signing remains confined to CI and ephemeral test-fixture creation.

`scripts/windows-delivery-sign.ps1` is a CI-only cryptographic effect adapter, called only by the
accepted-main protected publication job. The staging signing key is injected solely into its step,
imported with EphemeralKeySet, never logged or written to disk; the six-calendar-month credential is persisted
in Vault before GitHub binding. Missing/expired key or failed signature stops publication. No CMS
fallback exists. Future signer rotation remains an explicit `TrustedSignerSet` operation, not an
implicit download of new trust. This adapter grants no candidate adoption or provider deployment.

`scripts/windows-delivery-publication.py` remains a deterministic renderer/transport adapter. Its
installer-component consumer checks the exact Release Set archive size/hash, closed two-file ZIP
inventory and component-manifest source/digest before extracting the bounded executable. Filesystem
and child-process effects belong only to rendering and isolated synthetic compiler/native proofs;
there is no network, credential, runtime-policy or provider mutation authority. Native delivery
owners still decide admission. These are durable consumers, not a transitional parallel installer.

Local/hosted tests prove real pinned CMS acceptance and tamper/unknown-key/unsigned rejection,
repeatable signing, production wizard compilation without an OS signing service and native rejection
before registry/installation effects. They do not substitute for clean-Windows hosted B7 installation,
connection, first generation, launch, save and reopen.

## V2.3E / #802: authoritative local readiness and saved bytes

A directory or generation marker alone is never reusable proof. The single authoritative-open
owner checks the host-local materialization binding (typed tenant, Profile, selected generation,
portable browser identity and immutable-input inventory). For a verified existing binding with
matching digest, reopening the **server-selected** generation is a cache reuse and does not
download the object. A different cached generation, regardless of mtime, is not selected.

A missing binding on an existing generation is **not** permission to delete or adopt data.
With the currently claimed coordinator lease and exact local device/epoch lock, the existing owner
requests the exact current VERIFIED server descriptor and decrypts its authenticated object.
Recovery of host-local binding is permitted only when a full byte-for-byte comparison, including
mutable `user_data`, succeeds with no Firefox writer lock. Diverging local bytes and uncertain
locks are retained and produce a typed recovery refusal. Malformed/foreign binding evidence is
not silently repaired. A missing canonical generation is materialized only through the existing
staged snapshot owner with portable identity and sidecar evidence prepared before publication.
A pre-publish failure never makes the canonical path Ready; an exact retry remains supported.

Initial generation cloud commit precedes local binding persistence. Once the server has
confirmed its immutable generation, an error writing the local binding or releasing the Bridge
lock must **retain** the canonical workspace and never run destructive rematerialization cleanup.
The next authorized open still resolves the authoritative generation from the server.

Delivery health is an installation/activation fact, not a profile outcome. Existing-profile
launches start the durable attempt at the managed-runtime boundary **after** profile/cache/network
preflight and **before** the first possible runtime spawn. First-generation preparation uses the
same installed runtime owner around identity materialization. Pre-runtime profile refusal must
not consume PendingHealth; an attempt already marked HealthAttemptStarted cannot be silently
declared Healthy. Actual spawn/IPC failure remains recovery.

No new state database, status service, source of identity, schema or user-access bypass is added.
This section describes the source contract; CI and hosted runtime tests are not a claim of
installed Windows save/reopen success.
