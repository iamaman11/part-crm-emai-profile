# Architecture Enforcement Guardrails

**Document status:** SUBORDINATE_NORMATIVE_ENFORCEMENT_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Mandatory application requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**opsctl doctor boundary:** `docs/OPSCTL_DOCTOR_CONTRACT.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**Execution tracker:** #441  
**Production authorization:** NONE

This document does not introduce a new roadmap, lifecycle slice, capability registry, architecture inventory, semantic authority catalog, plugin system or policy framework. It makes already accepted architecture principles mechanically enforceable during F1/F2 and every later bounded transaction.

The governing rule is:

```text
architecture rule without an enforcement owner is guidance, not a permanent invariant
```

For every REQUIRED invariant, the repository must converge on one primary enforcement owner and at least one negative proof where a realistic regression is possible.

## 1. Enforcement hierarchy

Prefer the strongest enforcement layer that can express the invariant without duplicating semantics:

```text
1. type/module/crate visibility and dependency graph
2. typed constructors / closed enums / capability types
3. specialized source/dependency/effect checker owned by the concern
4. positive + negative fixture/test
5. exact-head CI / protected required context
6. PF-3 typed FitnessRuleRegistry composition
```

Do not replace a compile-time boundary with a grep-only rule when the dependency graph can make the forbidden edge impossible. Do not move domain policy into an architecture checker merely so that the checker can enforce it.

PF-3 later composes permanent enforcement metadata and anti-weakening semantics. F2 must establish the directly enforceable foundations now; it must not defer all real enforcement until PF-3.

## 2. Anti-centralization guardrails

### G1 — No God Crate / God Core

A foundation crate may own a narrow concern. It must not become the semantic owner for unrelated bounded contexts merely because multiple commands consume them.

Permanent budget:

```text
generic_cross_context_semantic_god_crate = 0
```

`opsctl-core`, if introduced, is operator-tool internal. It may host bounded pure modules, but must not become a universal application model, Product Runtime dependency or global architecture object graph.

### G2 — No semantic round-trip through generated projections

Allowed:

```text
natural owner -> generated projection
natural owner -> semantic consumer
```

Forbidden for internal current semantics:

```text
natural owner -> generated projection -> semantic consumer
```

Permanent budget:

```text
generated_projection_semantic_round_trip = 0
```

A generated projection may be externally persisted evidence/reporting output. A later consumer that needs the same current semantic fact must consume the natural owner or a bounded typed export, not re-import the projection as authority.

### G3 — Boundary-local DTOs only

External DTOs belong to one explicit integration/durable boundary. A DTO must not become a universal cross-context object graph.

Permanent budget:

```text
universal_cross_context_dto = 0
```

Prefer narrow DTOs such as `ReleaseSetVnDto`, `GovernanceObservationVnDto` or `HostedEvidenceEnvelopeVn` over generic `RepositoryStateDto`, `ArchitectureStateDto` or `EverythingDto` containers.

DTO-to-domain conversion occurs at the owning boundary. Core models remain representation-independent where that materially protects ownership and dependency direction.

### G4 — No generic authority/provider framework without real polymorphism

Do not introduce generic abstractions such as `AuthorityProvider`, `SemanticResolver`, `RepositoryAuthority`, plugin containers or DI registries merely to make the architecture look extensible.

Permanent budget:

```text
generic_authority_plugin_framework = 0
```

A trait/dynamic dispatch boundary is justified only by a real substitutable runtime/build-time boundary with at least two legitimate implementations or another concrete compile-time need. Known bounded owners should normally use direct typed composition.

### G5 — Compose proofs, not policies

Repository-wide gates may aggregate typed bounded results. They must not duplicate the semantic rule that produced those results.

```text
bounded owner -> typed result/proof
bounded owner -> typed result/proof
                    ↓
             proof aggregation
