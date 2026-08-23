# PF-1 — Canonical Architecture Inventory + Lifecycle Policy Cutover

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Functional Closure:** `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`  
**Pre-PF-1 prerequisite:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**Mandatory architecture requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**opsctl doctor:** `docs/OPSCTL_DOCTOR_CONTRACT.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**Tracker:** #430  
**Production authorization:** NONE  
**AR-12 implementation:** NOT AUTHORIZED

PF-1 begins only after F1/F2, the bounded pre-N2 F1 compatibility/current-v2 cleanup gate, and N1…N5 are accepted on protected `main`.

It is not a new AR slice. It is the bounded cutover from legacy Node lifecycle + Python architecture inventory/projection machinery to one typed deterministic Rust lifecycle/inventory implementation in standalone `opsctl`.

## 1. Binding prerequisite order

```text
F1/F2
 -> N1 AR-2 authority retirement
 -> bounded pre-N2 F1 compatibility/current-v2 cleanup gate
 -> N2 AR-6 Python estate retirement
 -> N3 current GitHub governance normalization
 -> N4 bounded AR-8 operator/provenance cleanup
 -> N5 AR-10 runtime authority retirement
 -> PF-1
```

The pre-N2 cleanup is a bounded correction transaction, not a new F/N/AR/PF slice.

PF-1 must not preserve retired AR JSON/Python/Node authorities merely because the old inventory/status loader consumed them.

## 2. Correct lifecycle boundary

Current legacy `.github/scripts/architecture-acceptance.mjs` owns policy semantics rather than merely raw observation. PF-1 removes that policy owner after parity/cutover.

Target:

```text
Git / GitHub / repository checkout
        ↓
outer observation adapters
        ↓
RawArchitectureAcceptanceEvidenceV1
        ↓
strict typed decode/validation
        ↓
pure LifecycleEvaluator
        ↓
DerivedLifecycleStateV1
```

Outer observation may collect exact SHAs/trees/parents, acceptance tags/annotation bytes, check/workflow observations, reviews/threads and accepted-main reread facts.

Outer observation does **not** decide:

```text
accepted_checkpoint
current_slice
architecture_complete
production_core_gate
production_ready
production_mutation
```

Those are typed policy decisions.

`opsctl` does not call `git`, `gh`, GitHub API, Node, Python, provider API or network in this path.

The static AR order remains owned by `architecture/architecture-program-sequence.json`.

## 3. Raw evidence contract

Introduce one closed/versioned raw architecture-acceptance observation contract. It binds enough identity to reproduce lifecycle evaluation without hidden state, including as applicable:

```text
schema_version + kind
repository identity
source/base/candidate/merge SHA + tree identities
ordered parent/base facts
observed acceptance tags + targets + annotation bytes/closed record
PR identity
required check/workflow observations
review/thread observations
accepted-main reread observation
producer/version/observation identity
```

Unknown fields in closed schemas fail closed. Missing/ambiguous required observations fail closed. No secrets/customer payloads.

Time/freshness values are explicit inputs; pure policy never reads current clock/cwd/env/randomness.

## 4. Lifecycle policy cutover

PF-1 audits and migrates still-valid lifecycle semantics, including:

- immutable acceptance metadata/tag rules;
- contiguous accepted program sequence;
- exact candidate/merge tree identity where required;
- first-parent/base proof;
- exact-head required-check/workflow evidence;
- review/thread constraints;
- accepted-main reread;
- pre-AR-17 production fail-closed state.

Legacy assumptions that conflict with accepted current governance, including stale issue ownership or old squash-only mechanics, are explicitly dispositioned rather than copied forward.

Temporary candidate parity is allowed. Dual current authority is not.

```text
BEFORE: Node = current lifecycle implementation
CANDIDATE: Rust exists but is non-authoritative
CUTOVER: all callers switch atomically
AFTER: Rust sole owner; Node caller_count=0; unique_current_invariant_count=0; DELETE
```

## 5. Inventory compiler target — bounded projections only

PF-1 must **not** implement:

```text
RepositoryAuthorityLoader
 -> ValidatedAuthoritySet
 -> giant central god-validator
