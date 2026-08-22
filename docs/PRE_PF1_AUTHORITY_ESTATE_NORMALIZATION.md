# Pre-PF-1 Authority Estate Normalization

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Functional Closure:** `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`  
**Mandatory requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**opsctl doctor:** `docs/OPSCTL_DOCTOR_CONTRACT.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**PF-1:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**Tracking umbrella:** #399  
**Production authorization:** NONE  
**AR-12 implementation:** NOT AUTHORIZED

This document defines mandatory foundation and authority-estate normalization before PF-1. It does not reopen accepted AR history, add a new AR/PF slice or change `architecture/architecture-program-sequence.json`.

## 1. Binding order

```text
F1  Release Set breaking-contract version discipline
+
F2  permanent architecture foundations
    - application mandatory requirements
    - opsctl pure-core / adapter boundary
    - opsctl doctor diagnostic boundary
    - canonical external JSON / digest discipline
    - Python role/effect boundary
 ->
N1  AR-2 runtime/resource topology authority retirement
 ->
N2  AR-6 Python-estate authority retirement + role/effect normalization
 ->
N3  AR-7 current GitHub-governance normalization
 ->
N4  bounded AR-8 operator/provenance cleanup
 ->
N5  AR-10 runtime semantic-authority retirement
 ->
PF-1
 -> PF-2
 -> PF-3
 -> fresh #399/#421 re-baseline
 -> FC-6
 -> FC-7
 -> AR-12 implementation entry
```

F1/F2/N1…N5 are foundation/normalization transactions, not lifecycle slices.

## 2. Mandatory artifact-role vocabulary

Every touched machine artifact is classified as:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

A transitional semantic source retires only after:

```text
natural owner identified/proved
-> accepted behavior preserved
-> current consumers switched
-> old caller_count = 0
-> old unique_current_invariant_count = 0
-> physical delete/demotion
-> provenance preserved in Git/evidence
```

Do not replace a retired semantic JSON with an equivalent successor JSON/TOML/YAML or giant Rust table.

## 3. F1 — Release Set version discipline

The change from:

```text
schemas.d1_evolution_authority_sha256
```

to:

```text
schemas.d1_repository_identity_sha256
```

is a breaking external-contract semantic change and must not remain under one current Release Set v2 meaning.

Requirements:

- new current writer/model version, target v3 unless exact evidence proves another bounded version choice;
- historical immutable v2 assets are never rewritten;
- a historical-v2 decoder/verifier exists only if #399/#421 proves a current need;
- historical decoder is isolated from current writer/model;
- content-address prefix/fixtures/workflows agree with the new version;
- PF-3 permanently rejects breaking durable-contract changes without version bump.

## 4. F2 — permanent architecture/effect foundations

Binding contracts:

- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`;
- `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`;
- `docs/OPSCTL_DOCTOR_CONTRACT.md`;
- `docs/PYTHON_USAGE_BOUNDARY.md`.

### 4.1 `opsctl`

```text
JSON/filesystem/explicit local artifacts
        ↓
adapters + versioned DTOs
        ↓
typed semantic input
        ↓
PURE CORE
        ↓
typed semantic result
        ↓
output adapter
```

