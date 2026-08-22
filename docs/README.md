# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and current-authority hierarchy live in [`INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase:** Phase 2I.
- **Architecture Re-baseline v3:** active, tracked by issue #266.
- **Accepted top-level architecture slices:** AR-0 through AR-11; AR-8A…AR-8F accepted subslices.
- **Current accepted checkpoint:** AR-11 — Release-set / Promotion Architecture.
- **Current architecture slice:** AR-12 — Fresh Rehearsal Environment, **DERIVED CURRENT / NOT STARTED**.
- **Current execution umbrella:** Post-AR-11 Functional Closure #399.
- **Current next work:** F1/F2, then N1…N5. PF-1 #430 is blocked until those transactions are accepted.
- Issue #375 is closed historical hardening, not a current blocker or execution authority.
- `architecture_complete=false`.
- `production_core_gate=BLOCKED`.
- `production_ready=false`.

### Binding continuation

```text
F1 Release Set version discipline
+
F2 permanent application/opsctl/doctor/canonical-JSON/Python foundations
 ->
N1 AR-2 authority retirement
 ->
N2 AR-6 Python-estate authority retirement
 ->
N3 AR-7 current governance normalization
 ->
N4 bounded AR-8 operator/provenance cleanup
 ->
N5 AR-10 runtime semantic-authority retirement
 ->
PF-1 #430
 ->
PF-2 / Draft PR #428
 ->
PF-3 #431
 ->
fresh #399/#421 re-baseline
 ->
FC-6
 ->
FC-7
 ->
AR-12 implementation entry
```

F1/F2/N1…N5 are foundation/normalization transactions, not new AR/PF lifecycle slices. `source_present != production_enabled` remains binding.

## Current sources

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — CURRENT_AUTHORITY, issue #266;
- [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) — permanent prospective application requirements;
- [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — permanent target architecture/development contract;
- [`OPSCTL_ARCHITECTURE_BOUNDARY.md`](OPSCTL_ARCHITECTURE_BOUNDARY.md) — `opsctl` Pure Core / Effect Shell boundary;
- [`OPSCTL_DOCTOR_CONTRACT.md`](OPSCTL_DOCTOR_CONTRACT.md) — permanent diagnostic-only `doctor` boundary;
- [`PYTHON_USAGE_BOUNDARY.md`](PYTHON_USAGE_BOUNDARY.md) — Python role/effect policy;
- [`PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`](PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md) — F1/F2 + N1…N5 prerequisite specification;
- [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md) — subordinate PF/FC execution plan;
- [`PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`](PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md) — PF-1 contract;
- [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md) — PF-3 typed fitness contract;
- [`../architecture/release-architecture-ar11.json`](../architecture/release-architecture-ar11.json) — accepted Release/Capability Profile authority;
- [`../architecture/runtime-cutover-ar10.json`](../architecture/runtime-cutover-ar10.json) — accepted runtime-cutover authority;
- [`../tools/opsctl/src/d1`](../tools/opsctl/src/d1) plus [`../migrations/d1`](../migrations/d1) and [`../migrations/resolver-d1`](../migrations/resolver-d1) — typed D1 policy and executable SQL migration authority;
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md), [`status.json`](status.json), [`../architecture/inventory.json`](../architecture/inventory.json), this README and [`INDEX.md`](INDEX.md) — projection/navigation surfaces only.

Projection lag never overrides current program authority. In particular, a tracked `status.json` or `architecture/inventory.json` snapshot that still names AR-12 as the next direct gate does **not** permit bypassing F1/F2/N1…N5/PF/FC prerequisites.

## Target architecture / verification

```text
canonical natural owner
-> typed policy/contracts
-> bounded-context domain + application
-> explicit ports/adapters/effects
-> composition roots
-> Release / Capability Profile admission
-> production exposure
```

Permanent rules include Single Authority, bounded-context ownership, inward dependencies, provider-free core, typed critical IDs/state/contracts, Pure Core / Effect Shell, explicit effects, context-owned persistence, typed config, versioned external/integration contracts, frontend projection only, touch-to-converge and cutover-to-deletion.

PF-1 must use typed lifecycle evaluation plus bounded minimal projections rather than a global repository authority bag. `architecture/inventory.json` remains generated projection only.

PF-3 semantic authority belongs to typed Rust (`FitnessRuleRegistry` or equivalent natural typed owner) with one primary enforcement owner per REQUIRED rule, positive/negative fixtures, anti-weakening and the Architecture Fitness Gate. A manually maintained semantic `architecture/architecture-fitness-policy.json` is not the target.

`opsctl doctor` remains read-only local diagnostic composition. It cannot become a provider observer, process runner, runtime launcher, second authority catalog or CI substitute.

No AR-0…AR-17 step may provision/promote production. AR-16 is the final whole-project convergence audit; AR-17 may authorize the Core gate while `production_ready=false`; PC-1 owns first Production Core enablement.

## Historical / evidence sources

Historical AR documents, closed execution plans, `history/**`, [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md), [`evidence/`](evidence/) and Git history remain provenance/evidence. They may contain wording that was current when accepted; they do not override the current hierarchy above.

In particular:

- [`PLAN_READINESS_REVIEW.md`](PLAN_READINESS_REVIEW.md) is an early Repository-Step readiness record, not current sequencing authority;
- [`POST_AR11_PRE_AR12_HARDENING_PLAN.md`](POST_AR11_PRE_AR12_HARDENING_PLAN.md) belongs to closed issue #375 and is historical execution evidence;
- root `IMPLEMENTATION_PLAN.md` and `PROFILE_LIFECYCLE_PLAN.md` are superseded forward-execution entrypoints.

For contributor commands and exact-head acceptance discipline see [`../CONTRIBUTING.md`](../CONTRIBUTING.md).
