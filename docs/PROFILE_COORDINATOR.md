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
and unauthorized Profiles retain neutral disclosure behavior.

Browser/operator ingress may expose the commands appropriate to its authenticated application role.
It does not weaken the separate machine boundary below.

## Machine-Authenticated Bridge Boundary

The shipping Profile Bridge uses the canonical machine route:

`GET|POST /bridge/v1/tenants/{tenantId}/profiles/{profileId}/coordinator`

This route is not authenticated by a retained browser credential. Before coordinator payload semantics
are trusted, the Worker resolves the Bridge machine through the dedicated machine perimeter and the
existing device-principal owner using edge-verified mTLS certificate identity.

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

The implementation never claims atomicity across Durable Objects and D1. It preserves replayable object
evidence, idempotent projection writes and explicit reconciliation state instead.

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
