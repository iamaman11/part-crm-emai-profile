# Browser Profile Platform — Development Plan

**Status:** normative post-composition development plan  
**Date:** 2026-08-07  
**Tracking:** issue #75  
**Audit baseline:** `08ffa240996954ca6a25e0784ab38c78841107c3`  
**Production readiness:** unchanged; external evidence gates remain authoritative

## 1. Purpose And Source Of Truth

This document records two things together:

1. a current-state architecture quality audit of the accepted repository-local composition;
2. the ordered development plan for the next functional expansion of the standalone Browser Profile Platform and its later CRM integration.

For post-composition execution order, this document is authoritative. Existing `DELIVERY_ROADMAP.md` remains the historical source for Repository Steps 0–10 and their acceptance discipline. `ARCHITECTURE.md`, accepted ADRs, security/privacy documents and versioned contracts remain authoritative for their own invariants. `DEVELOPER_CAPABILITY_MATRIX.md` remains authoritative for what is actually Composed, Library, Synthetic, Target or External at a given accepted `main`.

A planned item in this document is **not** an implementation claim. No external/provider/physical-host property becomes accepted without the existing external-evidence process.

## 2. Executive Architecture Assessment

The repository is a strong clean/hexagonal foundation, but the actual composition has grown beyond parts of the original target documentation. The current repository-local architecture is assessed at approximately **8.6/10** for engineering architecture quality. This score does not measure production readiness.

| Area | Assessment | Current state |
|---|---:|---|
| Domain isolation and invariants | 9.3/10 | Pure domain crates, typed IDs, strict state transitions and negative tests are strong. |
| Dependency direction | 8.8/10 | Compile-time allowlists protect inner layers; the documented adapter dependency diagram is now stricter than the actual and valid inward dependency shape. |
| Application-layer separation | 7.2/10 | `apps/control-plane-worker` performs substantial authorization/orchestration/idempotency/D1 mutation assembly that should move behind application use cases. |
| Adapter boundaries | 8.6/10 | D1/Access/DO/mailbox adapters are typed and fail closed, but app handlers know too many concrete adapter mutation types. |
| Data ownership and transaction semantics | 9.2/10 | D1 envelopes, optimistic versions, outbox/audit and DO/R2 separation are disciplined. |
| Frontend layering | 7.9/10 | React/TanStack composition is clean and server-authoritative, but route coverage is incomplete and public API types are handwritten rather than generated as the architecture claims. |
| Developer discoverability | 8.1/10 | Documentation is unusually strong, but several normative files lag the accepted composition and there are multiple overlapping roadmap/status documents. |
| CI architecture enforcement | 9.6/10 | Permanent exact-head gates, negative fixtures and forbidden dependency checks are a major strength. |
| Production/external claim discipline | 10/10 | Repository-local and external evidence are consistently separated; `production_ready=false` is preserved until real evidence exists. |

The correct next move is **not** to attach realtime, richer clients, search and device mailbox scheduling directly to the current Worker handlers. First converge the executable layering with the architecture contract, then build the new capabilities on those cleaned boundaries.

## 3. What Is Already Architecturally Strong

### 3.1 Inner layers are genuinely protected

`crates/*-domain`, primitives, contracts, ports and use cases are guarded by an executable dependency allowlist. Provider/runtime dependencies such as Workers SDK, Windows, SQLx, Tokio, Axum and Python bindings cannot silently enter pure boundaries.

### 3.2 Critical identities are typed and opaque

Tenant, actor, client, profile, generation, device, mailbox, idempotency and audit identifiers are value objects rather than email/name/path-derived identifiers. This is the correct basis for client registry expansion, CRM linking and realtime events.

### 3.3 Authorization is not a frontend concern

The Worker resolves live identity/membership/grants and uses neutral disclosure for foreign or unauthorized resources. Existing assignment logic is already explicitly separated from access grants, which matches the expanded requirements.

### 3.4 D1 mutation semantics are disciplined

The repository already uses optimistic aggregate versions, idempotency, audit and outbox concepts and tests transaction rollback behavior. This is the right foundation for at-least-once Queue consumers and notification delivery deduplication.

### 3.5 Durable Object and R2 ownership are conceptually clean

Profile coordination belongs to a per-profile Durable Object; D1 remains the business/catalog projection; immutable encrypted generations belong in R2. There is no false distributed transaction claim between D1/DO/R2.

### 3.6 Frontend is server-authoritative

TanStack Query owns remote state and high-impact mutations are not presented as successful before the server confirms them. This supports the required rule that WebSocket is a change signal, never the source of truth.

## 4. Architecture Gaps To Resolve Before Expansion

