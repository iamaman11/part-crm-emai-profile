# Pre-PF-1 Authority Estate Normalization

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Functional Closure plan:** `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`  
**PF-1 detailed specification:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**Tracking umbrella:** issue #399  
**Production authorization:** NONE  
**AR-12 implementation:** NOT AUTHORIZED by this document

This document defines a bounded normalization block that must complete before PF-1 implementation entry. It does **not** reopen accepted AR-2/AR-6/AR-7/AR-8/AR-10 history, create a new AR slice, create PF-0/PF-4, or alter `architecture/architecture-program-sequence.json`.

The purpose is to prevent PF-1 from becoming a mechanical Rust rewrite of stale phase-qualified JSON/Python/Node authorities. Accepted functionality remains; only current semantic ownership is normalized.

## 1. Binding order

The Post-AR-11 continuation order becomes:

```text
Pre-PF-1 Authority Estate Normalization
  N1  AR-2 runtime/resource topology authority retirement
   ->
  N2  AR-6 Python-estate current-authority retirement
   ->
  N3  AR-7 current GitHub-governance authority normalization
   ->
  N4  bounded AR-8 operator/provenance authority cleanup
   ->
  N5  AR-10 runtime semantic authority retirement
   ->
PF-1  typed lifecycle evaluator + deterministic inventory compiler + Node/Python predecessor deletion
   ->
PF-2  Universal Hosted Operational Evidence
   ->
PF-3  Architecture Fitness Baseline
   ->
fresh re-baseline #399/#421
   ->
FC-6 real staging same-bits / rollback rehearsal
   ->
FC-7 final whole-AR-11 functional audit
   ->
AR-12 implementation entry
```

N1..N5 are normalization transactions, not lifecycle slices. They preserve the accepted AR sequence and historical evidence.

## 2. Permanent ownership principle

For every touched artifact classify its current role as exactly one of:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

A `TRANSITIONAL_SEMANTIC_SOURCE` must not survive merely because a checker or inventory generator currently reads it. Its retirement contract is:

```text
identify the natural owner
-> preserve accepted behavior/invariants
-> switch every current consumer
-> prove predecessor caller_count = 0
-> prove predecessor unique_current_invariant_count = 0
-> physically delete or demote the predecessor from current authority
-> preserve provenance in Git/evidence
```

Do not replace a retired JSON authority with a successor JSON/TOML/YAML carrying the same semantics, and do not copy a giant old table into Rust and call that architectural convergence.

## 3. N1 — AR-2 runtime/resource topology authority retirement

### Historical purpose preserved

AR-2 classified Workers, D1, R2, Durable Objects, queues/DLQ, service bindings, schedules and provider lanes; it proved `GENERATION_VERIFICATION=DELETE`, retained resolver isolation and disabled the legacy D3 forward production lane.

### Current target ownership

```text
Cloudflare deployment/resource topology -> Wrangler/provider-native config
runtime/workload ownership             -> Product Rust composition/handlers/contracts
capability production admission        -> Release / Capability Profile
structural anti-regression rules       -> bounded fitness/checker rules
AR-2 decision provenance               -> Git/evidence
```

### Required cutover

- stop treating `architecture/runtime-topology-ar2.json` as current semantic input;
- remove inventory dependence on that file;
- remove duplicate manually-maintained resource-decision tables where the same fact is derivable from current Wrangler/Rust owners;
- retain explicit negative invariants such as no `GENERATION_VERIFICATION` queue workload and resolver isolation without retaining the historical AR-2 manifest as their semantic owner;
- update docs/projections so AR-2 is historical provenance, not a live topology database;
- after zero-caller/zero-unique-current-invariant proof, physically delete `architecture/runtime-topology-ar2.json` from the current tree.

No Cloudflare/provider mutation is part of N1.

## 4. N2 — AR-6 Python-estate current-authority retirement

### Historical purpose preserved

AR-6 established a complete Python census/disposition and introduced standalone Rust `tools/opsctl` without authorizing a global Python-to-Rust rewrite.

### Defect to remove

