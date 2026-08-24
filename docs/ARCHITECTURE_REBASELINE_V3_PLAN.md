# Architecture Re-baseline v3 — Current Program Authority

**Document status:** CURRENT_AUTHORITY  
**Program:** Architecture Re-baseline v3  
**Tracking issue:** #266  
**Post-AR-11 Functional Closure:** #399  
**Accepted checkpoint:** PF-1 — typed lifecycle/inventory authority cutover — #466
**Current transaction:** PF-2 — minimal typed hosted evidence — #471
**Next AR slice:** AR-12 — Fresh Rehearsal Environment — BLOCKED / NOT STARTED
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

Accepted pre-PF-1/PF-1 history is preserved by `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`, `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`, #441, #430 and #466. It is not mutable current execution authority.

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

### 1.1 Product outcome and simplification gate

The remaining program exists to ship a coherent Production Core, not to maximize architecture artifacts. From N2 through PF-3 the mandatory operating mode is **delete and simplify**:

```text
reduce current semantic authorities
+ reduce transitional execution paths
+ reduce duplicate representations and compatibility surfaces
+ shorten the path to product rehearsal / PC-1
```

For every N2…PF-3 transaction the PR records a compact before/after table for:

```text
current semantic authorities
transitional semantic sources
duplicate representations
legacy current callers
Python/Node semantic-authority paths
tracked generated projections
compatibility-only commands/workflows
current plan + validator + projection LOC
```

N2…N5 must strictly reduce their predecessor estate. PF-1…PF-3 may add only the named typed owner/enforcement needed by the phase and must delete its replaced machinery in the same accepted transaction. Across N2…PF-3, current planning/validation/projection/governance surface must be net smaller. A phase that adds another large plan, validator family, projection catalog, authority registry or compatibility layer without deleting a larger predecessor fails this gate even when all checks are green.

Permanent zero budgets:

```text
new parallel roadmap/current-plan document = 0
new 1:1 successor registry = 0
new hand-maintained global authority catalog = 0
new tracked projection without proved durable exact-byte consumer = 0
new checker whose primary purpose is checking another checker = 0
legacy predecessor kept alive only by internal CI/docs/self-test = 0
```

An internal generator, drift gate, validator, self-test, documentation reference or `opsctl`/CI caller that exists only because the legacy artifact exists is part of the deletion set, not proof of a durable consumer. Caller discovery is bounded to one pass plus affected deltas. When no external/runtime/durable exact-format consumer exists, switch or delete those internal callers and delete the predecessor atomically; do not create another observation phase to re-check the same fact.

Historical AR artifacts are evidence of how decisions were accepted. They are not automatically permanent current semantic authorities.

### 1.2 Architecture precedence and pre-production compatibility default

This repository has not yet had a production release. Therefore historical internal implementation shape is not a compatibility target by default.

Current product/security guarantees and proved durable/external obligations constrain which architecture solutions are valid. Subject to those obligations, the current prospective architecture owns the internal implementation shape. The precedence is:

```text
CURRENT PRODUCT / SECURITY / DURABLE OBLIGATIONS
        ↓ constrain acceptable solutions
CURRENT PROSPECTIVE ARCHITECTURE CONTRACT
        ↓ owns internal architectural shape
CURRENT NATURAL SEMANTIC OWNERS
        ↓
PROVED CURRENT CONSUMERS / EXTERNAL OBSERVATIONS
        ↓
ACCEPTED HISTORICAL AR OUTCOMES + IMMUTABLE EVIDENCE/PROVENANCE
        ↓
HISTORICAL INTERNAL IMPLEMENTATION SHAPE
```

An accepted AR preserves still-required behavior, safety guarantees, durable compatibility obligations and historical evidence. It does **not** permanently preserve the JSON/Python/Node/table/registry mechanism through which those guarantees were originally accepted. Conversely, the prospective architecture may not discard a real persisted/wire/external obligation merely because a different internal design would be simpler; such an obligation must be versioned, migrated or explicitly retired through its owning contract.

Accordingly:

```text
no proved current/external consumer
+ no durable/persisted/migration obligation
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
bounded pre-N2 F1 compatibility/current-v2 cleanup gate
    (correction transaction only; not a new F/N/AR/PF slice)
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
PF-2  minimal typed Hosted Operational Evidence (#471 CURRENT)
 ->
PF-3  provisional typed Architecture Fitness Baseline
 ->
FC-6 preflight
    (mandatory fresh #399 / #421 live re-baseline; read-only)
 ->
FC-6 real staging same-bits / rollback ceremony
 ->
FC-7 final whole-AR-11 functional closeout
 ->
AR-12 implementation entry
```

The fresh #399/#421 re-baseline is the mandatory **first read-only step of FC-6 preflight**. It is not a separate implementation transaction, PR or ceremony. Starting from accepted PF-3 `main`, it must refresh live governance/workflows, credential readiness/scope, current staging identity, current known-good identity, current Release Set identities and required hosted evidence/attestations, and return typed `READY | BLOCKED`. Only `READY` permits deploy-capable credentials and staging mutation.

