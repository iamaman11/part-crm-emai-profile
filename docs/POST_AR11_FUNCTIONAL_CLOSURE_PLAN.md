# Post-AR-11 Functional Closure Plan — Release / Promotion Contract to 10/10

**Document status:** SUBORDINATE_REMEDIATION_PLAN  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Architecture evolution contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**PF-1 detailed specification:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**PF-3 enforcement specification:** `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`  
**Accepted AR-11 design:** `docs/ARCHITECTURE_REBASELINE_V3_AR11.md`  
**Live execution tracker:** issue #399  
**Current FC-6 implementation/hardening tracker:** issue #421  
**PF-1 tracker:** issue #430  
**PF-2 candidate:** Draft PR #428  
**PF-3 tracker:** issue #431  
**Scope:** close residual functional gaps in the already accepted AR-11 release-set / promotion architecture and establish the reusable tooling/evidence/fitness foundations required before AR-12 implementation  
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
PF-1  Canonical Architecture Inventory + lifecycle-policy cutover  #430
  ->
PF-2  Universal Hosted Operational Evidence primitive              Draft PR #428
  ->
PF-3  Architecture Fitness Baseline                                #431
  ->
fresh re-baseline #399 / #421
  ->
FC-6 real staging same-bits / rollback rehearsal
  ->
FC-7 final whole-AR-11 functional audit
  ->
AR-12 implementation entry under the canonical program
```

PF-1/PF-2/PF-3 are **Functional Closure prerequisites**, not AR-11A, AR-11.5 or AR-12 slices. They do not alter `architecture/architecture-program-sequence.json`, reopen accepted AR-11 history or authorize production. No PF-4 or other planning phase is introduced.

No path in this plan may skip PF-3 and resume FC-6 directly after PF-2.

Issue #375 is closed/completed historical hardening. It is not a current blocker or lifecycle authority. Current machine files that still encode #375 or superseded acceptance mechanics are legacy current implementation debt owned by PF-1 cutover; they are not target architecture and never authorize AR-12.

## 2. Non-negotiable architecture and production invariants

```text
one protected main
one architecture hierarchy
one application/source history
one data/schema compatibility history
one canonical capability/release hierarchy
source_present != production_enabled
build once -> promote same bits
GitHub Actions/Environments = orchestration + approvals + credential + observation boundary
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
Observation facts separated from policy decisions
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
- make Rust invoke Git, `gh`, GitHub APIs, Node, Python or provider APIs to derive lifecycle state;
- feed Rust an already-interpreted legacy `.mjs derive` result as authoritative lifecycle truth;
- leave Node and Rust as two legitimate lifecycle/acceptance authorities after cutover;
- add `promotion execute` or provider mutation authority to `opsctl`;
- give `opsctl` GitHub/Cloudflare/provider network authority for convenience;
- rebuild application artifacts during promotion;
- introduce Terraform/generic IaC state;
- enable Production Core or mailbox capabilities;
- start AR-12;
- absorb AR-14 recovery, AR-15 Windows updater/signing or AR-17 production authorization into this closure work;
- retain compatibility shims without a proved current consumer or explicit accepted contract;
- weaken a gate, policy rule or negative fixture to obtain green CI.

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

At the latest accepted Functional Closure baseline, repository-side FC-6 hardening and hosted Actions registry reconciliation are already merged foundations. Live active Actions registry equality, durable accepted-main Release Set publication and canonical credential/readiness evidence must be re-observed after each prerequisite rather than trusted from an old SHA. Existing deploy/bootstrap credentials must never be widened or reused as a substitute for a correctly scoped staging observation credential.

Do not repeat accepted work because an old branch or plan snapshot looks unfinished. Re-read current code and accepted evidence first.

## 4. Why PF-1 -> PF-2 -> PF-3 is binding

PF-1 establishes the single typed architecture inventory and lifecycle/acceptance policy boundary. PF-2 changes `opsctl`, operator-contract, Actions registration and authority digests, so it must consume accepted PF-1 rather than maintaining historical Python inventory or Node lifecycle-policy paths.

PF-3 then converts the agreed development architecture into persistent machine enforcement before Functional Closure resumes. Without PF-3, FC-6 and later AR/PC work could reintroduce duplicated authorities, provider leakage, hidden effects, parallel feature-enable paths or permanent legacy shims even though the prose contract exists.

Therefore:

```text
PF-1 owns canonical inventory + lifecycle/acceptance implementation cutover
PF-2 owns reusable hosted evidence
PF-3 owns permanent architecture-fitness enforcement
FC-6 consumes all three
```

Reversing or skipping this order requires new defect evidence plus an explicit update to the canonical program and this plan.

## 5. PF-1 — Canonical Architecture Inventory + Lifecycle Policy cutover to `opsctl`

Detailed PF-1 requirements are owned by `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` together with issue #430. If older PF-1 wording conflicts with that detailed specification, the detailed specification is the current subordinate PF-1 contract under this plan.

### 5.1 Goal and end state

Make `opsctl` the **single current implementation authority** for:

- typed architecture acceptance/lifecycle policy evaluation over explicit raw observations;
- deterministic construction, rendering, checking and inspection of architecture inventory;
- exactly one bounded local `GENERATED_PROJECTION_WRITE` to `architecture/inventory.json`.

Target architecture:

```text
Git / GitHub / repository checkout
        ↓
outer observation adapters
        ↓
RawArchitectureAcceptanceEvidenceV1
        ↓
typed Rust validation
        ↓
pure AcceptancePolicy / LifecycleEvaluator
        ↓
DerivedLifecycleStateV1
        ↓
ValidatedAuthoritySet
        ↓
pure ArchitectureInventoryCompiler
        ↓
canonical render/check/inspect
        ↓
GENERATED_PROJECTION_WRITE only when explicitly requested
```

Forbidden final state:

```text
Rust lifecycle evaluator + Node lifecycle evaluator both authoritative
Rust inventory compiler + Python generator both authoritative
legacy engine constants copied into a new god-validator
manual inventory edits
opsctl -> git/gh/network/provider subprocesses
```

`architecture/inventory.json` remains a tracked generated projection. It does not become lifecycle acceptance authority or input authority for the stable/domain facts it projects.

### 5.2 Observation shell and pure core

GitHub Actions/repository adapters own observation effects. They may use existing Git/GitHub mechanisms to collect exact SHAs/trees/parents, annotated acceptance tags, check runs, workflow runs and review/thread observations.

They do **not** decide accepted/current lifecycle state.

Rust receives a closed/versioned raw observation contract and evaluates policy deterministically. It does not execute:

```text
git
gh
GitHub API
Node
Python
provider API
```

The static AR order remains owned by `architecture/architecture-program-sequence.json`; the Rust evaluator consumes that authority rather than inventing another sequence.

### 5.3 Corrected lifecycle / acceptance boundary

The previous rule that `.github/scripts/architecture-acceptance.mjs derive` must remain the canonical upstream deriver is **superseded by this audited PF-1 contract**.

The current Node file is a policy engine, not a unique observer. The current audit proves `architecture-acceptance-recorder.yml` already obtains raw Git/GitHub observations directly. Therefore target lifecycle flow is:

```text
architecture/architecture-program-sequence.json
+ current versioned architecture acceptance policy
+ RawArchitectureAcceptanceEvidenceV1
        ↓
pure Rust LifecycleEvaluator
        ↓
DerivedLifecycleStateV1
```

PF-1 must audit and correctly migrate still-valid acceptance semantics, including immutable acceptance metadata, exact-head evidence, candidate/merge-tree identity, first-parent/base proof, accepted-main reread and fail-closed production state.

PF-1 must also explicitly remove superseded current policy assumptions such as stale #375 ownership and `merge_method = squash` where they conflict with the accepted guarded merge-commit discipline. No policy field is changed merely to make CI pass.

### 5.4 Atomic authority cutover — no dual current implementation

Temporary candidate parity is allowed; dual legitimacy is not.

```text
BEFORE
Node = accepted lifecycle/acceptance implementation
Python = accepted inventory implementation

CANDIDATE BUILD/PARITY
Rust evaluators exist as non-authoritative candidate code

CUTOVER
all current callers switch atomically
policy/projection references switch
Rust becomes sole current implementation

AFTER
architecture-acceptance.mjs caller count = 0
architecture-acceptance.mjs unique-current-invariant count = 0
architecture-acceptance.mjs = DEAD -> DELETE
Python inventory predecessor caller count = 0
DEAD Python inventory predecessors -> DELETE
```

The current audit found no unique raw-observation capability requiring a retained Node observer; target disposition for `.github/scripts/architecture-acceptance.mjs` is therefore **DELETE**, not KEEP_AS_OBSERVER.

Known current callers to re-prove and switch include governance gate, acceptance recorder, Python inventory generator and lifecycle-projection policy references. Exact caller discovery is repeated on the candidate head before deletion.

### 5.5 Pure inventory core and domain-validation boundary

PF-1 separates repository adapters from pure compilation:

```text
RepositoryAuthorityLoader
        ↓
ValidatedAuthoritySet
        +
DerivedLifecycleStateV1
        ↓
PureInventoryBuilder
        ↓
ArchitectureInventory
        ↓
CanonicalRenderer
```

Inventory compilation must not become a monolithic copy of every AR/domain validator.

Responsibility split:

| Layer | Responsibility |
| --- | --- |
| Domain/machine authority validator | Full semantics owned by that bounded authority |
| Inventory compiler | kind/version/identity/ownership/reference and cross-authority consistency needed for projection |
| Inventory projection | deterministic readable representation with no independent semantic authority |

This boundary must be covered by tests so a future authority is added through a typed adapter/contract rather than by growing a central static registry.

### 5.6 CLI and effects

Target inventory surface remains:

```text
opsctl architecture inventory render [explicit typed observation/lifecycle input]
opsctl architecture inventory check
opsctl architecture inventory check [explicit typed observation/lifecycle input]
opsctl architecture inventory write [explicit typed observation/lifecycle input]
opsctl architecture inventory inspect
```

Exact final argument spelling is subordinate to the versioned contract. No arbitrary `--output`, hidden stdin authority, hidden state, Git/GitHub/provider invocation or Node/Python/Git child process is allowed.

