# Browser Profile Platform — Development Plan

**Status:** normative post-composition execution plan  
**Date:** 2026-08-08  
**Tracking:** Phase 1A accepted via #114/#115; Phase 0 complete; next sequential slice Phase 2A; Phase 1B eligible dependency-independently; plan consolidation history #96
**Production readiness:** unchanged; `production_ready=false` until external evidence gates are satisfied

## 1. Authority And Scope

This document is the **single normative source for post-composition execution order**.
It defines what comes next, which work must precede other work, and the acceptance
conditions for each phase.

Authority is intentionally separated:

- `DEVELOPMENT_PLAN.md` — execution order and phase acceptance;
- `ARCHITECTURE.md` + accepted ADRs — stable boundaries and architecture invariants;
- `DATA_CLASSIFICATION.md` — data sensitivity, storage and disclosure rules;
- `UI_ARCHITECTURE.md` — normative standalone product/UI target;
- `DEVELOPER_CAPABILITY_MATRIX.md` — what is actually Composed, Library, Synthetic, Target or External on accepted `main`;
- `DELIVERY_ROADMAP.md` — historical Repository Steps 0–10 and their acceptance record;
- `IMPLEMENTATION_PLAN.md` / lifecycle plans — design baseline and historical planning input, not current execution order.

If another roadmap or historical plan conflicts with this document on **what to do next**,
this document wins. If this document conflicts with an accepted architecture/security ADR
on an invariant, the invariant document wins and this plan must be corrected.

A planned item is never an implementation claim. External/provider/physical-host claims
remain External until the existing evidence process accepts real evidence.

## 2. Current Accepted Baseline And Active Slice

Repository Steps 0–10 and the accepted post-composition slices through **Phase 1A** provide
the current code baseline: typed domain/application boundaries, governed D1 writes,
profile generations, application-thin coordinator ingress, the first real application Cargo
boundary (`use-cases-identity`), synthetic Bridge/runtime lanes, mailbox metadata/jobs, React
composition, deterministic generated public frontend contracts, enforced frontend feature
boundaries, capability-owned fail-closed route classification, a deterministic machine-readable
architecture inventory, a versioned durable integration-event/outbox substrate with replay-safe
notification persistence and exact-head cross-component acceptance.

Phase 0L was accepted through issue/PR **#104** with guarded squash merge
`f26528f0f99d69a24ae1c4c307c1f3458ef64e05`. Identity governance plus verified-identity
ceremonies now compile/test independently in `use-cases-identity`; `identity_acl` deliberately
remains with cross-client/profile query helpers because moving it would create a false
identity-only boundary.

Phase 0M was accepted through issue **#106** / PR **#107** with guarded squash merge
`ada3a88a0ff8b995047fd20ae8b6b8ded837a753` from exact proven source head
`6c2f6c170ed90595ac50436191a79eb77d5d8c5d`. The existing `control-plane-contract` crate now owns
the migrated session/client/problem/mutation public Rust transport contracts; deterministic
OpenAPI and TypeScript artifacts are committed and regeneration is fail-closed. Real frontend
API surfaces consume those generated types, and permanent policy rejects direct sibling-feature
internal imports plus TypeScript/Vite alias escape hatches.

Phase 0N was accepted through issue **#110** / PR **#111** with guarded squash merge
`851a3b928fcd7b806f32cc32e2684ca5307d0114` from exact proven source head
`a2a5892daa5a8625e125e619c1f2d9944f567ebe`. Public `RouteClass` and Worker dispatch remained
stable while route matching moved into capability-owned classifiers behind one composed
fail-closed entrypoint. Unknown `/api/*`, `/auth/*` and `/bridge/*` variants cannot reach SPA
assets. `architecture/inventory.json` is deterministically derived/checkable for workspace
members, contiguous D1 migrations, route/classifier ownership, generated public contracts and
documentation authority; stale/tampered/missing inventory and selected documentation drift are
permanently rejected by preflight and CI.

