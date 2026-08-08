# Browser Profile Platform — Development Plan

**Status:** normative post-composition execution plan  
**Date:** 2026-08-08  
**Tracking:** Phase 1A accepted via #114/#115; Phase 0 complete; Phase 1B #120 is the unique NEXT; Phase 2 starts only after Phase 1B acceptance; external CRM is future development only; plan consolidation history #96
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

**Phase 0 remains complete on accepted `main`, and Phase 1A is accepted.** The active
implementation path is now deliberately linear: **Phase 1B is the unique NEXT** and Phase 2 does
not advance until Phase 1B is accepted and closed out on `main`. The already-created Phase 2A
issue #118/branch is queued only; it is not an active acceptance lane while Phase 1B is open.
External CRM integration is not an active phase and cannot block the standalone product path.

### 2.1 Critical-path execution policy

Development optimizes for the shortest **safe path to accepted product capability**, not for
the maximum number of concurrent branches or the largest refactor.

Rules:

- exactly one active implementation slice is on the product critical path at a time;
- the next implementation slice starts only after the previous slice is accepted, merged and its
  normative closeout advances the next marker;
- no dependency-independent product branch is used to bypass this sequence; operational collection
  of External evidence may continue outside the implementation path but never changes `NEXT`;
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

## 5. Phase 1 — Durable Integration And Delivery Foundation

**Goal:** finish the durable asynchronous substrate completely before product expansion depends on it.
Phase 1 is infrastructure/application foundation. Phase 2 does not begin until Phase 1 is complete.

### Phase 1A — Durable event/outbox foundation — ACCEPTED

Accepted implementation:

- versioned integration event envelope;
- evolved `outbox_events` and metadata-only notification-event persistence;
- outbox dispatcher and Queue adapter;
- tenant/consumer/outbox idempotency;
- payload sanitizer enforcing the existing PII/secret/content policy;
- duplicate-delivery neutrality and canonical-source validation.

