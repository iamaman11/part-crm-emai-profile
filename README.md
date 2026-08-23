# Browser Profile Platform

Browser Profile Platform is one modular product for governed users, clients/customer cards, browser profiles, mailbox capabilities, devices and notifications. Rust control-plane code, Cloudflare adapters, Windows Profile Bridge, Camouhost and the React operator UI share one source/data/compatibility lineage while production exposure is controlled independently by Release / Capability Profiles.

```text
source_present != production_enabled
```

## Current execution state

Protected `main` has accepted F1/F2 and N1. The sole next implementation transaction is **#454**; N2 is blocked until #454 is accepted.

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 NEXT — resolve real Release Set v2 consumer or retire executable compatibility
-> N2 Python-estate authority retirement
-> N3 GitHub-governance normalization
-> N4 operator/provenance cleanup
-> N5 runtime semantic-authority retirement
-> PF-1 lifecycle + bounded inventory cutover
-> PF-2 Hosted Operational Evidence
-> PF-3 Architecture Fitness + architecture-forming freeze
-> fresh #399/#421 re-baseline
-> FC-6 -> FC-7
-> AR-12 -> AR-13 -> AR-14 -> AR-15
-> AR-16 audit only
-> AR-17 qualification / Production Core gate decision
-> PC-1 Production Core v1
```

AR-12 is **NOT STARTED**. Production remains fail-closed:

```text
architecture_complete=false
production_core_gate=BLOCKED
production_ready=false
production_mutation=false
```

Closed PR #428 is superseded PF-2 history only; PF-2 later starts from accepted PF-1 `main`.

## Documentation authority

Start with [`docs/INDEX.md`](docs/INDEX.md).

The current program authority is [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md). Mutable pre-PF-1 execution state is tracked by issue #441. Detailed bounded contracts live in their subject documents rather than being duplicated here.

Key references:

- [`docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) — permanent prospective architecture rules;
- [`docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`](docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md) — #454 and N1…N5 ownership/retirement contract;
- issue #454 — sole current pre-N2 implementation transaction;
- [`docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`](docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md) / #430 — PF-1;
- [`docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`](docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md) / #431 — PF-3;
- #399 / #421 — Functional Closure obligations and later FC-6 re-baseline;
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — compact developer projection;
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contributor workflow and current GitHub-plugin execution note.

Generated `docs/status.json`, `architecture/inventory.json`, README/index surfaces and `docs/DEVELOPMENT_PLAN.md` are projections, not semantic/lifecycle authority.

## Architecture direction

Permanent themes are intentionally small and strong:

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

Historical AR artifacts preserve evidence, not a permanent right for old JSON/Python/Node implementation shape to remain current architecture.

## Production capability model

PC-1 is bounded to the Production Core: identity/users, clients/customer cards, browser profiles and bulk operations, client↔profile binding, real Camoufox runtime, Windows Profile Bridge, AR-15 updater/delivery, profile persistence/restore, access/grants, audit, health/readiness/observability and required recovery/notification foundations.

Mailbox administration, mailbox jobs/automation and outbound mail may remain implemented and tested on the same `main` while production-disabled. Later PC profiles enable them through the same product architecture and Release / Capability Profile authority.

## Fast local verification

```bash
python scripts/verify-fast.py
python scripts/verify-fast.py --with-compile
```

Permanent acceptance still requires one unchanged exact PR head, all applicable permanent workflows and live protected required contexts green, `behind_by=0`, zero blocking reviews/unresolved threads, merge under the current binding acceptance protocol, and accepted-main reread.
