# Browser Profile Platform — Development Plan

**Status:** normative post-composition execution plan
**Date:** 2026-08-10
**Tracking:** Phase 1 complete; Phase 2A/2B/2C/2D/2E/2F accepted via #118/#137, #138/#140, #142/#143, #144/#147, #148/#152 and #154/#155; Phase 2G is the unique NEXT after this docs closeout; expert-plan refinement #133; external CRM is future development only
**Production readiness:** unchanged; `production_ready=false` until Phase 2J accepts all mandatory real external evidence

---

## 1. Purpose and authority

This document is the **single normative implementation sequence** for the standalone Browser Profile Platform.
It converts the accepted architecture into one sequential engineering plan and removes ambiguity between
historical roadmap notes, architecture requirements and future CRM work.

Authority order for implementation work:

1. this `DEVELOPMENT_PLAN.md` for phase order and fixed extraction points;
2. `ARCHITECTURE.md` and accepted ADRs for stable boundaries/invariants;
3. `DEVELOPER_CAPABILITY_MATRIX.md` for what is actually accepted on `main` and at what evidence level;
4. capability contracts/tests/runbooks for implementation detail;
5. `DELIVERY_ROADMAP.md` and root `IMPLEMENTATION_PLAN.md` only as historical context.

If a historical document conflicts with this plan's phase order, **this plan wins**.

### 1.1 Product objective

Deliver a standalone browser-profile operating platform that can safely manage:

- tenants, users, memberships and explicit resource grants;
- client records and protected contact data;
- browser profiles and immutable encrypted generations;
- one-writer session coordination with monotonic fencing;
- local Windows materialization/recovery and runtime launch;
- Gmail/IMAP and browser-required mailbox execution behind one provider-neutral product contract;
- durable device jobs for browser-required work;
- authorized read models/search/mail queries;
- durable notification delivery/catch-up and later realtime invalidation;
- complete operator/admin UX;
- deterministic repository-local acceptance plus mandatory real external rollout evidence.

The final standalone product must remain architecturally compatible with a later CRM integration without
making CRM a prerequisite for standalone completion.

### 1.2 Non-negotiable architecture direction

All implementation continues to obey:

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

Outer adapters may depend inward. Inner layers may not depend on Cloudflare, Windows, Python,
Camoufox, React, provider SDKs or CRM implementation details.

### 1.3 Evidence rule

No green repository test may be presented as production evidence for a real provider/physical runtime.
Evidence levels remain:

- **Composed** — accepted executable composition + permanent CI;
- **Library** — reusable typed implementation not fully composed into the accepted user path;
- **Synthetic** — deterministic repository-local proof without claiming a real provider/runtime;
- **Target** — planned, absent/incomplete;
- **External** — real provider/host/signing/policy/physical evidence required.

`production_ready=true` is forbidden before Phase 2J explicitly accepts all mandatory External evidence.

---

## 2. Accepted baseline and exact continuation point

Phase 1 / Phase 1A / Phase 1B are accepted on `main`. Later phased implementation accepted
Phase 2A/2B/2C/2D/2E/2F and now makes Phase 2G the unique active next implementation slice.

Phase 2A implementation was accepted through issue #138 / implementation PR #139 from the accepted
Phase 1B docs-closeout `main` at `02ba8106d06746e137dd6ccdfbf47947273a3e0b`. The exact proven source
head `6a64d664b25b03ac6362814104780da062319ec0` passed all 12/12 permanent workflows with
`behind_by=0`, clean reviews/threads and was guarded-squash-merged as
`6dd6a1e2370e24e67f18734b7b4ed069580f8596`.

Phase 2B implementation was accepted through issue #140 / implementation PR #141 from the accepted
Phase 2A docs-closeout `main` at `5a893a4ac977f7877c809525171383563575afc9`. The exact proven source
head `508cb4ebc87e9ccb18f9ff6b4e1488ae9bb98f28` passed the same 12/12 permanent workflow bar with
`behind_by=0` and clean review state, then was guarded-squash-merged as
`5a0070ae8685958211576622487a00899933d574`.

Phase 2C implementation was accepted through issue #142 / implementation PR #143 from the accepted
Phase 2B docs-closeout `main` at `071bb9bb50b8dc03f0a0f8b11a67fb5c948ef124`. The exact proven source
head `ed0e865401f5423a849c53a68af30b71d3a5748e` passed the same 12/12 permanent workflow bar with
`behind_by=0` and clean review state, then was guarded-squash-merged as
`d963c7add635d86993e4edbe3f646a9cb0b9792b`.