The old root `opsctl inventory` surface has no compatibility entitlement before production. Retain an alias only if final repository-wide caller/contract proof demonstrates a legitimate current consumer; otherwise remove it as part of cutover.

### 5.7 Canonical bytes, atomic write and diagnostics

PF-1 reuses one neutral Rust canonical JSON/SHA-256 implementation for release metadata, architecture/lifecycle evidence, architecture inventory and later PF-2 evidence. No second serializer/digest authority.

Repeated render with identical validated inputs is byte-identical. Pure core must be directly testable from typed snapshots without requiring a giant fixture repository.

`render/check/inspect` are read-only. Exactly one bounded effect is added:

```text
repository_workspace_effect = GENERATED_PROJECTION_WRITE
```

reachable only through `architecture inventory write` and targeting exactly `architecture/inventory.json`.

`write` is fail-closed: validate all inputs -> build complete output in memory -> canonical serialize -> safe sibling staging/atomic replacement on supported platform -> read back -> parse/validate -> prove bytes identical. Failure before activation leaves the previous tracked inventory intact.

Plain `check` rebuilds stable/domain projection from canonical authorities/repository structure and validates tracked lifecycle snapshot without treating freshness as acceptance authority. Explicit synchronization/check consumes the typed raw-observation/lifecycle contract and requires projection match.

Diagnostics should identify JSON path/field, tracked value, expected value, owning authority/projection family and `CURRENT | DRIFTED | INVALID` without mutation.

### 5.8 PF-1 positive proofs

At minimum:

```text
current accepted repository -> authority loading succeeds
valid raw acceptance observations -> deterministic lifecycle result
current target guarded merge identity -> accepted by new evaluator
pure lifecycle evaluation repeated -> identical result
pure inventory build from typed snapshot -> deterministic
render twice -> byte-identical
write -> tracked file exactly equals render bytes
write twice -> idempotent / byte-identical
check immediately after write -> CURRENT
plain check tolerates only permitted non-authoritative lifecycle staleness
Linux -> pass
Windows -> pass
post-cutover governance gate/acceptance recorder -> Rust policy path
repository-wide old Node/Python caller scan -> zero
```

### 5.9 PF-1 negative proofs

At minimum reject:

```text
malformed/unknown raw acceptance observation schema
unknown/duplicate acceptance tag observation
tag/path/record slice mismatch
non-contiguous acceptance after a gap
wrong tag target / merge identity
candidate tree != merge tree where current policy requires equality
first-parent/base mismatch
incomplete/failed exact-head checks or applicable workflows
blocking review/unresolved thread evidence
stale accepted-main reread observation
interpreted upstream document attempting to declare accepted/current state as raw fact
missing required authority
malformed JSON
unknown authority kind/version
wrong authority ownership/status
lifecycle accepted/current successor mismatch
architecture_complete=true before owning stage
production_core_gate=AUTHORIZED before owning stage
production_ready=true before PC-1
production_mutation=true
one-byte tracked inventory drift
semantically changed tracked inventory
noncanonical bytes where canonicality is required
attempt to write any path other than architecture/inventory.json
arbitrary --output
old .mjs caller reachable after declared cutover
retired Python generator still reachable after declared cutover
dual current lifecycle implementation
process/network/Git/GitHub/provider access introduced into Rust lifecycle/inventory path
silent acceptance-policy weakening to satisfy a fixture
```

### 5.10 PF-1 Definition of Done

PF-1 closes only when one exact head proves:

- one current Rust lifecycle/acceptance evaluator over typed raw observations;
- one current Rust inventory compiler/checker/writer;
- no legitimate Node lifecycle-policy caller and `.github/scripts/architecture-acceptance.mjs` deleted after zero-caller/zero-unique-invariant proof;
- no legitimate Python inventory caller and every DEAD inventory predecessor deleted;
- current acceptance/lifecycle policy no longer encodes superseded #375/squash assumptions as target semantics;
- domain validation remains with specialized bounded owners instead of a god inventory validator;
- pure builder/repository adapters separated and directly tested;
- canonical bytes shared and deterministic;
- bounded effect and operator-contract parity exact;
- Linux/Windows and all applicable workflows/protected contexts green;
- `behind_by=0`, blocking reviews=0, unresolved threads=0;
- guarded merge bound to exact head;
- accepted-main reread proves intended tree/authority state;
- production remains fail-closed.

Only accepted PF-1 `main` may become the base for PF-2.

## 6. PF-2 — Universal Hosted Operational Evidence primitive

### 6.1 Goal and canonical boundary

Provide one reusable evidence architecture for hosted/provider observations needed by AR-11 Functional Closure and later operational slices without per-feature evidence frameworks.

```text
GitHub Actions / official provider tools
-> secret-free raw observation
-> opsctl typed policy / validation
-> HostedEvidenceEnvelopeV1
-> immutable GitHub Actions Artifact
-> GitHub Artifact Attestation / custom predicate
```

GitHub issue comments/tracker text may reference evidence but are not evidence store, signing authority or policy authority.

### 6.2 Ownership split

