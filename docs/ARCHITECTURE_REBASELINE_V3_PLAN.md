# Architecture Re-baseline v3 — Current Program Authority

**Document status:** CURRENT_AUTHORITY  
**Program:** Architecture Re-baseline v3  
**Tracking issue:** #266  
**Post-AR-11 Functional Closure:** #399  
**Accepted checkpoint:** AR-11 — Release-set / Promotion Architecture  
**Derived current AR slice:** AR-12 — Fresh Rehearsal Environment — NOT STARTED  
**Production authorization:** NONE  
**Architecture complete:** `false`  
**Production Core gate:** `BLOCKED`  
**Production ready:** `false`

This document is the single current architecture/program execution authority. Accepted AR-0…AR-11 history remains immutable evidence and is not retroactively rewritten. Current architecture, however, is owned by natural subject/domain/runtime/tooling owners rather than by historical AR-qualified transition artifacts.

The application remains one modular product with one protected `main`, one architecture hierarchy, one schema/compatibility lineage and one Release / Capability Profile authority for production admission.

```text
source_present != production_enabled
```

## 1. Binding prospective architecture contract

All future PF/FC/AR/PC work is governed by this document together with:

- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`;
- `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`;
- `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`;
- `docs/OPSCTL_DOCTOR_CONTRACT.md`;
- `docs/PYTHON_USAGE_BOUNDARY.md`.

Detailed pre-PF-1 normalization is owned by `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`.

Permanent architecture shape:

```text
natural canonical owners
        ↓
typed policy / contracts
        ↓
bounded-context domain + application
        ↓
explicit ports / adapters / effect capabilities
        ↓
composition roots
        ↓
Release / Capability Profile admission
        ↓
production exposure
```

Binding cross-cutting rules:

```text
Single semantic owner
Bounded Context ownership
Inward Dependency direction
Pure Core / Effect Shell
Observation != policy decision
Explicit effects/capabilities
Typed critical IDs/states/contracts
Command != Query
Context-owned persistence
Typed validated configuration at composition edges
Versioned integration/external contracts
Release Profile = sole production-enable authority
Frontend = projection, never security boundary
Cutover -> zero callers -> zero unique current invariants -> delete DEAD predecessor
Touch-to-converge, not repository-wide rewrite
```

Historical AR artifacts are evidence of how decisions were accepted. They are not automatically permanent current semantic authorities.

### 1.1 Architecture precedence and pre-production compatibility default

This repository has not yet had a production release. Therefore historical internal implementation shape is not a compatibility target by default.

When requirements conflict, precedence is:

```text
CURRENT PROSPECTIVE ARCHITECTURE CONTRACT
        ↓
current product/security/runtime invariants
        ↓
proved current external/durable contracts and real consumers
        ↓
accepted historical AR outcomes + immutable evidence/provenance
        ↓
historical internal implementation shape
```

An accepted AR preserves still-required behavior, safety guarantees, durable compatibility obligations and historical evidence. It does **not** permanently preserve the JSON/Python/Node/table/registry mechanism through which those guarantees were originally accepted.

Accordingly:

```text
no proved current/external consumer
        ↓
