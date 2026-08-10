# Developer Capability and Module Matrix

**Status:** normative accepted implementation/evidence orientation  
**Date:** 2026-08-10  
**Execution order:** [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md)

## 1. Purpose

This matrix answers **what is actually implemented on accepted `main` and at what evidence level**.
It does not define execution order; `DEVELOPMENT_PLAN.md` does.

A feature branch, queued branch, issue or PR description is not accepted implementation evidence.
A capability becomes an accepted `main` claim only after guarded merge under the exact-head policy.

## 2. Evidence Levels

| Level | Meaning |
|---|---|
| **Composed** | Wired into the accepted executable composition root and covered by permanent CI. |
| **Library** | Typed reusable implementation exists but is not fully wired into the accepted user path. |
| **Synthetic** | Invariants/protocol are proven deterministically without claiming a real provider/runtime. |
| **Target** | Normatively planned but executable implementation is absent or incomplete. |
| **External** | Requires real provider, physical host, signing, policy or independent evidence. |

No level by itself means production readiness.

## 3. Accepted Capability Matrix

| Capability | Level on accepted `main` | Accepted scope | Still Target / External |
|---|---|---|---|
| Rust workspace / primitives | Composed | Exact toolchain, typed opaque IDs, tenant/actor context, positive versions, strict lint/policy gates. | New capability IDs are added only in owning phases. |
| Identity / memberships / ACL | Composed | Access identity adapter, memberships, owner bootstrap/transfer, invitations, membership lifecycle, profile/client grants, neutral disclosure and governed D1 commands. `use-cases-identity` is independently isolated. | Production Access/IdP evidence is External. |
| Client Registry baseline | Composed | Phase 2A–2C accepted decomposed `client-domain`, independent `use-cases-clients`, protected contacts, checked lifecycle/merge, historical non-authorizing assignment, grant-safe projections and modular Client Registry UI. | Phase 2H completes broader operator/admin UX; external CRM remains future-only. |
| Profile catalog | Composed | Current create/query/grant/assignment metadata paths, profile state and active generation pointer. | Remaining real external runtime evidence is later Target/External. |
| Profile generation registry | Composed | Governed register/query/verify/activate/deactivate/quarantine, replay/evidence, audit/outbox and pointer integrity; Phase 2F adds device-authorized exact-object verification plus fenced/CAS dirty-generation activation. | Production R2/device unwrap/cross-device evidence is External. |
| Profile coordinator | Composed | Durable Object journal, sequence/version/epoch/fencing, timeout/drain/recovery, application-thin HTTP ingress and D1 projection; Phase 2F revalidates authoritative session/device/epoch/fence before accepting browser/device generation work. | Remote production concurrency evidence is External. |
| Full Profile Bridge operator flow | Composed / Synthetic | Phase 2F repository-local executable composes claim, device identity, Coordinator lease/fence, generation ownership, browser/network preflight, writer lock, graceful retained dirty close, immutable encrypted candidate, exact verify, authoritative commit, local successor/rematerialization and ownership release. | Real Camoufox, production keys, remote enrollment/coordinator, physical devices and real provider/R2 execution remain External. |
| Local profile lifecycle / materialization | Composed / Synthetic | Workspace marking/inventory/lock ownership, clone-only recovery, quota/support policy, retained writer ownership through dirty-generation commit, irreversible supersede and deterministic post-commit rematerialization recovery. | Real browser/kernel-lock and physical multi-device evidence remains Phase 2I/2J Target/External. |
| Encrypted cloud generations | Composed / Synthetic | XChaCha20-Poly1305 immutable container plus Phase 2F canonical exact generation key/checksum/metadata policy, short-lived create-only upload capability, exact verification and fenced/CAS activation sequencing. | Production R2/device unwrap/remote recovery atomicity evidence remains External. |
| Mailbox operations baseline | Composed / Synthetic | Phase 2E accepts decomposed `mailbox-domain`, independent `use-cases-mailboxes`, Gmail API/IMAP outer adapters, durable Queue retry/DLQ/idempotency/fencing, opaque fixed-service secret resolution, auth/suspended lifecycle and metadata-only provider observations; Phase 2F accepts the durable browser/device lane behind the same Phase 2D query ordering at repository-local deterministic evidence. | Real Gmail/IMAP/Camoufox/physical-device provider execution remains External evidence. |
| React web UI baseline | Composed / Synthetic | React/Vite/TS shell, Phase 2C feature-owned route composition and modular Client Registry UI are accepted; migrated public DTOs are generated, and Phase 2D adds generated Client Mail contracts plus incremental Client -> Mail UI. Sibling-feature internal/alias imports are fail-closed. | Phase 2H completes routes, full operator/admin UX, safe full-body mail rendering and remaining generated public coverage. |
| Cross-component standalone acceptance | Composed / Synthetic | Deterministic metadata-only manifest covers governed D1, generation integrity, Worker/adapters native+WASM, Phase 2F retained Bridge close -> immutable upload -> exact verify -> authoritative commit -> successor/release, and frontend build/tests. | Real deployment/provider/device evidence is External. |
| Integration event envelope/outbox | Composed / Synthetic | Phase 1A versioned envelope, event registry, evolved D1 outbox, metadata-only notification events, Queue dispatch, source guards and durable consumer idempotency; Phase 1B preserves the canonical event source for replay. | Future capability event types/consumers must extend the same registry and durable-source rules. |
| Notification delivery/catch-up operations | Composed / Synthetic | Phase 1B `notification-domain` + `use-cases-notifications`, deterministic bounded retry/DLQ, authorized immutable-audit replay, grant-aware durable catch-up/cursors, bounded compaction, sanitizer-safe owner operations, generated notification HTTP contracts and thin Worker Queue/Scheduled/API composition. | The Phase 2E mailbox consumer is accepted; Phase 2F durable device jobs are accepted independently; realtime UserNotificationHub remains Phase 2G. |
| Client contact protection | Composed | Phase 2A/2B accepted versioned normalization, separate encryption/HMAC key domains, ciphertext-only authoritative D1 persistence, key-version-aware protection and tenant-first indexed exact lookup; Phase 2D reuses the HMAC index behind live authorization/grants. | Production key operations/restore remain External; fuzzy/prefix PII search remains prohibited without a separate ADR. |
| Client Registry 2.0 | Composed | Phase 2A–2C accepted client-domain split, `use-cases-clients`, protected contacts, lifecycle/merge, grant-safe projections, historical assignment and ordinary Registry UI workflows. | Phase 2H completes cross-capability operator/admin polish; future CRM cutover remains outside active Phase 2. |
| Read models/global search | Library / Synthetic | Phase 2D accepted independent `use-cases-query`, capability-owned read-model ports/projections, bounded opaque-ID global search, grant-safe D1 predicates, cursor/cost bounds and query-plan evidence. Phase 2E adds the cloud mailbox query adapter and Phase 2F accepts the browser/Bridge mailbox lane behind the same authorization/eligibility contract at repository-local evidence. | Broader UX is Phase 2H; real provider/device evidence remains External. |
| Client-scoped mailbox message search/body | Composed / Synthetic | Phase 2D accepted provider-neutral search/get-message contracts and authorization -> mailbox eligibility -> provider sequencing; Phase 2E accepts bounded Gmail API/IMAP cloud adapters; Phase 2F accepts the browser/Bridge lane with trusted device-job/claim/Coordinator/generation binding while message bodies remain transient. | Real mailbox-provider/Camoufox/physical-device execution remains External. |
| Device job/browser mailbox execution | Composed / Synthetic | Phase 2F accepts independent `device-domain`/`use-cases-devices`, durable D1 device jobs/opaque claims, lifecycle/replay/fencing/freshness preconditions, browser/network identity checks, metadata-only generation upload capability, exact object verification, authoritative commit and retained Bridge ownership through local successor/recovery. | Real physical device, Camoufox and provider execution remains External; production readiness is not implied. |
| Realtime UserNotificationHub | Target | Durable-event-backed topology is normative. | Phase 2G implementation is not accepted. |
| Complete standalone UI/E2E | Target | Product target is normative. | Phase 2H–2I implementation is not accepted; Phase 2J closes real rollout evidence. |
| External CRM integration | Target / Future | Future-only contract-isolated Party/adapter direction is documented separately. | No active CRM implementation; it may be considered only after standalone Phase 2J. |
| Production readiness | External | Evidence intake/readiness interlocks exist. | Mandatory external evidence is incomplete; `production_ready=false` until Phase 2J acceptance. |