```text
GitHub Actions / Environments
  orchestration, approvals, OIDC, credential exposure, immutable run identity

official provider tooling
  live observation and explicitly authorized provider execution

opsctl
  typed evidence schemas/versions
  shared canonicalization/digests
  secret/material rejection
  environment/effect policy
  deterministic build/inspect/validate/verify
  NO provider/GitHub network authority

Actions Artifact
  immutable evidence subject transport

GitHub Artifact Attestation
  signing / provenance binding
```

Do not build a second evidence DB, queue, scheduler, daemon, signer, PKI, report service or feature-specific reporter workflow family.

### 6.3 Command/payload contract

Target active surface after PF-1 rebase:

```text
opsctl evidence build
opsctl evidence validate
opsctl evidence inspect
opsctl evidence verify
```

All remain offline and non-provider-mutating.

Preserve a small versioned sum type rather than arbitrary evidence bags. Initial candidate payload families include:

```text
credential_readiness v1
hosted_resource_state v1
release_set_transition v1
```

Future kinds normally extend the typed variant set and reuse the same envelope/publication path.

### 6.4 PR #428 rebase rule

PR #428 is implementation material but is not merge-authorized before PF-1.

After PF-1 acceptance:

1. reread #428 against new accepted `main`;
2. rebase/reimplement only still-valid Hosted Evidence changes;
3. remove manual/stale inventory maintenance;
4. update operator-contract and regenerate/check inventory only through PF-1 commands;
5. retain no bridge to retired Python inventory or Node lifecycle-policy generation;
6. directly revalidate reusable-workflow GitHub context semantics;
7. complete Rust/workflow fail-closed matrices;
8. treat all old #428 CI evidence as obsolete after head/base changes.

### 6.5 Formal trust model

PF-2 must document and mechanically preserve what each layer proves and does **not** prove:

| Layer | Proves | Does not prove |
| --- | --- | --- |
| Provider observation | What the authorized observer received from the provider at observation time | Independent truth of provider state or GitHub run identity |
| Typed envelope | Closed schema, canonical context and policy compliance | Truthfulness of an arbitrary payload source |
| Artifact digest | Exact subject-byte identity | Observer authority |
| GitHub Artifact Attestation | Binding of exact subject bytes to hosted workflow identity/provenance | Business-semantic correctness of the provider state |
| `opsctl evidence verify` | Locally checkable schema/context/policy/digest constraints | Independent provider truth when no trusted expected observation exists |

No UI, issue comment or attestation may be described as proving more than its layer actually establishes.

### 6.6 Freshness, replay and mutation eligibility

Hosted Evidence validity and eligibility for a later mutation are distinct.

Versioned evidence must bind, as applicable:

```text
observed_at
issued_at
repository
source SHA/ref
workflow identity/version
run id/attempt
environment
subject identity/digest
authority/policy version
observation scope
fence/current-state identity
freshness policy/window
replay policy
```

Canonical validity outcome must distinguish at least:

```text
VALID
VALID_BUT_STALE
INVALID
```

For any decision that may authorize mutation:

```text
VALID_BUT_STALE -> BLOCKED
INVALID -> BLOCKED
```

Freshness is policy-owned and environment/observation-specific; it is not an arbitrary CLI timeout. Replay of a cryptographically valid but superseded observation must be rejected when the owning decision requires current state.

### 6.7 Reusable publisher requirements

The publisher must:

- accept no provider secret inheritance;
- download exactly one expected evidence subject;
- independently reconstruct expected repository/source/ref/workflow/run/environment/effect context from trusted metadata/explicit caller inputs;
- run `opsctl evidence verify` before signing;
- attest exactly the verified bytes with a pinned official GitHub attestation primitive;
- use minimal `contents: read`, `id-token: write`, `attestations: write` permissions only where required;
- perform no provider mutation;
- perform no production enablement;
- remain one reusable publication primitive rather than a feature-specific family.

Reusable-workflow semantics for `github.sha`, `github.ref`, workflow identity, run id/attempt and caller-vs-called workflow identity must be proven against current GitHub behavior rather than assumed from memory.

### 6.8 Evidence security rules

Reject/exclude:

- secret/token/password/private-key values;
- secret-bearing unknown field names;
- customer/mail/browser/profile contents;
- fingerprint raw material;
- arbitrary provider mutation claims inconsistent with environment/effect policy;
- unknown schema/kind/payload versions;
- unexpected fields in closed schemas;
- malformed/oversized inputs;
- context mismatch between envelope and independently reconstructed expected context;
- impossible timestamps/future observations;
- stale evidence presented as mutation-eligible;
- replayed evidence whose fence/current-state identity no longer matches.

`opsctl evidence verify` proves local schema/canonical/context policy. Artifact Attestation proves subject-byte provenance/tampering resistance. Local verification alone must not be misrepresented as independent semantic truth for an arbitrary valid payload when no trusted expected payload identity exists.

### 6.9 PF-2 negative matrix

At minimum:

```text
unknown evidence kind -> reject
unknown payload version -> reject
unknown top-level field -> reject
recursive secret-bearing field -> reject
obvious secret material -> reject
malformed input -> reject
oversized input -> reject
noncanonical evidence bytes -> reject
wrong repository -> reject
wrong source SHA/ref -> reject
wrong workflow identity -> reject
wrong run id/attempt -> reject
wrong observation job -> reject
wrong environment -> reject
wrong effect flags -> reject
production effect-policy violation -> reject
invalid release transition decision/effect combination -> reject
wrong CLI options -> reject
unsupported/duplicate CLI arguments -> reject
artifact with extra files -> publisher reject
attestation subject differs from verified bytes -> verification failure
future/impossible observed_at or issued_at -> reject
valid-but-stale evidence used for mutation admission -> BLOCKED
replayed evidence with stale fence/current-state identity -> BLOCKED
```

### 6.10 PF-2 Definition of Done

PF-2 closes only when:

- one reusable Hosted Operational Evidence envelope/publication architecture exists;
- all supported payloads typed/versioned/fail-closed;
- formal trust model is documented and tested at relevant boundaries;
- freshness/replay semantics and `VALID | VALID_BUT_STALE | INVALID` are implemented where required;
- canonical JSON/digest shared with PF-1/release policy;
- `opsctl` remains offline/no-provider/no-secret/no-production mutation authority;
- publisher has no provider credentials;
- observation and signing authorities remain separated;
- reusable-workflow context binding has direct proof;
- negative matrix is permanent;
- official attestation verification is demonstrated for a non-secret subject where hosted proof is required;
- operator-contract/CLI/inventory parity uses PF-1 mechanisms;
- no second evidence framework/backend/PKI/signer exists;
- applicable workflows/protected contexts green on one exact head;
- guarded merge and accepted-main reread succeed;
- AR-12 remains not started and production remains fail-closed.

Only accepted PF-2 `main` may become the base for PF-3. **PF-2 acceptance does not authorize FC-6 resume.**

## 7. PF-3 — Architecture Fitness Baseline

PF-3 #431 makes the Architecture Evolution Quality Contract mechanically persistent before FC-6. Detailed requirements remain in `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`.

Canonical machine target:

```text
architecture/architecture-fitness-policy.json
```

It records rule identity, applicability, severity/status and one primary permanent enforcement owner. It does not duplicate domain facts from existing authorities.

Initial REQUIRED families:

```text
AF-AUTH    authority uniqueness / no second lifecycle or enablement authority
AF-DEP     inward dependencies / bounded-context and runtime->opsctl prohibition
AF-EFFECT  explicit DB/provider/fs/process/network/deployment effects
AF-TYPE    typed critical IDs, lifecycle state and versioned contracts
AF-CAP     source_present != production_enabled and Release Profile admission
AF-PERSIST context-owned persistence / migration authority
AF-CONFIG  typed validated configuration at composition edges
AF-EVENT   domain vs versioned integration-event discipline
AF-LEGACY  zero callers + zero unique invariants + DEAD deletion
AF-OPS     opsctl no hidden state/provider executor role
AF-READ    developer-readable owner/entry/authority/effect/gate mapping
```

### 7.1 Enforcement architecture

```text
canonical authorities
+ architecture-fitness-policy.json
+ repository/source graph
        ↓
existing specialized validators + bounded missing checks
        ↓
positive + negative fixtures
        ↓
Architecture Fitness Gate
```

Do not create a generic linter framework. Reuse specialized validators where they already own a rule. One fitness Rule ID has one primary enforcement owner.

The gate fails when:

- REQUIRED rule has no enforcement;
- declared enforcement is missing/unreachable;
- required negative fixture unexpectedly passes;
- authority/rule IDs are duplicated/unknown;
- a REQUIRED rule is silently downgraded/removed;
- candidate introduces prohibited dependency/effect/production-enable path.

### 7.2 Fitness-policy evolution / anti-weakening

A fitness policy must not be weakenable by editing the policy itself.

Changes such as the following require an explicit versioned supersession/migration record and permanent anti-weakening proof:

```text
remove REQUIRED rule
REQUIRED -> weaker status
narrow applicability
replace primary checker
remove required negative fixture
weaken expected fail-closed result
retire a rule without a successor/justification where the invariant still applies
```

A supersession record must identify at least:

```text
old_rule
successor_rule or explicit retirement reason
reason
compatibility/security impact
owning authority/slice
accepted source
```

The gate fails closed on silent weakening.

### 7.3 Architecture budgets

PF-3 must establish a small machine-readable/measurable budget set that protects usability as well as correctness. Exact thresholds are chosen from measured current baselines rather than invented numbers, but the policy must cover at least:

- semantic authority count per owned fact: exactly 1;
- mutation executor count per owned operation: exactly 1;
- REQUIRED fitness rules without active enforcement: 0;
- REQUIRED rules requiring a negative fixture but lacking one: 0;
- unclassified external effect classes in protected scope: 0;
- hidden production-enable authorities outside Release Profile: 0;
- deterministic Linux/Windows result equivalence for cross-platform `opsctl` policy;
- bounded local `opsctl`/required PR CI runtime budgets derived from an observed baseline;
- bounded Hosted Evidence subject size/freshness policy where PF-2 owns those values.

Performance budgets are guardrails, not an excuse to weaken correctness. A threshold change is itself governed and justified by observed evidence.

