# Documentation Authority Index

**Document status:** GENERATED_PROJECTION  
**Current program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)

Navigation only. This file creates no program/lifecycle/semantic authority. Fresh protected `main` and live GitHub state win over stale prose.

## Current state

```text
PF-1   ACCEPTED (#466)
PF-2   ACCEPTED; semantic-authority correction #477 + raw-provider-observation correction #480 ACCEPTED (#471 provenance)
PF-3   ACCEPTED provisional; truthfulness correction ACCEPTED (#478 / #431)
STAGING BASELINE ACCEPTED (#486); TEMPORARY MECHANISM REMOVED (#487)
PAS-2/TC-1 CURRENT PREREQUISITE BEFORE FC-6
FC-6   DEFERRED / NOT STARTED; PRE-PAS-2 AUDIT RECORDED READY
AR-12  NOT STARTED
production_mutation = false
```

The current protected-main SHA and live provider state must always be reread; this projection never
overrides them.

## Authority hierarchy

1. [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — canonical current program authority.
2. [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) + [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — permanent architecture/quality rules.
3. [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md), #399, #421 — live Functional Closure obligations.
4. [`PAS2_FRONTEND_TRANSPORT_CONTRACT_CLOSURE.md`](PAS2_FRONTEND_TRANSPORT_CONTRACT_CLOSURE.md) — bounded pre-FC-6 PAS-2 correction.
5. [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md), #431, #478 — accepted provisional PF-3 baseline and correction.
6. Accepted historical prerequisites: #471/#477/#480 for PF-2; #441, #430, #466 and their historical contracts for pre-PF-1/PF-1.
7. Bounded subject authorities: Rust/SQL/provider-native/runtime/release/security contracts and ADRs.

Closed trackers and historical evidence never become a second current-state authority merely because they contain old `CURRENT` text.

## Developer navigation

- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — compact execution projection and efficiency rules.
- [`../AGENTS.md`](../AGENTS.md) — repository execution contract.
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — contributor workflow.
- [`OPSCTL_ARCHITECTURE_BOUNDARY.md`](OPSCTL_ARCHITECTURE_BOUNDARY.md) — `opsctl` Pure Core / Effect Shell boundary.
- [`OPSCTL_DOCTOR_CONTRACT.md`](OPSCTL_DOCTOR_CONTRACT.md) — local read-only doctor boundary.
- [`PYTHON_USAGE_BOUNDARY.md`](PYTHON_USAGE_BOUNDARY.md) — Python role/effect policy.
- [`PRODUCT.md`](PRODUCT.md) — product/Production Core boundary.

## Current execution path

```text
accepted PF-2 semantic-authority + raw-provider-observation corrections
-> accepted PF-3 truthfulness correction
-> staging-baseline adoption accepted under temporary #486 authority
-> temporary adoption mechanism removed (#487)
-> fresh audit recorded FC-6 READY TO BEGIN / NOT STARTED
-> PAS-2/TC-1 executable frontend transport contract closure
-> final read-only readiness audit
-> FC-6 may begin only after a separate explicit instruction
-> FC-7 closeout
-> AR-12 fresh-environment rehearsal
-> AR-13 rotation rehearsal
-> AR-14 remote-recovery rehearsal
-> AR-15 Windows updater/delivery + final architecture-form freeze
-> AR-16 audit only
-> AR-17 qualification/authorization decision only
-> PC-1 Production Core v1
```

A historical read-only FC-6 re-baseline and #476 repository-only verifier correction are provenance, not permission to resume FC-6 during this documentation transaction.

## Projection/history rules

The following are projections/navigation, never independent semantic inputs:

- `architecture/architecture-program-sequence.json` — static program order only;
- `docs/status.json` — dated generated/status projection, not current stage authority;
- `docs/DEVELOPMENT_PLAN.md`;
- README/index surfaces.

Historical AR docs, `history/**`, `evidence/**`, `architecture/accepted-phases.json` and Git history preserve provenance. Historical implementation remains executable only for a proved current consumer or durable/persisted/migration obligation.