```

Forbidden:

```text
global gate -> reimplementation of release + D1 + runtime + lifecycle + evidence policy
```

Permanent budget:

```text
global_gate_duplicate_bounded_policy = 0
```

### G6 — Dependency direction is structural

Critical dependency rules must be enforced by Cargo/module/package boundaries wherever practical, supplemented by specialized dependency/source checks only where the build graph cannot express the rule.

At minimum:

```text
Product Runtime -> opsctl = 0
Product Runtime -> opsctl-core = 0
provider/framework/raw-effect dependency in provider-free pure core = 0
```

A comment or architecture diagram is not sufficient proof for these boundaries.

### G7 — Every durable machine artifact declares its role

Every new or materially changed durable machine artifact must be classified at review time as exactly one primary role:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

If its role is ambiguous, the change fails closed until ownership and consumers are resolved.

A generated file cannot become a semantic input merely because it is convenient to parse. A transitional semantic source requires an explicit retirement transaction.

### G8 — Cutover conservation and intentional non-conservation

For every retired predecessor, acceptance requires both:

```text
old_current_callers = 0
old_unique_current_invariants = 0
```

and evidence that all accepted current observable behavior owned by the transaction is preserved by the successor.

Deprecated, accidental or explicitly rejected legacy behavior is not copied merely to achieve byte-for-byte compatibility. Compatibility bridges require a proved current consumer or accepted external contract.

### G9 — Version axes are not collapsed

Do not collapse unrelated version meanings into one global version registry.

Keep distinct where semantically distinct:

```text
external schema/format version
semantic contract compatibility version
release-set/version identity
protocol version
```

Permanent budgets:

```text
breaking_external_contract_change_without_version_bump = 0
second_competing_version_registry_for_same_concern = 0
incidental_serialization_bytes_used_as_semantic_compatibility_authority = 0
```

Version transitions must be owned by the natural contract owner, not by a global architecture registry.

### G10 — Transaction scope cannot expand silently

A defect belongs in the current bounded transaction when at least one is true:

```text
caused_by_candidate
blocks_candidate_proof
violates_an_invariant_directly_touched_by_candidate
is_required_to_complete_the_owned_predecessor_cutover
```

An unrelated independent defect is recorded for the correct owning transaction instead of broadening the current PR into a repository-wide cleanup.

This rule does not permit ignoring a failing protected required context. It prevents unrelated redesign from being hidden inside a foundation cutover.

## 3. F1/F2 immediate enforcement matrix

F1/F2 must establish or preserve the following direct enforcement before merge. PF-3 may later register/combine these checks, but F1/F2 owns the actual foundation.

| Invariant | Primary enforcement form for F1/F2 | Required negative proof |
| --- | --- | --- |
| breaking Release Set contract requires new version | typed Release Set DTO/model/version transition + compatibility tests | old version with new breaking meaning is rejected |
| one Release Set version owner | release module/type ownership | second competing current writer/registry is unreachable/absent |
| `serde_json::Value` does not enter opsctl pure semantics | crate/module/API boundary + source/dependency test | attempted/golden forbidden leakage fixture fails |
| pure core has no filesystem/process/network/provider effects | dependency/module boundary + specialized source/dependency test | injected forbidden import/dependency fixture fails |
| Product Runtime cannot depend on opsctl/opsctl-core | Cargo dependency graph rule | forbidden dependency edge fixture/check fails |
| opsctl has no provider/network/process authority | dependency/effect/source rule | injected forbidden capability/dependency fails |
| opsctl cannot call Python for semantics | source/process-effect rule | Python child-process regression fixture fails |
| doctor remains read-only local composition | bounded doctor tests + effect/source rule | mutation/process/network/provider regression fails |
| generated projections are output-only | ownership/caller tests or specialized dependency check | projection-as-semantic-input regression fails |
| no global authority bag | typed API/source-shape negative check at relevant boundary | `GlobalAuthoritySet`/equivalent aggregation regression fails |
| no universal cross-context DTO/generic authority framework | architecture review + bounded source/dependency check where mechanically expressible | representative forbidden container/framework fixture fails where practical |
| canonical critical JSON rejects ambiguity | strict bounded decoder + vectors | duplicate member / oversized / invalid contract fails closed |
| canonical digest has explicit byte scope | typed digest API/tests + independent vectors | semantic-vs-exact byte scope mismatch is rejected/tested |

A rule is not considered permanently established merely because a prose document says it exists.

## 4. Pure-core shape rule

Pure does not mean global.

Forbidden target shape:

```text
EverythingAboutRepository
        ↓
evaluate_entire_repository(...)
        ↓
EverythingAboutArchitecture
```

Prefer bounded pure evaluators owned by their concern and typed composition of their outputs.

Permanent budget:

```text
generic_cross_bounded_semantic_aggregation_in_pure_core = 0
```

A pure evaluator must not gain broad unrelated inputs merely to make downstream composition convenient.

## 5. Artifact and API naming are not proof

Names such as `core`, `typed`, `authority`, `canonical`, `registry`, `projection` or `adapter` do not establish architecture correctness.

Acceptance is based on dependency direction, effect capability, owner/caller graph, contract/version ownership and negative proofs. A type called `Projection` that is consumed as authority is an authority regardless of its name.

## 6. Anti-weakening before PF-3

Until PF-3 provides the typed repository-wide fitness registry, F1/F2 and N1…N5 must protect their new permanent checks through the existing required CI/workflow structure and code ownership of the specialized checks.

A candidate must not silently:

- remove a REQUIRED negative fixture;
- downgrade a fail-closed invariant to warning-only;
- replace a structural boundary with an unchecked convention;
- add an allowlist/suppression that permits the forbidden architecture;
- move semantic truth into a generated projection or test registry;
- make the checker itself the new semantic owner.

Any intentional architecture-rule change requires an explicit normative contract update in the same bounded candidate, with replacement enforcement and negative proof.

## 7. Architecture-impact record for each bounded transaction

Before implementation changes materially alter architecture, record at least:

```text
bounded concern
natural semantic owner
current callers
allowed effects
forbidden effects
external/durable contracts + versions
artifact-role classification
predecessor/disposition
target dependency direction
positive proof
negative proof
```

This record may live in the PR body/transaction evidence; it must not become a permanent global authority catalog.

## 8. F1/F2 Definition of Done extension

In addition to the accepted F1/F2 DoD, the exact candidate head must prove:

```text
generic_cross_context_semantic_god_crate = 0
generated_projection_semantic_round_trip = 0
universal_cross_context_dto = 0
generic_authority_plugin_framework = 0
global_gate_duplicate_bounded_policy = 0
generic_cross_bounded_semantic_aggregation_in_pure_core = 0
second_competing_version_registry_for_same_concern = 0
incidental_serialization_bytes_used_as_semantic_compatibility_authority = 0
```

Where a budget is not fully machine-enforceable in F2 without creating a generic linter/framework, the candidate must use the strongest bounded structural/source test available and explicitly hand the remaining anti-weakening registration to PF-3. This exception cannot be used to defer directly enforceable boundaries.

## 9. Production invariant

This contract does not authorize production or provider mutation.

```text
source_present != production_enabled
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

F1/F2 remain architecture/contract foundations only. AR-12 remains blocked by the accepted execution sequence.
