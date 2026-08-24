# Repository Agent Execution Contract

This file is an execution guardrail, not a roadmap or semantic authority. It applies to the whole repository. More specific nested `AGENTS.md` files may narrow local rules but may not weaken this contract.

## 1. Fresh state is mandatory

Before planning or changing anything:

1. read current protected `main`, current branch, merge-base, ahead/behind and working-tree state when a local checkout is available;
2. refresh GitHub PRs, issues, checks, reviews, review threads and branch governance through the connected GitHub tooling;
3. read `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`, `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md` and the live tracker(s) for the current stage;
4. read relevant bounded contracts such as `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`, `docs/OPSCTL_DOCTOR_CONTRACT.md`, `docs/PYTHON_USAGE_BOUNDARY.md`, PF-3 or Functional Closure plans;
5. treat fresh Git/GitHub facts as higher authority than memory, chat history, handoff text, stale prose or an old SHA.

Current historical prerequisite trackers #441, #471 and #431 are provenance, not live execution state. PF-2 raw-provider-observation correction #480 is accepted source history; Functional Closure uses #399 and #421.

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

## 5. Current stage and freeze discipline

PF-1, PF-2 and PF-3 are accepted prerequisites. PF-2 semantic-authority convergence #477 and raw-provider-observation correction #480 are accepted; PF-3 truthfulness correction #478 is accepted. PF-3 remains provisional.

FC-6 is the next permitted stage only after final readiness audit and a **separate explicit user instruction**. Historical read-only FC-6 work/#476 does not authorize continuation during prerequisite/documentation closeout.

FC-6…AR-15 may make only the smallest correction required by a named failed product/rehearsal scenario. Final architecture-form freeze follows accepted AR-15; AR-16 audits and AR-17 qualifies/authorizes.

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