# Developer Capability and Module Matrix

**Status:** normative accepted implementation/evidence orientation  
**Date:** 2026-08-09  
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
| Profile generation registry | Composed | Governed register/query/verify/activate/deactivate/quarantine, replay/evidence, audit/outbox and pointer integrity. | Production R2/device unwrap/cross-device evidence is External. |
| Profile coordinator | Composed | Durable Object journal, sequence/version/epoch/fencing, timeout/drain/recovery, application-thin HTTP ingress and D1 projection. | Remote production concurrency evidence is External. |
| Full Profile Bridge operator flow | Composed / Synthetic | Synthetic executable composes claim, fake device identity, coordinator lease, generation ownership, writer lock, local lifecycle and fake Camouhost failure ordering. | Real Camoufox, production keys, remote enrollment/coordinator and real R2 lifecycle are External. |
| Local profile lifecycle / materialization | Library / Synthetic | Workspace marking, inventory, lock ownership, clone-only recovery, quota/support policy and synthetic operator composition. | Real browser/kernel-lock, multi-device and production R2 evidence remains Phase 2F/2I/2J Target/External. |
| Encrypted cloud generations | Synthetic | XChaCha20-Poly1305 container, immutable lifecycle, pointer/rollback/quarantine/orphan policy. | Production R2/device unwrap/remote recovery atomicity evidence is External. |
| Mailbox operations baseline | Composed / Synthetic | Phase 2E accepts decomposed `mailbox-domain`, independent `use-cases-mailboxes`, real Gmail API/IMAP outer adapters, durable Queue retry/DLQ/idempotency/fencing, opaque fixed-service secret resolution, auth/suspended lifecycle and metadata-only provider observations while preserving Phase 2D query ordering. | Durable browser/device lane is Phase 2F; real Gmail/IMAP provider execution remains External evidence. |
| React web UI baseline | Composed / Synthetic | React/Vite/TS shell, Phase 2C feature-owned route composition and modular Client Registry UI are accepted; migrated public DTOs are generated, and Phase 2D adds generated Client Mail contracts plus incremental Client -> Mail UI. Sibling-feature internal/alias imports are fail-closed. | Phase 2H completes routes, full operator/admin UX, safe full-body mail rendering and remaining generated public coverage. |
| Cross-component standalone acceptance | Composed / Synthetic | Deterministic metadata-only manifest covering governed D1, generation integrity, Worker/adapters native+WASM, synthetic Bridge and frontend build/tests. | Real deployment/provider/device evidence is External. |
| Integration event envelope/outbox | Composed / Synthetic | Phase 1A versioned envelope, event registry, evolved D1 outbox, metadata-only notification events, Queue dispatch, source guards and durable consumer idempotency; Phase 1B preserves the canonical event source for replay. | Future capability event types/consumers must extend the same registry and durable-source rules. |
| Notification delivery/catch-up operations | Composed / Synthetic | Phase 1B `notification-domain` + `use-cases-notifications`, deterministic bounded retry/DLQ, authorized immutable-audit replay, grant-aware durable catch-up/cursors, bounded compaction, sanitizer-safe owner operations, generated notification HTTP contracts and thin Worker Queue/Scheduled/API composition. | New mailbox/device consumers remain Phase 2E/2F; realtime UserNotificationHub remains Phase 2G. |
| Client contact protection | Composed | Phase 2A/2B accepted versioned normalization, separate encryption/HMAC key domains, ciphertext-only authoritative D1 persistence, key-version-aware protection and tenant-first indexed exact lookup; Phase 2D reuses the HMAC index behind live authorization/grants. | Production key operations/restore remain External; fuzzy/prefix PII search remains prohibited without a separate ADR. |
| Client Registry 2.0 | Composed | Phase 2A–2C accepted client-domain split, `use-cases-clients`, protected contacts, lifecycle/merge, grant-safe projections, historical assignment and ordinary Registry UI workflows. | Phase 2H completes cross-capability operator/admin polish; future CRM cutover remains outside active Phase 2. |
| Read models/global search | Library / Synthetic | Phase 2D accepted independent `use-cases-query`, capability-owned read-model ports/projections, bounded opaque-ID global search, grant-safe D1 predicates, cursor/cost bounds and query-plan evidence with permanent native/WASM CI. Phase 2E adds the real cloud mailbox query adapter behind the same authorization/eligibility contract. | Browser/Bridge mailbox reads remain Phase 2F; broader UX is Phase 2H; real provider evidence remains External. |
| Client-scoped mailbox message search/body | Library / Synthetic | Phase 2D accepted provider-neutral search/get-message contracts and authorization -> mailbox eligibility -> provider sequencing; Phase 2E accepts the bounded real Gmail API/IMAP cloud adapter, provider-scoped cursors/references and transient body parsing under the same application contract. | Phase 2F implements the browser/Bridge lane; real provider/physical evidence remains External. |
| Device job/browser mailbox execution | Target / Synthetic foundation | Bridge/session/materialization primitives exist synthetically. | Durable server device-job domain/application path and real browser lane are Phase 2F. |
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
| A3 | Domain aggregate splitting | **Accepted** — Phase 2A decomposed `client-domain`; Phase 2E decomposed `mailbox-domain` into binding/job/runtime-lane/observation ownership behind a thin compatibility facade. | Preserve continuously; new device state is owned separately by Phase 2F. |
| A4 | OpenAPI -> TypeScript generation | **Partially accepted** — generator/CI and migrated slice exist, but handwritten Profile/Mailbox/Generation/Coordinator projections still exist. | Expand generated coverage with every new 2A–2H public surface. |
| A5 | Feature-sliced SPA route composition | **Accepted in Phase 2C** — feature-owned public route APIs compose into the root router and sibling internals are permanently rejected. | Preserve during later route-family expansion. |
| A6 | Architecture consistency gate | **Accepted** — deterministic inventory/docs consistency in CI. | Expand coverage with new modules/routes/contracts. |
| A7 | Route classifier modularization | **Accepted** — capability-owned fail-closed classifiers. | Preserve for new route families. |
| A8 | CQRS/read-model boundary | **Accepted in Phase 2D** — independent `use-cases-query`, capability-owned read projections and authorization-before-projection/provider sequencing are permanently enforced. | Preserve in 2E/2F real provider lanes and later query surfaces. |
| 6.1 | Integration event envelope | **Accepted foundation** in Phase 1A. | Extend registry/versioned events only. |
| 6.2 | Durable-before-notify | **Accepted through Phase 1B durable delivery**; notification state/replay intent is durable before publication/signal. | Preserve; 2E–2G/2I extend the same ordering. |
| 6.3 | At-least-once consumer idempotency | **Accepted for current notification delivery/replay consumer** with bounded retry/DLQ and duplicate-neutral canonical replay. | 2E/2F new consumers must preserve it. |
| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query scope** — live membership/grants precede projection, exact-contact lookup, mailbox eligibility and provider/body invocation. | Preserve in 2E/2F real lanes; 2G extends the rule to realtime. |
| 6.5 | PII contact protection | **Accepted through Phase 2B/2D** — protected D1 contacts, separate versioned encryption/HMAC domains and tenant-first exact lookup are accepted; query reuse remains grant-safe. | Preserve continuously; fuzzy/prefix PII indexing still requires a separate accepted ADR. |
| 6.6 | Profile materialization | **Library/Synthetic foundation**. | 2F browser/device integration; 2I recovery; 2J real physical evidence. |

