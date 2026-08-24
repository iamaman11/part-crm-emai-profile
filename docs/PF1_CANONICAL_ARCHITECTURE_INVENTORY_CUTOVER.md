# PF-1 — Canonical Architecture Inventory + Lifecycle Policy Cutover

**Document status:** ACCEPTED_HISTORICAL_STAGE_CONTRACT
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Pre-PF-1 prerequisite:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**Issue / accepted implementation:** #430 / #466
**Production authorization:** NONE

PF-1 replaced transitional lifecycle/inventory machinery with a small typed composition layer. This document preserves its accepted boundary and anti-regression requirements; it is not mutable current execution authority and does not start AR-12.

## 1. Entry gate

PF-1 starts only from fresh protected `main` after accepted #454 + N2…N5.

Required state:

```text
current Release Set writer/model = v3
current v2 semantic authority = 0
historical v2 compatibility = justified_and_isolated OR retired
tracked inventory retention = JUSTIFIED_MINIMUM OR NOT_RETAINED
Python estate overlay current authority = 0
historical governance overlay current authority = 0
operator JSON used as Rust CLI authorization = 0
runtime-cutover-ar10 current semantic authority = 0
production_mutation = false
```

Historical/evidence references are allowed when provenance-only. The tracked `architecture/inventory.json` retention decision is already made during the common pre-PF-1 N2–N5 discovery; PF-1 consumes that result rather than postponing the decision until this stage.

## 2. Permanent `opsctl` boundary

```text
external bytes/files/explicit observations
-> strict adapters + versioned DTOs
-> typed semantic inputs
-> PURE CORE
-> typed results
-> output adapters
```

Hard budgets:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
Product Runtime -> opsctl/opsctl-core = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
global authority bag = 0
```

`opsctl doctor` remains local read-only diagnostics only.

## 3. Lifecycle target

```text
outer Git/GitHub/repository observations
-> RawArchitectureAcceptanceEvidenceV1 (or equivalent strict DTO)
-> pure LifecycleEvaluator
-> DerivedLifecycleStateV1
```

Lifecycle policy is typed and deterministic. Observation acquisition remains outside pure policy. Legacy Node lifecycle code retires after parity/caller/invariant proof.

## 4. Inventory target

PF-1 receives only narrow projections already validated by their natural owners:

```text
D1InventoryProjection
RuntimeTopologyProjection
ApplicationInventoryProjection
OperatorInventoryProjection
GovernanceInventoryProjection
RuntimeInventoryProjection
CredentialInventoryProjection
ReleaseInventoryProjection
+ lifecycle projection
        ↓
pure ArchitectureInventoryCompiler
        ↓
optional deterministic generated projection
```

Hard rule:

```text
PF-1 compiler may COMPOSE facts
PF-1 compiler may NOT DISCOVER bounded-subject semantics
PF-1 compiler may NOT DECIDE bounded-subject policy
```

Forbidden:

```text
GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet
giant god-validator/compiler
raw serde_json::Value authority bag
manual AR-qualified ownership registry as compiler input
inventory compiler reimplementing D1/release/runtime/operator/governance/credential policy
generated projection used as semantic input
```

## 5. `architecture/inventory.json` retention is an input, not a PF-1 decision

The pre-PF-1 normalization contract has already audited concrete current/durable consumers of the exact tracked bytes and produced exactly one result:

```text
JUSTIFIED_MINIMUM
-> PF-1 may emit only the minimum deterministic GENERATED_PROJECTION required by the proved consumer
-> retained write, if genuinely required, is atomic + deterministic + idempotent + single-target

