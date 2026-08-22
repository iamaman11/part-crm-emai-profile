# opsctl Architecture Boundary

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**PF-1 detailed specification:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**Production authorization:** NONE  
**AR-12 implementation:** NOT AUTHORIZED by this document

This contract defines the permanent internal boundary of the standalone Rust operator tool `tools/opsctl`. It does not make `opsctl` a product runtime, daemon, provider client, deployment engine, hidden state store, or shared application service.

The key invariant is not the number of Cargo dependencies. The key invariant is that representation and effects stop at adapters and do not leak into pure operational-policy semantics.

## 1. Permanent role

`opsctl` is a project-specific offline policy, verification, planning and projection tool.

Allowed roles include:

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

Forbidden roles include:

```text
product runtime
background daemon
RPC/server endpoint
provider mutation executor
GitHub/Cloudflare/Microsoft/Google network client
secret resolver
browser launcher
Camouhost/Camoufox launcher
deployment executor
hidden local database/state backend
product application dependency
```

GitHub Actions / official provider tools / product-owned adapters collect external observations. `opsctl` receives explicit bytes/files/stdin artifacts and evaluates them locally.

## 2. Required internal shape

Target dependency direction:

```text
CLI / composition root
        ↓
input/output adapters
        ↓
versioned contract DTO decode/encode
        ↓
typed semantic inputs
        ↓
PURE CORE
        ↓
typed semantic result
        ↓
output contract DTO
        ↓
canonical JSON / human rendering
```

The core never depends upward on adapters.

Conceptually:

```text
tools/opsctl
├── core
│   ├── architecture
│   ├── d1
│   ├── release
│   ├── promotion
│   ├── evidence
│   ├── fitness
│   └── typed semantic models
│
├── contracts
│   ├── versioned JSON DTOs
│   └── conversion to/from core types
│
├── adapters
│   ├── filesystem
│   ├── JSON decode/encode
│   ├── canonical JSON
│   └── local artifact hashing
│
└── cli / composition
```

Physical paths may differ. The dependency direction and effect boundary may not.

A small internal `opsctl-core` crate is explicitly allowed if it materially improves compile-time enforcement. It is not a shared product semantic crate and must not be consumed by Product Runtime. Do not create multiple crates merely for aesthetics; if a separate core crate is introduced, it exists to enforce the boundary described here.

## 3. Hard pure-core invariant

The pure core must not import or expose representation/effect types.

At minimum the following are forbidden across the adapter -> pure-core boundary:

```text
serde_json::Value
serde_json::Map
std::fs
std::process
std::net
std::env
Path / PathBuf as semantic identities
provider SDK/client types
GitHub API response types
Wrangler raw JSON types
HTTP framework types
```

The canonical rule is:

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

Filesystem paths belong to the shell/adapter layer. When the core needs repository identity it receives a typed normalized semantic value such as `RepositoryRelativePath`, not an OS filesystem handle.

Clock/time-dependent policy receives an explicit typed time observation; it must not call `SystemTime::now()` inside the pure evaluator.

## 4. JSON boundary

JSON is legitimate at external boundaries. It is not legitimate as an internal semantic object graph.

Required flow:

```text
bounded bytes
   ↓
strict JSON decoder
   ↓
versioned DTO
   ↓
validation/conversion
   ↓
typed core model
   ↓
pure policy
```

Versioned durable contracts should use explicit typed DTOs with closed schemas. Unknown fields fail closed unless the contract explicitly defines an extension point. Breaking shape/meaning changes require a schema-version bump.

For security/release/evidence-critical JSON, duplicate object member names must be rejected before semantic evaluation/canonicalization. Inputs are UTF-8 and size-bounded; parser depth/complexity must remain bounded. A JSON parser silently applying last-key-wins semantics is not an accepted security boundary.

`serde_json::Value` is allowed inside a narrowly-scoped JSON/canonicalization adapter when necessary. It must not be retained in semantic structs, stored as `identity_payload`, passed to compatibility/decision logic, or returned from core APIs.

## 5. Current audited examples

The current D1 implementation already demonstrates the desired split partially:

```text
d1/authority.rs
  filesystem + serde_json::Value adapter
        ↓
typed ReleaseSchemaContract / Preconditions / ledger names
        ↓
d1/plan.rs::evaluate(...)
  typed deterministic policy
```

