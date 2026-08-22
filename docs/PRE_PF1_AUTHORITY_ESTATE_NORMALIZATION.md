# Pre-PF-1 Authority Estate Normalization

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Functional Closure plan:** `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`  
**PF-1 detailed specification:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**Mandatory architecture requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**Tracking umbrella:** issue #399  
**Production authorization:** NONE  
**AR-12 implementation:** NOT AUTHORIZED by this document

This document defines the mandatory foundation and authority-estate normalization that must complete before PF-1 implementation entry. It does not reopen accepted AR-2/AR-6/AR-7/AR-8/AR-10 history, create a new AR slice, create PF-0/PF-4, or alter `architecture/architecture-program-sequence.json`.

The purpose is to prevent PF-1 from becoming a Rust rewrite of stale phase-qualified JSON/Python/Node authorities. Accepted functionality remains; current semantic ownership, representation and effect boundaries are normalized first.

## 1. Binding continuation order

```text
Pre-PF-1 foundations
  F1  Release Set breaking-contract version discipline
   +
  F2  opsctl pure-core / adapter boundary
      + canonical external JSON / digest discipline
      + application-wide mandatory architecture requirements
      + Python usage/effect boundary
   ->
Authority Estate Normalization
  N1  AR-2 runtime/resource topology authority retirement
   ->
  N2  AR-6 Python-estate current-authority retirement + Python role/effect normalization
   ->
  N3  AR-7 current GitHub-governance authority normalization
   ->
  N4  bounded AR-8 operator/provenance authority cleanup
   ->
  N5  AR-10 runtime semantic authority retirement
   ->
PF-1  typed lifecycle evaluator + deterministic inventory compiler + Node/Python predecessor deletion
   ->
PF-2  Universal Hosted Operational Evidence
   ->
PF-3  Architecture Fitness Baseline with typed Rust rule ownership
   ->
fresh re-baseline #399 / #421
   ->
FC-6 real staging same-bits / rollback rehearsal
   ->
FC-7 final whole-AR-11 functional audit
   ->
AR-12 implementation entry
```

F1/F2 and N1..N5 are foundation/normalization transactions, not lifecycle slices. They preserve the accepted AR sequence and historical evidence.

## 2. Mandatory ownership vocabulary

For every touched machine artifact classify its current role as exactly one of:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

A `TRANSITIONAL_SEMANTIC_SOURCE` retirement is complete only after:

```text
identify natural owner
-> preserve accepted behavior/invariants
-> switch every current consumer
-> predecessor caller_count = 0
-> predecessor unique_current_invariant_count = 0
-> physically delete/demote predecessor
-> preserve provenance in Git/evidence
```

Do not replace a retired semantic JSON with successor JSON/TOML/YAML containing the same semantics. Do not copy a giant old table into Rust and call that convergence.

JSON remains legitimate for external manifests/contracts, observations, durable evidence and generated projections.

## 3. F1 — Release Set contract version discipline

The accepted AR-9 authority retirement changed future Release Set D1 identity from the historical AR-9 authority-file digest to the actual repository-derived D1 identity.

The incompatible field/meaning change from:

```text
schemas.d1_evolution_authority_sha256
```

to:

```text
schemas.d1_repository_identity_sha256
```

must not remain under Release Set schema v2.

Requirements:

- create a new current Release Set contract version, target v3 unless exact-candidate evidence proves another valid bounded version decision;
- do not rewrite immutable historical v2 artifacts;
- do not accept two incompatible meanings under v2;
- retain a historical-v2 decoder only if a current #399/#421/FC consumer proves it is needed;
- current writer/build/verify/content-address prefix and fixtures agree on the new version;
- breaking-contract-without-version-bump becomes a permanent PF-3 failure.

## 4. F2 — permanent architecture/effect foundations

The following contracts are mandatory requirements, not advisory notes:

- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`;
- `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`;
- `docs/PYTHON_USAGE_BOUNDARY.md`.

### 4.1 opsctl boundary

Required flow:

```text
JSON bytes / filesystem / explicit artifacts
        ↓
adapters + versioned DTOs
        ↓
typed semantic inputs
        ↓
PURE CORE
        ↓
typed semantic result
        ↓
