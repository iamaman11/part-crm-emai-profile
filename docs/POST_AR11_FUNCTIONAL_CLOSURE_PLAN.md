# Post-AR-11 Functional Closure Plan — Release / Promotion Contract to 10/10

**Document status:** SUBORDINATE_REMEDIATION_PLAN  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Accepted AR-11 design:** `docs/ARCHITECTURE_REBASELINE_V3_AR11.md`  
**Live execution tracker:** issue #399  
**Current FC-6 implementation/hardening tracker:** issue #421  
**Historical baseline for this plan:** protected `main@586e6ea1be9f72b4f3a59c732714a8c12216985e`  
**Last sequencing re-baseline (informational only):** protected `main@7596f22ed606cdc7afbf75209bc3925ef80c9e07` on 2026-08-21  
**Scope:** close residual functional gaps in the already accepted AR-11 release-set / promotion architecture and its required operational tooling before AR-12 implementation  
**AR-12 implementation:** FORBIDDEN inside this plan  
**Production mutation / enablement:** FORBIDDEN except the already-authorized non-production staging mutation explicitly owned by FC-6 rehearsal  
**Historical AR-11 acceptance:** PRESERVED; this plan does not rewrite or revoke accepted history

This document is the single subordinate execution plan for Post-AR-11 Functional Closure. It is not a second lifecycle authority, not a new AR slice and not a parallel roadmap. The canonical AR order remains owned by the existing architecture program and Git-derived acceptance mechanism.

If a stale SHA, issue comment, PR description or this document conflicts with live protected `main` and canonical machine authorities, use this precedence:

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

The purpose of the current update is to make the continuation path explicit and remove two infrastructure/tooling defects before FC-6 continues:

```text
PF-1  Canonical Architecture Inventory cutover to opsctl
  ->
PF-2  Universal Hosted Operational Evidence primitive
  ->
re-baseline #399 / #421
  ->
resume FC-6 real staging same-bits / rollback rehearsal
  ->
FC-7 final whole-AR-11 functional audit
```

PF-1 and PF-2 are **Functional Closure Prerequisites**, not AR-11A/AR-11.5/AR-12 work. They do not change the binding AR sequence and do not authorize production.

---

## 1. Non-negotiable architecture and production invariants

The following accepted architecture remains binding throughout all work in this plan:

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

Do not:

- rebuild AR-11 from scratch;
- create a second architecture inventory, release registry, capability registry, lifecycle state machine, evidence database or hidden `opsctl` state backend;
- create a parallel inventory generator during PF-1;
- implement acceptance-tag parsing independently in Rust/Python during PF-1;
- add `promotion execute` or provider mutation authority to `opsctl`;
- give `opsctl` GitHub/Cloudflare/provider network authority merely to make local validation convenient;
- rebuild application artifacts during promotion;
- introduce Terraform or a generic IaC replacement;
- enable Production Core or mailbox capabilities;
- start AR-12 Fresh Rehearsal Environment work;
- absorb AR-14 recovery, AR-15 Windows updater/signing or AR-17 production authorization into these prerequisites;
- rewrite accepted AR-11 provenance;
- weaken a gate, convert UNKNOWN/UNPROVEN into PASS, or suppress a validator to obtain green CI.

Issue #375 is closed/completed historical hardening. It is not a current execution blocker or lifecycle authority. Any stale text that still says `#375 OPEN` is governance/projection drift and must be corrected only through the existing authority/projection mechanism; it must never be interpreted as permission to start AR-12.

---

## 2. Current functional baseline — preserve, do not reimplement blindly

The live tracker #399 and accepted Git history are the evidence ledger for completed Functional Closure work. At the last sequencing re-baseline:

- AR-11 remains historically accepted;
- AR-12 is the Git-derived current slice but implementation remains NOT STARTED for this plan;
- PR #422 repository-side FC-6 hardening is merged;
- PR #424 hosted Actions registry reconciliation is merged;
- live active GitHub Actions registry was directly observed as `active=23 canonical=23` after #424;
- current accepted-main durable Release Set publication is directly proven;
- #399 remains OPEN because FC-6 live staging ceremony and FC-7 final audit are not complete;
- #421 remains the current FC-6 operational hardening/readiness tracker;
- PR #428 contains a substantial Hosted Operational Evidence candidate but must not be merged before PF-1;
- remaining FC-6 hosted credential readiness still requires the correctly scoped staging observe credential/evidence; existing deploy/bootstrap credentials must not be widened or reused as a shortcut.

Existing AR-11 Functional Closure outcomes remain foundations unless a fresh live audit proves a defect, including:

1. historical accepted-source semantics rather than `source_sha == current main HEAD`;
2. content-addressed immutable Release Sets and exact durable artifact verification;
3. native `opsctl release inspect|verify|compatibility`;
4. native `opsctl promotion plan|preflight|verify`;
5. full rollback compatibility vocabulary `COMPATIBLE | INCOMPATIBLE | UNKNOWN`, with UNKNOWN fail-closed where evidence is required;
6. stale expected-current fencing and same-environment serialization;
7. `NO_CHANGE` as first-class convergence;
8. separation of read/observe preflight from protected mutation authority;
9. backend capability enforcement independent of frontend visibility;
10. retired legacy D3 Python operational promotion authority;
11. permanent workflow semantic validation and canonical secret-consumer/environment checks;
12. terminal machine-readable FC-6 failure audit semantics;
13. production promotion remains impossible during this architecture program.

Do not repeat accepted work because an old branch or old plan snapshot looks unfinished. Re-read current code and current accepted evidence first.

---

## 3. Why the prerequisite order is binding

The required order is:

```text
PF-1 opsctl Canonical Architecture Inventory Cutover
        ↓
PF-2 Hosted Operational Evidence
        ↓
FC-6 continuation
```

This order is a dependency, not a stylistic preference.

Hosted Evidence adds or changes `opsctl` surfaces, `architecture/operator-contract.json`, GitHub Actions registrations and canonical authority digests. Those changes necessarily affect `architecture/inventory.json`. Building PF-2 first on the historical Python inventory path causes duplicated work and has already exposed recurring stale-inventory failures.

PF-1 does not depend on Hosted Evidence. It depends only on existing canonical repository/domain authorities, stable repository structure, the already accepted lifecycle authority and deterministic serialization. Therefore PF-1 must establish the final inventory compiler/validator boundary first, and PF-2 must consume that boundary.

No implementation may reverse this order without new defect evidence and an explicit update to this plan.

---

## 4. PF-1 — Canonical Architecture Inventory cutover to `opsctl`

### 4.1 Goal

Make `opsctl` the **single current implementation authority** for deterministic construction, rendering, checking, inspection and bounded local writing of `architecture/inventory.json`.

The desired end state is:

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

The forbidden end state is:

```text
Rust generator
+ Python generator
+ historical engine constants
+ monkey patches
+ manual inventory edits
```

`architecture/inventory.json` remains a tracked generated canonical projection under the existing hierarchy. It does not become lifecycle acceptance authority.

### 4.2 Clean layering

PF-1 must preserve inward dependency direction and keep the implementation understandable to a new developer.

Recommended logical layers inside the existing `tools/opsctl` crate:

```text
architecture/model
    typed schemas and invariants only

architecture/authorities
    typed loaders/validators for canonical repository authorities

architecture/inventory/build
    pure composition from validated inputs -> ArchitectureInventory

architecture/inventory/render
    canonical deterministic serialization

architecture/inventory/check
    tracked-byte/semantic comparison + precise drift diagnostics

architecture/inventory/write
    bounded atomic GENERATED_PROJECTION_WRITE to one fixed target

cli / lib composition root
    argument parsing, input wiring and presentation only
```

Physical file names may differ if a smaller module layout is clearer. Do not create micro-modules for aesthetics. The rule is one responsibility per layer and no CLI/parser ownership of domain semantics.

### 4.3 CLI contract