```

Target input boundary:

```text
ValidatedInventoryInputs {
  lifecycle: LifecycleInventoryProjection,
  d1: D1InventoryProjection,
  runtime_topology: RuntimeTopologyProjection,
  application: ApplicationInventoryProjection,
  operator: OperatorInventoryProjection,
  governance: GovernanceInventoryProjection,
  runtime: RuntimeInventoryProjection,
  credentials: CredentialInventoryProjection,
  release: ReleaseInventoryProjection,
}
        ↓
pure ArchitectureInventoryCompiler
        ↓
ArchitectureInventory
        ↓
canonical renderer
```

Each natural owner validates its own semantics and exposes only the facts needed by inventory.

The compiler owns cross-reference consistency/canonical projection, not D1/release/runtime/credential policy itself.

## 6. Source ownership after normalization

PF-1 must consume current natural owners/projections, not the retired transitional sources:

```text
D1
  SQL + typed Rust D1 policy -> D1InventoryProjection

Runtime/resource topology
  Wrangler/provider config + Product ownership -> RuntimeTopologyProjection

Application architecture
  Rust structure + bounded source observations -> ApplicationInventoryProjection

Operator
  Rust CommandRegistry/effect registry -> OperatorInventoryProjection

GitHub governance
  current desired governance data + live raw observation -> GovernanceInventoryProjection

Camoufox runtime
  Product Rust + runtime-lock manifest + owned runtime facts -> RuntimeInventoryProjection

Credentials
  bounded current credential owners -> CredentialInventoryProjection

Release
  current versioned Release Set/release owners -> ReleaseInventoryProjection
```

Forbidden as PF-1 semantic inputs after their owning normalization completes:

```text
runtime-topology-ar2.json current semantic authority
python-estate AR-6/10/11 overlay chain
historical AR-7/AR-10 required-check overlay chain
operator-contract.json as CLI semantic owner
runtime-cutover-ar10.json current semantic authority
architecture/inventory.json as source for its own facts
docs/status.json as lifecycle semantic authority/manual current-state bag
manual AR-qualified application ownership registry as a second semantic owner
```

Application architecture facts that are directly discoverable from Rust structure/composition/route ownership stay with those natural sources. A source observer may report structural facts; it may not become a new hand-maintained application architecture database.

## 7. `opsctl` internal boundary

PF-1 follows `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`.

```text
filesystem/JSON/raw bytes
        ↓
adapters + versioned DTOs
        ↓
typed inputs
        ↓
PURE CORE
        ↓
typed result
        ↓
rendering/write adapter
```

Hard requirements:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
Path/PathBuf as semantic identity = 0
Product Runtime -> opsctl = 0
opsctl -> Python semantic subprocess = 0
```

A small internal `opsctl-core` crate is preferred where it materially enforces the boundary at compile time.

## 8. Inventory CLI/effects

Target surface:

```text
opsctl architecture inventory render
opsctl architecture inventory check
opsctl architecture inventory write
opsctl architecture inventory inspect
```

Exact arguments are subordinate to typed contracts.

`render/check/inspect` are read-only.

Only one bounded repository mutation is allowed:

```text
GENERATED_PROJECTION_WRITE -> architecture/inventory.json
```

No arbitrary output path, Git/GitHub/provider mutation, DB mutation or runtime execution.

`architecture/inventory.json` remains a tracked generated projection only.

## 9. Canonical bytes/write discipline

PF-1 uses the accepted F2 canonical/digest foundation.

Requirements:

```text
same validated inputs -> byte-identical render
write bytes == render bytes
repeated write idempotent
bounded/strict external decode
canonical/pretty representation separated
atomic cross-platform replacement
failure before activation preserves old file
post-write readback parse + byte equality proof
```