output adapter
```

Permanent zero budgets:

```text
serde_json::Value crossing adapter -> pure-core = 0
filesystem/process/network/provider effects in pure core = 0
runtime product dependency on opsctl = 0
opsctl runtime service endpoint = 0
opsctl provider/network/process authority = 0
global raw authority bag = 0
```

The current D1 split (`d1/authority.rs` adapter -> typed models -> `d1/plan.rs` evaluator) is the reference direction. Release, promotion, lifecycle, evidence, inventory and fitness touched paths converge to the same boundary.

A small internal `opsctl-core` crate is recommended when it materially makes the boundary compile-time enforceable. It is operator-tool internal and must not become a Product Runtime dependency.

### 4.2 canonical JSON/digest

Dependency count is not a quality target. Reviewed/pinned pure dependencies are preferable to bespoke cryptography/canonicalization when they reduce risk.

Before PF-2 attestable evidence depends on this layer:

- use a reviewed/pinned SHA-256 implementation unless a documented necessity proves otherwise;
- define one canonical external JSON scheme, preferably RFC 8785 JCS if compatible with the contract;
- distinguish semantic canonical-JSON digest from exact-file-byte digest;
- use independent published hash/canonicalization vectors;
- reject duplicate JSON object member names before semantic evaluation/canonicalization for security/release/evidence-critical contracts;
- bound input bytes and parser depth/complexity;
- use explicit schema version/kind and UTF-8;
- keep human pretty rendering separate from canonical digest bytes.

### 4.3 Python boundary

Python is not globally banned. It is allowed only in the roles/effects defined by `docs/PYTHON_USAGE_BOUNDARY.md`.

Permanent principle:

```text
Python may adapt, observe, generate, test or host a genuine cross-language runtime.
Python must not become a second semantic owner or an ungoverned provider mutation authority.
```

## 5. N1 — AR-2 runtime/resource topology authority retirement

### Historical purpose preserved

AR-2 classified Workers, D1, R2, Durable Objects, queues/DLQ, service bindings, schedules and provider lanes; it proved `GENERATION_VERIFICATION=DELETE`, retained resolver isolation and disabled the legacy D3 forward production lane.

### Target ownership

```text
Cloudflare deployment/resource topology -> Wrangler/provider-native config
runtime/workload ownership             -> Product Rust composition/handlers/contracts
capability production admission        -> Release / Capability Profile
structural anti-regression rules       -> bounded fitness/checker rules
AR-2 decision provenance               -> Git/evidence
```

Required cutover:

- stop treating `architecture/runtime-topology-ar2.json` as current semantic input;
- remove inventory dependence on it;
- remove duplicate manually-maintained resource-decision tables where current Wrangler/Rust owners derive the same fact;
- inspect Python deployment/config renderers and remove any unique duplicated topology semantics before retaining them as pure renderers;
- retain negative invariants such as no `GENERATION_VERIFICATION` queue workload and resolver isolation without retaining the historical AR-2 manifest as owner;
- after zero-callers/zero-unique-current-invariants, delete `architecture/runtime-topology-ar2.json` from the current tree.

No hosted Cloudflare mutation is authorized by N1.

## 6. N2 — AR-6 Python-estate authority retirement + Python normalization

### Historical purpose preserved

AR-6 established a Python census/disposition and introduced standalone Rust `tools/opsctl`. It did not justify a permanent hand-maintained Python-file database or a global Python-to-Rust rewrite.

### Defect

`architecture/python-estate-ar6.json` currently acts like a living file registry while still recording already-retired AR-10/MIGRATE paths. Repository source is the observation of which Python files exist; the permanent policy is role/effect based.

### Target Python role classes

```text
PRODUCT_RUNTIME_ADAPTER
SYNTHETIC_RUNTIME_FIXTURE
REPOSITORY_VALIDATOR
DETERMINISTIC_GENERATOR
TEST_OR_FIXTURE
DEVELOPER_ORCHESTRATOR
OUTER_OBSERVER
PROVIDER_CANARY
TRANSITIONAL_LEGACY_EXECUTABLE
```

### Mandatory N2 work

1. remove `architecture/python-estate-ar6.json` from current semantic/inventory/doctor authority;
2. remove `scripts/python-estate-ar6.py` from current authority/caller chains and delete it after zero-callers/zero-unique-current-invariants;
3. update `opsctl` repository-root discovery so it does not require AR-6 estate artifacts, `operator-contract.json`, or legacy Python inventory sentinels;
4. do **not** create a replacement JSON/TOML/YAML/Rust list of every Python file;
5. classify current Python by role + effects using source-derived observations;
6. prove legacy direct browser/profile Python executables retired by AR-10 remain absent/unreferenced;
7. retain `runtime/camouhost/real.py` as legitimate real Product Runtime adapter under the strict Bridge/IPC/runtime-lock boundary;
8. retain `runtime/camouhost/main.py` only as synthetic test fixture and prove it cannot become production runtime authority;
9. inspect validators/generators for duplicated semantic tables and assign those facts to N1/N4/N5/PF-1/PF-3 natural owners before retaining Python as observation/rendering implementation;
10. keep developer orchestration such as `scripts/verify-fast.py` non-authoritative and update it as Node/Python semantic predecessors are retired;
11. classify GitHub/provider-read Python as outer observation or transitional observer+policy; PF-2 owns evidence-policy separation;
12. replace `tools/r2_s3_canary.py` with a protected workflow using pinned official Wrangler R2 object `put/get/delete --remote`, unless exact implementation evidence proves a blocker; preserve ephemeral-canary cleanup/evidence, then delete the bespoke Python SigV4/credential path;
13. add negative tests that new unclassified Python production/network/provider-effect entrypoints fail closed.

### Explicit non-goal

N2 does not rewrite legitimate Python tests, deterministic generators, repository validators or the real Camouhost adapter to Rust merely for language uniformity.

## 7. N3 — AR-7 current GitHub-governance normalization

### Historical purpose preserved

AR-7 established protected `main`, required PR/check semantics, hosted `rehearsal/staging/production` Environments, non-bypass production approval and hosted read-only governance audit.

### Defect

Current required-check topology must not be reconstructed as:

```text
AR-7 baseline + AR-10 overlay + future AR overlays + ...
```

### Target

```text
current workflow/check registration -> current GitHub Actions/governance registry
expected governance configuration    -> one current bounded declarative contract/policy owner
observed hosted state                -> raw GitHub observation from outer workflow/API layer
accepted AR-7 baseline               -> historical evidence
```

Declarative desired state for an external control plane is legitimate data. It must not duplicate Product Runtime semantics.

Required cutover:

- stop using `architecture/github-governance-ar7.json` as an evolving historical overlay;
- move current required contexts/workflow registration to the current governance owner;
- preserve fail-closed live hosted-state verification;
- keep GitHub/API reads outside `opsctl` pure policy;
- historical AR-7 artifact remains only evidence if a real historical consumer needs it, otherwise retire after zero-callers/zero-unique-current-invariants.

## 8. N4 — bounded AR-8 operator/provenance cleanup

This is intentionally not a full credential-lifecycle rewrite.

### Operator authority

- typed Rust command registry becomes semantic owner of operator namespaces/actions/effect classes;
- CLI parsing/help/effect declaration and optional machine projection derive/validate against that owner;
- `architecture/operator-contract.json` must not dynamically authorize Rust CLI behavior;
- any retained operator JSON is generated projection, not competing authority.

### AR-8 provenance

- `architecture/ar8-*` and equivalent AR-8 phase artifacts are historical/provenance inputs only;
- normal current runtime/operator/inventory paths do not depend on phase overlays for semantics.

### Explicit deferral

`credential-lifecycle.json` and `profile-security.json` may remain bounded transitional subject contracts until AR-13/AR-14 or another accepted bounded cutover justifies typed ownership. PF-1 consumes only a narrow `CredentialInventoryProjection`, not a raw global credential bag.

## 9. N5 — AR-10 runtime semantic authority retirement

### Historical purpose preserved

AR-10 delivered the supported native Profile Bridge -> managed typed/versioned IPC -> real Camouhost -> pinned Camoufox persistent runtime path and retired historical direct executables.

### Target ownership

```text
runtime behavior / safety / launch semantics -> Product Rust
cross-language runtime dependency tuple       -> runtime/camouhost/runtime-lock.json
real Camoufox outer adapter                    -> runtime/camouhost/real.py
synthetic runtime fixture                      -> runtime/camouhost/main.py test-only
IPC semantic contract                          -> bridge-domain + cross-language validation
runtime failure guarantees                     -> implementation + tests
required CI contexts                           -> current GitHub governance owner
production/lifecycle state                     -> lifecycle/release owner
AR-10 acceptance/provenance                    -> Git/evidence
```

`runtime-lock.json` is a legitimate versioned cross-language manifest and remains.

Required cutover:

- reassign every unique current field in `architecture/runtime-cutover-ar10.json` to natural owner or history;
- remove inventory, acceptance, governance, release and doctor dependencies on it;
- do not move runtime execution into `opsctl`;
- preserve real Camoufox regression matrix and fake/test separation;
- preserve Python adapter boundary in `docs/PYTHON_USAGE_BOUNDARY.md`;
- after zero-callers/zero-unique-current-invariants, delete `architecture/runtime-cutover-ar10.json`.

## 10. PF-1 entry contract

PF-1 begins only after F1/F2 and N1..N5 are accepted on protected `main`.

Conceptual inputs:

```text
ValidatedProgramSequence
RawArchitectureAcceptanceEvidenceV1
cut-over typed AcceptancePolicy
D1InventoryProjection
RuntimeTopologyProjection
ApplicationInventoryProjection
OperatorInventoryProjection
GovernanceInventoryProjection
RuntimeInventoryProjection
CredentialInventoryProjection
ReleaseInventoryProjection
other bounded typed projections discovered on exact candidate
```

Forbidden target:

```text
GlobalRepositoryAuthorityLoader
  -> GlobalAuthoritySet
  -> every opsctl module