### A0 — Application orchestration leaks into the Worker app — **High priority**

Current `apps/control-plane-worker/src/api.rs` and `mailboxes.rs` perform significant workflow logic directly: request-specific authorization flow, idempotency checks, concrete D1 mutation construction, version calculations, replay handling and command sequencing. Meanwhile `crates/use-cases` is mostly decision functions rather than the orchestration layer described by `ARCHITECTURE.md`.

**Target:** Worker modules become transport/composition adapters only:

```text
HTTP / Queue / Scheduled / DO ingress
  -> parse + authenticate transport
  -> construct verified request context
  -> call one application command/query
  -> map typed result/problem to protocol
```

Application commands/queries own authorization intent, repository port calls, idempotency semantics, durable mutation ordering and outbox intent. Concrete D1 statements remain in Cloudflare adapters.

**Permanent guard:** add a repository policy preventing ordinary Worker route modules from importing `d1_*` mutation/repository implementation modules directly. Only the Worker composition root/provider factory may construct concrete adapters.

### A1 — The documented adapter dependency graph is ambiguous — **Medium priority**

The current architecture diagram says adapters depend on ports/contracts/primitives, while `cloudflare-adapters` correctly depends inward on mailbox/session domain types as well. In Clean Architecture an outer adapter may depend inward on domain types; the dangerous direction is the reverse.

**Target dependency rule:** make the documentation and checker agree on one rule:

```text
primitives
contracts -> primitives
domains -> contracts + primitives
application-ports -> domains + contracts + primitives
use-cases -> application-ports + domains + contracts + primitives
adapters -> application-ports + domains + contracts + primitives + provider SDKs
apps -> use-cases + adapters + contracts + primitives
frontend -> generated public contracts + frontend feature/shared packages only
```

Domain/use-case code must never depend outward on adapters/apps/provider SDKs.

### A2 — `application-ports/src/lib.rs` is becoming a grab-bag — **Medium priority**

The single file already mixes membership, client, profile, coordinator, generation storage, mailbox provider and audit ports. Realtime/search/device/CRM growth would make this difficult to navigate and review.

**Target:** capability modules with an intentionally small public facade:

```text
application-ports/src/
  identity.rs
  clients.rs
  profiles.rs
  generations.rs
  sessions.rs
  mailboxes.rs
  notifications.rs
  search.rs
  devices.rs
  audit.rs
  crm.rs
```

### A3 — Large domain `lib.rs` files will not scale — **Medium priority**

`client-domain` and `mailbox-domain` are currently understandable, but the requested contact points, merge/link state, mailbox runtime lanes and richer job lifecycle would turn single-file modules into maintenance hotspots.

**Target:** split by aggregate/value/state machine before adding fields, keeping `lib.rs` as a deliberate public API.

### A4 — Frontend contract generation claim is not true yet — **High priority**

`frontend/src/shared/api/types.ts` manually declares public DTOs/enums, while `UI_ARCHITECTURE.md` says DTO/enums are generated from OpenAPI. This creates contract-drift risk, especially for realtime envelopes and expanded mailbox states.

**Target:** generated public TypeScript contracts/client from the accepted versioned API schema. Handwritten code may wrap generated types but may not redefine server enums or DTO shapes.

**Permanent guard:** CI regenerates and fails on diff; feature modules cannot define duplicate public API enums.

### A5 — Route composition is behind the target information architecture — **Medium priority**

The accepted router currently exposes dashboard, clients, profiles, mailboxes and users/access. The planned standalone product requires explicit detail routes plus sessions/devices/audit/settings and complete tab structures.

**Target:** file/feature-owned route definitions assembled by the app shell; the central router must not become one growing import list.

### A6 — Documentation drift exists — **Medium priority**

Examples: the D1 catalog foundation document still describes the initial migration baseline while accepted capabilities now extend beyond it; architecture target structure omits several later accepted pure crates; UI documentation describes generated contracts not yet implemented.

**Target:** every accepted capability PR updates the capability matrix and only the normative document it actually changes. Add a documentation consistency gate for known machine-checkable claims (workspace crates, routes, migration inventory, generated frontend contracts).

### A7 — Route classification will become a monolith — **Medium priority**

`control-plane-contract` centrally enumerates every HTTP route. It is currently safe and well tested, but realtime, search, device jobs and CRM projections will make it a merge-conflict hotspot.

**Target:** keep fail-closed route classification but group routes by versioned capability modules with a single composed classifier. Unknown `/api/*`, `/auth/*` and `/bridge/*` methods/versions must continue to fail closed rather than reach SPA assets.