## 5. Current Module Ownership

```text
crates/primitives
  stable provider-neutral value objects

crates/*-domain
  pure provider-independent invariants/state machines

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

crates/use-cases
  remaining shared application contexts; notification/client/query/mailbox ownership does not return to this compatibility surface

crates/cloudflare-adapters
  D1/Access/DO/R2/Queue/provider implementations depending inward

apps/control-plane-worker
  thin Worker/DO/Queue/Scheduled composition and transport

apps/profile-bridge
  Windows-native local/device/runtime composition

frontend
  React presentation/navigation/query cache; migrated public API types consume generated TypeScript
```

The notification, client, query and mailbox extraction points were accepted in Phase 1B, Phase 2A,
Phase 2D and Phase 2E. The remaining fixed extraction point in active Phase 2 is devices in Phase 2F.

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
Real R2/device/Camoufox behavior remains External until proven.

### Mailbox path

Current accepted mailbox capability combines Phase 2E composed metadata/job scheduling with the Phase 2D
client-scoped message query contract and a bounded real Gmail API/IMAP cloud adapter. Authorization and
mailbox eligibility precede provider/body invocation; Queue/D1 coordination remains metadata-only and
message content is permitted only in the authorized transient response/UI. Real provider execution evidence
remains External; durable browser/Bridge execution is Phase 2F.

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