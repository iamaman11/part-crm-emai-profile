# Developer Capability and Module Matrix

**Status:** normative accepted implementation/evidence orientation  
**Date:** 2026-08-08  
**Execution order:** [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md)

## 1. Purpose

This matrix answers **what is actually implemented and at what evidence level on accepted
`main`**. It does not define what should be implemented next.

A feature branch or PR description is not accepted implementation evidence. A capability
becomes an accepted `main` claim only after the bounded diff is merged with the required
exact-head permanent CI/evidence.

## 2. Evidence Levels

| Level | Meaning |
|---|---|
| **Composed** | Wired into the accepted executable composition root and covered by its CI lane. |
| **Library** | Typed reusable implementation exists but is not fully wired into the accepted user path. |
| **Synthetic** | Invariants/protocol are proven with deterministic fake/generated evidence; real provider/runtime is not claimed. |
| **Target** | Normatively planned but executable implementation is absent/incomplete. |
| **External** | Requires real provider, physical host, policy, signing or independent evidence outside repository-local CI. |

No level by itself means production readiness.

## 3. Accepted Capability Matrix

| Capability | Level on accepted `main` | Accepted scope | Still Target / External |
|---|---|---|---|
| Rust workspace / primitives | Composed | Exact toolchain, typed opaque IDs, tenant/actor context, positive versions, strict lint/policy gates. | External runtime is not required for this claim. |
| Identity / memberships / ACL | Composed | Access identity adapter, memberships, owner bootstrap/transfer, invitation create/accept, membership status lifecycle, profile/client grants, neutral disclosure and governed D1 commands. Identity governance plus verified-identity ceremonies are independently Cargo-isolated in `use-cases-identity`; profile/client grant/revoke remain application-owned behind thin capability transports. | Production Access/IdP deployment is External; later client/profile/mailbox application crate extraction remains just-in-time Target work only where growth pressure justifies it. |
| Client Registry | Composed | Current create/query/assignment/grant metadata paths and D1 schema. Client create/query and client grant/revoke are application-owned; profile assignment remains non-authorizing. | Registry 2.0 contacts/merge/richer lifecycle and CRM Party authority are Target. |
| Profile catalog | Composed | Current create/query/grant/assignment metadata paths, typed profile state and active generation pointer. Create/query, assignment and profile grant/revoke are application-owned; assignment remains non-authorizing. | Remaining Phase 0 convergence is outside these accepted profile catalog paths. |
| Profile generation registry | Composed | Governed metadata register/query/verify/activate/deactivate/quarantine, replay/evidence, audit/outbox and pointer integrity. | Production R2 verification/device unwrap/cross-device evidence is External. |
| Profile coordinator | Composed | Durable Object journal, sequence/version/epoch/fencing, timeout/drain/recovery, application-thin HTTP ingress and D1 projection with permanent Step-5 boundary enforcement. | Remote production concurrency evidence is External. |
| Full Profile Bridge operator flow | Composed / Synthetic | Explicit synthetic executable composes claim, fake device identity/enrollment, coordinator lease, generation ownership, writer lock, local lifecycle, fake Camouhost protocol and failure ordering. | Real Camoufox, production device keys, remote enrollment/coordinator and production R2 lifecycle are External. |
| Device identity/key ports | Synthetic | Typed ports and deterministic fake implementations. | Production CNG/DPAPI/TPM protection/revoke/recovery is External. |
| Camouhost IPC/process supervision | Synthetic | Versioned typed messages, fake Camouhost, process state machine and clean-stop evidence. | Real bundled Python/Camoufox lifecycle on physical host is External. |
| Runtime bundle | Synthetic | Manifest/inventory/path safety/digest/approval/rollback tests and synthetic selection. | Trusted signed distribution/update channel is External. |
| Local profile lifecycle | Library / Synthetic | Workspace marking, inventory, lock ownership, clone-only recovery, quota/support policy and composed synthetic operator tests. | Full real-browser/kernel-lock physical-host evidence is External. |
| Encrypted cloud generations | Synthetic | XChaCha20-Poly1305 container, immutable in-memory lifecycle, pointer/rollback/quarantine/orphan policy. | Production R2 adapter/device unwrap/remote recovery atomicity evidence is External. |
| Certification | Synthetic | Typed policy, deterministic matrix, drift/prohibited outcomes, privacy-safe summary/update rollback state. | Real Camoufox observations and independent certification are External. |
| Mailbox operations | Composed / Synthetic | Provider-neutral binding/job domain, D1 persistence, secret-handle-only DTOs, idempotency/audit/outbox, Worker metadata/job paths and synthetic provider-decision path. | Real Gmail/IMAP/browser execution, message search/body retrieval, production scheduling and provider evidence are Target/External. |
| React web UI | Composed / Synthetic | Accepted React/Vite/TS operator shell and current session/client/profile/ACL/assignment/generation/coordinator/mailbox/user surfaces with permanent Frontend Gate. The migrated session/client/problem/mutation public contract slice is generated deterministically from the accepted Rust contract, and sibling-feature internal/alias imports are fail-closed. | Remaining public contract coverage expands incrementally with its backend slices; complete detail/list routes, client Mail search/body UI and real Bridge/provider deployment remain Target/External. |
| Cross-component standalone acceptance | Composed / Synthetic | Metadata-only deterministic manifest/validator covering governed D1, generation integrity, Worker/adapters native+WASM, synthetic Bridge and frontend tests/build. Permanent lanes enforce thin identity/client/profile/coordinator Worker boundaries, assignment-as-ACL negative evidence, capability-owned fail-closed route composition and deterministic architecture-inventory consistency. | Real deployment/provider/device evidence is External. |
| Integration events / durable notifications | Composed / Synthetic | Accepted Phase 1A versioned envelope, evolved D1 outbox, metadata-only `notification_events`, tenant/consumer/outbox idempotency, sanitized Queue dispatch, thin scheduled/Queue ingress, canonical-source guards and deterministic duplicate/failure-order evidence. | Phase 1B retry/backoff/max-attempt/DLQ/catch-up/retention and real provider/device/realtime delivery remain Target/External. |
| Client Registry 2.0 | Target | Target model/constraints defined in current plan. | Phase 2 implementation not accepted. |
| Read models/global search | Target | CQRS-lite/query boundary and search targets defined. | Phase 3 implementation not accepted. |
| Client-scoped mailbox message search/body | Target | Product/query/security contract is normative. | Provider-neutral query implementation and real provider/browser lanes are Phases 3–5. |
| Realtime UserNotificationHub | Target | Durable-event-backed topology is normative. | Phase 6 implementation not accepted. |
| Complete standalone UI/E2E | Target | UI/acceptance target is normative. | Phases 7–8 implementation not accepted. |
| CRM integration | Target | Contract/event isolation, Party reference and replaceable-adapter direction are defined. | CRM Party projection/OIDC/PostgreSQL/cutover not accepted. |
| Production readiness | External | Evidence intake/readiness interlocks exist. | Required external evidence remains incomplete; `production_ready=false`. |