### 7.4 PF-3 negative baseline

At minimum reject fixtures with:

```text
duplicate canonical owner
provider SDK import in protected pure scope
product/runtime dependency on opsctl
unauthorized filesystem/network/process/provider mutation
second production-enable registry/flag
execution surface with unknown activation unit
enabled release profile with incomplete dependency closure
unversioned required external/integration contract
forbidden cross-context persistence mutation
REQUIRED rule with missing enforcement
REQUIRED rule silently downgraded or applicability narrowed
required negative fixture removed
reachable DEAD predecessor after cutover
hidden operator state/provider executor authority
```

Linux is mandatory; Windows is mandatory where checker/operator behavior is cross-platform.

### 7.5 Architecture Impact after PF-3

Every later materially architecture-changing PF/FC/AR/PC candidate declares:

```text
bounded contexts touched
authorities touched
public/persisted/integration contracts touched
effect classes added/changed
execution surfaces added/changed
activation units / release profiles affected
schema/migration impact
legacy predecessor disposition
applicable fitness Rule IDs
```

`none` is valid only when justified by the diff.

### 7.6 PF-3 Definition of Done

PF-3 closes only when:

- versioned fitness policy exists;
- every initial REQUIRED rule has one primary reachable enforcement owner;
- policy evolution/supersession rules prevent silent weakening;
- small measured architecture/tooling budgets are accepted and machine-visible;
- positive/negative fixtures prove fail-closed behavior;
- permanent Architecture Fitness Gate is active in PR CI/governance as permitted;
- Architecture Impact discipline is documented/mechanically checked where practical;
- no second roadmap/domain/capability/lifecycle authority is created;
- application behavior/production fail-closed state unchanged;
- exact-head CI and protected contexts green;
- accepted-main reread succeeds;
- #399/#421 are re-baselined only after PF-3 acceptance.

Only accepted PF-3 `main` allows FC-6 resume.

## 8. Resume gate — fresh re-baseline after PF-3

Before any further FC-6 ceremony step:

1. reread exact protected `main` after PF-3;
2. reread #399/#421, open PRs/issues and current hosted state;
3. close/supersede stale competing candidates instead of opportunistically merging them;
4. rediscover protected required contexts and applicable workflows; historical counts are not timeless constants;
5. verify live Actions registry equals canonical registry;
6. verify accepted-main durable Release Set publication remains observable through PF-2 evidence where applicable;
7. verify staging observe credential readiness through canonical credential authority + Hosted Evidence;
8. request externally issued missing observe credential/metadata if required; never widen/reuse deploy credential as shortcut;
9. verify PF-3 fitness policy/gate current and no REQUIRED rule unenforced/quietly weakened;
10. verify no AR-12 implementation entered source;
11. verify production fail-closed invariants.

Only then update #399/#421 and resume FC-6.

## 9. FC-6 — Real staging same-bits / rollback rehearsal

FC-6 is the existing AR-11 Functional Closure staging proof, not AR-12 fresh-environment provisioning.

Use already supported staging resources and immutable accepted Release Sets only.

```text
A = older accepted-main durable Release Set
B = newer accepted-main durable Release Set
```

### 9.1 Versioned ceremony state machine

FC-6 must be represented by one versioned state machine rather than inferred only from shell-step ordering.

Minimum non-terminal progression:

```text
CREATED
-> SOURCE_RESOLVED
-> RELEASE_VERIFIED
-> OBSERVED
-> PREFLIGHT_PASSED
-> MUTATION_AUTHORIZED
-> MUTATION_STARTED
-> POST_VERIFY_PASSED
-> SUCCEEDED
```

Required terminal/control outcomes include:

```text
FAILED_PRE_MUTATION
MUTATION_OUTCOME_UNKNOWN
FAILED_POST_MUTATION
BLOCKED
NO_CHANGE
SUCCEEDED
```

Every transition binds its allowed predecessor, required evidence identities, permitted effect class, idempotency/fence identity, retry policy and terminal-audit obligation.

### 9.2 Unknown mutation outcome reconciliation

The dangerous case is explicitly modeled:

```text
provider mutation request sent
-> transport/workflow result lost or ambiguous
-> MUTATION_OUTCOME_UNKNOWN
```

Blind mutation retry is forbidden.

Required recovery path:

```text
MUTATION_OUTCOME_UNKNOWN
-> read-only provider reconciliation
-> APPLIED | NOT_APPLIED | DIVERGED | STILL_UNKNOWN
```

Then:

- `APPLIED` -> continue post-verify against the exact intended target;
- `NOT_APPLIED` -> retry only if idempotency/fence policy still authorizes it;
- `DIVERGED` -> block and use owned rollback/recovery policy;
- `STILL_UNKNOWN` -> block/escalate; never claim success or issue an uncontrolled retry.

This FC-6 reconciliation is bounded promotion-ceremony semantics and must not become the generic AR-14 recovery subsystem.

### 9.3 Required live proof