NOT_RETAINED
-> PF-1 does not resurrect tracked architecture/inventory.json
-> deterministic on-demand render/check may remain where useful
-> compatibility-only --write / tracked-byte drift ceremony stays retired
```

PF-1 therefore owns the **semantic inventory/lifecycle replacement**, not the first tracked-file retention decision. If pre-PF-1 normalization was able to retire the tracked file after switching its remaining callers, PF-1 accepts that state. If a durable exact-byte consumer justified retention, PF-1 preserves only that minimum projection and never treats it as semantic input.

`opsctl inventory`, `opsctl doctor`, repository-root detection, a generator/drift test, CI, a self-test or documentation that consumes the tracked file only because the file exists are internal cutover callers, not durable exact-byte consumers. Under `NOT_RETAINED`, they are deleted or redirected to typed natural owners/on-demand stdout in the same transaction as the tracked file; they cannot create a compatibility tail.

The same minimality rule applies to command surface. Keep only distinct useful operations; do not preserve `render/check/write/inspect` merely because the old system exposed them.

## 6. Application projection correction

Historical `_ar3_application_architecture.py` contains large manual semantic tables. PF-1 must not port them 1:1.

For every still-current fact:

```text
map to natural Rust/source/owned contract
-> retain bounded structural observation only if genuinely required
-> switch projection callers
-> manual AR-qualified ownership-table current authority = 0
```

If no unique observation role remains, delete the Python predecessor. If a narrow observer remains useful, it emits structural facts only and contains no competing mutable application-architecture registry.

## 7. Predecessor retirement

Known predecessor family includes legacy Node lifecycle and Python inventory/projection code. Exact-head discovery is authoritative; this document is not a permanent caller registry.

For each predecessor:

```text
still-current invariant mapped to natural owner
-> positive parity
-> negative/fail-closed parity
-> switch every current caller/workflow
-> old_current_callers = 0
-> old_unique_current_invariants = 0
-> delete DEAD predecessor
```

No compatibility bridge remains merely because PF-2/FC-6 is unfinished.

PF-1 is accepted only if it is a net simplification of the current lifecycle/inventory estate. The typed evaluator/compiler and focused tests may be added, but legacy Node/Python compilers, global tables, compatibility-only commands, tracked projections without consumers and duplicated validators are removed in the same transaction. A larger compiler/validator estate with the old estate still reachable is failure, not incremental progress.

## 8. `opsctl doctor` / repository root

PF-1 removes PF-1-owned lifecycle/inventory predecessor dependencies from `doctor`, repository-root resolution, workflows and developer orchestration.

Repository identity uses minimal durable surviving markers, never generated projections or files scheduled for retirement.

## 9. Proof expectations

Positive proof covers:

- deterministic lifecycle evaluation from typed observations;
- acceptance semantics under current guarded-merge governance;
- each bounded projection validated by its natural owner;
- compiler only composes typed facts;
- repeated render is byte-identical where bytes are contracted;
- retained write, if any, equals render and is idempotent;
- Linux/Windows equivalent semantic outcomes where applicable;
- `doctor`/repository-root free of retired sentinels;
- old Node/Python current caller scan = 0.

Negative proof rejects:

```text
malformed/unknown observation versions
duplicate/ambiguous acceptance observations
non-contiguous lifecycle
wrong source/tree/parent/base/tag identities
missing/failed required checks or blocking review/thread
premature production/architecture completion
raw semantic JSON bypass
global authority bag
bounded-subject policy duplicated in compiler
manual AR-qualified ownership registry as semantic input
1:1 successor port of retired AR-3 tables
arbitrary projection write target
process/network/provider access in pure core
dual lifecycle authority
reintroduced N1–N5 predecessor authority
silent policy weakening
```

## 10. Stage-specific DoD

```text
typed lifecycle evaluator = single current lifecycle policy owner
bounded projections validated by natural owners
compiler = composition only
manual AR-qualified application registry current authority = 0
legacy Node lifecycle current callers = 0
legacy Python inventory current callers = 0
legacy unique current invariants = 0 before deletion
tracked inventory/write surface = pre-decided JUSTIFIED_MINIMUM OR NOT_RETAINED
reintroduced N1–N5 semantic authority = 0
production_mutation = false
```

Shared exact-head CI/review/guarded-merge rules are owned by `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` and current branch protection.

Only accepted PF-1 protected `main` may become PF-2 base. Closed PR #428 is historical selective-salvage material only.
