# Documentation Authority Index

**Document status:** GENERATED_PROJECTION  
**Current program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)

Navigation only. This file creates no program/lifecycle/semantic authority.

## Current state

```text
F1/F2  ACCEPTED
N1     ACCEPTED
#454   ACCEPTED
N2–N5  ACCEPTED
PF-1   ACCEPTED (#466)
PF-2   CURRENT (#471)
PF-3   BLOCKED on PF-2
AR-12  NOT STARTED
```

Production remains fail-closed and `source_present != production_enabled` remains binding.

## Authority hierarchy

1. [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — canonical current program authority.
2. [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) + [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — permanent architecture/quality rules.
3. #471 — the single live PF-2 execution tracker; PF-2 has no duplicate Markdown plan.
4. [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md) / #431 — PF-3.
5. [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md), #399, #421 — Functional Closure.
6. Accepted historical contracts/provenance: [`PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`](PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md) / #441 and [`PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`](PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md) / #430 / #466.
7. Bounded subject authorities: Rust/SQL/provider-native/runtime/release/security contracts and ADRs.

Open PRs, generated projections and historical AR documents never outrank accepted protected `main` plus this hierarchy.

## Developer navigation

- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — compact execution model + efficiency rules.
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — developer workflow, exact-head acceptance and GitHub-plugin/no-local-`gh` guidance.
- [`OPSCTL_ARCHITECTURE_BOUNDARY.md`](OPSCTL_ARCHITECTURE_BOUNDARY.md) — `opsctl` pure-core/effect boundary.
- [`OPSCTL_DOCTOR_CONTRACT.md`](OPSCTL_DOCTOR_CONTRACT.md) — local read-only doctor boundary.
- [`PYTHON_USAGE_BOUNDARY.md`](PYTHON_USAGE_BOUNDARY.md) — Python role/effect policy.
- [`PRODUCT.md`](PRODUCT.md) — product/Production Core boundary.

## Current execution path

```text
PF-2 -> PF-3
-> FC-6 preflight (fresh #399/#421 live re-baseline)
-> FC-6 staging proof
-> FC-7 closeout
-> AR-12 fresh-environment rehearsal
-> AR-13 rotation rehearsal
-> AR-14 remote-recovery rehearsal
-> AR-15 Windows updater/delivery implementation + proof + final architecture-form freeze
-> AR-16 audit only
-> AR-17 qualification/authorization decision only
-> PC-1 Production Core v1
```

N2–N5 and PF-1 are accepted historical cutovers. PF-2 stays a minimal evidence pipeline: provider/GitHub/network/credential effects remain outside `opsctl`, which accepts strict secret-free observations and owns pure policy only. PF-3 is a provisional fitness baseline and accepted AR-15 establishes final architecture-form freeze. `fresh #399/#421 re-baseline` is FC-6 preflight, not another implementation phase. FC-7 is closeout unless proof exposes a real defect.

## Projection/history rules

These are projections/navigation, never independent semantic inputs:

- `architecture/architecture-program-sequence.json` for static program order;
- `docs/status.json`;
- `docs/DEVELOPMENT_PLAN.md`;
- README/index surfaces.

Historical AR docs, `history/**`, `evidence/**`, `architecture/accepted-phases.json` and Git history preserve provenance. Historical implementation remains executable only for a proved current consumer or durable/persisted/migration obligation.
