# Post-AR-11 Functional Closure Plan — Release / Promotion Contract to 10/10

**Document status:** SUBORDINATE_REMEDIATION_PLAN  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Architecture evolution contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**PF-3 enforcement specification:** `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`  
**Accepted AR-11 design:** `docs/ARCHITECTURE_REBASELINE_V3_AR11.md`  
**Live execution tracker:** issue #399  
**Current FC-6 implementation/hardening tracker:** issue #421  
**PF-1 tracker:** issue #430  
**PF-2 candidate:** Draft PR #428  
**PF-3 tracker:** issue #431  
**Scope:** close residual functional gaps in the already accepted AR-11 release-set / promotion architecture and establish the required reusable tooling/fitness foundations before AR-12 implementation  
**AR-12 implementation:** FORBIDDEN inside this plan  
**Production mutation / enablement:** FORBIDDEN except already-authorized non-production staging mutation explicitly owned by FC-6 rehearsal  
**Historical AR-11 acceptance:** PRESERVED; this plan does not rewrite or revoke accepted history

This document is the single subordinate execution plan for Post-AR-11 Functional Closure. It is not a second lifecycle authority, not a new AR slice and not a parallel roadmap. The canonical AR order remains owned by the Architecture Re-baseline program and Git-derived acceptance mechanism.

If stale text, an old SHA, issue comment or PR description conflicts with live protected `main` and current authorities, precedence is:

```text
live protected main
+ canonical architecture/domain authorities
+ current GitHub hosted evidence
+ issue #399 live execution tracker
+ this subordinate plan
+ historical progress notes
```

The project remains one modular application with one protected `main`, one architecture hierarchy and one data/schema compatibility history. Functionality may exist in source before it is production-enabled:

```text
source_present != production_enabled
```

## 1. Binding continuation sequence

The mandatory continuation path is:

```text
PF-1  Canonical Architecture Inventory cutover to opsctl       #430
  ->
PF-2  Universal Hosted Operational Evidence primitive          Draft PR #428
  ->
PF-3  Architecture Fitness Baseline                            #431
  ->
fresh re-baseline #399 / #421
  ->
FC-6 real staging same-bits / rollback rehearsal
  ->
FC-7 final whole-AR-11 functional audit
  ->
AR-12 implementation entry under the canonical program
```

PF-1/PF-2/PF-3 are **Functional Closure prerequisites**, not AR-11A, AR-11.5 or AR-12 slices. They do not alter `architecture/architecture-program-sequence.json`, reopen accepted AR-11 history or authorize production.

No path in this plan may skip PF-3 and resume FC-6 directly after PF-2.

Issue #375 is closed/completed historical hardening. It is not a current blocker or lifecycle authority. Any text that still says `#375 OPEN` is projection drift and must be corrected through the existing authority/projection mechanism; it never authorizes AR-12.

## 2. Non-negotiable architecture and production invariants

```text
one protected main
one architecture hierarchy
one application/source history
one data/schema compatibility history
one canonical capability/release hierarchy
source_present != production_enabled
build once -> promote same bits
GitHub Actions/Environments = orchestration + approvals + credential boundary
opsctl = local typed project-specific policy / validation / projection engine
provider executors = actual provider mutation authority
Git = intended source/release truth
provider state = deployed/runtime truth
```

Production remains fail-closed:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

The Architecture Evolution Quality Contract is binding for all touched scope. In particular:

```text
Single Authority
Bounded Context ownership
Inward Dependency direction
Pure Core / Effect Shell
Explicit effect capabilities
Typed critical IDs / states / versioned contracts
Command / Query separation
Context-owned persistence
Typed config at composition edges
Versioned integration events
Release Profile = sole production-enable authority
Frontend = projection, never security authority
Cutover -> zero callers -> zero unique invariants -> delete DEAD predecessor
Touch-to-converge, not global rewrite
```

Do not:

- rebuild AR-11 from scratch;
- create a second architecture inventory, release registry, capability registry, lifecycle state machine, evidence database or hidden `opsctl` state backend;
- create a parallel inventory generator during PF-1;
- independently parse acceptance tags in Rust/Python during PF-1;
- add provider mutation authority to `opsctl`;
- give `opsctl` GitHub/Cloudflare/provider network authority for convenience;
- rebuild application artifacts during promotion;
- introduce Terraform/generic IaC state;
- enable Production Core or mailbox capabilities;
- start AR-12;
- absorb AR-14 recovery, AR-15 Windows updater/signing or AR-17 production authorization into this closure work;
- retain compatibility shims without a proved current consumer;
- weaken a gate, convert UNKNOWN/UNPROVEN to PASS or suppress a validator to obtain green CI.

## 3. Current functional baseline — preserve, do not reimplement blindly

The live tracker #399 and accepted Git history are the evidence ledger. Existing AR-11 Functional Closure outcomes remain foundations unless a fresh audit proves a defect, including:

1. historical accepted-source semantics rather than `source_sha == current main HEAD`;
2. content-addressed immutable Release Sets and exact durable artifact verification;
3. native `opsctl release inspect|verify|compatibility`;
4. native `opsctl promotion plan|preflight|verify`;
5. rollback compatibility vocabulary `COMPATIBLE | INCOMPATIBLE | UNKNOWN`, with UNKNOWN fail-closed where evidence is required;
6. stale expected-current fencing and same-environment serialization;
7. `NO_CHANGE` as first-class convergence;
8. separation of read/observe preflight from protected mutation authority;
9. backend capability enforcement independent of frontend visibility;
10. retired legacy D3 Python operational promotion authority;
11. permanent workflow semantic validation and canonical secret-consumer/environment checks;
12. terminal machine-readable FC-6 failure audit semantics;
13. production promotion remains impossible during Architecture Re-baseline.

Do not repeat accepted work because an old branch or plan snapshot looks unfinished. Re-read current code and accepted evidence first.

## 4. Why PF-1 -> PF-2 -> PF-3 is binding

PF-1 establishes the single inventory compiler/check/write boundary. PF-2 changes `opsctl`, operator-contract, Actions registration and authority digests, so it must consume PF-1 instead of maintaining the historical Python inventory path.

PF-3 then converts the agreed development architecture into persistent machine enforcement before Functional Closure resumes. Without PF-3, FC-6 and later AR/PC work could reintroduce duplicated authorities, provider leakage, hidden effects, parallel feature-enable paths or permanent legacy shims even though the prose contract exists.

Therefore:

```text
PF-1 owns canonical inventory authority cutover
PF-2 owns reusable hosted evidence
PF-3 owns permanent architecture-fitness enforcement
FC-6 consumes all three
```

Reversing or skipping this order requires new defect evidence plus an explicit update to the canonical program and this plan.

## 5. PF-1 — Canonical Architecture Inventory cutover to `opsctl`

### 5.1 Goal and end state

Make `opsctl` the **single current implementation authority** for deterministic construction, rendering, checking, inspection and bounded local writing of `architecture/inventory.json`.

```text
canonical repository/domain authorities
+ stable repository structure
+ validated canonical lifecycle-derivation result
                ↓
typed Rust architecture inventory model
                ↓
one deterministic compiler
                ↓
architecture/inventory.json
```

Forbidden final state:

```text
Rust generator
+ Python generator
+ historical engine constants
+ monkey patches
+ manual inventory edits
```

### 5.2 Logical layers

```text
architecture/model
architecture/authorities
architecture/inventory/build
architecture/inventory/render
architecture/inventory/check
architecture/inventory/write
cli/lib composition root
```

Physical modules may be coarser when clearer. CLI/parser code does not own domain semantics.

### 5.3 CLI and lifecycle boundary

Target active surface:

```text
opsctl architecture inventory render --lifecycle-json PATH
opsctl architecture inventory check
opsctl architecture inventory check --lifecycle-json PATH
opsctl architecture inventory write --lifecycle-json PATH
opsctl architecture inventory inspect
```

No arbitrary `--output`, stdin authority, hidden state, Git/GitHub/provider invocation or Node/Python/Git child process is allowed.

The canonical lifecycle deriver remains:

```text
.github/scripts/architecture-acceptance.mjs derive
```

Rust consumes and validates its closed/versioned JSON result; Rust does not derive acceptance from Git history/tags.