Phase 2D implementation was accepted through issue #144 / implementation PR #147 from the accepted
Phase 2C docs-closeout `main` at `2dffcd0352a69a131d95900bb1d2631acb632407`. The exact source head
`edc3bd55407b28df4bac5c0f423a4e4a0bc7ae38` passed the same 12/12 permanent workflow bar with
`behind_by=0` and clean review state, then was guarded-squash-merged as
`e788debf7b49b3cb59eb83c0568920154bc0e9ee`.

Phase 2E implementation was accepted through issue #148 / implementation PR #152 from docs-closeout
`main` at `fd3d01eeb56f413f4d185d45721259eff7cfb846`. The exact source head
`0cefa67abe810db079102462f33ec28fcfc73f69` passed the same 12/12 permanent workflow bar with
`behind_by=0` and clean review state, then was guarded-squash-merged as
`6c6ba4564de88b40d282081e701a2d24f1611cc2`.

Phase 2F was accepted through issue #154 / PR #155 from the accepted Phase 2E docs-closeout `main` at
`fd3d01eeb56f413f4d185d45721259eff7cfb846`. The exact source head
`c36df418f9fa877c5143327e97b60087c33ffd02` passed all 12/12 permanent workflows with
`behind_by=0`, reviews=0 and unresolved review threads=0, then was guarded-squash-merged as
`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`.

These commits are provenance anchors, not permission to restart historical phases. The current phase
is determined only by the first non-accepted phase below.

---

## 3. Global delivery rules

Every phase must preserve these rules unless a newer accepted ADR explicitly changes them.

### 3.1 One active phase

Only one product phase is active at a time. Do not implement later-phase production code “while waiting”
for the current phase to merge. Read-only audit/planning is allowed; semantic implementation is not.

### 3.2 Inward-first order inside each phase

For every capability, use this default order:

1. provider-neutral domain/value/state-machine types;
2. application-owned ports;
3. use-case sequencing/authorization/idempotency;
4. persistence/runtime adapters;
5. executable composition;
6. permanent positive/negative checks;
7. deterministic acceptance;
8. real External evidence when that phase explicitly owns it.

### 3.3 No proof by mock substitution

Mocks/fakes are valid for repository-local invariants only. They do not prove:

- real Access/IdP behavior;
- Cloudflare deployment/routing/secrets;
- Gmail/IMAP provider behavior;
- real Camoufox/browser filesystem behavior;
- Windows CNG/DPAPI private-key protection;
- R2/device unwrap/restore under production conditions;
- physical multi-device contention/recovery;
- trusted signing/rollback/operator procedure.

### 3.4 Exact-head merge bar

Before every implementation or docs closeout merge:

- all required permanent workflows must complete successfully on the exact unchanged source head;
- branch must be `behind_by=0` versus current `main`;
- reviews=0 and unresolved review threads=0 unless an explicit review is requested and resolved;
- merge uses the exact accepted head SHA;
- implementation acceptance is followed by a docs-only closeout before the next phase starts.

### 3.5 No temporary gate escape hatches

Temporary workflows, diagnostics, ignored policy markers, disabled checks and generated drift are not
accepted closeout artifacts. Permanent policy belongs in the existing gate set with deliberate negative
fixtures where practical.

---

## 4. Architecture debt ledger — fixed phase ownership

The old audit items are now mapped to an explicit owner. They may not drift again.

| ID | Requirement | Current status / fixed owner |
|---|---|---|
| A1 | Adapter dependency direction | **Accepted.** Preserve continuously. |
| A2 | Split `application-ports` by capability | **Accepted in Phase 0A.** Add capability modules with their owning phases. |
| A3 | Domain aggregate splitting | **Accepted.** Phase 2A decomposed `client-domain`; Phase 2E decomposed `mailbox-domain`; Phase 2F owns provider-neutral device/job state in `device-domain`. |
| A4 | OpenAPI -> TypeScript generation | **Partially accepted.** Expand generated coverage with every new public DTO/enum in 2A–2H; handwritten public duplicates may not grow. |
| A5 | Feature-sliced SPA route composition | **Accepted in Phase 2C.** Preserve feature-owned public route APIs and the permanent sibling-internal/alias rejection gate. |
| A6 | Architecture consistency gate | **Accepted.** Expand inventory/docs consistency whenever modules/routes/contracts change. |
| A7 | Route classifier modularization | **Accepted.** Preserve capability-owned fail-closed classifiers. |
| A8 | Query-side/CQRS read-model boundary | **Accepted in Phase 2D.** `use-cases-query` is independently isolated; capability-owned read projections and authorization-before-projection/provider sequencing are permanent. Phase 2E cloud and Phase 2F browser mailbox lanes preserve it; Phase 2G extends it to realtime subscriptions. |
| 6.1 | Integration event envelope | **Accepted foundation** in Phase 1A. Extend registry/types only. |
| 6.2 | Durable-before-notify | **Accepted through Phase 1B durable delivery.** Phase 2E mailbox and Phase 2F device/browser paths preserve durable-before-side-effect/result acceptance; extend to realtime in 2G. |
| 6.3 | At-least-once consumer idempotency | **Accepted for notification/mailbox/device delivery/replay.** Preserve duplicate-neutral canonical mutation, fencing and bounded retry/DLQ; extend realtime replay in 2G. |
| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.** Phase 2E cloud and Phase 2F browser mailbox adapters preserve it; Phase 2G realtime subscriptions must preserve it. |
| 6.5 | PII contact protection | **Accepted through Phase 2B/2D.** Ciphertext-only persistence + versioned exact HMAC lookup are permanent. Production keys/restore remain External. |
| 6.6 | Profile materialization | **Accepted repository-local through Phase 2F.** Browser/device integration, retained writer ownership, immutable dirty-generation evolution and deterministic rematerialization/recovery are composed/synthetic; Phase 2I closes integrated recovery/DR and Phase 2J supplies physical E2E. |