F1/F2 and N1…N5 are foundation/normalization transactions under the current program, not lifecycle slices. Each transaction starts from accepted protected `main` and completes atomically on one PR/merge unless a fresh defect proves another bounded plan is required. The pre-N2 cleanup gate is exactly such a bounded correction discovered by live audit; it does not create a new roadmap state.

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

After accepted #454, the one common read-only N2–N5 discovery pass also audits concrete current/durable consumers of the **exact tracked bytes** of `architecture/inventory.json`. This retention decision belongs to pre-PF-1 normalization, not to PF-1's semantic compiler work:

```text
real durable exact-byte consumer exists
-> retain only the minimum deterministic GENERATED_PROJECTION required by it

consumer = NONE
-> retire tracked architecture/inventory.json once remaining callers are naturally cut over
-> retire compatibility-only --write / tracked-byte drift ceremony
-> keep deterministic on-demand render/check only where useful
```

A generator checking the file because it exists, a documentation reference, historical evidence, or a CI drift test whose only purpose is the tracked projection is not a durable consumer. Do not create a new phase/issue for this decision and do not pre-build PF-1's lifecycle evaluator or inventory compiler during N2–N5.

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
optional deterministic generated projection
```

Forbidden target:

```text
GlobalRepositoryAuthorityLoader
    -> GlobalAuthoritySet
    -> giant god-validator/compiler
