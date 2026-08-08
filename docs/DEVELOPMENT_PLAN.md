# Browser Profile Platform — Development Plan

**Status:** normative post-composition execution plan  
**Date:** 2026-08-08  
**Tracking:** issue #96  
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

Repository Steps 0–10 and the accepted post-composition slices through **Phase 0G** provide
the current code baseline: typed domain/application boundaries, governed D1 writes,
profile generations, coordinator/session foundations, synthetic Bridge/runtime lanes,
mailbox metadata/jobs, React composition and exact-head cross-component acceptance.

The current unfinished architecture-convergence slice is:

- **Phase 0H — Profile grant application boundary**;
- tracking issue **#92**;
- draft PR **#93**.

Phase 0H is **not accepted or merged** until its actual final diff, exact-head CI and merge
evidence satisfy its issue acceptance. PR descriptions are not implementation evidence.
Before merge the branch must be synchronized to the latest `main` and end at
`behind_by=0`.

After 0H, continue the Phase 0 sequence below. Do not begin Phase 1 product expansion
while Phase 0 acceptance remains incomplete.

## 3. Non-Negotiable Architecture Rules

### 3.1 Dependency direction

The allowed direction is inward:

```text
primitives
contracts -> primitives
domains -> contracts + primitives
application-ports -> domains + contracts + primitives
use-cases -> application-ports + domains + contracts + primitives
adapters -> application-ports + domains + contracts + primitives + provider SDKs
apps -> use-cases + adapters + contracts + primitives
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

### Phase 0H — Profile grant application boundary — ACTIVE

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

### Phase 0I — Client grant application boundary

Move `ClientGrantApi` grant/revoke orchestration out of legacy Worker governance using the
accepted application-boundary pattern.

Keep this slice separate from identity lifecycle. Preserve owner authorization, neutral
disclosure, idempotency domains, checked versions, D1 atomicity and stable public problems.

### Phase 0J — Identity governance lifecycle application boundary

Move remaining owner/bootstrap/transfer, invitation create/accept and membership
status/revoke orchestration behind identity application services.

Requirements:

- identity domain remains authoritative for owner/membership/grant rules;
- transport cannot assemble D1 identity mutations directly;
- owner-transfer ceremony and single-active-owner invariant are unchanged;
- invitation/membership state transitions remain idempotent/fail-closed;
- no UI-only authorization decisions.

### Phase 0K — Profile coordinator ingress thinness

Clean the remaining thick coordinator ingress/DO composition boundary.

Target:

- HTTP/DO ingress maps protocol and constructs adapters only;
- application/session use case owns orchestration across coordinator projection/storage ports;
- `session-domain` continues to own lease/fencing/session transitions;
- D1 remains authoritative catalog/projection storage;
- DO does not accumulate client/profile catalog business rules.

This slice must not redesign the proven coordinator state machine merely to move code.

### Phase 0L — Real application Cargo boundaries

The current capability modules inside one `crates/use-cases` crate are not the final growth
boundary. Before Phase 1/3 growth, split independent high-growth application contexts into
workspace crates where the dependency graph benefits from compile-time isolation.

Initial direction:

```text
use-cases-identity
use-cases-clients
use-cases-profiles
use-cases-mailboxes
```

Later phases add independent crates such as notification/search/device/CRM projection only
when those capabilities exist.

Rules:

- do not create one crate per function;
- shared neutral evidence/value/contracts remain in primitives/contracts/application-ports;
- a temporary compatibility facade may re-export during migration;
- no circular capability dependencies;
- provider SDKs remain outside all use-case crates;
- capability crates compile/test independently.

`application-ports` may remain one Cargo crate with capability modules while that keeps a
clear dependency graph; split it into multiple crates only if actual dependency pressure
justifies the added surface.

### Phase 0M — Generated frontend contracts and feature-boundary enforcement

Implement deterministic OpenAPI -> TypeScript generation and remove handwritten duplicate
server DTO/enums.

Add permanent CI:

- regeneration must produce no diff;
- feature X cannot import sibling feature Y internals;
- cross-feature composition uses `shared`, `entities`, app/routes or an explicitly public
  feature API;
- positive repository check + deliberately forbidden fixture.

### Phase 0N — Route classifier, architecture inventory and documentation consistency

Finish the remaining DX/convergence items:

- split fail-closed route classification by capability while retaining one composed
  classifier;
- unknown `/api/*`, `/auth/*`, `/bridge/*` versions/methods never fall through to SPA;
- machine-readable inventory for workspace crates, migrations, public routes and generated
  contracts;
- documentation consistency checks for machine-verifiable claims;
- keep `docs/INDEX.md` current and avoid parallel roadmaps.

### Phase 0 completion gate

Phase 0 is complete only when all are true:

- ordinary Worker/DO transports do not own provider/D1 business orchestration;
- remaining legacy governance routes have bounded application owners;
- coordinator ingress is thin without moving session semantics to the wrong domain;
- use-case growth has real Cargo isolation where justified;
- generated public TS contracts are CI-enforced;
- frontend sibling-feature boundaries are CI-enforced;
- route classification remains fail-closed and modular;
- architecture/docs inventory is consistent;
- all permanent workflows are green on the exact accepted head.

## 5. Phase 1 — Integration Events, Outbox And Notification Persistence

**Goal:** establish one durable event/outbox substrate before richer registry, mailbox and
realtime behavior depend on it.

Build:

- versioned integration event envelope;
- `notification_events`, `notification_deliveries`, evolved `outbox_events`,
  `consumer_idempotency`, `user_event_cursors` as required;
- outbox dispatcher and Queue adapter;
- idempotent consumer registry;
- authorized catch-up by user/grants and event cursor;
- bounded retention/compaction that never removes canonical business state.

Delivery policy is mandatory:

- deterministic attempt accounting;
- exponential backoff with bounded jitter;
- maximum automatic attempts;
- DLQ or equivalent terminal failure lane;
- sanitized alerting/operational visibility;
- operator-safe replay procedure;
- replay idempotency and auditability.

Acceptance:

- duplicate delivery is side-effect neutral;
- poison messages reach DLQ/terminal state after the configured bound;
- retries do not hot-loop;
- DLQ/event payload sanitizer rejects prohibited PII/secrets;
- replay after remediation cannot duplicate the logical effect;
- unauthorized users cannot query event history.

## 6. Phase 2 — Client Registry 2.0 And Assignment Model

**Goal:** complete the standalone business client model before search and CRM integration.

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
- no name/contact-derived technical IDs/paths/keys.

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
execution belongs to Phases 4–5.

Acceptance:

- no cross-tenant/result-count leakage;
- revoked grants disappear according to the accepted consistency contract;
- provider search/body fetch is not called before authorization succeeds;
- a foreign message reference cannot bypass client/mailbox authorization;
- full body can be returned by the fake without entering logs/audit/events/telemetry.

## 8. Phase 4 — Mailbox Operations 2.0 And Cloud Provider Lane

**Goal:** implement real cloud-capable mailbox operations behind one provider-neutral model.

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

**Goal:** deliver low-latency safe change signals after durable event persistence exists.

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

**Goal:** finish the operator product on top of accepted application/query contracts.

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

Code may prepare adapters, validators and runbooks, but production promotion still requires
real accepted evidence for the existing external gates, including as applicable:

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
evidence.

## 15. Architecture Gates For Every Future PR

Every applicable capability PR must satisfy:

1. **Layer gate** — no outward dependency from domain/application code.
2. **Transport-thinness gate** — ordinary ingress does not own provider/D1 orchestration.
3. **Contract gate** — public API/event/bridge changes are versioned/compatibility checked.
4. **Frontend generation gate** — generated contracts are deterministic and clean.
5. **Frontend feature gate** — sibling-feature internals cannot be imported.
6. **Tenant/IDOR gate** — authorization occurs before projection/provider fetch.
7. **Idempotency gate** — duplicate HTTP/Queue/device result has no duplicate logical effect.
8. **Transaction gate** — canonical D1 mutation + audit/outbox are atomic within one D1
   boundary.
9. **Secret/PII/content gate** — prohibited payloads never enter logs/events/audit/support.
10. **Failure-order gate** — external side effects follow the durable transition that
    authorizes them.
11. **Generation freshness gate** — active generation + lease/fencing controls writer launch
    and activation.
12. **Exact-head gate** — all permanent workflows green on one unchanged final head.
13. **Review gate** — zero blocking reviews/unresolved threads before merge.
14. **Evidence-scope gate** — synthetic/local evidence never promotes External claims.

For architecture-boundary migrations use the proven fail-safe switch discipline:

1. add inward port/use case and adapter;
2. prove native/WASM inward behavior;
3. switch live transport while retaining fallback;
4. prove post-switch native/WASM behavior;
5. remove only superseded fallback;
6. make permanent policy/docs reflect the proven final ownership;
7. run exact-head full acceptance;
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

## 17. Recommended PR Slicing

Do not implement a whole phase in one PR. Preferred order:

```text
Phase 0H profile grant
  -> 0I client grant
  -> 0J identity governance lifecycle
  -> 0K coordinator ingress
  -> 0L use-case Cargo boundaries
  -> 0M generated TS + frontend feature boundary
  -> 0N route classifier + architecture/docs inventory

Phase 1 event contract/domain
  -> persistence migration
  -> dispatcher/idempotent consumer
  -> retry/DLQ/replay
  -> authorized catch-up

Phase 2 client aggregate/contact crypto
  -> D1 adapter
  -> merge/assignment lifecycle
  -> API/UI projections

Phase 3 read-model schema
  -> global query service
  -> client-mail query contracts/fakes
  -> API/UI integration

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

Phase 7 UI capabilities incrementally
Phase 8 cross-component acceptance
Phase 9 CRM contracts/projection before cutover adapters
Phase 10 external evidence/promotion
```

Each slice gets its own issue, branch, bounded diff, exact-head CI and guarded squash merge.

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

Finish **Phase 0H (#92 / PR #93)** against the latest `main` using the fail-safe application
boundary sequence above. Only after its guarded merge start a fresh Phase 0I branch from the
new accepted `main`.
