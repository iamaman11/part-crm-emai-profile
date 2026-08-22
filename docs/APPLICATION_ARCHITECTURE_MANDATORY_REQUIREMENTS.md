# Mandatory Application Architecture Requirements

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**Production authorization:** NONE

This document records mandatory prospective architecture requirements for every PF, FC, AR and PC change after acceptance of this amendment. It does not reopen accepted AR-0…AR-11 history, create another roadmap, or authorize production. It exists so the permanent development rules are explicit and testable rather than implicit conventions.

The product remains one modular application with one protected `main`, one architecture hierarchy, one schema/compatibility lineage and one Release / Capability Profile authority for production admission.

## 1. Single semantic owner

Every semantic fact has exactly one current natural owner.

Allowed representations of one fact may include DTOs, database rows, generated JSON, CLI output, evidence envelopes or frontend projections, but those representations must not become independent semantic authorities.

For every touched machine artifact classify it as exactly one of:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

A `TRANSITIONAL_SEMANTIC_SOURCE` must have an explicit retirement disposition. Generated projections and historical evidence must never be read back as semantic authority for the facts they project or prove.

Permanent target budgets:

```text
semantic_owner_count_per_fact = 1
generated_projection_used_as_semantic_source = 0
global_authority_bag_count = 0
manual_architecture_semantic_json_authority_without_explicit_exception = 0
```

## 2. Bounded ownership and inward dependencies

Business semantics belong to bounded contexts and their application layers. Adapters, transport and composition remain outside.

```text
domain
  ↑
application / use cases + ports
  ↑
adapters
  ↑
composition roots
```

Provider SDKs, HTTP frameworks, Cloudflare bindings, filesystem/process/network primitives and raw environment access do not enter provider-free domain/application policy.

Do not create a global service layer, universal repository, generic plugin container or god policy crate.

## 3. Pure Core / Effect Shell

Decision, compatibility, lifecycle, evidence, fitness and compilation logic must be deterministic wherever practical.

Effects are explicit and stay at outer boundaries. Relevant effect classes include:

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

A pure evaluator receives typed values and returns typed decisions. It does not discover its own external state.

Time, randomness, current working directory, locale, timezone and environment variables are observations/effects, not hidden pure-core dependencies. Time-sensitive policy receives an explicit typed observed time.

## 4. Observation is not policy

External-state acquisition and semantic decision making are distinct stages:

```text
GitHub Actions / official provider tool / owned outer adapter
        ↓
raw observation
        ↓
versioned DTO decode + validation
        ↓
typed normalized observation
        ↓
pure policy
        ↓
typed decision
```

An observer may report facts it actually observed. It must not pre-decide lifecycle, readiness, compatibility, admission or evidence validity that belongs to the semantic owner.

A pure policy module must not gain GitHub, Cloudflare, Microsoft, Google, filesystem, subprocess or network authority merely to fetch inputs conveniently.

## 5. Product Runtime and opsctl are separate systems

`tools/opsctl` is a standalone offline operator/policy/verification tool. Product Runtime must never depend on it directly or through RPC.

Forbidden:

```text
Product Runtime -> opsctl
Product Runtime <-> RPC/gRPC <-> opsctl
opsctl as daemon/service
opsctl as browser/runtime launcher
opsctl as provider/deployment mutation executor
```

If Product Runtime and `opsctl` genuinely require exactly the same semantic invariant, a small neutral pure leaf crate is allowed only after the shared-semantic extraction test in `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md` passes. Sharing is the exception, not the default.

## 6. Permanent opsctl adapter/core invariant

The dependency count of `opsctl` is not a quality KPI. Its representation/effect boundary is.

Required flow:

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
output DTO / canonical representation
```

The following must not cross into `opsctl` pure semantic APIs:

```text
serde_json::Value
serde_json::Map
std::fs
std::process
std::net
std::env
Path / PathBuf as semantic identity
provider SDK/client types
GitHub API response types
Wrangler raw response types
HTTP framework types
```

Permanent invariant:

```text
serde_json::Value crossing adapter -> pure-core boundary = 0
```

A small internal `opsctl-core` crate is recommended when it materially strengthens compile-time enforcement. It remains operator-tool internal and is never a Product Runtime dependency.

## 7. Typed contract boundaries

Critical identities and lifecycle concepts use types/enums when doing so prevents invalid combinations. Examples include:

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

Do not turn every string into a wrapper without benefit. The criterion is prevention of a real class of error.

Command-shell inputs are not semantic inputs. OS paths, CLI arguments and raw JSON stay outside pure policy models.

## 8. Representation policy

Use the representation that matches the boundary:

| Boundary | Canonical representation |
|---|---|
| Inside one Rust semantic module | Rust types; no serialization object graph |
| Genuine shared semantic invariant | narrow neutral pure Rust crate, only when justified |
| Genuine independent process boundary | versioned wire protocol; Protobuf/`prost` may be used when justified |
| Durable evidence/manifests/observations | explicit versioned external contract, normally canonical JSON |
| Generated architecture view | JSON projection |
| D1 evolution | SQL migrations + typed Rust compatibility/rollout semantics |
| Provider deployment configuration | provider-native configuration |
| Secrets | owned external secret/provider boundary |

JSON is not forbidden. JSON used as a duplicate internal semantic authority is forbidden.

Protobuf is not a canonical digest/signature format. Serialization determinism does not by itself provide integrity or provenance.

## 9. Canonical JSON and digest discipline

For external documents whose logical content identity must survive key order/whitespace differences:

```text
typed value
  ↓
