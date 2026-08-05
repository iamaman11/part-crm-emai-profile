# Profile Coordinator

**Status:** Repository Step 5 implementation specification  
**Production readiness:** false

## Responsibility

Exactly one Durable Object name is derived from each opaque `ProfileId`. The object serializes launch coordination for that profile, while D1 remains a repairable projection and authorization catalog rather than a participant in a fictitious cross-service transaction.

## Command Boundary

Every accepted coordinator command carries:

- a typed idempotency key;
- a strictly increasing object-local sequence;
- an expected aggregate version;
- a non-decreasing observed timestamp;
- typed tenant, profile, actor, device and session identifiers where applicable.

The coordinator rejects sequence gaps, delayed or reordered timestamps, reused idempotency keys with different payloads, stale expected versions and commands from a previous lease epoch.

## Lease And Fencing

A claim consumes a live launch intent and issues:

- a monotonically increasing lease epoch;
- an opaque fencing token;
- an idle deadline;
- a non-extendable hard deadline.

A heartbeat or release is accepted only when session ID, epoch and fencing token all match the current lease. Once turnover occurs, every prior writer is permanently fenced from refreshing or committing against the coordinator.

## Lifecycle

The state machine distinguishes:

- `Idle` — available for a launch intent;
- `Active` — one current lease;
- `Draining` — active lease is expected to close before a bounded deadline;
- `Dirty` — graceful release reported unsynchronized local changes;
- `Uncertain` — idle, hard or drain timeout prevented proof of a clean close.

Dirty and uncertain outcomes never silently become idle. An explicit, evidenced recovery command is required before the next launch.

## Persistence And Reconciliation

Durable Object storage is authoritative for command ordering, lease epoch and fencing. D1 stores tenant-scoped profile/session projections and immutable outbox evidence. Projection lag is detectable by comparing object sequence/version with the projected sequence/version, and repair is idempotent.

The implementation must not claim atomicity across Durable Objects and D1. It must instead preserve replayable object evidence, idempotent projection writes and reconciliation state.

## Authorization

Coordinator ownership is not application authorization. Before issuing or claiming a launch intent, the Worker must resolve a verified active `ActorContext` and confirm explicit profile visibility through the existing owner/member ACL boundary. Historical client assignment alone never grants coordinator access. Missing, foreign and unauthorized profiles retain the neutral disclosure shape.

## Bounded Evidence

Repository acceptance requires native and WASM tests for duplicate commands, reordered heartbeats, stale fencing after turnover, timeout uncertainty, foreign tenant/profile identifiers, assignment-only access and unauthorized active members. Remote Cloudflare deployment, production credentials and physical multi-device evidence remain separate gates.
