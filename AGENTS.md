# Repository Agent Execution Contract

This file is an execution guardrail, not a roadmap or semantic authority. It applies to the whole repository. More specific nested `AGENTS.md` files may narrow local rules but may not weaken this contract.

## 1. Fresh state is mandatory

At entry to a new bounded transaction/mutation window, and whenever section 1.1 says the current window is invalidated:

1. read current protected `main`, current branch, merge-base, ahead/behind and working-tree state when a local checkout is available;
2. refresh GitHub PRs, issues, checks, reviews, review threads and branch governance through the available authenticated GitHub tooling;
3. read `docs/INDEX.md`, `docs/PRODUCT.md`, fresh Issue #266,
   `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`,
   `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`, the one owning Issue selected by #266 and exactly
   the `CURRENT_CHECKPOINT` named by #266;
4. read the relevant current bounded contracts linked from `docs/INDEX.md`, such as
   `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`, `docs/OPSCTL_DOCTOR_CONTRACT.md` or
   `docs/PYTHON_USAGE_BOUNDARY.md`; read `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` only when the current
   checkpoint requires stage-order/lifecycle reasoning or a stage transition;
5. treat fresh Git/GitHub facts as higher authority than memory, chat history, handoff text, stale prose or an old SHA.

Completed research, acceptance, predecessor-stage and superseded execution trackers are history/provenance,
not live execution state. Issue #266 is the sole live stage pointer.

### 1.0 Project-wide cold-start context budget

This rule applies to **every CURRENT stage now and in the future**, not to one named stage or one chat.
After reading fresh Issue #266, use its exact `CURRENT_STAGE_ISSUE` and exact `CURRENT_CHECKPOINT` as the
bounded live-context entrypoint:

```text
fresh protected main + governance
-> permanent repository contracts
-> fresh #266
-> CURRENT stage Issue body for stable scope / DoD / non-goals
-> exactly CURRENT_CHECKPOINT named by #266 for mutable current work
-> only natural-owner contracts/files and PR/CI/evidence needed by that checkpoint
```

Do not fetch the full comment history of the CURRENT stage Issue, completed-stage Issue history, old PR
history, or broad Actions history by default. Open historical comments/runs/PRs only when the exact
`CURRENT_CHECKPOINT`, a DoD evidence locator, or a current natural owner points to them. A chat handoff
may accelerate navigation but never expands this read set or replaces the fresh checkpoint.

While an Issue is CURRENT, its body owns stable stage scope, objective, DoD, non-goals and immutable
entry provenance. It must not be maintained as a second mutable current-state snapshot. Moving current
SHA/tree, open PR/head, workflow run, Release Set, authorization and provider/readiness state belong to
#266's minimum live selection/checkpoint or to their existing GitHub/provider natural owners. A prior
checkpoint becomes append-only provenance as soon as #266 advances to a newer exact checkpoint.

The exact `CURRENT_CHECKPOINT` must be sufficient to identify one next permitted bounded concern,
its natural owner, current authorization state and the evidence locators needed to continue. If #266
omits the checkpoint, points to a missing/stale checkpoint, or conflicts with fresh higher-authority
facts, stop and repair/disposition the live pointer/current-stage record; **do not reconstruct current
work by recursively rereading history**.

Do not begin implementation until the current checkpoint and exactly one next permitted bounded concern are identified,
and the mandatory change envelope records an explicit capability/profile impact disposition or `NONE`.

For every CURRENT executable stage with its own entry/exit criteria and DoD, exactly one owning Issue
must be linked from Issue #266 before branch mutation starts. Issue #266 sits above the stages and is
the only mutable selector of which stage is CURRENT. Do not require a second "program" Issue plus a
child Issue for the same CURRENT stage, and do not treat a historical parent/precursor Issue as a
concurrent current-stage owner. Create a stage Issue only after a fresh re-baseline confirms that #266
selected that stage as CURRENT. Do not pre-create Issues for future stages. Follow the lifecycle in the
binding plan only when a stage-order/lifecycle boundary is actually being evaluated; the stage Issue is
bounded working memory/evidence, not a semantic owner or a second roadmap.

### 1.1 Bounded mutation windows — anti-stale-write without analysis loops

A fresh re-baseline **opens a bounded mutation window; it does not replace or recursively restart the
writes inside that window**.

