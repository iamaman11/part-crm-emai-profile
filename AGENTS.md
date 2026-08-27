# Repository Agent Execution Contract

This file is an execution guardrail, not a roadmap or semantic authority. It applies to the whole repository. More specific nested `AGENTS.md` files may narrow local rules but may not weaken this contract.

## 1. Fresh state is mandatory

Before planning or changing anything:

1. read current protected `main`, current branch, merge-base, ahead/behind and working-tree state when a local checkout is available;
2. refresh GitHub PRs, issues, checks, reviews, review threads and branch governance through the available authenticated GitHub tooling;
3. read `docs/INDEX.md`, `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, Issue #266,
   `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`,
   `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md` and the one owning Issue for the live bounded
   transaction;
4. read the relevant current bounded contracts linked from `docs/INDEX.md`, such as
   `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`, `docs/OPSCTL_DOCTOR_CONTRACT.md` or
   `docs/PYTHON_USAGE_BOUNDARY.md`;
5. treat fresh Git/GitHub facts as higher authority than memory, chat history, handoff text, stale prose or an old SHA.

CAP-INDEX #505 and completed AR/PF/PAS/Functional Closure trackers are research/history provenance,
not live execution state. Issue #266 is the sole live transaction pointer.

Do not begin implementation until the current checkpoint and exactly one next permitted bounded concern are identified.

## 2. One transaction at a time

```text
fresh re-baseline
-> one bounded concern
-> identify natural owner, effects, contracts, callers and predecessor
-> implement the smallest coherent change
-> remove replaced predecessor/callers in the same transaction
-> inspect complete diff + simplification ledger
-> targeted proof
-> atomic commit
-> exact-head applicable permanent CI
-> protected required contexts green
-> behind_by = 0
-> reviews/threads clear
-> guarded merge bound to exact head SHA
-> accepted-main reread
-> only then select the next concern
```

Do not jump stages, continue from a superseded head, batch unrelated cleanup, or treat local/old CI as merge acceptance.

## 3. Simplification and negative complexity budget

Every architecture cutover measures whether it reduces semantic/execution ambiguity.

```text
new parallel roadmap/current-plan = 0
new 1:1 successor registry = 0
new global authority bag = 0
new tracked projection without durable exact-byte consumer = 0
new checker-for-checker = 0
legacy predecessor retained only by internal CI/docs/self-test = 0
```

An internal caller, validator, generator, drift gate, self-test or documentation reference that exists only because a legacy artifact exists is deletion scope, not proof of a durable consumer.

## 4. Permanent architecture boundaries

- one semantic fact has one natural owner;
- Product Runtime never depends on `opsctl` or `opsctl-core`;
- `opsctl` is local/offline typed policy, planning, verification and projection tooling, never Product Runtime, daemon, browser launcher, provider client or mutation executor;
- raw JSON/provider responses/effect handles do not cross into pure semantic APIs;
- JSON is only a versioned external contract/manifest, observation/evidence, generated output projection or isolated historical input with a named consumer;
- Python may adapt, observe, generate and test, and may host the owned Camouhost cross-language boundary; it may not become a second product/release/lifecycle/evidence/fitness authority or ungoverned provider mutation path;
- GitHub/Cloudflare/provider effects remain in protected workflows, official provider tooling or explicitly owned outer adapters;
- Release / Capability Profile is the sole production-enable authority;
- PF-3 enforcement truth is the actual specialized production checker + executable negative proof + CI caller, not a decorative metadata registry.

## 5. Current program and authorization discipline

The binding order, stage meaning and gates are defined only by
`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`. Do not infer or copy them from this bootstrap file. Read
fresh Issue #266 and the owning bounded Issue to discover the sole permitted transaction. Each row
requires its own branch/PR, exact-head acceptance, protected-main reread and tracker update before the
next row is selected.

`CODE_COMPLETE != SCENARIO_COMPLETE != PRODUCTION_AUTHORIZED`. E/P/V work does not authorize
provider, staging or Production mutation unless its owning Issue explicitly says so. Production requires
a separate R3 decision by the named authority for one unchanged exact candidate. Historical FC/AR
ceremonies and old readiness observations cannot substitute for that decision.

## 6. Prohibited shortcuts

Do not:

- change architecture merely to satisfy CI;
- weaken/skip/unregister/cosmetically satisfy a protected check;
- create PF-4, a new roadmap, global authority bag, generic plugin/DI/linter framework or speculative compatibility layer;
- preserve historical implementation shape without a proved current/durable obligation;
- use generated projections as semantic inputs;
- invent `expected_current` or infer provider state from stale evidence;
- run promotion/deployment/rollback as a diagnostic;
- perform staging/production mutation outside the explicit owning stage;
- force-push, bypass protected merge or merge a head different from the proven exact head;
- trust this file or a handoff more than fresh accepted Git/GitHub state.

When blocked, report the exact missing authority/evidence/permission. Do not invent a new architecture layer or phase to route around it.

## 7. Completion and handoff

After an accepted merge, reread protected `main`, update only existing live trackers whose mutable state changed, and record exact accepted source/head/tree, CI evidence, review/thread state and next permitted bounded concern. A handoff must instruct the next agent to repeat the fresh re-baseline rather than trust a copied SHA blindly.
