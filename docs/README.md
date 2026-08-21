# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and current-authority hierarchy live in [`INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase:** Phase 2I.
- **Architecture Re-baseline v3:** active, tracked by issue #266.
- **Accepted top-level architecture slices:** AR-0 through AR-11; AR-8A…AR-8F accepted subslices.
- **Current accepted checkpoint:** AR-11 — Release-set / Promotion Architecture.
- **Current architecture slice:** AR-12 — Fresh Rehearsal Environment, **DERIVED CURRENT / NOT STARTED**.
- **Current implementation authority:** Post-AR-11 Functional Closure #399, current prerequisite **PF-1 #430**.
- **Mandatory continuation:** PF-1 #430 -> PF-2 / Draft PR #428 -> PF-3 #431 -> fresh #399/#421 re-baseline -> FC-6 -> FC-7 -> AR-12 implementation entry.
- Issue #375 is closed historical hardening, not a current blocker.
- `architecture_complete=false`.
- `production_core_gate=BLOCKED`.
- `production_ready=false`.

### CURRENT_DELIVERY_MAP

| Delivery dimension | Current status | Scope / gate |
|---|---|---|
| Source implemented | **ACCEPTED THROUGH AR-11** | AR-12 is derived current and NOT STARTED. |
| Accepted on main | **COMPLETE THROUGH AR-11** | AR-11 is latest accepted top-level checkpoint. |
| Staging live | **PARTIAL** | AR-8C foundation is live/smoke-verified only. |
| Production authorized | **NO** | Only AR-17 may authorize `production_core_gate`. |
| Production enabled | **NO** | Only PC-1 may set `production_ready=true` for accepted Core after AR-17. |
| Current blocker | **POST-AR-11 FUNCTIONAL CLOSURE #399** | PF-1/PF-2/PF-3/FC-6/FC-7 precede AR-12 implementation. |
| Next gate | **PF-1 acceptance on protected main** | PF-2 is blocked on PF-1; FC-6 is blocked on PF-3 plus fresh re-baseline. |

`source_present != production_enabled` remains binding.

## Current sources

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — CURRENT_AUTHORITY, issue #266;
- [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — subordinate target architecture/development contract;
- [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md) — subordinate PF-1/PF-2/PF-3/FC-6/FC-7 execution plan;
- [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md) — PF-3 enforcement specification;
- [`../architecture/release-architecture-ar11.json`](../architecture/release-architecture-ar11.json) — accepted Release/Capability Profile authority;
- [`../architecture/runtime-cutover-ar10.json`](../architecture/runtime-cutover-ar10.json) — accepted runtime-cutover authority;
- [`../architecture/d1-evolution-ar9.json`](../architecture/d1-evolution-ar9.json) — accepted D1 evolution authority;
- [`../architecture/credential-authority.json`](../architecture/credential-authority.json), [`../architecture/credential-lifecycle.json`](../architecture/credential-lifecycle.json), [`../architecture/profile-security.json`](../architecture/profile-security.json), [`../architecture/operator-contract.json`](../architecture/operator-contract.json) — current subject authorities;
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — current product/program projection;
- [`ARCHITECTURE.md`](ARCHITECTURE.md) + accepted ADRs — stable architecture invariants;
- [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md) — data/privacy authority;
- [`THREAT_MODEL.md`](THREAT_MODEL.md) — current threat model;
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) — UI architecture;
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) — accepted capability/evidence projection;
- [`status.json`](status.json) — machine-readable state/readiness projection;
- [`../architecture/inventory.json`](../architecture/inventory.json) — tracked generated architecture projection;
- [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) — immutable accepted phase ledger.

## Target architecture / verification

Prospective work converges incrementally rather than by a global rewrite:

```text
canonical authorities
-> typed policy/contracts
-> bounded-context domain + application
-> explicit ports/adapters/effect capabilities
-> composition roots
-> Release / Capability Profile admission
-> production exposure
```

The permanent rules include Single Authority, bounded-context ownership, inward dependencies, provider-free core, typed IDs/state/contracts, command/query separation, explicit effects, context-owned persistence, typed config, versioned integration events, frontend projection only, touch-to-converge and cutover-to-deletion.

PF-3 #431 will make the cross-cutting rules machine-persistent through `architecture/architecture-fitness-policy.json`, Rule IDs, one primary enforcement owner per REQUIRED rule, positive/negative fixtures and an Architecture Fitness Gate. After PF-3, materially architecture-changing PF/FC/AR/PC candidates must declare Architecture Impact and pass applicable REQUIRED rules on the exact candidate head.

No AR-0…AR-17 step may provision/promote production. AR-16 is the final whole-project convergence audit; AR-17 may authorize the Core gate while `production_ready=false`; PC-1 owns first Production Core enablement.

## Historical / evidence sources

- [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md) — explicit predecessor-history stub;
- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — accepted R1–R9 historical closeout;
- [`ARCHITECTURE_REBASELINE_V3_AR0.md`](ARCHITECTURE_REBASELINE_V3_AR0.md) and [`ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md`](ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md) — AR-0 research/evidence;
- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md) and [`evidence/`](evidence/) — historical delivery/evidence.

Issue #203 remains predecessor history; issue #251's old real-production D3 sequence remains superseded forward execution while its repository-side foundation is preserved.

For contributor commands and exact-head acceptance discipline see [`../CONTRIBUTING.md`](../CONTRIBUTING.md).
