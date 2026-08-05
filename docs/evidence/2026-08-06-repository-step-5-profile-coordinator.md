# Repository Step 5 — Profile Coordinator Evidence

**Дата:** 2026-08-06  
**Статус:** accepted  
**Baseline:** `bd292093778c954f2126c2165fd65c78cbe37f65`  
**Accepted source head:** `e338186e53f02784d1d685ae3cd761f3cef34ef7`  
**Pull request:** #18  
**Exact-head Quality Gate run:** `31056722531`  
**Squash merge:** `78931f529152c209ebececbcbef1aca770b7e3e0`

## 1. Реализованный Coordinator Boundary

Step 5 implements one deterministic Durable Object name per opaque `ProfileId`.
The pure state machine owns launch ordering, active lease identity, monotonic lease
epoch, fencing token validation, idle and hard deadlines, drain state and explicit
recovery from dirty or uncertain closure.

Every accepted command carries a typed idempotency key, exact object-local
sequence, expected aggregate version and non-decreasing server-observed time.
The coordinator rejects sequence gaps, stale expected versions, conflicting
idempotency-key reuse, reordered time and writers whose session, epoch or fencing
token no longer matches the current lease.

## 2. Launch, Lease And Timeout Semantics

The accepted slice covers:

- bounded single-use launch intents tied to the verified actor and device;
- first claim issuance of a monotonic lease epoch and server-generated opaque
  fencing token;
- heartbeat extension bounded by the non-extendable hard TTL;
- clean, dirty and uncertain release outcomes;
- bounded drain transition;
- idle, hard and drain timeout transitions into `Uncertain`;
- owner-only explicit recovery from `Dirty` or `Uncertain` to `Idle`.

Writer commands perform timeout preflight in the state machine. Therefore a late
heartbeat, drain request or nominally clean release cannot bypass an expired
lease merely because a Durable Object alarm has not executed yet. The permanent
regression test proves a clean release at the idle deadline becomes uncertain.

## 3. Durable Object Persistence And Alarms

Durable Object storage persists a bounded replayable command journal. On every
request the typed Cloudflare adapter rebuilds the pure domain state by replay,
then applies the next command. An exact duplicate returns the original decision
without appending another journal entry; a conflicting duplicate fails closed.
The persisted tenant/profile identity cannot be rebound to another object
identity.

The Worker schedules alarms at the earliest pending launch-intent, idle, hard or
drain deadline. Alarm execution appends an ordinary typed `Tick` command, so
alarm-driven transitions use the same ordering, versioning and replay rules as
request-driven transitions.

## 4. Authentication And Authorization

The versioned route is:

`GET|POST /api/v1/tenants/{tenant_id}/profiles/{profile_id}/coordinator`

Before resolving a Durable Object namespace or stub, the Worker:

1. parses typed tenant and profile IDs;
2. verifies the external identity and resolves an active same-tenant membership
   into `ActorContext`;
3. checks explicit profile visibility through the accepted owner/member ACL
   repository;
4. returns the same neutral disclosure shape for missing, foreign and
   unauthorized profiles.

Historical profile/client assignment is not coordinator authorization. A
permanent deliberate negative fixture containing assignment-derived
authorization is rejected by CI. Recovery is restricted to the active tenant
owner; ordinary active members require an explicit profile grant for covered
coordinator access.

## 5. D1 Projection And Outbox Reconciliation

Durable Object storage is authoritative for command sequence, aggregate version,
lease epoch and fencing. D1 is a repairable tenant-scoped projection and
immutable outbox evidence boundary; the implementation does not claim an atomic
transaction across Durable Objects and D1.

Migration `0004_profile_coordinator_projection.sql` adds:

- an append-only coordinator projection-command table;
- a materialized latest projection per tenant/profile;
- identity, version, stale-sequence and same-sequence conflict guards;
- one coordinator outbox event per aggregate version;
- a D1-local trigger that commits command evidence, latest projection and outbox
  event together.