Before the first write in a window, bind the window to exact fresh facts and declare its bounded
write-set/invariants. At minimum bind the accepted protected `main`, current #266 -> stage-Issue
pointer, relevant authorization state, open-PR/head state, and exact source/blob/head SHAs needed for
the intended writes.

Within that declared window:

- execute the smallest coherent set of related GitHub/source writes instead of returning to discovery
  after each one;
- use optimistic concurrency wherever available: exact expected blob/head SHA, returned commit SHA,
  returned Issue/PR/comment ID, or an equally strong exact predecessor;
- carry forward the IDs/SHAs returned by each successful self-authored write to the next dependent
  write;
- verify the resulting exact head, complete diff, write-set and required static/targeted invariants at
  the end of the bounded write-set;
- never describe a planned branch/commit/PR/change as durable until GitHub returns and a subsequent
  exact-state read proves that object/state exists.

Expected self-authored writes in the same window — for example a stage-Issue checkpoint, branch
creation, exact-SHA file commit, or PR metadata update — **do not by themselves invalidate the
baseline** and are not a reason to start a fresh full re-baseline.

The current mutation window is invalidated and a fresh re-baseline is mandatory before continuing
when any of the following occurs:

- protected `main`, the expected branch/head, or another exact predecessor changes unexpectedly;
- another actor changes relevant Issue/PR/review/authorization/owner/pointer state;
- the intended write-set, semantic owner, effect scope, architecture/security boundary or required
  authorization must expand;
- evidence is stale, contradictory, incomplete, or indicates unknown external drift;
- entering exact-head Ready/merge acceptance after implementation is complete;
- immediately before a guarded merge;
- after merge, before accepted-main fixation, stage completion disposition, #266 advancement or
  next-stage conclusions;
- before any separately governed provider/staging/Production mutation or recovery mutation.

A read or expected write made by this transaction is not an invalidation event merely because GitHub
now contains a newer object created by that same bounded window. Conversely, an unexpected difference
must never be waved through as “probably ours”: prove it from the exact returned SHA/ID or re-baseline.

In operational terms, `drift` means an exact fact relied on by the open window no longer equals the
expected fact and the difference is not mechanically attributable to a returned self-authored write.
Examples are an unexpected `main`/head SHA, a changed #266 pointer, a changed authorization, a new
relevant PR, or provider evidence whose identity/freshness no longer matches the prepared transaction.
Drift does not require a separate polling system: detect it at the normal exact-state reads and
optimistic-concurrency boundaries. If the predecessor no longer matches, stop and re-baseline.

### 1.2 Human interaction budget for authorized operations

After the user has granted the exact authorization required by the CURRENT stage, **machine-known
operation inputs must remain machine-owned**. The operator must not become a manual transport layer for
facts that are already available from protected `main`, #266/the CURRENT stage Issue, immutable
Release/transaction artifacts, fresh provider observation or another existing natural owner.

For an ordinary authorized operation:

```text
HUMAN_INTERACTION_BUDGET <= ONE_SIMPLE_UI_ACTION
```

If the available integration can dispatch the canonical operation directly, require no additional
human action. If it cannot dispatch but GitHub exposes the canonical manual Action, the maximum fallback
is to ask the user to open the already prepared Action and press `Run workflow`. For an ordinary GitHub
`workflow_dispatch` surface, **zero-input means the manual dispatch schema itself declares no user
inputs**: the GitHub UI must not expose unrelated advanced/evidence/recovery fields merely because the
ordinary path would ignore or default them. Do not ask the user to transcribe or assemble source/tree
SHAs, Release Set IDs, TransactionIds, `expected_current`, database IDs, digests, JSON payloads,
request IDs or derived confirmation strings when those values can be resolved mechanically.

Advanced/evidence/recovery parameters may exist only on a non-operator/internal surface of the same
natural owner — for example `workflow_call` — or another already-owned non-manual contract. Keep that
surface structurally isolated from the ordinary manual effect path and fail closed if invocation mode
is ambiguous. Preserve every existing authorization, freshness, target-fence, replay and fail-closed
guarantee by resolving and verifying machine-known facts inside the existing natural owner; do not
satisfy this rule by adding a second wrapper/controller, a mutable status database, a duplicate
mutation path or a weaker confirmation path.

## 2. One CURRENT stage at a time

