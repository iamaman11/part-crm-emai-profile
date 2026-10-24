# Profile Coordinator

**Status:** current bounded coordinator contract  
**Production authorization:** not granted by this document

## Responsibility

Exactly one Durable Object name is derived from each opaque `ProfileId`. The object serializes launch
coordination for that Profile. D1 remains a repairable projection and authorization/catalog boundary,
not a participant in a fictitious cross-service transaction.

The coordinator owns session ordering, launch-intent consumption, lease epoch/fencing and recovery
state. It does **not** own Profile ACL, device identity, Profile generation state or local workspace
locking.

## Command Boundary

Every accepted coordinator command carries:

- a typed idempotency key;
- a strictly increasing object-local sequence;
- an expected aggregate version;
- server-observed transition time;
- typed tenant, Profile, device, launch-intent and session identifiers where applicable.

The coordinator rejects sequence gaps, reused idempotency keys with different payloads, stale expected
versions and commands from a previous lease epoch/fence.

## Launch Intent, Lease And Fencing

A claim consumes one live launch intent bound to the authorized device and issues:

- a monotonically increasing lease epoch;
- an opaque fencing token generated inside the authenticated server boundary;
- an idle deadline;
- a non-extendable hard deadline.

A claim is not a generic `acquire(profile, device)` operation. The exact `launchIntentId` created by the
server-authorized launch flow must be presented and consumed by the coordinator. This prevents a
machine that merely knows a Profile/device pair from manufacturing runtime ownership.

A heartbeat or release is accepted only when session ID, device, epoch and fencing token match the
current lease. Once ownership turns over, every prior writer is permanently fenced from refreshing or
committing against the coordinator. Clients never choose or reuse the fencing token issued for a new
claim.

## Lifecycle

The state machine distinguishes:

- `Idle` — available for a launch intent;
- `Active` — one current lease;
- `Draining` — active lease is expected to close before a bounded deadline;
- `Dirty` — graceful release reported unsynchronized local changes;
- `Uncertain` — idle, hard or drain timeout prevented proof of a clean close.

Dirty and uncertain outcomes never silently become idle. An explicit, evidenced recovery command is
required before the next launch.

## Browser / Operator HTTP Boundary

The browser/operator route remains:

`GET|POST /api/v1/tenants/{tenantId}/profiles/{profileId}/coordinator`

Before resolving the Durable Object stub, the Worker verifies the external user identity, active
membership and explicit Profile authorization through the existing ACL owner. Missing, cross-tenant
and unauthorized Profiles share the accepted **neutral disclosure response**.

Browser/operator ingress may expose the commands appropriate to its authenticated application role.
It does not weaken the separate machine boundary below.

## Machine-Authenticated Bridge Boundary

The shipping Profile Bridge uses the canonical machine route:

`GET|POST /bridge/v1/tenants/{tenantId}/profiles/{profileId}/coordinator`

This route is not authenticated by a retained browser credential. Before coordinator payload semantics
are trusted, the Worker resolves the Bridge machine through the dedicated machine perimeter and the
existing device-principal owner using a revocable device-bound application session and fresh
request proof from the registered non-exportable Windows CNG key.

Machine ingress is deliberately narrower:

```text
machine-authenticated Bridge
  -> snapshot
  -> claim exact server-issued launch intent for its own active device
  -> heartbeat its own exact session/epoch/fence
  -> release its own exact session/epoch/fence
```

A machine must not use this ingress to issue arbitrary launch intents, choose another device, recover
another owner, or widen its Profile authorization. The server revalidates tenant/Profile/device/session
binding before the coordinator mutation.

Route templates and wire DTOs are owned by `control-plane-contract`; Worker ingress and native Bridge
clients consume those owners rather than duplicating `/bridge/v1/...` strings or JSON semantics.

## Runtime Heartbeat Contract

Long-running shipping runtime ownership depends on `ProfileCoordinatorRuntimePort`. The Bridge operator,
not `main.rs`, owns the fail-closed consequence of heartbeat failure:

```text
active browser
-> verify supervised process is still running
-> heartbeat exact lease/fence
-> success: continue
-> failure/lost ownership: force terminate browser
   + release/cleanup best effort
   + enter recovery/terminal state
```

There is no default/no-op heartbeat implementation. A browser may never continue running unfenced after
coordinator ownership is lost.

Server-returned idle/hard deadlines are authoritative runtime timing observations. Missing, zero,
reversed or otherwise invalid timing fails closed.

## Persistence And Reconciliation

Durable Object storage is authoritative for command ordering, lease epoch and fencing. D1 stores
tenant-scoped Profile/session projections and immutable outbox evidence. Projection lag is detectable by
comparing object sequence/version with projected sequence/version, and repair is idempotent.

The implementation **must not claim atomicity across Durable Objects and D1**. It preserves replayable
object evidence, idempotent projection writes and explicit reconciliation state instead.

An ordinary snapshot reads the authoritative Durable Object and performs no D1 projection-command or
outbox write. Its sequence belongs to the transition being observed, not to a new `snapshot` command.
Command execution and exact command replay retain the existing idempotent D1 projection/outbox path;
conflicting command outcomes or projection bytes still fail closed. A snapshot does not certify that
the D1 projection has caught up with an alarm or an interrupted projection write. Historical snapshot
rows remain immutable evidence and require no schema migration or data reset.

The snapshot response sets `replayed=true` to report that this observation appended no command.
Bridge accepts that read marker while checking the exact snapshot identity, version/sequence and
idle state without an active writer. It does not treat the snapshot as lease authority: the following
Claim must still return a fresh, non-replayed response with the exact session/device/epoch/fence.

## Authorization

Coordinator ownership is not application authorization. Profile ACL, membership and device trust remain
owned by their respective application/security boundaries.

For public launch, authorization/device/readiness are established before launch authority issuance and
revalidated at machine redemption. Coordinator claim then consumes the exact authorized launch intent;
it does not recreate or replace those policies.

Historical Client assignment alone never grants coordinator access.

## Bounded Evidence

Repository acceptance requires positive and negative proof for:

- exact launch-intent claim;
- duplicate/replayed commands;
- wrong device/session/tenant/Profile;
- stale epoch or fencing token;
- reordered/conflicting heartbeat;
- heartbeat ownership loss causing runtime termination;
- timeout/uncertain state;
- assignment-only access and revoked authorization;
- machine route denial for commands outside the narrow Bridge role.

Remote environment admission and Production authorization remain separate later-stage evidence. Green
repository CI proves only the exact source candidate on which it ran.

## V2.3E / #802: evidenced no-writer cancellation

The native operator has one distinguished `close_without_browser_use` port operation.
Its **default implementation remains generic Uncertain**, so adapters that cannot prove the
difference fail closed. The shipping adapter emits existing `ReleaseDisposition::Clean` only
for the current matched coordinator cursor (tenant, Profile, device, session, epoch, fence,
sequence, expected version). It is not a new recovery ingress or a Save operation.

The operator may invoke this operation only before an attempted managed runtime spawn, after
acquiring and successfully releasing its exact Bridge lock, and after observing no pre-existing
Firefox writer artifacts. Preflight evidence saying ProfileBusy/RecoveryRequired disqualifies
Clean. Unreadable lock state or foreign writer, failed local lock cleanup, changed lease/fence,
unknown remote release response, and every post-spawn failure remain Uncertain/recovery. A
successful Bridge-lock removal alone is not proof of saved browser data. Full controlled save
still requires verified upload + authoritative server commit before clean release.

The historical Bridge 519 persisted `HealthAttemptStarted` and coordinator `Uncertain`
are **not** evidence of clean cancellation. They need exact independently verified owner
recovery; absence of a process or a newer executable version is insufficient. No manual
journal mutation, lease reset, R2 replacement, deletion of profile bytes or replayed claim
is authorized by this source change.