```

`architecture/inventory.json`, if the earlier retention decision still requires it, is a generated projection and never a semantic input for the facts it projects. PF-1 consumes the already-accepted `JUSTIFIED_MINIMUM | NOT_RETAINED` retention result; it does not defer or repeat the tracked-file decision.

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

Breaking external contract changes bump schema version. The incompatible change from `d1_evolution_authority_sha256` to `d1_repository_identity_sha256` must not remain under one current v2 meaning; current writer/model moves to v3. Historical immutable v2 verification exists only while a concrete current consumer/durable obligation is proved and remains isolated from the current writer/model.

#454 resolved the current-v2 ambiguity before N2:

```text
architecture/release-set-v2.json current semantic authority = 0
historical v2 executable = minimum isolated source/artifact integrity verification only
current writer/model = v3-only
current promotion/rollback target = v3-only
v2 -> v3 semantic coercion = 0
```

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

PF-2 is the current bounded concern. #471 is its only live execution tracker; do not add another PF-2 plan, provider registry or compatibility layer.

Target:

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

Hard boundary and stop conditions:

```text
GitHub/provider/network/clock/credential/publication effects in workflows or official tooling
opsctl provider/network/process/credential authority = 0
strict secret-free versioned DTO at the adapter boundary
serde_json::Value crossing adapter -> pure core = 0
typed EvidencePolicy owns only provider-neutral validity/freshness/trust semantics
generic provider/plugin/evidence framework = 0
second lifecycle/fitness/evidence engine = 0
new tracked projection without durable exact-byte consumer = 0
production_mutation = false
```

PF-2 implements one thin vertical path for evidence already required by protected workflows and PAS-1…PAS-7. Shared abstraction is added only after at least two concrete current consumers prove common semantics. Any replaced predecessor policy/caller is deleted in the same accepted transaction; dual evidence-validity authority is failure even when CI is green.

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

### 12.1 PF-3 is provisional; final architecture freeze follows AR-15

Accepted PF-3 establishes a **provisional fitness baseline**: already-selected constraints are typed, machine-enforced and protected against silent weakening. It is not the final claim that the architecture form has survived real deployment, recovery and Windows delivery.

After PF-3 acceptance:

```text
new generic architecture layer/framework = FORBIDDEN
new global authority/registry/lifecycle engine = FORBIDDEN
new compatibility architecture without proved consumer = FORBIDDEN
AR/FC/PC phase used as an open-ended redesign bucket = FORBIDDEN
```

FC-6 through AR-15 may make a bounded architecture correction only when a concrete product acceptance scenario or rehearsal fails and the correction is the smallest viable remedy. Such a correction names the failed scenario, changes the natural owner rather than adding a parallel owner, updates PF-3 enforcement in the same transaction and leaves the measured authority estate no larger unless a durable external obligation makes that impossible.

Final architecture-form freeze occurs only after AR-15 is accepted and its real Windows delivery/recovery rehearsal passes. From that point AR-16 is audit-only, AR-17 is qualification/authorization-only, and new generic architecture mechanisms are forbidden except through an explicit post-freeze architecture-change process. Neither PF-3 nor AR-15 sets `architecture_complete=true`; that lifecycle flag remains false until AR-17 qualification succeeds.

Post-PF-3 phase semantics are therefore:

```text
FC-6 / FC-7     functional closure + staging proof; scenario-driven bounded correction only
AR-12..AR-15    rehearsal/delivery; smallest scenario-driven correction only
AR-16           final whole-project audit only
AR-17           qualification/authorization decision only
PC-1            first Production Core release
```

If AR-16/AR-17 finds a violation, the gate blocks and the violation is corrected under the final architecture form accepted after AR-15. AR-16/AR-17 do not invent Architecture v4 in-place.

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

## 14. Product acceptance scenarios and capability roadmap

Architecture and phase completion are accepted only through these seven end-to-end product scenarios. Unit/integration checks remain necessary but cannot substitute for the scenario outcome.

| ID | Product acceptance scenario | Required observable result |
| --- | --- | --- |
| PAS-1 | Identity and governed access | Owner bootstrap/sign-in, invitation/membership and authorization work end to end; unauthorized, revoked and stale access fail before mutation. |
| PAS-2 | Client and browser-profile workflow | An operator uses the real UI/API to create/update clients and profiles, bind them, grant access and complete representative bulk operations with stable validation and audit. |
| PAS-3 | Encrypted profile lifecycle | A generation is created, encrypted, persisted, opened, closed and restored from durable data without secret/plaintext leakage or identity ambiguity. |
| PAS-4 | Real Windows browser execution | Windows Profile Bridge launches the pinned real Camoufox runtime through versioned IPC, enforces single-writer ownership, reports health and safely closes/updates/rolls back. |
| PAS-5 | Failure, retry and recovery | Crash, timeout, duplicate delivery and partial failure are retried/idempotently fenced; remote recovery restores a consistent usable profile and produces actionable observability. |
| PAS-6 | Fresh same-bits delivery and rollback | A clean staging environment is created from canonical inputs, exact accepted bits are deployed, verified, rolled back/LKG-restored and recreated without hidden operator state. |
| PAS-7 | Production admission fails closed | `production-core-v1` enables only its declared backend execution surfaces; mailbox/outbound/later capabilities, missing evidence, invalid credentials and unknown compatibility remain blocked before side effect. |

Each scenario contract must name: user-visible result; UI/API/runtime path; data and external contracts; authorization/security negatives; retry/idempotency behavior; observability and reason codes; platform/environment; measurable performance/reliability budget owned by the product requirement; and durable acceptance evidence. A phase cannot declare success using architecture artifacts alone.

Remaining-phase binding:

| Phase | Product scenarios it must advance or prove |
| --- | --- |
| N2…PF-3 | Remove authority ambiguity and build only the minimum enforcement needed to make PAS-1…PAS-7 trustworthy; no substitute product ceremony. |
| FC-6 / FC-7 | PAS-1, PAS-2, PAS-3, PAS-6 and PAS-7 on real staging, including same-bits/rollback and fail-closed negatives. |
| AR-12 | PAS-1, PAS-2, PAS-3 and PAS-6 from a genuinely fresh environment. |
| AR-13 | PAS-3, PAS-5 and PAS-7 under real credential/key/secret rotation. |
| AR-14 | PAS-3, PAS-5 and PAS-6 through remote recovery from durable state/artifacts. |
| AR-15 | PAS-4, PAS-5 and PAS-6 through the production-equivalent Windows delivery/updater chain; acceptance establishes the final architecture-form freeze. |
| AR-16 | Audit PAS-1…PAS-7 evidence and whole-project budgets; no implementation bucket. |
| AR-17 | Authorize only if PAS-1…PAS-7 and all mandatory gates are accepted; no new closeout engine. |
| PC-1 | Promote the exact accepted release and re-prove PAS-1…PAS-7 production admission/observability expectations. |

### 14.1 Production capability boundary

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
2. the bounded pre-N2 F1 compatibility/current-v2 cleanup gate is accepted, with one current Release Set semantic owner and historical-v2 compatibility either concretely justified/isolated or retired;
3. N1…N5 complete with zero-current-caller/zero-unique-invariant predecessor proofs and the tracked-inventory retention result is `JUSTIFIED_MINIMUM | NOT_RETAINED`;
4. PF-1 is accepted and Node/Python lifecycle/inventory predecessors are deleted;
5. PF-2 Hosted Evidence is accepted;
6. PF-3 provisional typed fitness enforcement is accepted without a new validator/projection estate;
7. FC-6 preflight executes the mandatory fresh #399/#421 live re-baseline from accepted PF-3 main and returns `READY | BLOCKED`;
8. only `READY` crosses into FC-6 real staging same-bits/rollback evidence; a legitimate typed `BLOCKED` state remains fail-closed and authorizes no mutation;
9. FC-7 reports repository-owned `P0=0`, `P1=0`, `P2=0` for AR-11 Functional Closure scope;
10. product acceptance scenarios required by FC-6/FC-7 have durable typed evidence and production remains fail-closed;
11. accepted-main reread proves no parallel authority path was introduced and the N2…PF-3 simplification ledger is net smaller;
12. final architecture-form freeze remains pending until accepted AR-15 proves PAS-4/PAS-5/PAS-6; PAS-1…PAS-7 remain the binding route through AR-16/AR-17 to PC-1.
