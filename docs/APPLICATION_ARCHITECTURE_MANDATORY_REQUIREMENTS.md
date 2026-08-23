# Mandatory Application Architecture Requirements

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**opsctl doctor:** `docs/OPSCTL_DOCTOR_CONTRACT.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**Production authorization:** NONE

These are mandatory prospective requirements for every PF/FC/AR/PC change. Accepted AR-0…AR-11 history is not reopened. The product remains one modular application with one protected `main`, one architecture hierarchy, one schema/compatibility lineage and one Release / Capability Profile authority for production admission.

## 0. Prospective architecture precedence

Accepted history preserves required outcomes, durable compatibility obligations and immutable evidence. It does not make historical internal implementation shape a permanent architecture constraint.

Current product/security guarantees and proved durable/external obligations constrain the valid solution space. Subject to those obligations, the current prospective architecture owns internal implementation shape:

```text
current product/security/durable obligations
    -> constrain acceptable solutions
current prospective architecture contract
    -> owns internal architectural shape
current natural semantic owners
-> proved current consumers / external observations
-> accepted historical AR outcomes + evidence/provenance
-> historical internal implementation shape
```

This repository has not yet had a production release. Therefore compatibility with obsolete internal implementation is **not** the default.

```text
proved current/external consumer absent
+ durable/persisted/migration obligation absent
-> compatibility bridge default = NO
```

A retained compatibility path requires a named current consumer or explicit durable contract, an exact version/shape, isolation from the current writer/semantic owner and an explicit retirement condition. Historical acceptance alone is not sufficient. Conversely, a real persisted/wire/external obligation may not be discarded merely because the new internal architecture is cleaner; it must be versioned, migrated or explicitly retired through its owning contract.

An older AR implementation must conform to the current architecture, not the reverse. Still-valid AR guarantees remain mandatory; obsolete JSON/Python/Node/registry/table mechanisms may and should retire once callers and unique current invariants are zero.

## 1. Single semantic owner

Every current semantic fact has exactly one natural owner.

Representations may include DTOs, rows, manifests, generated JSON, CLI output, evidence envelopes or frontend projections, but those representations do not become independent semantic authorities.

Every touched machine artifact is classified as:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

Permanent budgets:

```text
semantic_owner_count_per_fact = 1
generated_projection_used_as_semantic_source = 0
global_authority_bag_count = 0
manual_architecture_semantic_json_authority_without_explicit_exception = 0
```

## 2. Bounded ownership / inward dependencies

```text
domain
  ↑
application/use cases + ports
  ↑
adapters
  ↑
composition roots
```

Provider SDKs, HTTP frameworks, Cloudflare bindings, filesystem/process/network primitives and raw environment access do not enter provider-free domain/application policy.

Do not create global business service layers, universal repositories, generic plugin containers or god-policy crates.

## 3. Pure Core / Effect Shell

Decision, compatibility, lifecycle, evidence, fitness and compilation semantics are deterministic wherever practical.

Effects remain explicit outer concerns:

```text
DatabaseRead
DatabaseWrite
ProviderRead
ProviderWrite
SecretResolve
FilesystemRead
GeneratedProjectionWrite
ProcessExecution
NetworkAccess
DeploymentMutation
RuntimeExecution
```

A pure evaluator receives typed explicit observations and returns typed decisions. It does not discover external state itself.

Time/randomness/cwd/locale/timezone/environment are explicit observations/effects, never hidden pure-core dependencies.

## 4. Observation is not policy

```text
GitHub Actions / official provider tool / owned outer adapter
        ↓
raw observation
        ↓
versioned DTO decode
        ↓
typed normalized observation
        ↓
pure policy
        ↓
typed decision
```

Observers report facts they observed. They do not pre-decide lifecycle/readiness/compatibility/admission/evidence validity owned by pure policy.

## 5. Product Runtime and `opsctl` are separate

Forbidden:

```text
Product Runtime -> opsctl
Product Runtime -> opsctl-core
Product Runtime <-> RPC/gRPC <-> opsctl
opsctl daemon/service
opsctl browser/runtime launcher
opsctl provider/deployment mutation executor
```

A neutral shared pure leaf crate is allowed only when two real independent consumers need exactly the same invariant and the shared-semantic extraction test passes. Sharing is exception, not default.

## 6. Permanent `opsctl` adapter/core invariant

Dependency count is not a quality KPI. Representation/effect separation is.

```text
JSON bytes / filesystem / explicit artifacts
        ↓
