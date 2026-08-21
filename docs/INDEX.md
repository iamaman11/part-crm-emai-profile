# Documentation Authority Index

**Document status:** GENERATED_PROJECTION  
**Current program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)  
**Architecture evolution contract:** [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md)  
**Functional Closure plan:** [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md)  
**Tracking:** issue #266

This index classifies repository documents by authority role so a target, historical plan, issue comment or evidence file cannot silently become a second current roadmap.

## Current repository state

- Accepted repository-local product phase: **Phase 2I**, proven by [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- Architecture Re-baseline v3 is the active forward program.
- AR-0 through AR-11 are accepted top-level checkpoints; AR-8A…AR-8F are accepted subslices.
- AR-11 is accepted; AR-12 is the **derived current** architecture slice but implementation is **NOT STARTED**.
- Current implementation authority is Post-AR-11 Functional Closure #399, presently **PF-1 #430**.
- Mandatory prerequisite order is **PF-1 #430 -> PF-2 / Draft PR #428 -> PF-3 #431 -> fresh re-baseline #399/#421 -> FC-6 -> FC-7 -> AR-12 implementation entry**.
- Issue #375 is closed historical hardening and is not a current execution blocker or lifecycle authority.
- Post-AR-8C cleanup / DX issue #352 remains accepted history; AR-4D remains NOT_REQUIRED unless later accepted evidence reopens it.
- `architecture_complete=false`.
- `production_core_gate=BLOCKED`.
- `production_ready=false`.
- Phase 2J and the pre-2J issue #203 sequence are predecessor history, not the current implementation queue.
- AR-2 classified issue #251's old production-promotion sequence as superseded forward execution; its repository-side D3 evidence remains preserved.

## CURRENT_DELIVERY_MAP

Canonical machine projection: `architecture/inventory.json::current_delivery_map`. This section is a human-readable projection, not a second roadmap or release authority.

| Delivery dimension | Current status | Scope / gate |
|---|---|---|
| Source implemented | **ACCEPTED THROUGH AR-11** | AR-11 source is accepted on `main`; AR-12 is derived current and NOT STARTED. |
| Accepted on main | **COMPLETE THROUGH AR-11** | AR-8, AR-9, AR-10 and AR-11 are accepted; AR-11 remains the latest accepted top-level checkpoint. |
| Staging live | **PARTIAL** | AR-8C staging provider/credential foundation is live and smoke-verified only; later architecture acceptance does not imply broader staging or production deployment. |
| Production authorized | **NO** | `production_core_gate=BLOCKED`; only successful AR-17 may authorize the Production Core gate. |
| Production enabled | **NO** | `production_ready=false`; only successful PC-1 after AR-17 authorization may enable `production-core-v1`. |
| Current implementation blocker | **POST-AR-11 FUNCTIONAL CLOSURE #399** | PF-1/PF-2/PF-3 and FC-6/FC-7 remain prerequisites before AR-12 implementation. |
| Next gate | **PF-1 acceptance on protected main** | PF-2 stays blocked until PF-1; FC-6 stays blocked until PF-3 and fresh #399/#421 re-baseline. |

`source_present != production_enabled` is binding. Staging success never implies production authorization or enablement.

## Authority hierarchy

1. [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — single current architecture/program execution authority, tracked by #266.
2. [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — subordinate normative contract for how prospective PF/FC/AR/PC work must evolve the architecture; not a roadmap or capability registry.
3. [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md) — subordinate execution plan for PF-1/PF-2/PF-3/FC-6/FC-7 before AR-12 implementation.
4. [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md) — subordinate PF-3 specification; it requires future `architecture/architecture-fitness-policy.json` and permanent machine enforcement but does not itself alter lifecycle order.
5. [`../architecture/architecture-rebaseline-v3-transition.json`](../architecture/architecture-rebaseline-v3-transition.json) — non-authoritative machine transition projection of Git-derived lifecycle state.
6. Accepted AR/domain machine authorities such as [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json), [`../architecture/d1-evolution-ar9.json`](../architecture/d1-evolution-ar9.json), [`../architecture/runtime-cutover-ar10.json`](../architecture/runtime-cutover-ar10.json), [`../architecture/release-architecture-ar11.json`](../architecture/release-architecture-ar11.json), credential/lifecycle/profile-security/operator contracts — owners of their bounded facts, not competing roadmaps.
7. [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md), [`status.json`](status.json), this index and README entrypoints — current projections; they do not define a competing sequence.
8. [`ARCHITECTURE.md`](ARCHITECTURE.md), accepted ADRs, [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md), [`THREAT_MODEL.md`](THREAT_MODEL.md), [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) — stable normative authorities within their scopes.
9. [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) — capability/evidence projection accepted on `main`; source presence is not production enablement.
10. [`../architecture/inventory.json`](../architecture/inventory.json) — tracked generated architecture projection. It must not become input authority for the facts it projects; PF-1 #430 cuts current generation/check/write authority to `opsctl`.
11. [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) — immutable accepted product-phase provenance.

If these sources disagree, implementation stops and projections are corrected before the next bounded slice proceeds. An open branch/PR never outranks accepted `main`.

## Target architecture / development discipline

Prospective work follows the Architecture Evolution Quality Contract:

```text
canonical authorities
-> typed policy/contracts
-> bounded-context domain + application
-> ports/adapters + explicit effect capabilities
-> composition roots
-> Release / Capability Profile admission
-> production exposure
```

Binding themes include Single Authority, bounded-context ownership, inward dependencies, provider-free domain/application core, typed IDs/state/contracts, command/query separation, explicit effects, context-owned persistence, typed configuration, versioned integration events, frontend-as-projection, Release Profile as sole production enablement authority, touch-to-converge and cutover-to-deletion.

PF-3 #431 makes these rules persistent through machine Rule IDs, primary enforcement owners, positive/negative fixtures and an Architecture Fitness Gate. After PF-3, materially architecture-changing PF/FC/AR/PC candidates must declare Architecture Impact and pass all applicable REQUIRED rules on the exact candidate head.

## Document status model

The machine/document hierarchy uses these roles:

- `CURRENT_AUTHORITY` — current program execution authority;
- `SUBORDINATE_NORMATIVE_CONTRACT` — binding cross-cutting quality rules subordinate to current program/domain authorities;
- `SUBORDINATE_REMEDIATION_PLAN` / `SUBORDINATE_PREREQUISITE_SPEC` — bounded execution/specification documents that cannot alter the canonical lifecycle independently;
- `GENERATED_PROJECTION` — current navigation/status/product projection derived from authority;
- `STABLE_AUTHORITY` — normative within bounded architecture/security/data/product scope, not a roadmap;
- `TARGET` — forward target/research material not independently executable;
- `ACCEPTED_HISTORICAL` — accepted predecessor/history retained for provenance;
- `EVIDENCE` — research or acceptance evidence;
- `RUNBOOK` — operational procedure, not roadmap authority;
- `SUPERSEDED` — old current-looking entrypoint explicitly retired for forward execution.

## Current program references

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)
- [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md)
- [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md)
- [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md)
- [`../architecture/release-architecture-ar11.json`](../architecture/release-architecture-ar11.json)
- [`ARCHITECTURE_REBASELINE_V3_AR10.md`](ARCHITECTURE_REBASELINE_V3_AR10.md)
- [`../architecture/runtime-cutover-ar10.json`](../architecture/runtime-cutover-ar10.json)
- [`evidence/2026-08-19-ar9-final-acceptance.json`](evidence/2026-08-19-ar9-final-acceptance.json)
- [`../architecture/d1-evolution-ar9.json`](../architecture/d1-evolution-ar9.json)
- [`ARCHITECTURE_REBASELINE_V3_AR8.md`](ARCHITECTURE_REBASELINE_V3_AR8.md)
- [`ARCHITECTURE_REBASELINE_V3_AR7.md`](ARCHITECTURE_REBASELINE_V3_AR7.md)
- [`../architecture/github-governance-ar7.json`](../architecture/github-governance-ar7.json)
- [`ARCHITECTURE_REBASELINE_V3_AR6.md`](ARCHITECTURE_REBASELINE_V3_AR6.md)
- [`../architecture/python-estate-ar6.json`](../architecture/python-estate-ar6.json)
- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md)
- [`ARCHITECTURE_REBASELINE_V3_AR4C.md`](ARCHITECTURE_REBASELINE_V3_AR4C.md)
- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md)
- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md)
- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md)
- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md)
- [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json)
- [`ARCHITECTURE_REBASELINE_V3_AR0.md`](ARCHITECTURE_REBASELINE_V3_AR0.md)
- [`ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md`](ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md)
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md)
- [`ARCHITECTURE.md`](ARCHITECTURE.md)
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md)
- [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md)
- [`THREAT_MODEL.md`](THREAT_MODEL.md)
- [`REALTIME_NOTIFICATIONS.md`](REALTIME_NOTIFICATIONS.md)
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md)
- [`status.json`](status.json)
- [`../architecture/inventory.json`](../architecture/inventory.json)
- [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json)

## Historical and evidence context

- [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md) — accepted historical predecessor stub; exact accepted body retained under `history/`.
- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — accepted R1–R9 architecture closeout history.
- `history/DEVELOPMENT_PLAN_PRE_AR_V3_2026-08-15.md` — exact former #203-oriented development plan.
- `history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md` and `history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md` — exact former current-looking root plans.
- `history/ARCHITECTURE_REBASELINE_V3_PLAN_AR0_ACCEPTED_2026-08-15.md` — exact detailed AR-0 plan before AR-1 activation metadata cutover.
- `history/architecture-rebaseline-v3-transition-ar0-accepted-2026-08-15.json` — exact AR-0 machine transition baseline.
- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md) — historical Repository Steps 0–10.
- [`evidence/`](evidence/) — immutable/bounded acceptance evidence.

Historical files are provenance. Statements inside preserved historical bodies that call themselves current/active/canonical describe their former accepted context and do not become current authority after AR-1.