### A8 — Query-side capabilities need an explicit read-model boundary — **High priority before search**

Unified search and UI catalog/list views are not aggregate commands and should not be forced through mutation-oriented repositories.

**Target:** application query services + typed read-model ports + D1 projections. Authorization/grant filtering happens in the query before result construction. R2, browser filesystem and frontend filtering never participate in authorization/search.

## 5. Target Module Map For Expansion

The next architecture should preserve existing crate boundaries and add capability modules rather than one large “service” crate.

```text
apps/
  control-plane-worker/
    ingress/http/
    ingress/queue/
    ingress/scheduled/
    composition/
    durable_objects/
      profile_coordinator/
      user_notification_hub/
  profile-bridge/
    ingress/
    device_jobs/
    runtime/

crates/
  primitives/
  contracts/
    api/
    events/
    bridge/
  identity-access-domain/
  client-domain/
    client.rs
    contact_point.rs
    assignment.rs
    merge.rs
  profile-domain/
  session-domain/
  mailbox-domain/
    binding.rs
    job.rs
    runtime_lane.rs
    observation.rs
  notification-domain/
    event.rs
    delivery.rs
    cursor.rs
  application-ports/
    ... capability modules ...
  use-cases/
    identity/
    clients/
    profiles/
    mailboxes/
    notifications/
    search/
    devices/
    crm_projection/
  cloudflare-adapters/
    d1/
    r2/
    access/
    queues/
    durable_objects/
    mailbox_providers/

frontend/src/
  app/
  routes/
  features/
    clients/
    profiles/
    mailboxes/
    sessions/
    devices/
    users/
    audit/
    settings/
    realtime/
    search/
  entities/
  shared/
    api/generated/
    realtime/
    ui/
    forms/
    observability/
```

`notification-domain` owns safe event/delivery/cursor rules only. It does not know WebSocket or Durable Objects. `UserNotificationHub` is an outer Cloudflare runtime adapter/composition component.

## 6. Cross-Cutting Contracts For All New Work

### 6.1 Versioned integration event envelope

All integration/realtime events use a single safe envelope with opaque identifiers, resource version, occurrence time, actor reference and correlation ID. PII, profile payloads, cookies, OAuth tokens, proxy credentials and browser secrets are prohibited.

Initial event taxonomy:

- `client.created`
- `client.updated`
- `client.merged`
- `client.linked_to_crm`
- `profile.created`
- `profile.assigned`
- `profile.reassigned`
- `profile.access_changed`
- `profile.session_changed`
- `profile.generation_activated`
- `mailbox.changed`
- `mailbox.auth_required`
- `device.status_changed`
- `certification.changed`

Event schemas are versioned contracts, not ad-hoc Queue/WebSocket JSON.

### 6.2 Durable-before-notify rule

For every business mutation:

```text
validated command
  -> durable canonical state mutation
  -> audit + outbox in the same durable boundary where possible
  -> dispatcher / Queue
  -> notification delivery
```

No UI signal may precede the canonical D1/R2/DO commit that it represents.

### 6.3 At-least-once consumer rule

Cloudflare Queues delivers at least once. Every Queue consumer therefore requires an event/message idempotency key and must make duplicate delivery harmless. A duplicate message must not increment unread counters twice, create a second business event or repeat a destructive side effect.

### 6.4 Authorization-before-projection rule

Search, lists, detail projections, realtime subscription/catch-up and CRM-facing projections must apply tenant scope and live grants before forming the returned result. “Find then hide in frontend” is prohibited.

### 6.5 PII boundary

Client contact values are encrypted at rest. Exact lookup uses tenant-keyed HMAC tokens. Prefix/fuzzy PII search requires an explicit privacy/security ADR because blind n-gram indexes reveal searchable structure.

### 6.6 Profile storage, materialization and freshness contract

Browser profile state uses explicit cloud authority plus local materialization. It is **not** a bidirectional folder-sync model and Profile Bridge must never infer authority by listing R2 objects.

Authoritative ownership:

```text
D1
  -> profile/generation metadata
  -> authoritative active_generation_id + aggregate version

R2
  -> encrypted immutable ProfileGeneration objects
  -> manifests/inventory required by the accepted generation format

per-profile Durable Object
  -> materialization/open lease
  -> fencing epoch/token
  -> concurrent-session coordination

Windows Profile Bridge
  -> local encrypted staging/materialization/cache/workspace
  -> local generation metadata and recovery state
  -> Camoufox/Camouhost lifecycle
```

The local filesystem is operational state and cache, not the cross-device source of truth. React never receives R2 credentials and never reads/writes the local profile directory directly; browser lifecycle remains behind the authenticated Bridge protocol.

