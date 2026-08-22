# PF-1 — Canonical Architecture Inventory + Lifecycle Policy Cutover

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Functional Closure plan:** `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`  
**Architecture evolution contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Tracker:** issue #430  
**Sequence:** PF-1 -> PF-2 -> PF-3 -> re-baseline #399/#421 -> FC-6 -> FC-7  
**Production authorization:** NONE  
**AR-12 implementation:** NOT AUTHORIZED by this document

PF-1 is the first reference implementation of the target architecture. It is not a new AR slice and does not create a second lifecycle/program authority. Its final state must remove the current split in which architecture inventory generation is Python-owned while architecture acceptance/lifecycle semantics are Node-owned.

The target is one typed, deterministic Rust operational-policy core in `opsctl`, supplied by explicit repository/hosted observations from outer adapters.

## 1. Audited defect and corrected boundary

The current pre-PF-1 estate contains a real architecture defect:

- `.github/scripts/architecture-acceptance.mjs` is not merely a Git observer; it owns `contract`, `derive`, `premerge`, `record` and self-test policy semantics;
- `derive` interprets acceptance metadata and returns already-derived `accepted_checkpoint`, `current_slice` and production-gate state;
- current `architecture/architecture-acceptance-policy.json` still contains legacy governance assumptions including tracking issue #375 and `merge_method = squash`;
- current `architecture/lifecycle-projection-policy.json` names `.github/scripts/architecture-acceptance.mjs derive` as the live deriver/explicit sync source;
- meanwhile current target governance requires guarded exact-head merge commits that preserve the exact-green candidate as an auditable parent;
- `architecture-acceptance-recorder.yml` already obtains raw Git/GitHub observations directly using checkout/git, `gh api`, GraphQL review state and workflow/check-run APIs; therefore `architecture-acceptance.mjs` is not a uniquely required observation adapter.

Therefore PF-1 must not consume `.mjs derive` as trusted semantic input. Doing so would preserve a legacy policy engine upstream of the new typed Rust core.

Correct target boundary:

```text
Git / GitHub / repository checkout
        ↓
outer observation adapters
        ↓
RawArchitectureAcceptanceEvidenceV1
        ↓
opsctl typed validation
        ↓
pure AcceptancePolicy / LifecycleEvaluator
        ↓
DerivedLifecycleStateV1
        ↓
pure ArchitectureInventoryCompiler
        ↓
canonical render/check/inspect
        ↓
exactly one bounded GENERATED_PROJECTION_WRITE
        ↓
architecture/inventory.json
```

## 2. Observation shell vs policy core

### 2.1 Outer observation shell

GitHub Actions and repository-local adapters may obtain raw facts using mechanisms already owned by those surfaces:

- exact source/base/candidate/merge SHA;
- commit tree identities and ordered parents;
- annotated acceptance tag names, targets and annotation bytes;
- PR number/title/head/base identity;
- required check-run observations;
- applicable workflow-run observations;
- review decision and unresolved-thread observations;
- accepted-main reread identity;
- other versioned hosted observations explicitly required by the current acceptance policy.

The observation shell does **not** decide:

```text
accepted_checkpoint
current_slice
architecture_complete
production_core_gate
production_ready
production_mutation
accept/reject lifecycle policy outcome
```

Those are policy decisions.

### 2.2 Rust pure policy core

`opsctl` receives only explicit versioned input documents. It does not invoke:

```text
git
gh
GitHub API
Node
Python
provider APIs
network clients
```

The Rust policy core owns typed parsing and deterministic evaluation of:

- `architecture/architecture-program-sequence.json`;
- the current versioned architecture acceptance policy after cutover;
- `RawArchitectureAcceptanceEvidenceV1`;
- acceptance-record validity;
- lifecycle derivation;
- cross-authority lifecycle invariants required by the inventory compiler.

`ProcessExecution`, `NetworkAccess` and provider effects are forbidden in this path.

## 3. Raw evidence contract

PF-1 introduces one closed/versioned raw acceptance observation contract. Exact physical path/name may be chosen during implementation, but the semantic distinction is mandatory.

A raw observation document carries facts, not conclusions. It must bind enough identity to reproduce lifecycle evaluation without hidden state, including as applicable:

```text
schema_version
repository identity
source branch / accepted main identity
observed acceptance tag set
per-tag annotation bytes or closed parsed record
per-tag target commit SHA
commit tree SHA
first/ordered parent SHA set where required
candidate SHA/tree
merge SHA/tree
PR identity
required check-run observations
applicable permanent workflow observations
review/thread observations
accepted-main reread observation
observation producer/version
```

Unknown fields in closed schemas fail closed. Missing/ambiguous acceptance observations fail closed.