---

## 5. Master phase order

The standalone product sequence is:

1. **Phase 1 — profile/runtime foundation** — ACCEPTED;
2. **Phase 1A — integration events + durable outbox foundation** — ACCEPTED;
3. **Phase 1B — notification delivery/catch-up operations** — ACCEPTED;
4. **Phase 2A — Client Registry 2.0 foundation** — ACCEPTED;
5. **Phase 2B — protected client-contact persistence + exact lookup** — ACCEPTED;
6. **Phase 2C — client lifecycle/merge/assignment/read projections/UI** — ACCEPTED;
7. **Phase 2D — CQRS read models, global search and Client Mail query contract** — ACCEPTED;
8. **Phase 2E — mailbox domain decomposition + real cloud Gmail/IMAP lane** — ACCEPTED;
9. **Phase 2F — durable device jobs + browser mailbox lane + materialization integration** — ACCEPTED;
10. **Phase 2G — durable realtime notification hub** — **NEXT**;
11. **Phase 2H — complete standalone UI/operator workflows**;
12. **Phase 2I — integrated recovery/E2E/performance hardening**;
13. **Phase 2J — real-world rollout/evidence closeout**;
14. **external CRM integration** — future only after standalone acceptance.

No later phase may be used to justify postponing a fixed obligation from an earlier phase.

---

## 6. Phase 1 — Profile/runtime foundation — ACCEPTED

Phase 1 remains structurally accepted and protected by permanent gates. Phase 2G is the unique active
next implementation slice after accepted Phase 2F and this docs closeout.

### Phase 1 completion gate

Preserve:

- provider-neutral profile/session/runtime state machines;
- local materialization root and writer-lock safety;
- runtime bundle/launch IPC isolation;
- immutable encrypted generation primitives;
- bounded recovery/quota primitives;
- Windows/WASM/native checks already made permanent.

Later phases extend this baseline; they do not reopen Phase 1.

---

## 7. Phase 1A — Integration events + durable outbox foundation — ACCEPTED

Phase 1A remains the canonical source for integration events. Future capabilities add events to the
registry/outbox; they do not invent a second canonical event source.

### Accepted source

- exact accepted implementation lineage remains historical in merged PR/issues;
- durable-before-notify ordering and metadata-only envelopes are permanent;
- notification consumers are idempotent under at-least-once delivery;
- Phase 2E mailbox and Phase 2F device/browser consumers preserve this accepted durable ordering;
- realtime subscriptions remain Phase 2G scope and consume durable accepted event state rather than replacing it.

---

## 8. Phase 1B — Notification delivery/catch-up operations — ACCEPTED

Accepted scope remains:

- provider-neutral delivery attempt/terminal/cursor invariants;
- independent `use-cases-notifications` ownership;
- bounded retry/DLQ and duplicate-neutral delivery;
- authorized immutable-audit replay;
- grant-aware catch-up/cursors;
- bounded compaction/retention;
- owner-safe operations;
- generated notification contracts;
- thin Worker Queue/Scheduled/API composition.

`UserNotificationHub` realtime is intentionally not part of Phase 1B. It remains Phase 2G so realtime
is layered over durable notification/catch-up state rather than becoming canonical state.

---

## 9. Phase 2 — Standalone product completion

### Phase 2A — Client Registry 2.0 foundation — ACCEPTED

Accepted in #138/#139. Phase 2A permanently established:

- decomposed `client-domain` modules behind a thin facade;
- independent `use-cases-clients` ownership;
- typed client/contact/assignment/merge contracts;
- contact plaintext excluded from persistence ports by type;
- deterministic normalization/protection boundaries prepared for 2B;
- archive/restore and merge invariants;
- generated Client Registry public contracts;
- permanent architecture/privacy checks.

