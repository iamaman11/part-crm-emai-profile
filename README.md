# Browser Profile Platform

Browser Profile Platform is one modular product for governed users, clients/customer cards, browser profiles, mailbox capabilities, devices and notifications. Rust control-plane code, Cloudflare adapters, Windows Profile Bridge, Camouhost and the React operator UI share one source/data/compatibility lineage while production exposure is controlled independently by Release / Capability Profiles.

```text
source_present != production_enabled
```

## Current execution state

Current accepted code checkpoint before this documentation-only convergence:

```text
protected main = 81fba31e7c78966ec57e098d400d895d26e64dbf
PF-1              ACCEPTED (#466)
PF-2              ACCEPTED; authority correction ACCEPTED (#477 / #471)
PF-3              ACCEPTED provisional baseline; truthfulness correction ACCEPTED (#478 / #431)
FC-6              NEXT PERMITTED STAGE / NOT STARTED BY THIS TRANSACTION
AR-12             NOT STARTED
architecture_complete = false
architecture_form_frozen = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

The authoritative moving SHA is always fresh protected `main`; a stale prose SHA never overrides GitHub. PF-2 and PF-3 are accepted prerequisites, not current implementation work. Their historical trackers remain #471 and #431. The live Functional Closure trackers are #399 and #421.

A historical read-only FC-6 re-baseline and the repository-only Release Set v3 verifier correction #476 occurred before the current PF-2/PF-3 corrections. That history does **not** authorize continuation here: this transaction performs no FC-6 preflight execution, staging mutation, promotion, deployment or rollback.

## Documentation authority

Start with [`docs/INDEX.md`](docs/INDEX.md). Canonical current program authority is [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md).

Key references:

- [`docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) — permanent architecture rules;
- [`docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — architecture-quality and anti-weakening rules;
- [`docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`](docs/OPSCTL_ARCHITECTURE_BOUNDARY.md) — Pure Core / Effect Shell boundary for `opsctl`;
- [`docs/PYTHON_USAGE_BOUNDARY.md`](docs/PYTHON_USAGE_BOUNDARY.md) — Python role/effect policy;
- [`docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`](docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md) / #431 / #478 — provisional PF-3 fitness baseline and correction;
- [`docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md), #399, #421 — Functional Closure;
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — compact developer projection;
- [`docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md`](docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md) — exact-head/guarded-merge acceptance discipline;
- [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md) — execution guardrails.

Historical pre-PF-1/PF-1 evidence remains in #441, #430, #466 and their accepted contracts. Closed trackers and historical AR documents are provenance, not mutable current execution authority.

## Architecture direction

```text
one semantic fact -> one natural owner
bounded contexts + inward dependencies
Pure Core / Effect Shell
observation != policy decision
explicit typed effects/contracts
context-owned persistence
JSON only at explicit versioned boundaries
cutover -> zero live callers -> delete predecessor
specialized production checker + executable negative proof -> required CI
no decorative registry as a second source of truth
Release / Capability Profile = sole production-enable authority
```

PF-2 now consumes raw secret-free observations and derives trust/readiness/outcome in typed Rust. PF-3 no longer carries the decorative free-text fitness registry removed by #478: actual specialized production checkers, their executable negative fixtures/self-tests and required CI callers are the truthful enforcement surface.

PF-3 remains provisional. Final architecture-form freeze follows accepted AR-15 real Windows delivery/updater/LKG proof; AR-16 audits and AR-17 qualifies/authorizes.

## Production capability model

PC-1 is the first production release and is bounded to identity/users, clients/customer cards, browser profiles and bulk operations, client↔profile binding, grants/access, generations/sessions/devices, encrypted profile persistence/restore, real Camoufox, Windows Profile Bridge + AR-15 production-grade updater/delivery, and Core-required audit/health/readiness/observability/recovery foundations.

Mailbox administration, mailbox jobs/automation and outbound mail may remain implemented/tested on the same protected `main` while production-disabled. Later capability profiles enable them through the same architecture and Release / Capability Profile authority.

## Verification discipline

Repository-local fast checks are useful during development, but acceptance is an unchanged exact candidate head under current protected governance. Never substitute an old SHA, old workflow count or stale document for fresh Git/GitHub state.
