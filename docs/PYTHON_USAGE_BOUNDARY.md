# Python Usage and Authority Boundary

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Mandatory architecture requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**Production authorization:** NONE

Python is not prohibited in this repository. It is also not a default architecture layer. A Python file is legitimate only when its role, effects and semantic ownership are clear and it does not create a second current authority for Product Runtime, `opsctl`, release/promotion policy, lifecycle, evidence policy, fitness policy or provider mutation.

This contract replaces the long-term idea of maintaining a hand-edited per-file Python estate database. The repository tree is the observation of which Python files exist; this policy classifies allowed roles and forbidden authority/effect combinations.

## 1. Audit findings on current `main`

The accepted AR-6 estate document currently reports a 119-file Python estate and still records paths classified for AR-10 deletion/migration. At least some of those recorded legacy paths are already physically absent from current `main`; therefore `architecture/python-estate-ar6.json` is useful historical provenance but is not a reliable permanent current Python registry.

Current Python usage falls into materially different categories:

```text
A. genuine Product Runtime outer adapter
B. synthetic/test runtime fixture
C. repository validator / structural checker
D. deterministic artifact / contract generator
E. tests / fixtures
F. developer-local orchestration
G. outer hosted/provider observation
H. provider canary / bounded mutation
I. transitional legacy executable or duplicate semantic authority
```

Those categories must not be collapsed into a single `KEEP_PYTHON` label.

## 2. Permanent principle

```text
Python may adapt, observe, generate, test or host a genuine cross-language runtime.
Python must not become a second semantic owner or an ungoverned mutation authority.
```

Language choice is secondary to ownership/effects. A correct Python adapter may be better than a forced Rust rewrite. A Python script that duplicates current semantic policy or bypasses the product/operator authority hierarchy must be retired even if it is small and convenient.

## 3. Allowed role: Product Runtime cross-language adapter

### Current canonical example

`runtime/camouhost/real.py` is an allowed and necessary Python Product Runtime adapter because the real Camoufox/BrowserForge/Playwright integration is a genuine cross-language/runtime boundary.

Its permanent role is narrowly:

```text
Profile Bridge (Rust)
        ↓
versioned/bounded IPC + validated environment/runtime manifest
        ↓
Camouhost Python adapter
        ↓
Camoufox / BrowserForge / Playwright
        ↓
browser process/context
```

Required invariants:

- Product/application business semantics remain in Rust owners; Camouhost does not become a client/profile/mailbox business-service layer;
- Profile Bridge remains the accepted local lifecycle/writer/composition authority;
- only the accepted Bridge/runtime path launches the real adapter in supported product execution;
- stdin/stdout protocol is bounded and versioned; stdout is protocol-only and diagnostics go to stderr;
- runtime dependency versions and identity are pinned by `runtime/camouhost/runtime-lock.json` or its accepted successor;
- runtime manifest is an external cross-language data contract, not duplicated business policy;
- profile root, config and proxy inputs are bounded/validated; secret-bearing proxy material is not logged or emitted through IPC;
- one active browser writer per generation/profile boundary is enforced fail-closed;
- the adapter does not acquire independent provider/cloud/database mutation authority;
- unsupported/ambiguous runtime states fail closed;
- cross-platform OS-specific lock probing remains adapter/runtime responsibility and is tested on supported platforms;
- no direct dependency from Product Runtime to `opsctl` is introduced.

A rewrite of this adapter to Rust is **not** a goal unless a future evidence-backed design proves it reduces a real boundary/risk without losing Camoufox support.

## 4. Allowed role: synthetic runtime fixture

`runtime/camouhost/main.py` is allowed only as deterministic repository/test evidence.

Required invariants:

```text
synthetic fixture != production runtime
```

It must not be selectable as a production Camoufox implementation through an ungoverned flag. Tests must distinguish fake/synthetic evidence from real Camoufox cold-launch/runtime evidence.

## 5. Allowed role: repository validator / structural checker

Python may remain a practical implementation language for bounded repository validation, especially for text/AST/file-layout checks.

Allowed examples include checks for:

- forbidden dependency/import edges;
- generated contract drift;
- source-layout invariants;
- migration-file syntax/ordering observations;
- documentation/projection consistency;
- test fixture validation.

But a validator must not silently become a second semantic authority.

Target separation:

```text
repository/source observation
        ↓
validator/observer implementation
        ↓
normalized facts
        ↓
owned semantic rule / fitness evaluation
```

A Python checker may own implementation details of how a structural fact is observed. It must not maintain a parallel copy of business/runtime/release/lifecycle policy that already has a typed current owner.

Specific consequence:

- giant phase/AR-qualified ownership tables such as those used by the legacy architecture inventory cluster are transitional and must be retired by N1/N2/PF-1/PF-3 as their natural owners become available;
- PF-3 may keep bounded Python enforcement adapters where justified, but rule identity/applicability/anti-weakening semantics are owned by the typed Rust `FitnessRuleRegistry` or equivalent bounded typed owner;
- new permanent Python validators must not encode a second mutable capability/release/lifecycle registry.

