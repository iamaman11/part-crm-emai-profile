# opsctl doctor — Permanent Diagnostic Contract

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT
**opsctl boundary:** [`OPSCTL_ARCHITECTURE_BOUNDARY.md`](OPSCTL_ARCHITECTURE_BOUNDARY.md)
**Production authorization:** NONE

`opsctl doctor` is a small local repository-structure diagnostic. It is not a semantic-authority
registry, lifecycle engine, provider observer, deployment executor, Product Runtime health service or
substitute for exact-head CI.

## Current question and checks

It answers only:

> Does the selected local repository root contain the minimum durable structure needed by current
> opsctl workflows?

Current schema-v2 checks are:

```text
workspace_manifest   -> regular root Cargo.toml
catalog_migrations   -> regular migrations/d1 directory
resolver_migrations  -> regular migrations/resolver-d1 directory
```

Missing paths, symlinks and wrong file kinds fail closed. Output is deterministic versioned JSON with
`command=doctor`, `mode=read-only` and `mutation_executed=false`.

## Effect boundary

Allowed:

```text
explicit repository-root selection
local filesystem metadata/read
stdout/stderr rendering
```

Forbidden:

```text
process/Python/Node/Git/gh/Wrangler execution
network or provider read/write
secret or environment readback
database/deployment/customer-state mutation
runtime/browser execution
generated projection write
```

The implementation must not regain an `AUTHORITIES` list, generic JSON bag, generated-inventory
sentinel or historical AR/Python/Node registry. Adding a diagnostic requires a current local consumer,
one objective structural invariant, a natural owner and positive/negative proof under the future-check
standard in `ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`.

## Relationship to other owners

`doctor` may report a bounded local structural failure, but it never reimplements D1, release,
promotion, lifecycle, evidence, capability or domain policy. Those semantics stay with their typed
owners and command-specific validators.

`status` is separate: it derives a bounded lifecycle projection from explicit caller-supplied
acceptance observations. Neither command observes GitHub/provider state or becomes current factual
authority.

A green `doctor` does not prove branch protection, required checks, reviews, hosted Environments,
provider deployment, staging/Production readiness or any external evidence.

## Required proof

- minimal valid structure passes;
- each required path missing, symlinked or of the wrong kind fails;
- output schema and check IDs remain deterministic;
- process/network/provider/secret/mutation authority remains zero;
- repository-root discovery uses durable source markers, never a generated projection;
- Linux and Windows interpret equivalent repository facts consistently.

The executable Rust implementation and its tests are the current behavior owner. This document owns
the stable boundary, not a copied command registry.
