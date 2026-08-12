# Documentation Authority Index

This index defines which repository documents are normative **now** and which files are historical or
evidence-only. It is intentionally small so current implementation and security truth do not drift
across multiple roadmaps.

## Current repository state

- Accepted repository-local product phase: **Phase 2I**, proven by
  [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- The R1–R9 pre-2J architecture remediation is **CLOSED / ACCEPTED HISTORY** in
  [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md).
- The post-closeout pre-2J product-readiness remediation is **ACTIVE / BLOCKING Phase 2J**, tracked by
  issue #203 and governed by
  [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md).
- Initial repository-owned follow-up findings are **P0=0, P1=5, P2=1**.
- Phase 2J is **blocked / pending repository remediation** and has not started.
- Machine-readable readiness authority: [`status.json`](status.json), with `production_ready=false`.
- Canonical current repository-local security authority: [`THREAT_MODEL.md`](THREAT_MODEL.md).

## Authority hierarchy

1. [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md) is the
   current pre-2J execution authority while issue #203 remains active. It defines Batches 0/A/B/C/D/E/F
   and the conditions required before Phase 2J may return to unblocked/not-started.
2. [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) defines product phase order, ownership and acceptance and
   must reflect the current #203 hold.
3. [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) is the historical
   accepted R1–R9 remediation/closeout record. Its findings remain closed and regression-protected; the
   current product-readiness follow-up does not reactivate them.
4. [`ARCHITECTURE.md`](ARCHITECTURE.md), accepted ADRs and [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md)
   define stable architecture/security/privacy invariants.
5. [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) defines the standalone UI target.
6. [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) records what is accepted on `main`
   and at which evidence level.
7. [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) is the immutable phase
   provenance ledger; [`status.json`](status.json) is the current machine-readable projection.
8. [`THREAT_MODEL.md`](THREAT_MODEL.md) is the canonical current threat model. Phase-specific threat
   documents are accepted evidence/history only.

If these sources disagree, implementation stops and the authority documents are corrected before work
continues. An open branch/PR never outranks accepted `main`.

## Current architecture and capability references

- [`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`](PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md)
- [`ARCHITECTURE.md`](ARCHITECTURE.md)
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md)
- [`REALTIME_NOTIFICATIONS.md`](REALTIME_NOTIFICATIONS.md)
- [`PROFILE_GENERATION_REGISTRY.md`](PROFILE_GENERATION_REGISTRY.md)
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md)
- [`THREAT_MODEL.md`](THREAT_MODEL.md)
- [`TEST_EVIDENCE_INDEX.md`](TEST_EVIDENCE_INDEX.md)

## Historical and evidence context

- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — accepted R1–R9
  architecture closeout history; not the active remediation queue.
- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md) — historical Repository Steps 0–10.
- [`evidence/`](evidence/) — immutable/bounded acceptance evidence.
- [`PHASE2I_THREAT_MODEL.md`](PHASE2I_THREAT_MODEL.md) — Historical accepted Phase 2I evidence; current
  threat authority is `THREAT_MODEL.md`.
- Phase-specific governance/closeout/runbook files preserve the evidence and reasoning of their owning
  phase; they do not become a second current roadmap.

Future CRM/Party work remains future-only in [`FUTURE_DEVELOPMENT.md`](FUTURE_DEVELOPMENT.md) until the
standalone product passes Phase 2J.
