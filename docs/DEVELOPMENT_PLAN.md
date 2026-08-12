# Browser Profile Platform — Development Plan

**Status:** normative product phase plan; pre-2J repository-owned remediation is closed
**Date:** 2026-08-12
**Tracking:** Phase 1 complete; Phase 2A/2B/2C/2D/2E/2F/2G/2H/2I accepted via #118/#137, #138/#140, #142/#143, #144/#147, #148/#152, #154/#155, #159/#160, #163/#164 and #167/#168; pre-2J R1–R9 remediation and final repository-owned closeout are complete; Phase 2J is the next product phase but is unblocked/not started; expert-plan refinement #133; external CRM is future development only
**Production readiness:** unchanged; `production_ready=false` until Phase 2J accepts all mandatory real external evidence

## 1. Authority And Scope

This document is the **normative product phase plan**. `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md` is the
accepted R1–R9 remediation and final repository-owned closeout record; its CLOSED state no longer
blocks Phase 2J and does not itself constitute Phase 2J acceptance. This document defines the product
critical path, architecture ownership, mandatory prerequisites, bounded phase scope and acceptance
conditions.

Authority is intentionally separated:

- `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md` — accepted repository-owned remediation/closeout record;
- `DEVELOPMENT_PLAN.md` — product phase order, slice ownership and acceptance;
- `ARCHITECTURE.md` + accepted ADRs — stable architecture/security invariants;
- `DATA_CLASSIFICATION.md` — data sensitivity, persistence and disclosure rules;
- `UI_ARCHITECTURE.md` — standalone product/UI target;
- `DEVELOPER_CAPABILITY_MATRIX.md` — what is actually accepted on `main` and at what evidence level;
- `DELIVERY_ROADMAP.md` — historical Repository Steps 0–10 and their acceptance record;
- `FUTURE_DEVELOPMENT.md` — post-standalone evolution only, including external CRM integration.

The accepted pre-2J closeout controls only whether repository-owned remediation is complete; it does
not advance the accepted product-phase ledger beyond Phase 2I and cannot substitute for Phase 2J
External evidence. Accepted ADR/security/data invariants always win and this plan must be corrected
before implementation continues when they conflict.

A planned item is never an implementation claim. A branch or PR is never accepted capability until
it is merged under the exact-head acceptance discipline defined below.

## 2. Current Accepted Baseline And Critical Path

Accepted `main` already provides strong repository-local foundations:

- typed opaque identities and positive aggregate versions;
- application-owned client/profile/grant/assignment/identity/coordinator orchestration behind thin
  Worker transports;
- governed D1 mutation + idempotency + audit + outbox transaction patterns;
- immutable profile-generation lifecycle, synthetic Bridge/runtime/materialization foundations and
  strict session fencing ownership;
- capability-module `application-ports` and independent application crates
  `use-cases-identity`, `use-cases-notifications`, `use-cases-clients`, `use-cases-query`,
  `use-cases-mailboxes` and `use-cases-devices`;
- deterministic Rust -> OpenAPI -> TypeScript generation for governed public contract slices;
- permanent frontend sibling-feature boundary enforcement;
- capability-owned fail-closed HTTP route classifiers;
- deterministic `architecture/inventory.json` and documentation consistency enforcement;
- Phase 1A versioned integration-event envelope, durable outbox dispatch and consumer idempotency;
- Phase 1B extracted notification domain/application ownership, durable retry/DLQ, authorized audited
  replay, grant-aware catch-up, bounded retention and sanitizer-safe operational visibility;
- Phase 2A decomposed `client-domain`, opaque PII-independent contact identity, versioned contact
  normalization/protection semantics, protected-only persistence ports and extracted canonical
  client application ownership in `use-cases-clients`;
- Phase 2B authoritative protected client/contact persistence in D1, separate versioned
  encryption and exact-lookup key domains, application-owned checked-version lifecycle/contact
  commands, atomic mutation + idempotency + audit + outbox, and tenant-first indexed HMAC lookup;
- Phase 2C deterministic client merge, historical non-authorizing primary assignment, grant-safe
  Client Registry projections, feature-owned route composition and generated Client Registry contracts;
- Phase 2D independent `use-cases-query`, capability-owned read projections, bounded typed global
  search, grant-safe exact-contact HMAC lookup, provider-neutral Client Mail query contracts,
  deterministic cloud/Bridge query adapters and permanent query/privacy enforcement;
- Phase 2E decomposed `mailbox-domain`, independent `use-cases-mailboxes`, real Gmail API/IMAP outer
  adapters, durable Queue retry/DLQ/idempotency/fencing, opaque secret-resolution, the accepted Phase
  2D Client Mail contract on the cloud adapter, and permanent mailbox privacy/runtime enforcement;
- Phase 2F independent `device-domain`/`use-cases-devices`, durable D1 device jobs and browser mailbox
  execution, trusted claim/generation/Coordinator fencing checks, retained Bridge writer ownership through
  immutable upload + exact verification + fenced/CAS commit, and deterministic rematerialization recovery;
- Phase 2G versioned metadata-only realtime invalidations, provider-neutral notification realtime
  authorization/audience/sink orchestration, per-user hibernatable NotificationHub Durable Objects,
  durable cursor catch-up before live continuation, current membership/grant reauthorization,
  durable-before-live fanout, multi-tab/device broadcast, and frontend invalidation-only refetch/dedupe;
- Phase 2H complete standalone operator/admin composition across the required route families, generated
  bounded profile/member/mailbox discovery, canonical client/profile detail navigation, executable
  Client Mail search/get-message transport over accepted query ownership, safe sandboxed HTML mail
  rendering, explicit offline behavior and permanent route/generated-contract/privacy boundary checks;
- Phase 2I integrated release-candidate hardening with executable cross-capability security/failure/recovery
  evidence, D1/R2/coordinator/Bridge recovery drills, metadata-safe operational/capacity policy,
  dependency/license/source controls, threat-model closure, support-bundle privacy enforcement and a
  release-candidate contract/migration freeze.

Phase 1A was accepted through issue #114 / PR #115 from exact proven source head
`21b4bc65cd1bb117504c0a0cfe18c8c11e411f25` and guarded squash merge
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5`.

Phase 1B was accepted through issue #120 / PR #135 from exact proven source head
`22b2ef36a943d07d22755bf467ec6e7c27ef081d` and guarded squash merge
`f081e0709481d6bbaa150f5518ec8552124c78de`.

Phase 2A was accepted through issue #118 / PR #137 from exact proven source head
`2d80ee74bc8d05657414ea4e75dcf6f41c723926` and guarded squash merge
`a1eb2833a74d9156bce8f4b1c6e92815cc0d55bc`.

Phase 2B was accepted through issue #138 / PR #140 from exact proven source head
`895594e35b77ddd86395300b1644e9df6a712123` and guarded squash merge
`298062ea443c31c69212cb03b3988265b6bbcd48`.

Phase 2C was accepted through issue #142 / PR #143 from exact proven source head
`d3ad2e774a98ad5fed2565ba410ba9923062d170` and guarded squash merge
`042d0dc72fa37e99f971d61d21544609a69c6e31`.

Phase 2D was accepted through issue #144 / PR #147 from exact proven source head
`ad491e2f0c9ba9f79130923fdde6fe1407af4dc5` and guarded squash merge
`26f8fa82bdad02a5a0867b0d36748b915579ef1c`.

Phase 2E was accepted through issue #148 / PR #152 from exact proven source head
`0cefa67abe810db079102462f33ec28fcfc73f69` and guarded squash merge
`6c6ba4564de88b40d282081e701a2d24f1611cc2`.

Phase 2F was accepted through issue #154 / PR #155 from exact proven source head
`c36df418f9fa877c5143327e97b60087c33ffd02` and guarded squash merge
`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`.

Phase 2G was accepted through issue #159 / PR #160 from exact proven source head
`85ca77b430e7d184204082aea7d51a08fdd72cf9` and guarded squash merge
`48e24f1f365d87a07bf97322c81099dd6a89f046`.

Phase 2H was accepted through issue #163 / PR #164 from exact proven source head
`9add9b94d0de255b93e5a7c24584fcf6756462a7` and guarded squash merge
`a32768feddb3da69b872e701bc529aad3521e1b0`.

Phase 2I was accepted through issue #167 / PR #168 from exact proven source head
`c1075337cfc582d0f4c00ec34b1aa7cda9ac1101` and guarded squash merge
`800c634147d6300ea3989ff0cf87ade6e2387ee9`.

The critical path is deliberately linear:

```text
Phase 2A
  -> Phase 2B
  -> Phase 2C
  -> Phase 2D
  -> Phase 2E
  -> Phase 2F
  -> Phase 2G
  -> Phase 2H
  -> Phase 2I
  -> Phase 2J
  -> standalone production-ready product
  -> only then future CRM planning