Raw evidence must not contain secrets, provider customer data or unrelated application state.

## 4. Canonical lifecycle evaluation

The single target lifecycle algorithm is a pure deterministic Rust function over typed policy + typed sequence + typed raw observations.

Conceptually:

```text
ValidatedProgramSequence
+ ValidatedArchitectureAcceptancePolicy
+ ValidatedRawAcceptanceEvidence
        ↓
LifecycleEvaluator
        ↓
DerivedLifecycleStateV1 {
  accepted_checkpoint,
  current_slice,
  architecture_complete,
  production_core_gate,
  production_ready,
  production_mutation
}
```

The evaluator must preserve accepted historical invariants while removing legacy implementation assumptions that conflict with current governance.

The static AR order remains owned by `architecture/architecture-program-sequence.json`; Rust does not invent a second sequence.

Accepted metadata remains append-only/immutable according to the current accepted policy after its PF-1 cutover. Generated/human projections never decide accepted/current lifecycle state.

## 5. Acceptance-policy cutover

PF-1 owns the bounded cleanup of acceptance-policy implementation because lifecycle semantics feed the architecture inventory and current `architecture-acceptance.mjs` is a policy engine, not merely an observer.

The cutover must explicitly reconcile current target governance with the legacy policy fields before switching authority. At minimum audit and correctly disposition:

- stale tracking ownership referring to closed #375;
- legacy `merge_method = squash` versus current guarded merge-commit exact-head discipline;
- architecture PR identity rules that remain valid or are superseded;
- accepted metadata/tag semantics;
- exact-head required contexts/workflow evidence;
- candidate-tree == accepted-merge-tree proof;
- first-parent/base proof;
- accepted-main reread proof;
- pre-AR-17 and AR-17 production-gate state.

No policy field may be silently changed merely to make tests green. Every changed semantic must be justified by the current canonical program/governance contract.

## 6. No dual lifecycle authority during cutover

PF-1 may temporarily contain old and new implementations for parity testing, but there is never a state where both are legitimate current authorities.

Required transition:

```text
BEFORE
architecture-acceptance.mjs = current lifecycle/acceptance implementation

IMPLEMENTATION / PARITY
Rust evaluator exists as candidate only
old implementation remains the only accepted authority

CUTOVER COMMIT / CANDIDATE
all current callers switch atomically to Rust policy semantics
lifecycle projection policy switches to Rust authority
acceptance recorder/governance gate consume Rust validator/evaluator
old implementation loses current authority

AFTER
Rust evaluator = sole current lifecycle/acceptance policy implementation
architecture-acceptance.mjs caller count = 0
architecture-acceptance.mjs unique-current-invariant count = 0
architecture-acceptance.mjs = DEAD -> DELETE
```

A compatibility alias or retained Node observer is forbidden unless a final repository-wide proof finds a legitimate current consumer that cannot use the existing workflow/Git observation boundary. The current audit found no unique raw-observation capability in the Node file, so target disposition is **DELETE**, not KEEP_AS_OBSERVER.

## 7. Current caller retirement scope

At minimum re-prove and switch all current callers discovered at implementation time, including presently known uses in:

- `.github/workflows/github-governance-gate.yml` (`contract`, `derive`, `premerge`, `self-test`);
- `.github/workflows/architecture-acceptance-recorder.yml` (`contract`, `premerge`, `record`);
- `scripts/generate-architecture-inventory.py` (`derive` through Node subprocess);
- `architecture/lifecycle-projection-policy.json` live deriver/explicit-sync references;
- `tools/opsctl` doctor/repository-root assumptions that still require historical inventory executables;
- `.github/workflows/quality-gate.yml` and `.github/workflows/repository-quality-audit-gate.yml` inventory/opsctl caller surfaces;
- any repository validator, documentation, generated projection or test that names the old deriver or Python inventory generator.

Caller discovery must be repeated on the exact candidate head before deletion. `opsctl doctor`, repository-root detection and permanent quality/audit workflows must no longer require a retired Node/Python predecessor after cutover.

## 8. Inventory compiler layering

PF-1 must keep inventory compilation separate from both repository effects and domain-specific validation.

Target flow:

```text
RepositoryAuthorityLoader / typed input adapters
        ↓
ValidatedAuthoritySet
        +
DerivedLifecycleStateV1
        ↓
PureInventoryBuilder
        ↓
ArchitectureInventory
        ↓
CanonicalRenderer
```

This allows pure unit/property tests over typed snapshots without requiring a giant fixture repository.

### 8.1 Domain-validation boundary

PF-1 must not turn `opsctl architecture inventory` into a god-validator.

Responsibility split:

```text
Domain/machine authority validator
  -> full semantics owned by that bounded authority

Inventory compiler
  -> identity/kind/version/ownership/reference validation
  -> cross-authority consistency required for the projection

Inventory projection
  -> deterministic readable representation
  -> no independent semantic authority
```

If an invariant is fully owned inside one bounded context/authority, its specialized validator remains the primary semantic owner. PF-1 references or invokes typed results/contracts rather than duplicating the invariant.

### 8.2 Canonical authority inputs and source ownership

Known machine/domain contracts use typed Rust models. Inventory inputs are classified explicitly as:

```text
A. DERIVED_REPOSITORY_STRUCTURE
B. EXISTING_CANONICAL_MACHINE_OR_DOMAIN_AUTHORITY
C. INTENTIONAL_STATIC_CONTRACT only where no stronger authority exists
```

PF-1 must not mechanically translate historical Python `CLASSIFIERS`, route specs, document-status tables or other constant registries into Rust constants and call that a cutover. A static Rust table is the last resort and requires explicit ownership justification.

At minimum the exact-candidate audit must consume/validate the current applicable authority set including:

- `architecture/runtime-topology-ar2.json`;
- typed D1 policy from `tools/opsctl/src/d1` and executable migration history from `migrations/d1` plus `migrations/resolver-d1`;
- `architecture/runtime-cutover-ar10.json`;
- `architecture/release-architecture-ar11.json`;
- `architecture/credential-authority.json`;
- `architecture/credential-lifecycle.json`;
- `architecture/profile-security.json`;
- `architecture/operator-contract.json`;
- `architecture/architecture-program-sequence.json`;
- the cut-over current architecture acceptance policy;
- `architecture/lifecycle-projection-policy.json` as projection-policy input after it is corrected to the new authority boundary;
- legitimate workspace/application/runtime/generated-contract/document-classification inputs discovered on the candidate tree.

Generic JSON values are permitted only at genuine extension boundaries and cannot bypass kind/version/ownership validation. The applicable authority inventory must be rediscovered from the current candidate rather than frozen forever to this prose list.

## 9. Inventory CLI and effects

Target active inventory surface remains:

```text
opsctl architecture inventory render [explicit typed lifecycle/evidence input as required]
opsctl architecture inventory check
opsctl architecture inventory check [explicit typed lifecycle/evidence input]
opsctl architecture inventory write [explicit typed lifecycle/evidence input]
opsctl architecture inventory inspect
```

Exact final argument spelling is subordinate to the typed contract. The invariant is that explicit synchronization receives versioned observations/evidence, not an already-authoritative legacy derived decision from Node.

`render/check/inspect` remain read-only. Exactly one repository mutation is allowed:

```text
GENERATED_PROJECTION_WRITE -> architecture/inventory.json
```

No arbitrary `--output` path, hidden stdin authority, Git write, GitHub write, provider write, DB write or customer-state mutation.

## 10. Canonical bytes and atomic write

PF-1 reuses one neutral Rust canonical JSON/SHA-256 primitive shared with release policy and later PF-2.

Required properties:

- identical validated inputs -> byte-identical render;
- write bytes == render bytes for the same inputs;
- repeated write is idempotent;
- canonicalization has property/golden tests;
- invalid/noncanonical tracked bytes are diagnosed precisely;
- atomic replacement is cross-platform safe for supported Linux/Windows environments;
- failure before activation leaves the previous tracked inventory intact;
- post-write readback is parsed/validated and byte-compared.

## 11. Predecessor retirement matrix

PF-1 has two bounded predecessor groups.

### 11.1 Inventory-generation predecessors

Expected candidates, subject to exact caller/invariant proof:

- `scripts/generate-architecture-inventory.py`;
- `scripts/generate-architecture-inventory-engine.py`;
- `scripts/_architecture_inventory_core.py`.

### 11.2 Acceptance/lifecycle implementation predecessor

Target predecessor:

- `.github/scripts/architecture-acceptance.mjs`.

Deletion requires:

```text
all callers switched
+ positive parity for still-valid semantics
+ negative parity/fail-closed coverage
+ no unique current invariant left
+ policy/projection references switched
+ governance/acceptance workflows green
+ repository-wide caller scan = 0
```

### 11.3 Historical provenance and current executable-debt bookkeeping

Frozen historical provenance is not rewritten merely because a current executable becomes DEAD.

In particular:

- `architecture/python-estate-ar6.json` remains immutable accepted AR-6 provenance; PF-1 must not falsify its historical counts or classifications to match the new live estate;
- current PF-1 predecessor disposition belongs in `architecture/historical-executable-debt.json` and other current overlays owned by the current architecture program;
- current classifications must distinguish historical provenance from executable/current authority;
- deletion of `.github/scripts/architecture-acceptance.mjs` or the Python inventory cluster happens only after current caller/invariant proof, while historical commits/evidence remain intact.