### Phase 2B — Protected client contacts + exact lookup — ACCEPTED

Accepted in #140/#141. Permanent behavior:

- versioned deterministic normalization;
- separate contact encryption and lookup-HMAC key domains;
- ciphertext/nonce/key-version authoritative D1 storage;
- tenant-first equality/index-backed exact lookup;
- current+legacy lookup keyring compatibility;
- decrypt by stored key version;
- no plaintext persistence or decrypt-all search;
- exact lookup reused by Phase 2D queries after live authorization.

Production key operations, escrow/restore and External KMS evidence remain mandatory later; green tests do
not promote key readiness.

### Phase 2C — Client lifecycle, merge, assignment, projections and Registry UI — ACCEPTED

Accepted in #142/#143. Permanent behavior:

- checked update/archive/restore transitions;
- deterministic one-way merge with source revocation/grant removal, not grant transfer;
- historical primary assignment where assignment is explicitly non-authorizing;
- governed D1 merge/history and assignment sequencing;
- grant-safe client projections;
- feature-owned Client Registry route composition;
- sibling-feature internal/alias imports rejected in CI;
- deterministic replay/contention/authorization tests.

### Phase 2D — CQRS read models, global search and Client Mail query contract — ACCEPTED

Phase 2D was accepted through issue #144 / implementation PR #147 from exact proven source head
`edc3bd55407b28df4bac5c0f423a4e4a0bc7ae38` and guarded squash merge
`e788debf7b49b3cb59eb83c0568920154bc0e9ee`. The docs closeout preserves these accepted
invariants:

1. independent `use-cases-query` ownership separate from mutation aggregates;
2. capability-owned read-model ports/projections for client/profile/member/mailbox/global-search data;
3. live membership/grant checks before projection construction;
4. tenant/grant predicates in grant-sensitive D1 queries where practical;
5. opaque stable cursors, explicit limits and query-plan/index evidence;
6. typed opaque-ID global search with no fuzzy/prefix PII discovery;
7. exact-contact lookup only through the accepted Phase 2B HMAC index and only after live authorization;
8. no result-count/secret-handle/provider-credential leakage;
9. provider-neutral Client Mail `search_messages` / `get_message_body` contract;
10. authorization -> mailbox eligibility -> provider search/body sequencing;
11. foreign mailbox/message references remain neutral and never call provider;
12. full message body remains transient: no canonical D1/R2 copy, Web Storage, logs, audit/outbox,
    realtime or telemetry;
13. generated Query Mail public contracts and incremental Client -> Mail UI;
14. permanent positive/negative query ownership/privacy/provider-boundary gates.

The synthetic cloud/Bridge mailbox adapters remain deterministic contract evidence only. Real provider
execution begins in Phase 2E for cloud providers and Phase 2F for browser-required providers.

### Phase 2E — Mailbox domain decomposition and real cloud mailbox lane — ACCEPTED

Phase 2E moved mailbox scheduling/provider execution out of the Phase 2D synthetic-only boundary while
preserving the Phase 2D query/privacy contract.

#### Phase 2E execution order

1. decompose `mailbox-domain` into provider-neutral binding/job/runtime-lane/observation modules behind
   a thin compatibility facade;
2. extract independent `use-cases-mailboxes` ownership;
3. move mailbox scheduling/provider decision sequencing into application/domain layers;
4. keep D1 mailbox persistence/Queue/provider SDKs outer-adapter only;
5. accept additive D1 mailbox run/lease/fencing/idempotency fields with tenant isolation;
6. keep credential values behind opaque `SecretHandle` references and one fixed secret-resolver binding;
7. implement bounded Gmail API adapter;
8. implement bounded IMAP adapter, including explicit parser/response/cursor limits and Unicode literal handling;
9. implement durable Queue/scheduled run lifecycle with bounded retry/backoff/DLQ/idempotent replay;
10. preserve Phase 2D authorization -> mailbox eligibility -> provider/body ordering;
11. keep Queue/D1/audit/outbox/metrics metadata-only;
12. add native/WASM + positive/negative mailbox privacy/provider checks.

#### Accepted mailbox decomposition

The accepted decomposition owns provider-independent policy in `mailbox-domain` and application sequencing
in `use-cases-mailboxes`. Cloudflare adapters own D1/Queue/Gmail/IMAP translation. `apps/control-plane-worker`
remains composition/transport and does not regain mailbox policy.

#### Gmail API boundary

Accepted repository behavior includes:

- opaque secret handle -> outer secret resolver -> provider credential material;
- bounded page size/cursor/reference validation;
- bounded response decoding;
- provider errors normalized to provider-neutral observations;
- no credential/message-body persistence in D1 coordination, Queue, audit/outbox or telemetry.

#### IMAP boundary

Accepted repository behavior includes:

- bounded command/line/literal/response parsing;
- stable opaque provider message references;
- Unicode search through bounded synchronizing literals rather than unsafe interpolation;
- bounded message-body retrieval;
- deterministic provider failure normalization;
- no secret/body telemetry or coordination leakage.

#### Queue/scheduled lifecycle

Accepted mailbox execution uses durable metadata-only dispatch/claim/lease/fencing state before provider
I/O. Duplicate Queue delivery cannot create a second canonical provider-result mutation. Retry is bounded;
terminal/DLQ/auth-required/suspended states are explicit and deterministic.

#### Phase 2E acceptance evidence

Phase 2E was accepted through issue #148 / implementation PR #152 from exact proven source head
`0cefa67abe810db079102462f33ec28fcfc73f69` and guarded squash merge
`6c6ba4564de88b40d282081e701a2d24f1611cc2`. The accepted exact head passed the repository's
12/12 permanent workflow bar with `behind_by=0` and clean review state.

Repository-local evidence covers mailbox domain/application ownership, D1 isolation/CAS/fencing,
bounded Gmail/IMAP adapters, durable Queue replay/retry/DLQ behavior and metadata-only privacy. Real
Gmail/IMAP provider execution remains **External** and does not alter `production_ready=false`.

### Phase 2F — Durable device jobs, browser mailbox lane and materialization integration — ACCEPTED

**Purpose:** make browser-required providers a first-class durable device execution lane while
finishing the repository-owned portion of 6.6.

#### 2F execution order

1. add provider-neutral `device-domain` job/claim/lifecycle state;
2. extract independent `use-cases-devices` ownership;
3. add device-job application/query ports and additive D1 job persistence;
4. implement pending/profile-busy/running/retry/auth-required/terminal lifecycle;
5. bind each claim/result to tenant, device, profile, base generation, opaque claim identity,
   Coordinator lease epoch and fencing token;
6. require active-generation and certification preconditions before browser execution;
7. require materialization freshness plus `BrowserIdentityManifest` and
   `NetworkIdentityPolicy` + `NetworkIdentityObservation` before launch;
8. preserve fail-closed browser-writer ownership: PID alone is never sufficient ownership proof and
   runtime lock files are never blindly deleted;
9. preserve the Phase 2D Client Mail authorization -> mailbox eligibility -> provider/body contract;
10. reject stale claim/generation/fencing results after any relevant state change;
11. persist dirty browser state only through immutable encrypted generation upload -> exact verify ->
    fenced/CAS activation;
12. add deterministic multi-device/offline/contention/replay/recovery evidence and permanent
    positive/negative policy checks.

#### 2F browser identity/runtime policy

The repository-owned runtime contract uses:

- `BrowserIdentityManifest` to bind approved runtime bundle identity, compatibility policy and stable
  fingerprint source/configuration;
- `NetworkIdentityPolicy` plus observed route identity for country/region/timezone/network-class/ASN or
  carrier constraints and optional stickiness;
- a fail-closed writer decision that reconciles workspace lock/token/epoch, supervised process identity
  and Coordinator lease/fence; PID alone is never sufficient ownership proof;
- no automatic deletion of browser runtime locks merely to obtain ownership;
- clone-only stale-writer recovery with bounded inventory/restore/store probes; a blanket
  `PRAGMA integrity_check` is not canonical Firefox-profile health authority;
- explicit candidate generation + re-certification for runtime migration instead of silent identity drift.

#### 2F acceptance

- a device cannot claim another tenant/device job;
- stale/replayed claims and results are rejected deterministically;
- single profile writer remains enforced across devices;
- stale base generation/certification/fencing blocks browser launch and result acceptance;
- browser/network/materialization mismatch fails before launch;
- offline/reconnect/resume behavior is bounded and replay-safe;
- browser provider obeys the accepted Client Mail authorization/privacy contract;
- no browser credential/message body enters ordinary D1 coordination, Queue, audit/outbox or telemetry;
- dirty local state becomes a new immutable encrypted generation, never an in-place overwrite;
- invalid candidates are quarantined/rematerialized without silent rollback corruption;
- local materialization remains cache/workspace, not authority.

#### Phase 2F acceptance evidence

Phase 2F was accepted through issue #154 / PR #155 from exact proven source head
`c36df418f9fa877c5143327e97b60087c33ffd02` and guarded squash merge
`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`. The exact source head completed all 12/12 permanent
workflows successfully with `behind_by=0`, reviews=0 and unresolved review threads=0.

