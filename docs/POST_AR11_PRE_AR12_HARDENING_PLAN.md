# Post-AR-11 / Pre-AR-12 Hardening Execution Plan

**Document status:** SUBORDINATE_EXECUTION_PLAN  
**Tracking issue:** #375  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Lifecycle authority:** `architecture/architecture-acceptance-policy.json` + `architecture/architecture-program-sequence.json` + `architecture/lifecycle-projection-policy.json` + `.github/scripts/architecture-acceptance.mjs` + immutable acceptance Git metadata  
**Scope:** post-AR-11 cleanup and architectural hardening only  
**AR-12 implementation:** FORBIDDEN until this plan's Definition of Done is satisfied  
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
- `docs/POST_AR11_PRE_AR12_HARDENING_PLAN.md`;
- issue #375 and latest evidence comments;
- open PRs and relevant unmerged branches;
- branch protection and required contexts;
- `architecture/github-actions-registry.json` and live GitHub Actions state where applicable;
- `architecture/architecture-acceptance-policy.json`;
- `architecture/architecture-program-sequence.json`;
- `architecture/lifecycle-projection-policy.json`;
- `.github/scripts/architecture-acceptance.mjs` behavior and self-test.

Do not begin by writing new code from a stale handoff.

### Last synchronized execution snapshot — informational only

At this plan synchronization on 2026-08-20:

- protected `main`: `14b4a145bb12b7a84f3da8c9fa6a376c39a748f8` (#390);
- AR-11: mechanically accepted through the generic migration bootstrap;
- Git-derived current slice: AR-12;
- AR-12 implementation: NOT STARTED;
- tracking issue #375: OPEN;
- observed protected required contexts: 23;
- historically observed applicable permanent PR workflows: 17, but this is not a timeless constant;
- `agent/post-ar11-lifecycle-read-cutover` is an unaccepted candidate branch and has no lifecycle authority until exact-head PR acceptance and merge proof.

Every future agent must replace this snapshot with a live reread before acting.

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

For any candidate retirement use exactly this safety sequence:

```text
map invariant
-> identify current required semantics
-> port invariant to neutral/current authority
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

## 4. Accepted execution progress and remaining bounded sequence

The default remaining sequence is Unit A through Unit F. Boundaries may be refined only when live evidence proves a cleaner split. Do not combine unrelated lifecycle, `opsctl`, inventory, runtime, workflow and naming changes in one large PR.

### 4.1 Accepted progress — do not repeat without new defect evidence

The following post-AR-11 work is already accepted and should be treated as prerequisite history, not reimplemented:

- **#381 / A1 — ACCEPTED:** introduced and bound `architecture/lifecycle-projection-policy.json`, strengthened documentation authority, classified tracked lifecycle artifacts as projections and preserved this canonical plan.
- **#382 — ACCEPTED prerequisite:** fixed deterministic Release Set verification so colocated root `release-set.json` is accepted only when it parses to the exact verified manifest; all other unexpected artifacts still fail closed. Also moved the affected download action to an immutable Node-24-capable pin.
- **#383/#384 — ACCEPTED prerequisite:** one-shot allowlisted cleanup of stale hosted Actions registrations, direct live read-only proof of canonical active registry, then removal of temporary mutation authority; permanent governance returned to read-only.
- **#385 / A2a — ACCEPTED:** removed obsolete pre-2J/#203 lifecycle ownership from generic inventory-core documentation validation without changing generated inventory lifecycle projection.
- **#386 / A2b — ACCEPTED:** extracted neutral credential/security invariant authority and parity/negative tests.
- **#387 / A2c — ACCEPTED:** cut the current inventory credential source/validation caller over to the neutral credential owner.
- **#388 / A2d — ACCEPTED:** decoupled neutral credential validation from the historical inventory engine and closed AR-8D successor/security parity gaps.
- **#389 / A2e — ACCEPTED:** cut the remaining current credential negative-fixture and summary hooks over to neutral wrappers, establishing zero current credential-validator callers in the historical engine.
- **#390 / A2f — ACCEPTED:** removed duplicate credential implementation from the historical engine and retained only thin compatibility delegation. The engine itself remains `KEEP_PYTHON` / `CURRENT_INVARIANT`; it was not proven `DEAD`.

Do not restore removed one-shot workflow mutation machinery, historical credential implementation or old lifecycle closeout code without new concrete defect evidence.

### Unit A — Lifecycle projection authority finalization

Goal: make Git-derived lifecycle semantics singular and unambiguous, remove tracked snapshot ownership of accepted/current AR state, and finish neutralization without introducing a second derivation algorithm.

#### A1 — Projection policy and authority binding — ACCEPTED

Already completed by #381. Do not redesign without a proven defect.

#### A2 — Historical lifecycle assertion and credential/security prerequisite extraction — ACCEPTED

A2a through A2f are accepted through #385–#390. Neutral credential authority now owns current credential/security validation. Historical lifecycle compatibility still remains and must be removed only with parity evidence.

#### A3a — Lifecycle read-side authority cutover — NEXT BOUNDED UNIT

This step must change read-side authority before changing projection bytes.

Required implementation:

1. Current inventory generation consumes the existing canonical `.github/scripts/architecture-acceptance.mjs derive` result.
2. Python/Rust consumers may validate the returned shape and static successor relationship, but must not independently enumerate, parse or interpret architecture acceptance tags.
3. `docs/status.json`, `architecture/inventory.json`, `architecture/architecture-rebaseline-v3-transition.json` and human documentation must not be read to decide `accepted_checkpoint`, `current_slice` or acceptance.
4. Bind consumption to `architecture/lifecycle-projection-policy.json` and fail closed if that policy drifts to permit tracked snapshot authority or duplicate derivation.
5. Add negative proof for malformed/contradictory canonical deriver output and run the canonical acceptance negative self-test.
6. Preserve current projection output bytes/semantics intentionally in A3a. This is a read-authority cutover, not projection synchronization.

A3a must not:

- update stale AR-10/AR-11 projection values yet;
- implement AR-12;
- create a Python/Rust acceptance-tag parser;
- change capability activation;
- mutate production/provider state.

Required proof:

- canonical derive still reports AR-11 accepted and AR-12 current;
- current inventory path calls the canonical derivation authority;
- mutation of tracked lifecycle snapshot fields cannot change derived lifecycle state;
- malformed canonical output fails closed;
- inventory remains deterministic and projection-compatible;
- full applicable repository/Python estate/Runtime/Camoufox/Windows gates remain green on the exact candidate.

#### A3b — Lifecycle projection synchronization — PENDING A3a ACCEPTANCE

Only after A3a is accepted:

1. Remove hardcoded current-lifecycle ownership such as `CURRENT_SLICE = "AR-11"`, `CURRENT_DELIVERY_CHECKPOINT = "AR-10"`, AR-specific accepted-slice append logic and lifecycle monkey-patching from the current inventory path.
2. Build lifecycle projection fields from the canonical derived state, not from tracked mutable snapshots.
3. Update `architecture/inventory.json`, `docs/status.json`, `architecture/architecture-rebaseline-v3-transition.json` and relevant human docs to project:

```text
accepted checkpoint = AR-11
current architecture slice = AR-12 (Git-derived)
AR-12 implementation = NOT STARTED
production = BLOCKED
```

4. Mark these files explicitly as non-authoritative compatibility/human projections.
5. Preserve all fail-closed production invariants.
6. Prove that projection staleness cannot create or revoke acceptance.

Stale tracked projection during the A3a/A3b transition is tolerated only as explicit migration debt. It is not lifecycle authority and must be synchronized before Unit A closure.

#### A4 — Remaining lifecycle compatibility neutralization / Unit A closeout

After A3b, map any remaining AR-8/AR-10/AR-11 lifecycle compatibility executable or global. Replace/rename only when exact invariant mapping and parity prove a neutral current successor. Do not remove a compatibility executable merely to improve naming.

Unit A required final checks:

- exactly one acceptance derivation implementation exists;
- missing/conflicting/non-contiguous acceptance evidence fails closed;
- no second source commit is required to project future acceptance;
- no tracked projection can decide accepted/current state;
- no inventory generator can advance lifecycle state;
- `architecture_complete=false`, `production_core_gate=BLOCKED`, `production_ready=false`, `production_mutation=false` remain unchanged;
- documentation checker rejects any projection attempting to become lifecycle authority;
- full repository/Python estate gates remain green.

### Unit B — Canonical `opsctl` role and exact command registry

Goal: define `opsctl` as the repository-local Operational Policy & Decision Control Plane and mechanically register the implementation surface.

Target role:

```text
inspect -> classify -> plan -> preflight -> verify -> explain
```

Never:

```text
provision -> deploy -> provider mutate -> secret readback -> database mutate -> customer-state mutate
```

Provider execution boundary must remain:

```text
opsctl
policy / compatibility / plan / verify
    -> GitHub Actions orchestration
    -> GitHub Environment approval + scoped credentials
    -> Wrangler / dedicated provider executor actual mutation
    -> snapshot / evidence
    -> opsctl verify
```

Do not move Cloudflare/provider mutation into Rust merely for aesthetic consistency. Rust owns semantics, policy, compatibility, planning and verification; provider executors own mutation behind approval boundaries.

Tasks:

1. Evolve `architecture/operator-contract.json` rather than creating a competing registry unless a separate implementation contract is demonstrably necessary.
2. Register every command family with namespace, actions, mode, authority/source, inputs, output type/schema, side effects, network authority, provider mutation authority, secret authority, activation owner and ACTIVE/RESERVED status.
3. Re-read and register the actual active surface; expected families include `doctor`, `status`, `inventory`, `credentials status`, `credentials rotation-plan`, D1 status/plan/compatibility/verify, release inspect/verify/compatibility and promotion plan/preflight/verify.
4. Mark recovery RESERVED for AR-14 and readiness RESERVED for AR-16. They must have no executable surface before their owning slice.
5. Remove obsolete flat spellings from current authority; historical names may remain only as explicit historical provenance.
6. Add a permanent fail-closed gate enforcing actual Rust CLI surface == canonical registered active surface and rejecting premature reserved commands.

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

Explicitly reject production `Command::new("wrangler")`, `Command::new("node")`, `Command::new("npx")`, provider HTTP clients, GitHub mutation clients, D1 mutation clients and secret retrieval.

Required checks:

- authority -> implementation and implementation -> authority exact-surface parity;
- negative tests for unknown/unowned commands and premature reserved namespaces;
- source scan for child-process/network/provider mutation authority;
- operator contract contains current canonical namespaced spellings only;
- existing command semantics remain behaviorally unchanged.

### Unit C — `opsctl` internal modularity and operator semantics

Goal: make the operator tool understandable to a new developer in minutes and scalable through AR-16 without a central god-parser or mixed-responsibility modules.

Tasks:

1. Separate repository IO, lifecycle-policy consumption, subsystem semantics and operator presentation.
2. Keep `lib.rs` as composition/dispatch root, not a business/policy implementation warehouse.
3. Do not let `cli.rs` become a god-parser; move subsystem-specific parser details next to owning modules where this reduces coupling.
4. Preserve clear vertical modules for credentials, D1, release, promotion, future recovery and readiness.
5. Define a coherent machine-readable output envelope with stable fields such as `schema_version`, `command`, `status`, `mode`, `mutation_executed`, `decision` and `evidence`, while allowing typed command-specific payloads.
6. `opsctl status` may display live lifecycle state and compatibility projections, but must clearly distinguish them and must not implement a divergent lifecycle parser.
7. Keep runtime/application crates independent from `opsctl`.
8. Document the mechanical extension procedure: owning AR authorization -> operator contract -> typed command -> side-effect registration -> tests -> exact-surface gate -> full CI.

Required checks:

- no runtime/application dependency on `tools/opsctl`;
- no semantic behavior expansion beyond registered authority;
- parser golden/negative tests;
- Rust fmt, clippy and tests on Linux/Windows-supported paths;
- output compatibility tests for active families;
- a new developer can identify repository IO, policy/domain semantics, provider execution and presentation ownership without historical AR archaeology.

### Unit D — Inventory modularization and deterministic aggregation

Goal: make `architecture/inventory.json` a deterministic compatibility/operator projection assembled from domain-owned authorities, not a giant source of everything and not lifecycle authority.

Target shape:

```text
domain-owned architecture authorities
    -> typed/strict validation
    -> deterministic builders
    -> stable aggregation
    -> architecture/inventory.json
```

Tasks:

1. Map each inventory section to an existing domain authority where one already exists: D1, runtime, release, credentials, operator contract and other established authorities.
2. Do not create parallel registries merely to split files.
3. For every section make ownership explicit: domain owner, source authority, schema/version, validation, builder/generator, stable/live/historical class and authoritative/projection class.
4. Use typed builders/models with explicit `schema_version` and strict validation at trust boundaries.
5. Consider JSON Schema only where it materially improves repository/IDE validation without creating manually duplicated schema drift.
6. Do not mechanically convert Git-reviewed architecture metadata to protobuf. Protobuf remains appropriate for wire/IPC concerns such as Profile Bridge <-> Camouhost, not for repository metadata merely to feel typed.
7. Use fail-closed unknown-field rejection where correct, but do not apply it mechanically when compatibility requires controlled unknown fields.
8. Remove lifecycle constants, AR-specific mutation overlays and global monkey-patching from generation.
9. Preserve stable architecture information intentionally; unknown giant chunks must not survive through accidental pass-through.
10. Make repeated generation byte-stable/idempotent.

Required checks:

- every generated section has an explainable owner/source;
- repeated generation produces identical bytes;
- generator cannot advance accepted/current/next AR state;
- malformed/unknown authority input fails closed;
- stable architecture information is retained by explicit ownership;
- authority vs projection and stable vs live vs historical are mechanically clear;
- giant-file rewrite risk is materially reduced.

### Unit E — Historical executable classification and evidence-proven retirement

Goal: retire only evidence-proven zombie executable machinery after successor parity exists. This unit is a classification-and-retirement audit, not a deletion quota.

Priority review set includes:

- `scripts/generate-architecture-inventory-engine.py`;
- `scripts/generate-ar8-completion-status.py`;
- `scripts/check-documentation-authority-legacy.py`;
- historically named pre2j / phase / AR checkers;
- D3 successor/bootstrap checks that may still contain permanent current invariants.

Important current constraint:

`generate-architecture-inventory-engine.py` remains `KEEP_PYTHON` / `CURRENT_INVARIANT` after #390. It must remain while it still owns required compatibility/invariants. Historical naming cleanliness is not evidence for `DEAD`. Only after all remaining current semantics have neutral successors, parity is proven and current callers are zero may it be reclassified and removed.

Tasks:

1. Build caller graph and invariant map for each candidate.
2. Preserve `CURRENT_INVARIANT` and `UPGRADE_ROLLBACK_REQUIRED` functionality.
3. Ensure `TRANSITION_PROVENANCE_ONLY` executables have zero accidental current execution authority/callers.
4. Remove only `DEAD` executables after parity proof.
5. Update historical executable debt taxonomy and canonical Python estate after every addition/removal.
6. Perform semantic scans, not only filename grep, for closeout writers, `current_slice`/`accepted_checkpoint` writers, historical materialization, self-writing CI, temporary workflow restoration, legacy promotion executors, obsolete bootstrap mutation and orphan staging mutation.
7. Preserve static historical documents/evidence where provenance remains useful.

Required checks:

- unknown Python fails closed;
- no retired executable materialization/execution path remains;
- no unique invariant is lost;
- rollback-required tools remain available;
- provenance-only executables cannot accidentally become current authority.

### Unit F — Final pre-AR-12 audit and closure of #375

Goal: prove the repository can begin AR-12 without historical lifecycle ownership and without weakening production/runtime gates.

Audit dimensions:

- lifecycle authority singularity;
- production fail-closed state;
- preservation of independently activatable application functionality;
- historical executable debt;
- inventory ownership/aggregation;
- `opsctl` role, registry, modularity and safety;
- developer-facing architecture clarity;
- Python estate completeness;
- GitHub Actions supply-chain hygiene;
- permanent PR CI, protected contexts and post-merge evidence.

Repository-wide semantic scan must include `.github/scripts/**`, `.github/workflows/**`, `scripts/**`, `tools/**` and `architecture/**` and must analyze callers/authority, not only filenames.

Do not close #375 until every final DoD item below is mechanically proven.

## 5. PR proof vs accepted-main proof boundary

A defect discovered after #381 demonstrated that PR-green and accepted-main-green are different evidence classes. Preserve this architecture principle:

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

If the available evidence surface cannot observe a required post-merge workflow, classify it `UNPROVEN`, never implicitly `SUCCESS`. Final #375 closure requires direct hosted evidence for every required post-merge-only check or a canonical mechanism that makes the evidence mechanically observable.

A deterministic contract defect first discovered only after merge is an architecture/test-coverage defect and must be fixed with a bounded follow-up; it is not the desired normal model.

## 6. PR and exact-head discipline

Every bounded unit must follow:

```text
exact accepted main
-> new bounded branch
-> Draft PR immediately
-> one concern
-> implementation
-> freeze exact candidate SHA
-> full applicable CI
-> re-read main/reviews/threads/protection
-> Ready
-> guarded squash bound to expected exact head
-> accepted-main reread
-> candidate tree == merge tree
-> relevant post-merge proof
```

After final CI begins on a chosen exact candidate, any source change invalidates the old CI evidence. Re-run the complete applicable evidence set on the new exact SHA.

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

## 7. GitHub Actions supply-chain hygiene

Whenever `.github/workflows/**` or action dependencies change:

- use official/approved actions;
- pin actions to immutable full commit SHAs;
- require a supported runtime such as current Node support required by GitHub-hosted runners;
- do not introduce floating mutable tags;
- keep canonical governance policy green;
- treat deprecation warnings as bounded maintenance evidence, not automatic proof of functional failure;
- do not mass-update unrelated Actions "while here".

Temporary mutation/evidence workflows must have an explicit bounded lifetime and must be removed after their purpose is proven. Permanent governance should remain read-only unless an owning architecture explicitly grants a narrowly scoped mutation boundary.

## 8. Merge and architecture-acceptance boundaries

Use squash merge with expected exact head SHA for bounded cleanup PRs unless live governance requires a more specific accepted path. After merge, prove candidate tree == accepted merge tree and reread protected `main`.

AR-12+ architecture acceptance remains one guarded source merge plus immutable acceptance Git metadata over the same history. Do not reintroduce:

- per-AR closeout transformer scripts;
- AR-specific closeout PRs;
- self-writing workflows;
- second source PRs solely to record acceptance;
- force-moving/deleting acceptance tags;
- manual `current_slice` / `accepted_checkpoint` state as lifecycle authority;
- second independent lifecycle derivation algorithm.

## 9. Definition of Done before AR-12 implementation

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
- [ ] Human documentation projects AR-11 accepted / AR-12 current / AR-12 NOT STARTED and is explicitly non-authoritative.

### Production and capabilities

- [ ] `architecture_complete=false`.
- [ ] `production_core_gate=BLOCKED`.
- [ ] `production_ready=false`.
- [ ] `production_mutation=false`.
- [ ] No capability was production-enabled accidentally.
- [ ] No application functionality was deleted merely because it is disabled.
- [ ] `source_present != production_enabled` remains mechanically enforced.

### Historical executable debt

- [ ] Every suspicious executable is classified.
- [ ] `DEAD` items are removed.
- [ ] Current invariants are retained or renamed only with parity proof.
- [ ] Upgrade/rollback-required tools are retained.
- [ ] Transition-provenance executables have no accidental current execution authority.
- [ ] No retired executable materialization/execution path remains.
- [ ] No file is removed merely because its name is historical.
- [ ] `generate-architecture-inventory-engine.py` is retained unless and until live evidence proves it `DEAD`.

### Inventory

- [ ] Inventory has no live lifecycle ownership.
- [ ] No AR-8/AR-10/AR-11 monkey-patch lifecycle model controls current state.
- [ ] Current inventory generation consumes canonical lifecycle derivation rather than tracked lifecycle snapshots.
- [ ] Aggregation is deterministic and domain ownership is clear.
- [ ] Stable information has explicit ownership.
- [ ] Historical snapshots are explicitly non-authoritative.
- [ ] Stable/live/historical and authority/projection classes are developer-comprehensible.
- [ ] Giant-file rewrite risk is materially reduced.

### `opsctl`

- [ ] Canonical role is explicit.
- [ ] Actual CLI equals registered active CLI.
- [ ] Old flat current-authority command names are removed.
- [ ] D1, Release and Promotion are registered.
- [ ] Recovery is RESERVED for AR-14.
- [ ] Readiness is RESERVED for AR-16.
- [ ] Provider mutation = false.
- [ ] Network authority = false.
- [ ] Secret readback = false.
- [ ] Customer/database/deployment mutation = false.
- [ ] Production child-process execution authority = 0.
- [ ] Repository IO, policy and presentation concerns are separated.
- [ ] CLI/module architecture is suitable for AR-12 through AR-16.
- [ ] Output semantics are coherent and machine-readable.
- [ ] Developer documentation clearly explains ownership, provider-execution boundary and extension rules.

### CI, merge and post-merge proof

- [ ] All applicable permanent PR workflows are green on one exact final head.
- [ ] All protected required contexts are green on that same exact head.
- [ ] Real Camoufox cold-launch proof is green.
- [ ] Windows Profile Bridge regression is green.
- [ ] `behind_by=0`, blocking reviews=0, unresolved threads=0.
- [ ] Guarded squash is bound to the exact candidate head.
- [ ] Accepted `main` is re-read after merge.
- [ ] Candidate tree equals accepted merge tree.
- [ ] Every relevant `push: main` / accepted-main-only / hosted-state / durable-publication workflow is directly verified after merge.
- [ ] No required post-merge evidence is silently treated green when it is unobservable.
- [ ] No deterministic contract defect is first discovered only after merge on the final certification path.
- [ ] GitHub Actions dependencies touched by hardening remain immutable-pinned and runtime-supported.

### Developer experience and architecture clarity

- [ ] A new developer can locate application business logic, domain code, runtime adapters, operator policy, provider execution, GitHub orchestration, architecture authority and historical evidence without reading multiple obsolete AR implementations.
- [ ] One concern has one clear owner; projections may be many, competing authorities may not.
- [ ] No unnecessary full rewrite or registry proliferation was introduced.

### Tracking

- [ ] #375 remains OPEN until every item above is proven.
- [ ] #375 is closed only after final Unit F exact-head and post-merge evidence is complete.

Only after this checklist is complete may the project begin AR-12 implementation. At that point the intended platform is:

```text
lifecycle = Git-derived
inventory = typed/domain-owned deterministic projection
opsctl = operational policy & decision plane
GitHub = orchestration/approval
Wrangler/provider executors = mutation boundary
application capabilities = independently activatable
production = still BLOCKED
```

The final engineering rule for all remaining work is:

```text
make ownership explicit
preserve real invariants
prove parity mechanically
remove only proven debt
keep execution bounded
```
