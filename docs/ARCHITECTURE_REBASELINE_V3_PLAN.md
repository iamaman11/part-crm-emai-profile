# Architecture Re-baseline v3 — Current Program Authority

**Document status:** CURRENT_AUTHORITY  
**Program:** Architecture Re-baseline v3  
**Tracking issue:** #266  
**Subordinate pre-production/tooling issue:** #268  
**Accepted activation prerequisite:** AR-0 / PR #267  
**Accepted AR-0 main:** `e00420704950af5ca9352d2f0f02d3a9c9688527`  
**Current accepted architecture checkpoint:** AR-2 — Runtime Topology + D3 Compatibility  
**Next slice:** AR-3 — Application Architecture Contract  
**Accepted product phase:** Phase 2I  
**Architecture complete:** `false`  
**Production Core gate:** `BLOCKED`  
**Production ready:** `false`

## 1. Authority

This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-2 is the latest accepted checkpoint; its normalized runtime-topology decision is `architecture/runtime-topology-ar2.json` with acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR2.md`.

The accepted AR-0 research package is preserved without rewriting:

- `history/ARCHITECTURE_REBASELINE_V3_PLAN_AR0_ACCEPTED_2026-08-15.md` — exact pre-activation plan body accepted by PR #267;
- `docs/ARCHITECTURE_REBASELINE_V3_AR0.md` — AR-0 evidence;
- `docs/ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md` — second-pass evidence;
- `history/architecture-rebaseline-v3-transition-ar0-accepted-2026-08-15.json` — exact pre-activation transition model.

Pre-2J plans, Repository Steps 0–10, accepted product phases through Phase 2I and their evidence remain accepted history. They are not the current implementation queue when they conflict with this authority.

Stable domain authorities remain authoritative in their own scopes and are not competing program roadmaps: `docs/ARCHITECTURE.md`, accepted ADRs, `docs/DATA_CLASSIFICATION.md`, `docs/THREAT_MODEL.md`, `docs/UI_ARCHITECTURE.md`, generated contract authorities and `architecture/accepted-phases.json`.

`docs/DEVELOPMENT_PLAN.md`, `docs/status.json`, `docs/INDEX.md`, root/docs README files and `architecture/inventory.json` are current projections of this program authority. They must not establish a second execution sequence.

## 2. Binding state model

The following states are independent and must never be collapsed:

```text
architecture_complete
production_core_gate
production_ready
```

During AR-0…AR-17:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
```

After successful AR-16, repository-owned audit severity must be `P0=0` and `P1=0`; no production mutation is authorized.

After successful AR-17:

```text
architecture_complete = true
production_core_gate = AUTHORIZED
production_ready = false
```

Only successful PC-1 may set `production_ready=true`, and only for the accepted `production-core-v1` scope.

## 3. Binding sequence

```text
AR-0   Delta Architecture Inventory                              DONE
AR-1   Architecture Authority Re-baseline                        DONE
AR-2   Runtime Topology + D3 Compatibility                       CURRENT / ACCEPTED CHECKPOINT
AR-3   Application Architecture Contract                         NEXT
AR-4A  Composition-root consolidation
AR-4B  Client Mail route ownership
AR-4C  Outbound Mail composition extraction
AR-4D  Profile extraction only if AR-3 proves benefit
AR-5   Wrangler / Runtime Authority Cleanup
AR-6   Full Python Estate + read-only Rust opsctl
AR-7   Environments + GitHub Governance + Operational Boundaries
AR-8   Secrets / Keys / OAuth Refresh Concurrency
AR-9   D1 Evolution / Schema Compatibility
AR-10  Runtime and Historical Executable Simplification
AR-11  Release-set / Promotion Architecture
AR-12  Fresh Rehearsal Environment
AR-13  Rotation Rehearsal
AR-14  Remote Recovery Rehearsal
AR-15  Windows Release & Update Architecture
AR-16  Final Whole-project 10/10 Audit
AR-17  Architecture Closeout + Production Core Gate
```

The architecture program ends after AR-17. Only then:

```text
PC-1   Production Core v1
PC-2   Mailbox Administration
PC-3   Mailbox Jobs / Automation
PC-4   Outbound / subsequent capabilities
```

No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity.

## 4. Production capability invariant

`source_present != production_enabled` is mandatory.

Production Core v1 is intended to enable only the accepted release profile for:

- authentication / authorization / membership;
- users;
- clients / customer cards;
- browser profiles;
- Camoufox/profile runtime;
- single and bulk browser-profile operations;
- client ↔ browser-profile binding;
- required audit, health, readiness, observability, release and recovery foundations.

Mailbox administration, bulk mailbox operations, client ↔ mailbox binding, mailbox jobs/automation and outbound mail side effects may remain source-present in the same `main` but must remain production-disabled until their later capability gates.

UI visibility is not a security boundary. Production-disabled capabilities must fail closed backend-side.

There is one `main`, one architecture and one schema/compatibility history. No production-lite branch, mailbox fork or second migration lineage is permitted.

The canonical machine hierarchy remains `architecture/inventory.json`; it is extended over time and must not be replaced by a second capability registry.

## 5. Operational authority

Target responsibility split:

- **GitHub Actions / Environments:** orchestration, CI, approvals, workflow concurrency, credential exposure boundary, exact-head evidence and artifact/evidence retention;
- **Rust `opsctl`:** typed project-specific operational semantics after its owning cutover;
- **Wrangler / provider APIs:** actual allowed Cloudflare/provider mutation execution;
- **Python:** validators, generators, fixtures/tests, research/evidence and other explicitly classified helpers.

For every mutable concern:

```text
one concern -> one legitimate mutable authority
```

A legacy Python mutator and an `opsctl` mutator must never remain simultaneously legitimate for the same lifecycle. No global Python→Rust rewrite is authorized.

Terraform and hidden generic-IaC state are forbidden for this program. The intended stack is GitHub Actions / Environments + Rust `opsctl` + Wrangler / Cloudflare APIs.

## 6. Preserved architecture decisions

AR work is delta remediation, not greenfield reconstruction. Existing accepted controls remain unless a bounded AR slice proves a defect, including:

- inward domain/application dependency direction and provider-free pure crates;
- typed D1 boundaries, transaction/schema invariants and migration replay no-op;
- governed writes and identity ACL;
- mailbox atomicity/replay and browser-mail execution invariants;
- immutable R2 generation objects;
- frontend sibling-feature boundaries;
- deterministic generated contracts and architecture inventory drift checks;
- Cloudflare bindings/config checks and immutable release provenance;
- exact-source releases, secret scanning, Rust fmt/clippy/tests and WASM checks.

D1 historical migration provenance is preserved. Fresh bootstrap and upgrade migration are different concerns. One legitimate migration executor is required; a DB-level distributed lock is added only if an independent concurrent executor is proven and cannot be eliminated.

The existing mailbox onboarding / `ReauthRequired` state model remains the OAuth lifecycle authority. AR-8 adds refresh single-flight/CAS/fencing, stale-refresh overwrite prevention and durable provider-revocation reconciliation into that state; it must not introduce a second OAuth state machine.

`MAILBOX_JOBS` Queue + DLQ remain. Recovery is operator reconciliation over current D1 authority, not a parallel mailbox-domain DLQ state machine.

AR-2 additionally establishes these accepted topology inputs without executing provider mutation:

- `GENERATION_VERIFICATION = DELETE`; source/Wrangler binding removal belongs to AR-5 and the queue must not be provisioned by PC-1;
- `INTEGRATION_EVENTS`, `MAILBOX_JOBS` and its DLQ remain real transport boundaries over D1 authority;
- mailbox-secret-resolver Worker + dedicated D1 + service binding remain a deliberate security isolation boundary;
- accepted D3 repository-side bootstrap/same-bits/promotion machinery is preserved as foundation for AR-11;
- the legacy D3 production lane is fail-closed because production mutation remains forbidden through AR-17; PC-1 owns first production provisioning/promotion after AR-17 using the AR-11 release-set authority.

## 7. Accepted AR-1 authority transaction

AR-1 changed governance/authority only. Its accepted merge made all of these true at once:

1. exactly one current architecture/program authority exists — this file, tracked by #266;
2. root and docs entry points project that authority;
3. `docs/status.json` projects the same fail-closed production state;
4. `architecture/architecture-rebaseline-v3-transition.json` records the activated transition;
5. `architecture/inventory.json` is extended, not replaced, with program/document-status authority;
6. historical/current-looking plans are preserved byte-for-byte under `history/` and their old paths are explicit historical/supersession stubs where necessary;
7. documentation checker self-tests reject stale #203 authority, premature production readiness and state drift;
8. architecture inventory generator/checker rejects docs ↔ machine-state disagreement;
9. stable product/runtime/API/schema/provider behavior remains protected.

Issue #203 and its pre-2J plan are accepted predecessor history, not the forward program tracker after AR-1. Its still-open blocker lifecycle remains available to accepted predecessor exception/freeze gates but has no forward execution authority.

## 8. Slice discipline

Every AR slice follows one bounded loop:

```text
research
-> decision authority
-> bounded implementation
-> mechanical verification
-> exact-head acceptance/provenance
```

No mega-PR and no opportunistic future-slice work. New findings are classified into the correct owning AR slice unless they prove a structural sequencing/authority defect.

A slice is not accepted from isolated green jobs. Acceptance requires the complete applicable permanent workflow set on one unchanged exact PR head, `behind_by=0`, clean reviews/threads/Conversation, no unrelated diff, and revalidation of the base immediately before guarded merge.

Any new commit invalidates prior exact-head evidence.

## 9. AR-2 exit / AR-3 entry

AR-2 is complete only after its guarded merge to the then-current `main`, post-merge re-read confirms docs, topology decision, machine transition, inventory, status and issue ledger agree, and predecessor issue #251 is closed as `not_planned` for its superseded forward production sequence while its repository-side history/evidence remains preserved.

Only then may AR-3 begin. AR-3 starts from that newly accepted `main` and owns the canonical runtime-resource/application ownership projection; it must consume `architecture/runtime-topology-ar2.json` rather than invent a competing topology registry. PR #269 remains feasibility material for AR-6 and is not AR-3 authority.