## 4. Architecture Obligation Status

This table prevents historical architecture requirements from being misread as completed merely
because phase numbering changed.

| ID | Requirement | Accepted-main status | Fixed execution owner |
|---|---|---|---|
| A1 | Adapter dependency boundary | **Accepted** — corrected inward dependency rule + executable allowlists. | Preserve continuously. |
| A2 | `application-ports` splitting | **Accepted** — Phase 0A/PR #79 split capability modules with thin facade. | Preserve; add modules with owning capabilities. |
| A3 | Domain aggregate splitting | **Accepted** — Phase 2A decomposed `client-domain`; Phase 2E decomposed `mailbox-domain` into binding/job/runtime-lane/observation ownership behind a thin compatibility facade; Phase 2F owns device state separately in `device-domain`. | Preserve continuously. |
| A4 | OpenAPI -> TypeScript generation | **Partially accepted** — generator/CI and migrated slice exist, but handwritten Profile/Mailbox/Generation/Coordinator projections still exist. | Expand generated coverage with every new 2A–2H public surface. |
| A5 | Feature-sliced SPA route composition | **Accepted in Phase 2C** — feature-owned public route APIs compose into the root router and sibling internals are permanently rejected. | Preserve during later route-family expansion. |
| A6 | Architecture consistency gate | **Accepted** — deterministic inventory/docs consistency in CI. | Expand coverage with new modules/routes/contracts. |
| A7 | Route classifier modularization | **Accepted** — capability-owned fail-closed classifiers. | Preserve for new route families. |
| A8 | CQRS/read-model boundary | **Accepted in Phase 2D** — independent `use-cases-query`, capability-owned read projections and authorization-before-projection/provider sequencing are permanently enforced. | Preserved through the accepted Phase 2E cloud and Phase 2F browser lanes; extend to later query/realtime surfaces. |
| 6.1 | Integration event envelope | **Accepted foundation** in Phase 1A. | Extend registry/versioned events only. |
| 6.2 | Durable-before-notify | **Accepted through Phase 1B durable delivery**; notification state/replay intent is durable before publication/signal. | Phase 2E extends this ordering for mailbox execution; Phase 2F preserves durable-before-result acceptance for device/browser work; extend through 2G and 2I. |
| 6.3 | At-least-once consumer idempotency | **Accepted for current notification delivery/replay consumer** with bounded retry/DLQ and duplicate-neutral canonical replay. | Phase 2E mailbox execution and Phase 2F durable device-job replay/idempotency preserve the same duplicate-neutral durability rule. |
| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query scope** — live membership/grants precede projection, exact-contact lookup, mailbox eligibility and provider/body invocation. | Preserved through the accepted Phase 2E cloud and Phase 2F browser lanes; 2G extends the rule to realtime. |
| 6.5 | PII contact protection | **Accepted through Phase 2B/2D** — protected D1 contacts, separate versioned encryption/HMAC domains and tenant-first exact lookup are accepted; query reuse remains grant-safe. | Preserve continuously; fuzzy/prefix PII indexing still requires a separate accepted ADR. |
| 6.6 | Profile materialization | **Accepted repository-local through Phase 2F** — browser/device integration, retained writer ownership, immutable dirty-generation evolution and deterministic rematerialization/recovery evidence are composed/synthetic. | Phase 2I closes broader recovery/E2E; Phase 2J supplies real physical/provider evidence. |