```

Pre-2J remediation is closed; Phase 2J is the next product phase but is not yet implementation-active.
It must start in a separate bounded work batch from the exact accepted closeout `main`; future CRM work
remains blocked by the same linear rule until Phase 2J is accepted.

### 2.1 Critical-path rules

1. A later product slice never merges before the immediately preceding slice is accepted and its
   docs closeout advances the phase marker.
2. Every substantive slice has one bounded issue, one implementation branch and one draft PR.
3. Every slice proceeds inward-to-outward: contracts/invariants -> domain -> ports -> application
   use cases -> adapters -> ingress/composition -> frontend -> evidence/docs.
4. A layer is omitted only when the slice issue explicitly records it as not applicable before code
   is written; layer order never reverses.
5. New public DTO/event/Bridge shapes are versioned and generated where applicable; handwritten
   duplicate public API shapes are not allowed to accumulate.
6. Provider SDKs, D1/R2/DO APIs, Windows APIs and React never enter pure domain/application layers.
7. External evidence collection may continue operationally, but it does not create a parallel
   implementation lane and cannot promote a capability before its owning slice.
8. Full CI is acceptance evidence, not an interactive development loop. Fast deterministic checks
   and targeted native/WASM tests precede expensive matrix runs.

## 3. Non-Negotiable Clean Architecture

### 3.1 Dependency direction

The only valid dependency direction is inward:

```text
primitives
contracts -> primitives
domains -> contracts + primitives
application-ports -> domains + contracts + primitives
use-cases-* -> application-ports + domains + contracts + primitives
adapters -> application-ports + domains + contracts + primitives + provider/runtime SDKs
apps -> use-cases-* + adapters + contracts + primitives
frontend -> generated public contracts + frontend public feature/entity/shared APIs
```

Outer adapters may depend inward on domain types. Inner layers may never depend on Cloudflare,
Windows, React, D1/R2/DO implementations, provider clients or transport frameworks.

### 3.2 Layer ownership

| Concern | Canonical owner |
|---|---|
| opaque IDs, timestamps, positive versions, neutral shared values | `primitives` |
| versioned wire/event/Bridge shapes | `contracts` / `control-plane-contract` |
| provider-independent state/invariants | appropriate `*-domain` crate |
| workflow-required interfaces | capability module in `application-ports` |
| authorization intent, sequencing, replay/idempotency, orchestration | capability `use-cases-*` crate/module |
| D1/R2/Queue/DO/Access/provider implementation | adapter layer |
| HTTP/Queue/Scheduled/DO/WebSocket mapping and dependency construction | app ingress/composition |
| Windows filesystem/process/browser/device integration | Profile Bridge outer/runtime adapters |
| navigation/forms/query invalidation/presentation | frontend feature/entity/shared layers |

Business behavior must have exactly one canonical owner. Adapters translate; they do not silently
redefine domain policy. UI reflects server decisions; it does not recreate authorization or state
machines.

### 3.3 Transport thinness

Ordinary ingress follows one pattern:

```text
parse protocol
  -> authenticate/resolve verified context
  -> call one application command/query
  -> map typed result/problem to protocol
```

Ingress must not construct D1 mutation objects, calculate business versions, own retry policy,
implement grant rules, make provider-selection policy or assemble cross-resource state machines.

### 3.4 Command/query separation

Mutation paths use aggregates/application commands and preserve canonical mutation + idempotency +
audit + outbox ordering.

Read/list/search paths use explicit query services/read-model ports. Search must not load arbitrary
aggregate graphs merely to filter them afterward. Authorization is applied before projection or
provider fetch.

### 3.5 D1 / Durable Object / R2 / Bridge authority

```text
D1
  -> authoritative business/catalog metadata
  -> aggregate versions and active_generation_id
  -> durable command evidence, audit, outbox and query projections

per-profile Durable Object
  -> lease/session serialization
  -> epoch/fencing and minimal recoverable coordination state

per-user notification Durable Object
  -> realtime connection coordination only
  -> never canonical business state

R2
  -> immutable encrypted generation/evidence objects

Profile Bridge
  -> local materialization/cache/workspace
  -> native device/process/browser lifecycle
```

There is no invented distributed transaction across D1/DO/R2/Bridge. Cross-boundary workflows use
immutable objects, durable intent, fencing, idempotency and reconciliation.

### 3.6 Authorization-before-projection/fetch

Tenant scope and live membership/grants are checked before:

- list/search/detail projection construction;
- event-history/catch-up exposure;
- provider mailbox search;
- full message-body retrieval;
- device job claim/result acceptance;
- realtime subscription delivery.

“Fetch everything, then hide it in React” is prohibited.

### 3.7 Durable-before-notify

```text
validated command
  -> durable canonical mutation
  -> audit + outbox in the same D1 boundary where possible
  -> dispatcher / Queue
  -> durable delivery state
  -> realtime/UI signal
