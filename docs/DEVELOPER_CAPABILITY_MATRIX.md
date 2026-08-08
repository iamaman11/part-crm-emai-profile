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
| Identity / memberships / ACL | Composed | Access identity adapter, memberships, owner lifecycle, invitations, profile/client grants, neutral disclosure, governed D1 commands. Profile grant/revoke is application-owned behind a thin profile Worker transport; legacy direct Worker grant orchestration is permanently rejected. | Production Access/IdP deployment is External; client-grant and identity-governance application-boundary convergence remains Phase 0 work. |
| Client Registry | Composed | Current create/query/assignment/grant metadata paths and D1 schema. | Registry 2.0 contacts/merge/richer lifecycle and CRM Party authority are Target. |
| Profile catalog | Composed | Current create/query/grant/assignment metadata paths, typed profile state and active generation pointer. Create/query, assignment and profile grant/revoke are application-owned; assignment remains non-authorizing. | Remaining Phase 0 convergence is outside these accepted profile catalog paths. |
| Profile generation registry | Composed | Governed metadata register/query/verify/activate/deactivate/quarantine, replay/evidence, audit/outbox and pointer integrity. | Production R2 verification/device unwrap/cross-device evidence is External. |
| Profile coordinator | Composed | Durable Object journal, sequence/version/epoch/fencing, timeout/drain/recovery and D1 projection. | Phase 0 coordinator-ingress thinness is Target; remote production concurrency evidence is External. |
| Full Profile Bridge operator flow | Composed / Synthetic | Explicit synthetic executable composes claim, fake device identity/enrollment, coordinator lease, generation ownership, writer lock, local lifecycle, fake Camouhost protocol and failure ordering. | Real Camoufox, production device keys, remote enrollment/coordinator and production R2 lifecycle are External. |
| Device identity/key ports | Synthetic | Typed ports and deterministic fake implementations. | Production CNG/DPAPI/TPM protection/revoke/recovery is External. |
| Camouhost IPC/process supervision | Synthetic | Versioned typed messages, fake Camouhost, process state machine and clean-stop evidence. | Real bundled Python/Camoufox lifecycle on physical host is External. |
| Runtime bundle | Synthetic | Manifest/inventory/path safety/digest/approval/rollback tests and synthetic selection. | Trusted signed distribution/update channel is External. |
| Local profile lifecycle | Library / Synthetic | Workspace marking, inventory, lock ownership, clone-only recovery, quota/support policy and composed synthetic operator tests. | Full real-browser/kernel-lock physical-host evidence is External. |
| Encrypted cloud generations | Synthetic | XChaCha20-Poly1305 container, immutable in-memory lifecycle, pointer/rollback/quarantine/orphan policy. | Production R2 adapter/device unwrap/remote recovery atomicity evidence is External. |
| Certification | Synthetic | Typed policy, deterministic matrix, drift/prohibited outcomes, privacy-safe summary/update rollback state. | Real Camoufox observations and independent certification are External. |
| Mailbox operations | Composed / Synthetic | Provider-neutral binding/job domain, D1 persistence, secret-handle-only DTOs, idempotency/audit/outbox, Worker metadata/job paths and synthetic provider-decision path. | Real Gmail/IMAP/browser execution, message search/body retrieval, production scheduling and provider evidence are Target/External. |
| React web UI | Composed / Synthetic | Accepted React/Vite/TS operator shell and current session/client/profile/ACL/assignment/generation/coordinator/mailbox/user surfaces with permanent Frontend Gate. | Generated public TS contracts, sibling-feature import enforcement, complete detail/list routes, client Mail search/body UI, real Bridge/provider deployment remain Target/External. |
| Cross-component standalone acceptance | Composed / Synthetic | Metadata-only deterministic manifest/validator covering governed D1, generation integrity, Worker/adapters native+WASM, synthetic Bridge and frontend tests/build. The permanent lane also enforces the thin profile Worker boundary while retaining assignment-as-ACL negative evidence. | Real deployment/provider/device evidence is External. |
| Integration events / durable notifications | Target | Architecture/plan contracts defined. | Phase 1 implementation not accepted. |
| Client Registry 2.0 | Target | Target model/constraints defined in current plan. | Phase 2 implementation not accepted. |
| Read models/global search | Target | CQRS-lite/query boundary and search targets defined. | Phase 3 implementation not accepted. |
| Client-scoped mailbox message search/body | Target | Product/query/security contract is normative. | Provider-neutral query implementation and real provider/browser lanes are Phases 3–5. |
| Realtime UserNotificationHub | Target | Durable-event-backed topology is normative. | Phase 6 implementation not accepted. |
| Complete standalone UI/E2E | Target | UI/acceptance target is normative. | Phases 7–8 implementation not accepted. |
| CRM integration | Target | Contract/event isolation, Party reference and replaceable-adapter direction are defined. | CRM Party projection/OIDC/PostgreSQL/cutover not accepted. |
| Production readiness | External | Evidence intake/readiness interlocks exist. | Required external evidence remains incomplete; `production_ready=false`. |

## 4. Current Application-Boundary Convergence

Accepted capability behavior and clean application ownership are separate claims. Some
capabilities are already Composed through legacy Worker orchestration while Phase 0 is moving
that orchestration behind application use cases without changing public behavior.

Accepted Phase 0 slices through 0H established the pattern for client/profile create/query,
mailbox binding/job, generation, profile assignment and profile grant/revoke paths.

As of this matrix date:

- Phase 0H profile-grant application-boundary ownership is accepted through #92 / PR #93;
- later client-grant, identity-governance and coordinator-ingress convergence remains Target;
- the current execution plan, not this matrix, determines their order.

Do not interpret a feature branch's `Composed` wiring or PR description as accepted `main`.

## 5. Current Module Ownership

```text
crates/primitives
  stable provider-neutral value objects

crates/*-domain
  pure provider-independent invariants/state machines

crates/application-ports
  capability-owned interfaces required by application workflows

crates/use-cases
  current accepted application crate with capability modules
  (Phase 0 target: split independent high-growth contexts into real Cargo crates)

crates/cloudflare-adapters
  D1/Access/DO/R2/Queue/provider implementations that depend inward

apps/control-plane-worker
  Worker/DO composition and transport; Phase 0 continues removing legacy orchestration

apps/profile-bridge
  Windows-native local/device/runtime composition

frontend
  React presentation/navigation/query cache; generated public contracts are a Phase 0 target
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
  -> live membership/grant resolution
  -> application/legacy bounded path depending on accepted Phase 0 migration status
  -> typed adapter
  -> governed durable result/projection
```

The Worker always re-authorizes. UI does not invent authorization or storage access. For
`ProfileGrantApi`, owner denial remains neutral before request-body parsing, the profile
application use case owns version/idempotency/replay sequencing and only the adapter maps the
concrete governed D1 grant/revoke mutation.

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
