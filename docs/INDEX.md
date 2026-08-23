# Documentation Authority Index

**Document status:** GENERATED_PROJECTION  
**Current program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)

This file is navigation only. It does not create program, lifecycle or semantic authority.

## Current state

```text
F1/F2  ACCEPTED
N1     ACCEPTED
#454   NEXT — sole pre-N2 implementation transaction
N2     BLOCKED on #454
N3     BLOCKED on N2
N4     BLOCKED on N3
N5     BLOCKED on N4
PF-1   BLOCKED on N5 + fresh #430 entry reread
AR-12  NOT STARTED
```

Production remains disabled/fail-closed. `source_present != production_enabled` is binding.

Closed PR #428 is superseded PF-2 history only.

## Authority hierarchy

Use the smallest document that owns the question:

1. [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — canonical current program authority.
2. [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) and [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — permanent prospective architecture/quality rules.
3. [`PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`](PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md) — detailed #454/N1…N5 normalization contract.
4. Issue #441 — live mutable pre-PF-1 execution tracker; issue #454 — sole current pre-N2 work item.
5. [`PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`](PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md) / #430 — PF-1 contract and live entry gate.
6. [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md) / #431 — PF-3 fitness/freeze contract.
7. [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md), #399 and #421 — Functional Closure and later FC-6 proof obligations.
8. Bounded subject authorities such as [`ARCHITECTURE.md`](ARCHITECTURE.md), ADRs, data/security/runtime/profile/release contracts and natural Rust/SQL/provider-native owners.

Open PRs, generated projections and historical AR documents never outrank accepted protected `main` plus the hierarchy above.

## Developer navigation

- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — compact current execution projection and “what remains before N2”.
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — PR workflow, local verification, exact-head acceptance and GitHub-plugin/no-`gh` environment guidance.
- [`OPSCTL_ARCHITECTURE_BOUNDARY.md`](OPSCTL_ARCHITECTURE_BOUNDARY.md) — `opsctl` pure-core/effect boundary.
- [`OPSCTL_DOCTOR_CONTRACT.md`](OPSCTL_DOCTOR_CONTRACT.md) — `opsctl doctor` local read-only boundary.
- [`PYTHON_USAGE_BOUNDARY.md`](PYTHON_USAGE_BOUNDARY.md) — Python role/effect policy.
- [`PRODUCT.md`](PRODUCT.md) — product and Production Core boundary.

## Current execution path

```text
#454
-> N2
-> N3
-> N4
-> N5
-> PF-1
-> PF-2
-> PF-3 architecture-forming freeze
-> fresh #399/#421 re-baseline
-> FC-6
-> FC-7
-> AR-12 -> AR-13 -> AR-14 -> AR-15 -> AR-16 -> AR-17
-> PC-1
```

#399/#421 are not extra work to execute before N2; their FC-6 obligations are re-proved after PF-3.

## Projection and history rules

The following are projections/navigation, never independent semantic input:

- `architecture/inventory.json`;
- `docs/status.json`;
- `docs/DEVELOPMENT_PLAN.md`;
- root/docs README and this index.

Historical AR documents, `history/**`, `evidence/**`, `architecture/accepted-phases.json` and Git history preserve provenance. A historical implementation remains executable only when a concrete current consumer or durable/persisted obligation justifies it.
