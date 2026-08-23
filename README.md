# Browser Profile Platform

Browser Profile Platform is one modular product for governed users, clients/customer cards, browser profiles, mailbox capabilities, devices and notifications. Rust control-plane code, Cloudflare adapters, Windows Profile Bridge, Camouhost and the React operator UI share one source/data/compatibility lineage while production exposure is controlled independently by Release / Capability Profiles.

```text
source_present != production_enabled
```

## Current execution state

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 NEXT — sole actual pre-N2 implementation transaction
-> N2 -> N3 -> N4 -> N5 authority retirement
-> PF-1 lifecycle + bounded inventory cutover
-> PF-2 minimal hosted evidence pipeline
-> PF-3 Architecture Fitness + architecture-forming freeze
-> FC-6 preflight (fresh #399/#421 live re-baseline) + staging proof
-> FC-7 closeout
-> AR-12 fresh-environment rehearsal
-> AR-13 rotation rehearsal
-> AR-14 remote-recovery rehearsal
-> AR-15 Windows updater/delivery implementation + proof
-> AR-16 final whole-project audit only
-> AR-17 qualification / Production Core gate decision only
-> PC-1 Production Core v1
```

AR-12 is **NOT STARTED**. Production remains fail-closed:

```text
architecture_complete=false
production_core_gate=BLOCKED
production_ready=false
production_mutation=false
```

N2–N5 are sequential normalization transactions, not new architecture programs. Their initial repository-wide predecessor discovery may be shared as one read-only ephemeral pass after accepted #454. The mandatory post-PF-3 #399/#421 re-baseline executes as FC-6 preflight, not another implementation phase. FC-7 is a closeout checkpoint unless proof exposes a concrete defect.

## Documentation authority

Start with [`docs/INDEX.md`](docs/INDEX.md). Canonical program authority is [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md); mutable pre-PF-1 state is #441.

Key references:

- [`docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) — permanent architecture rules;
- [`docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`](docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md) — #454/N2–N5 retirement contract;
- #454 — sole current pre-N2 implementation transaction;
- [`docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`](docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md) / #430 — PF-1;
- [`docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`](docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md) / #431 — PF-3;
- [`docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md), #399, #421 — Functional Closure;
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — compact developer projection and efficiency rules;
- [`docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md`](docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md) — shared exact-head/guarded-merge acceptance;
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contributor workflow and GitHub-plugin/no-local-`gh` environment guidance.

Generated status/inventory/README/developer-plan surfaces are projections, not semantic/lifecycle authority.

## Architecture direction

```text
one semantic fact -> one natural owner
bounded contexts + inward dependencies
Pure Core / Effect Shell
observation != policy decision
explicit typed effects/contracts
context-owned persistence
generated projection != semantic source
no current consumer + no durable obligation -> no compatibility bridge
cutover -> zero callers -> zero unique current invariants -> delete DEAD predecessor
Release / Capability Profile = sole production-enable authority
```

PF-1 compiler composes validated bounded projections; it does not become a global authority brain. PF-2 stays a minimal evidence pipeline rather than a provider/plugin platform. PF-3 reuses specialized checkers through a small typed enforcement index rather than building a generic linter framework.

Historical AR artifacts preserve evidence, not a permanent right for old JSON/Python/Node implementation shape to remain current architecture.

## Production capability model

PC-1 is the first production release and is bounded to identity/users, clients/customer cards, browser profiles and bulk operations, client↔profile binding, grants/access, generations/sessions/devices, required encrypted profile persistence/restore, real Camoufox, Windows Profile Bridge + AR-15 production-grade updater/delivery, and Core-required audit/health/readiness/observability/recovery foundations.

Mailbox administration, mailbox jobs/automation and outbound mail may remain implemented/tested on the same protected `main` while production-disabled. Later PC profiles enable them through the same architecture and Release / Capability Profile authority.

## Fast local verification

```bash
python scripts/verify-fast.py
python scripts/verify-fast.py --with-compile
```

Permanent acceptance still requires one unchanged exact PR head under current protected governance. Stage documents do not clone that checklist; `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` owns it.