Target active surface:

```text
opsctl architecture inventory render
opsctl architecture inventory check
opsctl architecture inventory write
opsctl architecture inventory inspect
```

The existing `opsctl inventory` surface may remain only as a compatibility/read-only alias to the tracked canonical inventory or to `architecture inventory inspect`, provided there is exactly one semantic implementation and operator-contract parity is explicit.

Unknown actions/arguments fail closed.

### 4.4 Lifecycle authority boundary

PF-1 must **not** create a Rust acceptance-tag parser or a second accepted/current derivation algorithm.

Current lifecycle authority remains:

```text
architecture/architecture-acceptance-policy.json
+ architecture/architecture-program-sequence.json
+ immutable Git acceptance metadata
-> .github/scripts/architecture-acceptance.mjs derive
```

`opsctl architecture inventory ...` consumes a versioned, validated lifecycle-derivation input produced by that authority. CI/orchestration may materialize the canonical derivation result as a temporary JSON input before invoking `opsctl`; `opsctl` validates shape, static successor consistency and fail-closed invariants but does not independently enumerate/interpret acceptance tags.

Do not hide a permanent `node`, Python, `git`, `gh` or provider subprocess inside `opsctl` merely to make the command appear self-contained.

A future migration of lifecycle derivation to Rust would be a separate explicit authority cutover with parity proof; it is not PF-1.

### 4.5 Typed authority inputs

Known security/release/lifecycle-critical authorities must use typed Rust structures rather than an unbounded `serde_json::Value` domain model where schemas are known.

At minimum preserve/validate the current semantics for:

- credential authority;
- credential lifecycle;
- operator contract;
- profile security;
- static architecture program sequence and lifecycle projection policy;
- AR-9 D1 evolution authority;
- AR-10 runtime cutover authority;
- AR-11 release architecture authority;
- stable workspace/application/runtime/generated-contract inventory inputs currently owned by the historical Python inventory path;
- documentation classifications required by current inventory semantics.

Generic JSON values are allowed only at genuine extension boundaries and must not bypass version/kind validation.

### 4.6 One canonical serializer/digest primitive

Do not add another JSON canonicalization implementation.

PF-1 must reuse or factor the existing Rust canonical JSON/SHA-256 primitive already used by release/evidence policy so the project converges on one deterministic byte model:

```text
release metadata
architecture inventory
hosted evidence
future typed generated projections
        ↓
one canonical JSON / digest primitive
```

Repeated render with identical inputs must be byte-identical.

### 4.7 Effect model

Do not falsely label a file-writing command as `side_effects=NONE`.

Extend the existing operator effect taxonomy narrowly:

```text
network_authority = false
provider_mutation = false
database_mutation = false
deployment_mutation = false
secret_readback = false
customer_state_mutation = false
production_mutation = false

repository_workspace_effect:
  NONE                       # render/check/inspect
  GENERATED_PROJECTION_WRITE # inventory write only
```

`write` may modify exactly the repository-owned canonical target:

```text
architecture/inventory.json
```

No arbitrary output path, Git commit/push, GitHub API, provider API or other repository file mutation authority is granted by this command.

### 4.8 Deterministic and atomic write

`write` must follow a fail-closed transaction:

```text
resolve canonical repo root
-> validate all inputs
-> build typed inventory in memory
-> validate complete output
-> canonical serialize
-> write sibling temporary file
-> flush/safely replace target atomically where supported
-> read back
-> parse/validate again
-> prove read-back bytes == canonical in-memory bytes
```

Failure before activation must leave the previous tracked inventory intact.

### 4.9 `check` diagnostics

A stale inventory failure must explain the drift, not merely say “run --write”.

`check` should report stable machine-readable or developer-readable differences including, where practical:

- JSON path / field;
- tracked value;
- expected value;
- owning source authority or projection family;
- decision `CURRENT | DRIFTED | INVALID`;
- no mutation executed.

Noncanonical bytes must fail where canonical byte identity is part of the contract.