## 4. Current Application-Boundary Convergence

Accepted capability behavior and clean application ownership are separate claims. Phase 0 moves
provider-independent orchestration behind application use cases without changing public behavior.

Accepted Phase 0 slices through **0N** establish application ownership for client create/query/grant,
profile create/query/assignment/grant, mailbox binding/job, generation, identity governance/
ceremonies and coordinator ingress, plus the first real compile-time application Cargo boundary,
generated frontend contract/feature-boundary enforcement, modular fail-closed route ownership and
a deterministic machine-readable architecture/docs inventory. Phase 0 is complete on accepted
`main`.

As of this matrix date:

- accepted `main` includes Phase 0K application-thin coordinator ingress with permanent Step-5 ownership enforcement;
- accepted Phase 0L places identity governance plus verified-identity ceremonies in independent `use-cases-identity`, proven by native tests, explicit Workers-WASM compile and composed regressions;
- `identity_acl` intentionally remains in shared `use-cases` because its current helpers cross client/profile contexts;
- accepted Phase 0M uses `control-plane-contract` as the canonical migrated public Rust transport source, commits deterministic OpenAPI/TypeScript output, consumes generated types on real frontend API surfaces, and permanently rejects sibling-feature internals plus resolver-alias bypasses;
- accepted Phase 0N splits route matching into capability-owned classifiers behind one composed fail-closed entrypoint, prevents unknown `/api/*`, `/auth/*` and `/bridge/*` variants from reaching SPA assets, and permanently verifies deterministic `architecture/inventory.json` plus selected documentation consistency claims;
- accepted Phase 1A composes the versioned integration-event envelope, evolved durable outbox, metadata-only notification persistence, Queue dispatcher/consumer and durable consumer idempotency behind provider-neutral ports, with canonical-source and payload-sanitization evidence on native/WASM paths;
- the current execution plan, not this matrix, determines subsequent order; Phase 2A is the next sequential slice, while Phase 1B is eligible dependency-independently and remains required before real async provider/device and realtime execution.