Every CURRENT named executable stage selected by Issue #266 uses the same execution lifecycle. A
stage may require more than one coherent implementation transaction or PR while its objective and DoD
remain unchanged; those are implementation units inside the same stage Issue, not new CURRENT stages.
When work acquires an independent objective/DoD, authorization boundary, or acceptance boundary, it is
a new stage: finish/disposition the current stage, update #266, and only then create/select its Issue.

```text
STAGE ENTRY
-> fresh re-baseline
-> identify one bounded concern, natural owner, effects, contracts, callers and predecessor
-> declare bounded write-set + invariants + capability/profile impact disposition
-> open one bounded mutation window
-> implement the smallest coherent change
-> remove replaced predecessor/callers in the same transaction
-> inspect complete exact-head diff + simplification ledger
-> targeted/static proof
-> atomic/coherent commit set on the declared branch
-> exact-head applicable permanent CI
-> protected required contexts green
-> behind_by = 0
-> reviews/threads clear
-> FRESH PRE-MERGE RE-BASELINE
-> guarded merge bound to the proven exact head SHA
-> FRESH POST-MERGE protected-main reread
-> record accepted source/head/tree + CI/evidence + disposition in the CURRENT stage Issue
-> advance Issue #266 to exactly one next permitted stage or next bounded concern inside the same stage
```

Do not jump stages, continue from a superseded head, batch unrelated cleanup, or treat local/old CI as merge acceptance.
Do not turn the re-baseline checkpoints above into a loop between individual expected writes inside one bounded mutation window.

If a stage itself contains separable authorities/effects, use separate bounded mutation windows at
those real authority boundaries rather than one window per API call. In particular, source/PR/CI
acceptance never grants provider mutation authority. A rehearsal/deploy/recovery write is a separate
provider-effect window with its own exact fresh baseline/observation and explicit transaction-scoped
authorization before the first provider write.

That authorization must identify the exact immutable effect being permitted: at minimum the target,
source/transaction identity, effect scope, and any required expiry/freshness/fence constraints. It is
one-shot for that exact transaction/attempt unless the owning stage explicitly defines otherwise. Any
material change to target, source, prepared plan/transaction ID, provider pre-state, effect scope,
authority, or required freshness invalidates the authorization; re-prepare/re-observe and obtain a new
exact authorization before any provider write. A green PR or accepted source proves readiness of code,
not permission to mutate a provider.

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

### 3.1 Project simplicity invariant

**Reliability comes from fewer authoritative paths with stronger invariants, not from more layers.**
The project-wide default is to reuse or simplify an existing natural owner and shipping path. Adding
architecture is exceptional, not the default response to a new requirement, failed test or missing
proof.

A change may introduce a new semantic owner, runtime/service, persistent state store, execution path,
operator step, abstraction/framework/registry or checker only when the bounded change demonstrates all
of the following:

1. no existing natural owner can correctly own the current requirement;
2. the requirement is necessary for current correctness, security or the CURRENT stage DoD, not for
   anticipated future reuse or aesthetic symmetry;
3. extending the existing owner would violate an accepted hard architecture/security boundary;
4. the new element removes at least equivalent semantic/execution ambiguity, or its unavoidable net
   complexity is explicitly justified;
5. user/operator complexity does not increase unless the underlying product requirement inherently
   requires that interaction;
6. no simpler design preserves the same correctness and security guarantees.

If any item cannot be demonstrated, **do not add the new architectural element**. Prefer deletion,
composition through the existing owner, or a bounded test/evidence adapter that cannot become a second
shipping authority.

Every new or materially changed implementation PR/change envelope must include this disposition,
using `0` when there is no increase:

```text
COMPLEXITY_DISPOSITION
new semantic owner = 0
new runtime/service = 0
new persistent state = 0
new execution path = 0
new operator step = 0
new framework/registry/checker = 0
removed/replaced complexity = <what or NONE>
```

If any `new ...` value is non-zero, the same disposition must state:

```text
WHY EXISTING OWNER CANNOT HANDLE IT = <current concrete boundary>
WHY THIS IS CURRENTLY REQUIRED = <current correctness/security/DoD obligation>
SIMPLER ALTERNATIVE REJECTED BECAUSE = <specific failed invariant or hard boundary>
```

A vague claim such as "cleaner", "more flexible", "future-proof", "reusable", "easier to test" or
"may be needed later" is not sufficient justification for new project architecture.

### 3.2 Test complexity is not production authority