### 4.10 Historical Python inventory retirement

PF-1 is a real cutover, not a permanent dual implementation.

Before retiring any Python file, apply:

```text
map unique invariants
-> port required current semantics
-> positive + negative parity tests
-> switch every current caller/CI gate
-> prove zero current callers
-> prove zero unique current invariants
-> update Python/historical executable classification
-> retire/delete predecessor where classified DEAD
```

The current active path based on `scripts/generate-architecture-inventory.py` + `scripts/generate-architecture-inventory-engine.py` must cease to be current executable authority after PF-1 acceptance. `_architecture_inventory_core.py` or other helpers are removed only if caller/invariant proof classifies them DEAD; do not delete by naming convention.

No CI job may continue invoking the retired current generator after cutover.

### 4.11 PF-1 required positive proofs

At minimum prove:

```text
current accepted repository -> render succeeds
render twice -> byte-identical
write -> tracked file equals render bytes
write twice -> byte-identical/idempotent
check immediately after write -> CURRENT
existing legitimate stable/domain projection coverage -> preserved
Linux -> pass
Windows -> pass
```

### 4.12 PF-1 required negative proofs

At minimum reject:

```text
missing required authority
malformed JSON
unknown authority kind/version
wrong authority ownership/status
invalid source/document path
unknown/duplicate classification where uniqueness is required
lifecycle accepted/current successor mismatch
architecture_complete=true before owning stage
production_core_gate=AUTHORIZED before owning stage
production_ready=true
production_mutation=true
one-byte tracked inventory drift
semantically changed tracked inventory
noncanonical inventory bytes where canonicality is required
attempt to write any path other than architecture/inventory.json
retired Python generator still reachable from current CI/caller graph
second lifecycle derivation implementation
```

### 4.13 PF-1 Definition of Done

PF-1 is accepted only when one exact candidate proves all of the following:

- one current inventory compiler/validator implementation exists: Rust `opsctl`;
- no current Python inventory generator remains callable by CI/developer canonical commands;
- every unique current invariant from the predecessor is either ported with proof or explicitly proven obsolete;
- tracked `architecture/inventory.json` is generated by `opsctl` and passes native `check`;
- canonical serialization is deterministic and shared, not duplicated;
- `write` authority is bounded to `GENERATED_PROJECTION_WRITE` for one fixed path;
- lifecycle derivation remains singular and external to inventory compilation;
- operator-contract ↔ CLI parity is exact;
- all change-applicable permanent workflows and all protected required contexts are green on the same exact head;
- `behind_by=0`, blocking reviews=0, unresolved threads=0;
- guarded merge is bound to the exact green head;
- accepted-main reread proves intended tree/authority state;
- production fail-closed invariants remain unchanged.

Only accepted PF-1 `main` may become the base for PF-2.

---

## 5. PF-2 — Universal Hosted Operational Evidence primitive

### 5.1 Goal

Provide one reusable evidence architecture for hosted/provider observations needed by AR-11 Functional Closure and later operational slices without creating per-feature evidence frameworks.

Canonical boundary:

```text
GitHub Actions / official provider tools
-> secret-free raw observation
-> opsctl typed policy / validation
-> HostedEvidenceEnvelopeV1
-> immutable GitHub Actions Artifact
-> GitHub Artifact Attestation / custom predicate
```

GitHub issue comments and tracker text may reference evidence but are not the evidence store, signing authority or policy authority.

### 5.2 Ownership split

```text
GitHub Actions / Environments
  orchestration, approvals, OIDC, credential exposure, immutable run identity

official provider tooling
  live observation and explicitly authorized provider execution

opsctl
  typed evidence schemas/versions
  canonicalization/digests
  secret/material rejection
  environment/effect policy
  deterministic inspect/validate/verify
  NO provider/GitHub network authority

Actions Artifact
  immutable transport subject

GitHub Artifact Attestation
  signing/provenance binding
```

Do not build a second evidence DB, queue, scheduler, daemon, signer, PKI, report service or feature-specific reporter workflow family.