Acceptance used exact source head `21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`, 12/12 permanent
workflows green, `behind_by=0`, zero blocking reviews/threads and guarded squash merge #115
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5`.

### Phase 1B — Delivery hardening, catch-up and operations — NEXT

**Goal:** make the Phase 1A substrate operationally safe before any richer mailbox/device/realtime
product behavior is allowed to depend on it.

Implement in this order inside the bounded Phase 1B slice family:

1. durable `notification_deliveries` and `user_event_cursors` needed for delivery/catch-up state;
2. deterministic attempt accounting and next-attempt scheduling;
3. exponential backoff with bounded jitter and a configured maximum automatic attempt count;
4. terminal failure/DLQ lane with sanitized failure metadata only;
5. operator-safe remediation/replay with explicit idempotency and audit evidence;
6. authorized catch-up that applies tenant/live-membership/grant checks before event history exposure;
7. bounded retention/compaction for delivery/cursor state without deleting canonical business state;
8. sanitized operational visibility/alerts that never expose prohibited PII, secrets or mailbox bodies.

Phase 1B acceptance requires all of the following on one unchanged final head:

- poison messages reach terminal/DLQ state after the configured bound;
- retries cannot hot-loop and retry timing is deterministic under the accepted jitter contract;
- replay after remediation cannot duplicate the logical business effect;
- unauthorized/revoked actors cannot query catch-up/event history;
- disconnect/reconnect catch-up is durable rather than process-memory-only;
- Phase 1A event sanitation, canonical-source, transaction and duplicate-neutrality evidence remains green;
- permanent 12/12 workflows succeed, `behind_by=0`, diff is bounded, reviews/threads are zero,
  and guarded squash merge uses the exact accepted source SHA.

**Phase 1 completion gate:** Phase 1 is complete only after Phase 1B is accepted and its docs closeout
advances Phase 2A to `NEXT`. No Phase 2 implementation PR may merge before that gate.

## 6. Phase 2 — Expert Standalone Product Completion

**Goal:** turn the accepted platform foundation into a complete, secure, operator-usable standalone
product. Phase 2 is the entire remaining active product program. Every slice below is mandatory and
strictly sequential: **2A → 2B → 2C → 2D → 2E → 2F → 2G → 2H → 2I → 2J**.

A Phase 2 slice starts only after the immediately preceding slice is accepted and its normative
closeout advances the next marker. No later Phase 2 slice is an alternative or parallel lane.

### Phase 2A — Client aggregate and contact crypto foundation

Build the inward client/contact foundation first:

- extend the provider-neutral client aggregate/value model for `PERSON|ORGANIZATION`, lifecycle
  status and versioned metadata;
- typed contact-point identity/type/value contracts without using PII as technical identifiers;
- encrypted-at-rest contact display representation;
- tenant-keyed HMAC exact-lookup token contract;
- application-owned create/update/archive intent behind ports before D1/transport wiring;
- native + Workers-WASM proof before outer adapters.

Acceptance:

- plaintext contact values cannot cross the persistence port by type;
- no plaintext contact scan is required for exact lookup;
- no client name/email/phone/URL is used in IDs, paths, keys, metric labels or correlation IDs;
- Phase 1 durable mutation/audit/outbox invariants remain green.

### Phase 2B — Client persistence and lifecycle command path

Wire the 2A model into authoritative storage and application commands:

- additive D1 schema for encrypted contact values, key/version metadata and tenant-keyed lookup tokens;
- application-owned client update/archive lifecycle with checked aggregate versions;
- atomic canonical client mutation + idempotency + audit + outbox;
- contact encryption/token adapter behind the inward crypto port;
- tenant-scoped exact-contact lookup without plaintext scan;
- migration and negative tests proving raw contact persistence is rejected.

Acceptance:

- D1 never stores raw contact display values;
- wrong-tenant tokens cannot resolve another tenant's contact;
- failed crypto/persistence leaves no partial canonical mutation/audit/outbox state;
- create/update/archive replay is duplicate-neutral.

### Phase 2C — Client merge, assignment, grant-safe projections and Client Registry UI

Complete the business registry semantics before broad search:

- explicit `MERGED` lifecycle and deterministic source/target merge rules;
- historical `ProfileClientAssignment` reassignment: close previous active assignment, create next,
  emit audit/outbox, never grant access;
- one active primary client assignment per profile and multiple profiles per client;
- grant-filtered client/profile/assignment/activity projections;
- generated public contracts for the accepted new client surfaces;
- usable Client Registry UI for create/update/archive/merge/contact/assignment/grant projections.

Acceptance:

- merge cannot create identity/tenant ambiguity or resurrect archived state;
- assignment remains non-authorizing in application, D1 and UI evidence;
- revoked members lose projections without count/existence leakage;
- ordinary registry workflows are usable without CLI.

### Phase 2D — Read models, global search and client-mail query contract

Build explicit query boundaries after registry semantics are stable:

- CQRS-lite read-model ports/services for lists/search/detail projections;
- tenant/grant filtering before projection construction;
- bounded indexed global search for clients, profiles, permitted members and mailbox metadata;
- provider-neutral `SearchClientMailboxMessages` and `GetClientMailboxMessage` application contracts;
- deterministic fake provider/Bridge query adapters;
- incremental Client -> Mail UI against those contracts.

Mandatory query order:

```text
authenticate actor
  -> tenant + live membership/grants
  -> authorize client/mailbox context
  -> resolve eligible mailbox bindings
  -> provider/Bridge query adapter
  -> bounded result/body projection
```

Acceptance:

- no cross-tenant/result-count leakage;
- provider/body fetch is never called before authorization succeeds;
- foreign message references cannot bypass client/mailbox authorization;
- full synthetic body can be displayed without entering audit/events/logs/telemetry/browser storage.

### Phase 2E — Cloud mailbox provider lane

Implement the real cloud-capable provider path only after Phase 1 and the query contract are complete:

- cloud provider adapter contract implementations (Gmail API/IMAP as selected by product support);
- scheduled Queue execution using the accepted Phase 1 retry/DLQ/idempotency substrate;
- mailbox state/observation mutation + audit/outbox;
- provider-native subject/sender/recipient/body search where supported;
- bounded result mapping and selected full-body fetch;
- deterministic auth-required/suspended/failure transitions.

Acceptance:

- duplicate Queue delivery cannot duplicate logical results/counters/events;
- revoked/suspended bindings cannot execute;
- message content never enters ordinary audit/outbox/realtime payloads;
- provider integration evidence is clearly separated into repository-local versus External claims.

### Phase 2F — Durable device jobs and browser mailbox lane

Add providers that require an authorized browser profile:

- durable device job/request state and authenticated claim/result protocol;
- offline device -> explicit `PENDING_DEVICE`;
- profile contention -> explicit `PROFILE_BUSY`;
- claim/result idempotency, lease and fencing checks;
- current generation/certification validation before browser execution;
- Profile Bridge implementation of the same Phase 2D message query/body contract;
- stale device result rejection after claim turnover.

Acceptance:

- Bridge cannot claim another tenant/device job;
- offline/contended states are never reported as false success/empty result;
- stale results cannot overwrite a newer claim/generation;
- cloud and browser lanes satisfy one provider-neutral application contract.

### Phase 2G — Durable realtime notification hub

Build realtime only after durable retry/catch-up and business/query semantics exist:

```text
canonical state + outbox
  -> Queue dispatcher
  -> per-user UserNotificationHub Durable Object
  -> Hibernatable WebSocket
  -> React invalidation
  -> HTTPS refetch of canonical projection