```

No UI/realtime success signal may precede the durable state it represents. Realtime remains an
invalidation/change signal; HTTPS/query projections remain authoritative.

### 3.8 PII, credentials and mailbox content

- client contact display values are encrypted at rest;
- exact contact lookup uses tenant-keyed, domain-separated HMAC tokens;
- encryption keys and lookup/HMAC keys are separate key domains;
- contact normalization is versioned and deterministic before lookup-token generation;
- raw names/emails/phones/URLs never become technical IDs, R2 keys, filesystem paths, metric labels,
  correlation IDs or ordinary log fields;
- fuzzy/prefix PII indexing is prohibited without a dedicated approved privacy/security ADR;
- mailbox message/body content is `CONFIDENTIAL` product data: it may be returned to an authorized
  user but never enters ordinary logs, audit, metrics, integration/realtime events or support bundles;
- message bodies and credentials are never persisted in browser Web Storage;
- HTML mail is sanitized/sandboxed and remote active content is disabled by default.

### 3.9 Modularity rule

Module extraction is not cosmetic and is no longer an undefined “later JIT” task. The following
compile-time boundaries are mandatory at the growth point that now justifies them:

- Phase 1B -> `notification-domain` and `use-cases-notifications`;
- Phase 2A -> `use-cases-clients` plus decomposition of `client-domain`;
- Phase 2D -> `use-cases-query` for cross-capability read/search services;
- Phase 2E -> `use-cases-mailboxes` plus decomposition of `mailbox-domain`;
- Phase 2F -> provider-independent device-job state in its own domain boundary and
  `use-cases-devices`.

`application-ports` remains one Cargo crate with capability-owned modules throughout this roadmap.
That boundary is already accepted and avoids needless crate-per-interface fragmentation.

`crates/use-cases` remains the canonical application owner for Profile Catalog and Profile Generation Registry workflows
until a future explicitly scoped extraction is accepted. Identity, client, query, mailbox, notification and
device contexts already extracted into independent crates never move back into the shared surface merely
for symmetry; no `use-cases-profiles` extraction is implied without a dedicated owning slice.

## 4. Architecture Obligation Traceability

This table is normative. An obligation cannot disappear merely because phases are renamed.
“Accepted foundation” means the existing part is proven; later rows still listed under an owning
slice remain mandatory expansion work.

| ID | Obligation | Accepted state | Mandatory remaining owner |
|---|---|---|---|
| A1 | Adapter dependency boundary | **Accepted.** Correct inward dependency rule documented and enforced by architecture allowlists. | Preserve in every slice; no separate future refactor. |
| A2 | `application-ports` capability split | **Accepted.** Capability modules + thin facade implemented in Phase 0A/PR #79. | Add new modules only with owning capabilities; keep one crate. |
| A3 | Domain aggregate decomposition | **Accepted.** Phase 2A decomposed `client-domain`; Phase 2E decomposed `mailbox-domain`; Phase 2F owns durable device state separately in `device-domain`. | Preserve continuously; split further only at an explicit growth point. |
| A4 | Rust/OpenAPI/TypeScript generation | **Accepted for active Phase 2H public UI additions.** Generated contract ownership and drift checks cover the Phase 2H operator-query and executable Client Mail public surfaces. | Preserve/extend for new Phase 2I public surfaces; migrate legacy surfaces only in explicit owning changes. |
| A5 | Feature-owned SPA route composition | **Accepted through Phase 2H.** Phase 2C established feature-owned public route APIs; Phase 2H expanded all required route families without returning route ownership to the app shell. | Preserve through Phase 2I. |
| A6 | Architecture consistency gate | **Accepted.** Deterministic architecture inventory/docs checks in CI. | Expand inventory/checks whenever new governed modules/routes/contracts appear. |
| A7 | Route classifier modularization | **Accepted.** Capability classifiers behind one fail-closed entrypoint. | New route families must add an owning classifier module; no return to monolith. |
| A8 | Query-side/CQRS read-model boundary | **Accepted in Phase 2D and preserved through Phase 2I.** `use-cases-query`, capability-owned read-model ports/projections, bounded typed global search, grant-safe exact-contact lookup and provider-neutral Client Mail sequencing remain authoritative; Phase 2H exposes them through generated thin transport/UI while Phase 2G realtime only invalidates/refetches canonical paths. | Preserve through Phase 2I; do not move query policy into adapters/UI/realtime. |
| 6.1 | Versioned integration event envelope | **Accepted foundation** in Phase 1A; Phase 2G adds a versioned canonical metadata-only invalidation contract instead of ad-hoc WebSocket JSON. | Reuse/extend versioned registries for later capabilities. |
| 6.2 | Durable-before-notify | **Accepted through Phase 2I.** Phase 1B durable delivery remains canonical; Phase 2E/2F consumers preserve ordering; Phase 2G live fanout occurs only after durable `Delivered`; Phase 2H UI continues to treat realtime as invalidation/refetch rather than success authority. | Preserve through 2I. |
| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2I.** Notification replay, mailbox/device consumers and realtime duplicate delivery preserve duplicate-neutral canonical/UI logical effects; Phase 2H does not introduce a second client-side business state authority. | Preserve for every later consumer and UI surface. |
| 6.4 | Authorization-before-projection | **Accepted through Phase 2I.** Live membership/grants precede list/search/detail/provider access and realtime subscription/catch-up; Phase 2H operator queries and Client Mail ingress call accepted application authorization before projection/provider/body access. | Preserve through Phase 2I. |
| 6.5 | PII protection boundary | **Accepted through Phase 2B.** Contact IDs are PII-independent; authoritative D1 contact storage is ciphertext-only; encryption/HMAC key domains and versions are separate; exact lookup is tenant-scoped and index-backed; rotation candidates do not require plaintext scans. | Preserve in every later client/query/mail surface; fuzzy/prefix PII search still requires a separate accepted ADR. |
| 6.6 | Profile materialization | **Accepted repository-local through Phase 2I.** Retained writer ownership, immutable dirty-generation evolution, exact verification, fenced/CAS commit and deterministic rematerialization are composed/synthetic. | Phase 2I accepted broader repository-local recovery/E2E evidence; Phase 2J supplies real physical/provider evidence. |

## 5. Phase 0 — Architecture Convergence — ACCEPTED

Phase 0 is complete on accepted `main`. Relevant accepted outcomes include:

- capability-owned `application-ports` modules and thin facade (A2);
- application-owned client/profile/mailbox/generation/identity/coordinator orchestration;
- `use-cases-identity` as the first independent application context;
- generated public contract pipeline and frontend sibling-feature enforcement (A4 foundation);
- modular fail-closed HTTP route classifiers (A7);
- deterministic architecture inventory/docs consistency gate (A6).

Phase 0 completion did **not** by itself satisfy A3, A5 or A8. Their fixed growth-point work is now
tracked by accepted evidence: A3 is complete through the Phase 2A client and Phase 2E mailbox
decompositions plus separate Phase 2F device ownership; A5 is accepted through 2H and A8 is accepted
through 2H while retaining its Phase 2D application owner.

## 6. Phase 1 — Durable Integration And Delivery Foundation

**Goal:** complete the asynchronous reliability substrate before product expansion depends on it.
**Status:** ACCEPTED. Phase 1 is complete; Phase 2A through Phase 2I are accepted; Phase 2J is the unique next implementation slice after this closeout.

### Phase 1A — Durable event/outbox foundation — ACCEPTED

Accepted scope:

- versioned `IntegrationEventEnvelope` and event registry;
- additive outbox versioning;
- metadata-only durable notification event persistence;
- Queue publisher/consumer adapters behind inward ports;
- durable tenant/consumer/outbox idempotency;
- payload sanitizer rejecting PII/secrets/mail bodies;
- canonical-source guards and duplicate-delivery neutrality.

Phase 1A deliberately did not implement retry/backoff/DLQ/cursors/catch-up/retention.

### Phase 1B — Delivery hardening, catch-up and operations — ACCEPTED

**Goal:** turn Phase 1A into an operationally safe, replayable, observable at-least-once delivery
platform before clients/mailboxes/devices/realtime build on it.

**Accepted evidence:** issue #120 / PR #135; exact proven source head
`22b2ef36a943d07d22755bf467ec6e7c27ef081d`; squash merge
`f081e0709481d6bbaa150f5518ec8552124c78de`; 12/12 permanent workflows green on the unchanged
source head; `behind_by=0`; reviews=0; unresolved threads=0. `production_ready=false` remains unchanged.

#### 1B execution order

1. **Create the provider-neutral notification domain boundary.**
   - add `notification-domain` ownership for delivery state, attempts, terminal state and cursor
     invariants;
   - no Worker/Queue/D1 types in that domain;
   - define positive attempt/version bounds and explicit terminal transitions.
2. **Extract `use-cases-notifications`.**
   - move Phase 1A dispatcher/consumer orchestration into the independent application context;
   - keep compatibility re-exports only while required by migration;
   - prove native + Workers-WASM compilation before live composition changes.
3. **Add inward ports.**
   - delivery repository;
   - cursor/catch-up repository;
   - clock/jitter source where deterministic policy requires it;
   - operator replay/remediation port surface;
   - no Cloudflare Queue API in the interfaces.
4. **Add additive D1 persistence.**
   - `notification_deliveries`;
   - `user_event_cursors`;
   - attempt/next-attempt/terminal metadata;
   - source/event foreign-key integrity and tenant scoping;
   - sanitized failure metadata only.
5. **Implement deterministic retry policy.**
   - bounded exponential backoff;
   - bounded deterministic jitter contract;
   - configured maximum automatic attempts;
   - no zero-delay hot loop;
   - checked arithmetic only.
6. **Implement terminal/DLQ semantics.**
   - poison delivery reaches a durable terminal state after the bound;
   - terminal transition is idempotent;
   - canonical business state is never deleted because notification delivery failed.
7. **Implement operator remediation/replay.**
   - explicit authorization;
   - immutable audit evidence for replay intent;
   - replay reuses canonical event identity and remains duplicate-neutral;
   - no raw payload editing in operator tooling.
8. **Implement authorized durable catch-up.**
   - actor authentication -> tenant/live membership/grants -> event eligibility -> bounded cursor page;
   - revoked access disappears before projection construction;
   - cursor advancement is durable and monotonic.
9. **Implement retention/compaction.**
   - bounded retention for delivery/cursor operational state;
   - canonical business records, audit and required evidence are never compacted by notification policy;
   - retention policy is documented and deterministic.
10. **Add sanitized operations visibility.**
    - counts/age/terminal state/lag metrics only;
    - no event payloads, contact data, mailbox content or credentials in metrics/support output.
11. **Switch composition and close the old shared ownership.**
    - Worker scheduled/Queue ingress calls `use-cases-notifications` only;
    - adapter construction stays in composition;
    - permanent CI rejects reintroduction of delivery policy into Worker/adapters.

#### 1B acceptance

- duplicate Queue deliveries do not duplicate logical effect, unread state or audit business effect;
- retry timing is bounded and cannot hot-loop;
- max attempts deterministically reach terminal/DLQ state;
- replay after remediation is duplicate-neutral;
- unauthorized/revoked actors cannot read catch-up/event history;
- disconnect/reconnect catch-up survives process restarts;
- retention never removes canonical business state;
- all operational outputs are sanitizer-safe;
- Phase 1A event registry/sanitizer/source guards remain green;
- `notification-domain` and `use-cases-notifications` compile/test independently on required targets;
- permanent architecture negative fixtures reject provider/runtime dependencies in new inner crates;
- exact unchanged final head: 12/12 permanent workflows success, `behind_by=0`, bounded diff,
  reviews=0, unresolved threads=0, guarded squash merge.

#### 1B non-goals

No client contact model, client merge, search, real mailbox provider, device execution, realtime
WebSocket hub or CRM work enters Phase 1B.

**Phase 1 completion gate:** ACCEPTED by the implementation merge and bounded documentation closeout.
Phase 2A through Phase 2I are also accepted; the pre-2J repository-owned remediation is closed and
Phase 2J is the next product phase, while future CRM work remains blocked by the same linear rule.

## 7. Phase 2 — Expert Standalone Product Completion

**Goal:** build the complete standalone application on the accepted durable foundation. Every slice is
mandatory and sequential: **2A -> 2B -> 2C -> 2D -> 2E -> 2F -> 2G -> 2H -> 2I -> 2J**.

### Phase 2A — Client domain decomposition, aggregate and contact-protection foundation — ACCEPTED

**Purpose:** create a clean, growth-ready inward Client Registry model before touching authoritative
contact persistence.

**Accepted evidence:** issue #118 / PR #137; exact proven source head
`2d80ee74bc8d05657414ea4e75dcf6f41c723926`; guarded squash merge
`a1eb2833a74d9156bce8f4b1c6e92815cc0d55bc`; 12/12 permanent workflows green on the unchanged
source head; `behind_by=0`; reviews=0; unresolved threads=0. `production_ready=false` remains unchanged.

#### 2A execution order

1. **Resolve A3 for `client-domain` first.**
   - convert `client-domain/src/lib.rs` to a thin public facade;
   - move client aggregate/lifecycle to `client.rs`;
   - move contact values to `contact_point.rs`;
   - move assignment invariants to `assignment.rs`;
   - reserve merge state/rules in `merge.rs` without implementing Phase 2C merge workflows;
   - preserve current public behavior during the mechanical split.
2. **Extend primitives with opaque contact identity.**
   - add `ContactPointId` generated independently of PII;
   - no email/phone/name hashing is used as a resource ID.
3. **Define versioned client/contact value semantics.**
   - `PERSON|ORGANIZATION`;
   - `ACTIVE|ARCHIVED|MERGED` lifecycle vocabulary;
   - typed contact kind and contact status;
   - versioned deterministic normalization contract per contact kind;
   - protected persisted representation contains ciphertext, lookup token and key/normalization version metadata, never plaintext.
4. **Extract `use-cases-clients`.**
   - move accepted client create/query/grant ownership from shared `use-cases` into the independent
     client application context;
   - preserve thin compatibility facade only during migration;
   - compile/test independently before new behavior is added.
5. **Add contact-protection application boundary.**
   - transient plaintext may enter only the application command/contact-protection call;
   - persistence ports accept only protected contact values;
   - define separate encryption and exact-lookup key domains;
   - define domain-separated HMAC input contract including schema/version + contact kind + normalized value;
   - no cryptographic key material crosses into domain objects.
6. **Add application command intent for client create/update/archive.**
   - authorization/version/replay sequencing is application-owned;
   - no D1 implementation yet for new contact storage;
   - public transport remains unchanged until inward tests pass.
7. **Add pure/native/WASM proof and permanent boundary policy.**

#### 2A acceptance

- `client-domain` facade is thin and aggregate/value modules have explicit ownership;
- `use-cases-clients` compiles/tests independently;
- persistence interfaces cannot accept raw contact plaintext by type;
- contact technical IDs are opaque and PII-independent;
- deterministic normalization/HMAC input vectors are versioned and tested;
- cross-tenant lookup-key derivation contract cannot produce a shared tenant token;
- no new D1 plaintext contact column or transport exposure exists;
- existing client create/query/grant/assignment behavior remains compatible;
- Phase 1 durability and sanitizer invariants remain green.

#### 2A non-goals

No D1 contact ciphertext migration, no key-rotation persistence, no merge workflow, no assignment
redesign, no global search, no full Client Registry UI and no CRM.

### Phase 2B — Client persistence, contact crypto adapter and lifecycle commands — ACCEPTED

**Purpose:** make the 2A model authoritative and safe in D1 without weakening governed writes.

**Accepted evidence:** issue #138 / PR #140; exact proven source head
`895594e35b77ddd86395300b1644e9df6a712123`; guarded squash merge
`298062ea443c31c69212cb03b3988265b6bbcd48`; 12/12 permanent workflows green on the unchanged
source head; `behind_by=0`; reviews=0; unresolved threads=0. `production_ready=false` remains unchanged.

#### 2B execution order

1. Add forward-only D1 schema for contact points and required client lifecycle metadata.
2. Persist only encrypted contact display value + lookup token + key version + normalization version.
3. Implement outer contact-protection adapter using the approved AEAD/HMAC design and separate key
   domains; keys come from outer configuration/secret management, never D1/domain constants.
4. Implement key-version-aware protection and lookup so planned rotation is explicit rather than an
   incompatible schema rewrite.
5. Implement application-owned create/update/archive/contact add-update-remove commands with checked
   aggregate versions and stable replay domains.
6. Preserve one atomic D1 business boundary for canonical mutation + idempotency + audit + outbox.
7. Implement tenant-scoped exact-contact lookup through HMAC token indexes; never scan/decrypt all
   rows to search.
8. Add migration, rollback/failure-order and raw-PII negative fixtures.
9. Extend generated public contracts only for accepted surfaces; do not handwrite duplicate DTOs.

#### 2B acceptance

- D1 has no raw client contact display column/value;
- wrong-tenant lookup cannot resolve another tenant;
- token/ciphertext/key-version constraints fail closed;
- failed crypto/storage leaves no partial canonical mutation/idempotency/audit/outbox state;
- create/update/archive/contact mutations are replay-safe;
- key-version changes do not require plaintext database scans;
- logs/audit/events never contain plaintext, lookup tokens or cryptographic keys.

### Phase 2C — Client merge, assignment, grant-safe projections and modular Client Registry UI — ACCEPTED

**Purpose:** finish Client Registry business semantics and establish scalable frontend route
composition before the SPA grows further.

#### 2C execution order

1. Implement deterministic client merge domain rules in `client-domain/merge.rs`.
   - source/target same tenant;
   - no cycles/self-merge;
   - merged source cannot be resurrected;
   - checked versions and explicit conflict rules;
   - merge never grants access.
2. Complete historical `ProfileClientAssignment` semantics.
   - close prior active assignment;
   - create the next assignment;
   - at most one active primary assignment per profile;
   - one client may own many profiles;
   - assignment remains explicitly non-authorizing.
3. Implement merge/reassignment application commands + governed D1 transactions + audit/outbox.
4. Build grant-safe client/profile/assignment/activity projections.
5. **Resolve A5 before route-family growth.**
   - move feature route definitions behind public feature route modules;
   - root app composition imports feature route APIs, not feature-internal workspace components;
   - prohibit direct sibling-feature route internals;
   - add permanent positive/negative route-composition CI.
6. Expand canonical Rust/OpenAPI/generated TypeScript contracts for all new client surfaces.
7. Build Client Registry UI: list/detail/create/update/archive/contact/merge/assignment/grant history.
8. Add neutral unauthorized/not-found behavior and full frontend regression coverage.

#### 2C acceptance

- merge invariants are proven at domain + D1 levels;
- assignment cannot authorize client/profile access in application, SQL or UI paths;
- revoked member projections disappear without count/existence leakage;
- root router no longer directly owns feature-internal workspace composition;
- public client DTOs are generated rather than duplicated in handwritten `types.ts`;
- ordinary Client Registry workflows require no CLI.

#### Phase 2C acceptance evidence

Phase 2C was accepted through issue #142 / implementation PR #143 from exact proven source head
`d3ad2e774a98ad5fed2565ba410ba9923062d170` and guarded squash merge
`042d0dc72fa37e99f971d61d21544609a69c6e31`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope
includes deterministic one-way client merge, historical primary assignment semantics, application-owned
merge/reassignment, governed atomic D1 merge/history, grant-safe Client Registry projections, A5
feature-owned route composition, generated Client Registry contracts, modular Client Registry UI and
permanent Phase 2C positive/negative/SQLite enforcement in both Quality and Repository Audit gates.
`production_ready=false` remains intentional.

### Phase 2D — CQRS read models, global search and client-mail query contract — ACCEPTED

**Purpose:** resolve A8 with a dedicated query architecture before broad discovery/provider reads.

#### 2D execution order

1. Create `use-cases-query` as the independent cross-capability query application context.
2. Add capability-owned read-model ports (`clients`, `profiles`, `members`, `mailboxes`, `mail`).
3. Define stable read projections distinct from mutation aggregates.
4. Add D1 read projections/indexes required by supported list/filter/search predicates.
5. Enforce query order:

```text
authenticate actor
  -> tenant + live membership/grants
  -> authorize resource/query scope
  -> query indexed projection / eligible mailbox bindings
  -> provider/Bridge query only when authorized
  -> bounded projection