## 6. Allowed role: deterministic generator / renderer

Python may generate artifacts when the output is a representation/projection and the generator is not a competing semantic owner.

Legitimate categories can include:

```text
OpenAPI rendering
frontend contract rendering
release artifact packaging/provenance rendering
provider-native config rendering
runtime bundle assembly
bootstrap SQL materialization/verification
```

Required invariants:

- source semantic facts belong to their natural owner;
- generated files are marked/treated as projections or artifacts;
- generator output can be deterministically checked for drift;
- generator does not perform hidden provider deployment/mutation unless explicitly classified as a separate outer executor;
- generator does not introduce a second production-enable authority;
- if a generator currently embeds unique semantic topology/policy, the owning normalization slice must extract that semantic fact before retaining the generator.

N1 must specifically inspect Python that renders Cloudflare deployment config so provider topology semantics are not duplicated between Python tables and Wrangler/current product owners.

## 7. Allowed role: tests and fixtures

Python tests/fixtures remain allowed when they validate behavior/contracts and are not production dependencies.

Test code may use local SQLite, temporary files, subprocesses or synthetic fixtures as needed, provided:

- no production authorization is inferred from a test-only path;
- secrets are not persisted in fixtures;
- external mutation tests are isolated and explicitly owned;
- permanent production code does not depend on test Python modules.

A broad Python-to-Rust test rewrite is not required.

## 8. Allowed role: developer-local orchestration

A developer helper such as `scripts/verify-fast.py` may invoke subprocesses (`cargo`, Python validators, Node during transition) because it is an outer developer orchestration shell, not `opsctl` pure policy.

Required invariants:

- it is non-authoritative and cannot substitute for exact-head permanent CI;
- it does not become Product Runtime or production mutation authority;
- it does not carry provider credentials;
- as predecessor tools are retired, its command list must converge to current accepted owners rather than preserve deleted Node/Python semantic engines;
- its subprocess capability does **not** relax the `opsctl` prohibition on process execution.

## 9. Outer hosted/provider observation

Python may perform provider/GitHub reads only as an explicitly outer observation adapter where use of Python is justified.

Target flow:

```text
GitHub/provider API
        ↓
Python/workflow observation adapter
        ↓
raw or normalized versioned observation
        ↓
Rust pure policy / evidence / lifecycle evaluation
```

The observer must not combine external acquisition with the semantic decision when the target architecture has a typed policy owner.

### Current convergence target

`scripts/check-external-review-attestations.py` currently performs both GitHub API GETs and semantic verification of claim identity/body/timestamp. This is acceptable transitional current behavior but is **not** the PF-2 target boundary.

PF-2 must split it conceptually into:

```text
outer GitHub review observation
        ↓
versioned HostedEvidence/ReviewObservation DTO
        ↓
pure Rust EvidencePolicy / attestation verification
```

After PF-2 cutover, any retained Python/network component is observation acquisition only unless a narrower justified adapter role is explicitly documented.

## 10. Provider mutation: default forbidden for Python

New Python provider/deployment mutation authority is forbidden by default.

Actual provider mutation belongs to protected workflow orchestration plus official provider tooling or a narrowly owned provider executor. `opsctl` remains policy/plan/verify only.

A Python provider-mutating helper is allowed only as a temporary or exceptional bounded adapter when all are proven:

1. there is no suitable official/pinned provider tool or existing owned adapter;
2. mutation scope is disposable/test-only or explicitly authorized by the owning plan;
3. credentials are workflow-scoped and least-privilege;
4. pre/postconditions and cleanup are fail-closed;
5. no business/runtime semantic authority is embedded;
6. a retirement/continued-justification disposition exists.

### Current R2 canary disposition

`tools/r2_s3_canary.py` directly reads R2 access key + secret, implements AWS SigV4 itself and performs remote PUT/LIST/GET/DELETE. That is higher risk than required for the target architecture.

Current Cloudflare Wrangler supports remote R2 object `put`, `get` and `delete`; Wrangler v4 requires explicit `--remote` for remote object operations. Therefore N2 must evaluate and, absent a proven blocker, replace this bespoke Python/SigV4 provider-mutation helper with a protected workflow using the pinned official Wrangler R2 object commands, preserving the same ephemeral canary and cleanup proof. After parity and zero-callers, delete `tools/r2_s3_canary.py`.

The target result is:

```text
GitHub protected workflow
        ↓
pinned Wrangler --remote
        ↓
ephemeral R2 canary mutation
        ↓
secret-free observation/evidence
```

No `opsctl` provider credentials or mutation are introduced.

## 11. Forbidden Python roles

Permanent Python is forbidden as:

```text
second Product business/domain authority
second release/capability admission authority
second D1 compatibility/rollout authority
second lifecycle/acceptance authority
second evidence-validity policy authority after PF-2 cutover
second fitness-rule semantic registry after PF-3 cutover
hidden production mutation executor
secret readback/reporting tool
untracked background daemon/service
runtime path that bypasses Profile Bridge for browser/profile lifecycle
compatibility shim with zero current consumers
```