Before any Camoufox launch, Bridge resolves the authoritative profile projection through the Worker/application API and compares the local materialized generation with `active_generation_id`.

```text
resolve authoritative active_generation_id
  -> local generation matches
       -> validate local integrity/runtime compatibility
       -> obtain/confirm the required open lease and launch
  -> local generation missing or stale
       -> obtain a fenced materialization lease
       -> fetch the authorized active generation object
       -> verify manifest/schema/runtime bundle/digest
       -> decrypt into staging
       -> verify inventory + SQLite/profile integrity
       -> atomic local activation
       -> continue the ordinary fenced open flow
```

A stale local generation may be retained for rollback/recovery policy, but it may not start a writer session as though it were current. Bridge does not choose “the newest object” from R2; only the control-plane projection determines which generation is active.

A state-changing browser session never overwrites the current R2 object. Graceful close produces a new immutable generation:

```text
active Gn
  -> local writer session under lease/fencing
  -> graceful close + quiescence evidence
  -> DIRTY_LOCAL snapshot candidate Gn+1
  -> package + encrypt
  -> conditional immutable R2 create
  -> verify remote digest + restore readability
  -> D1 compare-and-set Gn -> Gn+1 using expected profile version/fencing token
  -> audit + outbox / generation-activated event
  -> synced local state becomes eviction-eligible according to policy
```

If R2 upload or verification fails, the dirty local workspace remains recoverable as `DIRTY_LOCAL`/`SYNC_RETRY_PENDING`; it must not be deleted or reported as synced. If immutable upload succeeds but the D1 compare-and-set fails, that object is not authoritative and must be handled as an unactivated/orphan candidate by bounded recovery/retention logic rather than silently promoted. A stale fencing token can never activate a generation.

Multi-device behavior follows the same contract: once device A activates `G5`, device B holding `G4` must materialize `G5` before its next writer launch. Device B cannot later publish a generation derived from stale `G4` over `G5`; lease/fencing plus D1 compare-and-set reject that transition.

Local eviction is allowed only after the relevant generation is durably present/verified in R2 and the authoritative D1 state confirms the safe lifecycle outcome. Unsynced dirty state survives network loss, Worker/R2 failure and Bridge restart.

Required acceptance scenarios:

- local `G3`, active `G3` -> no cloud download; local integrity/open path proceeds;
- local `G3`, active `G4` -> `G4` is verified and atomically materialized before browser launch;
- device A activates `G5` -> device B holding `G4` observes and materializes `G5` on its next open;
- stale device/session result cannot replace an already activated newer generation;
- R2 upload/verification failure preserves local dirty state and never advances D1 active generation;
- crash between immutable R2 upload and D1 CAS is recoverable without treating the uploaded object as authoritative;
- duplicate/retried generation-finalization command is idempotent and cannot create multiple logical activations;
- unauthorized/revoked device or actor cannot resolve/download a generation through the application API;
- Bridge behavior does not depend on R2 object listing order, timestamps or “latest object” heuristics.

## 7. Ordered Development Phases

## Phase 0 — Architecture Convergence And Developer DX

**Goal:** make the executable architecture match the intended clean layers before adding major capabilities.

Work:

- split `application-ports` and `use-cases` into capability modules;
- move Worker application orchestration behind use-case command/query services;
- keep Worker route modules limited to transport mapping and dependency injection;
- document and enforce the corrected dependency direction;
- modularize route classification by capability without weakening fail-closed behavior;
- introduce generated TypeScript API contracts and remove handwritten duplicate DTO enums;
- add machine-readable architecture inventory for workspace crates, migrations, public routes and generated contracts;
- update stale architecture/D1/UI developer docs to the accepted composition.

Acceptance:

- no ordinary Worker handler directly constructs a D1 mutation type;
- application services are testable with fake ports without Workers SDK;
- adapters remain provider-specific and depend only inward;
- frontend contract regeneration is deterministic and CI-enforced;
- architecture checker has positive + deliberately forbidden fixtures;
- all permanent workflows green on exact head.

## Phase 1 — Event, Outbox And Notification Persistence Foundation

**Goal:** establish one durable event model used by realtime, mailbox, client/profile activity and CRM integration.

Add D1 structures:

- `notification_events`;
- `notification_deliveries`;
- `outbox_events` evolution/versioning where required;
- `consumer_idempotency`;
- `user_event_cursors`.

Build:

- `notification-domain` safe envelope and cursor/delivery state;
- versioned integration event contracts;
- outbox dispatcher use case and Queue adapter;
- idempotent consumer registry;
- catch-up query by user/grants and event cursor;
- bounded retention/compaction policy that never deletes canonical business state.

