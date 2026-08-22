# Documentation Authority Index

**Document status:** GENERATED_PROJECTION  
**Current program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)  
**Tracking:** #266

This index is a navigation/projection document. It does not create a second roadmap or semantic authority.

## Current state

- AR-0…AR-11 are accepted history/checkpoints.
- AR-12 is derived current in the static program sequence but implementation is **NOT STARTED**.
- `architecture_complete=false`.
- `production_core_gate=BLOCKED`.
- `production_ready=false`.
- Post-AR-11 Functional Closure #399 is the current execution umbrella.
- The prerequisite order before AR-12 is:

```text
F1 Release Set version discipline
+
F2 permanent application/opsctl/doctor/canonical-JSON/Python foundations
 ->
N1 AR-2 authority retirement
 ->
N2 AR-6 Python-estate authority retirement
 ->
N3 AR-7 current governance normalization
 ->
N4 bounded AR-8 operator/provenance cleanup
 ->
N5 AR-10 runtime semantic-authority retirement
 ->
PF-1 typed lifecycle + deterministic bounded-projection inventory cutover
 ->
PF-2 Universal Hosted Operational Evidence
 ->
PF-3 typed Rust Architecture Fitness Baseline
 ->
fresh #399/#421 re-baseline
 ->
FC-6
 ->
FC-7
 ->
AR-12 implementation entry
```

F1/F2/N1…N5 are foundation/normalization transactions, not AR/PF lifecycle slices. `architecture/architecture-program-sequence.json` is unchanged.

## Current normative hierarchy

1. [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — single current architecture/program execution authority.
2. [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) — permanent prospective application architecture requirements.
3. [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — permanent quality/evolution rules.
4. [`OPSCTL_ARCHITECTURE_BOUNDARY.md`](OPSCTL_ARCHITECTURE_BOUNDARY.md) — permanent standalone `opsctl` pure-core/adapter/effect boundary.
5. [`OPSCTL_DOCTOR_CONTRACT.md`](OPSCTL_DOCTOR_CONTRACT.md) — permanent `opsctl doctor` diagnostic boundary.
6. [`PYTHON_USAGE_BOUNDARY.md`](PYTHON_USAGE_BOUNDARY.md) — Python role/effect/authority policy.
7. [`PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`](PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md) — F1/F2 + N1…N5 prerequisite specification.
8. [`POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md) — subordinate execution plan through FC-7.
9. [`PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`](PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md) — PF-1 detailed lifecycle/inventory cutover contract.
10. [`PF3_ARCHITECTURE_FITNESS_BASELINE.md`](PF3_ARCHITECTURE_FITNESS_BASELINE.md) — PF-3 typed fitness semantics/enforcement contract.
11. Stable bounded authorities such as [`ARCHITECTURE.md`](ARCHITECTURE.md), accepted ADRs, [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md), [`THREAT_MODEL.md`](THREAT_MODEL.md), [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) and current domain/runtime/release contracts.
12. [`../architecture/inventory.json`](../architecture/inventory.json), `status.json`, `DEVELOPMENT_PLAN.md`, this index and README entrypoints — generated/current projections only.
13. Historical AR/evidence documents — provenance, not automatically current semantic authorities.

An open PR/branch never outranks accepted protected `main`.

## Mandatory application model

```text
natural canonical owner
-> typed policy/contracts
-> bounded-context domain + application
-> ports/adapters/effects
-> composition roots
-> Release / Capability Profile admission
-> production exposure
```

Permanent rules include:

```text
one semantic fact -> one natural owner
observation != policy decision
Pure Core / Effect Shell
source_present != production_enabled
Product Runtime -> opsctl = forbidden
generated projection used as semantic source = forbidden
cutover -> zero callers -> zero unique current invariants -> delete DEAD predecessor
```

## `opsctl` navigation

Permanent architecture:

```text
CLI/composition
-> filesystem/JSON adapters
-> versioned DTOs
-> typed semantic input
-> PURE CORE
-> typed result
-> output adapter
```

Hard zero budgets include:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
Product Runtime -> opsctl/opsctl-core = 0
global authority bag = 0
```

`opsctl doctor` is read-only local diagnostic composition and cannot become a provider observer, process runner, runtime launcher, second authority catalog or CI substitute.

## Python navigation

Python is allowed by role/effects, not by a permanent per-file whitelist.

Legitimate examples include:

```text
runtime/camouhost/real.py -> genuine Camoufox outer runtime adapter
runtime/camouhost/main.py -> synthetic/test fixture
repository validators/source observers
deterministic generators/renderers
tests/developer orchestration
outer observation adapters where justified
```

Forbidden permanent Python roles include duplicate Product/release/D1/lifecycle/evidence/fitness semantic authority, runtime bypass of Profile Bridge, hidden provider mutation and secret readback.

The AR-6/AR-10/AR-11 Python estate overlay chain is transitional current-authority machinery and is retired by N2; no successor 1:1 Python file registry is permitted.

## PF-1 target

```text
RawArchitectureAcceptanceEvidenceV1
-> typed LifecycleEvaluator
-> DerivedLifecycleStateV1

bounded typed inventory projections
-> pure ArchitectureInventoryCompiler
-> architecture/inventory.json
```

PF-1 must not create a `GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet`. It retires/deletes the legacy Node lifecycle engine and Python inventory/projection cluster after parity + zero-caller/zero-unique-invariant proof.

## PF-2 target

```text
outer GitHub/provider observation
-> strict DTO
-> typed Rust EvidencePolicy
-> HostedEvidenceEnvelopeV1
-> canonical durable JSON
-> immutable artifact/attestation
```

## PF-3 target

```text
typed Rust FitnessRuleRegistry
-> evaluator/enforcement mapping
-> positive/negative fixtures
-> Architecture Fitness Gate
-> optional generated report/index
```

A manually maintained semantic `architecture/architecture-fitness-policy.json` is not the target owner.

## Current delivery map

| Dimension | Status |
| --- | --- |
| Source implemented | Accepted through AR-11 |
| AR-12 | Derived current / NOT STARTED |
| Staging | Partial non-production foundations only |
| Production authorized | NO |
| Production enabled | NO |
| Current umbrella | Post-AR-11 Functional Closure #399 |
| Current next work | F1/F2, then N1…N5 |

## Historical/evidence context

Accepted AR-0…AR-11 documents, `history/**`, `docs/evidence/**`, `architecture/accepted-phases.json` and Git history preserve provenance. Historical files may contain wording that was current in their accepted context; that does not make them present current authority.