Phase 1A was accepted through issue **#114** / PR **#115** with guarded squash merge
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5` from exact proven source head
`21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`. The accepted foundation versions the integration
event envelope, evolves the existing durable outbox, persists metadata-only notification events,
adds tenant/consumer/outbox idempotency, dispatches through the Queue adapter and keeps Queue/
scheduled ingress application-thin. Canonical-source guards reject forged event metadata/payload,
prohibited PII/secrets/mail bodies fail closed before persistence, and duplicate accepted delivery
has no duplicate logical effect. Phase 1B retry/backoff/DLQ/catch-up/retention remains unimplemented.

**Phase 0 remains complete on accepted `main`, and Phase 1A is accepted.** The **next planned
sequential slice is Phase 2A — client aggregate and contact crypto foundation**, starting from the
accepted Phase 1A `main` in its own bounded issue/branch/PR. Phase 1B is eligible to proceed
only dependency-independently and must finish before real asynchronous provider/device execution
in Phases 4–5 and before Phase 6 realtime.

### 2.1 Critical-path execution policy

Development optimizes for the shortest **safe path to accepted product capability**, not for
the maximum number of concurrent branches or the largest refactor.

Rules:

- one sequential architecture/governance slice is active at a time when slices touch the same
  Worker/governed-write/application boundary;
- dependency-independent work may proceed in parallel only when it cannot create competing
  edits or invalidate the active slice baseline;
- every push runs `python scripts/verify-fast.py`; boundary switches/final acceptance also run
  `python scripts/verify-fast.py --with-compile` before expensive full CI;
- use the permanent workflow matrix for acceptance, not as an interactive formatter/compiler;
- Cargo/application crate extraction is **just in time**: split a capability when current or
  immediately upcoming growth benefits from compile-time isolation, not as speculative churn;
- frontend capability ships incrementally with the backend/query contract that enables it;
  Phase 7 is completion/polish, not a big-bang frontend start;
- long-lead External work (Cloudflare environments, Windows hosts/signing, key recovery,
  privacy/license/security review) runs as a parallel operational workstream from now onward,
  while production promotion remains Phase 10 and still requires accepted evidence.

## 3. Non-Negotiable Architecture Rules

### 3.1 Dependency direction

The allowed direction is inward:

```text
primitives
contracts -> primitives
domains -> contracts + primitives
application-ports -> domains + contracts + primitives
use-cases-* -> application-ports + domains + contracts + primitives
adapters -> application-ports + domains + contracts + primitives + provider SDKs
apps -> use-cases-* + adapters + contracts + primitives
frontend -> generated public contracts + frontend shared/entities/feature public APIs
```

Outer adapters may depend on inner domain types. Inner layers may never depend on
Cloudflare, Windows, React, D1/R2/DO implementations or other outer runtime SDKs.

### 3.2 Worker/ingress thinness

Ordinary HTTP/Queue/Scheduled/DO ingress owns only protocol work:

```text
parse/authenticate transport
  -> construct verified request context
  -> call one application command/query
  -> map typed result/problem to protocol
```

Application use cases own authorization intent, idempotency/replay semantics, aggregate
version sequencing, repository ordering and outbox intent. Concrete D1 mutation/repository
types stay in adapters/composition.

### 3.3 Domain ownership

- `identity-access-domain` owns tenant owner, membership/grant decisions;
- `client-domain` owns client/contact/assignment invariants;
- `profile-domain` owns profile/generation lifecycle policy;
- `session-domain` owns launch intent, lease epoch, fencing, session/recovery state;
- `mailbox-domain` owns provider-neutral mailbox binding/job/runtime-lane state;
- notification/search/CRM domains or pure value modules are introduced only where real
  provider-independent state exists.

**Do not move lease/fencing/session semantics into `profile-domain`.** A per-profile Durable
Object is an outer runtime coordinator for the `session-domain` state machine, not a second
profile catalog.

### 3.4 D1 / Durable Object / R2 ownership

```text
D1
  -> authoritative business/catalog metadata
  -> active_generation_id + aggregate versions
  -> audit/outbox/read projections

per-profile Durable Object
  -> lease/session serialization
  -> monotonic fencing epoch/token
  -> minimal recoverable coordination state

R2
  -> encrypted immutable generation objects/evidence objects

Windows Profile Bridge
  -> local encrypted staging/materialization/cache/workspace
  -> native process/browser lifecycle
```

D1, DO and R2 do not form a distributed transaction. Generation publication uses immutable
object creation, verification, fenced D1 compare-and-set, idempotency and reconciliation.
Stale writers/devices cannot overwrite a newer active generation. Failed upload/verification
cannot discard `DIRTY_LOCAL` state.

### 3.5 Authorization-before-projection/fetch

Tenant scope and live membership/grants are applied **before** constructing list/search/detail
results and before provider message-body retrieval. “Fetch everything, then hide in React” is
forbidden. Missing and unauthorized resources remain disclosure-neutral where the public
contract is neutral.

### 3.6 Durable-before-notify

```text
validated command
  -> durable canonical mutation
  -> audit + outbox in the same durable boundary where possible
  -> dispatcher / Queue
  -> notification delivery