Acceptance:

- duplicate Queue delivery is side-effect neutral;
- event payload sanitizer rejects PII/secret-bearing fixtures;
- event is persisted only after canonical business mutation;
- cursor replay works after simulated disconnect;
- unauthorized users cannot query event history for ungranted resources.

## Phase 2 — Client Registry 2.0 And Assignment Model

**Goal:** turn the current minimal client record into the complete standalone registry while preserving opaque IDs and future CRM linking.

Client card:

- `client_id`, `PERSON|ORGANIZATION`;
- display name + optional legal name;
- `ACTIVE|ARCHIVED|MERGED`;
- country, locale, timezone, tags;
- structured notes;
- encrypted contact points: email, phone, URL;
- optional `external_party_ref`;
- aggregate version and timestamps;
- projections for profiles, mailbox bindings, grants, assignment/activity/audit history.

Assignment entity:

```text
ProfileClientAssignment
  assignment_id
  tenant_id
  profile_id
  client_id
  status
  valid_from
  valid_to
  assigned_by
  reason
```

Reassignment closes the previous active assignment, creates a new one, writes audit/outbox and emits the safe event. Assignment remains explicitly non-authoritative for access.

Security:

- contact ciphertext and key version metadata in D1;
- tenant-keyed HMAC exact lookup tokens;
- no contact/name in resource IDs, URL keys, filesystem paths or R2 keys;
- fuzzy/prefix PII search blocked until a separate approved ADR.

Acceptance:

- owner can create/update/archive/merge clients through use cases;
- one profile has at most one active primary client assignment;
- one client may own multiple profiles;
- assignment never grants client/profile access;
- member sees only grant-permitted card/profile projections;
- exact contact lookup does not require plaintext scan.

## Phase 3 — Unified Search And Read Projections

**Goal:** provide safe server-side discovery without weakening tenant/grant isolation.

Endpoint:

```text
GET /api/v1/search?q=<query>&types=client,profile,mailbox
```

Search projections cover:

- client display name;
- exact contact HMAC lookup;
- profile label;
- mailbox provider;
- tags;
- client assignment;
- profile lifecycle status;
- cloud status;
- certification status;
- mailbox status;
- assigned user;
- runtime lane.

Rules:

- typed tenant scope is mandatory;
- live membership/grants are joined/applied before result construction;
- stable cursor pagination and bounded query cost;
- R2 is never queried;
- resource labels/contact projections follow field-level disclosure policy.

Acceptance:

- IDOR/cross-tenant negative suite demonstrates no result-count or item leakage;
- revoked grant disappears from search immediately according to the accepted consistency contract;
- query plans are index-backed for supported predicates;
- fuzzy PII search remains unavailable without an accepted ADR.

## Phase 4 — Mailbox Operations 2.0: One Job Contract, Two Runtime Lanes

**Goal:** support Cloud provider APIs and Windows/Camoufox execution with one application job model.

Mailbox job states become:

```text
SCHEDULED
QUEUED
PENDING_DEVICE
RUNNING
SUCCEEDED
RETRY_PENDING
AUTH_REQUIRED
PROFILE_BUSY
FAILED
SUSPENDED
```

Runtime lanes:

1. **Cloud Worker** — Gmail API, IMAP and future standard provider APIs;
2. **Windows Profile Bridge + Camoufox** — providers requiring an authorized browser profile.

Canonical flow:

```text
Scheduled trigger
  -> Queue mailbox.check.requested
  -> mailbox policy + authorization/runtime selection
  -> provider execution or durable device job
  -> D1 transaction: mailbox state + unread + observation + audit + outbox
  -> notification pipeline
```

Provider-specific implementation stays in adapters. The domain owns job transitions and runtime-lane eligibility, not Gmail/IMAP/Mail.ru protocol details.

Acceptance:

- Gmail/IMAP and browser lane pass the same application contract suite;
- duplicate Queue deliveries do not duplicate business results/unread changes;
- revoked/suspended binding cannot execute;
- provider payload/message content never enters ordinary audit/events;
- retry/backoff and auth-required transitions are deterministic.

## Phase 5 — Device Job Channel And Camoufox Mailbox Mode

**Goal:** make browser-assisted mailbox checks durable when the assigned Windows device is offline.

Target flow:

```text
Cloud Queue
  -> durable mailbox job in D1
  -> DeviceCommandHub or /bridge/v1/jobs/claim
  -> device-bound authenticated Profile Bridge
  -> profile lease + current generation
  -> certified Camoufox mailbox mode
  -> graceful close
  -> generation update if session state changed
  -> mailbox result command
  -> D1 commit + outbox
```

