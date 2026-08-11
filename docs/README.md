# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and
current-authority hierarchy live in [`docs/INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase: Phase 2I.** Acceptance provenance:
  [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- **Pre-2J remediation: ACTIVE / BLOCKING PHASE 2J.** Active blocker/closure rule:
  [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md).
- **Phase 2J is not implementation-active while that plan is ACTIVE.**
- **Production readiness:** `production_ready=false`; machine-readable projection:
  [`status.json`](status.json).

Repository Steps 0–10 are historical delivery history. Use [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md)
and [`evidence/`](evidence/) only for historical provenance, not to decide current implementation order.

## Current normative sources

- [`INDEX.md`](INDEX.md) — documentation authority/governance;
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — product phase order and acceptance rules;
- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — active temporary
  pre-2J blocker while status is ACTIVE;
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