Successful authenticated GET or POST responses project the latest object
snapshot into D1 idempotently. Alarm-only changes may temporarily leave D1
behind; the next authenticated coordinator request reads the authoritative object
snapshot and repairs the projection. The projected sequence is queryable, making
lag detectable rather than silently hidden.

## 6. Permanent CI Result

Exact-head Quality Gate run `31056722531` succeeded on accepted source head
`e338186e53f02784d1d685ae3cd761f3cef34ef7`.

### Rust Linux And WASM

- permanent Step 5 source-boundary policy passed;
- explicit actor -> ACL -> Durable Object ordering check passed;
- deliberate assignment-derived authorization fixture was rejected;
- architecture, typed D1, governed-write and immutable contract gates passed;
- D1 schema, Step 4 ACL and governed-command regression suites passed;
- deterministic coordinator projection/reconciliation SQLite tests passed;
- rustfmt, Clippy with warnings denied, native domain tests and Cloudflare adapter
  tests passed;
- governed pure crates compiled for `wasm32-unknown-unknown`;
- delivery-status and tracked-tree high-confidence secret checks passed.

### D1 Catalog Migrations

- pinned Wrangler `4.94.0` applied migrations `0001` through `0004` to isolated
  local D1 state;
- migration replay was a no-op;
- coordinator projection-command and latest-projection tables were queried after
  migration.

### Rust Windows

- all native non-Worker/non-Cloudflare-adapter workspace tests passed.

### Cloudflare Worker Release Build

- the authenticated Worker and real typed coordinator adapters checked for WASM;
- pinned `worker-build 0.8.5` release packaging passed;
- generated Worker shim and Wasm artifact verification passed.

## 7. Доказанные Свойства

The accepted repository evidence proves the covered properties:

- one opaque profile ID maps deterministically to one coordinator object name;
- the first valid claim receives epoch `1`, and every subsequent turnover
  increments the epoch;
- prior session/epoch/fencing tuples cannot refresh or release the new lease;
- duplicate commands are deterministic and do not grow persisted storage;
- reordered commands, stale versions and conflicting idempotency-key reuse fail
  closed;
- idle, hard and drain expiry preserve uncertain state rather than reporting a
  clean close;
- a late nominally clean release cannot bypass timeout semantics;
- missing, foreign and unauthorized profile access retains neutral disclosure;
- assignment alone never authorizes coordinator access;
- D1 projection lag is detectable and a later authoritative snapshot can repair
  it idempotently;
- the actual Worker dependency graph packages the Durable Object and D1
  coordinator adapters into a release artifact.

## 8. Defects Found And Corrected

- the initial command-envelope constructor was incorrectly `const` for owned
  command values and was made a normal validated constructor;
- exact intent-expiry comparison was tightened to expire at the boundary;
- a delayed clean release could initially succeed before the alarm executed;
  timeout preflight is now part of heartbeat, drain and release transitions;
- Worker alarm scheduling required explicit conversion to the JavaScript date
  type expected by `ScheduledTime`;
- the first positive static-policy scan included its own deliberate negative
  fixture; repository scans now exclude fixture roots while direct negative
  fixture execution remains mandatory;
- temporary formatting and patch workflows were removed and permanent source
  hygiene rejects their return.

## 9. Ограничения И Внешние Gates

This evidence does not prove:

- remote Cloudflare staging or production deployment;
- real Durable Object eviction, geographic failover, remote contention limits or
  production alarm timing;
- immediate D1 projection after an alarm when no authenticated request follows;
- physical multi-device browser-profile transfer;
- Windows Profile Bridge, device keys, DPAPI/CNG, process supervision or local OS
  lock integration;
- Camouhost/Camoufox runtime execution;
- encrypted R2 generations, production key management, backup or disaster
  recovery;
- production credentials, trusted code signing, privacy readiness or production
  readiness.

No Cloudflare credential, production secret, remote resource, real user profile,
mailbox content or personal data was used. All actors, devices, sessions, tenants
and profiles were synthetic. `production_ready` remains `false`.