Do not interpret a feature branch's `Composed` wiring or PR description as accepted `main`.

## 5. Current Module Ownership

```text
crates/primitives
  stable provider-neutral value objects

crates/*-domain
  pure provider-independent invariants/state machines

crates/application-ports
  capability-owned interfaces required by application workflows, including accepted integration-event outbox/publisher/notification/idempotency ports

crates/control-plane-contract
  accepted canonical public control-plane transport contract, deterministic OpenAPI export and capability-owned fail-closed route classifiers behind one composed entrypoint

crates/use-cases-identity
  accepted independent identity governance + verified-identity ceremony application context

crates/use-cases
  accepted shared application crate for remaining contexts; identity modules are compatibility re-exports only; Phase 1A dispatcher and foundation consumer semantics are application-owned here

crates/cloudflare-adapters
  D1/Access/DO/R2/Queue/provider implementations that depend inward, including the accepted Phase 1A D1 integration-event repository and Queue publisher adapter

apps/control-plane-worker
  thin Worker/DO/Queue/Scheduled composition and transport; coordinator and accepted Phase 1A event ingress remain application-thin on accepted main

apps/profile-bridge
  Windows-native local/device/runtime composition

frontend
  React presentation/navigation/query cache; migrated public API types consume committed generated TypeScript, with permanent sibling-feature boundary enforcement
```

A rule expressible without a provider belongs in domain/application code, not an adapter/UI.
Lease/fencing/session state belongs to `session-domain`; a Durable Object is its runtime
coordination adapter and is not a second business catalog.

## 6. Current End-To-End Boundaries

### Browser/API path

```text
React / same-origin request
  -> fail-closed route classification
  -> Access identity verification
  -> live membership/grant resolution or verified pre-membership ceremony context
  -> capability application use case
  -> typed adapter
  -> governed durable result/projection
```

The Worker always re-authorizes. UI does not invent authorization or storage access. For owner
transfer, invitation create and membership status, non-owner denial remains neutral before
request-body parsing; application use cases own authorization, checked versions, command domains
and exact replay sequencing. Owner bootstrap and invitation accept use separate verified-identity
ceremony contracts so a transient actor context used for evidence generation is never treated as
membership authorization. Concrete D1 mutation types remain adapter-only.

### Profile generation/runtime path

Catalog generation metadata and synthetic Bridge/runtime composition are repository-tested,
but real R2/device/Camoufox behavior remains External. D1 is authoritative for the active
generation pointer; DO/session fencing controls writer concurrency; local workspace is
materialization/cache/recoverable dirty state, not cloud authority.

### Mailbox path

Current accepted mailbox capability is metadata/job oriented. Full message payload search/body
view is a Target product capability. The planned contract is client-scoped and authorized
before provider fetch; message body is allowed in the authorized product view but not logs,
audit/events/telemetry.

## 7. Definition Of A Complete New Capability

A capability is not accepted as Composed until all applicable items exist:

1. versioned public/internal contract;
2. pure domain decision where provider-independent policy exists;
3. minimal owned application ports;
4. application authorization/idempotency/version sequencing;
5. concrete adapter/migration where required;
6. executable composition wiring;
7. replay/failure/forbidden-access/boundary tests;
8. permanent CI policy where the boundary matters;
9. matrix/docs updated only for proven claims;
10. exact-head green + bounded review + guarded merge;
11. real external evidence only for provider/physical/runtime claims.

## 8. Documentation Authority

- execution order: `DEVELOPMENT_PLAN.md`;
- stable architecture: `ARCHITECTURE.md` + accepted ADRs;
- data handling: `DATA_CLASSIFICATION.md`;
- product/UI target: `UI_ARCHITECTURE.md`;
- accepted implementation level: this matrix;
- historical delivery: `DELIVERY_ROADMAP.md`.

See [`INDEX.md`](./INDEX.md) for the repository documentation map.