## 12. Python and `opsctl`

`opsctl` must never regain a Python child-process dependency for policy/validation.

Forbidden:

```text
opsctl -> python validator
opsctl -> python generator -> semantic result
opsctl -> python provider observer
opsctl -> python mutation executor
```

If a Python outer observer is needed, workflow/orchestration runs it first and passes explicit versioned data to `opsctl`.

Developer-local scripts may call `opsctl`; `opsctl` does not call them back.

## 13. Python and Product Runtime

The only currently justified production Python execution path is the owned Camouhost cross-language runtime boundary (plus any future separately accepted genuine cross-language runtime adapter).

Product runtime rules:

- no HTTP/Worker/domain crate calls arbitrary Python scripts;
- no Python script becomes a sidecar service unless a future architecture slice proves a real independent process boundary and owns its protocol/lifecycle/security;
- no direct Python browser/profile lifecycle tool may coexist as an alternative legitimate path to Profile Bridge;
- Python runtime dependencies are pinned and reproducibly packaged;
- runtime startup verifies expected versions/manifest identity fail-closed;
- Python runtime diagnostics obey secret/data-classification policy.

## 14. No permanent Python estate database

Do not replace `architecture/python-estate-ar6.json` with another JSON/TOML/YAML/Rust list of every Python file.

Target classification is rule-based and source-derived:

```text
repository tree
        ↓
structural/effect observations
        ↓
Python role/effect policy
        ↓
compliance result
```

PF-3 may enforce role/effect rules mechanically, including detection of new Python production/network/provider-mutation entrypoints, without maintaining a hand-edited 1:1 file registry.

## 15. N2 — mandatory Python normalization work

N2 must do all of the following before PF-1 entry:

1. stop treating `architecture/python-estate-ar6.json` as current semantic authority;
2. remove `scripts/python-estate-ar6.py` from current authority/caller chains and delete it when zero-callers/zero-unique-current-invariants are proven;
3. update `opsctl` repository-root discovery so it does not require AR-6 estate artifacts or legacy Python inventory scripts;
4. classify current Python by **role and effects**, not per-file historical AR status;
5. prove legacy AR-10 direct Python browser/profile executables remain physically absent and unreferenced;
6. identify Python validators/generators whose unique semantic facts must be reassigned by N1/N4/N5/PF-1/PF-3;
7. retain `runtime/camouhost/real.py` as legitimate runtime adapter and `runtime/camouhost/main.py` as test-only fixture with explicit separation;
8. classify GitHub/provider-read scripts as outer observers or transitional observer+policy tools; PF-2 owns the evidence-policy cutover;
9. replace `tools/r2_s3_canary.py` with pinned official Wrangler-based remote canary workflow unless exact implementation evidence proves a blocking requirement; preserve canary cleanup/evidence and then delete the bespoke Python path;
10. update developer orchestration such as `verify-fast.py` as Node/Python semantic predecessors are retired;
11. add negative tests that new Python runtime/provider-effect entrypoints fail closed unless they match an explicitly allowed role.

N2 does **not** globally rewrite Python tests, generators or valid adapters to Rust.

## 16. PF-1 / PF-2 / PF-3 consequences

### PF-1

PF-1 deletes the legacy Python architecture inventory/projection cluster after parity and zero-caller proof. Python cannot remain the semantic lifecycle/inventory owner after the Rust cutover.

### PF-2

PF-2 may reuse Python/workflow outer observation acquisition, but evidence normalization/validity/freshness/trust decisions live in typed Rust pure policy. Hosted evidence publication/attestation remains workflow/GitHub infrastructure responsibility.

### PF-3

PF-3 must enforce at least:

```text
unclassified_python_production_entrypoint = 0
unclassified_python_network_or_provider_effect = 0
python_duplicate_semantic_authority = 0
python_runtime_bypass_of_profile_bridge = 0
opsctl_python_child_process = 0
python_provider_mutation_without_explicit_exception = 0
python_secret_readback_surface = 0
legacy_python_estate_registry_current_authority = 0
```

The fitness implementation may use bounded Python source observers where justified, but the semantic rule registry is typed Rust and there is no second manual JSON/Python rule catalog.

## 17. Definition of Done for a new or changed Python entrypoint

A Python entrypoint is acceptable only when its PR proves:

- one explicit role from this contract;
- natural semantic owner identified;
- effect set identified;
- no duplicate current authority;
- no hidden production-enable path;
- no undeclared provider/network/secret capability;
- bounded/versioned external contract where it crosses process/system boundaries;
- deterministic tests for policy-free transformation where applicable;
- negative tests for unsafe inputs/effects where applicable;
- exact-head CI and cross-platform proof when the entrypoint is cross-platform;
- retirement disposition when the role is transitional.

The desired developer mental model is:

```text
Rust owns product/operator semantics.
Python is allowed at genuine adapters, observations, generators and tests.
Workflows/official provider tools own hosted effects.
No language is allowed to create a second authority.
```