Repository-local evidence covers independent `device-domain`/`use-cases-devices` ownership, durable D1
device jobs and opaque claims, claim/fence/base-generation/certification/browser/network preconditions,
metadata-only create-only generation upload capability, exact immutable-object verification, authoritative
fenced/CAS generation commit, graceful retained writer ownership through commit, irreversible base
supersede and deterministic post-commit rematerialization. The standalone synthetic operator flow proves
`close -> immutable upload -> exact verify -> authoritative commit -> local successor -> ownership release`
without claiming real physical-device/Camoufox/provider execution.

Real physical-device, Camoufox, mailbox-provider and production R2/key/device evidence remains
**External**. `production_ready=false` remains intentional.

### Phase 2G — Отложено: realtime overlay и human-premise контур — NEXT

**Purpose:** add realtime only after durable delivery/catch-up and authoritative query paths exist.

#### 2G execution order

1. define provider-neutral realtime subscription/cursor/projection contracts;
2. authorize subscription/catch-up through live membership/grants before projection;
3. add realtime application sequencing over the accepted durable notification/catch-up source;
4. implement one `UserNotificationHub` Durable Object per user as an outer coordination adapter;
5. keep WebSocket state non-canonical and payloads metadata-safe;
6. implement bounded reconnect/backpressure/cursor-resume/compaction behavior;
7. revalidate grants for catch-up/live delivery and remove revoked access before projection;
8. compose browser transport only after durable replay semantics pass deterministic tests;
9. add positive/negative realtime authorization/privacy/replay/performance checks.

#### 2G acceptance

- D1/audit/outbox remains canonical; websocket/DO state is projection/signal only;
- reconnect/catch-up is deterministic and cursor-bounded;
- duplicate/live replay cannot create canonical side effects;
- revoked grants disappear before catch-up/live projection;
- payloads contain no secrets, mailbox body or prohibited PII;
- bounded backpressure/slow-consumer behavior is explicit;
- frontend consumes typed generated realtime contracts;
- native/WASM/frontend/permanent policy gates are green on one exact head.

### Phase 2H — Complete standalone UI / operator workflows

**Purpose:** make the standalone product operationally complete before integrated rollout hardening.

#### 2H execution order

1. finish remaining route families and operator/admin navigation;
2. complete generated public DTO/enum coverage and remove unjustified handwritten duplicates;
3. complete Client/Profile/Mailbox/Device/Generation operator workflows;
4. add safe full-body mail rendering with HTML sanitization/sandboxing/tracking-disabled defaults;
5. expose recovery/retry/auth-required/quarantine/DLQ states without leaking sensitive payload;
6. finish empty/loading/error/forbidden/not-found/accessibility/keyboard states;
7. preserve feature public APIs and sibling-internal import gates;
8. add frontend contract/route/a11y/operator-flow tests.

#### 2H acceptance

- all standalone workflows are reachable through supported UI/API contracts;
- generated contracts cover new public surfaces;
- no UI-only authorization exists;
- mailbox body is not persisted in Web Storage/telemetry;
- safe rendering and neutral disclosure are tested;
- feature-sliced boundaries remain enforced.

### Phase 2I — Integrated recovery / E2E / performance hardening

**Purpose:** prove the complete standalone architecture deterministically before real rollout evidence.

#### 2I execution order

1. integrate multi-capability deterministic recovery suites;
2. test restart/offline/replay/contention across D1/DO/Bridge/outbox/Queue/realtime;
3. test clone/rematerialization/quota/eviction/reconciliation with accepted Phase 2F materialization rules;
4. add bounded query-plan/load/performance evidence;
5. test notification/mailbox/device retry/DLQ/replay interactions;
6. prove synthetic standalone E2E from identity/client/profile through mailbox/device/recovery/UI;
7. audit logs/audit/outbox/realtime/evidence for prohibited PII/secrets/content;
8. close all deterministic rollback/recovery/runbook gaps that do not require real infrastructure.

#### 2I acceptance

- deterministic full-product E2E passes repeatedly;
- restart/offline/replay/contention recovery is bounded;
- query/load evidence meets documented budgets;
- no canonical state relies on volatile realtime/browser/UI state;
- no prohibited data crosses ordinary operational boundaries;
- all permanent workflows pass one exact unchanged head.

### Phase 2J — Real-world rollout / evidence closeout

**Purpose:** close every mandatory External evidence gap before any production-readiness promotion.

#### 2J mandatory External evidence

1. real Cloudflare deployment/routing/Access/D1/DO/R2/Queue evidence;
2. real Gmail API and IMAP provider evidence;
3. real physical Windows device + Camoufox execution evidence;
4. Windows private-key protection / proof-of-possession evidence;
5. production key hierarchy, unwrap, rotation, restore and rollback evidence;
6. trusted runtime/update signing and rollback evidence;
7. physical multi-device contention/offline/recovery evidence;
8. real encrypted generation upload/verify/restore/recovery evidence;
9. operator runbooks, backup/restore, alerting, DLQ/reconciliation evidence;
10. final privacy/security/readiness matrix with all mandatory external gates satisfied.