adapters
        ↓
versioned external DTOs
        ↓
typed semantic inputs
        ↓
PURE CORE
        ↓
typed semantic results
        ↓
output adapter
```

Forbidden in pure semantic APIs:

```text
serde_json::Value
serde_json::Map
std::fs
std::process
std::net
std::env
Path / PathBuf as semantic identity
provider SDK/client types
GitHub raw response types
Wrangler raw response types
HTTP framework types
```

Permanent invariant:

```text
serde_json::Value crossing adapter -> pure-core = 0
```

A small internal `opsctl-core` crate is preferred when it materially provides compile-time enforcement. It is operator-tool internal and never Product Runtime dependency.

## 7. Permanent `opsctl doctor` invariant

`opsctl doctor` is read-only local diagnostic composition, not a second authority registry or exact-head CI substitute.

Allowed:

```text
FilesystemRead
repository-root resolution
strict local contract decode through owned adapters
bounded diagnostic aggregation
stdout/stderr rendering
```

Forbidden:

```text
ProcessExecution
Python/Node child process
Git/GitHub/provider/network access
SecretResolve
ProviderWrite/DatabaseWrite/DeploymentMutation
RuntimeExecution
GeneratedProjectionWrite
duplicate domain/release/lifecycle/evidence policy
```

Repository-root identity must use durable surviving markers, not generated projections or AR/Python/Node artifacts scheduled for retirement.

`doctor` may aggregate results from bounded semantic owners but must not reimplement their rules or keep a manually duplicated global `AUTHORITIES` catalog.

Detailed contract: `docs/OPSCTL_DOCTOR_CONTRACT.md`.

## 8. Typed contract boundaries

Use typed identities/enums where they prevent real ambiguity, including as applicable:

```text
ReleaseSetId
GitCommitSha
Sha256Digest
SchemaRevision
MigrationRevision
CapabilityProfileId
EvidenceKind
EvidenceSchemaVersion
RepositoryRelativePath
Environment
ReasonCode
```

Command-shell inputs are not semantic inputs. OS paths, raw CLI arguments and raw JSON stay outside pure policy models.

## 9. Representation policy

| Boundary | Representation |
| --- | --- |
| Inside one Rust semantic module | Rust types |
| Genuine shared semantic invariant | narrow neutral pure Rust crate only when justified |
| Genuine independent process boundary | versioned wire protocol; Protobuf/`prost` only when justified |
| Durable evidence/manifests/observations | explicit versioned external contract, normally canonical JSON |
| Generated architecture view | JSON projection |
| D1 evolution | SQL + typed Rust rollout/compatibility semantics |
| Provider deployment config | provider-native configuration |
| Secrets | external owned secret/provider boundary |

JSON is not forbidden. Duplicate internal semantic JSON authority is forbidden.

Protobuf determinism does not by itself provide canonical identity, integrity or provenance.

## 10. Canonical JSON / digest discipline

For logical JSON identity:

```text
typed value -> canonical JSON bytes -> SHA-256
```

For exact artifact identity:

```text
exact file bytes -> SHA-256
```

Every digest declares its byte scope.

Before PF-2 attestable evidence relies on the layer:

- reviewed/pinned SHA-256 implementation;
- one explicit canonical external JSON standard, preferably RFC 8785 JCS where compatible;
- independent canonicalization/hash vectors;
- duplicate-member rejection before canonicalization for release/security/evidence-critical inputs;
- bounded input size/depth/complexity;
- UTF-8 + explicit kind/schema version;
- pretty rendering separated from canonical digest bytes.

## 11. External contract versioning

Persisted/durable/external contracts are explicitly versioned. Breaking shape/meaning changes bump version.

The Release Set change from `d1_evolution_authority_sha256` to `d1_repository_identity_sha256` cannot remain current v2. Historical immutable v2 assets are never rewritten; current successor uses a new version.

Historical compatibility code is not implied by immutable historical evidence. It exists only for a proved current consumer or explicit accepted durable contract under §0.

```text
breaking_external_contract_change_without_version_bump = 0
```

## 12. Machine output / error taxonomy

Pure core returns typed results, not JSON strings.

Keep distinct:

```text
InputIoError
DecodeError
ContractValidationError
SemanticPolicyError
PolicyDecision::Blocked/Unknown/Incompatible
OutputEncodingError
```

Machine JSON outputs are versioned. Stable reason codes are typed internally and rendered at output boundary.

## 13. Release / Capability Profile is sole production admission

```text
source_present != production_enabled
```

No second feature flag, env var, frontend visibility rule, Python helper or operator command independently authorizes production execution. Backend surfaces fail closed before mutation when admission is absent.

## 14. Cutover discipline

```text
new natural owner
-> positive parity
-> negative anti-regression
-> all current callers switched
-> predecessor caller_count = 0
-> predecessor unique_current_invariant_count = 0
-> predecessor deleted/demoted in same accepted transaction
-> provenance preserved in Git/evidence
```

Compatibility is retained only for a proved current consumer or explicit accepted contract. Temporary candidate parity inside one branch is allowed; long-lived dual current implementations on accepted `main` are not.

## 15. Python usage

Python is an implementation language, not an architecture layer.

```text
Python may adapt, observe, generate, test or host a genuine cross-language runtime.
Python must not become a second semantic owner or ungoverned provider mutation authority.
```

The real Camouhost adapter is legitimate Product Runtime outer adapter. Synthetic Camouhost is test-only. Historical Python estate registry/overlays are not permanent current authority.

## 16. PF-1 / PF-2 / PF-3 application and architecture freeze

PF-1 consumes bounded typed projections and explicit raw lifecycle observations. It must not build a global raw authority bag or keep Node/Python semantic predecessors after cutover. `opsctl doctor`/repository-root are mandatory callers in that cutover. N2…N5 must have already resolved their own subject authority ambiguities; PF-1 is not a catch-all cleanup phase.

PF-2 uses the same adapter/core boundary. GitHub/provider reads and clocks remain outer observations; typed Rust `EvidencePolicy` owns validity/freshness/trust semantics.

PF-3 owns fitness semantics in typed Rust `FitnessRuleRegistry` or equivalent. A JSON fitness file may only be generated projection/index.

**PF-3 acceptance is the Architecture Re-baseline v3 architecture-forming freeze point.** It does not authorize production and does not set the lifecycle flag `architecture_complete=true`, but after PF-3 the planned roadmap may no longer introduce new generic architecture layers, global authority frameworks, duplicate lifecycle engines, generic compatibility frameworks or redesign buckets.

Post-PF-3 roles are fixed:

```text
FC-6 / FC-7     functional closure and staging proof
AR-12..AR-15    implementation/rehearsal/delivery on the established architecture
AR-16           audit only
AR-17           qualification/authorization decision only
PC-1+           functional/capability development and production rollout on the architecture
```

If a later audit finds a violation, the violation is corrected under this architecture or through an explicit governed architecture change. AR-16/AR-17 do not become architecture-redesign phases.

Minimum PF-3 zero/one budgets include:

```text
semantic_owner_count_per_fact = 1
serde_json_value_crossing_into_opsctl_pure_core = 0
filesystem/process/network/provider_in_opsctl_pure_core = 0
runtime_dependency_on_opsctl = 0
opsctl_runtime_service_endpoint = 0
global_authority_bag = 0
generated_projection_used_as_semantic_input = 0
opsctl_doctor_process_network_provider_python_child_process = 0
legacy_doctor_sentinel_dependency = 0
breaking_external_contract_without_version_bump = 0
duplicate_json_member_accepted_in_attestable_contract = 0
manual_architecture_semantic_json_authority_without_exception = 0
unclassified_python_production_or_provider_effect = 0
python_duplicate_semantic_authority = 0
```

## 17. Definition of Done for materially changed architecture

Every materially changing bounded candidate proves on one exact head:

1. fresh protected-main/trackers/competing-PR re-baseline;
2. explicit natural owner and bounded context;
3. explicit contracts/effects/observation boundaries;
4. positive + negative tests at lowest sufficient layer;
5. no duplicate authority/global bag/hidden production-enable path;
6. predecessor disposition + zero callers/unique invariants for cutovers;
7. cross-platform semantic equivalence where required;
8. all applicable permanent workflows/protected contexts green;
9. `behind_by=0`, blocking reviews=0, unresolved threads=0;
10. guarded merge bound to exact proven head;
11. accepted-main reread before next transaction.

These rules are mandatory quality constraints, not authorization for a repository-wide aesthetic rewrite. Correct untouched code remains untouched; touched scope converges.