1. A/B resolve to exact durable immutable release assets;
2. both sources are accepted protected-main history;
3. `release verify A/B` are VALID;
4. observe current staging using least-privilege observe credential;
5. target and rollback-known-good compatibility use the same canonical evaluator;
6. staging A -> B plan/preflight is READY or fails closed for a typed legitimate reason;
7. protected executor uses exact B bytes with no rebuild;
8. ceremony transition/evidence reaches a valid post-mutation state or `MUTATION_OUTCOME_UNKNOWN` reconciliation path;
9. post-deploy `promotion verify B` = VERIFIED;
10. second B plan = NO_CHANGE;
11. A is reevaluated against post-B observed state as rollback known-good;
12. if compatible, B -> A uses original durable A bytes through the same canonical workflow;
13. post-rollback `promotion verify A` = VERIFIED;
14. second A plan = NO_CHANGE;
15. incompatible/UNKNOWN rollback blocks before mutation;
16. stale provider state between preflight/executor trips expected-current fence;
17. ambiguous provider outcome cannot trigger blind retry and is reconciled read-only first;
18. applicable stage evidence is captured through PF-2 primitives and must be mutation-eligible, not merely cryptographically valid;
19. applicable dependency/effect/capability rules remain green through PF-3;
20. production remains untouched.

If no naturally compatible A/B pair exists, correct typed policy BLOCKED evidence is valid. Compatibility must never be falsified to complete ceremony.

## 10. FC-7 — Final whole-AR-11 functional audit

After FC-6, perform a fresh audit from current protected `main`.

### 10.1 Release authority

- one canonical Release Set model;
- accepted-source proof is historical/authoritative, not current-HEAD equality;
- durable publication immutable;
- all locally provable release-critical identities verified;
- unknown release state fails closed.

### 10.2 Inventory/tooling

- one current `opsctl` inventory compiler/checker/writer;
- one current typed Rust acceptance/lifecycle evaluator over raw observations;
- `.github/scripts/architecture-acceptance.mjs` absent after proven cutover;
- zero current legacy Python generator caller;
- deterministic/idempotent tracked projection;
- one canonical serializer/digest implementation family;
- no duplicate lifecycle derivation or interpreted legacy upstream authority;
- exact operator-contract ↔ CLI ↔ inventory parity.

### 10.3 Hosted evidence

- one reusable typed evidence primitive;
- formal trust model clearly states proof limits;
- freshness/replay and mutation-eligibility rules enforced;
- observation/signing/policy responsibilities separated;
- attested exact bytes independently verifiable;
- no secret material or second evidence backend.

### 10.4 Architecture fitness

- PF-3 policy/gate accepted and current;
- no REQUIRED rule without enforcement;
- no silent policy weakening/supersession gap;
- accepted budgets current and justified;
- affected positive/negative fixtures green;
- no duplicate authority, provider leakage, hidden effect, second production-enable path or reachable DEAD predecessor in closure-touched scope;
- Architecture Impact discipline ready for AR-12 onward.

### 10.5 Capability isolation

- source-present disabled capabilities backend-inexecutable;
- frontend remains projection only;
- no independent production feature flags;
- capability checks occur before the first owned side effect.

### 10.6 Promotion / rollback

- deterministic ceremony state machine;
- deterministic plan;
- NO_CHANGE convergence;
- stale fence;
- same-environment serialization;
- historical accepted Release Set promotion;
- no rebuild;
- least-privilege credential boundary;
- rollback compatibility uses current observed state;
- incompatible/UNKNOWN blocks before mutation;
- ambiguous mutation outcome reconciles read-only before any retry;
- post-deploy VERIFIED is the only successful mutation completion state.

### 10.7 Behavioural certification

The original mandatory AR-11 30-case behavioural matrix and closure regressions 31–37 remain binding. Each requirement maps 1:1 to permanent test/gate ID and exact expected fail-closed result. Static markers alone are insufficient where behavioral proof is required.

### 10.8 Platforms / governance

- Linux and Windows native `opsctl` tests;
- applicable workflows green on exact candidates;
- literal current protected required contexts green;
- guarded merges use exact expected heads;
- accepted-main/post-merge evidence directly observable where required;
- inaccessible required evidence is UNPROVEN, never implicit SUCCESS.

Classify final findings as:

```text
P0
P1
P2
P3
NOT_A_DEFECT
LATER_SLICE_BY_DESIGN
```

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
| Static program/lifecycle order | `architecture/architecture-program-sequence.json` under Architecture Re-baseline authority |
| Git/GitHub acceptance observations | GitHub Actions/repository observation shell |
| Acceptance/lifecycle policy evaluation after PF-1 | `tools/opsctl` typed Rust architecture policy core |
| PF-1 detailed cutover contract | `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` / #430 |
| Cross-cutting evolution rules | `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md` |
| Fitness enforcement specification | `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431 |
| Fitness machine policy after PF-3 | `architecture/architecture-fitness-policy.json` |
| Architecture inventory model/compiler/check/write | `tools/opsctl` PF-1 architecture module |
| Architecture inventory tracked projection | `architecture/inventory.json` |
| Acceptance/lifecycle predecessor | `.github/scripts/architecture-acceptance.mjs`, DELETE after PF-1 proof |
| Inventory predecessor | Python inventory cluster, retired after invariant/caller proof |
| Frozen Python provenance | `architecture/python-estate-ar6.json` |
| Current executable-debt disposition | `architecture/historical-executable-debt.json` |
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

Python remains valid for separately classified validators/tests/fixtures/collection adapters when it does not duplicate a concern explicitly cut over to another authority. PF-1 is a bounded architecture-policy/inventory cutover, not a global Python-to-Rust rewrite.

## 12. Testing and stage-gate discipline

Every bounded PF/FC implementation includes positive and negative evidence in the same candidate. Use the lowest layer that can prove the requirement:

```text
pure Rust unit/property tests
-> Rust filesystem/integration tests
-> repository architecture/fitness policy tests
-> workflow semantic/static tests
-> exact-head GitHub Actions
-> accepted-main hosted observation
-> real staging proof only for inherently hosted/provider state
```

Do not use a higher/hosted layer to compensate for missing deterministic unit coverage. Do not use a synthetic lower layer to claim truth that depends on real hosted/provider state.

For authority/security transitions, prove explicitly that the predecessor/forbidden path can no longer execute.

Before each bounded merge:

1. start from latest accepted protected `main`;
2. reread this plan, #399/#421 as applicable, open PRs and live callers;
3. confirm no competing PR owns the same invariant;
4. keep one semantically cohesive proof boundary;
5. add permanent positive + negative tests;
6. no self-writing CI accepted into `main`;
7. no temporary hosted mutation authority unless independently justified, narrowly allowlisted, accepted-main-only and retired by explicit lifecycle;
8. Architecture Impact declaration where PF-3 applies;
9. rediscover applicable permanent workflows/protected contexts from live policy;
10. require every applicable workflow green on exact candidate head;
11. require every protected required context green on exact candidate head;
12. require every applicable REQUIRED fitness rule and owned negative fixture green;
13. require `behind_by=0`;
14. require blocking reviews=0;
15. require unresolved review threads=0;
16. guarded merge bound to exact expected head;
17. reread accepted `main` immediately after merge;
18. prove candidate tree == accepted merge tree where canonical policy requires it;
19. observe required push/main-only hosted evidence directly;
20. treat unobservable required evidence as UNPROVEN, never implicit success;
21. any candidate-head change invalidates all previous exact-head evidence.

## 13. Final Definition of Done — AR-11 Functional Closure 10/10

One current accepted repository state must simultaneously prove:

### Architecture/developer system

- target architecture/evolution contract accepted in `main`;
- PF-3 fitness baseline accepted and machine-enforced;
- `opsctl` is sole current acceptance/lifecycle policy evaluator and architecture inventory compiler/checker/writer for PF-1 scope;
- Git/GitHub raw observation effects remain outside Rust policy core;
- `.github/scripts/architecture-acceptance.mjs` deleted after zero-caller/zero-unique-invariant proof;
- no parallel current Python inventory authority;
- typed layers separated from CLI/adapters;
- deterministic render/check/write byte-stable;
- explicit bounded generated-file write authority;
- singular lifecycle derivation from raw observations;
- domain semantic validators remain bounded rather than absorbed into inventory;
- architecture inventory understandable from current code/docs without historical issue archaeology;
- no hidden/parallel authority introduced.

### Hosted operational evidence

- one reusable typed/versioned Hosted Evidence primitive;
- immutable attested subject bytes;
- formal trust model states what each layer proves/does not prove;
- freshness/replay and mutation eligibility are explicit;
- local policy and GitHub provenance verification clearly separated;
- no secret-bearing evidence or hidden backend;
- future evidence extends by typed payload variant, not new framework.

### Accepted source / immutable release

- historical accepted-main Release Sets remain valid inputs after `main` advances;
- non-main/unaccepted sources reject;
- durable assets contain every byte required for later verification;
- source/component/provenance/toolchain/contract/schema/runtime identities checked as required;
- same ID/different bytes is fatal.

### Promotion / rollback

- versioned ceremony state machine with explicit terminal states;
- unknown mutation outcome cannot trigger blind retry and must reconcile read-only;
- no rebuild on promotion;
- deterministic plan and NO_CHANGE work;
- concurrency/expected-current fencing enforced;
- observe vs deploy credentials separated;
- historical compatible release can promote from original bytes;
- rollback compatibility evaluates current schema/protocol/runtime state;
- incompatible/UNKNOWN rollback fails closed;
- post-deploy verification converges to VERIFIED then NO_CHANGE;
- production remains unreachable.

### Capability isolation

- `source_present=true` and `production_enabled=false` mechanically demonstrable;
- disabled HTTP/queue/scheduled/service/outbound paths cannot produce forbidden side effects;
- frontend manipulation cannot bypass backend capability gates.

### Evidence / audit

- original AR-11 30-case matrix has permanent 1:1 behavioural mapping;
- closure regressions 31–37 permanently covered;
- PF-1/PF-2/PF-3 negative matrices permanently covered;
- PF-3 anti-weakening and budget checks active;
- Linux/Windows suites pass;
- real staging same-bits promotion/rollback evidence exists where compatibility permits or is correctly policy-BLOCKED;
- final audit P0=0/P1=0/P2=0 for Functional Closure scope.

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