#### 2J acceptance

- every mandatory External evidence item is accepted and linked;
- no blocker remains in the readiness matrix;
- restore/recovery/operator procedures are reproducible;
- exact-head permanent CI and external attestation gates agree;
- only then may `docs/status.json` change from `production_ready=false`.

---

## 10. Cross-phase security/privacy constraints

The following are continuous, not phase-local:

### 10.1 Identity and authorization

- Cloudflare Access proves identity only;
- live application membership/grants authorize every resource operation;
- assignment never grants authorization;
- foreign/missing resources use neutral disclosure;
- provider/realtime/body calls happen only after live authorization/eligibility.

### 10.2 Sensitive data

- client contact plaintext never crosses persistence ports;
- contact encryption and lookup-HMAC keys remain distinct/versioned domains;
- mailbox credentials remain behind opaque secret handles;
- mailbox subject/address/body remains out of ordinary D1 coordination, Queue, audit/outbox, realtime,
  logs, metrics and support output;
- root/KEK/DEK/contact keys/device private keys never enter Git/D1/R2/logs/audit/events/client bundles.

### 10.3 Browser/device safety

- active generation/certification/lease/fence/browser/network identity are preconditions, not hints;
- PID alone is never writer ownership proof;
- runtime lock files are not blindly deleted;
- live browser directories are not snapshotted as authoritative state;
- local dirty state cannot overwrite an immutable generation;
- stale claims/generation/fencing cannot activate results.

### 10.4 Storage/coordination

- D1 is authoritative catalog/business state;
- Durable Objects serialize/coordinate; they are not parallel business catalogs;
- R2 generation objects are immutable;
- D1 activation is expected-version/fence/CAS controlled;
- Queue is at-least-once and consumers are idempotent;
- realtime is projection/invalidation, never canonical state.

---

## 11. Performance and boundedness requirements

These rules apply before the final performance phase as each relevant capability is introduced.

### 11.1 API/query bounds

Every query defines:

- maximum page size;
- stable cursor semantics;
- maximum provider response/body size where applicable;
- indexed/equality query plan where required;
- neutral behavior for unauthorized/foreign references;
- no decrypt-all/scan-all PII search.

### 11.2 Provider bounds

Mailbox/cloud/browser adapters define:

- connect/read/response limits;
- provider page/reference/cursor validation;
- parser/literal/body limits;
- normalized retry/auth/terminal failures;
- no sensitive logging.

### 11.3 Device/runtime bounds

Device jobs define:

- claim lease/heartbeat/attempt limits;
- bounded profile-busy retry;
- stale claim/fence rejection;
- bounded browser launch/close/kill/recovery timeouts;
- bounded local workspace/quota/recovery behavior.

### 11.4 Realtime bounds

Realtime defines:

- connection/subscription limits;
- bounded catch-up page/history;
- slow-consumer/backpressure policy;
- cursor compaction/expiry;
- reconnect/replay bounds.

---

## 12. Failure / recovery ownership matrix

| Failure | Canonical owner | Expected recovery |
|---|---|---|
| D1 CAS/version conflict | application/use-case + D1 adapter | reload/retry under explicit conflict semantics |
| stale Coordinator fence | session/runtime application | reject stale writer/result; reacquire current lease |
| R2 immutable key conflict | generation publication boundary | exact verify existing object or fail closed |
| dirty local generation | Profile Bridge/materialization | immutable candidate -> verify -> fenced activation |
| browser crash/lock ambiguity | Profile Bridge + Coordinator | fail closed; no blind lock deletion; clone-only recovery |
| Gmail/IMAP transient failure | mailbox application/provider adapter | bounded retry/backoff/DLQ |
| mailbox auth revoked | mailbox application | `auth_required`/suspended terminal policy, no credential leak |
| Queue duplicate delivery | mailbox/notification consumer | idempotent duplicate-neutral replay |
| device offline/heartbeat expiry | device application | bounded lease expiry/retry/reclaim |
| realtime disconnect | realtime application/adapter | durable cursor catch-up; no lost canonical state |
| contact key rotation | contact protection adapter/application | current writes + multi-version lookup/decrypt/backfill |
| revoked grant during query | authorization/query use case | suppress projection/provider invocation immediately |

---

## 13. Architecture requirement status reconciliation

This table is a release checklist for the old architecture audit IDs.