```

6. Implement bounded global search for clients, profiles, permitted users/members and mailbox metadata.
7. Implement exact contact lookup through Phase 2B HMAC indexes only.
8. Add provider-neutral `SearchClientMailboxMessages` and `GetClientMailboxMessage` contracts.
9. Add deterministic fake cloud/Bridge query adapters and full-body synthetic projection tests.
10. Add stable cursor pagination, cost bounds and query-plan/index evidence.
11. Expand generated public contracts and incremental Client -> Mail UI.

#### 2D acceptance

- no cross-tenant/result-count leakage;
- revocation is applied before projection/provider call;
- provider/body fetch cannot occur before authorization;
- foreign message reference cannot bypass client/mailbox eligibility;
- supported predicates use bounded/index-backed query plans;
- synthetic full message body never enters logs/audit/events/telemetry/Web Storage;
- fuzzy/prefix PII search remains absent unless a separate ADR is accepted.

#### Phase 2D acceptance evidence

Phase 2D was accepted through issue #144 / implementation PR #147 from exact proven source head
`ad491e2f0c9ba9f79130923fdde6fe1407af4dc5` and guarded squash merge
`26f8fa82bdad02a5a0867b0d36748b915579ef1c`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope
includes independent `use-cases-query`, capability-owned grant-safe read projections, bounded typed
opaque-ID global search, Phase 2B HMAC-index exact-contact lookup with live grants, provider-neutral
Client Mail search/body contracts, authorization-before-eligibility/provider sequencing, deterministic
fake cloud/Bridge full-body adapters, indexed query-plan evidence, Rust-derived generated mail
contracts, incremental Client -> Mail UI and permanent Phase 2D privacy/authorization enforcement.
`production_ready=false` remains intentional.

### Phase 2E — Mailbox domain decomposition and real cloud mailbox lane — ACCEPTED

**Purpose:** resolve the mailbox half of A3 before adding real provider complexity.

#### 2E execution order

1. Decompose `mailbox-domain` before behavior growth:
   - `binding.rs`;
   - `job.rs`;
   - `runtime_lane.rs`;
   - `observation.rs`;
   - thin `lib.rs` facade preserving existing public compatibility.
2. Extract `use-cases-mailboxes` from shared `use-cases` and prove native/WASM independence.
3. Extend the provider-neutral job state model for scheduled/queued/running/retry/auth/suspended
   outcomes required by the cloud lane.
4. Implement real cloud provider adapters for the product-approved Gmail API/IMAP support surface.
5. Route scheduled execution through the accepted Phase 1B retry/DLQ/idempotency substrate.
6. Implement provider observations + canonical mailbox mutation + audit/outbox without message content.
7. Implement Phase 2D search/get-message contract on the cloud adapter.
8. Add credential-handle lifecycle and explicit auth-required/suspended transitions.
9. Add provider failure taxonomy, rate-limit/backpressure handling and bounded operational metrics.
10. Keep repository-local tests separate from real provider External evidence.

#### 2E acceptance

- inner mailbox crates have no provider SDK dependencies;
- duplicate Queue delivery cannot duplicate logical provider-result processing;
- revoked/suspended binding cannot execute;
- credentials exist only behind opaque secret handles/outer secret stores;
- subject/sender/recipient/body content does not enter audit/outbox/realtime/metrics;
- cloud query implementation conforms to the exact 2D application contract.

Browser/Camoufox execution, device jobs, fingerprint/profile identity, proxy/network runtime policy,
browser workspace locks and browser-driven generation evolution are explicit Phase 2F concerns and
must not leak into the 2E cloud lane.

#### Phase 2E acceptance evidence

Phase 2E was accepted through issue #148 / implementation PR #152 from exact proven source head
`0cefa67abe810db079102462f33ec28fcfc73f69` and guarded squash merge
`6c6ba4564de88b40d282081e701a2d24f1611cc2`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope
includes decomposed `mailbox-domain`, independent `use-cases-mailboxes`, provider-neutral cloud job
lifecycle, real Gmail API/IMAP outer adapters, durable Queue retry/DLQ/idempotency/fencing, metadata-only
provider observations, fixed opaque-handle secret resolution through `MAILBOX_SECRET_RESOLVER`, the
accepted Phase 2D authorization -> eligibility -> provider query contract on the cloud adapter, bounded
UTF-8 IMAP literal search, and permanent positive/negative privacy/runtime enforcement. Real Gmail/IMAP
provider execution remains External evidence rather than a repository-local production claim.
`production_ready=false` remains intentional.

### Phase 2F — Durable device jobs, browser mailbox lane and materialization integration — ACCEPTED

**Purpose:** make browser-required providers a first-class durable device execution lane while
finishing the repository-owned portion of 6.6.

#### 2F execution order

1. Introduce provider-independent durable device-job domain state with opaque job/claim identities.
2. Create `use-cases-devices` for issue/claim/heartbeat/result/cancel/recovery orchestration.
3. Add authenticated device job ports and D1 persistence.
4. Define explicit `PENDING_DEVICE`, `PROFILE_BUSY`, running, retry, auth and terminal semantics.
5. Bind claims to tenant/device/profile/generation and monotonic lease/fencing evidence.
6. Require current active generation + certification policy before browser execution.
7. Integrate Profile Bridge materialization freshness and runtime-identity preflight before writer launch.
   - materialize the exact accepted generation into an isolated clone/workspace; never snapshot a live browser directory;
   - define a provider-neutral `BrowserIdentityManifest` that binds the accepted runtime bundle version/digest to the fingerprint source/configuration and compatibility policy; launches reuse that accepted manifest, while runtime/fingerprint changes require an explicit candidate-generation migration and re-certification path rather than implicit regeneration;
   - do not freeze individual low-level signals such as User-Agent or transport/header details across an incompatible browser-runtime upgrade; compatibility is proven for the manifest/runtime pair, not assumed from copied values;
   - define `NetworkIdentityPolicy` + `NetworkIdentityObservation` around the actual proxy egress used for the browser job: bounded country/region and timezone compatibility, required network class where applicable, optional allowlisted ASN/carrier constraints, and session stickiness only when the selected policy requires it; never assume per-session IP rotation is universally safe;
   - classify network mismatch explicitly (for example retryable route churn versus operator-remediated policy mismatch) and fail closed before Camoufox launch when the observation does not satisfy the accepted policy;
   - treat browser/workspace lock evidence through a fail-closed writer-recovery decision: combine the local workspace lease token/epoch, supervised native process identity and current coordinator lease/fencing evidence; PID alone is never sufficient ownership proof; any active or uncertain writer state returns `PROFILE_BUSY` or `RECOVERY_REQUIRED`;
   - only after all ownership evidence is proven stale may recovery materialize a fresh isolated clone; never mutate the source generation or blindly delete `.parentlock`, `lock` or equivalent runtime lock files.
8. Implement the exact Phase 2D search/get-message contract through the Bridge/browser adapter.
9. Reject stale result after claim turnover, generation change or fencing advancement.
10. Persist successful dirty browser state only through a new immutable encrypted generation: fully stop/supervise the writer, validate the candidate with bounded restore/inventory and policy-selected read-only store probes where useful, upload, verify, then fenced/CAS activation of the D1 active-generation pointer. A blanket `PRAGMA integrity_check` over every Firefox SQLite file is not a universal health/authority signal. Never mutate the active R2 object or depend on cherry-picked provider cookie names. On corruption or failed validation quarantine the candidate; rollback may target only a previously verified/policy-compatible generation. On network/R2 failure preserve dirty local state and route recovery through existing generation rules.
11. Add multi-device/offline/contention/replay/recovery synthetic E2E evidence.

#### 2F acceptance

- a device cannot claim another tenant/device job;
- offline/contended states never become false empty-success;
- stale result cannot overwrite newer claim/generation;
- browser writer launch cannot use a stale local generation;
- cloud and browser lanes satisfy one application query/job contract;
- browser identity/fingerprint configuration cannot change implicitly between launches; runtime upgrades use an explicit `BrowserIdentityManifest` compatibility/migration + re-certification path;
- browser launch is blocked when `NetworkIdentityObservation` does not satisfy the accepted `NetworkIdentityPolicy`; policy may require bounded geo/timezone/network-class/ASN constraints without assuming universal mobile-IP rotation behavior;
- writer recovery is fail closed: local lease token/epoch + supervised native process identity + coordinator fencing are reconciled, PID alone is insufficient, uncertain ownership is `PROFILE_BUSY`/`RECOVERY_REQUIRED`, and browser lock files are never blindly deleted;
- recovery validation is bounded and policy-driven rather than treating blanket Firefox SQLite `PRAGMA integrity_check` as canonical health proof; invalid candidates are quarantined and rollback uses only verified compatible generations;
- dirty browser mutations are not reported as persisted until immutable generation upload, verification and fenced/CAS activation succeed; failure preserves recoverable dirty state;
- local materialization remains cache/workspace, not authority.

#### Phase 2F acceptance evidence

Phase 2F was accepted through issue #154 / PR #155 from exact proven source head
`c36df418f9fa877c5143327e97b60087c33ffd02` and guarded squash merge
`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope
includes independent provider-neutral device ownership, durable device jobs/claims, browser mailbox
execution, generation/fencing freshness, retained writer ownership, immutable generation upload, exact
verification, authoritative fenced/CAS commit and deterministic post-commit rematerialization recovery.
Real physical-device, Camoufox, provider, remote R2/key and production-runtime evidence remains External;
`production_ready=false` remains intentional.

