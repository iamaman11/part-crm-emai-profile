# Architecture Evolution Quality Contract

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Mandatory application requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**opsctl doctor boundary:** `docs/OPSCTL_DOCTOR_CONTRACT.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**PF-1:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**PF-3:** `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`  
**Production authorization:** NONE

This contract records permanent prospective architecture/development rules. It is not a second roadmap, lifecycle authority, capability registry, architecture inventory or generic framework specification.

Accepted AR-0…AR-11 history remains immutable. Future work preserves correct accepted behavior while converging touched current scope to this contract.

```text
source_present != production_enabled
```

## 0. Precedence and compatibility default

Current product/security guarantees and proved durable/external obligations constrain the valid solution space. Subject to those obligations, current prospective architecture owns the internal implementation shape.

```text
current product/security/durable obligations
    -> constrain acceptable solutions
current prospective architecture
    -> owns internal architecture shape
natural semantic owners
-> current consumers / external observations
-> historical outcomes + immutable evidence
-> historical internal implementation shape
```

Historical implementation is not a compatibility contract by itself.

```text
proved current/external consumer = 0
AND durable/persisted/migration obligation = 0
-> compatibility bridge default = NO
```

A compatibility path may remain only for a named consumer/obligation, exact version/shape, isolated reader/adapter semantics and an explicit retirement condition. A real persisted/wire/external obligation cannot be discarded merely because a cleaner internal architecture exists; it must be versioned, migrated or explicitly retired through its owner.

## 1. Target architecture

```text
natural canonical owner
        ↓
typed policy / contracts
        ↓
bounded-context domain + application
        ↓
ports / adapters / explicit effect capabilities
        ↓
composition roots
        ↓
Release / Capability Profile admission
        ↓
production exposure
```

The primary bounded contexts remain identity, clients, profiles, devices/runtime, mailboxes and notifications, with adapters/composition outside the domain/application core.

One protected `main` means one source history, one architecture hierarchy and one schema/compatibility lineage. It does not require one OS process: Worker, Profile Bridge, Camouhost and standalone `opsctl` remain legitimate technical boundaries when independently justified.

## 2. Single semantic owner

Every current semantic fact has exactly one natural owner.

Representations such as DTOs, rows, manifests, generated JSON, evidence envelopes, CLI output and frontend projections may carry a fact but do not become competing semantic authorities.

For current mutable concerns:

```text
one concern -> one legitimate current authority
```

Historical AR artifacts are provenance unless they are explicitly classified as the current natural owner.

## 3. Artifact-role taxonomy

Every durable machine artifact is classified as:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

A transitional semantic source is retired through:

```text
natural owner proved
-> callers switched
-> old caller count = 0
-> old unique-current-invariant count = 0
-> DEAD predecessor deleted
-> history preserved in Git/evidence
```

No successor JSON/YAML/TOML/Rust table may merely reproduce the retired duplicate authority under a new name.

## 4. Inward dependency / bounded context

Target dependency direction:

```text
domain
  ↑
application/use cases + ports
  ↑
adapters
  ↑
composition roots
```

Provider SDKs, HTTP frameworks, Cloudflare bindings, Microsoft/Google clients, filesystem/process/network primitives and concrete persistence implementations do not leak into provider-free domain/application semantics.

Product Runtime never depends on `opsctl` or `opsctl-core`.

Shared semantic crates are exception, not default. Extract one only when two real independent consumers need exactly the same pure invariant and one owner prevents a real duplicate semantic authority. A generic `common`, global service layer or god-policy crate is forbidden.

## 5. Pure Core / Effect Shell

Policy, compatibility, planning, compilation and state transitions are deterministic over typed explicit inputs wherever practical.

Effects stay at explicit outer boundaries.

Representative effect classes:

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

A pure evaluator does not read current time, cwd, locale, timezone, env, randomness, filesystem or network. Time/freshness/repository identity are explicit typed inputs when semantically required.

## 6. Permanent `opsctl` boundary

`opsctl` is a standalone project-specific offline policy/planning/verification/projection tool.

Permanent flow:

```text
external bytes/files/observations
        ↓
adapters
        ↓
closed versioned DTO
        ↓
typed semantic input
        ↓
PURE CORE
        ↓
typed result
        ↓
output adapter
```

Hard requirements:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
Product Runtime -> opsctl = 0
Product Runtime -> opsctl-core = 0
```

`Path`/`PathBuf` are shell/filesystem representations, not semantic identities. Core uses normalized typed identities such as repository-relative paths where needed.

