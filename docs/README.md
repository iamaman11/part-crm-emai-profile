# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and
current-authority hierarchy live in [`INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase:** Phase 2I.
- **Architecture Re-baseline v3:** active, tracked by issue #266.
- **Accepted top-level architecture slices:** AR-0 through AR-8; AR-8A…AR-8F are accepted and AR-9 is current.
- **Current accepted checkpoint:** AR-8 — complete secrets / keys / credentials hardening.
- **Current implementation:** AR-9 — D1 Evolution / Schema Compatibility.
- **Architecture complete:** `false`.
- **Production Core gate:** `BLOCKED`.
- **Production readiness:** `production_ready=false`.

### CURRENT_DELIVERY_MAP

Canonical machine projection: `architecture/inventory.json::current_delivery_map`. This section is a human-readable projection, not a second roadmap or release authority.

| Delivery dimension | Current status | Scope / gate |
|---|---|---|
| Source implemented | **ACCEPTED** | AR-8 source is accepted on `main`; AR-9 is the current architecture slice. |
| Accepted on main | **COMPLETE THROUGH AR-8** | AR-8A…AR-8F and final closeout are accepted; `full_ar8_accepted=true`. |
| Staging live | **PARTIAL** | AR-8C staging provider/credential foundation is live and smoke-verified only; later AR-8 acceptance does not imply a broader staging or production deployment. |
| Production authorized | **NO** | `production_core_gate=BLOCKED`; only successful AR-17 may authorize the Production Core gate. |
| Production enabled | **NO** | `production_ready=false`; only successful PC-1 after AR-17 authorization may enable accepted `production-core-v1` scope. |
| Current blocker | **NONE** | AR-8 is accepted; no AR-8 blocker remains. |
| Next gate | **AR-9 acceptance** | AR-9 — D1 Evolution / Schema Compatibility is the current slice. |

`source_present != production_enabled` is mechanically enforced. Staging success never implies production authorization or enablement.

[`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md). AR-2 runtime-topology
authority is [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json),
and machine transition state is
[`../architecture/architecture-rebaseline-v3-transition.json`](../architecture/architecture-rebaseline-v3-transition.json).

No AR-0…AR-17 step may provision or promote production. AR-16 is the final whole-project P0/P1 audit;
AR-17 may authorize the Production Core gate but still leaves `production_ready=false`; PC-1 is the
first program step that may perform real Production Core mutation.

## Current sources

- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — CURRENT_AUTHORITY, issue #266;
- [`ARCHITECTURE_REBASELINE_V3_AR8.md`](ARCHITECTURE_REBASELINE_V3_AR8.md) — accepted AR-8 secrets/keys/credentials hardening evidence;
- [`ARCHITECTURE_REBASELINE_V3_AR7.md`](ARCHITECTURE_REBASELINE_V3_AR7.md) — accepted AR-7 GitHub governance/Environment evidence;
- [`../architecture/github-governance-ar7.json`](../architecture/github-governance-ar7.json) — accepted machine-readable GitHub governance contract;
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
preserving its repository-side D3 foundation; AR-8 is accepted, AR-9 is current, AR-7 remains accepted governance, AR-6 remains the Python/opsctl authority, AR-5 remains the runtime-authority cleanup, AR-4C remains the latest application-architecture remediation, and AR-4D remains NOT_REQUIRED.

For contributor commands and exact-head acceptance discipline see [`../CONTRIBUTING.md`](../CONTRIBUTING.md).