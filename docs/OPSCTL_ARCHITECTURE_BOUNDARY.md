# opsctl Architecture Boundary

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Mandatory application requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**PF-1 detailed specification:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**Production authorization:** NONE

This contract defines the permanent role and internal architecture boundary of standalone Rust `tools/opsctl`. It does not make `opsctl` Product Runtime, a daemon, provider client, deployment executor, hidden state store or shared application service.

The key quality target is **not dependency count**. It is that representation and effects stop at adapters and do not leak into pure operational semantics.

## 1. Permanent role

Allowed roles:

```text
inspect
validate
verify
plan
compatibility evaluation
lifecycle/acceptance evaluation over explicit observations
architecture inventory compilation
fitness evaluation
canonical external-contract rendering
```

Forbidden roles:

```text
product runtime
background daemon
RPC/server endpoint
provider mutation executor
GitHub/Cloudflare/Microsoft/Google network client
secret resolver
browser/Camouhost/Camoufox launcher
deployment executor
hidden local database/state backend
product application dependency
```

Outer GitHub Actions, official provider tools or explicitly owned adapters collect external observations. `opsctl` evaluates explicit bytes/files/stdin artifacts locally.

## 2. Required internal shape

```text
CLI / composition root
        ↓
input adapters
        ↓
versioned external DTO decode
        ↓
typed semantic inputs
        ↓
PURE CORE
        ↓
typed semantic result
        ↓
output DTO
        ↓
canonical JSON / human rendering
```

Conceptually:

```text
tools/opsctl
├── cli + composition
├── adapters
│   ├── filesystem
│   ├── JSON decode/encode
│   ├── canonical JSON
│   └── local artifact hashing
├── contracts
│   └── versioned external DTOs + conversion
└── core
    ├── architecture
    ├── d1
    ├── release
    ├── promotion
    ├── evidence
    ├── fitness
    └── typed semantic models
```

Physical paths may differ. Dependency direction may not.

## 3. Internal `opsctl-core` boundary

F2 introduced the small internal `tools/opsctl/core` (`opsctl-core`) boundary because Release Set semantics already required a real compile-time separation. This is accepted architecture, not a requirement to move every `opsctl` module into that crate immediately.

Permanent direction:

```text
opsctl shell/adapters -> opsctl-core where a real pure semantic owner belongs there
opsctl-core -X-> filesystem/network/process/provider/serde_json::Value
Product Runtime -X-> opsctl-core
```

New modules move into or remain outside `opsctl-core` according to the effect/representation boundary, not for layering aesthetics. The core crate may use narrowly reviewed pure dependencies where needed. Zero dependencies is not a goal; zero hidden effects/representation leakage is.

## 4. Hard pure-core invariant

The following must not enter or appear in pure semantic APIs:

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

Canonical budget:

```text
serde_json::Value crossing adapter -> pure-core boundary = 0
```

Correct:

```rust
fn evaluate(
    observation: D1LedgerObservation,
    policy: &D1Policy,
) -> Decision
```

Incorrect:

```rust
fn evaluate(value: serde_json::Value) -> serde_json::Value
```

Filesystem paths belong to the shell/adapter layer. A semantic repository-relative identity is represented by a typed normalized value, not an OS handle.

Clock/time-sensitive policy receives an explicit typed observation; pure policy does not call `SystemTime::now()`. The same applies to randomness, current working directory, locale, timezone and environment.

## 5. JSON and DTO boundary

JSON is legitimate at external boundaries; it is not the internal semantic object graph.

Required flow:

```text
bounded UTF-8 bytes
   ↓
strict JSON decoder
   ↓
versioned DTO
   ↓
validation + conversion
   ↓
typed core model
```

For release/security/evidence-critical JSON:

- duplicate object member names fail closed before canonicalization;
- unknown fields fail closed unless the contract defines an extension point;
- input byte size and parser depth/complexity are bounded;
- breaking shape/meaning changes bump schema version.

`serde_json::Value` may be used inside narrowly scoped decode/canonicalization adapters. It must not be retained in semantic structs, stored as semantic identity payload, passed to compatibility/decision logic, or returned by core APIs.

Using `serde` derives on external DTOs is allowed. Core models should remain representation-independent where that materially reduces coupling.

## 6. Current audited convergence examples

### D1 — reference direction

Current D1 already demonstrates the desired split substantially:

```text
d1/authority.rs
  filesystem + external decode
        ↓
typed ReleaseSchemaContract / Preconditions / ledger observation
        ↓
d1/plan.rs::evaluate(...)
  typed deterministic policy
```

Remaining command-shell `Path` fields are adapter/orchestration inputs and must not be mistaken for semantic model types.

