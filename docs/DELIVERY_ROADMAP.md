# Delivery Roadmap — Historical Repository Steps

**Status:** historical accepted delivery record; not current execution order  
**Date:** 2026-08-08  
**Current execution plan:** [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md)

This document preserves the original Repository Step model and its acceptance discipline.
It is useful for understanding how the standalone foundation was delivered, but it no
longer defines what to implement next.

For all post-composition work, `DEVELOPMENT_PLAN.md` is the only normative execution order.
If this historical roadmap conflicts with that plan on sequencing or future scope, the
current development plan wins.

`IMPLEMENTATION_PLAN.md` and the original lifecycle plans remain design/history inputs;
they do not override the current post-composition plan.

## 1. Historical Acceptance Discipline

Each accepted Repository Step used the same fail-safe delivery model:

1. baseline `main` commit fixed;
2. bounded branch/PR;
3. code/tests/evidence updated together;
4. permanent workflows green on one exact final head;
5. diff/reviews/unresolved threads checked;
6. guarded squash merge;
7. status/evidence updated only for claims actually proven.

External work was never simulated: Cloudflare production resources, physical Windows hosts,
trusted signing, key escrow, legal/provider approval and similar claims require real evidence.

That acceptance discipline remains applicable to the current phased plan.

## 2. Accepted Historical Repository Steps 0–10

### Step 0 — Executable Foundation

Exact Rust toolchain, locked workspace, primitives/tenant scope, Linux/Windows/WASM quality
gates, machine-readable status, security/delivery/evidence governance and external-gate
tracking.

### Step 1 — Cloudflare Cold-Build Spike

Minimal workers-rs Worker, pinned dependencies, Static Assets routing, D1/R2/Queue/DO
binding boundaries, fake bindings and production Worker build path.

### Step 2 — Domain And Contract Skeleton

Opaque IDs, actor/tenant context, identity/client/profile/session/mailbox domains,
application ports/use-case boundaries, versioned contract roots and forbidden-dependency
tests.

### Step 3 — D1 Catalog Foundation

Tenant/membership/client/profile/assignment/grant migrations, typed tenant-scoped
repositories, optimistic versions, idempotency, audit/outbox and migration/isolation tests.

### Step 4 — Identity, Clients And ACL Slice

Access identity adapter, owner lifecycle, invitations/memberships, client/profile metadata,
assignments/grants and initial owner/member API/UI behavior.

### Step 5 — Profile Coordinator

Per-profile Durable Object, monotonic lease epoch/fencing, launch/heartbeat/drain/recovery,
D1 projection and stale-writer rejection.

### Step 6 — Windows Bridge Feasibility

Windows-native Rust executable, custom URI, device-key abstraction, process supervision,
local workspace/SQLite outbox skeleton and fake Camouhost IPC.

### Step 7 — Camouhost Runtime Bundle

Runtime packaging boundary, typed IPC, exact runtime manifest, signed/content-addressed
bundle direction and synthetic create/open/close evidence.

### Step 8 — Local Profile Lifecycle

Safe materialization paths, OS locks, inventory/integrity checks, crash recovery,
forgotten-window/quota policy and generation lifecycle.

### Step 9 — Encrypted Cloud Generations

AEAD generation container, immutable object policy, verification/pointer CAS, restore,
rollback, orphan reconciliation and key/data recovery direction.

### Step 10 — Certification And Multi-Device

Certification policy/matrix, device-scoped unwrap/revoke, multi-device direction and signed
Bridge/runtime update/rollback model.

Exact implementation/evidence level for any capability is read from
`DEVELOPER_CAPABILITY_MATRIX.md`, not inferred from this historical summary.

## 3. Superseded Future Step Sketches

The old roadmap also contained **Step 11 — Mailbox Operations** and **Step 12 — Production
Operations And CRM Adapter**. Those steps were planning sketches and are **superseded as an
execution sequence**.

Their useful requirements were not discarded; they were decomposed and reordered in the
current `DEVELOPMENT_PLAN.md`:

- mailbox/read-model/provider/device work -> current Phases 3–5;
- event/outbox/DLQ foundations -> Phase 1;
- realtime -> Phase 6;
- complete UI/E2E -> Phases 7–8;
- CRM boundary/cutover -> Phase 9;
- production/external evidence -> Phase 10.

Do not open a new implementation slice from the old Step 11/12 wording.

## 4. External Evidence Gates

External gates may progress in parallel but cannot be marked complete by repository code
alone. They include, as applicable:

- legacy credential/provider remediation;
- isolated Cloudflare dev/staging/prod resources and budgets;
- trusted Windows code-signing certificate;
- independent physical Windows evidence hosts;
- root-key escrow/recovery process;
- legal/acceptable-use/provider approval;
- repository/product license decisions;
- real provider/runtime/certification evidence.

The authoritative current requirements for production promotion are in Phase 10 of
`DEVELOPMENT_PLAN.md` and the repository's external-evidence governance.

## 5. Historical Status Rule

A historical step was accepted only after merge and green permanent workflows. A local or
synthetic smoke test never promoted unrelated external claims. The same evidence discipline
continues under the current phased plan.