explicit canonical JSON scheme
  ↓
canonical bytes
  ↓
SHA-256
```

For artifacts whose exact checked-in bytes are intentionally the identity:

```text
exact file bytes
  ↓
SHA-256
```

Every digest field must define which of those scopes it uses.

Before PF-2 attestable evidence relies on the canonical layer:

- replace bespoke handwritten SHA-256 with a reviewed/pinned implementation unless a documented necessity proves otherwise;
- define one canonical external-JSON standard, preferably RFC 8785 JCS when compatible with the contract;
- validate independent canonicalization/hash vectors;
- reject duplicate JSON object member names before semantic evaluation/canonicalization for release/security/evidence-critical inputs;
- bound input size and parser depth/complexity;
- require UTF-8 and explicit schema version/kind;
- keep pretty/human rendering separate from canonical digest bytes.

## 10. External contract versioning

Persisted/durable/external contracts are explicitly versioned. A breaking shape or meaning change requires a new schema version.

The already-observed Release Set change from:

```text
schemas.d1_evolution_authority_sha256
```

to:

```text
schemas.d1_repository_identity_sha256
```

is a breaking contract change and must not remain under Release Set v2. The current successor must use a new version (target v3 unless exact-candidate evidence proves another valid bounded version decision). Historical immutable v2 artifacts are not rewritten.

Permanent invariant:

```text
breaking_external_contract_change_without_version_bump = 0
```

## 11. Machine output and error taxonomy

Pure core returns typed results, not JSON strings. Output adapters map typed results to stable machine and human representations.

Keep at least these classes distinct:

```text
InputIoError
DecodeError
ContractValidationError
SemanticPolicyError
PolicyDecision::Blocked/Unknown/Incompatible
OutputEncodingError
```

A semantic `BLOCKED`, `UNKNOWN` or `INCOMPATIBLE` decision is not collapsed into an infrastructure failure.

Machine JSON outputs have explicit schema versions. Stable reason codes are typed internally and rendered as strings at the output boundary.

## 12. Release / Capability Profile is sole production admission authority

`source_present != production_enabled` remains binding.

No second feature flag, environment variable, frontend visibility rule, Python helper or operator command may independently authorize production execution. Backend execution surfaces fail closed before mutation when capability admission is absent.

## 13. Cutover discipline

Every authority or implementation replacement follows:

```text
new natural owner implemented
-> positive parity proof
-> negative anti-regression proof
-> all current callers switched
-> predecessor caller_count = 0
-> predecessor unique_current_invariant_count = 0
-> predecessor deleted/demoted in same accepted transaction
-> provenance preserved in Git/evidence
```

Do not retain compatibility before production without a proved current consumer or explicit accepted compatibility contract.

## 14. Python usage

Python is an implementation language, not an architecture layer. It is allowed only in roles defined by `docs/PYTHON_USAGE_BOUNDARY.md`.

The high-level rule is:

```text
Python may adapt, observe, generate, test or host a genuine cross-language runtime.
Python must not become a second owner of Product/opsctl semantics or an ungoverned provider mutation authority.
```

The real Camouhost Python adapter is a legitimate Product Runtime outer adapter. Synthetic Camouhost is test-only. Historical Python estate inventory is not permanent current authority.

## 15. PF-1 / PF-2 / PF-3 application

PF-1 must consume bounded typed projections and explicit lifecycle observations. It must not build a global raw authority bag or keep legacy Node/Python semantic predecessors after cutover.

PF-2 must use the same adapter/core boundary for hosted evidence. GitHub/provider reads and clocks remain outer observations; `EvidencePolicy` is pure. Durable evidence uses the accepted versioned canonical contract and digest primitive.

PF-3 must make these rules persistent. Fitness-rule semantics belong to a typed Rust `FitnessRuleRegistry` (or equivalent bounded typed owner). A JSON fitness catalog may exist only as generated projection/index, never as a second manually maintained semantic authority.

PF-3 must mechanically enforce at least:

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
unclassified_python_production_or_provider_effect = 0
```

## 16. Definition of Done for materially changed architecture

Every materially changing bounded candidate must prove on its exact head:

1. fresh protected-main re-baseline and competing PR review;
2. explicit natural semantic owner and bounded context;
3. explicit contracts and external effects;
4. positive and negative tests at the lowest sufficient layer;
5. no unexplained duplicate authority;
6. no hidden production-enable or provider-mutation path;
7. predecessor disposition and zero-caller proof where a cutover occurs;
8. platform-equivalent semantic result where cross-platform support applies;
9. all applicable permanent workflows green;
10. behind-by zero, blocking reviews zero and unresolved threads zero before guarded merge;
11. post-merge reread of protected `main` before the next bounded transaction.

These requirements are mandatory quality constraints. They are not authorization for a repository-wide rewrite: untouched correct code remains untouched; touched scope converges.