This pattern is accepted as directionally correct, but the final architecture must make the boundary explicit and reusable rather than relying on module convention.

Known current convergence debt includes:

- `release/model.rs` parses generic `serde_json::Value` and retains an `identity_payload: Value` inside `ReleaseSetManifest`;
- `promotion/snapshot.rs` combines filesystem loading, generic JSON parsing and typed semantic snapshot construction in one module/type;
- `repository.rs` owns filesystem effects and currently uses transitional AR-6/AR-8/Python sentinels to decide repository-root identity;
- `lib.rs::execute` is an acceptable composition root but individual command `run` functions must not collapse adapter loading, semantic policy and output serialization into one untestable layer;
- current operator registry tests still treat `architecture/operator-contract.json` as command authority; bounded AR-8 normalization reverses that ownership.

These are touched-scope convergence targets, not authorization for an unrelated repository-wide rewrite.

## 6. Command pipeline

Every non-trivial command should have an explicit five-stage shape:

```text
1. parse CLI
2. acquire allowed local inputs/effects
3. decode/validate into typed inputs
4. call pure semantic operation
5. render typed result
```

A command-specific composition function may orchestrate those stages. It must not become the semantic owner of rules that belong to a bounded core module.

Example:

```text
opsctl d1 plan
  -> filesystem adapter reads ledger/release/precondition bytes
  -> JSON adapters decode typed observations/contracts
  -> D1 repository adapter observes SQL repository state
  -> pure D1 evaluator returns typed Evaluation
  -> output adapter renders versioned JSON
```

The output JSON is a representation of the decision, not the decision authority.

## 7. Explicit effects

The default effect capability set of `opsctl` is intentionally small.

Allowed current effect classes:

```text
FilesystemRead
GeneratedProjectionWrite   # only where explicitly owned, e.g. PF-1 inventory write
Stdout/Stderr presentation
```

Forbidden without a future explicit architecture change:

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

Provider/GitHub read observations are gathered by outer workflows/tools and supplied as versioned data. `opsctl` does not acquire provider credentials merely to make observation collection convenient.

`GeneratedProjectionWrite` is not general filesystem mutation. PF-1 may own exactly the bounded write to `architecture/inventory.json`. PF-2 evidence should normally render to stdout/explicit artifact bytes while the workflow owns artifact publication/attestation.

## 8. Typed identities and error/result model

Critical semantic strings should converge to newtypes/enums where doing so prevents confusion or invalid combinations, including as applicable:

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
ReasonCode enums
```

Core decisions and reason codes are typed. String formatting is an output-adapter concern.

Errors remain layered:

```text
InputIoError
DecodeError
ContractValidationError
SemanticPolicyError
CompatibilityDecision / blocked result
OutputEncodingError
```

A policy-level `BLOCKED`, `UNKNOWN`, or `INCOMPATIBLE` result is not silently collapsed into an infrastructure parsing error.

## 9. Contract serialization and digest discipline

The current handwritten SHA-256 and project-specific JSON key-sorting implementation are transitional implementation details, not permanent cryptographic/canonicalization authority.

Before PF-2 attestable evidence relies on this layer:

- use a reviewed/pinned SHA-256 implementation rather than maintaining bespoke cryptographic code unless a documented necessity exists;
- define one canonical external JSON scheme for digest/signature identity, preferably RFC 8785 JCS if all contract constraints are satisfied;
- keep human pretty rendering separate from canonical digest bytes;
- validate against published canonicalization/hash test vectors;
- prohibit floating dependency versions in security/release-critical tooling;
- commit and verify the lockfile and run existing supply-chain/license/security gates.

The dependency count is not a quality KPI. Adding a narrowly-scoped, audited pure dependency can reduce risk compared with handwritten cryptography or canonicalization.

As of the current planning review, suitable candidates include pinned `serde` for typed DTO derives, RustCrypto `sha2` for SHA-256 and an audited RFC-8785-compatible canonicalizer. Final dependency choice requires repository supply-chain review and exact-version pinning; this document does not authorize blind dependency addition.

## 10. Release Set version discipline

A breaking external-contract change must never retain the same schema version.

The already-audited change from:

```text
schemas.d1_evolution_authority_sha256
```

to:

```text
schemas.d1_repository_identity_sha256
```

changes Release Set v2 meaning/shape and therefore requires a new current Release Set contract version before subsequent PF work relies on it. Historical immutable v2 artifacts remain historical evidence and must not be rewritten.

The exact historical-v2 reader disposition is chosen from real current consumers: retain a bounded historical decoder only if a current verification/rehearsal requirement needs it; otherwise do not keep compatibility code by default before production.

## 11. Shared semantic crate rule

Default is no Product Runtime / `opsctl` sharing.

A neutral shared semantic crate is allowed only when all are true:

1. at least two real independent consumers exist;
2. both need exactly the same semantic invariant;
3. without one owner a real duplicate semantic authority would exist;
4. the crate is pure and narrow;
5. it depends on neither consumer;
6. it has no filesystem/network/provider/process/runtime effects;
7. it does not become a generic `common`/policy/service god crate.

Never use RPC/protobuf merely to share semantics between Product Runtime and `opsctl`.

```text
Product Runtime -> opsctl            FORBIDDEN
Product Runtime <-> RPC <-> opsctl   FORBIDDEN
```

For a genuine independently-versioned process boundary, a wire protocol such as Protobuf may be evaluated separately. That does not make Protobuf a canonical evidence digest format.

## 12. Inventory boundary

PF-1 inventory compilation receives closed bounded projections, not a global raw authority bag.

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

Each bounded owner validates its own full semantics and exposes only the inventory facts required by the compiler.

Forbidden:

```text
GlobalRepositoryAuthorityLoader
GlobalAuthoritySet
serde_json::Value authority bag
inventory.json used as semantic input
inventory compiler reimplementing D1/release/runtime policy
```

## 13. PF-2 evidence boundary

PF-2 must follow exactly the same shape:

```text
raw hosted/provider observation bytes
        ↓
