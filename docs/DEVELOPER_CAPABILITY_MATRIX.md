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
| Client Registry baseline | Composed | Current client create/query/grant/assignment metadata paths and D1 schema. Client create/query and grants are application-owned; assignment is non-authorizing. | Domain decomposition, encrypted contacts, richer lifecycle/merge and complete Registry UI are Phase 2A–2C Target. |
| Profile catalog | Composed | Current create/query/grant/assignment metadata paths, profile state and active generation pointer. | Remaining real external runtime evidence is later Target/External. |
| Profile generation registry | Composed | Governed register/query/verify/activate/deactivate/quarantine, replay/evidence, audit/outbox and pointer integrity. | Production R2/device unwrap/cross-device evidence is External. |
| Profile coordinator | Composed | Durable Object journal, sequence/version/epoch/fencing, timeout/drain/recovery, application-thin HTTP ingress and D1 projection. | Remote production concurrency evidence is External. |
| Full Profile Bridge operator flow | Composed / Synthetic | Synthetic executable composes claim, fake device identity, coordinator lease, generation ownership, writer lock, local lifecycle and fake Camouhost failure ordering. | Real Camoufox, production keys, remote enrollment/coordinator and real R2 lifecycle are External. |
| Local profile lifecycle / materialization | Library / Synthetic | Workspace marking, inventory, lock ownership, clone-only recovery, quota/support policy and synthetic operator composition. | Real browser/kernel-lock, multi-device and production R2 evidence remains Phase 2F/2I/2J Target/External. |
| Encrypted cloud generations | Synthetic | XChaCha20-Poly1305 container, immutable lifecycle, pointer/rollback/quarantine/orphan policy. | Production R2/device unwrap/remote recovery atomicity evidence is External. |
| Mailbox operations baseline | Composed / Synthetic | Provider-neutral binding/job domain, D1 persistence, secret-handle-only DTOs, idempotency/audit/outbox, Worker metadata/job paths and synthetic provider decisions. | Mailbox-domain decomposition, real cloud/browser execution and message search/body retrieval are Phase 2D–2F Target/External. |
| React web UI baseline | Composed / Synthetic | React/Vite/TS shell and current session/client/profile/ACL/assignment/generation/coordinator/mailbox/user surfaces. Migrated session/client/problem/mutation contracts are generated. Sibling-feature internal/alias imports are fail-closed. | Feature-owned route composition (A5), complete routes, full generated public DTO coverage, Client Mail and complete admin UX remain Phase 2C/2H Target. |
| Cross-component standalone acceptance | Composed / Synthetic | Deterministic metadata-only manifest covering governed D1, generation integrity, Worker/adapters native+WASM, synthetic Bridge and frontend build/tests. | Real deployment/provider/device evidence is External. |
| Integration event envelope/outbox | Composed / Synthetic | Phase 1A versioned envelope, event registry, evolved D1 outbox, metadata-only notification events, Queue dispatch, source guards and durable consumer idempotency; Phase 1B preserves the canonical event source for replay. | Future capability event types/consumers must extend the same registry and durable-source rules. |
| Notification delivery/catch-up operations | Composed / Synthetic | Phase 1B `notification-domain` + `use-cases-notifications`, deterministic bounded retry/DLQ, authorized immutable-audit replay, grant-aware durable catch-up/cursors, bounded compaction, sanitizer-safe owner operations, generated notification HTTP contracts and thin Worker Queue/Scheduled/API composition. | New mailbox/device consumers remain Phase 2E/2F; realtime UserNotificationHub remains Phase 2G. |
| Client contact protection | Target | Data-classification and architecture contracts require encrypted display values and tenant-keyed exact lookup. | Actual client contact encryption/HMAC/key-version persistence is **not** implemented; Phase 2A–2B owns it. |
| Client Registry 2.0 | Target | Expert standalone registry target and strict 2A–2C sequence are normative. | `client-domain` split, `use-cases-clients`, contacts, lifecycle/merge, projections and UI remain Target. |
| Read models/global search | Target | CQRS/query/security target is normative. | `use-cases-query`, read-model ports/projections and global search are Phase 2D. |
| Client-scoped mailbox message search/body | Target | Product/query/security contract is normative. | 2D defines query contract; 2E cloud and 2F browser implementations remain Target. |
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
| A3 | Domain aggregate splitting | **Open** — `client-domain` and `mailbox-domain` remain monolithic today. | Client half 2A; mailbox half 2E. |
| A4 | OpenAPI -> TypeScript generation | **Partially accepted** — generator/CI and migrated slice exist, but handwritten Profile/Mailbox/Generation/Coordinator projections still exist. | Expand generated coverage with every new 2A–2H public surface. |
| A5 | Feature-sliced SPA route composition | **Open** — root router still directly assembles feature workspaces. | Phase 2C before route-family expansion. |
| A6 | Architecture consistency gate | **Accepted** — deterministic inventory/docs consistency in CI. | Expand coverage with new modules/routes/contracts. |
| A7 | Route classifier modularization | **Accepted** — capability-owned fail-closed classifiers. | Preserve for new route families. |
| A8 | CQRS/read-model boundary | **Open**. | Phase 2D via independent `use-cases-query`. |
| 6.1 | Integration event envelope | **Accepted foundation** in Phase 1A. | Extend registry/versioned events only. |
| 6.2 | Durable-before-notify | **Accepted through Phase 1B durable delivery**; notification state/replay intent is durable before publication/signal. | Preserve; 2E–2G/2I extend the same ordering. |
| 6.3 | At-least-once consumer idempotency | **Accepted for current notification delivery/replay consumer** with bounded retry/DLQ and duplicate-neutral canonical replay. | 2E/2F new consumers must preserve it. |
| 6.4 | Authorization-before-projection | **Composed for Phase 1B catch-up** with live membership/grants before metadata projection. | 2D query/provider fetch; 2G realtime. |
| 6.5 | PII contact protection | **Open for client contacts**. Phase 1A sanitizer does not satisfy this requirement. | 2A protected-value/crypto boundary; 2B D1 encryption/HMAC/key rotation. |
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

crates/use-cases
  remaining shared application contexts; notification ownership is fully extracted

crates/cloudflare-adapters
  D1/Access/DO/R2/Queue/provider implementations depending inward

apps/control-plane-worker
  thin Worker/DO/Queue/Scheduled composition and transport

apps/profile-bridge
  Windows-native local/device/runtime composition

frontend
  React presentation/navigation/query cache; migrated public API types consume generated TypeScript
```

The notification extraction point was accepted in Phase 1B. Remaining fixed extraction points are
normative in `DEVELOPMENT_PLAN.md`: clients in 2A, query in 2D, mailboxes in 2E and devices in 2F.

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

Current accepted mailbox capability is metadata/job oriented. Full message search/body view remains
Target. The planned contract is client-scoped and authorizes before provider fetch; message content
is permitted only in the authorized response/UI and prohibited from ordinary technical channels.

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