The old root `opsctl inventory` surface has no compatibility entitlement before production. Retain an alias only if a final repository-wide caller/contract proof demonstrates a legitimate current consumer; otherwise remove it as part of the cutover.

### 5.4 Typed authorities and source ownership

Known schemas must use typed Rust models. Inventory inputs are classified as:

```text
A. derived repository structure
B. existing canonical machine/domain authority
C. intentional static contract only where no authority exists
```

Do not mechanically translate Python registries/constants into Rust registries.

At minimum consume/validate the current authorities for runtime topology, D1 evolution, runtime cutover, release architecture, credentials, credential lifecycle, profile security, operator contract, static architecture program sequence and lifecycle projection policy, plus legitimate workspace/application/runtime/generated-contract/document classification inputs.

`architecture/inventory.json` is never an input authority for the stable/domain facts it projects.

### 5.5 Canonical bytes and effects

PF-1 reuses one neutral Rust canonical JSON/SHA-256 implementation for release metadata, architecture inventory and later PF-2 evidence. No second serializer/digest authority.

`render/check/inspect` are repository read-only. Exactly one bounded effect is added:

```text
GENERATED_PROJECTION_WRITE
```

reachable only through `architecture inventory write` and targeting only:

```text
architecture/inventory.json
```

Write is fail-closed and atomic: validate all inputs -> build in memory -> canonical serialize -> sibling temp -> durable/atomic replace where supported -> read back -> parse/validate -> prove exact bytes.

### 5.6 Lifecycle snapshot semantics

Plain `check` rebuilds stable/domain projection from canonical authorities/repository structure and validates the tracked lifecycle snapshot without treating snapshot freshness as acceptance authority.

`check --lifecycle-json` additionally requires the tracked lifecycle projection to match the supplied canonical derivation.

For the same lifecycle input:

```text
write bytes == render bytes
```

and immediate explicit check must be CURRENT.

### 5.7 Predecessor retirement

Cutover order:

```text
map unique invariants
-> port current semantics
-> positive + negative parity tests
-> switch every current caller/CI gate
-> prove zero current callers
-> prove zero unique current invariants
-> update current historical-executable classification
-> retire/delete predecessor classified DEAD
```

Expected Python predecessor cluster includes `scripts/generate-architecture-inventory.py`, `scripts/generate-architecture-inventory-engine.py` and helpers only when caller/invariant proof classifies them DEAD. Frozen `architecture/python-estate-ar6.json` remains immutable AR-6 provenance; current disposition belongs in current historical-executable debt/overlays.

`opsctl doctor`, repository-root detection, quality workflows and audit workflows must no longer require the retired generator after cutover.

### 5.8 PF-1 proof / DoD

Permanent positive and negative proofs cover deterministic render/write/check, Linux and Windows, malformed/missing authorities, unknown versions, authority ownership drift, lifecycle mismatch, premature architecture/production state, tracked byte/semantic drift, forbidden output path, second lifecycle derivation and reachable retired Python authority.

PF-1 closes only when one exact head proves:

- one current Rust inventory compiler/checker/writer;
- no legitimate Python inventory caller;
- all unique predecessor invariants ported or intentionally retired;
- tracked inventory generated only through native write;
- canonical bytes shared and deterministic;
- bounded effect and operator-contract parity exact;
- Linux/Windows and all applicable protected contexts green;
- `behind_by=0`, reviews/threads clear;
- guarded merge bound to exact head;
- accepted-main reread proves intended tree;
- production remains fail-closed.

Only accepted PF-1 `main` may become the base for PF-2.

## 6. PF-2 — Universal Hosted Operational Evidence primitive

### 6.1 Canonical boundary

```text
GitHub Actions / official provider tools
-> secret-free raw observation
-> opsctl typed policy / validation
-> HostedEvidenceEnvelopeV1
-> immutable GitHub Actions Artifact
-> GitHub Artifact Attestation / custom predicate
```

GitHub issue comments may reference evidence but are not evidence store/signing/policy authority.

### 6.2 Ownership and command surface

`opsctl` owns typed evidence schemas/versions, shared canonicalization/digests, secret/material rejection, environment/effect policy and deterministic offline `build|validate|inspect|verify`. It has no provider/GitHub network authority.

