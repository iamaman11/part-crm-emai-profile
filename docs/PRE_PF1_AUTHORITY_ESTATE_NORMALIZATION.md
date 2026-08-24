# Pre-PF-1 Authority Estate Normalization

**Document status:** ACCEPTED_HISTORICAL_PREREQUISITE
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Completed tracker:** #441
**Accepted PF-1:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` / #430 / #466
**Production authorization:** NONE

This document preserves the accepted #454/N2–N5 authority-retirement contract. It is historical anti-regression context, not mutable current execution authority, and does not create new lifecycle slices or change `architecture/architecture-program-sequence.json`.

## 1. Binding order

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 bounded Release Set v2 correction ACCEPTED
-> N2 ACCEPTED
-> N3 ACCEPTED
-> N4 ACCEPTED
-> N5 ACCEPTED
-> PF-1 ACCEPTED (#466)
```

#454 and N2–N5 are bounded normalization transactions, not F3/N1.5/AR-* or PF-0.

## 2. One retirement rule

Every touched machine artifact is classified as:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

A transitional semantic source retires through one reusable rule:

```text
natural owner identified
-> still-valid product/security/durable invariants preserved
-> current callers switched
-> old_current_callers = 0
-> old_unique_current_invariants = 0
-> physical delete/demotion
-> history preserved in Git/evidence
```

Do not replace a retired JSON/Python/Node/table authority with an equivalent successor registry in another language/format.

Shared exact-head CI/review/merge acceptance is owned by `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md`, not repeated per N-step.

An internal checker/generator/drift gate/self-test/`opsctl` sentinel/workflow that exists only to consume the predecessor belongs to the predecessor estate. It is not a durable consumer. The owning transaction removes or redirects it and deletes the predecessor atomically after moving the few unique current invariants; it does not first add another layer that validates the old validator.

## 3. Efficiency rule for N2–N5 + tracked-inventory retention

After accepted #454, perform one read-only discovery pass across all remaining predecessor estates to identify callers, unique current invariants and likely natural owners. Keep this discovery **ephemeral** (working notes / PR discussion / CI artifact), not as a new checked-in authority file.

The same common discovery must audit concrete **current/durable consumers of the exact tracked bytes** of `architecture/inventory.json`. This is a pre-PF-1 normalization decision; it is deliberately separated from PF-1's later semantic lifecycle/inventory replacement.

Count only a concrete current external/durable consumer, release contract or another proved current consumer that actually requires the tracked exact bytes. The following are **not** consumer proof:

```text
generator checks inventory because inventory exists
docs reference an inventory section
historical evidence
CI drift test that exists solely for the tracked projection
```

Resolve the retention result before PF-1:

```text
real durable exact-byte consumer exists
-> retain only the minimum deterministic GENERATED_PROJECTION required by it

consumer = NONE
-> retire tracked architecture/inventory.json when its remaining callers are naturally cut over
-> retire compatibility-only --write / tracked-byte drift ceremony
-> keep deterministic on-demand render/check only where useful
```

This does **not** authorize building `LifecycleEvaluator`, `ArchitectureInventoryCompiler` or another PF-1 semantic successor during N2–N5. If `consumer = NONE`, deletion should occur in the first natural bounded normalization transaction where remaining callers/invariants have already been switched; do not create a new phase or standalone cleanup issue for it.

Before each sequential N2/N3/N4/N5 transaction, refresh only:

```text
that transaction's affected reachability
+ changes since the common discovery
```

Separate merge boundaries remain because the semantic owners are different; repeated repository-wide archaeology is not required.

Every N2…N5 PR includes a compact before/after simplification ledger. Each transaction must strictly reduce current authorities, transitional sources, duplicate representations and compatibility-only callers. New current plan documents, successor registries, tracked projections without durable consumers, meta-checkers and net growth of the predecessor-governance surface are stage failures.

## 4. #454 — Release Set v2

#454 is accepted. It deleted the tracked v2 current authority, made v3 the only current writer/target model and isolated minimum historical v2 source/artifact integrity verification from current target semantics.

Accepted result:

```text
current_writer = v3
current_v2_semantic_authority = 0
historical_v2_executable = MINIMUM_ISOLATED_VERIFY_ONLY
v2_to_v3_semantic_coercion = 0
production_mutation = false
```

## 5. N2 — Python estate

Historical AR-6/AR-10/AR-11 per-file census/overlay data is provenance, not permanent current authority.

Target:

```text
repository/source observation
+
role/effect policy
-> classified Python usage
```