## 12. Positive proofs

At minimum PF-1 must prove:

- current accepted repository authorities load successfully;
- applicable authority inputs are sourced from current machine/domain owners or justified repository derivation, not blindly copied registries;
- existing legitimate stable/domain projection coverage is preserved;
- raw observation fixtures parse deterministically;
- valid historical acceptance chain derives the expected checkpoint/current slice;
- current target governance merge identity is accepted by the new policy evaluator;
- lifecycle evaluation is deterministic and pure;
- inventory render is byte-identical across repeated runs;
- pure builder works from typed snapshots without real repository subprocesses;
- write == render and repeated write is idempotent;
- plain check handles allowed non-authoritative snapshot staleness;
- Linux and Windows pass;
- `opsctl doctor`, repository-root detection, quality gate and repository-quality-audit gate no longer depend on retired predecessors;
- acceptance recorder and governance gate use the new Rust semantics after cutover;
- old Node/Python current callers are absent after cutover.

## 13. Negative proofs

At minimum reject:

- malformed/unknown raw evidence schema;
- unknown/duplicate acceptance tag observation;
- tag/path/record slice mismatch;
- non-contiguous accepted lifecycle after a gap;
- incorrect tag target/merge identity;
- candidate tree != accepted merge tree where policy requires equality;
- first parent/base mismatch;
- incomplete/failed exact-head required checks;
- incomplete/failed applicable permanent workflows;
- blocking review/unresolved thread evidence;
- stale/incorrect accepted-main reread identity;
- lifecycle accepted/current successor mismatch;
- pre-AR-17 `architecture_complete=true`;
- pre-AR-17 `production_core_gate=AUTHORIZED`;
- premature `production_ready=true` or `production_mutation=true`;
- unknown authority kind/version;
- missing required canonical authority;
- generic/untyped input bypassing a known authority kind/version contract;
- central inventory logic duplicating a domain-only semantic invariant instead of using the owned validator/contract;
- inventory byte drift;
- attempt to write any path except `architecture/inventory.json`;
- hidden process/network/Git/GitHub/provider access in Rust lifecycle/inventory path;
- interpreted upstream document that attempts to declare accepted/current lifecycle as raw observation;
- old `.mjs` caller remaining after declared cutover;
- retired Python inventory caller remaining after declared cutover;
- dual current lifecycle implementation/authority;
- silent policy weakening to accommodate a fixture.

## 14. Definition of Done

PF-1 closes only when one exact candidate head proves all of the following:

1. one typed Rust acceptance/lifecycle evaluator is the sole current policy implementation;
2. one typed Rust architecture inventory compiler/checker/writer is the sole current inventory implementation;
3. Git/GitHub observations remain outside Rust and enter through explicit versioned raw evidence;
4. Rust performs no Git/Node/Python/GitHub/provider subprocess/network access;
5. domain-specific semantic validators remain owned by their bounded authorities rather than duplicated into inventory;
6. pure typed builder and repository adapters are separable and directly tested;
7. applicable canonical authority inputs are typed/classified and existing legitimate stable/domain projection coverage is preserved;
8. `architecture-acceptance.mjs` current caller count = 0 and unique-current-invariant count = 0;
9. `architecture-acceptance.mjs` is deleted from the current executable estate;
10. Python inventory predecessor caller count = 0 and every DEAD predecessor is deleted;
11. `architecture/python-estate-ar6.json` remains immutable history while current executable-debt overlays describe the new live disposition;
12. lifecycle projection policy and acceptance policy point to the new single authority model and no longer encode superseded #375/squash semantics as current target policy;
13. `opsctl doctor`, repository-root detection and permanent quality/audit workflows no longer require retired predecessors;
14. one shared canonical JSON/digest authority is used;
15. `GENERATED_PROJECTION_WRITE` is the only new repository-write effect and is fixed to `architecture/inventory.json`;
16. permanent positive/negative tests cover lifecycle, policy, inventory and cutover invariants;
17. Linux/Windows and all applicable workflows/protected contexts are green on the same unchanged head;
18. `behind_by=0`, blocking reviews=0, unresolved threads=0;
19. guarded merge is bound to the exact candidate head and preserves the proven candidate tree according to current governance;
20. accepted-main reread confirms no old lifecycle/inventory authority remains reachable;
21. production stays fail-closed and AR-12 remains NOT STARTED.

Only accepted PF-1 `main` may become the base for PF-2. No PF-4 or other new planning phase is introduced.