Actions/Environments own orchestration/OIDC/credential exposure/run identity. Official provider tools own live observation. GitHub Artifact is the immutable subject transport; GitHub Artifact Attestation owns signing/provenance binding.

Do not create a second evidence DB, queue, daemon, scheduler, signer, PKI or per-feature reporting-workflow family.

Initial typed payload families remain small/versioned, including credential readiness, hosted resource state and release-set transition where still valid after PF-1 rebase.

### 6.3 Security / proof matrix

PF-2 must fail closed for unknown schema/kind/version/fields, recursive secret-bearing fields, obvious secret material, customer/mail/browser/profile payloads, malformed/oversized/noncanonical input, wrong repo/source/ref/workflow/run/environment/effect context, invalid transition combinations, extra artifact files and attestation subject mismatch.

The reusable publisher accepts no provider secret inheritance, independently verifies exact evidence bytes with `opsctl`, then attests those bytes with a pinned official GitHub primitive. Caller/callee reusable-workflow context semantics must be directly revalidated, not assumed.

PF-2 closes only when one reusable typed evidence path exists, responsibilities are separated, negative matrix is permanent, operator-contract/CLI/inventory parity is produced through PF-1 and production remains fail-closed.

Only accepted PF-2 `main` may become the base for PF-3. **PF-2 acceptance does not authorize FC-6 resume.**

## 7. PF-3 — Architecture Fitness Baseline

PF-3 #431 makes the Architecture Evolution Quality Contract mechanically persistent before FC-6.

Canonical machine target:

```text
architecture/architecture-fitness-policy.json
```

It stores rule IDs, applicability/severity/status and one primary permanent enforcement owner per rule. It does not duplicate domain facts from existing authorities.

Initial REQUIRED families include:

```text
AF-AUTH    authority uniqueness
AF-DEP     dependency direction / bounded contexts
AF-EFFECT  explicit side effects
AF-TYPE    typed IDs/state/contracts
AF-CAP     production capability admission
AF-PERSIST persistence/migration ownership
AF-CONFIG  typed configuration boundary
AF-EVENT   domain/integration event discipline
AF-LEGACY  cutover and predecessor retirement
AF-OPS     opsctl boundary
AF-READ    developer readability
```

A REQUIRED rule without active/reachable machine enforcement is itself a gate failure. Permanent positive and negative fixtures must prove the gate and its own enforcement registry fail closed.

After PF-3, every materially architecture-changing PF/FC/AR/PC candidate must declare Architecture Impact covering contexts, authorities, contracts, effects, execution surfaces, activation units/release profiles, schema/migrations, legacy disposition and affected fitness Rule IDs.

PF-3 closes only when its versioned policy, enforcement mapping, positive/negative fixtures and permanent Architecture Fitness Gate are accepted on exact-head `main` with production behavior unchanged.

Only accepted PF-3 `main` allows the fresh #399/#421 re-baseline and FC-6 resume.

## 8. Resume gate — fresh re-baseline after PF-3

Before any further FC-6 ceremony step:

1. reread exact protected `main` after PF-3;
2. reread #399/#421, open PRs/issues and current hosted state;
3. rediscover protected required contexts and applicable workflows;
4. verify live Actions registry equals canonical registry;
5. verify accepted-main durable Release Set publication remains observable through PF-2 evidence where applicable;
6. verify staging observe credential readiness through canonical credential authority + Hosted Evidence;
7. request externally issued missing observe credential/metadata if required; never widen/reuse deploy credential as shortcut;
8. verify PF-3 fitness policy/gate is current and no REQUIRED rule is unenforced;
9. verify no AR-12 implementation entered source;
10. verify production fail-closed invariants.

Only then update #399/#421 and resume FC-6.

## 9. FC-6 — Real staging same-bits / rollback rehearsal

FC-6 is the existing AR-11 Functional Closure staging proof, not AR-12 fresh-environment provisioning.

Use already supported staging resources and immutable accepted Release Sets only.

```text
A = older accepted-main durable Release Set
B = newer accepted-main durable Release Set
```

Required proof:

1. A/B resolve to exact durable immutable release assets;
2. both sources are accepted protected-main history;
3. `release verify A/B` are VALID;
4. observe current staging using least-privilege observe credential;
5. target and rollback-known-good compatibility use the same canonical evaluator;
6. staging A -> B plan/preflight is READY or fails closed for typed reason;
7. protected executor uses exact B bytes with no rebuild;
8. post-deploy `promotion verify B` = VERIFIED;
9. second B plan = NO_CHANGE;
10. A is reevaluated against post-B observed state;
11. if compatible, B -> A uses original durable A bytes through the same workflow;
12. post-rollback `promotion verify A` = VERIFIED;
13. second A plan = NO_CHANGE;
14. incompatible/UNKNOWN rollback blocks before mutation;
15. stale provider state between preflight/executor trips expected-current fence;
16. applicable evidence is captured through PF-2;
17. applicable architecture/effect/capability rules remain green through PF-3;
18. production remains untouched.

If no naturally compatible A/B pair exists, closure may record correct policy BLOCKED evidence; compatibility must never be falsified merely to complete ceremony.

## 10. FC-7 — Final whole-AR-11 functional audit

After FC-6, audit current protected `main` across:

### Release/promotion
- one canonical Release Set model;
- historical accepted-source authority, immutable publication and exact bytes;
- all locally provable release-critical identities verified;
- deterministic plan, NO_CHANGE, stale fence and same-environment serialization;
- no rebuild;
- least-privilege observe/deploy separation;
- rollback compatibility from current observed state;
- incompatible/UNKNOWN blocks before mutation;
- post-deploy VERIFIED only success state.

### Inventory/tooling
- one current `opsctl` inventory compiler/checker/writer;
- zero current legacy Python generator caller;
- deterministic/idempotent projection;
- one canonical serializer/digest implementation;
- singular lifecycle derivation;
- exact operator-contract/CLI/inventory parity.

### Hosted evidence
- one reusable typed evidence primitive;
- observation/signing/policy responsibilities separated;
- attested exact bytes independently verifiable;
- no secret material or second evidence backend.

### Architecture fitness
- PF-3 policy/gate accepted and current;
- no REQUIRED rule without enforcement;
- affected negative fixtures green;
- no duplicate authority, provider leakage, hidden effect, second production-enable path or reachable DEAD predecessor in closure-touched scope;
- Architecture Impact discipline ready for AR-12 onward.

### Capability isolation
- source-present disabled capabilities backend-inexecutable;
- frontend remains projection only;
- no independent production feature flags.

### Behavioural certification / governance
- original mandatory AR-11 30-case behavioural matrix plus closure regressions 31–37 map 1:1 to permanent behavioural tests/gates;
- Linux and Windows native `opsctl` tests;
- all applicable workflows/protected contexts green on exact candidates;
- guarded merges bind exact expected heads;
- accepted-main evidence directly observable where required;
- inaccessible evidence is UNPROVEN, never implicit success.

Classify final findings as `P0 | P1 | P2 | P3 | NOT_A_DEFECT | LATER_SLICE_BY_DESIGN`.

Functional Closure is complete only when:

```text
P0 = 0
P1 = 0
P2 = 0 for AR-11 Functional Closure scope
PF-1 = ACCEPTED AND VERIFIED
PF-2 = ACCEPTED AND VERIFIED
PF-3 = ACCEPTED AND VERIFIED
all mandatory AR-11 behavioural requirements = PROVED
FC-6 staging proof = PROVED or correctly BLOCKED by accepted compatibility policy
production_mutation = false
AR-12 implementation mixed into closure = false
```

## 11. Canonical ownership map