`architecture/python-estate-ar6.json` must not remain a living manually-maintained database of every current `.py` file. Repository structure is the observation; Python is allowed when it owns a legitimate adapter/test/validator role.

### Current target ownership

```text
tracked Python files              -> repository filesystem/source tree
legitimate runtime adapters       -> their bounded runtime owner
legitimate tests/fixtures         -> test owner
forbidden duplicate executable    -> structural/fitness rule
AR-6 accepted census              -> Git/evidence
opsctl semantics                  -> tools/opsctl Rust modules
```

### Required cutover

- remove `python-estate-ar6.json` from current operational/inventory authority;
- replace per-file phase-qualified disposition authority with bounded role rules;
- preserve legitimate Python such as the Camouhost outer adapter where the cross-language boundary is real;
- ensure `opsctl doctor` and other current tools no longer require the AR-6 census as a semantic dependency;
- do not add a Rust list of every Python file;
- after zero-caller/zero-unique-current-invariant proof, physically delete the current AR-6 census artifact while preserving accepted AR-6 evidence/history.

## 5. N3 — AR-7 current GitHub-governance authority normalization

### Historical purpose preserved

AR-7 established protected `main`, required PR/check semantics, hosted `rehearsal/staging/production` Environments, non-bypass production approval and hosted read-only governance audit.

### Defect to remove

Current required-check topology must not be reconstructed by accumulating historical overlays such as:

```text
AR-7 required checks + AR-10 extension + future AR-X extension + ...
```

### Current target ownership

```text
current workflow/check registration -> current GitHub Actions/governance registry
expected governance policy          -> one current typed/bounded governance policy owner
observed hosted state               -> raw GitHub observation from outer workflow/API layer
accepted AR-7 baseline              -> historical evidence
```

### Required cutover

- stop using `architecture/github-governance-ar7.json` as a current evolving required-check registry;
- move the two AR-10-added permanent contexts and all current check/workflow registration to the current governance owner rather than an AR-10 historical overlay;
- preserve live hosted-state verification and fail-closed mismatch behavior;
- keep provider/API observation outside pure Rust policy;
- historical AR-7 JSON may remain only as explicit immutable evidence if a real evidence consumer requires it; otherwise zero-caller/zero-unique-current-invariant proof permits deletion from the current authority estate.

## 6. N4 — bounded AR-8 operator/provenance authority cleanup

This is intentionally **not** a full credential-lifecycle rewrite.

### Historical purpose preserved

AR-8 established secrets/keys/OAuth concurrency, credential lifecycle, provider application-credential handling, profile security domains and metadata-only operator/rehearsal contracts.

### Required bounded changes

1. **Operator command/effect authority**
   - Rust CLI/command registry becomes the semantic owner of operator namespaces/actions/effect classes;
   - `architecture/operator-contract.json` must not dynamically authorize Rust CLI behavior;
   - if a machine-readable operator view is retained, it is a generated projection from the typed registry, not a second semantic authority.

2. **AR-8 provenance isolation**
   - `architecture/ar8-*` and equivalent AR-8B/AR-8D/AR-8E/AR-8F artifacts are historical/provenance inputs only;
   - normal current runtime/operator/inventory paths must not depend on historical phase overlays for semantics.

### Explicit deferral

`credential-lifecycle.json` and `profile-security.json` may remain bounded transitional subject contracts until their own typed ownership is justified by AR-13/AR-14 or another accepted bounded cutover. PF-1 may consume only a narrow `CredentialInventoryProjection`, not a raw global credential-authority bag.

## 7. N5 — AR-10 runtime semantic authority retirement

### Historical purpose preserved

AR-10 delivered the supported native Profile Bridge -> managed typed/versioned IPC -> real Camouhost -> pinned Camoufox persistent runtime path, retired historical direct executables and removed production `opsctl` child-process authority.

### Current target ownership