### 5.3 Initial command surface

Target active surface from the existing #428 implementation, re-evaluated after PF-1:

```text
opsctl evidence build
opsctl evidence validate
opsctl evidence inspect
opsctl evidence verify
```

All surfaces remain offline and non-provider-mutating.

### 5.4 Initial typed payload families

Preserve a small versioned sum type rather than arbitrary evidence bags. The existing candidate families are:

```text
credential_readiness v1
hosted_resource_state v1
release_set_transition v1
```

Future evidence kinds must normally extend the Rust typed variant set and reuse the same envelope/publication path rather than create another evidence subsystem.

### 5.5 Rebase/candidate rule for PR #428

PR #428 is preserved as implementation work but is **not merge-authorized before PF-1**.

After PF-1 acceptance:

1. re-read #428 against new accepted `main`;
2. rebase/reimplement only the still-valid Hosted Evidence changes;
3. remove manual/stale `architecture/inventory.json` maintenance from the candidate;
4. update operator-contract and regenerate/check inventory exclusively through PF-1 `opsctl` inventory commands;
5. retain no compatibility bridge to the retired Python inventory generator;
6. re-run security/workflow semantics review and complete missing tests;
7. treat all old #428 CI evidence as invalid after the head/base changes.

### 5.6 Hosted workflow requirements

The reusable publisher must:

- accept no provider secret inheritance;
- download exactly one expected evidence subject;
- reconstruct independent expected run/workflow/source/environment/effect context from trusted metadata/explicit caller inputs;
- invoke `opsctl evidence verify` before signing;
- attest exact verified evidence bytes with a pinned official GitHub attestation action;
- use minimal `contents: read`, `id-token: write`, `attestations: write` permissions only where required;
- perform no provider mutation;
- perform no production enablement;
- remain one reusable publication primitive rather than a manual workflow family.

Reusable-workflow GitHub context semantics (`github.sha`, `github.ref`, workflow identity, run id/attempt and caller vs called workflow identity) must be explicitly validated against current GitHub behavior before final acceptance. Do not assume caller/callee identity semantics from memory.

### 5.7 Evidence security rules

Evidence objects must reject or exclude:

- secret/token/password/private-key values;
- secret-bearing unknown field names;
- customer/mail/browser/profile payloads;
- fingerprint raw material;
- arbitrary provider mutation claims inconsistent with environment/effect policy;
- unknown schema/kind/payload versions;
- unexpected top-level fields where schema is closed;
- malformed or oversized inputs beyond bounded policy;
- context mismatch between evidence and independently reconstructed expected context.

`opsctl evidence verify` proves local schema/canonical/context policy. GitHub Artifact Attestation proves subject-byte provenance/tampering resistance. Do not claim that local `opsctl verify` alone can detect a semantically valid payload change when no independent expected payload digest exists.

### 5.8 PF-2 required negative matrix

At minimum prove:

```text
unknown evidence kind -> reject
unknown payload version -> reject
unknown top-level field -> reject
recursive secret-bearing field -> reject
obvious secret material -> reject
malformed input -> reject
oversized input -> reject
noncanonical evidence bytes -> verify reject
wrong repository -> reject
wrong source SHA/ref -> reject
wrong workflow identity -> reject
wrong run id/attempt -> reject
wrong observation job -> reject
wrong environment -> reject
wrong effect flags -> reject
production effect-policy violation -> reject
invalid release transition decision/effect combination -> reject
wrong CLI action options -> reject
unsupported/duplicate action arguments -> reject
artifact with extra files -> publisher reject
attestation subject differs from verified bytes -> official attestation verification fails
```

### 5.9 PF-2 Definition of Done

PF-2 is accepted only when:

- one reusable Hosted Operational Evidence envelope/publication architecture exists;
- all supported payloads are typed/versioned/fail-closed;
- canonical JSON/digest logic is shared with existing Rust primitives;
- `opsctl` remains offline/no-provider/no-secret/no-production mutation authority;
- publisher has no provider credentials;
- provider observation and signing authority are separated;
- reusable workflow context binding has direct proof;
- negative matrix above is permanent;
- official attestation verification is documented and demonstrated for a non-secret evidence subject where hosted proof is required;
- operator-contract/CLI/inventory parity is produced through PF-1 mechanisms;
- no second evidence framework/backend/PKI/signer is introduced;
- all applicable permanent workflows + protected contexts are green on one exact head;
- guarded merge and accepted-main reread succeed;
- production remains fail-closed and AR-12 remains not started.

Only accepted PF-2 `main` may resume FC-6 execution.

---

## 6. Resume gate — re-baseline #399 and #421 after PF-2

Before any further FC-6 ceremony step:

1. re-read exact protected `main` after PF-2;
2. re-read open PRs/issues and close/supersede stale candidates rather than merging them opportunistically;
3. re-read #399 and #421 live state;
4. rediscover current protected required contexts and applicable permanent workflows; do not reuse historical counts as timeless constants;
5. confirm live Actions registry == canonical registry;
6. confirm current accepted-main durable Release Set publication remains observable through the new Hosted Evidence path where applicable;
7. confirm staging observe credential readiness through canonical credential authority and Hosted Evidence;
8. if `CLOUDFLARE_OBSERVE_API_TOKEN` or its issuance/policy metadata is still absent, request the required externally issued credential/metadata explicitly; do not fabricate it and do not widen/reuse the deploy credential;
9. confirm no AR-12 implementation has entered source;
10. confirm production fail-closed invariants.

Only then update #399/#421 execution status and resume FC-6.

---

## 7. FC-6 — Real staging same-bits / rollback rehearsal

FC-6 remains the existing AR-11 Functional Closure staging proof. It is not AR-12 fresh-environment provisioning.

Use already supported staging resources and immutable accepted Release Sets only.

The canonical scenario remains:

```text
A = older accepted-main durable Release Set
B = newer accepted-main durable Release Set
```

Required live proof:

1. A and B resolve to exact durable immutable release assets;
2. both sources are accepted protected-main history;
3. `release verify A` and `release verify B` are VALID;
4. observe current staging state using the least-privilege observe credential;
5. target compatibility and rollback known-good compatibility evaluate through the same canonical authority;
6. staging A -> B plan/preflight is READY or fail-closed for a typed legitimate reason;
7. protected staging executor uses exact B bytes with no rebuild;
8. post-deploy `promotion verify B` = VERIFIED;
9. second B plan = NO_CHANGE;
10. A is evaluated against post-B observed state as rollback known-good;
11. if compatible, B -> A uses original durable A bytes through the same canonical workflow;
12. post-rollback `promotion verify A` = VERIFIED;
13. second A plan = NO_CHANGE;
14. at least one incompatible/UNKNOWN rollback case blocks before mutation;
15. stale provider state between preflight and executor trips the expected-current fence;
16. evidence for every stage is captured through PF-2 primitives where the evidence kind applies;
17. production remains untouched.

If no naturally compatible A/B pair exists, that is valid only when the evaluator blocks rollback for the correct typed reason; do not falsify compatibility to complete the ceremony.

---

## 8. FC-7 — Final whole-AR-11 functional acceptance audit

After FC-6 live proof, perform a fresh audit from current protected `main`.

Audit at least:

### Release authority

- one canonical Release Set model;
- accepted-source proof is historical/authoritative, not current-HEAD equality;
- durable publication immutable;
- every locally provable release-critical identity is actually verified;
- unknown release state fails closed.

### Architecture inventory/tooling

- one current `opsctl` inventory compiler/checker;
- no current legacy Python generator caller;
- deterministic/idempotent tracked projection;
- one canonical serializer/digest implementation family;
- no duplicate lifecycle derivation;
- exact operator-contract ↔ CLI ↔ inventory parity.

### Hosted evidence

- one typed reusable evidence primitive;
- provider observation/signing/policy responsibilities separated;
- attested exact bytes independently verifiable;
- no secret material or second evidence backend.