### Phase 2G — Durable realtime notification hub — ACCEPTED

**Purpose:** add realtime only after durable delivery/catch-up and authoritative query paths exist.

#### 2G execution order

1. Extend notification contracts for realtime-safe change signals only.
2. Add per-user notification-hub application ports/use cases to `use-cases-notifications`.
3. Implement outer per-user Durable Object + Hibernatable WebSocket adapter.
4. Authenticate and authorize subscription before delivery.
5. On reconnect, use Phase 1B durable cursor catch-up before live continuation.
6. Reauthorize on bounded intervals/events and disconnect revoked memberships.
7. Keep event payloads metadata-safe; never send contact plaintext or mailbox body.
8. Frontend consumes realtime only to invalidate/refetch canonical HTTPS query data.
9. Add multi-tab/device, disconnect/reconnect, cursor-gap and revoke race tests.

#### 2G acceptance

- process-memory loss does not lose canonical notification/event state;
- reconnect catches up from durable cursor;
- revoked actor stops receiving events without waiting for page reload;
- duplicate event delivery does not duplicate UI logical state;
- WebSocket/DO is never business authority.

#### Phase 2G acceptance evidence

Phase 2G was accepted through issue #159 / PR #160 from exact proven source head
`85ca77b430e7d184204082aea7d51a08fdd72cf9` and guarded squash merge
`48e24f1f365d87a07bf97322c81099dd6a89f046`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope
includes a versioned canonical metadata-only invalidation signal, provider-neutral realtime event
authorization/audience/sink ports and durable-first application orchestration, current D1 membership and
client/profile grant checks, per-user hibernatable `NotificationHub` Durable Objects, durable cursor
catch-up before live continuation, bounded reauthorization and policy disconnect, a handshake-derived
synchronization gate, live fanout only after durable `Delivered`, multi-tab/device broadcast, strict
frontend parsing/deduplication with TanStack Query invalidation-only refetch, and permanent positive and
negative realtime architecture/privacy evidence. D1 outbox/event/cursor state and canonical HTTPS query
paths remain authoritative; real remote Cloudflare/browser deployment evidence remains External and
`production_ready=false` remains intentional.