compatibility bridge default = NO
```

A compatibility reader/bridge may remain only when a concrete current consumer or explicit durable contract is named, the exact version/shape is identified, the compatibility path is isolated from the current writer/semantic owner, and the retirement condition is explicit. "Accepted before" or "might be useful later" is not sufficient justification.

New prospective architecture does not bend around AR-11 or any earlier AR implementation accident. Still-valid AR-11 release/rollback/same-bits/NO_CHANGE/fail-closed outcomes must be implemented through the current architecture; obsolete AR-11 internal machinery must conform, migrate or retire.

## 2. Artifact-role taxonomy

Every durable machine artifact must be classified as one of:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

A `TRANSITIONAL_SEMANTIC_SOURCE` must be retired when its natural owner is available:

```text
find natural owner
-> preserve accepted behavior/invariants
-> switch all current consumers
-> caller_count = 0
-> unique_current_invariant_count = 0
-> physical delete/demotion
-> preserve Git/evidence history
```

JSON is not prohibited. JSON is valid for versioned manifests, durable observations/evidence, external configuration and generated projections. JSON is invalid as a manually duplicated second semantic owner when typed executable semantics already have a natural owner.

## 3. Canonical AR sequence

`architecture/architecture-program-sequence.json` remains the static-order-only program data authority. It does not carry mutable accepted/current state.

```text
AR-0   Delta Architecture Inventory                              DONE / ACCEPTED
AR-1   Architecture Authority Re-baseline                        DONE / ACCEPTED
AR-2   Runtime Topology + D3 Compatibility                       DONE / ACCEPTED
AR-3   Application Architecture Contract                         DONE / ACCEPTED
AR-4A  Composition-root consolidation                            DONE / ACCEPTED
AR-4B  Client Mail route ownership                               DONE / ACCEPTED
AR-4C  Outbound Mail composition extraction                      DONE / ACCEPTED
AR-4D  Profile extraction                                        NOT REQUIRED unless later evidence reopens
AR-5   Wrangler / Runtime Authority Cleanup                      DONE / ACCEPTED
AR-6   Full Python Estate + read-only Rust opsctl                DONE / ACCEPTED
AR-7   Environments + GitHub Governance + Operational Boundaries DONE / ACCEPTED
AR-8   Secrets / Keys / OAuth Refresh Concurrency                 DONE / ACCEPTED
AR-9   D1 Evolution / Schema Compatibility                       DONE / ACCEPTED
AR-10  Runtime and Historical Executable Simplification          DONE / ACCEPTED
AR-11  Release-set / Promotion Architecture                      DONE / ACCEPTED
AR-12  Fresh Rehearsal Environment                               DERIVED CURRENT / NOT STARTED
AR-13  Rotation Rehearsal
AR-14  Remote Recovery Rehearsal
AR-15  Windows Delivery Program — inherited Batch E
AR-16  Final Whole-project 10/10 Audit
AR-17  Architecture Closeout + Production Core Gate
```

The normalization/foundation work below does **not** create AR-2B/AR-6B/AR-10B, PF-0 or PF-4 and does not change this static sequence.

No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity.

## 4. Binding continuation order before AR-12

AR-12 implementation entry is blocked by Post-AR-11 Functional Closure and the following exact prerequisite order:

```text
F1  Release Set breaking-contract version discipline
+
F2  Mandatory architecture foundations
    - opsctl pure-core / adapter boundary
    - opsctl doctor diagnostic boundary
    - canonical JSON/digest contract
    - Python usage/effect boundary
    - permanent application architecture requirements
 ->
N1  AR-2 runtime/resource topology current-authority retirement
 ->
N2  AR-6 Python-estate current-authority retirement + role/effect normalization
 ->
N3  AR-7 current GitHub-governance authority normalization
 ->
N4  bounded AR-8 operator/provenance authority cleanup
 ->
N5  AR-10 runtime semantic-authority retirement
 ->
PF-1  typed lifecycle evaluator + deterministic inventory compiler + Node/Python predecessor deletion
 ->
PF-2  Universal Hosted Operational Evidence
 ->
PF-3  typed Architecture Fitness Baseline + architecture-forming freeze point
 ->
fresh re-baseline #399 / #421
 ->
FC-6  real staging same-bits / rollback rehearsal
 ->
FC-7  final whole-AR-11 functional audit
 ->
AR-12 implementation entry
```

F1/F2 and N1…N5 are foundation/normalization transactions under the current program, not lifecycle slices. Each transaction starts from accepted protected `main` and completes atomically on one PR/merge unless a fresh defect proves another bounded plan is required.

N2…N5 must finish their own authority retirements rather than leaving ownership ambiguity for PF-1. PF-1 is intentionally bounded to lifecycle/inventory replacement and retirement of its own Node/Python predecessors; it is not a catch-all architecture cleanup phase.

## 5. Purpose of Pre-PF-1 normalization

PF-1 must not become a Rust port of a giant historical authority bag.

The following current transition artifacts are explicitly targeted before PF-1 where caller/invariant proofs permit:

```text
AR-2 runtime-topology current semantic intermediary
AR-6/AR-10/AR-11 Python estate overlay chain
AR-7 + AR-10 historical required-check overlay model
AR-8 operator-contract CLI semantic authority
AR-10 runtime-cutover semantic intermediary
```

Their accepted historical meaning remains in Git/evidence. Current facts move to natural owners such as Wrangler configuration, Product Rust, SQL migrations, typed Rust policy, runtime manifests and current governance data.

## 6. Target PF-1 boundary

PF-1 receives bounded validated projections, not raw historical AR authority documents:

```text
ValidatedProgramSequence
RawArchitectureAcceptanceEvidenceV1
        ↓
LifecycleEvaluator
        ↓
DerivedLifecycleStateV1

D1InventoryProjection
RuntimeTopologyProjection
ApplicationInventoryProjection
OperatorInventoryProjection
GovernanceInventoryProjection
RuntimeInventoryProjection
CredentialInventoryProjection
ReleaseInventoryProjection
        ↓
ArchitectureInventoryCompiler
        ↓
architecture/inventory.json
```

Forbidden target:

```text
GlobalRepositoryAuthorityLoader
    -> GlobalAuthoritySet
    -> giant god-validator/compiler
