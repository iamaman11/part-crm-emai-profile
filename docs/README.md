# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and
current-authority hierarchy live in [`docs/INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase: Phase 2I.** Acceptance provenance:
  [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- **R1–R9 pre-2J architecture remediation: CLOSED / ACCEPTED HISTORY.** Accepted closeout record:
  [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md).
- **Pre-2J product-readiness remediation: ACTIVE / BLOCKING Phase 2J.** Current execution authority:
  [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md), tracked by
  issue #203, with initial repository-owned P0=0, P1=5 and P2=1.
- **Phase 2J is blocked and not started.** It may return to unblocked/not-started only after accepted
  Batch F; External evidence remains unaccepted and cannot be replaced by synthetic/repository-local proof.
- **Production readiness:** `production_ready=false`; machine-readable projection:
  [`status.json`](status.json).

Repository Steps 0–10 are historical delivery history. Use [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md)
and [`evidence/`](evidence/) only for historical provenance, not to decide current implementation order.

## Current normative sources

- [`INDEX.md`](INDEX.md) — documentation authority/governance;
- [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md) — current
  pre-2J remediation execution authority, issue #203;
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — product phase order and acceptance rules;
- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — historical accepted
  R1–R9 remediation and final repository-owned closeout record;
- [`ARCHITECTURE.md`](ARCHITECTURE.md) and accepted ADRs — architecture invariants;
- [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md) — data/privacy classes;
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) — standalone UI target;
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) — accepted capability/evidence level;
- [`status.json`](status.json) — current machine-readable readiness projection;
- [`THREAT_MODEL.md`](THREAT_MODEL.md) — canonical current security threat model;
- [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) — immutable accepted phase ledger.

Phase-specific threat/release/closeout documents, including
[`PHASE2I_THREAT_MODEL.md`](PHASE2I_THREAT_MODEL.md), are accepted evidence/history. They do not replace
`THREAT_MODEL.md` as the current security authority.

For contributor commands and exact-head acceptance discipline see [`../CONTRIBUTING.md`](../CONTRIBUTING.md).