## 5. Current Module Ownership

```text
crates/primitives
  stable provider-neutral value objects

crates/*-domain
  pure provider-independent invariants/state machines, including accepted device-domain ownership

crates/application-ports
  one accepted Cargo crate with capability-owned interface modules

crates/control-plane-contract
  canonical migrated public control-plane contracts, generated OpenAPI source and fail-closed route classifiers

crates/use-cases-identity
  independent identity governance + verified-identity application context

crates/notification-domain
  provider-neutral delivery attempt/terminal/cursor invariants

crates/use-cases-notifications
  independent notification dispatch, retry, replay, catch-up, retention and operations application context

crates/use-cases-clients
  independent Client Registry command/application context accepted in Phase 2A–2C

crates/use-cases-query
  independent cross-capability read/search application context accepted in Phase 2D

crates/use-cases-mailboxes
  independent mailbox binding/job/scheduled application context accepted in Phase 2E

crates/use-cases-devices
  independent durable device-job/browser execution application context accepted in Phase 2F

crates/use-cases
  remaining shared application contexts; notification/client/query/mailbox/device ownership does not return to this compatibility surface

crates/cloudflare-adapters
  D1/Access/DO/R2/Queue/provider implementations depending inward

apps/control-plane-worker
  thin Worker/DO/Queue/Scheduled composition and transport

apps/profile-bridge
  Windows-native local/device/runtime composition including accepted Phase 2F retained dirty-close orchestration

frontend
  React presentation/navigation/query cache; migrated public API types consume generated TypeScript
```

