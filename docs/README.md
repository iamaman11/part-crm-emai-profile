# Documentation entrypoint

Start with `docs/INDEX.md` for navigation and `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` for canonical program authority.

Current execution state is intentionally simple:

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 ACCEPTED
-> N2 -> N3 -> N4 -> N5 ACCEPTED
-> PF-1 ACCEPTED (#466)
-> PF-2 ACCEPTED (#471/#477/#480)
-> PF-3 ACCEPTED provisional (#431/#478)
-> staging-baseline adoption ACCEPTED (#486), temporary mechanism removed (#487)
-> fresh audit recorded FC-6 READY TO BEGIN / NOT STARTED
-> PAS-2/TC-1 executable frontend transport closure
-> FC-6 preflight + staging proof
-> FC-7 closeout
-> AR-12..AR-17 qualification path
-> PC-1 Production Core v1
```

`fresh #399/#421 re-baseline` is a required live observation at FC-6 entry, not another implementation phase. N2–N5 and PF-1/PF-2/PF-3 are accepted prerequisite cutovers; PF-3 remains provisional. Staging-baseline adoption completed through #486 and its temporary mechanism was removed through #487. PAS-2/TC-1 is the subsequently discovered bounded pre-FC-6 correction for executable frontend transport validation and predecessor removal; readiness is repeated after it. AR-12/13/14 are rehearsals, accepted AR-15 establishes the final architecture-form freeze through real Windows delivery/updater proof, AR-16 is audit-only, and AR-17 is qualification/authorization-only as far as practical.

Production remains fail-closed:

```text
architecture_complete=false
production_core_gate=BLOCKED
production_ready=false
production_mutation=false
source_present != production_enabled
```

Use:

- `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` — canonical architecture/program authority;
- #471 — live PF-2 execution state;
- #441 / #430 / #466 — accepted historical pre-PF-1 and PF-1 execution provenance;
- #454 — accepted Release Set v2 correction;
- `docs/DEVELOPMENT_PLAN.md` — compact developer-facing projection and efficiency rules;
- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md` — permanent application architecture contract;
- `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md` — detailed authority-retirement contract;
- `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` / #430 / #466 — accepted PF-1;
- `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431 — PF-3;
- `docs/PAS2_FRONTEND_TRANSPORT_CONTRACT_CLOSURE.md` — bounded pre-FC-6 executable frontend transport correction;
- `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`, #399, #421 — Functional Closure;
- `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` — shared exact-head/guarded-merge acceptance discipline;
- `CONTRIBUTING.md` — developer workflow and current GitHub-plugin/no-local-`gh` execution guidance;
- `docs/PRODUCT.md` — product/capability boundary.

Historical AR documents remain evidence and context. They are not automatically current semantic authorities.