### Phase 2H — Complete standalone UI and administration UX — ACCEPTED

**Purpose:** make every ordinary operator workflow discoverable and usable without CLI while preserving
all previously accepted authority/privacy boundaries.

#### 2H execution order

1. Complete feature-owned routes for:

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

2. Complete Clients/Profiles workflows and activity history.
3. Complete Users & Access before low-priority infrastructure convenience surfaces.
4. Complete Client detail -> Mail search -> result -> sanitized full-body view.
5. Complete mailbox provider/binding/job/auth/retry administration to the level supported by accepted server contracts.
6. Complete device/session/generation/recovery administration to the level supported by accepted server contracts.
7. Complete audit/settings/operational error surfaces without inventing unsupported tenant-wide raw feeds or secret-bearing configuration.
8. Extend canonical generated contracts until active public DTO/enums consumed by these features are generated or explicitly private frontend view models.
9. Add accessibility, keyboard/navigation-safe composition and loading/empty/error/pending/offline/retry/terminal states where supported.
10. Add safe HTML mail rendering and browser-storage/telemetry negative tests.

#### 2H acceptance

- ordinary supported operation requires no manual opaque-ID-only workaround where a supported list/search exists;
- governed mutation UI never reports optimistic success before server confirmation;
- feature routes remain modular and sibling-feature internals remain inaccessible;
- public server DTO/enums added by Phase 2H are generated rather than redefined by frontend feature code;
- Client detail -> Mail search -> result -> sanitized full-body view is executable through the accepted application contract;
- confidential content never persists in Web Storage/telemetry and HTML mail remote active content is disabled by default;
- realtime remains metadata-only invalidation/refetch authority, never business state;
- unsupported device/audit/settings operations are represented truthfully rather than through fake browser-side authority.

#### Phase 2H acceptance evidence

Phase 2H was accepted through issue #163 / implementation PR #164 from exact proven source head
`9add9b94d0de255b93e5a7c24584fcf6756462a7` and guarded squash merge
`a32768feddb3da69b872e701bc529aad3521e1b0`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope
includes canonical client/profile detail navigation; generated bounded profile/member/mailbox operator
query contracts and thin Worker ingress over accepted `use-cases-query`; executable generated Client
Mail search/get-message transport with authorization-before-eligibility/provider sequencing; standalone
member/mailbox/profile directories; useful `/sessions`, `/devices`, `/audit` and `/settings` surfaces
bounded to accepted server capabilities; explicit offline state; sandboxed CSP-restricted HTML mail
rendering with remote active content disabled; and permanent positive/negative Phase 2H route,
generated-contract, Client Mail, Web Storage/telemetry and realtime-authority checks. No Phase 2I/2J
product work entered the accepted diff and `production_ready=false` remains intentional.

### Phase 2I — Standalone E2E, security, recovery and operational hardening — ACCEPTED

**Purpose:** prove the integrated product as a release candidate before production evidence promotion.

#### 2I execution order

