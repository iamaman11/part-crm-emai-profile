# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and
current-authority hierarchy live in [`INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase:** Phase 2I.
- **Architecture Re-baseline v3:** active, tracked by issue #266.
- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C, AR-5 and AR-6.
- **Current accepted checkpoint:** AR-6 — Full Python Estate + read-only Rust opsctl.
- **Next slice:** AR-7 — Environments + GitHub Governance + Operational Boundaries.
- **Architecture complete:** `false`.
- **Production Core gate:** `BLOCKED`.
- **Production readiness:** `production_ready=false`.

The single current architecture/program authority is
[`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md). AR-2 runtime-topology
authority is [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json),
and machine transition state is
[`../architecture/architecture-rebaseline-v3-transition.json`](../architecture/architecture-rebaseline-v3-transition.json).

No AR-0…AR-17 step may provision or promote production. AR-16 is the final whole-project P0/P1 audit;
AR-17 may authorize the Production Core gate but still leaves `production_ready=false`; PC-1 is the
first program step that may perform real Production Core mutation.

## Current sources

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — CURRENT_AUTHORITY, issue #266;
- [`ARCHITECTURE_REBASELINE_V3_AR6.md`](ARCHITECTURE_REBASELINE_V3_AR6.md) — accepted AR-6 Python-estate/read-only-opsctl evidence;
- [`../architecture/python-estate-ar6.json`](../architecture/python-estate-ar6.json) — accepted full tracked Python disposition;
- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;
- [`ARCHITECTURE_REBASELINE_V3_AR4C.md`](ARCHITECTURE_REBASELINE_V3_AR4C.md) — accepted AR-4C Outbound Mail composition-extraction evidence;
- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;
- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;
- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 base application architecture evidence;
- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md) — accepted AR-2 topology/D3 evidence;
- [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json) — accepted AR-2 topology/D3 decision input retained by AR-3;
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

Issue #203 remains a predecessor blocker lifecycle rather than the forward program tracker after AR-1.
AR-2 classified issue #251's old production-promotion sequence as superseded forward execution while
preserving its repository-side D3 foundation; AR-6 is accepted, AR-5 remains the runtime-authority cleanup, AR-4C remains the latest application-architecture remediation, AR-4D remains NOT_REQUIRED, and AR-7 is the only next architecture slice.

For contributor commands and exact-head acceptance discipline see [`../CONTRIBUTING.md`](../CONTRIBUTING.md).