# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and
current-authority hierarchy live in [`INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase:** Phase 2I.
- **Architecture Re-baseline v3:** active, tracked by issue #266.
- **AR-0:** accepted through PR #267.
- **Current slice after accepted AR-1 merge:** AR-1 — Architecture Authority Re-baseline.
- **Architecture complete:** `false`.
- **Production Core gate:** `BLOCKED`.
- **Production readiness:** `production_ready=false`.

The single current architecture/program authority is
[`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md). Machine transition state is
[`../architecture/architecture-rebaseline-v3-transition.json`](../architecture/architecture-rebaseline-v3-transition.json).

No AR-0…AR-17 step may provision or promote production. AR-16 is the final whole-project P0/P1 audit;
AR-17 may authorize the Production Core gate but still leaves `production_ready=false`; PC-1 is the
first program step that may perform real Production Core mutation.

## Current sources

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — CURRENT_AUTHORITY, issue #266;
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — generated/current product-program projection plus immutable phase provenance;
- [`ARCHITECTURE.md`](ARCHITECTURE.md) + accepted ADRs — stable architecture invariants;
- [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md) — data/privacy authority;
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) — standalone UI target;
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) — accepted capability/evidence level;
- [`status.json`](status.json) — machine-readable current/readiness projection;
- [`THREAT_MODEL.md`](THREAT_MODEL.md) — canonical current threat model;
- [`../architecture/inventory.json`](../architecture/inventory.json) — canonical architecture inventory;
- [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) — immutable accepted phase ledger.

## Historical / evidence sources

- [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md) — explicit
  predecessor-history stub; the exact accepted pre-AR-v3 body is preserved under `history/`;
- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — accepted R1–R9
  historical closeout;
- [`ARCHITECTURE_REBASELINE_V3_AR0.md`](ARCHITECTURE_REBASELINE_V3_AR0.md) and
  [`ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md`](ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md)
  — AR-0 research/evidence;
- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md) and [`evidence/`](evidence/) — historical delivery/evidence.

Issue #203 remains predecessor history rather than the forward program tracker after AR-1. Issue #251
remains an open external predecessor and is classified by AR-2; AR-1 performs no production mutation.

For contributor commands and exact-head acceptance discipline see [`../CONTRIBUTING.md`](../CONTRIBUTING.md).