| ID | Current accepted level | Remaining owner |
|---|---|---|
| A1 | Accepted | preserve |
| A2 | Accepted | preserve |
| A3 | Accepted: client/mailbox/device decomposition | preserve; later new domains follow same rule |
| A4 | Partially accepted | extend generated public coverage through 2G–2H |
| A5 | Accepted in 2C | preserve through remaining UI work |
| A6 | Accepted | expand inventory with new modules/routes/contracts |
| A7 | Accepted | preserve for new route families |
| A8 | Accepted in 2D | preserve through realtime/query extensions |
| 6.1 | Accepted foundation | extend event registry only |
| 6.2 | Accepted durable delivery + mailbox/device preservation | extend to realtime/integrated flows |
| 6.3 | Accepted notification/mailbox/device replay/idempotency | extend realtime/integrated consumers |
| 6.4 | Accepted through 2D and preserved by 2E/2F | extend to realtime |
| 6.5 | Accepted repository protection/query reuse | production key/restore External in 2J |
| 6.6 | Accepted repository-local through Phase 2F | Phase 2I integrated recovery + 2J physical evidence |

---

## 14. Evidence matrix by phase

| Phase | Repository-local evidence | External evidence required before final readiness |
|---|---|---|
| 1 | runtime/materialization/encryption/session synthetic/native | real device/runtime/cloud later |
| 1A | event/outbox/idempotency | real deployment later |
| 1B | notification retry/replay/catch-up | real provider/operations later |
| 2A | client-domain/use-case/privacy architecture | none beyond later key/provider deployment |
| 2B | protected D1 contacts/exact lookup/rotation logic | production key/restore |
| 2C | lifecycle/merge/assignment/UI | real operator deployment later |
| 2D | bounded grant-safe query/mail contract | real provider later |
| 2E | mailbox decomposition, Gmail/IMAP adapter/Queue deterministic evidence | real Gmail/IMAP provider |
| 2F | device jobs, browser/materialization retained-close, immutable dirty-generation commit and deterministic recovery evidence | real physical device/Camoufox/provider/R2/key execution |
| 2G | realtime replay/authorization/backpressure | real deployed websocket/DO behavior later |
| 2H | complete UI/operator synthetic acceptance | real operator/accessibility environment later |
| 2I | integrated deterministic E2E/recovery/performance | none of the required real rollout evidence may be substituted |
| 2J | final repo + runbooks/readiness | all mandatory External evidence must be real and accepted |

---

## 15. Phase change protocol

A phase may advance only through this sequence:

1. open one bounded issue from accepted `main`;
2. create one implementation branch/PR;
3. implement inward-first;
4. add/expand permanent positive/negative guards;
5. reach one exact unchanged head with all required permanent workflows green;
6. prove `behind_by=0`, reviews=0, unresolved threads=0;
7. guarded merge using exact accepted SHA;
8. verify merged `main`;
9. open docs-only closeout;
10. update this plan + architecture/matrix/generator status markers only for proven claims;
11. exact-head CI + guarded docs merge;
12. only then open the next phase.

No implementation phase may mark itself accepted in normative docs before its guarded merge.

---

## 16. Definition of Done for a capability

A capability is not complete merely because code compiles. Applicable completion requires:

1. provider-neutral domain/value/state rules;
2. capability-owned ports/use cases;
3. governed persistence/runtime adapter;
4. thin executable composition;
5. tenant/grant/IDOR negative tests;
6. replay/version/fencing/idempotency tests;
7. sensitive-data boundary tests;
8. permanent architecture check + deliberate negative fixture where useful;
9. generated public contracts for new API DTOs/enums;
10. native/WASM/Windows/frontend checks as applicable;
11. exact-head guarded merge;
12. evidence level accurately recorded as Composed/Library/Synthetic/External.

---

## 17. Explicitly deferred work

The following may be designed/read-only audited before their phase, but production implementation is deferred:

- realtime WebSocket/UserNotificationHub before 2G;
- complete operator/admin UI before 2H;
- full integrated recovery/performance closeout before 2I;
- real production/physical/provider readiness claims before 2J;
- external CRM identity/storage/Party cutover before standalone 2J acceptance.

Future CRM planning belongs in `FUTURE_DEVELOPMENT.md`, not active standalone issues.

---

## 18. Production-readiness rule

Repository-local completion and production readiness are intentionally separate.

`docs/status.json` remains:

```json
{
  "production_ready": false
}
```

until **all** mandatory Phase 2J External evidence is accepted. Earlier green phases may raise
architectural confidence but may not flip this flag.

---

## 19. Immediate Next Action

Open **Phase 2G** only after this Phase 2F docs closeout is accepted and merged from accepted Phase 2F
`main` at `42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`. Start inward-first with provider-neutral realtime
subscription/cursor/projection contracts and authorized durable catch-up application ownership before any
Durable Object/WebSocket adapter composition.

Do not start Phase 2H+, production rollout evidence or external CRM work in parallel. Real
physical-device/Camoufox/provider evidence from Phase 2F remains External and does not change
`production_ready=false`.