```

Requirements:

- multiple tabs/devices;
- durable cursor catch-up after reconnect;
- bounded reauthorization and membership-revoke disconnect;
- no authoritative business state stored only in the WebSocket/DO process memory;
- no prohibited PII/secrets/message bodies in realtime envelopes;
- React treats realtime as invalidation/change signal and refetches canonical projections.

### Phase 2H — Complete standalone UI and administration UX

Finish all ordinary operator workflows without CLI:

1. Clients / Profiles;
2. Users & Access;
3. Client detail -> Mail search/result/body;
4. Mailboxes provider/binding/job administration;
5. Devices and sessions infrastructure administration;
6. Audit/settings/error/recovery surfaces.

Required route family remains:

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

UI acceptance:

- generated public contracts only for migrated public DTOs;
- strict sibling-feature boundaries;
- no optimistic success for governed mutations;
- explicit pending/offline/auth-required/profile-busy/retry/terminal states;
- sanitized/sandboxed HTML mail with remote active content disabled by default;
- no mailbox body or credential persistence in Web Storage/telemetry.

### Phase 2I — Standalone E2E, security, recovery and operational hardening

Prove the complete standalone product before production promotion:

- end-to-end owner/member/client/profile/mailbox/device/realtime workflows;
- grant/IDOR/revocation negative matrix;
- duplicate/replay/terminal failure/recovery scenarios;
- generation freshness/fencing/R2 failure and device turnover scenarios;
- backup/restore and disaster/recovery runbooks for D1/R2/DO/Bridge-owned state;
- bounded load/cost/performance tests for search, Queue, notification catch-up and UI-critical APIs;
- support/evidence bundles remain allowlist/sanitized;
- no uncontrolled plaintext PII/secret/message body in logs, audit, events or artifacts.

Acceptance requires one exact-head standalone release-candidate evidence set with all permanent
workflows green and no unresolved architecture/security gaps inside repository-owned scope.

### Phase 2J — Standalone production-readiness evidence and rollout

Close the final release gates for the standalone product. This is the only active phase slice that
can change `production_ready=false` after all required evidence is accepted.

Required evidence includes:

- isolated production Cloudflare resources, budgets and remote D1/R2/DO/Queue behavior;
- trusted Windows signing/update path;
- primary and secondary physical Windows evidence;
- production device-key protection/unwrap and recovery procedure;
- key escrow/restore drill;
- privacy/retention approval;
- product/license decisions;
- real provider/fingerprint certification for supported production lanes;
- remote backup/recovery/failure-order evidence;
- independent security/cryptographic review for applicable production cryptography;
- rollout/rollback/monitoring/runbook acceptance.

Phase 2J acceptance changes `production_ready` only when every mandatory External gate is backed by
real reviewable evidence. Missing evidence keeps `production_ready=false`; there is no code-only
shortcut.

## 7. Active Development Completion Gate

The active development roadmap is complete only when Phase 2J is accepted. At that point the
standalone application must be expert-grade and independently usable without any external CRM:

- Client Registry 2.0 and assignment/grant semantics complete;
- secure encrypted contact handling and exact lookup complete;
- grant-safe global search and client mailbox search/body viewing complete;
- cloud and browser mailbox lanes complete for supported providers;
- durable retries/DLQ/replay/catch-up complete;
- realtime durable-event-backed and non-authoritative;
- complete operator/admin UI usable without CLI;
- E2E/security/recovery/operational evidence accepted;
- production evidence accepted and `production_ready=true` only at this final gate.

External CRM integration is explicitly **not** a prerequisite for this completion gate.

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
| future CRM mapping | future-only CRM adapter + versioned integration contract; outside active Phase 1–2 execution |

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

## 17. Mandatory Sequential Execution Order

The active product path has no alternative or parallel implementation lane. The repository executes
exactly this order:

```text
Phase 0H -> 0I -> 0J -> 0K -> 0L -> 0M -> 0N        ACCEPTED
Phase 1A durable event/outbox foundation              ACCEPTED
Phase 1B delivery hardening/catch-up/operations       NEXT
Phase 2A client aggregate/contact crypto
Phase 2B client persistence/lifecycle
Phase 2C merge/assignment/grant-safe projections + Client Registry UI
Phase 2D read models/global search/client-mail query contract
Phase 2E cloud mailbox provider lane
Phase 2F durable device/browser mailbox lane
Phase 2G realtime notification hub
Phase 2H complete standalone UI/admin UX
Phase 2I standalone E2E/security/recovery/operations hardening
Phase 2J production-readiness evidence and rollout
```

Rules:

1. only the item marked `NEXT` is implementation-active;
2. every slice gets its own bounded issue/branch/draft PR;
3. the next slice starts only after exact-head acceptance, guarded merge and normative closeout of
   the previous slice;
4. no later Phase 2 branch is merged early because it appears dependency-independent;
5. External evidence collection may happen operationally, but it cannot change execution order or
   promote a repository capability claim before its owning slice;
6. external CRM integration is not part of this sequence and is documented only in
   `FUTURE_DEVELOPMENT.md`.

## 18. Standalone Product Definition Of Done

The active Phase 1–2 roadmap is complete only when the standalone application itself is complete;
CRM integration is not part of this definition.

Definition of done:

- clean application/adapter boundaries remain enforced in code and CI;
- Client Registry supports create/update/archive/merge, encrypted contacts, exact lookup,
  grants and historical profile assignment;
- client/profile/user/mailbox search is tenant/grant-safe and bounded/index-backed where required;
- authorized users can search a client's eligible mail and open the full body;
- mailbox content remains outside ordinary telemetry/audit/event payloads and browser storage;
- cloud and browser mailbox lanes share provider-neutral application contracts;
- durable jobs/retries/DLQ/replay/catch-up are operationally safe;
- realtime is durable-event-backed, revocation-aware and never authoritative by itself;
- complete UI/admin workflows work without CLI for ordinary operation;
- D1 remains authoritative for catalog/business metadata, DO for session/realtime coordination,
  R2 for immutable encrypted generation objects and Bridge for local runtime/materialization;
- stale devices/sessions cannot overwrite newer generations/claims;
- backup, restore, recovery, rollout and rollback procedures are proven at the required evidence level;
- required real provider/physical-host/security/privacy evidence is accepted;
- all permanent workflows are green on exact accepted heads and unresolved reviews/threads are zero;
- `production_ready=true` is allowed only after Phase 2J accepts every mandatory external gate.

A standalone release must not require an external CRM. CRM/Party authority integration is future
product evolution documented separately and can be evaluated only after this definition of done is met.

## 19. Immediate Next Action

Start **Phase 1B — delivery hardening, catch-up and operations** under issue #120 from the accepted
Phase 1A main. Phase 1B is the **only** active implementation slice.

Do not advance Phase 2A issue #118 or the existing `phase2a-client-contact-foundation` branch while
Phase 1B is unaccepted. Retain that branch only as queued work; rebase/review it after the Phase 1B
closeout marks Phase 2A `NEXT`.

Primary Phase 1B acceptance target:

```text
notification_deliveries + user_event_cursors
  -> deterministic attempt accounting
  -> bounded exponential backoff + jitter
  -> maximum automatic attempts
  -> terminal/DLQ failure lane
  -> operator-safe idempotent replay
  -> authorized durable catch-up
  -> bounded retention/compaction
  -> sanitized operational visibility
  -> Phase 1A invariants remain green
  -> exact-head permanent CI + guarded merge
```

After Phase 1B acceptance, proceed exactly through Phase 2A -> 2B -> 2C -> 2D -> 2E -> 2F -> 2G
-> 2H -> 2I -> 2J. No active Phase 3 exists. External CRM integration remains future development
only and must not enter the standalone critical path.

Keep `production_ready=false` until Phase 2J accepts all mandatory real external evidence.
