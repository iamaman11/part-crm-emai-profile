# Documentation Navigation

This file is a compatibility entrypoint. Use [`INDEX.md`](INDEX.md) for the current documentation hierarchy.

## Current execution

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 NEXT — sole pre-N2 implementation transaction
-> N2 BLOCKED on #454
-> N3 -> N4 -> N5
-> PF-1 -> PF-2 -> PF-3
-> fresh #399/#421 re-baseline
-> FC-6 -> FC-7
-> AR-12
```

AR-12 is **NOT STARTED**. Production remains fail-closed:

```text
architecture_complete=false
production_core_gate=BLOCKED
production_ready=false
production_mutation=false
```

Closed PR #428 is superseded PF-2 history only.

## Start here

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — canonical current program authority;
- issue #441 — live pre-PF-1 execution tracker;
- issue #454 — sole current pre-N2 implementation transaction;
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — compact developer-facing execution projection;
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — contribution workflow, local checks and GitHub-plugin/no-`gh` environment guidance;
- [`PRODUCT.md`](PRODUCT.md) — product/Production Core boundary.

Detailed architecture, normalization, PF and Functional Closure rules are linked from [`INDEX.md`](INDEX.md) and should not be duplicated here.

`docs/status.json`, `architecture/inventory.json`, README/index surfaces and `DEVELOPMENT_PLAN.md` are projections/navigation only. Historical AR/evidence documents preserve provenance and do not override current protected `main` or the canonical hierarchy.

`source_present != production_enabled` remains binding.