Permanent zero budgets:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
runtime Product -> opsctl/opsctl-core = 0
opsctl provider/network/process authority = 0
opsctl runtime service endpoint = 0
opsctl -> Python semantic child process = 0
global authority bag = 0
```

A small internal `opsctl-core` is preferred when it materially makes these constraints compile-time enforceable.

### 4.2 `opsctl doctor`

`doctor` is read-only local diagnostic composition only.

Current implementation debt includes a generic JSON `AUTHORITIES` list plus dependencies on AR-6/operator/Python inventory sentinels. Target:

```text
local filesystem observations
-> strict typed adapters
-> bounded owner diagnostics
-> DoctorReport
-> rendering
```

Forbidden:

```text
Python/Node/process execution
network/GitHub/provider access
secret resolution
runtime/browser execution
provider/database mutation
generic semantic authority bag
generated projection as semantic root sentinel
```

N2/N4/PF-1 remove retiring sentinel dependencies. PF-3 prevents regression.

### 4.3 canonical JSON/digest

Before PF-2 attestable evidence relies on this layer:

- reviewed/pinned SHA-256 implementation;
- one explicit canonical external JSON contract, preferably RFC 8785 JCS where compatible;
- duplicate JSON member rejection before canonicalization for release/security/evidence-critical documents;
- bounded bytes/depth/complexity;
- strict UTF-8 and explicit `kind`/`schema_version`;
- independent canonicalization/hash vectors;
- canonical identity bytes separate from pretty rendering;
- explicit digest scope: semantic canonical bytes vs exact artifact bytes.

### 4.4 Python

```text
Python may adapt, observe, generate, test or host a genuine cross-language runtime.
Python must not become a second semantic owner or an ungoverned provider mutation authority.
```

The permanent Python policy is role/effect based and source-derived, not a per-file whitelist.

## 5. N1 — AR-2 runtime/resource topology retirement

Historical AR-2 decisions remain evidence. Current topology ownership becomes:

```text
Cloudflare deployment/resource topology -> Wrangler/provider-native config
runtime/workload ownership             -> Product Rust
production admission                   -> Release / Capability Profile
anti-regression                         -> bounded fitness/checker rules
AR-2 provenance                         -> Git/evidence
```

Required:

- stop using `architecture/runtime-topology-ar2.json` as current semantic input;
- remove inventory/checker dependency on duplicated resource-decision tables;
- audit Python deployment/config renderers for unique duplicated topology semantics;
- preserve negative invariants such as no dead `GENERATION_VERIFICATION` path and resolver isolation;
- delete the AR-2 JSON after zero callers/zero unique current invariants.

No hosted provider mutation is authorized by N1.

## 6. N2 — AR-6 Python-estate retirement

Historical census remains provenance. Current Python estate is derived from repository/source observations + role/effect policy.

No successor 1:1 Python file registry is allowed.

Mandatory N2 work:

1. remove `architecture/python-estate-ar6.json` and AR-10/AR-11 estate overlays from current semantic/inventory/doctor authority paths;
2. remove `scripts/python-estate-ar6.py` from current authority/callers and delete after zero callers/unique invariants;
3. update `opsctl` repository-root detection away from AR-6/operator/Python inventory sentinels;
4. preserve `runtime/camouhost/real.py` as legitimate Product Runtime adapter behind Profile Bridge/IPC/runtime-lock;
5. preserve `runtime/camouhost/main.py` only as synthetic/test fixture;
6. prove retired direct Python browser/profile executables remain absent/unreferenced;
7. inspect validators/generators for duplicated semantic tables and reassign unique facts to N1/N4/N5/PF-1/PF-3 natural owners;
8. keep developer orchestration such as `verify-fast.py` non-authoritative and update its caller list as predecessors retire;
9. classify GitHub/provider-read Python as outer observation or transitional observer+policy; PF-2 owns the evidence-policy split;
10. replace bespoke Python provider-mutating helpers such as the R2 SigV4 canary with protected pinned official provider tooling where exact evidence shows parity, then delete old path after zero callers;
11. add negative tests for unclassified Python production/network/provider effects.

N2 does not globally rewrite legitimate Python tests/generators/adapters to Rust.

## 7. N3 — AR-7 current GitHub governance normalization

Historical AR-7 baseline remains evidence.

Current required-check/governance state must not be reconstructed as:

```text
AR-7 baseline + AR-10 overlay + future historical overlays
```

Target:

```text
current desired governance configuration
+
live GitHub observation
        ↓