1. Build full owner/member/client/profile/mailbox/device/realtime E2E suites.
2. Build tenant/IDOR/revocation/result-count leakage negative matrix.
3. Build duplicate/replay/concurrency/terminal-failure/remediation matrix.
4. Build profile generation freshness/fencing/materialization/R2 failure/recovery matrix.
5. Build mailbox provider outage/rate-limit/auth-expiry/device-offline/profile-busy recovery matrix.
6. Produce D1/R2/DO/Bridge backup/restore/disaster-recovery runbooks and execute repository-local drills.
7. Define and test operational SLO indicators: queue age, retry age, terminal failures, provider error
   classes, device backlog, catch-up lag and critical API latency without PII labels.
8. Run bounded capacity/cost/query-plan tests for search, Queue, catch-up and UI-critical APIs.
9. Run dependency/license/security source checks and threat-model closure for newly introduced surfaces.
10. Verify support/evidence bundles are allowlist-based and metadata-safe.
11. Freeze release-candidate contracts/migrations and run exact-head cross-component acceptance.

#### 2I acceptance

- no unresolved repository-owned architecture/security correctness gaps;
- backup/restore/recovery procedures have executable evidence;
- failure injection does not violate canonical state/fencing/idempotency invariants;
- performance/cost bounds are documented for supported scale assumptions;
- all permanent workflows green on one exact release-candidate head.

#### Phase 2I acceptance evidence

Phase 2I was accepted through issue #167 / implementation PR #168 from exact proven source head
`c1075337cfc582d0f4c00ec34b1aa7cda9ac1101` and guarded squash merge
`800c634147d6300ea3989ff0cf87ade6e2387ee9`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted repository-
owned scope includes integrated identity/client/profile/mailbox/device/realtime/UI release-candidate
checks; tenant/IDOR/revocation/result-count privacy negatives; duplicate/replay/fencing/failure recovery;
D1-compatible backup/restore and corruption rejection; immutable-generation/coordinator/Bridge recovery;
metadata-safe operational indicators and source-backed capacity bounds; dependency source/integrity and
installed Rust/npm license checks; threat-model closure; allowlist-only support/evidence policy; and
release-candidate contract/migration freeze. Real provider, Camoufox, physical-device, remote R2/key,
trusted-signing and production Cloudflare evidence remains Phase 2J External scope, and
`production_ready=false` remains intentional.

### Phase 2J — Production-readiness evidence and controlled rollout — UNBLOCKED / NOT STARTED

**Purpose:** close only the real-world evidence that repository-local CI cannot prove. This is the only
slice that may change `production_ready=false`.

#### 2J execution order

1. Provision isolated production Cloudflare resources/budgets and prove remote D1/R2/DO/Queue behavior.
2. Prove trusted Windows signing/update/rollback path.
3. Prove primary + secondary physical Windows hosts and real multi-device concurrency/recovery.
4. Prove production device-key protection/unwrap/revoke/recovery.
5. Execute escrow/key restore drill.
6. Complete privacy/retention/product-license approval.
7. Complete real provider/fingerprint certification for supported production lanes.
8. Execute remote backup/recovery/failure-order drills.
9. Complete independent security/cryptographic review for applicable production cryptography.
10. Accept monitoring/on-call/runbook/rollout/rollback procedures.
11. Perform staged rollout with explicit rollback trigger criteria.
12. Promote `production_ready=true` only after every mandatory external gate has immutable reviewed evidence.

#### 2J acceptance

Missing or failed mandatory evidence keeps `production_ready=false`; there is no code-only shortcut.
External CRM is not part of this gate.

## 8. Per-Slice Inward-First Delivery Protocol

Every implementation slice follows this sequence unless an earlier step is explicitly marked N/A in
its issue before coding:

1. **Issue contract:** scope, invariants, non-goals, files/owners, acceptance evidence.
2. **Pure contract/value changes:** opaque IDs and versioned internal/public shapes.
3. **Domain proof:** provider-independent state/invariants + negative tests.
4. **Port proof:** minimal interfaces required by the use case; no implementation leakage.
5. **Application proof:** authorization, sequencing, idempotency/replay, failure taxonomy.
6. **Independent compile proof:** native + required WASM/Windows targets for new inner crates.
7. **Adapter proof:** D1/R2/Queue/DO/provider implementation + migration/failure tests.
8. **Composition switch:** thin ingress wires the proven application path; fallback retained only during
   the bounded switch when required.
9. **Post-switch proof:** native/WASM/release composition green.
10. **Fallback cleanup:** remove superseded transport/provider orchestration.
11. **Frontend/generated contracts:** only after backend/query contract is stable.
12. **Permanent policy/evidence:** positive + negative CI fixtures for architectural boundaries.
13. **Sync and exact-head acceptance:** behind=0, one unchanged head, full permanent workflows.
14. **Guarded squash merge + docs closeout:** only then advance the next slice.

## 9. Target Module Map By End Of Phase 2

```text
crates/
  primitives/
  contracts/
  control-plane-contract/
  identity-access-domain/
  client-domain/
    lib.rs                 # thin facade
    client.rs
    contact_point.rs
    assignment.rs
    merge.rs
  profile-domain/
  session-domain/
  mailbox-domain/
    lib.rs                 # thin facade
    binding.rs
    job.rs
    runtime_lane.rs
    observation.rs
  notification-domain/
  device-domain/           # provider-independent durable device job/claim state
  application-ports/
    identity.rs
    clients.rs
    profiles.rs
    generations.rs
    sessions.rs
    mailboxes.rs
    notifications.rs
    query.rs
    devices.rs
    audit.rs
  use-cases/             # canonical remaining Profile Catalog / Generation Registry workflows
  use-cases-identity/
  use-cases-clients/
  use-cases-notifications/
  use-cases-query/
  use-cases-mailboxes/
  use-cases-devices/
  cloudflare-adapters/

apps/
  control-plane-worker/
    composition/
    ingress/http/
    ingress/queue/
    ingress/scheduled/
    durable_objects/
  profile-bridge/

frontend/src/
  app/
  routes/
  features/
    clients/
    profiles/
    access/
    mailboxes/
    mail/
    sessions/
    devices/
    audit/
    settings/
    realtime/
    search/
  entities/
  shared/
    api/generated/
    ui/
    observability/
```

This is a target ownership map, not permission to create empty placeholder crates. Each named new
crate is created only in its already-fixed owning phase above. Shared `use-cases` is the current
canonical Profile Catalog / Generation Registry application owner; a future `use-cases-profiles`
extraction requires an explicit owning slice and is not implied by architectural symmetry.

## 10. Public Contract And Migration Discipline

- additive compatible evolution is preferred within v1;
- breaking public behavior requires an explicit versioning decision before implementation;
- canonical Rust contract is the source for generated OpenAPI/TypeScript on governed surfaces;
- generated artifacts are committed and CI must fail on regeneration drift;
- no feature handwrites a server enum/DTO already owned by the generated contract;
- D1 migrations are forward-only, contiguous and replay-safe;
- destructive migration requires explicit backup/rollback/rebuild evidence and cannot be hidden in a
  feature PR;
- aggregate version changes are checked, never saturating/wrapping;
- direct SQL bypass of governed invariants receives negative tests/triggers where appropriate.

## 11. Security And Privacy Completion Rules

A feature is incomplete if its happy path works but any applicable negative property is unproven:

- tenant isolation;
- revoked grant behavior;
- neutral disclosure;
- raw PII/secret/content absence from technical channels;
- replay/duplicate neutrality;
- version/concurrency conflict behavior;
- stale device/session fencing;
- provider failure classification;
- recovery after partial external failure;
- support/evidence sanitization.

Security boundaries are enforced in code/CI, not left solely to developer convention.

## 12. Observability And Operations Rules

Operational telemetry uses bounded, low-cardinality, non-PII dimensions. Required categories by the
end of 2I include:

- request outcome/error class and latency;
- Queue age/attempt/terminal count;
- notification catch-up lag;
- mailbox provider outcome class/rate-limit/auth-required counts;
- device backlog/offline/profile-busy counts;
- generation/session recovery state counts;
- deployment/release health.