The web-app WebSocket is never a device command channel.

Rules:

- offline device -> `PENDING_DEVICE`, not false failure/success;
- claim is lease/idempotency/fencing aware;
- only assigned/authorized device can claim;
- job survives Bridge restart/network loss;
- profile busy maps to `PROFILE_BUSY` without unsafe concurrent launch;
- headless mode requires separate Mail.ru certification; challenge/fingerprint drift forces approved headful background mode.

Acceptance:

- power-off/restart simulation preserves claimable job;
- stale device result is rejected after claim turnover;
- duplicate result submission is idempotent;
- Bridge cannot fetch arbitrary tenant jobs;
- real Camoufox behavior remains External until physical-host certification evidence exists.

## Phase 6 — Realtime UserNotificationHub

**Goal:** deliver low-latency safe change signals while keeping D1/API projections authoritative.

Topology:

```text
canonical state + outbox
  -> Queue dispatcher
  -> per-user UserNotificationHub Durable Object
  -> Hibernatable WebSocket
  -> React
  -> TanStack Query invalidation
  -> HTTPS GET canonical projection
```

Endpoint:

```text
wss://app.example.com/api/v1/realtime
```

Before upgrade the Worker verifies Access JWT, active tenant membership and user state, then routes to the logical per-user Durable Object.

Capabilities:

- multiple tabs/devices per user;
- reconnect with exponential backoff + jitter;
- client sends last accepted `event_id`;
- catch-up from durable event history or current projection;
- session progress, mailbox changes, revocation, device status, cloud sync and certification changes;
- membership revoke closes existing sockets and blocks reconnect/commands;
- bounded connection lifetime and periodic reauthorization policy;
- versioned event envelope;
- no unnecessary heartbeat that continuously wakes a hibernating DO.

Connection UI states:

```text
CONNECTED
RECONNECTING
OFFLINE
CATCHING_UP
```

Acceptance:

- online accepted event normally reaches UI within the product latency SLO;
- WebSocket event never contains prohibited PII/secret payload;
- UI always refetches canonical projection after invalidation;
- disconnect/reconnect recovers missed changes;
- duplicate delivery is harmless;
- membership revoke terminates all user connections;
- DO hibernation lifecycle tests prove no in-memory-only cursor/security dependency.

Cloudflare currently recommends the Durable Objects Hibernation WebSocket API for server-side WebSocket workloads because clients remain connected while the object can hibernate. Connection attachments/storage must therefore contain only bounded non-sensitive reconstruction metadata.

## Phase 7 — Standalone UI Information Architecture And Reusable Features

**Goal:** complete the operator product without moving business rules into React.

Required routes:

```text
/
/profiles
/profiles/:profileId
/clients
/clients/:clientId
/mailboxes
/sessions
/devices
/users
/audit
/settings
```

Client Detail tabs:

- Overview
- Contact Points
- Profiles
- Mailboxes
- Access
- Assignment History
- Activity
- Audit

Profile Detail tabs:

- Overview
- Client Assignment
- Session
- Generations
- Mailbox
- Certification
- Access
- Audit

Frontend rules:

- generated public API/event types only;
- TanStack Query is canonical remote cache;
- realtime module only invalidates/refetches or updates explicitly safe ephemeral progress;
- feature package cannot import sibling feature internals;
- route modules compose feature public APIs only;
- no optimistic “Saved” for grant, assignment, generation, session-close, mailbox result or profile sync commits;
- standalone features expose reusable entry points so future CRM shell can mount them without moving domain logic into frontend.

Acceptance:

- owner/member/revoked E2E suites;
- keyboard/accessibility critical flows;
- offline/reconnect states;
- no-secret/no-PII snapshot/telemetry checks;
- direct endpoint abuse remains denied server-side.

## Phase 8 — Standalone End-To-End Product Acceptance

**Goal:** prove the expanded standalone product as one composition without inflating external claims.

Required repository-local/synthetic acceptance:

- owner creates and finds a client through UI;
- client links to multiple profiles;
- profile has at most one active primary client;
- member cannot see foreign profiles/client cards;
- search does not disclose forbidden objects;
- Gmail/IMAP adapter fake and Camoufox mailbox fake use the same job contract;
- mailbox result commits before notification;
- reconnect catches up missed events;
- duplicate Queue delivery is idempotent;
- membership revoke blocks commands and realtime;
- offline Bridge preserves `PENDING_DEVICE` job;
- local/current generation equality avoids unnecessary materialization download;
- stale local generation is materialized from the authoritative active generation before writer launch;
- stale fencing/CAS cannot overwrite a newer active generation;
- simulated R2 failure preserves dirty local state and leaves the active generation unchanged;
- all new permanent gates run on one exact head SHA.

