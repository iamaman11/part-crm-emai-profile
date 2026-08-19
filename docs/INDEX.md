# Documentation Authority Index

**Document status:** GENERATED_PROJECTION  
**Current program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)  
**Tracking:** issue #266

This index classifies repository documents by authority role so a target, historical plan or evidence
file cannot silently become a second current roadmap.

## Current repository state

- Accepted repository-local product phase: **Phase 2I**, proven by
  [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- Architecture Re-baseline v3 is the active forward program.
- AR-0 through AR-9 are accepted top-level checkpoints; AR-8A…AR-8F are accepted subslices.
- AR-9 is accepted; AR-10 is the current implementation slice.
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
| Source implemented | **ACCEPTED** | AR-9 source is accepted on `main`; AR-10 is the current architecture slice. |
| Accepted on main | **COMPLETE THROUGH AR-9** | AR-8 and AR-9 are accepted; AR-9 evidence is `docs/evidence/2026-08-19-ar9-final-acceptance.json`. |
| Staging live | **PARTIAL** | AR-8C staging provider/credential foundation is live and smoke-verified only; later architecture acceptance does not imply a broader staging or production deployment. |
| Production authorized | **NO** | `production_core_gate=BLOCKED`; only successful AR-17 may authorize the Production Core gate. |
| Production enabled | **NO** | `production_ready=false`; only successful PC-1 after AR-17 authorization may enable accepted `production-core-v1` scope. |
| Current blocker | **NONE** | AR-9 is accepted; no predecessor blocker remains. |
| Next gate | **AR-10 acceptance** | AR-10 — Runtime and Historical Executable Simplification is the current slice. |

`source_present != production_enabled` is mechanically enforced. Staging success never implies production authorization or enablement.

## Authority hierarchy

1. [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) is the single current
   architecture/program execution authority, tracked by issue #266.
2. [`../architecture/architecture-rebaseline-v3-transition.json`](../architecture/architecture-rebaseline-v3-transition.json)
   is the machine transition projection of that authority.
3. [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json) is the
   accepted AR-2 topology/D3 decision input; AR-9 is accepted and AR-10 is current; #352 remains accepted history; AR-7 accepts the GitHub governance/Environment boundary, AR-6 accepted the Python/opsctl operational-tooling dimension, AR-5 accepted generation-verification runtime/deployment cleanup, and the application/runtime ownership projection remains accepted through AR-4C.
4. [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) and [`status.json`](status.json) are current projections;
   they do not define a competing execution sequence.
5. [`ARCHITECTURE.md`](ARCHITECTURE.md), accepted ADRs, [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md),
   [`THREAT_MODEL.md`](THREAT_MODEL.md) and [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) remain normative
   within their stable architecture/security/data/UI scopes.
6. [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) records capability/evidence accepted
   on `main`; source presence is not production enablement.
7. [`../architecture/inventory.json`](../architecture/inventory.json) is the canonical machine inventory
   and document-status hierarchy. It is extended rather than replaced.
8. [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) is immutable accepted
   product-phase provenance.

If these sources disagree, implementation stops and the authority projections are corrected before the
next AR slice proceeds. An open branch/PR never outranks accepted `main`.

## Document status model

The machine inventory uses these roles:

- `CURRENT_AUTHORITY` — current program execution authority;
- `GENERATED_PROJECTION` — current navigation/status/product projection derived from authority;
- `STABLE_AUTHORITY` — normative within a bounded architecture/security/data/product scope, not a roadmap;
- `TARGET` — forward target/research material not independently executable;
- `ACCEPTED_HISTORICAL` — accepted predecessor/history retained for provenance;
- `EVIDENCE` — research or acceptance evidence;
- `RUNBOOK` — operational procedure, not roadmap authority;
- `SUPERSEDED` — old current-looking entrypoint explicitly retired for forward execution.

## Current program references

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)
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

- [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md) —
  ACCEPTED_HISTORICAL predecessor stub; exact accepted body is retained in
  `history/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN_PRE_AR_V3_2026-08-15.md`.
- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — accepted R1–R9
  architecture closeout history.
- `history/DEVELOPMENT_PLAN_PRE_AR_V3_2026-08-15.md` — exact former #203-oriented development plan.
- `history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md` and
  `history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md` — exact former current-looking root plans.
- `history/ARCHITECTURE_REBASELINE_V3_PLAN_AR0_ACCEPTED_2026-08-15.md` — exact detailed AR-0 plan before
  AR-1 activation metadata cutover.
- `history/architecture-rebaseline-v3-transition-ar0-accepted-2026-08-15.json` — exact AR-0 machine
  transition baseline.
- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md) — historical Repository Steps 0–10.
- [`evidence/`](evidence/) — immutable/bounded acceptance evidence.

The historical files are evidence/provenance. Statements inside preserved historical bodies that call
themselves “current”, “active” or “canonical” describe their former accepted context and are not current
authority after AR-1. AR-2 does not rewrite that history.