```

`architecture/inventory.json` is a generated projection and never a semantic input for the facts it projects.

PF-1 may temporarily host old/new implementations inside one candidate branch for parity proof, but accepted `main` must not retain a standing compatibility architecture between them. The intended cutover is atomic: switch current callers, prove predecessor caller/invariant counts are zero, then delete the predecessor in the same accepted transaction.

## 7. Permanent `opsctl` boundary

`tools/opsctl` is a standalone project-specific operator/policy tool. Product Runtime never depends on it.

Target internal direction:

```text
CLI / composition
        ↓
adapters: filesystem / strict external decoding / rendering
        ↓
versioned DTOs
        ↓
typed semantic input
        ↓
PURE CORE
        ↓
typed result
        ↓
output adapter
```

Hard invariant:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
runtime product dependency on opsctl = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
global authority bag = 0
```

A small internal `opsctl-core` crate is preferred when it materially makes these constraints compile-time enforceable. It is never a Product Runtime shared crate.

`opsctl` does not execute deployments, provider mutation, GitHub APIs, Camouhost/Camoufox or background services. GitHub Actions/official provider tools/outer adapters collect observations and perform allowed hosted/provider effects.

## 8. Permanent `opsctl doctor` boundary

`opsctl doctor` is local read-only diagnostic composition only.

Allowed:

```text
FilesystemRead
repository-root resolution
strict local contract decoding through owned adapters
bounded diagnostic aggregation
stdout/stderr rendering
```

Forbidden:

```text
ProcessExecution
Python/Node subprocess
network/GitHub/provider access
secret resolution
provider/database mutation
runtime/browser execution
semantic authority catalog duplication
```

Current dependencies on AR-6 Python-estate files, `operator-contract.json`, Python inventory generators and generated/retiring sentinels are transitional. N2/N4/PF-1 remove them. Repository identity must use durable surviving markers, not files scheduled for retirement or generated projections. Detailed contract: `docs/OPSCTL_DOCTOR_CONTRACT.md`.

## 9. Python boundary

Python is allowed by role/effect, not by a permanent per-file whitelist.

Allowed roles include:

```text
genuine cross-language runtime adapter
synthetic/test fixture
repository/source observer
bounded validator
deterministic generator/renderer
test/fixture
developer-local orchestration
outer hosted/provider observation adapter where justified
```

Python is forbidden as a second Product/business/release/lifecycle/evidence/fitness semantic authority or ungoverned provider mutation executor.

`runtime/camouhost/real.py` remains a legitimate Product Runtime adapter behind Profile Bridge + versioned IPC + `runtime-lock.json`. `runtime/camouhost/main.py` remains synthetic/test-only.

The historical Python estate baseline/overlay chain is retired by N2; there is no successor 1:1 registry of all Python files. PF-3 enforces role/effect constraints source-derivably.

## 10. Release Set and canonical-contract foundation

Breaking external contract changes bump schema version. The incompatible change from `d1_evolution_authority_sha256` to `d1_repository_identity_sha256` must not remain under one current v2 meaning; current writer/model moves to a new version (target v3 unless fresh evidence proves another bounded decision). Historical immutable v2 verification, if still needed by #399/#421, is isolated from the current writer/model.

Historical-v2 compatibility exists only when an exact current consumer is proved under the compatibility rule in §1.1. Without that proof, historical evidence remains immutable in Git/artifacts but executable compatibility machinery is retired rather than kept speculatively.

Attestable/content-addressed JSON uses one explicit canonical external contract with:

```text
explicit kind + schema_version
bounded bytes/depth/complexity
duplicate-member rejection before canonicalization
reviewed/pinned SHA-256
independent canonicalization/hash vectors
canonical bytes separated from pretty rendering
explicit digest scope: semantic canonical bytes OR exact artifact bytes
```

## 11. PF-2 boundary

PF-2 target:

```text
GitHub Actions / official provider tools
        ↓
secret-free raw observation
        ↓
strict versioned DTO
        ↓
typed Rust EvidencePolicy
        ↓
HostedEvidenceEnvelopeV1
        ↓
canonical durable JSON
        ↓
immutable Actions Artifact / GitHub Artifact Attestation
```

Observation acquisition is not evidence-validity policy. Network/provider reads remain outside `opsctl` pure core. Freshness/replay decisions receive explicit typed observations.

## 12. PF-3 boundary

PF-3 semantic fitness authority is typed Rust:

```text
FitnessRuleRegistry
        ↓
fitness evaluator / enforcement mapping
        ↓
Architecture Fitness Gate
        ↓
optional generated projection/report
```

A hand-maintained `architecture/architecture-fitness-policy.json` must not become the semantic owner. If a JSON view exists, it is generated/index/projection only.