Forbidden labels/fields include tenant/client/profile contact values, mailbox content, credentials,
raw provider errors that can contain secrets, unbounded resource IDs and filesystem/user paths.

## 13. Future CRM Boundary

There is no active Phase 3 CRM implementation. External CRM/Party integration remains in
`FUTURE_DEVELOPMENT.md` and may be planned only after Phase 2J accepts the standalone product.

The standalone product keeps opaque internal IDs and independent authority until a future explicit
migration proves parity. No active Phase 1–2 model may be coupled to a specific CRM SDK/schema.

## 14. Definition Of Architecture Quality 10/10

For this project “10/10 architecture” means the repository exhibits all of these simultaneously:

- clear ownership by layer and capability;
- no provider/runtime dependency inversion violations;
- business invariants proven in pure code before outer wiring;
- commands and queries separated where their models differ;
- module/crate boundaries introduced at explicit growth points, not as speculative churn;
- generated public contracts prevent frontend/backend drift;
- feature-owned frontend routing prevents app-shell monolith growth;
- authorization precedes all projections/provider reads;
- PII/credentials/content have typed and executable protection boundaries;
- asynchronous behavior is durable, replayable, bounded and observable;
- failures/recovery/concurrency are first-class paths, not afterthoughts;
- CI has negative fixtures that prove forbidden architecture cannot silently return;
- exact-head acceptance and evidence claims never outrun what was actually tested.

## 15. Architecture Gates For Every Future PR

Every applicable PR must satisfy:

1. **Fast-preflight gate** — formatting/policy/targeted compile before expensive CI.
2. **Layer gate** — no outward dependency from domain/application code.
3. **Capability ownership gate** — symbols live in their owning module/crate, facades stay thin.
4. **Transport-thinness gate** — ingress does not own D1/provider/business orchestration.
5. **Contract gate** — public/event/Bridge changes are versioned and compatible.
6. **Generated-contract gate** — OpenAPI/TypeScript regeneration is deterministic and clean.
7. **Frontend feature/route gate** — sibling internals and central route monolith regressions fail closed.
8. **Tenant/IDOR gate** — authorization before projection/provider/device/realtime access.
9. **Idempotency gate** — duplicate HTTP/Queue/device/replay has no duplicate logical effect.
10. **Transaction gate** — canonical D1 mutation + idempotency + audit + outbox are atomic where they
    share a D1 boundary.
11. **Secret/PII/content gate** — prohibited data never enters logs/events/audit/metrics/support.
12. **Failure-order gate** — external side effects occur only after the durable transition authorizing them.
13. **Retry/backpressure gate** — bounded attempts/delay/cost; no hot loops.
14. **Generation/fencing gate** — stale generation/device/session cannot become authoritative.
15. **Migration gate** — forward-only/replay-safe schema with negative invariant tests.
16. **Recovery gate** — partial external failure has an explicit durable recoverable state.
17. **Exact-head gate** — all permanent workflows success on one unchanged final SHA.
18. **Review gate** — zero blocking reviews and unresolved threads.
19. **Evidence-scope gate** — synthetic/local evidence never promotes External claims.
20. **Accepted-provenance gate** — `architecture/accepted-phases.json` and historical issue/PR/source-head/merge-SHA claims must agree; tampered provenance fails closed.

## 16. Developer Workflow And Discoverability

A new developer should be able to answer “where does this change belong?” without repository-wide
searching. Use the ownership table in section 3.2 and the target module map in section 9.

For each slice:

- read the issue + this phase section before editing;
- identify canonical owner and existing accepted compatibility behavior;
- add/update tests at the innermost owning layer first;
- use `python scripts/verify-fast.py` during development;
- use `python scripts/verify-fast.py --with-compile` at boundary switches/final candidate;
- do not use full CI as a formatter/compiler loop;
- do not modify unrelated migrations, lockfile, workflows or neighboring capabilities;
- update capability matrix only for proven accepted claims;
- add machine-enforceable policy when a boundary matters enough that regression would be expensive.

## 17. Mandatory Sequential Execution Order

The active product path has no alternative implementation lane:

```text
Phase 0 architecture convergence                              ACCEPTED
Phase 1A durable event/outbox foundation                      ACCEPTED
Phase 1B notification domain + retry/DLQ/catch-up/operations  ACCEPTED
Phase 2A client-domain split + use-cases-clients + contact protection foundation  ACCEPTED
Phase 2B encrypted contact persistence + client lifecycle commands                 ACCEPTED
Phase 2C merge/assignment/projections + feature-owned routes + Client Registry UI                   ACCEPTED
Phase 2D use-cases-query + CQRS read models + global/client-mail query contracts                     ACCEPTED
Phase 2E mailbox-domain split + use-cases-mailboxes + cloud provider lane                            ACCEPTED
Phase 2F device-domain + use-cases-devices + browser/Bridge mailbox lane                              ACCEPTED
Phase 2G durable realtime notification hub                                                            ACCEPTED
Phase 2H complete standalone UI/admin UX                                                               ACCEPTED
Phase 2I integrated E2E/security/recovery/operations hardening                                        ACCEPTED
Phase 2J real production evidence + controlled rollout                                                UNBLOCKED / NOT STARTED
```

Rules:

1. Phase 2J is the unique next product phase but must begin in its own bounded work batch from accepted closeout `main`;
2. each phase may use bounded sub-PRs only when they preserve the listed internal order and the phase
   itself does not close until every listed outcome is accepted;
3. Phase 2J acceptance requires the mandatory real External evidence listed above and cannot inherit repository-local proof as production evidence;
4. no queued branch is allowed to bypass this order;
5. future CRM is outside this sequence.

## 18. Standalone Product Definition Of Done

The active roadmap is complete only when Phase 2J is accepted and the standalone application works
independently of any CRM.

Definition of done:

- all A1–A8 architecture obligations are accepted or continuously enforced as applicable;
- 6.1–6.6 cross-cutting contracts are implemented at the required evidence level;
- Client Registry supports create/update/archive/merge, encrypted contacts, exact lookup, grants and
  historical profile assignment;
- client/profile/user/mailbox discovery is tenant/grant-safe, bounded and index-backed;
- authorized users can search eligible client mail and open full sanitized body;
- cloud and browser mailbox lanes implement one provider-neutral application contract;
- retries/DLQ/replay/catch-up are durable and operationally safe;
- realtime is durable-event-backed, revocation-aware and non-authoritative;
- complete operator/admin UI works without CLI for ordinary supported operation;
- D1/DO/R2/Bridge authority boundaries remain explicit;
- stale devices/sessions cannot overwrite newer generations/claims;
- backup/restore/recovery/load/cost/rollout/rollback are proven;
- required real provider/physical-host/security/privacy evidence is accepted;
- all permanent workflows are green on exact accepted heads with zero unresolved/blocking reviews;
- `production_ready=true` is allowed only after Phase 2J accepts every mandatory external gate.

## 19. Immediate Next Action

The repository-owned pre-2J closeout is complete. Phase 2J is unblocked but not started and must begin
only in the next separate bounded work batch from the exact accepted closeout `main` SHA. Do not reuse
pre-2J repository-local/synthetic evidence as External acceptance, and do not change
`production_ready=false` before every mandatory Phase 2J gate is accepted.

Execute Phase 2J only for real-world evidence that repository-local CI cannot prove:

```text
isolated production Cloudflare resources/budgets + remote D1/R2/DO/Queue proof
  -> trusted Windows signing/update/rollback
  -> primary + secondary physical Windows hosts and real multi-device recovery
  -> production device-key protection/unwrap/revoke/recovery
  -> escrow/key restore drill
  -> privacy/retention/product-license approval
  -> real provider/fingerprint certification
  -> remote backup/recovery/failure-order drills
  -> independent security/cryptographic review where applicable
  -> monitoring/on-call/runbook/rollout/rollback acceptance
  -> staged rollout with explicit rollback triggers
  -> production_ready=true only after every mandatory External gate is accepted
```

Phase 2I repository-local release-candidate evidence remains accepted input and must not be relabeled as
production proof. Missing or failed real provider/device/signing/key/remote-runtime evidence keeps
`production_ready=false`. External CRM remains future-only until standalone Phase 2J is accepted.