Every digest declares whether it covers semantic canonical bytes or exact artifact bytes.

## 10. `opsctl doctor`, `opsctl status` and repository-root cutover

PF-1 must treat `doctor.rs`, `status.rs` and `repository.rs` as first-class callers of the lifecycle/inventory cutover.

Current `tools/opsctl/src/status.rs` is a thin passthrough to `docs/status.json`. Fresh audit found that the tracked status projection is materially stale: it still contains superseded #268/Python-estate/governance/operator/Node-lifecycle/current-work assumptions and pre-AR-10 runtime evidence. Therefore it must **not** be manually refreshed into another hand-maintained current-state authority bag.

Target:

```text
DerivedLifecycleStateV1 + bounded owned projections
        ↓
typed StatusProjection / StatusReport
        ↓
versioned machine rendering
```

`docs/status.json`, if retained as a tracked file, is generated projection only. `opsctl status` must derive/render from current typed owners or consume a generated projection whose generation is owned by PF-1; it must not remain a blind passthrough to manually maintained lifecycle semantics.

After N2/N4/PF-1:

- no AR-6 Python estate file is required;
- no `scripts/python-estate-ar6.py` is required;
- no legacy Python inventory generator is required;
- no `operator-contract.json` is required as CLI semantic authority;
- no Node lifecycle predecessor is required;
- `docs/status.json` is not a semantic lifecycle source and no stale historical field controls current execution;
- repository-root identity uses durable surviving repository markers, not generated projections/retired AR sentinels;
- `doctor` remains local read-only diagnostic composition and does not call Python/Node/process/network/provider/runtime.

Detailed doctor DoD: `docs/OPSCTL_DOCTOR_CONTRACT.md`.

## 11. Python predecessor retirement

PF-1 owns retirement of the **architecture inventory/projection cluster**, not a global Python rewrite.

Expected predecessors subject to exact-candidate caller/invariant proof:

```text
scripts/generate-architecture-inventory.py
scripts/generate-architecture-inventory-engine.py
scripts/_architecture_inventory_core.py
scripts/_ar3_application_architecture.py   # semantic ownership/projection role must be dispositioned
```

The last path is not automatically deleted merely because it is Python. PF-1 must specifically audit its current manual semantic tables, including the semantic equivalents of `PROCESS_OWNERSHIP`, `CAPABILITY_OWNERSHIP`, `COMPOSITION_FINDINGS`, `_REQUIRED_SNIPPETS` and `_FORBIDDEN_SNIPPETS`.

Required disposition:

```text
for each still-valid fact/invariant:
    map to natural Rust/source/owned contract
    preserve bounded structural observation only where needed

manual AR-qualified ownership/policy table current authority = 0
1:1 port of those tables into Rust/JSON/YAML/TOML = FORBIDDEN
```

If `_ar3_application_architecture.py` has no remaining unique structural-observer role after caller/invariant cutover, delete it. If a bounded source-observer implementation remains useful, it must emit observations only and contain no competing mutable application-architecture registry.

N2 already retires the per-file Python estate authority chain. PF-1 must not resurrect it as a compatibility registry.

Legitimate Python runtime/test/generator adapters remain according to `docs/PYTHON_USAGE_BOUNDARY.md`.

## 12. Node predecessor retirement

Target predecessor:

```text
.github/scripts/architecture-acceptance.mjs
```

Delete only after:

```text
all callers switched
positive parity for valid semantics
negative/fail-closed parity
old caller count = 0
old unique-current-invariant count = 0
policy/projection references switched
hosted governance/acceptance workflows green
```

The current audit found no unique raw-observation capability that justifies a retained Node lifecycle observer.

## 13. Known caller classes to rediscover on exact head

At minimum audit:

```text
github-governance-gate workflow
architecture-acceptance-recorder workflow
quality/repository audit workflows
lifecycle-projection policy references
legacy Python inventory generator
AR-3 application architecture projection/validator callers
opsctl doctor
opsctl status / docs/status.json consumers
opsctl repository-root detection
documentation/projection validators
developer verify-fast orchestration
```

Exact candidate discovery is authoritative; prose lists are not permanent caller registries.

## 14. Positive proofs

At minimum:

- valid raw evidence derives the expected accepted/current lifecycle;
- current guarded merge identity is accepted;
- pure lifecycle evaluation deterministic across repeated runs;
- `opsctl status` reports lifecycle/current-work facts from typed current owners rather than stale manually maintained JSON;
- bounded inventory projections compile without raw authority bag;
- application projection derives current facts from natural Rust/source owners rather than a hand-maintained AR-qualified ownership registry;
- inventory render byte-identical across repeated runs;
- write == render and repeated write is idempotent;
- specialized bounded validators remain primary semantic owners;
- Linux/Windows equivalent typed input gives equivalent semantic result;
- `doctor`, `status` and repository-root no longer require retired predecessors;
- governance/acceptance workflows use Rust policy after cutover;
- repository-wide old Node/Python caller scan = 0.

## 15. Negative proofs

Reject/prove absence of:

```text
unknown/malformed raw evidence schema
duplicate/ambiguous acceptance observation
non-contiguous lifecycle
wrong tag/merge/tree/parent/base identity
failed/missing exact-head checks/workflows
blocking review/unresolved thread
stale accepted-main reread
premature architecture_complete/production gate/production_ready/mutation
unknown bounded projection kind/version
raw serde_json::Value semantic bypass
global authority bag
docs/status.json used as a manually maintained lifecycle/current-work authority
opsctl status blind passthrough to stale lifecycle semantics
inventory compiler duplicating bounded policy
manual AR-qualified application ownership registry used as semantic input
1:1 Rust/JSON/YAML/TOML port of retired AR-3 ownership tables
inventory byte drift
write to any path except architecture/inventory.json
process/network/Git/GitHub/provider access in lifecycle/inventory core
Node caller after cutover
retired Python inventory caller after cutover
dual lifecycle authority
silent policy weakening
```

## 16. Definition of Done

PF-1 closes only when one exact candidate head proves:

1. typed Rust lifecycle evaluator is sole current implementation;
2. typed Rust inventory compiler is sole current inventory implementation;
3. raw Git/GitHub observation remains outside pure Rust policy;
4. compiler consumes bounded typed projections, not a global authority bag;
5. `serde_json::Value`/filesystem/process/network/provider do not cross into pure core;
6. domain-specific validators remain with natural owners;
7. `opsctl status`/`docs/status.json` no longer constitute a manually maintained lifecycle/current-work authority; retained status JSON is generated projection only;
8. manual AR-qualified application ownership registry current authority is zero and `_ar3_application_architecture.py` is deleted or reduced to a bounded observation-only role with no duplicate semantic tables;
9. Node lifecycle predecessor has zero callers/unique invariants and is deleted;
10. Python inventory predecessors have zero callers/unique invariants and DEAD files are deleted;
11. `opsctl doctor` and repository-root no longer require retired AR/Python/Node sentinels;
12. `architecture/inventory.json` is generated projection only;
13. one bounded `GENERATED_PROJECTION_WRITE` exists only for explicitly owned generated projections and no arbitrary repository mutation surface is introduced;
14. positive/negative tests cover lifecycle/status/inventory/cutover;
15. Linux/Windows + all applicable permanent workflows pass on one exact head;
16. protected required contexts pass, `behind_by=0`, blocking reviews=0, unresolved threads=0;
17. guarded merge is bound to exact proven head;
18. post-merge accepted-main reread finds no reachable old lifecycle/inventory/status authority;
19. production remains fail-closed and AR-12 remains NOT STARTED.

Only accepted PF-1 `main` may become PF-2 base.
