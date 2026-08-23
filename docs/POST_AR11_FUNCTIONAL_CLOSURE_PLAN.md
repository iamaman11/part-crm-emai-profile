# Post-AR-11 Functional Closure Plan — Release / Promotion Contract to 10/10

**Document status:** SUBORDINATE_REMEDIATION_PLAN  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Mandatory architecture requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**Architecture quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**PF-1:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` / #430  
**PF-2:** fresh implementation from accepted PF-1 `main`; closed PR #428 is superseded historical salvage only  
**PF-3:** `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431  
**Live tracker:** #399  
**FC-6 tracker:** #421  
**AR-12 implementation:** FORBIDDEN inside this plan  
**Production enablement:** FORBIDDEN

This is the single subordinate execution plan for Post-AR-11 Functional Closure. It preserves accepted AR-11 history and does not create a new AR/PF lifecycle sequence.

## 1. Binding continuation order

```text
F1  Release Set breaking-contract version discipline
+
F2  permanent architecture foundations
    - application architecture mandatory contract
    - opsctl pure-core / adapter boundary
    - opsctl doctor diagnostic boundary
    - canonical JSON/digest contract
    - Python usage/effect boundary
 ->
N1  AR-2 runtime/resource topology current-authority retirement
 ->
N2  AR-6 Python-estate authority retirement + Python role/effect normalization
 ->
N3  AR-7 current GitHub-governance normalization
 ->
N4  bounded AR-8 operator/provenance cleanup
 ->
N5  AR-10 runtime semantic-authority retirement
 ->
PF-1 Canonical Architecture Inventory + typed lifecycle-policy cutover
 ->
PF-2 Universal Hosted Operational Evidence from a fresh accepted PF-1 base
 ->
PF-3 typed Architecture Fitness Baseline
 ->
fresh re-baseline #399/#421
 ->
FC-6 real staging same-bits / rollback rehearsal
 ->
FC-7 final whole-AR-11 functional audit
 ->
AR-12 implementation entry
```

F1/F2/N1…N5 are foundation/normalization transactions, not new lifecycle slices. `architecture/architecture-program-sequence.json` is unchanged.

No path may skip normalization, PF-3 or fresh re-baseline and resume FC-6 from stale evidence. Current accepted execution state is tracked only by #441/#399 and protected `main`; this static plan does not hard-code a moving implementation SHA.

## 2. Non-negotiable invariants

```text
one protected main
one architecture hierarchy
one schema/compatibility history
one Release / Capability Profile production-enable authority
source_present != production_enabled
build once -> promote same immutable bits
GitHub Actions/Environments = orchestration/approvals/credential/hosted-observation boundary
opsctl = offline typed policy/planning/verification/projection only
official provider tools / owned executors = actual provider mutation boundary
```

Production remains fail-closed:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

Do not introduce Terraform, hidden generic IaC state, a second inventory/release/capability/lifecycle/evidence authority, provider mutation in `opsctl`, rebuild-on-promotion or AR-12 implementation.

## 3. Why normalization precedes PF-1

Current PF-1 originally assumed historical AR-qualified JSON/Python inputs. That would turn PF-1 into a Rust translation of the legacy architecture engine rather than a clean boundary.

Pre-PF-1 normalization first returns facts to natural owners:

```text
AR-2 topology -> Wrangler/provider config + Product ownership
AR-6 Python estate -> repository/source role/effect policy
AR-7 current governance -> current desired governance data + live observation
AR-8 operator semantics -> Rust CommandRegistry/effect registry
AR-10 runtime semantics -> Product Rust + runtime-lock + Camouhost adapter + tests
```

Then PF-1 can consume bounded typed projections rather than raw historical authority documents.

## 4. F1 — Release Set version discipline

The breaking change from:

```text
d1_evolution_authority_sha256
```

to:

```text
d1_repository_identity_sha256
```

must not silently retain the same current Release Set v2 contract meaning.

Requirements:

1. re-read #399/#421 to determine whether historical immutable v2 A/B artifacts remain required consumers;
2. create a new current writer/model version (target v3 unless exact evidence proves another bounded decision);
3. never rewrite immutable historical v2 assets;
4. isolate any required `HistoricalReleaseSetV2Decoder/Verifier` from the current writer/model;
5. keep current release-set IDs/content-address rules explicit and versioned;
6. update all tests/workflows/docs atomically with the contract transition.

## 5. F2 — foundational architecture contracts

Before N1, accept the permanent boundaries in:

- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`;
- `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`;
- `docs/OPSCTL_DOCTOR_CONTRACT.md`;
- `docs/PYTHON_USAGE_BOUNDARY.md`.

F2 also establishes the reviewed canonical external JSON/digest layer required by PF-2:

```text
strict bounded decode
duplicate-member rejection
explicit kind + schema_version
canonical semantic bytes where applicable
exact-file bytes where explicitly contracted
reviewed/pinned SHA-256
independent vectors
pretty rendering separate from canonical identity
```

## 6. N1…N5 — authority estate normalization

Detailed DoD is `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`. Live accepted-main handoff/state is #441.

Each transaction follows:

```text
find natural owner
-> preserve accepted functionality/invariants
-> switch current consumers
-> old caller_count = 0
-> old unique_current_invariant_count = 0
-> delete/demote predecessor
-> preserve Git/evidence history
```

### N1 — AR-2

Retire current semantic dependence on `architecture/runtime-topology-ar2.json` where real topology is already owned by provider config/Product Rust/Release Profile.

### N2 — AR-6 / Python

Retire the AR-6/AR-10/AR-11 per-file Python estate overlay chain as current authority. Preserve legitimate Python by role/effects. No successor 1:1 file registry.

Also:

- update `opsctl doctor` and repository-root detection away from AR-6/Python sentinels;
- preserve `runtime/camouhost/real.py` as legitimate runtime adapter;
- preserve `runtime/camouhost/main.py` as synthetic/test-only;
- migrate bespoke provider-mutation helpers to protected official tooling where available and then delete old paths after parity/zero callers.

### N3 — AR-7 governance

Current required hosted checks/configuration must not be computed as historical AR-7 baseline + AR-10 overlay + future overlays. Use current desired governance data plus live observation and typed policy evaluation. Historical AR-7 evidence remains history.

### N4 — bounded AR-8

Move operator command/effect semantic ownership to Rust. `operator-contract.json` becomes generated/parity representation or is retired as current semantic owner. Do not globally rewrite credential/security subject contracts in this bounded step.

### N5 — AR-10

Retire `architecture/runtime-cutover-ar10.json` as current semantic authority after all unique facts are reassigned.

Keep:

```text
Product Rust runtime behavior
runtime/camouhost/runtime-lock.json as versioned cross-language manifest
runtime/camouhost/real.py as Camoufox adapter
runtime tests
current GitHub governance
Git/evidence history
```

N2–N5 must make their natural source owners unambiguous but must not pre-build a generic PF-1 projection/compiler framework. The exact handoff conditions are tracked in #441.

## 7. PF-1 — lifecycle + canonical inventory

PF-1 starts only after N5 acceptance and a fresh #430 entry-gate reread against then-current protected `main`.

PF-1 target:

```text
outer Git/GitHub raw observations
-> typed LifecycleEvaluator
-> DerivedLifecycleStateV1

bounded owner projections
-> pure ArchitectureInventoryCompiler
-> architecture/inventory.json generated projection
```

Forbidden:

```text
GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet
inventory.json as semantic source
opsctl -> Git/GitHub/Node/Python/provider/network
```

PF-1 deletes the old Node lifecycle policy and Python architecture inventory/projection cluster after parity + zero-callers/unique-invariants.

`opsctl doctor` and repository-root detection are mandatory caller surfaces in the cutover; no retired AR/Python/Node sentinel remains required.

Only accepted PF-1 `main` may become PF-2 base.

## 8. PF-2 — Universal Hosted Operational Evidence

PF-2 must start from a **new clean branch based on accepted PF-1 protected `main`**. Closed PR #428 predates normalization and is historical/selective-salvage material only; it must not be resumed, rebased or treated as accepted PF-2 evidence.

Target reusable architecture:

```text
GitHub Actions / official provider tools
        ↓
secret-free raw observation
        ↓
strict versioned observation DTO
        ↓
typed Rust EvidencePolicy
        ↓
HostedEvidenceEnvelopeV1
        ↓
canonical durable JSON
        ↓