adapter DTO decode
        ↓
typed normalized observation
        ↓
pure EvidencePolicy
        ↓
typed EvidenceDecision / EvidenceEnvelope data
        ↓
canonical JSON adapter
        ↓
SHA-256 / artifact attestation
```

Freshness/replay decisions receive explicit typed time/run/subject observations. The pure core does not query GitHub or the clock.

`VALID`, `VALID_BUT_STALE`, and `INVALID` remain semantically distinct from mutation admission.

## 14. PF-3 enforcement model

PF-3 must not reintroduce a manually maintained semantic JSON policy authority.

Target:

```text
typed Rust FitnessRuleRegistry
        ↓
fitness evaluator / enforcement mapping
        ↓
optional generated machine projection/report
```

If `architecture/architecture-fitness-policy.json` is retained, it is generated projection/index data, not the owner of rule semantics.

At minimum PF-3 must mechanically enforce zero for:

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
```

It must also enforce one semantic owner per fact and one mutation executor per owned mutation operation.

## 15. Testing model

Pure-core tests use only in-memory typed inputs and deterministic expected outputs. They do not require a fixture repository, filesystem, environment variables, subprocesses or network.

Adapter tests own:

```text
JSON malformed/unknown/duplicate fields
size bounds
path normalization / symlink / regular-file policy
cross-platform path handling
canonicalization vectors
hash vectors
filesystem failures
```

End-to-end CLI tests prove composition only after lower layers are covered.

Linux and Windows must produce the same semantic decision and canonical bytes for the same versioned input.

## 16. Definition of Done for the boundary

Before PF-1 is accepted, the exact candidate must prove:

```text
serde_json::Value in pure-core public/private semantic APIs = 0
filesystem/process/network/provider effects in pure core = 0
product runtime -> opsctl dependency = 0
opsctl provider/network/process authority = 0
core tests requiring filesystem = 0 for pure policy modules
external durable contracts have explicit versions
breaking contract changes bump versions
canonical digest implementation has independent vectors
JSON duplicate-key ambiguity is rejected where security/attestation critical
inventory compiler consumes bounded typed projections
operator command/effect authority is Rust-owned
legacy Node/Python policy/projection predecessors are retired per PF-1
```

The desired developer mental model is simple:

```text
Adapters observe/read/encode.
Contracts transport/version data.
Core decides.
Composition wires.
Workflows perform hosted/provider effects.
Product Runtime never depends on opsctl.
```