### Release — accepted F1/F2 reference direction

F1/F2 moved the current Release Set writer/model to a typed v3 pure-core boundary. Generic JSON/canonical bytes remain adapter concerns; current semantic release shape is not owned by `serde_json::Value`.

Historical v2 decoding is isolated from the current writer/model and is legitimate only while a concrete current historical consumer/durable obligation is proved. The bounded pre-N2 cleanup gate in `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md` decides whether that executable compatibility remains necessary; historical evidence by itself is not sufficient.

### Promotion — convergence debt

Current DeploymentSnapshot loading still combines filesystem read, generic JSON parse and typed semantic construction in touched paths. Promotion code should converge, when its owning work touches it, to:

```text
filesystem adapter
-> DeploymentSnapshotVnDto
-> typed DeploymentObservation
-> pure promotion/preflight policy
```

This is touch-to-converge debt, not authorization for an unrelated repository-wide rewrite.

### Repository root discovery — convergence debt

Current repository-root discovery depends on transitional AR-6/AR-8/Python sentinels. N2/N4/PF-1 must remove those sentinels so repository identity does not depend on files scheduled for retirement.

## 7. Command pipeline

Every non-trivial command has five conceptual stages:

```text
1. parse CLI
2. acquire allowed local inputs/effects
3. decode/validate into typed semantic inputs
4. call pure semantic operation
5. render typed result
```

Command composition may orchestrate those stages. It is not the semantic owner of rules belonging to a core module.

Pure-core tests receive only in-memory typed inputs and deterministic expected outputs. They do not require a fixture repository, filesystem, environment, subprocess or network.

## 8. Explicit effect capability set

Default allowed `opsctl` shell effects:

```text
FilesystemRead
Stdout/Stderr presentation
GeneratedProjectionWrite  # only explicitly bounded ownership such as PF-1 inventory
```

Forbidden without a future accepted architecture change:

```text
ProcessExecution
NetworkAccess
ProviderRead
ProviderWrite
DatabaseWrite
DeploymentMutation
SecretResolve
RuntimeExecution
```

`GeneratedProjectionWrite` is not general filesystem mutation. PF-1 may own the bounded write to `architecture/inventory.json`; workflow infrastructure owns hosted artifact publication/attestation.

Provider/GitHub observations are gathered externally and passed in as versioned data.

## 9. Python relationship

`opsctl` must not call Python for policy, validation, observation or mutation.

Forbidden:

```text
opsctl -> Python validator
opsctl -> Python generator -> semantic decision
opsctl -> Python provider observer
opsctl -> Python mutation executor
```

This does not forbid developer helpers from calling `opsctl`, and it does not forbid Python outer observers executed by workflows before `opsctl` is invoked.

The permanent direction is one-way:

```text
workflow / developer shell / Python observer
        ↓
explicit versioned data or CLI call
        ↓
opsctl
```

Never `opsctl -> Python` for semantic work.

## 10. Typed identities and results

Critical semantic strings should converge to newtypes/enums when this prevents real ambiguity, including as applicable:

```text
ReleaseSetId
GitCommitSha
Sha256Digest
SchemaRevision
MigrationRevision
CapabilityProfileId
RepositoryRelativePath
EvidenceKind
EvidenceSchemaVersion
Environment
ReasonCode
```

Core decisions/reason codes are typed; output adapters render string codes.

Command-shell request types containing `Path`/CLI data are explicitly adapters/orchestration, not pure semantic models.

## 11. Error taxonomy and machine output

Keep these layers distinct:

```text
InputIoError
DecodeError
ContractValidationError
SemanticPolicyError
PolicyDecision::Blocked/Unknown/Incompatible
OutputEncodingError
```

A blocked/unknown/incompatible semantic result is not collapsed into an input/I/O error.

Pure core returns typed results. JSON strings are output-adapter products. Machine output contracts are explicitly versioned and stable by command contract.

## 12. Canonical JSON and digest discipline

F2 established the current canonical external JSON/digest foundation in the `opsctl` adapter layer:

- pinned reviewed SHA-256 implementation (`sha2`);
- RFC 8785/JCS canonicalization through a pinned canonicalizer;
- duplicate-member rejection before canonicalization for strict inputs;
- bounded JSON byte/depth parsing;
- canonical bytes separated from pretty rendering;
- independent digest/canonicalization vectors/tests.

This layer is an **adapter/contract foundation**, not a semantic business/release authority. `serde_json::Value` remains confined to strict decoding/canonicalization adapters and does not cross into pure semantic models.

Two digest scopes remain explicit:

```text
semantic JSON identity -> canonical semantic bytes -> SHA-256
exact artifact identity -> exact file bytes -> SHA-256
```