Minimum zero-budget families include authority duplication, forbidden dependency/effect edges, generated-projection semantic use, runtime dependency on `opsctl`, `opsctl` process/network/provider authority, Python duplicate semantic authority, unclassified Python production/provider effects, and silent breaking-contract changes without version bump.

### 12.1 PF-3 is the architecture-forming freeze point

Accepted PF-3 is the last planned stage allowed to introduce a new generic architecture mechanism for Architecture Re-baseline v3.

After PF-3 acceptance:

```text
new generic architecture layer/framework = FORBIDDEN
new global authority/registry/lifecycle engine = FORBIDDEN
new compatibility architecture without proved consumer = FORBIDDEN
AR/FC/PC phase used as a redesign bucket = FORBIDDEN
```

Later work may add or evolve product functionality, bounded contexts, use cases, ports/adapters, provider integrations, explicit contract versions, migrations, security corrections, recovery, delivery and performance within the established architecture. A material architecture change remains possible only through the explicit architecture-change process and PF-3 fitness anti-weakening; it is not normal roadmap work.

This freeze is a **design/enforcement milestone**, not production authorization and not the lifecycle flag `architecture_complete=true`. That lifecycle flag remains false until AR-17 qualification succeeds.

Post-PF-3 phase semantics are therefore:

```text
FC-6 / FC-7     functional closure + staging proof; no architecture redesign
AR-12..AR-15    implementation/rehearsal/delivery on the frozen architecture
AR-16           final whole-project audit only
AR-17           qualification/authorization decision only
PC-1            first Production Core release
```

If AR-16/AR-17 finds a violation, the gate blocks and the violation is corrected under the frozen architecture. AR-16/AR-17 do not invent Architecture v4 in-place.

## 13. Production state model

These states remain independent:

```text
architecture_complete
production_core_gate
production_ready
```

Until AR-17 succeeds:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
```

PF-3 acceptance means the prospective architecture is designed and machine-enforced, but does not set `architecture_complete=true`; AR-12…AR-16 still have to prove the product, delivery and rehearsals on that architecture.

Only AR-17 may authorize the Production Core gate. Only later PC-1 may set `production_ready=true` for the accepted `production-core-v1` Release Profile.

No PF/F/N/FC work here authorizes production.

## 14. Production capability roadmap

After AR-17:

```text
PC-1  Production Core v1
PC-2  Mailbox Administration
PC-3  Mailbox Jobs / Automation
PC-4  Outbound / later capabilities
```

`PC-1 production-core-v1` is explicitly bounded to:

```text
authentication / authorization / membership foundation
users
clients / customer cards
browser profiles
single + bulk browser-profile operations
client <-> browser-profile binding
grants / access
profile metadata + generations + sessions + devices
encrypted immutable profile persistence / restore required by the profile lifecycle
real Camoufox runtime
Windows Profile Bridge
production-grade Windows updater/publisher/delivery chain from AR-15
runtime/profile certification required by the Core profile
audit
health / readiness / observability
notifications/recovery foundations required by the Core lifecycle
```

The following may be present and tested on the same `main` but remain `production_enabled=false` in PC-1:

```text
mailbox administration
bulk mailbox operations
client <-> mailbox bindings
mailbox jobs / automation
outbound mail/email side effects
later CRM/communications capabilities
```

This is one application and one data/compatibility lineage, not a `production-lite` fork. PC-2/PC-3 enable existing or newly completed mailbox capability only through their accepted Release / Capability Profile; source presence never grants production access.

## 15. GitHub/governance acceptance

Every bounded transaction/PR must:

```text
fresh re-baseline protected main + trackers + competing PRs
-> identify natural owner / effects / contracts / legacy predecessor
-> implement with positive + negative proofs
-> exact-head permanent CI green
-> protected required contexts green
-> behind_by = 0
-> blocking reviews = 0
-> unresolved threads = 0
-> guarded merge bound to the exact candidate head
-> accepted-main reread
```

Historical required-context counts/names are observations, not timeless constants; re-read live branch protection for every merge.

## 16. Definition of Done for the pre-AR-12 prerequisite program

AR-12 remains blocked until:

1. F1 and F2 are accepted on protected `main`;
2. N1…N5 complete with zero-current-caller/zero-unique-invariant predecessor proofs;
3. PF-1 is accepted and Node/Python lifecycle/inventory predecessors are deleted;
4. PF-2 Hosted Evidence is accepted;
5. PF-3 typed fitness enforcement and architecture-forming freeze are accepted;
6. #399/#421 are freshly re-baselined from that accepted main;
7. FC-6 completes real staging same-bits/rollback evidence or returns a legitimate typed `BLOCKED` state;
8. FC-7 reports repository-owned `P0=0`, `P1=0`, `P2=0` for AR-11 Functional Closure scope;
9. production remains fail-closed;
10. accepted-main reread proves no parallel authority path was introduced.