Requirements:

- atomically retire `architecture/python-estate-ar6.json`, AR-10/AR-11 overlays and `scripts/python-estate-ar6.py` after one bounded invariant/reachability pass;
- delete or redirect internal quality/repository-audit/Camoufox workflow calls, compile checks, inventory/runtime-cutover references and self-tests that exist only for that estate in the same cutover; none is durable-consumer proof;
- no successor 1:1 Python file registry;
- remove retiring Python/AR sentinels from `opsctl doctor` and repository-root detection;
- preserve `runtime/camouhost/real.py` as the genuine Camoufox outer-runtime adapter behind Profile Bridge + versioned IPC + `runtime-lock.json`;
- preserve `runtime/camouhost/main.py` synthetic/test-only;
- prove retired direct Python browser/profile executables remain absent/unreferenced;
- keep legitimate Python tests/generators/adapters/orchestration instead of rewriting them to Rust for symmetry;
- obsolete helper with caller=0 -> delete; helper with caller>0 -> move only its still-current responsibility to the natural owner;
- unclassified Python production/network/provider effects fail closed.

Python may adapt/observe/generate/test/host a genuine cross-language runtime. It must not become a second Product/release/lifecycle/evidence/fitness semantic owner or ungoverned provider mutation authority.

## 6. N3 — GitHub governance

Historical AR-7/AR-10 overlay reconstruction retires.

Target:

```text
current desired governance configuration
+
live GitHub observation
-> typed governance evaluation
```

Desired configuration for an external system may legitimately remain versioned declarative data. Live GitHub/provider reads remain outer observations; they do not move into `opsctl` pure policy.

No historical baseline+overlay stack remains the evolving current governance model after cutover.

## 7. N4 — operator/provenance

Typed Rust command/effect metadata becomes the operator semantic owner.

Prefer metadata colocated with existing command definitions and aggregation through the current parser/composition root. Do not create a second generic command framework.

`architecture/operator-contract.json`:

```text
no real current external consumer -> delete after caller proof
real consumer exists -> generated projection only
```

It must never authorize Rust behavior. AR-8 provenance leaves normal semantic paths. Credential/security subject contracts outside this bounded operator concern may remain until their natural owning cutover.

## 8. N5 — runtime semantic authority

For every still-current field in `architecture/runtime-cutover-ar10.json`:

```text
not current -> retire
current -> Product Rust | runtime-lock | Bridge/IPC | governance | release/lifecycle owner
```

Current runtime ownership:

```text
runtime behavior/safety/launch -> Product Rust
runtime dependency tuple       -> runtime/camouhost/runtime-lock.json
real Camoufox adapter          -> runtime/camouhost/real.py
synthetic fixture              -> runtime/camouhost/main.py test-only
IPC contract                   -> Bridge/domain + cross-language validation
runtime failure guarantees     -> implementation + tests
hosted required checks         -> current governance
lifecycle/release state        -> lifecycle/release owner
```

`runtime-lock.json` remains a legitimate versioned cross-language manifest. Do not move runtime execution into `opsctl` and do not create `RuntimeCutoverRegistryV2`.

## 9. Permanent `opsctl` budgets

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
Product Runtime -> opsctl/opsctl-core = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
global authority bag = 0
```

`opsctl doctor` remains local read-only diagnostic composition only.

## 10. PF-1 entry

PF-1 begins only when accepted protected `main` proves:

```text
#454 accepted
N2 accepted
N3 accepted
N4 accepted
N5 accepted
tracked inventory retention = JUSTIFIED_MINIMUM | NOT_RETAINED
current Release Set writer/model = v3
current v2 semantic authority = 0
Python estate overlay current authority = 0
historical governance overlay stack current authority = 0
operator JSON used as CLI authorization = 0
runtime-cutover-ar10 current semantic authority = 0
retired direct Python browser/profile executables = 0
production_mutation = false
```

N2–N5 leave natural owners unambiguous and resolve whether tracked inventory bytes have a real durable consumer. They do not pre-build PF-1's generic lifecycle/projection/compiler machinery.

## 11. Hosted tooling boundary

Use an authenticated supported GitHub client available in the execution environment. Verify viewer identity, repository and exact head before mutation; keep repository-local checks local. Do not shell-scrape GitHub or move GitHub/network authority into `opsctl` as a workaround.

Canonical references: `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`, `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`, `docs/OPSCTL_DOCTOR_CONTRACT.md`, `docs/PYTHON_USAGE_BOUNDARY.md`, #441, #454.
