# Repository Agent Execution Contract

This file is an execution guardrail, not a roadmap or semantic authority. It applies to the whole repository. More specific nested `AGENTS.md` files may narrow local implementation rules but may not weaken this contract.

## 1. Fresh state is mandatory

Before planning or changing anything:

1. read the current protected `main`, current branch, merge-base, ahead/behind and working-tree state;
2. refresh GitHub PRs, issues, checks, reviews, review threads and branch governance through the connected GitHub tooling;
3. read `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`, `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md` and live tracker #441;
4. read the relevant bounded contract, especially `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`, `docs/OPSCTL_DOCTOR_CONTRACT.md`, `docs/PYTHON_USAGE_BOUNDARY.md`, PF-1/PF-3 or Functional Closure plans when the scope touches them;
5. treat fresh Git/GitHub facts as higher authority than memory, chat history, a handoff prompt, stale prose or an old SHA.

Do not begin implementation until the current checkpoint and exactly one next permitted bounded concern are identified.

## 2. One transaction at a time

Required flow:

```text
fresh re-baseline
-> one bounded concern
-> identify natural owner, effects, contracts, callers and predecessor
-> implement the smallest coherent change
-> inspect the complete diff and simplification ledger
-> targeted local proof
-> one atomic commit
-> exact-head applicable permanent CI
-> behind_by = 0
-> blocking reviews = 0
-> unresolved threads = 0
-> guarded merge bound to expected head SHA
-> accepted-main reread
-> only then select the next concern
```

Do not batch N2/N3/N4/N5/PF work into a mega-PR, jump phases, continue from a superseded head or treat a green local check as merge acceptance.

## 3. N2 through PF-3 means delete and simplify

Every N2…PF-3 transaction must measure before/after:

```text
current semantic authorities
transitional semantic sources
duplicate representations
legacy current callers
Python/Node semantic-authority paths
tracked generated projections
compatibility-only commands/workflows
current plan + validator + projection LOC
```

N2…N5 must strictly reduce their predecessor estate. PF-1…PF-3 may add only their named typed owner/enforcement and must delete the replaced machinery in the same accepted transaction. Green CI does not excuse a larger duplicate estate.

An internal CI caller, validator, generator, drift gate, self-test, documentation reference or `opsctl` sentinel that exists only because a legacy artifact exists is deletion scope, not a durable consumer. Only a named runtime/external/persisted contract that requires the exact legacy shape/bytes may justify retention.

Use one bounded reachability/invariant pass plus affected deltas. Switch or delete internal callers and delete the predecessor atomically. Do not create a successor census, compatibility tail, meta-checker, follow-up cleanup phase or parallel plan.

## 4. Permanent architecture boundaries

- one current semantic fact has one natural owner;
- Product Runtime never depends on `opsctl` or `opsctl-core`;
- `opsctl` is local/offline typed policy, planning, verification and projection tooling, never Product Runtime, a daemon, browser launcher, network/provider client or mutation executor;
- `serde_json::Value`, raw JSON, filesystem paths, provider responses and effect handles do not cross into pure semantic APIs;
- JSON is only a versioned external contract/manifest, observation/evidence, generated output projection or isolated historical input with a named consumer;
- Python may adapt, observe, generate and test, and may host the owned Camouhost cross-language runtime boundary; it may not become a second product/release/lifecycle/evidence/fitness authority or ungoverned provider mutation path;
- Cloudflare/GitHub/provider effects remain in protected workflows, official provider tooling such as pinned Wrangler, or explicitly owned outer adapters; never move them into `opsctl` to solve tooling inconvenience;
- Release / Capability Profile is the only production-enable authority; production remains fail-closed until its owning stage authorizes it.

## 5. Product and freeze discipline

PAS-1…PAS-7 in the canonical plan are the end-to-end product acceptance authority for the remaining phases. Validators and architecture artifacts cannot substitute for the assigned scenario evidence.

PF-3 is a provisional fitness baseline, not the final architecture-form freeze. FC-6…AR-15 may make only the smallest correction required by a named failed product scenario or rehearsal, preserving one natural owner and updating anti-weakening proof. Final architecture-form freeze follows accepted AR-15; AR-16 audits and AR-17 qualifies/authorizes.

## 6. Prohibited shortcuts

Do not:

- change architecture merely to satisfy CI;
- weaken, skip, unregister or cosmetically satisfy a protected check;
- create F3, PF-0, PF-4, a new roadmap, global authority bag, 1:1 successor registry, generic plugin/DI/linter framework or speculative compatibility layer;
- preserve historical internal shape without a proved current/durable obligation;
- use generated projections as semantic inputs;
- perform production mutation during N/PF/FC/AR work unless the canonical owning stage explicitly authorizes that exact effect;
- force-push, bypass protected merge or merge a head different from the proven exact head;
- trust this file or a handoff more than fresh accepted Git/GitHub state.

When blocked, report the exact missing authority/evidence/permission. Do not invent a new architecture layer or phase to route around it.

## 7. Completion and handoff

After an accepted merge, reread protected `main`, update only the existing live tracker(s) whose mutable state changed, and record the exact accepted head, CI result, review/thread state and next permitted bounded concern. A handoff must instruct the next agent to repeat the fresh re-baseline rather than trust the handoff SHA blindly.