```

Realtime events are change signals, not authority. UI refetches canonical projections.

### 3.7 PII, secrets and mailbox content

- contact display values are encrypted at rest;
- exact contact lookup uses tenant-keyed HMAC tokens;
- fuzzy/prefix PII indexes require an explicit privacy/security ADR;
- mailbox message metadata/body is authorized `CONFIDENTIAL` product content;
- full body may be displayed to an authorized user but never enters ordinary logs, audit,
  metrics, realtime/integration events or support bundles;
- message body is not persisted in browser Web Storage;
- HTML mail is sanitized/sandboxed; remote images/active content are disabled by default;
- attachments are a separate capability requiring explicit access/content-handling policy.

## 4. Phase 0 — Architecture Convergence And Developer DX

**Goal:** finish executable clean boundaries before feature expansion.

Phase 0 is intentionally split into bounded slices. Each slice preserves public behavior
unless its own issue explicitly changes a contract.

### Phase 0H — Profile grant application boundary — ACCEPTED

Move only profile grant/revoke orchestration from legacy Worker governance into the profile
application boundary.

Required outcome:

- pure profile grant ports/use cases;
- D1 implementation behind the profile application adapter;
- live `ProfileGrantApi` routed through thin `profiles.rs` only after inward native/WASM proof;
- legacy fallback removed only after the switched Worker path is proven;
- permanent positive/negative boundary, capability-layout, governed-write and
  cross-component evidence updated;
- assignment remains non-authorizing;
- no unrelated client-grant/identity-lifecycle changes.

Acceptance is exactly the bounded issue #92 discipline, including one unchanged final head,
12 permanent workflows green, `behind_by=0`, bounded diff, no unexplained `Cargo.lock`
change and zero blocking/unresolved reviews.

### Phase 0I — Client grant application boundary — ACCEPTED

Move `ClientGrantApi` grant/revoke orchestration out of legacy Worker governance using the
accepted application-boundary pattern.

Keep this slice separate from identity lifecycle. Preserve owner authorization, neutral
disclosure, idempotency domains, checked versions, D1 atomicity and stable public problems.

### Phase 0J — Identity governance lifecycle application boundary — ACCEPTED

Move remaining owner/bootstrap/transfer, invitation create/accept and membership
status/revoke orchestration behind identity application services.

Requirements:

- identity domain remains authoritative for owner/membership/grant rules;
- transport cannot assemble D1 identity mutations directly;
- owner-transfer ceremony and single-active-owner invariant are unchanged;
- invitation/membership state transitions remain idempotent/fail-closed;
- no UI-only authorization decisions.

### Phase 0K — Profile coordinator ingress thinness — ACCEPTED

Clean the remaining thick coordinator ingress/DO composition boundary.

Target:

- HTTP/DO ingress maps protocol and constructs adapters only;
- application/session use case owns orchestration across coordinator projection/storage ports;
- `session-domain` continues to own lease/fencing/session transitions;
- D1 remains authoritative catalog/projection storage;
- DO does not accumulate client/profile catalog business rules.

This slice must not redesign the proven coordinator state machine merely to move code.

### Phase 0L — Just-in-time application Cargo boundaries — ACCEPTED

The current capability modules inside one `crates/use-cases` crate are not the final growth
boundary. Establish the first independent application crates where the dependency graph and
immediately upcoming growth justify compile-time isolation, then continue extracting later
capabilities just in time rather than performing one speculative all-capabilities migration.

Expected growth direction remains:

```text
use-cases-identity
use-cases-clients
use-cases-profiles
use-cases-mailboxes
```

but only the contexts with demonstrated dependency/growth pressure are mandatory in the first
0L slice. Later phases add or extract notification/search/device/CRM application contexts only
when those capabilities exist.

Rules:

- do not create one crate per function;
- do not split a capability merely to satisfy a naming target;
- shared neutral evidence/value/contracts remain in primitives/contracts/application-ports;
- a temporary compatibility facade may re-export during migration;
- no circular capability dependencies;
- provider SDKs remain outside all use-case crates;
- extracted capability crates compile/test independently.

`application-ports` may remain one Cargo crate with capability modules while that keeps a
clear dependency graph; split it into multiple crates only if actual dependency pressure
justifies the added surface.

### Phase 0M — Generated frontend contracts and feature-boundary enforcement — ACCEPTED

Accepted implementation:

- `control-plane-contract` owns the migrated canonical public Rust DTO/schema source for the
  session/client/problem/mutation vertical slice; live Worker session/client transports use it;
- Rust deterministically exports `contracts/generated/control-plane.openapi.json`, and the
  repository-owned pinned-toolchain generator deterministically renders
  `frontend/src/shared/api/generated/control-plane.ts` with explicit `DO NOT EDIT` ownership;
- real frontend session/client/problem/mutation surfaces consume generated types and migrated
  handwritten duplicate DTO/enums are removed;
- `python scripts/generate-frontend-contracts.py --check` makes regeneration drift fail closed in
  fast preflight and permanent Quality Gate;
- frontend feature policy rejects direct sibling-feature internals through alternate relative
  paths and fails closed on TypeScript/Vite resolver aliases until explicitly understood;
- positive repository checks plus sibling-internal and alias-bypass negative fixtures are
  permanent Frontend/Quality/Repository Quality evidence;
- acceptance used exact source head `6c2f6c170ed90595ac50436191a79eb77d5d8c5d`, 12/12 permanent
  workflows green, `behind_by=0`, zero blocking reviews/threads and guarded squash merge #107.

### Phase 0N — Route classifier, architecture inventory and documentation consistency — ACCEPTED

Accepted implementation:

- public `RouteClass` and Worker dispatch remain stable while route matching is split into
  capability-owned `foundation`, `identity`, `clients`, `profiles`, `generations` and `mailboxes`
  classifier modules behind one composed `classify_route` entrypoint;
- composition remains fail closed: unknown versions/routes/wrong methods under `/api/*` and
  `/auth/*` resolve to dynamic-not-found, while `/bridge` and `/bridge/*` remain denied by default;
  these namespaces cannot fall through to static SPA assets;
- `architecture/inventory.json` is committed deterministic machine-readable evidence for Cargo
  workspace members, contiguous D1 migrations, public route/classifier ownership, generated public
  contracts and documentation authority;
- `scripts/generate-architecture-inventory.py --check` derives/checks repository truth and rejects
  missing paths, route ownership drift, multiple/misaligned `NEXT` documentation claims and
  production-readiness claim drift;
- a real negative harness proves stale, tampered and missing inventory are rejected;
- fast preflight plus permanent Quality and Repository Quality gates enforce inventory/docs
  consistency, and `docs/INDEX.md` indexes the machine-readable inventory without adding a roadmap;
- acceptance used exact source head `a2a5892daa5a8625e125e619c1f2d9944f567ebe`, 12/12 permanent
  workflows green, `behind_by=0`, zero blocking reviews/threads and guarded squash merge #111.

### Phase 0 completion gate

Phase 0 is complete only when all are true:

- ordinary Worker/DO transports do not own provider/D1 business orchestration;
- remaining legacy governance routes have bounded application owners;
- coordinator ingress is thin without moving session semantics to the wrong domain;
- current high-growth use-case contexts have real Cargo isolation where justified;
- generated public TS contracts are CI-enforced;
- frontend sibling-feature boundaries are CI-enforced;
- route classification remains fail-closed and modular;
- architecture/docs inventory is consistent;
- all permanent workflows are green on the exact accepted head.

## 5. Phase 1 — Integration Events, Outbox And Notification Persistence

**Goal:** establish one durable event/outbox substrate before richer registry, mailbox and
realtime behavior depend on it, while avoiding unnecessary blocking of independent product
work once the safe foundation exists.

### Phase 1A — Durable event/outbox foundation — ACCEPTED

Accepted implementation:

- versioned integration event envelope;
- evolved `outbox_events` and minimal notification-event persistence required by the contract;
- outbox dispatcher and Queue adapter;
- idempotent consumer registry / `consumer_idempotency` as required;
- payload sanitizer enforcing the existing PII/secret/content policy;
- duplicate-delivery neutrality.

Acceptance:

- canonical mutation + audit/outbox remain atomic within their D1 boundary;
- duplicate delivery has no duplicate logical effect;
- prohibited PII/secrets/mail bodies are rejected from event payloads;
- consumer processing is replay-safe for the accepted event set.

Acceptance used exact source head `21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`, 12/12 permanent
workflows green, `behind_by=0`, zero blocking reviews/threads and guarded squash merge #115
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5`.

With Phase 1A accepted, **Phase 2 may begin** because Client Registry expansion only needs the
durable event/outbox contract. Phase 1B may proceed in parallel when it does not overlap with
the active Phase 2 files/contracts; it remains mandatory before real asynchronous Phases 4–5
and before Phase 6 realtime.

### Phase 1B — Delivery hardening, catch-up and operations

Complete before real asynchronous provider/device execution in Phases 4–5 and before realtime
Phase 6:

- `notification_deliveries`, `user_event_cursors` and authorized catch-up as required;
- deterministic attempt accounting;
- exponential backoff with bounded jitter;
- maximum automatic attempts;
- DLQ or equivalent terminal failure lane;
- sanitized alerting/operational visibility;
- operator-safe replay procedure;
- replay idempotency and auditability;
- bounded retention/compaction that never removes canonical business state.

Acceptance:

- poison messages reach DLQ/terminal state after the configured bound;
- retries do not hot-loop;
- replay after remediation cannot duplicate the logical effect;
- unauthorized users cannot query event history/catch-up;
- operational payloads remain sanitized.

## 6. Phase 2 — Client Registry 2.0 And Assignment Model

**Goal:** complete the standalone business client model before search and CRM integration.

### Phase 2A — Client aggregate and contact crypto foundation — NEXT

Start Phase 2 with the first bounded registry slice rather than the whole phase:

- provider-neutral client aggregate/value model for `PERSON|ORGANIZATION`, lifecycle status and
  versioned metadata;
- encrypted-at-rest contact display values and tenant-keyed HMAC exact-lookup tokens;
- no plaintext contact scan and no name/contact-derived technical identifiers;
- application-owned create/update/archive intent behind ports before transport wiring;
- additive D1 schema/adapter work only after inward native/WASM proof;
- assignment/merge lifecycle and wider API/UI projections remain later Phase 2 slices unless the
  bounded 2A issue proves they are required for the same invariant.

Phase 2A acceptance must retain Phase 1A durable mutation/audit/outbox semantics and all existing
authorization-before-projection, PII and generated-contract boundaries.

Client card target:

- opaque `client_id`, `PERSON|ORGANIZATION`;
- display name + optional legal name;
- `ACTIVE|ARCHIVED|MERGED`;
- country, locale, timezone, tags, structured notes;
- encrypted email/phone/URL contact points;
- optional opaque `external_party_ref`;
- aggregate version/timestamps;
- profile, mailbox, grant, assignment/activity/audit projections.

`ProfileClientAssignment` is historical/business association only. Reassignment closes the
previous active assignment, creates the new assignment and emits audit/outbox. Assignment
never grants profile/client access.

Acceptance:

- owner create/update/archive/merge through application services;
- one profile has at most one active primary client assignment;
- one client may have multiple profiles;
- member projections are grant-filtered;
- exact contact lookup does not require plaintext scan;
- no name/contact-derived technical IDs/paths/keys;
- Client Registry UI is incrementally usable for the accepted create/update/archive/assignment
  projections rather than deferred to Phase 7.

## 7. Phase 3 — Read Models, Global Search And Client Mail Query Contract

**Goal:** add explicit CQRS-lite read boundaries and safe discovery.

### 7.1 Read-model boundary

Lists/search/detail projections use application query services + typed read-model ports.
Mutation aggregates are not hydrated merely to render large tables/search results.
Authorization/grant filtering happens before projection construction.

### 7.2 Global search

Conceptual endpoint:

```text
GET /api/v1/search?q=<query>&types=client,profile,member,mailbox
```

Primary global result types:

- clients;
- profiles;
- members/users subject to owner/admin disclosure policy;
- mailbox binding/provider/status metadata.

Devices remain searchable/filterable inside the Devices administration screen but are not a
primary business global-search type.

Rules:

- typed tenant scope mandatory;
- stable cursor pagination and bounded cost;
- supported predicates are index-backed;
- R2/browser filesystem are never queried for catalog search;
- fuzzy/prefix PII search remains blocked without an accepted ADR.

### 7.3 Client-scoped mailbox message search

Provide provider-neutral application queries conceptually equivalent to:

- `SearchClientMailboxMessages`;
- `GetClientMailboxMessage`.

Primary UX:

```text
Client -> Mail -> search -> results -> open message -> full body
```

Authorized users can search a selected client's eligible mailbox bindings by at least:

- subject;
- sender;
- recipient;
- message body text;
- date/time filters.

Search results are bounded metadata/snippet projections. Opening a result fetches the full
authorized message body by opaque provider-scoped reference.

Mandatory order:

```text
authenticate actor
  -> tenant + live membership/grants
  -> authorize client/mailbox context
  -> resolve only eligible mailbox bindings
  -> provider/Bridge query adapter
  -> bounded result/body projection
```

The initial product does **not** require central D1 full-text storage, blind index or n-gram
index. Provider-native search/fetch is preferred where practical; an adapter may perform a
bounded internal search behind the same port. A future central/local index requires its own
storage/security/retention decision.

Phase 3 proves API/read-model/authorization semantics with deterministic fakes. Real provider
execution belongs to Phases 4–5. The matching search/client-mail UI is implemented against the
fake/query contract in this phase so provider work is not coupled to a later big-bang UI.

Acceptance:

- no cross-tenant/result-count leakage;
- revoked grants disappear according to the accepted consistency contract;
- provider search/body fetch is not called before authorization succeeds;
- a foreign message reference cannot bypass client/mailbox authorization;
- full body can be returned by the fake without entering logs/audit/events/telemetry.

## 8. Phase 4 — Mailbox Operations 2.0 And Cloud Provider Lane

**Goal:** implement real cloud-capable mailbox operations behind one provider-neutral model.

**Prerequisite:** Phase 1B delivery hardening is accepted before enabling real asynchronous
scheduled provider execution.

Mailbox job states:

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

Cloud lane includes Gmail API, IMAP and future standard provider APIs.

Canonical scheduled flow:

```text
Scheduled trigger
  -> Queue mailbox.check.requested
  -> mailbox policy/runtime selection
  -> provider execution
  -> D1: mailbox state + observation + audit + outbox
  -> notification pipeline
```

The cloud provider adapters also implement the Phase 3 message-query contract:

- subject/sender/recipient/body search where provider capabilities allow;
- bounded provider-neutral result mapping;
- selected full message-body fetch;
- no canonical D1/R2 body copy required;
- message content never enters ordinary audit/outbox/realtime payloads.

Mailbox scheduled checks and interactive message search are separate application use cases
even when they share a provider adapter.

Acceptance:

- duplicate Queue deliveries do not duplicate business results/counters/events;
- revoked/suspended binding cannot execute;
- retry/auth-required transitions are deterministic;
- provider fake/real integration contract proves search + body retrieval;
- unauthorized search/body fetch stops before provider access;
- real provider claims remain External until real evidence exists.

## 9. Phase 5 — Durable Device Jobs And Browser/Camoufox Mailbox Lane

**Goal:** support providers requiring an authorized browser profile without using the web
WebSocket as a device command channel.

**Prerequisite:** Phase 1B delivery hardening is accepted.

Target flow:

```text
Cloud Queue / interactive request
  -> durable job/request state
  -> device claim channel
  -> authenticated device-bound Profile Bridge
  -> current profile generation + lease/fencing
  -> certified Camoufox mailbox mode
  -> result/body projection
  -> D1 commit/outbox where mutation exists
```

Requirements:

- offline device -> explicit `PENDING_DEVICE`, never false empty/success;
- profile contention -> `PROFILE_BUSY`;
- claim/result are idempotent, leased and fencing-aware;
- stale device result is rejected after claim turnover;
- Bridge cannot claim arbitrary tenant jobs;
- browser lane implements the same `SearchClientMailboxMessages` /
  `GetClientMailboxMessage` contract as the cloud lane;
- challenge/fingerprint drift follows certification policy rather than silently changing
  browser mode.

Real Camoufox/physical-host behavior remains External until accepted evidence exists.

## 10. Phase 6 — Realtime UserNotificationHub

**Goal:** deliver low-latency safe change signals after durable event persistence and Phase 1B
catch-up/delivery hardening exist.

Topology:

```text
canonical state + outbox
  -> Queue dispatcher
  -> per-user UserNotificationHub Durable Object
  -> Hibernatable WebSocket
  -> React invalidation
  -> HTTPS refetch of canonical projection
```

Capabilities:

- multiple tabs/devices;
- reconnect with exponential backoff+jitter;
- cursor/catch-up after disconnect;
- session/mailbox/grant/device/cloud/certification change signals;
- membership revoke closes sockets and blocks reconnect;
- bounded reauthorization policy;
- no prohibited PII/secrets/message bodies in event envelopes.

Acceptance includes duplicate safety, catch-up, revocation and DO hibernation reconstruction
without in-memory-only security/cursor state.

## 11. Phase 7 — Complete Standalone UI

**Goal:** finish and polish the operator product on top of UI capabilities delivered
incrementally in Phases 2–6; do not defer ordinary usable frontend flows until this phase.

Primary information-architecture priority:

1. Clients / Profiles;
2. Users & Access;
3. Mail on the Client detail;
4. Mailboxes for provider/binding/job administration;
5. Devices for infrastructure administration.

Required routes include:

```text
/
/clients
/clients/:clientId
/profiles
/profiles/:profileId
/users
/mailboxes
/sessions
/devices
/audit
/settings
```

Client detail includes first-class `Mail`:

- message search and filters;
- bounded result list with subject/sender/time/snippet;
- full authorized message body viewer;
- explicit offline/auth-required/browser-lane-pending states;
- sanitized/sandboxed HTML, remote images/active content disabled by default;
- no body persistence in Web Storage/telemetry.

Users & Access provides member search/filter, invitation/membership/grant administration and
owner-transfer workflows. Devices remains a technical/admin screen.

Frontend rules:

- generated public API/event types only;
- TanStack Query owns remote state;
- realtime invalidates/refetches authoritative state;
- sibling feature internals cannot be imported;
- no optimistic success for grants, assignment, generation activation, session close,
  mailbox result or profile sync commits.

## 12. Phase 8 — Standalone End-To-End Acceptance

**Goal:** prove the expanded standalone composition without inflating External claims.

Repository-local/synthetic acceptance includes:

- owner creates/finds a client;
- one client relates to multiple profiles and one profile has at most one active primary
  client assignment;
- member cannot see ungranted clients/profiles/messages;
- owner/admin can find a member and inspect allowed grant projection;
- authorized user searches client mail by subject/sender/body and opens full body;
- foreign client/message reference cannot disclose result/body;
- body absent from log/audit/outbox/realtime/telemetry evidence;
- cloud and browser mailbox fakes satisfy the same application contracts;
- duplicate Queue delivery is idempotent;
- realtime reconnect catches up missed events;
- membership revoke blocks commands/realtime;
- offline device jobs remain durable;
- generation freshness/fencing/R2 failure invariants remain green;
- all permanent workflows run on one exact final head.

Real provider/physical-host claims remain External.

## 13. Phase 9 — CRM Boundary And Party Integration

**Goal:** allow CRM to become authoritative for Party/client master data without coupling
browser runtime internals to CRM.

After cutover:

```text
CRM Party/Customer Master
  -> canonical name/contact/status + party_ref

Browser Profile Platform
  -> profiles, assignments, generations, sessions, certification, mailbox runtime
```

Migration sequence:

1. preserve standalone `client_id`;
2. add/verify opaque `external_party_ref`;
3. consume versioned CRM Party projection/events through a CRM adapter;
4. reconcile/link standalone client and CRM Party;
5. switch authority for name/contact/status only after parity acceptance;
6. keep profile assignments/runtime ownership in Profile Platform;
7. block local edits to CRM-owned fields or translate explicit commands through the CRM
   adapter;
8. optionally replace D1 catalog adapter with PostgreSQL/SQLx + RLS without changing
   domain/application contracts;
9. optionally replace Access identity adapter with CRM OIDC;
10. leave R2 generations and Profile Bridge lifecycle independent.

Integration is **event/contract isolated and async-first**, not dogmatically “100% async”.
Durable events/projections are the default synchronization mechanism, while a user-triggered
HTTP command may still return a synchronous acknowledgement/result. Core domain/application
code never imports CRM tables/entities/SDK implementation details.

## 14. Phase 10 — Production Evidence And Rollout

Production promotion remains Phase 10, but the underlying long-lead External work is a
**parallel operational workstream beginning immediately**. Repository implementation must not
wait until Phase 10 to request/provision resources whose lead time could block release.

Code may prepare adapters, validators and runbooks, while the parallel workstream advances real
accepted evidence for the existing external gates, including as applicable:

- revoke/rotate and verify any known exposed legacy credential before prototype reuse;
- isolated Cloudflare resources/budgets;
- trusted Windows signing/update channel;
- primary/secondary physical Windows evidence;
- key escrow/restore procedure;
- privacy/retention approval;
- product/license decisions;
- real provider/fingerprint certification;
- production device-key protection/unwrap;
- remote R2/D1/DO recovery behavior;
- independent security/cryptographic review.

`production_ready` remains `false` until those gates are satisfied with real reviewable
evidence. Parallel preparation or provisioning never changes an External evidence claim by
itself.

## 15. Architecture Gates For Every Future PR

Every applicable capability PR must satisfy:

1. **Fast-preflight gate** — locally reproducible formatting/policy/compile failures are caught
   before expensive full CI (`scripts/verify-fast.py`, plus `--with-compile` where applicable).
2. **Layer gate** — no outward dependency from domain/application code.
3. **Transport-thinness gate** — ordinary ingress does not own provider/D1 orchestration.
4. **Contract gate** — public API/event/bridge changes are versioned/compatibility checked.
5. **Frontend generation gate** — generated contracts are deterministic and clean.
6. **Frontend feature gate** — sibling-feature internals cannot be imported.
7. **Tenant/IDOR gate** — authorization occurs before projection/provider fetch.
8. **Idempotency gate** — duplicate HTTP/Queue/device result has no duplicate logical effect.
9. **Transaction gate** — canonical D1 mutation + audit/outbox are atomic within one D1
   boundary.
10. **Secret/PII/content gate** — prohibited payloads never enter logs/events/audit/support.
11. **Failure-order gate** — external side effects follow the durable transition that
    authorizes them.
12. **Generation freshness gate** — active generation + lease/fencing controls writer launch
    and activation.
13. **Exact-head gate** — all permanent workflows green on one unchanged final head.
14. **Review gate** — zero blocking reviews/unresolved threads before merge.
15. **Evidence-scope gate** — synthetic/local evidence never promotes External claims.

For architecture-boundary migrations use the proven fail-safe switch discipline:

1. add inward port/use case and adapter;
2. run fast preflight and prove native/WASM inward behavior;
3. switch live transport while retaining fallback;
4. prove post-switch native/WASM behavior;
5. remove only superseded fallback;
6. make permanent policy/docs reflect the proven final ownership;
7. synchronize to current `main`, run fast preflight, then exact-head full acceptance;
8. guarded squash merge with expected head SHA.

## 16. Documentation And Developer Workflow

A developer should be able to determine ownership without repository-wide guessing:

| Change | Owner |
|---|---|
| provider-independent invariant | appropriate `*-domain` |
| application workflow | capability use-case crate/module |
| port required by workflow | capability-owned `application-ports` module |
| D1/R2/Queue/DO/Access implementation | adapter layer |
| HTTP/Queue/Scheduled/DO/WebSocket mapping | app ingress/composition |
| Windows filesystem/process/device behavior | Bridge/windows adapter boundary |
| display/navigation/query invalidation | frontend feature/shared layer |
| public wire shape | versioned contract |
| CRM mapping | CRM adapter + versioned integration contract |

Documentation discipline:

- no parallel normative execution roadmap;
- every accepted capability PR updates `DEVELOPER_CAPABILITY_MATRIX.md` only for claims it
  actually changes;
- invariant changes require ADR/architecture update before implementation acceptance;
- `docs/INDEX.md` must classify new normative/historical/evidence documents;
- machine-checkable claims should be enforced by CI rather than prose alone.

Development-loop discipline is defined in `CONTRIBUTING.md`; this plan defines sequencing.
Where possible, cheap deterministic checks run before push and full permanent CI runs only on a
head intended to advance acceptance.

## 17. Recommended PR Slicing

Do not implement a whole phase in one PR. Preferred order:

```text
Phase 0H profile grant
  -> 0I client grant
  -> 0J identity governance lifecycle
  -> 0K coordinator ingress
  -> 0L first justified use-case Cargo extraction(s)
  -> 0M generated TS + frontend feature boundary
  -> 0N route classifier + architecture/docs inventory

Phase 1A event contract/persistence foundation
  -> dispatcher/idempotent consumer

Phase 2 client aggregate/contact crypto
  -> D1 adapter
  -> merge/assignment lifecycle
  -> API + incremental UI projections

Phase 1B may proceed dependency-independently after 1A
  -> retry/max-attempt/DLQ/replay
  -> authorized catch-up/retention
  -> must finish before real Phase 4/5 async execution and Phase 6 realtime

Phase 3 read-model schema
  -> global query service
  -> client-mail query contracts/fakes
  -> API + incremental UI integration

Phase 4 cloud mailbox provider contract
  -> scheduler/Queue
  -> provider message search/body

Phase 5 durable device jobs
  -> Bridge claim/result
  -> browser-lane message search/body

Phase 6 notification hub
  -> auth/reauth
  -> catch-up/reconnect
  -> frontend realtime

Phase 7 complete/polish remaining UI capability gaps
Phase 8 cross-component acceptance
Phase 9 CRM contracts/projection before cutover adapters
Phase 10 production promotion over evidence advanced in the parallel External workstream
```

Each sequential slice gets its own issue, branch, bounded diff, exact-head CI and guarded squash
merge. Parallel work must be dependency-independent and must not turn multiple stale drafts into
the critical path.

## 18. Final Product Definition Of Done

The standalone + CRM-ready target is complete only when, at the correct evidence level:

- clean application/adapter boundaries are enforced in code and CI;
- client/profile/user/mailbox search is grant-safe;
- authorized users can search a client's messages and read the full body;
- mailbox content remains outside ordinary telemetry/audit/event payloads;
- Client Registry 2.0 and profile-client assignment semantics are complete;
- cloud and browser mailbox lanes share provider-neutral application contracts;
- durable jobs/retries/DLQ/replay are operationally safe;
- realtime is durable-event-backed and never authoritative itself;
- complete UI works without CLI for ordinary product/admin workflows;
- D1 is authoritative for catalog pointers, DO for session coordination, R2 for immutable
  encrypted objects and Bridge for local runtime/materialization;
- stale devices/sessions cannot overwrite newer generations;
- standalone `client_id` links to CRM Party without changing profile IDs;
- CRM integration requires no R2 generation migration or browser lifecycle rewrite;
- all External production gates are separately satisfied with real evidence.

## 19. Immediate Next Action

Start **Phase 2A — client aggregate and contact crypto foundation** from the accepted Phase 1A
`main`. Create a fresh bounded issue/branch/PR before implementation; do not fold Phase 2A into the
completed #114/#115 history. Phase 1B delivery hardening may proceed only dependency-independently
and must not be mixed into the first Client Registry slice.

Primary Phase 2A acceptance target:

```text
provider-neutral client aggregate + lifecycle values
  -> encrypted contact display values
  -> tenant-keyed HMAC exact-contact lookup tokens
  -> no plaintext contact scan or PII-derived technical identifiers
  -> application-owned create/update/archive intent behind ports
  -> additive D1 adapter/schema only after inward proof
  -> canonical mutation + audit/outbox remains atomic
  -> exact-head permanent CI + guarded merge
```

Keep all accepted Phase 0 and Phase 1A boundaries, generated-contract/feature-boundary rules and
architecture inventory checks intact. Continue long-lead External gate preparation in parallel
without changing `production_ready=false`; real provider/physical-host evidence remains External.