typed governance policy evaluation
```

Desired external-system configuration may legitimately remain versioned declarative data. GitHub/API reads stay outside `opsctl` pure policy.

## 8. N4 — bounded AR-8 operator/provenance cleanup

This is not a full credential-lifecycle rewrite.

Required:

- typed Rust CommandRegistry/effect registry becomes operator semantic owner;
- CLI parser/help/machine projection derive/validate against that owner;
- `architecture/operator-contract.json` does not authorize Rust behavior;
- retained operator JSON, if any, is generated projection only;
- AR-8 phase/provenance artifacts leave normal current semantic paths;
- `credential-lifecycle.json` / `profile-security.json` may remain bounded transitional subject contracts until their owning future cutover;
- PF-1 consumes only a narrow `CredentialInventoryProjection`.

`opsctl doctor`/repository-root detection must stop requiring `operator-contract.json` as CLI semantic sentinel after this cutover.

## 9. N5 — AR-10 runtime semantic-authority retirement

Historical AR-10 acceptance remains evidence. Current ownership:

```text
runtime behavior/safety/launch -> Product Rust
runtime dependency tuple       -> runtime/camouhost/runtime-lock.json
real Camoufox adapter          -> runtime/camouhost/real.py
synthetic fixture              -> runtime/camouhost/main.py test-only
IPC semantic contract          -> bridge-domain + cross-language validation
runtime failure guarantees     -> implementation + tests
required hosted contexts       -> current GitHub governance
production/lifecycle state     -> lifecycle/release owner
```

`runtime-lock.json` remains a legitimate versioned cross-language manifest.

Required:

- reassign every unique current field in `architecture/runtime-cutover-ar10.json`;
- remove inventory/acceptance/governance/release/doctor dependencies;
- preserve real Camoufox and Windows/Profile Bridge regressions;
- do not move runtime execution into `opsctl`;
- delete `runtime-cutover-ar10.json` after zero callers/unique current invariants.

## 10. PF-1 entry contract

PF-1 begins only from protected `main` where F1/F2/N1…N5 are accepted.

Target inputs:

```text
ValidatedProgramSequence
RawArchitectureAcceptanceEvidenceV1
D1InventoryProjection
RuntimeTopologyProjection
ApplicationInventoryProjection
OperatorInventoryProjection
GovernanceInventoryProjection
RuntimeInventoryProjection
CredentialInventoryProjection
ReleaseInventoryProjection
```

Forbidden:

```text
GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet
raw serde_json::Value authority bag
architecture/inventory.json as semantic input
```

Target:

```text
outer Git/GitHub raw observations
-> typed LifecycleEvaluator
-> DerivedLifecycleStateV1
-> bounded typed inventory projections
-> pure ArchitectureInventoryCompiler
-> canonical render/check/inspect/write
-> delete legacy Node lifecycle engine
-> delete legacy Python inventory/projection cluster
```

`opsctl doctor` and repository-root detection are mandatory current callers in the cutover.

## 11. PF-2 entry contract

PF-2 consumes accepted F2/PF-1 foundations:

```text
outer provider/GitHub observation
-> strict versioned DTO
-> typed normalized observation
-> pure EvidencePolicy
-> typed decision/envelope
-> canonical JSON + digest
-> immutable hosted artifact/attestation
```

Network/provider reads, clocks and artifact publication stay outside pure core.

Current Python that combines GitHub API acquisition with semantic evidence decision is transitional; PF-2 splits acquisition from Rust evidence policy.

## 12. PF-3 entry contract

PF-3 fitness semantics are typed Rust:

```text
FitnessRuleRegistry
-> evaluator/enforcement mapping
-> positive/negative fixtures
-> Architecture Fitness Gate
-> optional generated report/index
```

A hand-maintained semantic `architecture/architecture-fitness-policy.json` is forbidden.

Minimum permanent zero/one budgets include:

```text
semantic_owner_count_per_fact = 1
serde_json_value_crossing_into_opsctl_pure_core = 0
filesystem/process/network/provider_in_opsctl_pure_core = 0
runtime_dependency_on_opsctl = 0
global_authority_bag = 0
generated_projection_used_as_semantic_input = 0
opsctl_doctor_process_network_provider_python_child_process = 0
legacy_doctor_sentinel_dependency = 0
unclassified_python_production_or_provider_effect = 0
python_duplicate_semantic_authority = 0
breaking_external_contract_without_version_bump = 0
duplicate_json_member_accepted_in_attestable_contract = 0
```

## 13. Transaction discipline

Each F/N step starts from accepted protected `main` and proves on one exact head:

```text
fresh baseline
complete caller discovery
field/invariant ownership matrix
positive parity
negative anti-regression
old callers = 0
old unique current invariants = 0
exact-head CI/governance green
physical retirement/demotion
post-merge accepted-main reread
```

Do not force all F/N implementation into one giant PR. Order is binding; transaction size remains bounded.

## 14. Non-goals

This normalization does not globally ban Python/JSON, rewrite correct product code for symmetry, move Camoufox into `opsctl`, create Product Runtime↔opsctl RPC, create generic DI/plugin framework, rewrite full credential lifecycle early, enable production or start AR-12.

## 15. PF-1 entry DoD

PF-1 remains blocked until accepted `main` proves:

```text
Release Set breaking shape correctly versioned = true
opsctl adapter/pure-core contract = established
opsctl doctor contract = established
canonical JSON/digest foundation = established
runtime-topology-ar2 current semantic authority = 0
Python estate overlay current authority = 0
Python estate generator current authority = 0
historical AR-7 overlays used as evolving governance = 0
operator-contract JSON used as CLI authorization = 0
AR-8 provenance used as normal semantic input = 0
runtime-cutover-ar10 current semantic authority = 0
runtime Product dependency on opsctl = 0
opsctl runtime/provider/network/process authority = 0
generated projection used as semantic source = 0
unclassified high-risk Python provider/runtime path = 0
```

Accepted functionality/security behavior remains unchanged and production remains fail-closed.