### Capability isolation

- source-present disabled capabilities remain backend-inexecutable;
- frontend remains projection only;
- no independent production feature flags.

### Promotion / rollback

- deterministic plan;
- NO_CHANGE convergence;
- stale fence;
- same-environment serialization;
- historical accepted Release Set promotion;
- no rebuild;
- least-privilege credential boundary;
- rollback compatibility uses current observed state;
- incompatible/UNKNOWN rollback blocks before mutation;
- post-deploy VERIFIED is the only success state.

### Behavioural certification

The original mandatory AR-11 30-case behavioural matrix and closure regressions 31–37 remain binding. The final audit must confirm each requirement maps 1:1 to a permanent test/gate identifier and expected fail-closed result. Static source markers alone are not sufficient where behavioural proof is required.

### Platforms / CI / governance

- Linux and Windows native `opsctl` tests;
- applicable permanent workflows green on exact candidates;
- literal current protected required contexts green;
- guarded merges use exact expected heads;
- accepted-main/post-merge evidence directly observable where required;
- no hidden success inference from inaccessible evidence.

Classify every final finding:

```text
P0
P1
P2
P3
NOT_A_DEFECT
LATER_SLICE_BY_DESIGN
```

AR-11 Functional Closure is complete only when:

```text
P0 = 0
P1 = 0
P2 = 0 for AR-11 Functional Closure scope
PF-1 = ACCEPTED AND VERIFIED
PF-2 = ACCEPTED AND VERIFIED
all mandatory AR-11 behavioural requirements = PROVED
FC-6 staging proof = PROVED or correctly BLOCKED by accepted compatibility policy
production_mutation = false
AR-12 implementation mixed into closure = false
```

---

## 9. Canonical ownership map for continuation

Exact paths must be re-read from live `main`; this map describes responsibilities, not frozen filenames.

| Concern | Canonical owner / boundary |
| --- | --- |
| Program/lifecycle order | existing Architecture Re-baseline authorities + Git-derived acceptance mechanism |
| Architecture inventory model/compiler/check/write | `tools/opsctl` PF-1 architecture module |
| Architecture inventory tracked projection | `architecture/inventory.json` |
| Inventory predecessor | existing Python inventory generator path, retired only after parity/caller proof |
| Operator command authority | `architecture/operator-contract.json` |
| Canonical JSON / digest | shared Rust policy primitive; no inventory/evidence duplicate |
| Hosted evidence typed envelope | `tools/opsctl` evidence module |
| Hosted evidence orchestration | one reusable permanent GitHub Actions publisher |
| Evidence subject transport | immutable GitHub Actions Artifact |
| Evidence signing/provenance | GitHub Artifact Attestation / official action |
| Provider observation | official provider tools under least privilege |
| Release Set typed model | existing `tools/opsctl/src/release/**` architecture |
| Promotion plan/preflight/verify | existing `tools/opsctl/src/promotion/**` architecture |
| Durable build/publish | existing canonical Release Set Build workflow |
| Staging mutation executor | existing canonical Release Set Promotion protected staging path |
| FC live tracker | issue #399 |
| FC-6 operational hardening/readiness | issue #421 |

Python remains acceptable for separately classified validators/generators/fixtures/collection adapters when it does not duplicate a concern that has been explicitly cut over to `opsctl`. PF-1 is deliberately such a cutover for architecture inventory generation; it is not a global Python-to-Rust rewrite.

---

## 10. Testing philosophy — required for PF-1, PF-2 and resumed FC work

Every bounded implementation must include positive and negative evidence in the same candidate.

Use the strongest appropriate layer:

```text
pure Rust unit tests
-> Rust filesystem/integration tests
-> repository fitness/policy tests
-> workflow semantic/static tests
-> exact-head GitHub Actions
-> accepted-main hosted observation
-> real staging proof only where the requirement is inherently hosted/provider-dependent
```