| Concern | Canonical owner / boundary |
| --- | --- |
| Program/lifecycle order | Architecture Re-baseline authority + Git-derived acceptance |
| Cross-cutting evolution rules | `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md` |
| Fitness enforcement specification | `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431 |
| Fitness machine policy after PF-3 | `architecture/architecture-fitness-policy.json` |
| Architecture inventory model/compiler/check/write | `tools/opsctl` PF-1 architecture module |
| Architecture inventory tracked projection | `architecture/inventory.json` |
| Inventory predecessor | Python inventory cluster, retired after invariant/caller proof |
| Operator command/effect authority | `architecture/operator-contract.json` |
| Canonical JSON / digest | shared neutral Rust primitive |
| Hosted evidence typed envelope | `tools/opsctl` evidence module |
| Hosted evidence orchestration | one reusable GitHub Actions publisher |
| Evidence transport | immutable GitHub Actions Artifact |
| Evidence signing/provenance | GitHub Artifact Attestation / official action |
| Provider observation | official provider tools under least privilege |
| Release Set model | existing `tools/opsctl/src/release/**` |
| Promotion plan/preflight/verify | existing `tools/opsctl/src/promotion/**` |
| Durable build/publish | canonical Release Set Build workflow |
| Staging mutation executor | canonical protected staging promotion path |
| FC live tracker | issue #399 |
| FC-6 hardening/readiness | issue #421 |

Python remains valid for separately classified validators/tests/fixtures/collection adapters when it does not duplicate a concern explicitly cut over to another authority. PF-1 is a bounded inventory cutover, not a global Python-to-Rust rewrite.

## 12. Testing and stage-gate discipline

Every bounded PF/FC implementation includes positive and negative evidence in the same candidate. Use the lowest layer that can prove the requirement:

```text
pure Rust unit tests
-> Rust filesystem/integration tests
-> repository architecture/fitness policy tests
-> workflow semantic/static tests
-> exact-head GitHub Actions
-> accepted-main hosted observation
-> real staging proof only for inherently hosted/provider state
```

For authority/security transitions, prove explicitly that the predecessor/forbidden path can no longer execute.

Before each merge:

1. fresh protected-main re-baseline;
2. current plan/tracker/competing-PR reread;
3. bounded Architecture Impact where PF-3 applies;
4. permanent positive + negative tests;
5. all applicable workflows green on exact head;
6. all protected required contexts green on exact head;
7. all REQUIRED fitness rules green when PF-3 applies;
8. `behind_by=0`;
9. blocking reviews=0;
10. unresolved threads=0;
11. guarded merge bound to exact expected head;
12. accepted-main reread;
13. candidate-tree/accepted-tree proof where canonical acceptance requires it;
14. direct hosted evidence read where required;
15. any new candidate commit invalidates previous exact-head evidence.

## 13. Final Definition of Done — AR-11 Functional Closure 10/10

One current accepted repository state must simultaneously prove:

### Architecture/developer system
- target architecture/evolution contract accepted in `main`;
- PF-3 fitness baseline accepted and machine-enforced;
- `opsctl` is the sole current architecture inventory compiler/checker/writer;
- no parallel Python inventory authority;
- typed layers separated from CLI/adapters;
- deterministic render/check/write;
- explicit bounded generated-file effect;
- singular lifecycle derivation;
- no hidden/parallel authority introduced.

### Hosted operational evidence
- one reusable typed/versioned Hosted Evidence primitive;
- immutable attested subject bytes;
- local policy and hosted provenance verification separated;
- no secret-bearing evidence or hidden backend.

### Release/promotion/rollback
- historical accepted Release Sets remain valid after `main` advances;
- non-main/unaccepted sources reject;
- durable assets contain all required verification bytes;
- same ID/different bytes is fatal;
- no rebuild on promotion;
- NO_CHANGE and concurrency/fencing work;
- observe/deploy credentials separated;
- compatible historical release can promote/rollback from original bytes;
- incompatible/UNKNOWN rollback fails closed;
- post-deploy verification converges to VERIFIED then NO_CHANGE;
- production unreachable.

### Capability isolation
- `source_present=true` with `production_enabled=false` is mechanically demonstrable;
- disabled HTTP/queue/scheduled/service/outbound paths cannot produce forbidden side effects;
- frontend cannot bypass backend capability admission.

### Evidence/audit
- AR-11 30-case matrix + regressions 31–37 permanently mapped;
- PF-1, PF-2 and PF-3 negative matrices permanent;
- Linux/Windows suites pass;
- FC-6 hosted same-bits/rollback evidence exists or correctly records policy BLOCKED;
- final audit P0=0/P1=0/P2=0 for closure scope.

Final state remains:

```text
AR-11 = historically ACCEPTED + functionally CLOSED
AR-12 = derived current / implementation NOT STARTED during this plan
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

Only after this Definition of Done is mechanically proven may AR-11 be described as fully functional / 10/10 closed and AR-12 implementation entry be considered separately under the canonical program.