A small internal `opsctl-core` package is appropriate when it materially gives compile-time enforcement. Dependency count itself is not a quality KPI; zero hidden effects/representation leakage is.

## 7. `opsctl doctor`

`doctor` is local read-only diagnostic composition, not a global semantic authority catalog.

Allowed: local filesystem observations, repository-root resolution, strict contract decode through owned adapters and diagnostic rendering.

Forbidden: Python/Node/process execution, Git/GitHub/provider/network access, secret resolution, provider/database mutation, runtime/browser execution, generated projection writes and duplicated bounded policy.

Repository-root identity must not depend on generated projections or retired AR/Python/Node sentinels. N2/N4/PF-1 remove current transitional dependencies. Detailed contract: `docs/OPSCTL_DOCTOR_CONTRACT.md`.

## 8. Python rule

Python is judged by role/effects, not by language prejudice or permanent per-file whitelist.

Allowed examples:

```text
genuine cross-language runtime adapter
synthetic/test fixture
repository/source observer
bounded validator
deterministic generator/renderer
test
developer-local orchestration
outer observation adapter where justified
```

Forbidden permanent roles:

```text
second Product/domain authority
second release/capability authority
second D1/lifecycle/evidence/fitness semantic authority
runtime path bypassing Profile Bridge
hidden provider mutation executor
secret readback/reporting surface
opsctl semantic child process
```

`runtime/camouhost/real.py` remains a legitimate Camoufox outer runtime adapter. `runtime/camouhost/main.py` remains synthetic/test-only.

The historical AR-6/AR-10/AR-11 per-file estate overlay model is transitional and is retired by N2. PF-3 enforces Python role/effect policy from repository/source observations rather than a hand-maintained file registry.

## 9. Observation / decision boundary

```text
Git/GitHub/provider/repository observations
        ↓
versioned raw/normalized observation
        ↓
typed validation
        ↓
pure policy decision
```

An observation adapter reports what it observed. It does not pre-decide the semantic conclusion owned by the pure policy evaluator.

A pure evaluator does not acquire network/provider/process effects merely to fetch its own inputs.

This rule is binding for PF-1 lifecycle, PF-2 evidence and N3 governance normalization.

Desired external-system configuration may remain versioned declarative data when it is truly configuration, e.g. GitHub branch/environment/check desired state. Live hosted state is observation; typed policy evaluates expected vs observed.

## 10. Typed identity/state/contracts

Use typed identities and explicit state machines where they prevent real ambiguity. External/persisted/integration/observation contracts are explicitly versioned.

Representative types include:

```text
ClientId
MailboxId
ProfileId
GenerationId
DeviceId
ReleaseSetId
GitCommitSha
Sha256Digest
MigrationRevision
CapabilityProfileId
EvidenceKind
EvidenceSchemaVersion
RepositoryRelativePath
ReasonCode
```

Core reason codes/outcomes are typed. String/JSON rendering belongs to adapters.

Do not create a second lifecycle/state machine when an accepted natural owner already exists.

## 11. Release / capability admission

Release / Capability Profile is the only production-enable authority.

```text
Release Profile
        ↓
Effective Capability Set
        ↓
backend execution-surface admission
```

Environment variables, frontend visibility or ad-hoc config flags never independently authorize production capability.

Frontend is projection only. Production-disabled backend paths reject before side effect.

## 12. Persistence / migration ownership

Persistence remains context-owned. No universal business repository/global CRUD authority or forbidden cross-context direct mutation.

For D1:

```text
SQL migration bytes
    = executable schema history

typed Rust D1 policy
    = non-derivable rollout/compatibility semantics

Wrangler ledger/provider output
    = observation
```

Generated inventory/release metadata may project those facts but does not own them.

## 13. Configuration / secrets

Raw environment/provider bindings are resolved at bootstrap/composition edges into validated typed configuration.

Secret material stays behind owned secret boundaries and is not converted into general config/readback/evidence.

## 14. Error / command-query discipline

Queries do not acquire incidental mutation. Commands have explicit effect capability and fail closed before the first side effect when auth/capability/preconditions fail.

Keep I/O/decode/contract/policy-decision/infrastructure errors distinct. A semantic `BLOCKED`, `UNKNOWN` or `INCOMPATIBLE` result is not collapsed into an I/O/decode error.

Machine stdout contracts are versioned; stderr is diagnostics.

## 15. Canonical external JSON and digest discipline

Use the accepted F2 canonical external JSON/digest foundation where content addressing/attestation requires semantic identity.

Security/release/evidence-critical JSON requires:

```text
explicit kind + schema_version
bounded bytes/depth/complexity
strict UTF-8
duplicate member rejection before canonicalization
closed fields unless explicit extension point
reviewed/pinned SHA-256
independent canonicalization/hash vectors
canonical bytes separate from pretty output
```

Digest scope is explicit:

```text
semantic JSON identity -> canonical semantic bytes -> SHA-256
exact artifact identity -> exact file bytes -> SHA-256
```

Do not hash Protobuf serialized bytes as a universal canonical identity.

Breaking external-contract changes bump version. Historical readers may be isolated only when a real current historical consumer/durable obligation is proved; current writer/model never silently changes meaning under the old version. The bounded pre-N2 F1 cleanup gate resolves the remaining v2 current-consumer question before N2 starts.

## 16. PF-1 target

PF-1 is the first full reference implementation after F/N normalization:

```text
outer Git/GitHub raw observations
-> typed lifecycle evaluator
-> bounded typed inventory projections
-> pure ArchitectureInventoryCompiler
-> canonical render/check/inspect
-> one bounded GENERATED_PROJECTION_WRITE to architecture/inventory.json
```

PF-1 must not create a `GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet`. It deletes legacy Node lifecycle and Python inventory/projection current authorities after parity + zero-caller + zero-unique-invariant proof.

PF-1 also disposes the manual AR-qualified application ownership/projection authority embodied in legacy `_ar3_application_architecture.py` tables. Still-valid application facts come from Rust structure/natural owners plus bounded observations; the old tables are not ported 1:1 into Rust or another machine registry.

## 17. PF-2 target

PF-2 Hosted Evidence reuses the same strict versioned DTO/canonical/digest principles:

```text
outer provider/GitHub observation
-> typed normalization
-> pure Rust EvidencePolicy
-> HostedEvidenceEnvelopeV1
-> canonical durable JSON
-> immutable hosted artifact/attestation
```

Evidence validity, freshness/replay and mutation eligibility are separate typed concepts. `opsctl` remains offline and has no provider credentials/mutation authority.

## 18. PF-3 target and architecture-forming freeze

PF-3 makes these rules permanent through typed Rust fitness semantics:

```text
FitnessRuleRegistry
-> evaluator / enforcement mapping
-> positive + negative fixtures
-> Architecture Fitness Gate
-> optional generated projection/report
```

A hand-maintained semantic `architecture-fitness-policy.json` is forbidden. JSON may be a generated index/projection only.

Required zero/one budgets include:

```text
semantic owner count per fact = 1
mutation executor count per owned mutation = 1
required rule without enforcement = 0
missing required negative fixture = 0
generated projection used as semantic source = 0
global authority bag = 0
runtime dependency on opsctl = 0
opsctl process/network/provider authority = 0
serde_json::Value crossing into pure core = 0
Python duplicate semantic authority = 0
unclassified Python production/provider effect = 0
breaking durable contract without version bump = 0
compatibility shim without proved consumer/durable obligation = 0
manual AR-qualified application ownership registry current authority = 0
```

Accepted PF-3 is the final planned Architecture Re-baseline v3 **architecture-forming freeze point**. After it, FC/AR/PC work may implement bounded functionality and explicit contract versions inside the established architecture, but may not introduce a new generic architecture layer, global authority/registry, duplicate lifecycle/evidence/fitness engine or speculative compatibility framework.

PF-3 acceptance does **not** set `architecture_complete=true` and does not authorize production. AR-16 remains audit-only and AR-17 qualification/authorization-only. A genuine later material architecture change requires the explicit governed architecture-change/anti-weakening path rather than being hidden inside a roadmap phase.

## 19. Development-stage verification protocol

For every materially changing candidate:

```text
fresh protected-main re-baseline
-> identify owners/contracts/effects/predecessors
-> implement at lowest bounded layer
-> positive + negative tests
-> explicit Architecture Impact where PF-3 applies
-> exact-head permanent CI green
-> protected required contexts green
-> behind_by=0
-> blocking reviews=0
-> unresolved threads=0
-> merge bound to exact proven head
-> post-merge accepted-main reread
```

Before PF-3, F/N/PF-1/PF-2 use their explicit DoD and existing permanent security/architecture gates.

## 20. Touch-to-converge

```text
GOOD             -> preserve
TOUCHED          -> converge now
LEGACY_UNTOUCHED -> classify until owning work touches it
```

No repository-wide rewrite is authorized solely for aesthetic consistency. A touched critical scope cannot use `legacy` as a permanent exemption.

## 21. Production invariant

Until AR-17:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

Only AR-17 may authorize Production Core. Only later PC-1 may enable accepted Production Core capabilities.
