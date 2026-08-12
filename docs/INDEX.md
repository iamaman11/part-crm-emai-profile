# Documentation Authority Index

This index defines which repository documents are normative **now** and which files are historical or
evidence-only. It is intentionally small so current implementation and security truth do not drift
across multiple roadmaps.

## Current repository state

- Accepted repository-local product phase: **Phase 2I**, proven by
  [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- Pre-2J repository-owned remediation is **CLOSED**; Phase 2J is unblocked but not started.
- Machine-readable readiness authority: [`status.json`](status.json), with `production_ready=false`.
- Canonical current repository-local security authority: [`THREAT_MODEL.md`](THREAT_MODEL.md).

## Authority hierarchy

1. [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) is the accepted
   R1–R9 remediation and final repository-owned pre-2J closeout record. Its closed state no longer blocks
   the next product phase and does not itself constitute Phase 2J acceptance.
2. [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) defines product phase order, ownership and acceptance.
3. [`ARCHITECTURE.md`](ARCHITECTURE.md), accepted ADRs and [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md)
   define stable architecture/security/privacy invariants.
4. [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) defines the standalone UI target.
5. [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) records what is accepted on `main`
   and at which evidence level.
6. [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) is the immutable phase
   provenance ledger; [`status.json`](status.json) is the current machine-readable projection.
7. [`THREAT_MODEL.md`](THREAT_MODEL.md) is the canonical current threat model. Phase-specific threat
   documents are accepted evidence/history only.

If these sources disagree, implementation stops and the authority documents are corrected before work
continues. An open branch/PR never outranks accepted `main`.

## Current architecture and capability references

- [`ARCHITECTURE.md`](ARCHITECTURE.md)
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md)
- [`REALTIME_NOTIFICATIONS.md`](REALTIME_NOTIFICATIONS.md)
- [`PROFILE_GENERATION_REGISTRY.md`](PROFILE_GENERATION_REGISTRY.md)
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md)
- [`THREAT_MODEL.md`](THREAT_MODEL.md)
- [`TEST_EVIDENCE_INDEX.md`](TEST_EVIDENCE_INDEX.md)

## Historical and evidence context

- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md) — historical Repository Steps 0–10.
- [`evidence/`](evidence/) — immutable/bounded acceptance evidence.
- [`PHASE2I_THREAT_MODEL.md`](PHASE2I_THREAT_MODEL.md) — Historical accepted Phase 2I evidence; current
  threat authority is `THREAT_MODEL.md`.
- Phase-specific governance/closeout/runbook files preserve the evidence and reasoning of their owning
  phase; they do not become a second current roadmap.

Future CRM/Party work remains future-only in [`FUTURE_DEVELOPMENT.md`](FUTURE_DEVELOPMENT.md) until the
standalone product passes Phase 2J.