```

PF-1 target:

```text
outer Git/GitHub raw observations
-> typed lifecycle/acceptance evaluator
-> DerivedLifecycleStateV1
-> bounded validated inventory projections
-> pure deterministic ArchitectureInventoryCompiler
-> canonical render/check/inspect/write
-> delete legacy Node lifecycle engine
-> delete legacy Python inventory/projection cluster
```

`serde_json::Value`, `Path/PathBuf` and filesystem/provider/process types do not enter lifecycle/inventory pure policy functions. `architecture/inventory.json` is generated projection and cannot be semantic input for its own facts.

## 11. PF-2 entry contract

PF-2 consumes the accepted PF-1/F2 boundary rather than creating an evidence-specific parallel architecture.

```text
GitHub Actions / official provider tools / outer observation adapter
-> raw observations
-> versioned DTOs
-> typed normalized observations
-> pure EvidencePolicy
-> typed EvidenceDecision / envelope data
-> canonical external JSON
-> SHA-256
-> immutable artifact / GitHub attestation
```

Provider/GitHub reads, clocks and artifact publication remain outside pure core.

Current Python that combines GitHub API acquisition with semantic attestation decision is transitional. PF-2 separates observation acquisition from Rust evidence policy before retiring duplicate predecessor semantics.

`VALID`, `VALID_BUT_STALE` and `INVALID` remain distinct from mutation admission.

## 12. PF-3 owner and enforcement correction

PF-3 must not introduce `architecture/architecture-fitness-policy.json` as a manually maintained semantic authority.

Target:

```text
typed Rust FitnessRuleRegistry
        ↓