Real provider/physical-host claims remain External.

## Phase 9 — CRM Boundary And Party Integration

**Goal:** switch client data authority to CRM without migrating browser runtime ownership.

Ownership after integration:

```text
CRM Party/Customer Master
  -> canonical name/contact/status + party_ref
Browser Profile Platform
  -> profiles, assignments, generations, sessions, certification, mailbox runtime
```

Migration/cutover sequence:

1. preserve standalone `client_id`;
2. add/verify `external_party_ref`;
3. consume versioned CRM Party projection/events through a CRM adapter;
4. reconcile/link standalone client and CRM Party;
5. switch authority for name/contact/status only after parity acceptance;
6. keep profile assignments in Profile Platform;
7. block standalone edits for CRM-owned fields or translate them into CRM commands;
8. optionally replace D1 catalog adapter with PostgreSQL/SQLx + RLS without changing domain/use-case contracts;
9. replace Cloudflare Access identity adapter with CRM OIDC adapter if required;
10. leave R2, Profile Bridge and browser lifecycle unchanged.

No direct CRM-table, R2, D1 or browser-filesystem coupling is allowed.

CRM UI route:

```text
CRM /clients/:partyId/browser-profiles
```

It consumes a versioned Profile Platform API projection: profiles, cloud status, active session, mailbox status, certification, permitted actions and recent audit projection.

Acceptance:

- linking a standalone client to CRM Party does not change profile IDs;
- CRM authority cutover is versioned, replayable and reversible until final promotion;
- profile/generation/session runtime remains independently operable;
- no R2 generation migration is required for CRM integration.

## Phase 10 — Production Evidence And Rollout

Repository code may prepare adapters, validators and runbooks, but actual production promotion still requires the external gates already tracked by issues #1 and #3 and the immutable external-evidence protocol.

This includes real Cloudflare resources, primary/secondary physical Windows evidence, trusted signing, key escrow restore, privacy/retention approval, product license, real fingerprint certification, production device-key unwrap, remote R2/D1 atomicity and independent security/cryptographic review.

## 8. Standalone Ownership Before CRM

Until CRM cutover, authoritative ownership remains:

| Data | Authoritative owner |
|---|---|
| Users / memberships / grants | Standalone Identity & Access |
| Client cards | Standalone Client Registry in D1 |
| Profiles / generation metadata | Profile Catalog in D1 |
| Encrypted generation objects | R2 |
| Sessions / leases / fencing | per-profile Durable Objects |
| Mailbox bindings / jobs / results | Mailbox Operations in D1 |
| Audit / outbox / notification history | D1 |
| Realtime connections | per-user `UserNotificationHub` Durable Objects |
| Device-local runtime state | Profile Bridge local encrypted workspace + SQLite |

The first production deployment remains one organization with multiple users. Owner sees organization resources according to owner policy; members see only live grants.

## 9. Required Architecture Gates For Future PRs

Every new capability PR must satisfy applicable gates:

1. **Layer gate:** no outward dependency from domain/use-cases.
2. **Worker-thinness gate:** protocol handlers do not own D1 mutation workflow logic.
3. **Contract gate:** OpenAPI/event/bridge contract changes are versioned and compatibility checked.
4. **Frontend generation gate:** generated types/client are clean after regeneration.
5. **Tenant/IDOR gate:** foreign and absent resources remain neutral; query/search results are pre-filtered.
6. **Idempotency gate:** duplicate Queue/HTTP/device result does not duplicate logical side effects.
7. **Transaction gate:** canonical state + audit/outbox remain atomic within one D1 boundary.
8. **Secret/PII gate:** events/logs/audit/realtime/support artifacts reject prohibited payloads.
9. **Failure-order gate:** external side effects occur only after the durable state transition that authorizes them.
10. **Exact-head gate:** every permanent workflow green on the same final head before merge.
11. **Evidence scope gate:** synthetic/local tests never promote external production claims.
12. **Generation freshness gate:** Bridge writer launch and generation activation are bound to the authoritative active generation plus valid lease/fencing; stale local state cannot overwrite a newer cloud generation.

## 10. Developer Workflow And Documentation Rules

A developer should be able to answer “where does this change belong?” without searching the whole repository.

Decision table:

| Change | Owner |
|---|---|
| provider-independent state invariant | `*-domain` |
| workflow across repositories/providers | `use-cases/<capability>` |
| interface required by a workflow | `application-ports/<capability>` |
| D1/R2/Queue/DO/Access implementation | `cloudflare-adapters` |
| HTTP/Queue/Scheduled/WebSocket ingress mapping | `apps/control-plane-worker` |
| Windows process/filesystem/device implementation | Bridge/windows adapter boundary |
| display/navigation/query invalidation | frontend feature/shared layer |
| public wire shape | versioned contract |
| cross-system CRM mapping | CRM adapter + versioned projection contract |

Documentation discipline:

- `DEVELOPMENT_PLAN.md` = what is next and in what order;
- `DEVELOPER_CAPABILITY_MATRIX.md` = what is actually implemented/accepted;
- `ARCHITECTURE.md` = stable boundaries and dependency rules;
- ADR = changed invariant/architecture decision;
- evidence/status = only what was actually proven;
- no duplicate roadmap is allowed to silently redefine execution order.

## 11. Recommended PR Slicing

Do not implement a phase as one giant PR. Prefer bounded acceptance slices:

- Phase 0: ports/use-case modules -> Worker orchestration extraction -> generated frontend contracts -> documentation/architecture policy consolidation;
- Phase 1: event contract/domain -> D1 migration -> outbox dispatcher/idempotent consumer -> catch-up query;
- Phase 2: client aggregate/contact crypto -> D1 adapter -> assignment/reassignment -> API/UI;
- Phase 3: search projection schema -> query service -> API -> UI;
- Phase 4: mailbox domain state expansion -> cloud runtime adapter contract -> scheduler/Queue consumer;
- Phase 5: durable device jobs -> Bridge claim/result -> synthetic Camoufox mailbox mode;
- Phase 6: notification hub DO -> WebSocket auth/reauth -> catch-up/reconnect -> frontend realtime state;
- Phase 7: routes/features/details incrementally by bounded context;
- Phase 8: cross-component acceptance only after component gates are stable;
- Phase 9: CRM contracts first, adapter/cutover second.

Each slice gets its own issue, branch, exact-head CI acceptance and squash merge.

## 12. Final Product Definition Of Done

The expanded standalone + CRM-ready architecture is complete only when all of the following are accepted at their correct evidence level:

- owner creates and finds a client through UI;
- client relates to multiple profiles;
- a profile has no more than one active primary client assignment;
- member cannot see ungranted profiles or client cards;
- search never reveals forbidden resources;
- contact PII is encrypted and exact lookup uses tenant-keyed HMAC;
- Gmail/IMAP Worker and Camoufox mailbox worker implement one job contract;
- mailbox result is durable before realtime notification;
- online UI receives safe change notification within the accepted latency SLO;
- offline UI catches up after reconnect;
- duplicate Queue delivery creates no duplicate result/counter/event;
- membership revoke closes realtime sessions and denies new commands;
- Bridge offline state preserves mailbox work as `PENDING_DEVICE`;
- WebSocket is never used as the local Camoufox command channel;
- UI never shows `Saved` before authoritative commit;
- D1 is authoritative for `active_generation_id`; R2 generations are encrypted and immutable; Bridge local profiles are materializations/cache/workspaces rather than cross-device authority;
- Bridge verifies generation freshness through the control plane before writer launch and materializes the authoritative generation when local state is stale;
- a changed browser session creates a new generation and only verified R2 upload plus fenced D1 compare-and-set can activate it;
- R2/network failure never discards unsynced dirty local state, and stale devices/sessions cannot overwrite a newer active generation;
- standalone `client_id` can link to CRM Party without changing profile IDs;
- CRM integration does not require R2-generation migration or browser-lifecycle rewrite;
- all external production gates are separately satisfied with real reviewable evidence.

## 13. First Execution Slice

The next implementation issue should be **Phase 0 / Application Boundary Convergence**. It should not add product features. Its bounded goal is to move one representative capability (recommended: client/profile ACL query+mutation path) from Worker-owned orchestration into application use-case services using ports, then add a permanent rule that prevents the old direct-adapter pattern from spreading.

After that pattern is accepted, migrate mailbox and generation handlers using the same architecture. Only then start Phase 1 event/notification persistence.

## 14. External Technical References

- Cloudflare Durable Objects WebSockets / Hibernation: https://developers.cloudflare.com/durable-objects/best-practices/websockets/
- Cloudflare Queues delivery guarantees: https://developers.cloudflare.com/queues/reference/delivery-guarantees/
- Cloudflare D1 index guidance: https://developers.cloudflare.com/d1/best-practices/use-indexes/

These references validate platform capabilities only. They do not constitute deployment or production evidence for this repository.