Do not use a higher, slower layer to compensate for missing deterministic unit coverage. Do not use a lower synthetic layer to claim a requirement whose truth depends on real hosted/provider state.

For security/authority transitions, include explicit negative proof that the predecessor/forbidden authority cannot still execute.

---

## 11. Acceptance discipline for every bounded merge

Before each PF or FC implementation merge:

1. start from latest accepted protected `main`;
2. re-read this plan, #399/#421 as applicable, open PRs and live callers;
3. confirm no competing open PR owns the same invariant;
4. use one semantically cohesive proof boundary;
5. add permanent positive + negative tests in the candidate;
6. no self-writing CI accepted into `main`;
7. no temporary hosted mutation authority unless independently justified, narrowly allowlisted, accepted-main-only and removed/retired by its explicit lifecycle;
8. rediscover applicable permanent workflows and protected contexts from live policy;
9. require every applicable workflow green on the exact candidate head;
10. require every protected required context green on the exact candidate head;
11. require `behind_by=0`;
12. require blocking reviews=0;
13. require unresolved review threads=0;
14. guarded squash merge must be bound to the exact expected head;
15. reread accepted `main` immediately after merge;
16. prove candidate tree == accepted merge tree where required by canonical acceptance policy;
17. observe required push/main-only hosted evidence directly;
18. treat unobservable required evidence as UNPROVEN, never implicit SUCCESS;
19. changing the candidate head invalidates all previous exact-head evidence.

---

## 12. Final Definition of Done — AR-11 Functional Closure 10/10

The closure is complete only when one current accepted repository state simultaneously proves:

### Inventory / developer architecture

- `opsctl` is the single current architecture inventory compiler/checker/writer;
- no parallel current Python inventory authority remains;
- typed layers are separated from CLI/adapters;
- deterministic render/check/write is byte-stable;
- local generated-file write authority is explicit and narrowly bounded;
- lifecycle derivation remains singular;
- architecture inventory is understandable from current code/docs without historical issue archaeology.

### Hosted operational evidence

- one reusable typed/versioned Hosted Evidence primitive exists;
- artifact subject bytes are immutable and attested;
- local policy verification and GitHub provenance verification are clearly separated;
- no secret-bearing evidence or hidden backend exists;
- future operational evidence can extend by typed payload variant without new workflow/reporting architecture.

### Accepted source / immutable release

- historical accepted-main Release Sets remain valid policy inputs after main advances;
- non-main/unaccepted sources reject;
- durable assets contain every byte needed for later local verification;
- source/component/provenance/toolchain/contract/schema/runtime identities are checked as required;
- same ID/different bytes is fatal.

### Promotion / rollback

- no rebuild on promotion;
- deterministic plan and NO_CHANGE work;
- concurrency and expected-current fencing are enforced;
- observe vs deploy credentials are separated;
- historical compatible Release Set can be promoted from original bytes;
- rollback compatibility evaluates current schema/protocol/runtime state;
- incompatible/UNKNOWN rollback fails closed;
- post-deploy verification converges to VERIFIED and then NO_CHANGE;
- production remains unreachable.

### Capability isolation

- `source_present=true` and `production_enabled=false` remains mechanically demonstrable;
- disabled HTTP/queue/schedule/outbound paths cannot produce side effects;
- frontend manipulation cannot bypass backend capability gates.

### Evidence / audit

- original 30-case negative matrix has permanent 1:1 behavioural mapping;
- closure regressions 31–37 are permanently covered;
- PF-1 and PF-2 negative matrices are permanently covered;
- Linux and Windows suites pass;
- real staging same-bits promotion/rollback evidence exists where compatibility permits;
- final audit finds P0=0, P1=0 and P2=0 for Functional Closure scope.

Final state remains:

```text
AR-11 = historically ACCEPTED + functionally CLOSED
AR-12 = current / implementation NOT STARTED during this plan
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

Only after this Definition of Done is mechanically proven may the project describe AR-11 as **fully functional / 10/10 closed** and separately consider AR-12 implementation entry under the canonical architecture program.