rule evaluation / enforcement mapping
        ↓
optional generated JSON projection/report
```

Python may remain a bounded structural observation/check implementation where justified, but the rule identity/applicability/anti-weakening semantic owner is typed Rust.

Minimum permanent zero/one budgets:

```text
semantic_owner_count_per_fact = 1
serde_json_value_crossing_into_opsctl_pure_core = 0
filesystem_import_in_opsctl_pure_core = 0
process_execution_in_opsctl = 0
network_access_in_opsctl = 0
provider_sdk_dependency_in_opsctl = 0
runtime_dependency_on_opsctl = 0
opsctl_runtime_service_endpoint = 0
global_authority_bag = 0
generated_projection_used_as_semantic_input = 0
breaking_external_contract_change_without_version_bump = 0
unversioned_durable_external_contract = 0
duplicate_json_member_accepted_in_attestable_contract = 0
manual_architecture_semantic_json_authority_without_explicit_exception = 0
unclassified_python_production_entrypoint = 0
unclassified_python_network_or_provider_effect = 0
python_duplicate_semantic_authority = 0
python_runtime_bypass_of_profile_bridge = 0
opsctl_python_child_process = 0
python_provider_mutation_without_explicit_exception = 0
legacy_python_estate_registry_current_authority = 0
```

## 13. Merge/transaction discipline

Each foundation/normalization step is a bounded authority transaction. Accepted `main` is re-baselined before the next transaction.

For each require:

```text
fresh protected-main baseline
complete caller discovery
field/invariant ownership matrix
positive parity proof
negative anti-regression proof
zero current callers to predecessor
zero unique current invariants in predecessor
exact-head CI/governance evidence
physical retirement/demotion in same merge candidate
post-merge reread
```

Do not force F1/F2/N1..N5 into one implementation PR. Their order is binding unless fresh evidence justifies an explicit plan amendment.

## 14. Non-goals

This normalization does not:

- rewrite accepted AR history;
- add product functionality;
- enable production;
- provision/delete hosted Cloudflare resources;
- globally ban Python or JSON;
- globally rewrite valid Python to Rust;
- move Camoufox runtime into `opsctl`;
- create Product Runtime <-> opsctl RPC;
- create generic DI/plugin/service-locator infrastructure;
- rewrite full credential lifecycle before its owner slice;
- start AR-12.

## 15. Exit criteria for PF-1 entry

PF-1 is blocked until the exact protected-main tree proves:

```text
Release Set current breaking shape correctly versioned = true
opsctl adapter/pure-core contract established = true
canonical JSON/digest contract established for future PF-2 = true
runtime-topology-ar2 current semantic authority = 0
python-estate-ar6 current semantic authority = 0
python-estate generator current authority = 0
historical AR-7 overlays used as evolving governance = 0
operator-contract JSON used as CLI authorization authority = 0
AR-8 provenance used as normal semantic input = 0
runtime-cutover-ar10 current semantic authority = 0
runtime product dependency on opsctl = 0
opsctl runtime/provider/network/process authority = 0
generated projection used as semantic source = 0
unclassified high-risk Python provider/runtime path = 0
```

Accepted functionality, security boundaries, source behavior and production fail-closed state remain unchanged. Only then does continuation advance to PF-1.