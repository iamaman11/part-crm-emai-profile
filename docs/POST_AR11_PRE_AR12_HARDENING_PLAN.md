# Post-AR-11 / Pre-AR-12 Hardening Execution Plan

**Document status:** SUBORDINATE_EXECUTION_PLAN  
**Tracking issue:** #375  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Lifecycle authority:** `architecture/architecture-acceptance-policy.json` + `architecture/architecture-program-sequence.json` + `architecture/lifecycle-projection-policy.json` + `.github/scripts/architecture-acceptance.mjs` + immutable acceptance Git metadata  
**Scope:** post-AR-11 cleanup and architectural hardening only  
**AR-12 implementation:** FORBIDDEN until this plan's Definition of Done is satisfied and #375 is closed from proved evidence  
**Production mutation:** FORBIDDEN

This document is the canonical execution checklist for issue #375. It is a subordinate plan and handoff target, not a second lifecycle authority. It does not record acceptance and must never be used to decide the accepted/current AR slice independently of the generic Git-derived acceptance mechanism.

If this plan, a handoff note, a stale SHA, a workflow count, a progress note or a tracked projection disagrees with live protected `main` and canonical architecture authorities, use this precedence:

```text
live protected main
+ canonical architecture authorities
+ current GitHub evidence
+ this execution plan
+ historical handoff/progress snapshots
```

## 1. Objective and non-negotiable invariants

The application remains one modular product with one protected `main`, one architecture, one application and one data/schema history. Capabilities may be implemented continuously and enabled independently:

```text
source_present != production_enabled
```

Post-AR-11 cleanup is not application-functionality cleanup. Do not delete source from `apps/**`, `crates/**`, `frontend/**`, `runtime/**`, `migrations/**` or `contracts/**` merely because a capability is currently production-disabled, belongs to mailbox functionality, was implemented before the current activation model, or is not part of Production Core v1.

Application functionality and historical operational machinery are different categories. `unused by current production profile` does not imply `dead`.

Until the owning future architecture stage explicitly changes them, preserve mechanically:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

AR-12 implementation must not begin during this work. No production provisioning, production promotion, production secret mutation, production provider mutation, production customer-state mutation, capability enablement or GitHub Environment approval bypass is permitted.

AR-17 is a future architectural authorization stage. Even accepted AR-17 does not itself provision or enable production. The first Production Core provisioning/enablement remains a separate future stage, PC-1.

## 2. Mandatory live-state reread before every bounded unit

Before modifying source or accepting any bounded unit, re-read live GitHub state rather than trusting a snapshot. At minimum inspect:

- protected `main` exact SHA;
- this plan;
- issue #375 and latest evidence comments;
- open PRs and relevant unmerged branches;
- branch protection and required contexts;
- `architecture/github-actions-registry.json` and live GitHub Actions state where applicable;
- `architecture/architecture-acceptance-policy.json`;
- `architecture/architecture-program-sequence.json`;
- `architecture/lifecycle-projection-policy.json`;
- `.github/scripts/architecture-acceptance.mjs` behavior and self-test;
- the exact current call sites for the bounded concern being changed.

Do not begin by writing new code from a stale handoff. A stale unmerged branch is evidence/archaeology only until rebased or reimplemented from current accepted `main`; it has no execution or lifecycle authority.

### Last synchronized execution snapshot — informational only

At this plan synchronization on 2026-08-20:

- protected `main`: `b20d3d23e022933e51ffd827624f1b02685c9d9e` (#392);
- accepted merge tree: `3c6a01b82f8c58e32405c7206374b30de2950511`;
- AR-11: mechanically accepted through the generic Git-derived mechanism;
- Git-derived current slice: AR-12;
- AR-12 implementation: NOT STARTED;
- tracking issue #375: OPEN;
- latest accepted hardening unit: **A3a / #392**;
- #392 exact green candidate: `d045a48f354ea1292374689e00a784a2b4a3e4d5`;
- #392 candidate tree == merge tree: `3c6a01b82f8c58e32405c7206374b30de2950511`;
- #392 observed applicable permanent PR workflows: 17/17 SUCCESS;
- #392 observed protected required contexts: 23/23 SUCCESS;
- #392 pre-merge behind/reviews/threads: 0/0/0;
- `architecture/inventory.json`, `docs/status.json` and `architecture/architecture-rebaseline-v3-transition.json` still intentionally project the pre-A3b AR-10/AR-11 state and are migration debt, not lifecycle authority;
- stale `agent/post-ar11-*` branches discovered during the live audit are not continuation bases and must not be merged merely because their names match remaining work;
- the post-merge evidence visibility gap for required `push: main` / accepted-main-only workflows remains unresolved for final #375 certification; unobservable required evidence remains `UNPROVEN`, never implicit `SUCCESS`.

Observed workflow/context counts are evidence for this snapshot only, not timeless constants. Every future bounded unit must rediscover applicable workflows and required contexts from live registry/protection.

## 3. Historical executable safety model

Every suspicious historical executable must be classified before removal:

```text
CURRENT_INVARIANT
UPGRADE_ROLLBACK_REQUIRED
TRANSITION_PROVENANCE_ONLY
DEAD
```

Only `DEAD` is removable.

Historical naming is not classification evidence. Names containing `AR8`, `AR10`, `pre2j`, `phase2i`, `legacy` or `old` can still protect permanent current invariants.

For any candidate retirement use this safety sequence:

```text
map invariant
-> identify current required semantics
-> port invariant to neutral/current authority only if needed
-> add fail-closed negative tests
-> prove parity
-> switch current callers
-> prove zero current callers / zero unique invariants
-> classify predecessor DEAD
-> update debt registry / Python estate
-> remove predecessor
```

Never delete first and use CI breakage as the discovery mechanism.

Never restore/materialize retired executable code from Git history merely to execute it. Static Git archaeology (`git show`, `git cat-file`, `git merge-base`, `git rev-parse`) is evidence-only and permitted:

```text
history = evidence
history != executable authority
```

Continue to enforce `architecture/historical-executable-debt.json` and `scripts/check-historical-executable-debt.py`. Unknown Python executables must fail closed under the canonical Python estate.

## 4. Critical reclassification after accepted A3a

The old A-F structure remains useful only as an audit map. It is **not** a requirement to create one PR per Unit or one PR per checklist bullet.

The 2026-08-20 live audit reclassifies the remaining work as follows:

| Concern | Status | Required disposition before AR-12 |
| --- | --- | --- |
| A1 projection policy | ALREADY_DONE | Accepted in #381; do not redesign without a defect. |
| A2 prerequisite extraction | ALREADY_DONE | Accepted through #385–#390. |
| A3a read-side lifecycle cutover | ALREADY_DONE | Accepted in #392. |
| A3b lifecycle projection synchronization | REQUIRED / NEXT | Remove stale lifecycle monkey-patching and generate supported projections from canonical derived state. |
| A4 compatibility neutralization | REPHRASE / CONDITIONAL | After A3b, run a semantic caller/invariant scan. Create a separate unit only for residual independently provable debt. No mandatory A4 PR. |
| Unit B `opsctl` authority/command registry | REQUIRED | `architecture/operator-contract.json` does not currently describe the full actual namespaced Rust CLI surface. Close authority↔implementation parity without adding a second registry. |
| Unit C `opsctl` modularity | PARTIALLY_DONE / MERGE_WITH_B_WHERE_COHERENT | Rust already has subsystem modules and `lib.rs` is a composition root. Refactor only parser/presentation coupling that is needed for B or demonstrably harms maintainability. No micro-module quota. |
| Unit D inventory modularization | PARTIALLY_DONE / REPHRASE | Core/wrapper/engine separation already exists. A3b removes lifecycle ownership debt. Afterward extract only clearly mixed domain responsibilities that materially reduce coupling while preserving deterministic bytes. No giant rewrite requirement. |
| Unit E historical executable audit | PARTIALLY_DONE / REPHRASE | `architecture/historical-executable-debt.json` already classifies many current/dead/provenance paths. Complete residual caller/invariant gaps and remove only newly proved `DEAD`; no deletion quota. |
| Unit F final audit / #375 closeout | REQUIRED | Re-prove final DoD on live `main`, including post-merge evidence observability. |
| Post-merge evidence observability | REQUIRED FOR FINAL CLOSEOUT | Required main-only evidence must have a direct, reproducible observation path. Agent/tool visibility limitations may not be converted into success. |

### 4.1 Granularity rule

A merge boundary is an independently provable architectural concern, not a line/file/checklist boundary:

```text
semantic cohesion
+ one proof obligation
+ safe intermediate main state
+ independent rollback value
= one bounded PR / one merge
```

Therefore:

- multiple tightly related edits, tests, generated projections and CI fixes required for one invariant stay in one PR;
- a defect caused by the current bounded unit should normally be fixed in that same PR; changing the head invalidates all previous exact-head evidence;
- independent authorities or independently reviewable/rollbackable architecture concerns must not be bundled merely to save CI time;
- `ALREADY_DONE`, `OBSOLETE`, `NOT_WORTH_THE_COMPLEXITY` and evidence-proven unnecessary checklist items create no PR;
- PR size is determined by semantic cohesion and proof boundary, not line/file count.

## 5. Accepted execution progress — do not repeat without new defect evidence

The following post-AR-11 work is accepted prerequisite history:

- **#381 / A1 — ACCEPTED:** introduced and bound `architecture/lifecycle-projection-policy.json`, strengthened documentation authority, classified tracked lifecycle artifacts as projections and established this subordinate execution plan.
- **#382 — ACCEPTED prerequisite:** fixed deterministic Release Set verification and moved the affected download action to an immutable Node-24-capable pin.
- **#383/#384 — ACCEPTED prerequisite:** one-shot allowlisted cleanup of stale hosted Actions registrations, live read-only registry proof, then removal of temporary mutation authority; permanent governance returned to read-only.
- **#385 / A2a — ACCEPTED:** removed obsolete pre-2J/#203 lifecycle ownership from generic inventory-core documentation validation without changing generated lifecycle projection.
- **#386 / A2b — ACCEPTED:** extracted neutral credential/security invariant authority and parity/negative tests.
- **#387 / A2c — ACCEPTED:** cut current inventory credential source/validation caller over to the neutral credential owner.
- **#388 / A2d — ACCEPTED:** decoupled neutral credential validation from the historical inventory engine and closed AR-8D successor/security parity gaps.
- **#389 / A2e — ACCEPTED:** cut remaining current credential negative-fixture and summary hooks over to neutral wrappers.
- **#390 / A2f — ACCEPTED:** removed duplicate credential implementation from the historical engine and retained only thin compatibility delegation. The engine itself remains `KEEP_PYTHON` / `CURRENT_INVARIANT`.
- **#391 — ACCEPTED plan synchronization:** synchronized this plan after A2f; merge `df41607c6b8483a66cbd0f043acf37113c92461e`.
- **#392 / A3a — ACCEPTED:** cut current lifecycle reads over to canonical Git authority through `.github/scripts/architecture-acceptance.mjs derive`; Python validates returned shape but does not parse acceptance tags; jobs that actually derive lifecycle state received sufficient Git metadata (`fetch-depth: 0`) without globally deepening all workflows. Projection bytes were intentionally left unchanged for A3b. Accepted merge `b20d3d23e022933e51ffd827624f1b02685c9d9e`.

Do not restore removed one-shot workflow mutation machinery, historical credential implementation or old lifecycle closeout code without new concrete defect evidence.

## 6. Unit A — Lifecycle projection authority finalization

Goal: make Git-derived lifecycle semantics singular and unambiguous and ensure tracked projections reflect, but never own, accepted/current state.

### A1 — Projection policy and authority binding — ACCEPTED

Completed by #381.

### A2 — Historical lifecycle assertion and credential/security prerequisite extraction — ACCEPTED

Completed through #385–#390.

### A3a — Lifecycle read-side authority cutover — ACCEPTED

Completed by #392.

Accepted invariant:

```text
Git acceptance metadata
+ architecture-acceptance-policy.json
+ architecture-program-sequence.json
-> architecture-acceptance.mjs derive
-> validated consumers
```

Current Python consumers may validate shape/static successor relationships but must not independently enumerate, parse or interpret acceptance tags. Tracked status/inventory/transition fields cannot decide accepted/current lifecycle state.

### A3b — Lifecycle projection synchronization — REQUIRED / NEXT BOUNDED UNIT

The live audit proves this is real debt, not a checklist artifact.

Current `scripts/generate-architecture-inventory.py` still contains compatibility monkey-patching equivalent to:

```text
CURRENT_SLICE = AR-11
NEXT_SLICE = AR-12
CURRENT_DELIVERY_CHECKPOINT = AR-10
AR-specific accepted-slice append
stale current_delivery_map overlay
```

Current tracked projections correspondingly still report AR-10 accepted / AR-11 current in places such as `architecture/inventory.json`, `docs/status.json` and `architecture/architecture-rebaseline-v3-transition.json`.

A3b proof boundary is one statement:

```text
all supported current lifecycle projections are deterministically built from
canonical Git-derived lifecycle state and cannot become lifecycle authority
```

Required implementation:

1. Remove current-lifecycle ownership constants, AR-specific append logic and lifecycle monkey-patching from the current inventory path wherever canonical derived state can directly supply the value.
2. Do not move acceptance-tag parsing into Python/Rust. Consume only validated canonical derivation output.
3. Project the accepted/current values as:

```text
accepted checkpoint = AR-11
current architecture slice = AR-12
AR-12 implementation = NOT STARTED
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

4. Synchronize supported tracked projections, including at minimum:
   - `architecture/inventory.json`;
   - `docs/status.json`;
   - `architecture/architecture-rebaseline-v3-transition.json`;
   - human-readable current-status documentation only where current semantic state is projected.
5. Make projection/non-authority role explicit in generated fields or owning policy where it is not already mechanically clear.
6. Preserve stable historical acceptance/evidence records; do not rewrite historical provenance merely to make old documents look current.
7. Preserve deterministic/idempotent generation and all existing domain-owned inventory data.
8. Add/retain negative proof that changing tracked projection values cannot change canonical derived lifecycle state.
9. Keep `generate-architecture-inventory-engine.py` as `CURRENT_INVARIANT` unless a separate later proof establishes otherwise.

A3b must not:

- implement AR-12 functionality;
- create a second lifecycle derivation algorithm;
- treat `docs/status.json`, inventory or transition JSON as authority;
- delete historical executable functionality merely because names are old;
- broaden `fetch-depth: 0` to unrelated jobs;
- mutate production/provider state.

Required proof:

- canonical derive reports AR-11 accepted / AR-12 current;
- generated projection reports the same values;
- repeated generation is byte-stable/idempotent;
- tracked projection mutation cannot create/revoke acceptance;
- malformed canonical output fails closed;
- no Python/Rust acceptance-tag parser exists;
- all change-applicable repository/Python/Runtime/Camoufox/Windows gates are green on one exact candidate;
- candidate tree identity is preserved through guarded squash acceptance.

### A4 — Residual lifecycle compatibility audit — CONDITIONAL

After A3b, run semantic scans/caller mapping for remaining AR-8/AR-10/AR-11 lifecycle compatibility globals/executables.

Possible outcomes for each finding:

```text
ALREADY_DONE
CURRENT_INVARIANT
TRANSITION_PROVENANCE_ONLY
DEAD
REQUIRED_BOUNDED_FIX
NOT_WORTH_THE_COMPLEXITY
```

Do not create an A4 PR merely because the plan contains an A4 heading. A separate A4 unit exists only if the audit finds a distinct independently provable lifecycle compatibility concern. Naming cleanup alone is not enough justification.

Unit A closes when:

- exactly one acceptance derivation implementation exists;
- missing/conflicting/non-contiguous acceptance evidence fails closed;
- no tracked projection can decide accepted/current state;
- no inventory generator can advance lifecycle state;
- no second source commit is required to record future acceptance;
- documentation checker rejects projection attempts to become authority;
- production fail-closed invariants remain unchanged.

## 7. Unit B — Canonical `opsctl` role and exact command registry — REQUIRED

Goal: make `opsctl` the repository-local Operational Policy & Decision Control Plane with exact authority↔implementation parity.

Target role:

```text
inspect -> classify -> plan -> preflight -> verify -> explain
```

Never:

```text
provision -> deploy -> provider mutate -> secret readback -> database mutate -> customer-state mutate
```

Provider execution boundary remains:

```text
opsctl
policy / compatibility / plan / verify
    -> GitHub Actions orchestration
    -> GitHub Environment approval + scoped credentials
    -> Wrangler / dedicated provider executor actual mutation
    -> snapshot / evidence
    -> opsctl verify
```

Live audit finding: `architecture/operator-contract.json` currently registers only a small/older operator surface, while the Rust CLI already exposes namespaced families including:

```text
doctor
status
inventory
credentials status
credentials rotation-plan
d1 status
d1 plan
d1 compatibility
d1 verify
release inspect
release verify
release compatibility
promotion plan
promotion preflight
promotion verify
```

Therefore Unit B is not optional cleanup: authority↔implementation parity is currently incomplete.

Required implementation:

1. Evolve `architecture/operator-contract.json`; do not create a competing command registry unless a separately versioned implementation contract is objectively required and can be proven not to duplicate authority.
2. Register every active command family with namespace/action, mode, authority/source, inputs, output semantics, side effects, network authority, provider mutation authority, secret authority, activation owner and ACTIVE/RESERVED status.
3. Bind the registry to the actual Rust CLI surface with a permanent fail-closed parity gate in both directions.
4. Keep recovery RESERVED for AR-14 and readiness RESERVED for AR-16 unless the program authority says otherwise; reserved namespaces must not accidentally become executable current commands.
5. Retire obsolete flat current-authority spellings only where real callers are absent; historical names may remain as provenance.
6. Preserve current command behavior unless a concrete defect requires a separate change.

Permanent safety assertions:

```text
provider_mutation = false
network_authority = false
secret_readback = false
production_mutation = false
customer_state_mutation = false
database_mutation = false
deployment_mutation = false
production_child_process_spawn_sites = 0
```

Explicitly reject production `Command::new("wrangler")`, `Command::new("node")`, `Command::new("npx")`, provider HTTP clients, GitHub mutation clients, D1 mutation clients and secret retrieval within `opsctl` authority.

Required checks:

- authority -> implementation exact-surface parity;
- implementation -> authority exact-surface parity;
- negative tests for unknown/unowned commands and premature reserved namespaces;
- source scan for child-process/network/provider mutation authority;
- existing command semantics remain behaviorally unchanged unless separately justified.

## 8. Unit C — `opsctl` internal modularity — PARTIALLY_DONE / CONDITIONAL

The live Rust structure already contains subsystem modules for credentials, D1, release, promotion, repository access, recovery/readiness reservations, status/inventory and a small composition `lib.rs`. Therefore a broad modularity rewrite is **not** a pre-AR12 requirement by default.

Current concern worth reviewing: `tools/opsctl/src/cli.rs` remains a large centralized parser. That alone is not proof of bad architecture.

During Unit B:

1. Keep `lib.rs` as composition/dispatch root, not a policy implementation warehouse.
2. If exact command-registry parity is substantially simpler and clearer by extracting subsystem parser definitions next to owning modules, perform only those cohesive extractions in the same B proof boundary.
3. If Unit B closes cleanly without parser extraction and the CLI remains developer-comprehensible, classify the remaining C refactor `NOT_WORTH_THE_COMPLEXITY` for pre-AR12.
4. Create a separate Unit C PR only if a post-B audit proves a distinct coupling/maintainability defect with its own tests and rollback value.
5. Do not split the parser into micro-files solely to reduce line count.
6. Keep runtime/application crates independent from `tools/opsctl`.

A future stable machine-readable output envelope may be desirable, but it is not a mandatory pre-AR12 rewrite unless current commands already have contradictory/unusable machine semantics that block the next architecture slices.

## 9. Unit D — Inventory architecture — PARTIALLY_DONE / REPHRASED

Goal: retain `architecture/inventory.json` as deterministic compatibility/operator projection assembled from explicit owners, without making inventory lifecycle authority.

Current structure already separates significant concerns across:

```text
scripts/_architecture_inventory_core.py
scripts/generate-architecture-inventory.py
scripts/generate-architecture-inventory-engine.py
subject-domain validators/builders
```

Therefore pre-AR12 does not require a wholesale collectors/validators/serializer rewrite.

Required sequence:

1. Let A3b remove the known lifecycle monkey-patch/overlay debt first.
2. Re-audit remaining collection / validation / aggregation / serialization mixing after A3b, not before.
3. Extract only responsibilities that have a clear domain owner and whose extraction materially reduces coupling or duplicate authority.
4. Do not create parallel registries solely to make the generator look modular.
5. Preserve stable architecture information explicitly; do not pass giant unknown chunks through accidentally.
6. Preserve deterministic byte-for-byte/idempotent generation.
7. Use strict validation/typed models at trust boundaries where it adds real protection; do not mechanically add JSON Schema/protobuf to reviewed repository metadata.
8. A large-file reduction target is not a DoD metric. Developer comprehensibility and ownership clarity are.

Unit D may resolve as `ALREADY_SUFFICIENT_AFTER_A3B` if the post-A3b audit finds no material mixed-responsibility defect.

## 10. Unit E — Historical executable classification and evidence-proven retirement — PARTIALLY_DONE / REPHRASED

`architecture/historical-executable-debt.json` is already the current debt inventory and contains both retained current invariants and proved dead/absent paths. Unit E is therefore a residual audit, not a new classification project.

Important current classifications include:

- `scripts/generate-architecture-inventory.py` — `CURRENT_INVARIANT`;
- `scripts/generate-architecture-inventory-engine.py` — `CURRENT_INVARIANT`;
- `scripts/generate-ar8-completion-status.py` — `CURRENT_INVARIANT` read-only compatibility validator;
- `scripts/check-documentation-authority.py` — `CURRENT_INVARIANT`;
- `scripts/check-documentation-authority-legacy.py` — `TRANSITION_PROVENANCE_ONLY`, zero current executable callers required;
- several D3/pre2j checks remain `CURRENT_INVARIANT` despite historical names;
- multiple retired AR-8 overlays/workflows are already `DEAD` and required absent.

Required residual work:

1. Re-run caller graph/invariant mapping after Unit A and any opsctl/inventory changes.
2. Validate every tracked debt entry against current call sites and Python estate.
3. Add newly discovered suspicious executables fail-closed rather than silently ignoring them.
4. Preserve `CURRENT_INVARIANT` and `UPGRADE_ROLLBACK_REQUIRED` functionality.
5. Prove `TRANSITION_PROVENANCE_ONLY` paths have no accidental current execution authority.
6. Remove only newly proved `DEAD` items; zero removals is a valid result if nothing else is proved dead.
7. Do not restore/materialize retired executables from Git history for tests.
8. Prefer renaming historical current-invariant files only when it materially improves ownership clarity and parity/callers are mechanically controlled; naming aesthetics alone are insufficient.

## 11. Unit F — Final pre-AR12 audit and closure of #375 — REQUIRED

Goal: prove the repository can begin AR-12 without historical lifecycle ownership and without weakening production/runtime gates.

Audit dimensions:

- lifecycle authority singularity;
- projection freshness and non-authority;
- production fail-closed state;
- preservation of independently activatable application functionality;
- historical executable debt;
- inventory ownership/determinism;
- `opsctl` role, registry, modularity and safety;
- developer-facing architecture clarity;
- Python estate completeness;
- GitHub Actions supply-chain hygiene;
- permanent PR CI, protected contexts and post-merge evidence.

Repository-wide semantic scan must include `.github/scripts/**`, `.github/workflows/**`, `scripts/**`, `tools/**` and `architecture/**` and must analyze callers/authority, not only filenames.

Do not close #375 until every final DoD item is mechanically proven.

## 12. PR proof vs accepted-main proof boundary

Preserve this architecture principle:

```text
PR proves everything provable without mutation.
main performs only accepted-main identity / durable external-state operations.
```

If a check can run without production mutation, accepted-main identity or durable external publication, it belongs on the exact PR candidate before merge. This includes, where applicable:

- build/assemble/parse;
- contract validation;
- compatibility verification;
- manifest/Release Set verification;
- deterministic negative tests;
- static policy/security checks.

Accepted `main` should do only work that truly requires accepted-main identity, live hosted state, durable publication or explicitly approved external mutation.

Never claim:

```text
all PR workflows green == all post-merge workflows green
```

They are separate proofs.

After every merge separately verify every change-relevant:

- `push: main` workflow;
- accepted-main-only workflow;
- hosted governance/live-state verification;
- durable publication/build workflow;
- other main-only operation.

If the available evidence surface cannot observe a required post-merge workflow, classify it `UNPROVEN`, never implicitly `SUCCESS`.

Final #375 closure requires a direct, reproducible observation path for every required post-merge-only check. This path should be repository/tooling independent of any particular agent UI where practical (for example a documented GitHub API/`gh` evidence command or repository-owned verifier). Do not add a new writer/authority merely to make evidence convenient.

A deterministic contract defect first discovered only after merge is an architecture/test-coverage defect and must be fixed with a bounded follow-up; it is not the desired normal model.

## 13. PR, exact-head and semantic-boundary discipline

Every independently provable bounded unit follows:

```text
exact accepted main
-> new bounded branch
-> implement one complete coherent concern
-> local/deterministic proof
-> Draft PR
-> iterate inside that PR until the concern is complete
-> freeze exact candidate SHA
-> all applicable permanent workflows
-> all required protected contexts
-> re-read main/reviews/threads/protection
-> Ready
-> guarded squash bound to expected exact head
-> accepted-main reread
-> candidate tree == merge tree
-> relevant post-merge proof
```

Do **not** interpret this as one line/file/edit = one PR.

Several edits required for the same invariant belong together. Examples include implementation + tests + generated projection update + a directly caused CI fix. Conversely, do not bundle lifecycle authority, opsctl authority, unrelated inventory refactors and historical retirement merely to reduce CI runs.

After final CI begins on a chosen exact candidate, any source change invalidates old exact-head evidence. Re-run the complete applicable evidence set on the new exact SHA.

Before merge prove:

```text
candidate SHA = exact reviewed source
behind_by = 0
blocking_reviews = 0
unresolved_review_threads = 0
all applicable permanent workflows = SUCCESS
all protected required contexts = SUCCESS
```

If `main` moves, stop and recalculate evidence. Do not merge from stale state.

The live registry and branch protection are authoritative; never hard-code historical counts such as 17 workflows or 23 contexts as timeless constants.

Every relevant pre-AR-12 exact candidate must preserve Real Camoufox cold-launch proof and Windows Profile Bridge regression. A red Camoufox workflow must first be classified by exact failing layer: repository policy, Python estate, runner/Xvfb, dependency installation, real Camoufox launch, identity, persistence or Bridge IPC. Do not change runtime code for a failure that occurred before browser launch.

Camoufox production architecture remains:

```text
native Rust Profile Bridge
-> typed/versioned IPC
-> Camouhost
-> pinned Camoufox / BrowserForge / Playwright
-> visible browser
```

Do not replace the real runtime production path with a fake fixture.

## 14. GitHub Actions supply-chain hygiene

Whenever `.github/workflows/**` or action dependencies change:

- use official/approved actions;
- pin actions to immutable full commit SHAs;
- require a supported runtime such as current Node support required by GitHub-hosted runners;
- do not introduce floating mutable tags;
- keep canonical governance policy green;
- treat deprecation warnings as bounded maintenance evidence, not automatic proof of functional failure;
- do not mass-update unrelated Actions "while here".

Temporary mutation/evidence workflows must have an explicit bounded lifetime and must be removed after their purpose is proven. Permanent governance should remain read-only unless an owning architecture explicitly grants a narrowly scoped mutation boundary.

Deep Git history is capability-specific. Use `fetch-depth: 0` only in jobs whose actual canonical derivation/static-history proof requires it; do not spread full history to every job by convention.

## 15. Merge and architecture-acceptance boundaries

Use squash merge with expected exact head SHA for bounded cleanup PRs unless live governance requires a more specific accepted path. After merge, prove candidate tree == accepted merge tree and reread protected `main`.

AR-12+ architecture acceptance remains one guarded source merge plus immutable acceptance Git metadata over the same history. Do not reintroduce:

- per-AR closeout transformer scripts;
- AR-specific closeout PRs;
- self-writing workflows;
- second source PRs solely to record acceptance;
- force-moving/deleting acceptance tags;
- manual `current_slice` / `accepted_checkpoint` state as lifecycle authority;
- second independent lifecycle derivation algorithm.

The phrase `one-merge architecture acceptance` describes acceptance of an architecture slice without a second source closeout merge. It does **not** mean every tiny hardening edit requires its own merge and does **not** mean all independent pre-AR12 concerns belong in one giant PR.

## 16. Definition of Done before AR-12 implementation

The following checklist is intentionally re-proved in the final Unit F audit even where accepted historical evidence already exists.

### Lifecycle

- [ ] AR-11 is mechanically accepted by the canonical Git-derived mechanism.
- [ ] AR-12 is mechanically derived as current.
- [ ] Exactly one acceptance derivation implementation exists.
- [ ] Python/Rust/current consumers do not independently enumerate or interpret acceptance tags.
- [ ] No manual current-slice/accepted-checkpoint authority exists.
- [ ] Mutating stale `docs/status.json`, inventory or transition lifecycle fields cannot change derived lifecycle state.
- [ ] Malformed/conflicting/non-contiguous acceptance evidence fails closed.
- [ ] No per-AR closeout machinery or self-writing CI is required.
- [ ] No second acceptance source merge is required.
- [ ] Acceptance-tag protocol remains intact.
- [ ] Human/current projections show AR-11 accepted / AR-12 current / AR-12 NOT STARTED and are explicitly non-authoritative.

### Production and capabilities

- [ ] `architecture_complete=false`.
- [ ] `production_core_gate=BLOCKED`.
- [ ] `production_ready=false`.
- [ ] `production_mutation=false`.
- [ ] No capability was production-enabled accidentally.
- [ ] No application functionality was deleted merely because it is disabled.
- [ ] `source_present != production_enabled` remains mechanically enforced.

### Historical executable debt

- [ ] Every suspicious executable is classified or covered by a mechanically complete current inventory.
- [ ] Every currently classified `DEAD` path is absent as required.
- [ ] Newly proved `DEAD` items, if any, are removed.
- [ ] Current invariants are retained or renamed only with parity proof.
- [ ] Upgrade/rollback-required tools are retained.
- [ ] Transition-provenance executables have no accidental current execution authority.
- [ ] No retired executable materialization/execution path remains.
- [ ] No file is removed merely because its name is historical.
- [ ] `generate-architecture-inventory-engine.py` is retained unless live evidence proves it `DEAD`.

### Inventory

- [ ] Inventory has no live lifecycle ownership.
- [ ] No AR-8/AR-10/AR-11 monkey-patch lifecycle model controls current state.
- [ ] Current inventory generation consumes canonical lifecycle derivation rather than tracked lifecycle snapshots.
- [ ] Aggregation is deterministic and domain ownership is clear enough for a new developer to follow.
- [ ] Repeated generation is byte-stable/idempotent.
- [ ] Stable information has explicit ownership.
- [ ] Historical snapshots are explicitly non-authoritative.
- [ ] Any remaining large modules are justified by cohesion rather than retained accidentally or split merely by line-count targets.

### `opsctl`

- [ ] Canonical role is explicit.
- [ ] Actual active CLI equals registered active CLI in both directions.
- [ ] Obsolete flat current-authority command names have no active authority/callers.
- [ ] D1, Release and Promotion command families are registered exactly.
- [ ] Recovery is RESERVED for AR-14 unless later authority changes it.
- [ ] Readiness is RESERVED for AR-16 unless later authority changes it.
- [ ] Provider mutation = false.
- [ ] Network authority = false.
- [ ] Secret readback = false.
- [ ] Customer/database/deployment mutation = false.
- [ ] Production child-process execution authority = 0.
- [ ] Repository IO, policy/domain semantics, provider execution and presentation ownership are understandable.
- [ ] No unnecessary parser/module rewrite was introduced.

### CI, merge and post-merge proof

- [ ] All applicable permanent PR workflows are green on one exact final candidate head.
- [ ] All protected required contexts are green on that same exact head.
- [ ] Real Camoufox cold-launch proof is green.
- [ ] Windows Profile Bridge regression is green.
- [ ] `behind_by=0`, blocking reviews=0, unresolved threads=0.
- [ ] Guarded squash is bound to the exact candidate head.
- [ ] Accepted `main` is re-read after merge.
- [ ] Candidate tree equals accepted merge tree.
- [ ] Every relevant `push: main` / accepted-main-only / hosted-state / durable-publication workflow is directly verified after merge.
- [ ] No required post-merge evidence is silently treated green when it is unobservable.
- [ ] A reproducible evidence-observation mechanism exists for every required final main-only proof.
- [ ] No deterministic contract defect is first discovered only after merge on the final certification path.
- [ ] GitHub Actions dependencies touched by hardening remain immutable-pinned and runtime-supported.

### Developer experience and architecture clarity

- [ ] A new developer can locate application business logic, domain code, runtime adapters, operator policy, provider execution, GitHub orchestration, architecture authority and historical evidence without reading multiple obsolete AR implementations.
- [ ] One concern has one clear owner; projections may be many, competing authorities may not.
- [ ] No unnecessary full rewrite, registry proliferation or micro-module proliferation was introduced.
- [ ] PR/merge history follows semantic proof boundaries rather than checklist or file-count boundaries.

### Tracking

- [ ] #375 remains OPEN until every item above is proven.
- [ ] #375 is closed only after final Unit F accepted-main and post-merge evidence is complete.

Only after this checklist is complete may the project begin AR-12 implementation. At that point the intended platform is:

```text
lifecycle = Git-derived
inventory = deterministic/domain-owned projection
opsctl = operational policy & decision plane
GitHub = orchestration/approval
Wrangler/provider executors = mutation boundary
application capabilities = independently activatable
production = still BLOCKED
```

The final engineering rules for all remaining work are:

```text
make ownership explicit
preserve real invariants
prove parity mechanically
remove only proven debt
prefer the smallest complete proof boundary, not the smallest possible diff
avoid complexity that has no independent safety/clarity payoff
```