Never hash Protobuf serialized bytes as a supposed universal canonical identity.

Before PF-2 attestable evidence depends on additional contract kinds, those kinds must reuse this foundation and add their own closed/versioned DTO validation rather than invent another canonicalization/hash mechanism.

## 13. Release Set version discipline

A breaking external-contract change never retains the same schema version.

The accepted current Release Set writer/model is v3 for the `d1_repository_identity_sha256` semantics. Historical v2 assets are immutable evidence/possible compatibility inputs and are never rewritten into v3.

Historical v2 reader compatibility is isolated from current writer semantics and kept only for a proved current consumer/durable obligation. Before N2 starts, the bounded F1 cleanup gate must prove such a consumer or retire executable v2 compatibility and remove `architecture/release-set-v2.json` from any current-authority role.

## 14. Shared semantic crate extraction test

Default is no Product Runtime / `opsctl` semantic sharing.

A neutral shared semantic crate is allowed only when all are true:

1. at least two real independent consumers exist;
2. both require exactly the same invariant;
3. without one owner a real duplicate semantic authority would exist;
4. the crate is pure and narrow;
5. it depends on neither consumer;
6. it has no filesystem/network/provider/process/runtime effects;
7. it cannot become a generic `common`/service/policy god crate.

Forbidden:

```text
Product Runtime -> opsctl
Product Runtime <-> RPC/gRPC <-> opsctl
```

A genuine independent process boundary may separately justify a versioned wire protocol such as Protobuf/`prost`.

## 15. Inventory boundary

PF-1 inventory compilation receives closed bounded projections, not raw authority documents:

```text
ValidatedInventoryInputs {
  lifecycle,
  d1,
  runtime_topology,
  application,
  operator,
  governance,
  runtime,
  credentials,
  release,
}
```

Each bounded owner validates its own semantics and exports only the inventory facts needed by the compiler.

Forbidden:

```text
GlobalRepositoryAuthorityLoader
GlobalAuthoritySet
serde_json::Value authority bag
inventory.json as semantic input
inventory compiler reimplementing D1/release/runtime policy
manual AR-qualified application ownership registry as compiler input
```

## 16. PF-2 evidence boundary

PF-2 follows the same model:

```text
raw hosted/provider observation bytes
        ↓
external DTO decode
        ↓
typed normalized observation
        ↓
pure EvidencePolicy
        ↓
typed EvidenceDecision / envelope data
        ↓
canonical JSON adapter
        ↓
SHA-256 / hosted artifact attestation
```

GitHub/provider reads and clocks remain outer effects. Freshness/replay decisions receive explicit typed observations.

## 17. PF-3 enforcement

PF-3 must not make a hand-maintained JSON catalog the semantic fitness owner.

Target:

```text
typed Rust FitnessRuleRegistry
        ↓
fitness evaluator / enforcement mapping
        ↓
optional generated machine projection/report
```

Minimum zero budgets include:

```text
serde_json_value_crossing_into_pure_core
filesystem_import_in_pure_core
process_execution_in_opsctl
network_access_in_opsctl
provider_sdk_dependency_in_opsctl
runtime_dependency_on_opsctl
opsctl_runtime_service_endpoint
generated_projection_used_as_semantic_input
global_authority_bag
breaking_external_contract_change_without_version_bump
unversioned_durable_external_contract
duplicate_json_member_accepted_in_attestable_contract
manual_architecture_semantic_json_authority_without_explicit_exception
manual_AR_qualified_application_ownership_registry_current_authority
compatibility_shim_without_proved_consumer_or_durable_obligation
opsctl_python_child_process
```

PF-3 also enforces one semantic owner per fact and one mutation executor per owned mutation operation. Accepted PF-3 is the architecture-forming freeze point described by `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`.

## 18. Definition of Done

Before PF-1 acceptance, the exact candidate must prove:

```text
serde_json::Value in pure-core semantic APIs = 0
filesystem/process/network/provider effects in pure core = 0
product runtime -> opsctl dependency = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
pure-policy tests requiring filesystem/network/process = 0
external durable contracts explicitly versioned = true
breaking contract changes version-bumped = true
canonical digest layer has independent vectors = true
security/release/evidence duplicate-key ambiguity rejected = true
inventory compiler consumes bounded typed projections = true
manual AR-qualified application ownership registry current authority = 0
operator command/effect semantic authority is Rust-owned = true
legacy Node/Python lifecycle/inventory predecessors retired = true
```

Developer mental model:

```text
Adapters read/observe/encode.
Contracts transport/version data.
Core decides.
Composition wires.
Workflows/official tools perform hosted/provider effects.
Product Runtime never depends on opsctl.
```