immutable Actions Artifact / GitHub Artifact Attestation
```

`opsctl` remains offline. It does not become provider observer, signer, network client or mutation executor.

### Trust model

Different layers prove different things:

| Layer | Proves | Does not prove |
| --- | --- | --- |
| Provider/GitHub observation | what authorized observer received at observation time | independent semantic truth |
| Typed envelope/policy | schema/context/policy validity | truthfulness of arbitrary source |
| Digest | exact subject identity | observer authority |
| GitHub Artifact Attestation | subject-byte binding to hosted provenance | business correctness |
| `opsctl evidence verify` | locally checkable contract/context/policy | live provider truth without trusted observation |

### Freshness/replay

At minimum distinguish:

```text
VALID
VALID_BUT_STALE
INVALID
```

Stale/invalid evidence is blocked from mutation admission where current evidence is required.

### Current Python convergence

`scripts/check-external-review-attestations.py` currently combines GitHub API acquisition and semantic verification. PF-2 must split acquisition to outer observation; validity/trust/freshness semantics move to typed Rust pure policy. Retained Python may observe, not own the evidence decision.

### PF-2 DoD

PF-2 closes only when:

- one reusable envelope/publication model exists;
- supported payloads are typed/versioned/fail-closed;
- canonical/digest foundation is shared with accepted F2;
- formal trust/freshness/replay semantics are tested;
- publisher has no provider credential/mutation authority;
- official attestation verification is demonstrated where required;
- no second evidence backend/framework/PKI is introduced;
- exact-head CI/guarded merge/accepted-main reread succeed;
- production remains fail-closed.

Only accepted PF-2 `main` may become PF-3 base.

## 9. PF-3 — typed Architecture Fitness Baseline

PF-3 semantic authority is Rust `FitnessRuleRegistry` or equivalent, **not** a hand-maintained `architecture-fitness-policy.json`.

```text
FitnessRuleRegistry
-> evaluator/enforcement mapping
-> positive/negative fixtures
-> Architecture Fitness Gate
-> optional generated report/index
```

Minimum families cover authority uniqueness, dependency direction, explicit effects, typed contracts, Release Profile admission, persistence/config/events, cutover discipline, `opsctl`, `doctor`, Python role/effects and developer readability.

PF-3 also prevents silent weakening and establishes fixed zero/one safety budgets plus measured tooling budgets.

Only accepted PF-3 `main` permits FC-6 resume.

## 10. Fresh re-baseline gate after PF-3

Before any FC-6 mutation ceremony:

1. reread protected `main` exact SHA;
2. reread #399/#421 and open competing PRs/issues;
3. rediscover protected required contexts/live GitHub governance;
4. verify current Actions registry/desired state vs live observation;
5. resolve exact immutable Release Sets A/B from accepted main;
6. verify staging observation credentials through owned credential/evidence paths;
7. verify PF-3 required rules/budgets green;
8. verify no AR-12 implementation entered source;
9. verify production remains fail-closed.

Update #399/#421 only from this fresh baseline.

## 11. FC-6 — real staging same-bits / rollback rehearsal

FC-6 proves accepted AR-11 release/promotion behavior against real staging. It is not AR-12 provisioning and not production.

Use immutable accepted Release Sets:

```text
A = older accepted-main Release Set
B = newer accepted-main Release Set
```

### Ceremony state machine

Minimum progression:

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

Terminal/control outcomes include:

```text
FAILED_PRE_MUTATION
MUTATION_OUTCOME_UNKNOWN
FAILED_POST_MUTATION
BLOCKED
NO_CHANGE
SUCCEEDED
```

Every transition binds predecessor, evidence identities, allowed effect class, fence/idempotency identity, retry policy and terminal audit obligation.

### Unknown mutation outcome

If provider mutation may have been sent but result is ambiguous:

```text
MUTATION_OUTCOME_UNKNOWN
-> read-only reconciliation
-> APPLIED | NOT_APPLIED | DIVERGED | STILL_UNKNOWN
```

No blind retry.

- `APPLIED` -> exact target post-verify;
- `NOT_APPLIED` -> retry only if fence/idempotency still permits;
- `DIVERGED` -> block and invoke owned rollback/recovery policy;
- `STILL_UNKNOWN` -> block/escalate.

This does not become generic AR-14 recovery.

### Required proof

FC-6 must demonstrate, as applicable:

```text
A and B resolve to durable immutable accepted assets
same bits promoted; no rebuild during promotion
schema/runtime compatibility evaluated by current typed owners
stale expected-current fence rejected
NO_CHANGE behaves as first-class convergence
A -> B staging promotion verified
B -> A rollback verified when compatible
rollback blocked/fail-forward classified when incompatible
ambiguous mutation path reconciled fail-closed
all evidence bound to exact source/release/environment/run identities
production mutation remains impossible
```

If a legitimate external prerequisite is unavailable, FC-6 may produce a typed, evidence-backed `BLOCKED` state; it must not fake success or weaken a gate.

## 12. FC-7 — final whole-AR-11 functional audit

After FC-6 terminal evidence:

- re-audit all AR-11 Functional Closure requirements;
- verify F1/F2/N1…N5/PF-1/PF-2/PF-3 accepted owners remain current;
- verify no retired legacy path regained callers;
- verify `source_present != production_enabled` and production fail-closed state;
- verify repository-owned severity for closure scope is `P0=0`, `P1=0`, `P2=0`;
- update canonical projections/trackers without creating second authority;
- only then permit AR-12 implementation entry.

## 13. Global Definition of Done

Post-AR-11 Functional Closure prerequisite program is complete only when:

1. F1/F2 are accepted;
2. N1…N5 complete with zero-current-caller/zero-unique-invariant retirement proofs;
3. PF-1 accepted and legacy Node/Python lifecycle/inventory owners deleted;
4. PF-2 accepted with hosted evidence trust/freshness/attestation proofs;
5. PF-3 accepted with typed fitness semantics and anti-weakening;
6. fresh #399/#421 re-baseline completed;
7. FC-6 terminal real-staging evidence accepted or legitimate typed `BLOCKED` state explicitly remains the blocker;
8. FC-7 whole-scope audit passes with P0/P1/P2 zero for repository-owned closure defects;
9. exact-head CI/protected contexts/guarded merge discipline held at every acceptance;
10. production remains disabled and AR-12 source remains not started until closure is accepted.