The notification, client, query, mailbox and device extraction points were accepted in Phase 1B,
Phase 2A, Phase 2D, Phase 2E and Phase 2F. No accepted independent application capability may move
back into the shared compatibility surface merely for convenience.

## 6. Current End-To-End Boundaries

### Browser/API path

```text
React / same-origin request
  -> fail-closed route classification
  -> verified identity
  -> live membership/grant resolution
  -> capability application command/query
  -> typed adapter
  -> governed durable result/projection
```

UI never invents authorization or storage access. Concrete D1 mutation types remain adapter-only.

### Profile generation/runtime path

D1 is authoritative for generation metadata/active pointer; DO/session fencing owns writer
coordination; local workspace is materialization/cache/recoverable dirty state, not cloud authority.
Phase 2F accepts repository-local Bridge -> immutable encrypted generation -> exact object verify ->
fenced/CAS activation -> local successor/recovery ordering. Real R2/device/Camoufox behavior remains
External until proven.

### Mailbox path

Current accepted mailbox capability combines Phase 2E composed metadata/job scheduling with the Phase 2D
client-scoped message query contract and bounded Gmail API/IMAP cloud adapters, plus the Phase 2F
browser/Bridge lane at repository-local deterministic evidence. Authorization and mailbox eligibility
precede provider/body invocation; device/browser execution additionally requires trusted device-job/claim,
base-generation, Coordinator fencing and browser/network preconditions. Queue/D1 coordination remains
metadata-only and message content is permitted only in the authorized transient response/UI. Real provider,
Camoufox and physical-device execution evidence remains External.

## 7. Definition Of A Complete New Capability

A capability is not accepted as Composed until all applicable items exist:

1. versioned public/internal contract;
2. pure domain decision where provider-independent state exists;
3. minimal capability-owned application ports;
4. application authorization/idempotency/version/failure sequencing;
5. concrete adapter/migration where required;
6. thin executable composition wiring;
7. replay/failure/forbidden-access/boundary tests;
8. permanent positive + negative CI policy for expensive architecture regressions;
9. generated public frontend contracts where applicable;
10. capability matrix/docs updated only for proven claims;
11. exact-head green + bounded review + guarded merge;
12. real external evidence only for provider/physical/runtime claims.

## 8. Documentation Authority

- execution order and fixed modular extraction points: `DEVELOPMENT_PLAN.md`;
- stable architecture: `ARCHITECTURE.md` + accepted ADRs;
- data handling: `DATA_CLASSIFICATION.md`;
- product/UI target: `UI_ARCHITECTURE.md`;
- accepted implementation level: this matrix;
- historical delivery: `DELIVERY_ROADMAP.md`;
- post-standalone CRM evolution: `FUTURE_DEVELOPMENT.md`.

See [`INDEX.md`](./INDEX.md) for the documentation map.