Test/evidence scaffolding may be more complex than the product path when that complexity is required to
prove real security or failure semantics. Its existence does not justify a production abstraction.

```text
test-only adapter/harness != reusable production abstraction
proof orchestration        != semantic owner
CI state                    != product state
```

A test-only adapter must remain bounded to evidence generation and must not become a second production
runtime, control plane, authorization path, persistence authority or mutation owner. Negative tests for
replay, expiry, concurrency, recovery or similar states should exercise the same shipping owner/state
machine rather than create one production mechanism per negative case.

### 3.3 Product complexity and verification complexity are accounted separately

Verification may be intentionally thorough while the shipping product remains simple. Additional
verification complexity must not, by itself, increase any of:

```text
user actions
operator inputs
shipping runtime services
shipping persistent stores
semantic owners
production execution paths
```

At a real stage/architecture cutover, perform one bounded simplification disposition: identify temporary
scaffolding, superseded predecessors and stage-only artifacts that can now be removed. Delete only what
has a proved absence of durable consumers; do not turn this into speculative refactoring or a new
simplification framework/checker.

The target shape remains:

```text
one fact
-> one natural owner
-> one shipping path
-> one standard operator procedure
```

## 4. Permanent architecture boundaries

- one semantic fact has one natural owner;
- one natural-owner verdict -> one lossless terminal OperationalOutcome for every new or touched critical operational procedure;
- **Capture before assert**: structured owner output must survive shell/YAML gate enforcement, and orchestration may name the phase/gate but must not rename or re-own an available semantic reason;
- infrastructure failure before an owner verdict must remain explicitly non-semantic and must never fabricate an owner reason; provider effect is exact when provable and otherwise UNKNOWN/fail-closed;
- Product Runtime never depends on `opsctl` or `opsctl-core`;
- `opsctl` is local/offline typed policy, planning, verification and projection tooling, never Product Runtime, daemon, browser launcher, provider client or mutation executor;
- raw JSON/provider responses/effect handles do not cross into pure semantic APIs;
- JSON is only a versioned external contract/manifest, observation/evidence, generated output projection or isolated historical input with a named consumer;
- Python may adapt, observe, generate and test, and may host the owned Camouhost cross-language boundary; it may not become a second product/release/lifecycle/evidence/fitness authority or ungoverned provider mutation path;
- GitHub/Cloudflare/provider effects remain in protected workflows, official provider tooling or explicitly owned outer adapters;
- `source_present != production_enabled`; source, UI, bindings or a compiled adapter never grant runtime admission;
- Release / Capability Profile is the sole production-enable authority;
- PF-3 enforcement truth is the actual specialized production checker + executable negative proof + CI caller, not a decorative metadata registry.

## 5. Current-stage and authorization discipline

The binding top-level order, stage meaning and gates are defined only by
`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` plus any subsequently accepted stage split/disposition
recorded by #266 and the CURRENT stage Issue. Do not infer or copy current position from this bootstrap
file. Read fresh Issue #266 and the one CURRENT stage Issue to discover the sole permitted work. A
historical parent/precursor Issue may remain linked as provenance but does not own the live pointer or
duplicate the CURRENT snapshot.

`CODE_COMPLETE != SCENARIO_COMPLETE != PRODUCTION_AUTHORIZED`. Source/CI work does not authorize
provider, staging or Production mutation unless its owning stage explicitly says so. Production requires
a separate explicit decision by the current named Production authority for one unchanged exact
candidate. Historical acceptance ceremonies and old readiness observations cannot substitute for that
decision.

A source/CI transaction that prepares a later provider rehearsal or deployment does not pre-authorize
that effect. Before the first provider write, require the exact authorization demanded by the CURRENT
stage Issue, revalidate the exact immutable transaction/effect target and freshness/fence conditions,
and open a new provider-effect mutation window. An authorization for one transaction/target/attempt
must never be inherited by a later stage or recovery action.

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

After an accepted merge, reread protected `main`; record exact accepted source/head/tree, CI evidence
and disposition in the CURRENT stage Issue. If the stage DoD is satisfied, update Issue #266 to mark
that stage accepted and select at most one next stage, then close the completed stage Issue as
provenance. If the stage remains CURRENT, #266 continues to point to the same Issue and only the next
bounded concern advances. A handoff must instruct the next agent to repeat the fresh re-baseline rather
than trust a copied SHA blindly.