```text
runtime behavior / safety / launch semantics -> Product Rust
cross-language runtime dependency tuple       -> runtime/camouhost/runtime-lock.json
Camoufox-specific outer adapter               -> runtime/camouhost/real.py
IPC semantic contract                          -> bridge-domain + cross-language validation
runtime failure guarantees                     -> implementation + tests
permanent required CI contexts                 -> current GitHub governance owner
production/lifecycle state                     -> lifecycle/release owner
AR-10 acceptance/provenance                    -> Git/evidence
```

`runtime/camouhost/runtime-lock.json` remains a legitimate versioned cross-language manifest and must **not** be removed merely because `architecture/runtime-cutover-ar10.json` is retired.

### Required cutover

- reassign every unique current field in `architecture/runtime-cutover-ar10.json` to its natural owner or historical evidence;
- remove inventory, acceptance, governance, release and doctor dependencies on the AR-10 semantic manifest;
- do not move runtime execution into `opsctl`;
- preserve the real Camoufox/runtime regression matrix and synthetic-runtime test-only separation;
- after zero-caller/zero-unique-current-invariant proof, physically delete `architecture/runtime-cutover-ar10.json`.

## 8. PF-1 entry contract after N1..N5

PF-1 must begin from normalized owners, not from an AR-qualified authority bag.

Target PF-1 inputs are conceptually:

```text
ValidatedProgramSequence
RawArchitectureAcceptanceEvidenceV1
cut-over typed AcceptancePolicy
D1InventoryProjection
RuntimeTopologyProjection
ApplicationInventoryProjection
OperatorInventoryProjection
GovernanceInventoryProjection
RuntimeInventoryProjection
CredentialInventoryProjection
ReleaseInventoryProjection
other bounded typed projections discovered on the exact candidate
```

The compiler does not receive raw historical AR manifests merely because an old Python generator did.

PF-1's scope is therefore narrowed to:

```text
outer Git/GitHub raw observations
-> typed lifecycle/acceptance evaluator
-> DerivedLifecycleStateV1
-> bounded validated inventory projections
-> pure deterministic ArchitectureInventoryCompiler
-> canonical render/check/inspect/write
-> delete legacy Node lifecycle engine
-> delete legacy Python inventory/projection cluster
```

PF-1 must not recreate a `GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet -> every module` architecture.

## 9. Transaction and merge discipline

Each normalization step is a bounded authority transaction. Accepted `main` is re-baselined before the next transaction. A step may use temporary candidate parity, but one merged state must never leave both predecessor and successor as legitimate current authorities.

For each step require:

```text
fresh protected-main baseline
complete caller discovery
field/invariant ownership matrix
positive parity proof
negative anti-regression proof
zero current callers to predecessor
zero unique current invariants in predecessor
exact-head CI/governance evidence
physical retirement/demotion in the same merge candidate
post-merge reread
```

A single implementation PR may contain one normalization transaction. The five transactions do not need to be forced into one giant PR; their **order is binding**.

## 10. Non-goals

This normalization block does not:

- rewrite accepted AR history;
- add new product functionality;
- activate production capabilities;
- provision/delete hosted Cloudflare resources;
- implement PF-2/PF-3/FC-6 early;
- start AR-12;
- globally ban Python or JSON;
- convert legitimate manifests/evidence/observations into Rust constants;
- create runtime-to-opsctl dependencies or RPC;
- create generic DI/plugin/service-locator infrastructure;
- rewrite the full AR-8 credential lifecycle;
- rewrite AR-11 release architecture.

## 11. Exit criteria for PF-1 entry

PF-1 is unblocked only when all five normalization transactions are accepted on protected `main` and the exact current tree proves:

```text
runtime-topology-ar2 current semantic authority = 0
python-estate-ar6 current semantic authority = 0
historical AR-7 overlays used as current evolving governance = 0
operator-contract JSON used as CLI authorization authority = 0
AR-8 provenance artifacts used as normal current semantic input = 0
runtime-cutover-ar10 current semantic authority = 0
runtime product dependency on opsctl = 0
opsctl runtime execution authority = 0
generated projection used as semantic source for its own facts = 0
```

Accepted functionality, source behavior, security boundaries and production fail-closed state must remain unchanged.

Only then does the continuation advance to PF-1.