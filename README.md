# Browser Profile Platform

Browser Profile Platform is one modular product for governed users, client/customer cards, reusable
browser profiles, local browser execution and independently enabled future capabilities.

```text
source_present != production_enabled
```

The first product slice is intentionally finite: managed login, Client creation/editing, persistent
Browser Profile creation, Client/Profile attachment, authorized launch through Windows Profile Bridge
and real Camoufox, confirmed save, reopen and logout. Mailboxes, Notifications, Automation and unknown
future providers do not block that slice.

## Start here

1. [Documentation authority index](docs/INDEX.md)
2. [Product definition](docs/PRODUCT.md)
3. [Architecture](docs/ARCHITECTURE.md)
4. [Current CAP execution program](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md)
5. [Repository agent contract](AGENTS.md) and [contributor guide](CONTRIBUTING.md)

[Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) is the live transaction
pointer. Always read fresh protected `main`, the tracker and the owning bounded Issue; README never
duplicates a current SHA, provider/readiness observation or workflow state.

## Architecture invariants

```text
one semantic responsibility -> one natural owner
domain -> application/ports -> adapters -> composition
Rust contracts -> OpenAPI -> generated frontend operations -> feature UI
source present != production enabled
cut over callers -> prove zero predecessor use -> delete predecessor
one objective invariant -> one primary proof at the cheapest sufficient tier
```

Backend owns authorization, capability admission, business validation and state transitions. Frontend
is a generated-contract consumer and presentation/interaction layer. Product Runtime never depends on
`opsctl`; operator tooling has no provider/network/mutation authority.

Permanent rules live in
[mandatory architecture requirements](docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md),
[architecture evolution quality contract](docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md),
[contract policy](docs/CONTRACT_POLICY.md) and bounded owner documents linked from the index.

## Current delivery program

The complete ordered program, stage meanings and exit criteria have exactly one repository owner: the
[current execution program](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md). The sole active transaction and
accepted-main evidence live in fresh [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266).
Every transaction uses its own bounded Issue/branch/PR and accepted-main reread; this README deliberately
does not copy the mutable order.

## Historical material

Completed AR, PF, PAS and Functional Closure documents and Issues are provenance. They may explain why a
current invariant exists, but they do not select current work or authorize staging/Production actions.
