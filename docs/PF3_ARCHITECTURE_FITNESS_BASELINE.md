# PF-3 — Architecture Fitness Baseline

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Mandatory requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**PF-1:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**Tracker:** #431  
**Production authorization:** NONE  
**AR-12 implementation:** NOT AUTHORIZED

PF-3 makes the agreed architecture discipline permanently machine-enforced before Functional Closure resumes. It is not a new AR slice, generic linter/plugin framework, capability registry, second lifecycle authority or second architecture inventory.

## 1. Prerequisite order

```text
F1/F2 -> N1 -> bounded pre-N2 F1 compatibility cleanup -> N2..N5 -> PF-1 -> PF-2 -> PF-3
      -> fresh #399/#421 re-baseline -> FC-6 -> FC-7 -> AR-12
```

PF-3 starts only from accepted PF-2 `main`.

### 1.1 PF-3 is the architecture-forming freeze point

PF-3 is the final planned stage of Architecture Re-baseline v3 allowed to introduce or replace a **generic architecture mechanism**. Its acceptance means the prospective architecture shape is established and machine-enforced.

After PF-3 acceptance, normal roadmap execution may not introduce:

```text
new generic architecture layer/framework
new global authority/registry
new lifecycle/evidence/fitness engine competing with accepted owners
new compatibility framework without a proved consumer/durable obligation
new cross-cutting service locator/plugin container/god-policy layer
FC/AR/PC phase used as an architecture-redesign bucket
```

Later stages may still implement or evolve bounded product functionality, use cases, adapters, provider integrations, migrations, Windows delivery, recovery, performance and explicit versioned contracts **inside the established architecture**.

A later discovery that genuinely requires a material architecture change does not silently reopen PF-3 or turn AR-16/AR-17 into redesign phases. The change must use the explicit governed architecture-change/anti-weakening path, identify affected RuleIds and authorities, and remain blocked until the new architecture evidence is accepted.

This freeze is distinct from lifecycle closeout:

```text
PF-3 accepted
    = architecture form designed + machine-enforced

architecture_complete=true
    = NOT YET; only AR-17 qualification may set/authorize this lifecycle outcome
```

Post-PF-3 planned stage semantics are fixed:

```text
fresh #399/#421 re-baseline  = re-evaluate functional closure against accepted architecture
FC-6 / FC-7                  = staging/functional proof, no architecture redesign
AR-12                        = fresh rehearsal environment implementation/proof
AR-13                        = rotation rehearsal
AR-14                        = remote recovery rehearsal
AR-15                        = Windows delivery/updater implementation + proof within frozen architecture
AR-16                        = final whole-project audit only
AR-17                        = qualification / Production Core gate decision only
PC-1                         = first Production Core release
```

## 2. Semantic owner correction

PF-3 must **not** create a manually maintained semantic:

```text
architecture/architecture-fitness-policy.json
```

The permanent fitness semantic owner is typed Rust:

```text
FitnessRuleRegistry
        ↓
FitnessEvaluator / enforcement mapping
        ↓
positive + negative fixtures
        ↓
Architecture Fitness Gate
        ↓
optional generated JSON/index/report
```

If a JSON fitness view exists, it is generated projection/index only. It does not independently define rule identity, applicability, severity, ownership or anti-weakening semantics.

## 3. Rule model

Every fitness rule has a stable typed identity and at least the semantic equivalents of:

```text
RuleId
name/category
scope/applicability
severity/status
source/natural authority
primary enforcement owner
negative fixture requirement
supersession/retirement semantics
```

Required status vocabulary is closed and fail-closed, conceptually:

```text
REQUIRED
DEFERRED_WITH_OWNER
RETIRED
```

A REQUIRED rule without reachable primary enforcement is a gate failure.

## 4. One rule -> one primary enforcement owner

Do not duplicate the same semantic check across unrelated scripts merely for coverage.

```text
Rule semantics
    = typed Rust registry

Primary enforcement
    = one bounded checker/evaluator owner

Additional tests
    = exercise the rule, not competing authorities
```

Existing specialized validators may remain primary implementation adapters where they naturally observe a structural fact. They do not become independent rule registries.

## 5. Initial rule families

### AF-AUTH — authority uniqueness

Enforce at least:

```text
semantic_owner_count_per_fact = 1
current_mutable_authority_count_per_concern = 1
generated_projection_used_as_semantic_input = 0
global_authority_bag = 0
manual_architecture_semantic_json_authority_without_exception = 0
manual_AR_qualified_application_ownership_registry_current_authority = 0
second lifecycle derivation = 0
second production-enable registry = 0
```

### AF-DEP — bounded context / dependency direction

```text
provider SDK in provider-free domain/application scope = 0
runtime Product -> opsctl dependency = 0
runtime Product -> opsctl-core dependency = 0
forbidden cross-context concrete persistence dependency = 0
generic god service/common semantic owner = 0
```

### AF-EFFECT — effects

Protected pure scopes must not acquire unowned:

```text
Filesystem
ProcessExecution
NetworkAccess
ProviderRead/Write
SecretResolve
DatabaseWrite
DeploymentMutation
RuntimeExecution
```

Pure `opsctl` policy receives explicit typed inputs; it does not collect observations itself.

### AF-OPS — `opsctl` internal boundary

At minimum:

```text
serde_json_value_crossing_adapter_to_pure_core = 0
serde_json_map_crossing_adapter_to_pure_core = 0
filesystem_import_in_opsctl_pure_core = 0
process_execution_in_opsctl = 0
network_access_in_opsctl = 0
provider_sdk_dependency_in_opsctl = 0
opsctl_python_child_process = 0
opsctl_runtime_service_endpoint = 0
hidden_opsctl_persistent_state = 0
```

Equivalent compile-time module/crate enforcement is preferred over brittle string scanning where practical.

### AF-DOCTOR — `opsctl doctor`

```text
opsctl_doctor_process_execution = 0
opsctl_doctor_network_access = 0
opsctl_doctor_provider_access = 0
opsctl_doctor_python_or_node_child_process = 0
opsctl_doctor_legacy_authority_sentinel = 0
doctor_generic_json_authority_bag = 0
doctor_duplicate_semantic_policy = 0
repository_root_depends_on_retired_sentinel = 0
```

`doctor` is diagnostic composition, not a semantic registry.

### AF-PYTHON — Python role/effect policy

```text
unclassified_python_production_entrypoint = 0
unclassified_python_network_or_provider_effect = 0
python_duplicate_semantic_authority = 0
python_runtime_bypass_of_profile_bridge = 0
python_provider_mutation_without_explicit_exception = 0
python_secret_readback_surface = 0
legacy_python_estate_registry_current_authority = 0
```

Legitimate Camouhost runtime adapter/tests/generators remain allowed under `docs/PYTHON_USAGE_BOUNDARY.md`.

### AF-TYPE — typed identities/contracts

```text
critical semantic IDs interchangeably raw where owned type required = 0
unversioned durable external contract = 0
breaking durable contract change without version bump = 0
pre-interpreted raw observation smuggling policy decision = 0
duplicate JSON member accepted in attestable/security contract = 0
```

### AF-COMPAT — compatibility justification

Compatibility code/data that affects current execution must have a proved owner and consumer:

```text
compatibility_shim_without_proved_consumer_or_durable_obligation = 0
historical_contract_marked_current_without_current_consumer = 0
current_writer_emits_legacy_breaking_version = 0
silent_legacy_to_current_semantic_coercion = 0
```

Historical evidence by itself never satisfies a current compatibility-consumer requirement.

### AF-CAP — capability/production exposure

```text
hidden production-enable authority outside Release Profile = 0
production-disabled execution surface allowed past backend admission = 0
unknown activation unit/execution-surface mapping = 0
enabled profile with incomplete dependency closure = 0
```

`source_present != production_enabled` remains mandatory.

### AF-PERSIST — persistence/migration

```text
forbidden cross-context direct persistence mutation = 0
multiple current migration executors for same operation = 0
migration/release compatibility unlink = 0
fresh bootstrap treated as upgrade provenance = 0
```

### AF-CONFIG — configuration/secrets

```text
raw environment/provider binding reads in protected inner scope = 0
secret material promoted to general config/readback = 0
```

### AF-EVENT — events/queues

Enforce versioned/explicit cross-context integration-event boundaries and prevent queue transport from becoming a parallel business state machine.

### AF-LEGACY — cutover discipline

For every declared cutover:

```text
old_current_callers = 0
old_unique_current_invariants = 0
DEAD predecessor reachable = 0
compatibility shim without proved consumer/contract = 0
```

### AF-FREEZE — post-PF-3 architecture-forming boundary

After PF-3 acceptance:

```text
unapproved new generic architecture mechanism = 0
new global semantic registry = 0
new cross-cutting compatibility framework without proved obligation = 0
AR-16/AR-17 redesign work = 0
material architecture change without governed supersession/RuleId impact = 0
```

A bounded new product capability is not itself an architecture violation. The rule prevents cross-cutting architecture reinvention, not normal product evolution.

### AF-READ — developer readability

For each Production-Core-relevant bounded context, developers must be able to locate:

```text
natural owner
canonical contract/authority
entry/composition root
domain/application boundary
ports/adapters
effect boundary
persistence owner
execution surfaces
activation unit/release profile
```

For operational flows, observation producer, DTO, pure policy evaluator and mutation executor must be distinguishable.

## 6. Fitness anti-weakening

The fitness system must detect attempts to weaken itself.

Governed changes include:

```text
remove REQUIRED rule
REQUIRED -> weaker state
narrow applicability
replace primary enforcement owner
remove required negative fixture
weaken expected fail-closed result
retire rule while invariant still applies
```

Such a change requires typed/versioned supersession metadata/evidence containing semantic equivalent of:

```text
old_rule
successor_rule OR explicit retirement reason
reason
compatibility/security impact
owning authority/program work
accepted source
```

Silent weakening fails the gate.

## 7. Architecture/tooling budgets

Exact-zero/one safety budgets are fixed. Non-safety performance thresholds are measured from accepted baseline, not invented.

Minimum:

```text
semantic owner count per fact = 1
mutation executor count per owned mutation = 1
REQUIRED rule without active enforcement = 0
required negative fixture missing = 0
unclassified external effect in protected scope = 0
hidden production-enable authority = 0
cross-platform deterministic policy divergence = 0
```

Measured budgets may additionally cover local `opsctl` latency, required PR CI duration and PF-2 evidence size/freshness. Threshold changes are governed and cannot weaken correctness.

## 8. Python observers in PF-3

PF-3 does not require a Python-to-Rust rewrite of every source checker.

A bounded Python AST/text/filesystem observer may remain where practical, provided:

```text
Python observer -> structural fact
Typed Rust FitnessRuleRegistry -> rule semantics/applicability
Architecture Fitness Gate -> enforcement outcome
```

Forbidden:

```text
Python file -> second mutable rule catalog
Python per-file estate database -> current semantic authority
Python source observer -> hand-maintained application ownership database
```

## 9. `opsctl-core` enforcement

If F2 introduces internal `opsctl-core`, PF-3 verifies its dependency boundary mechanically.

If F2 uses module-only separation instead, PF-3 must prove equivalent strength; convention-only separation is insufficient for critical rules.

Product Runtime dependency on either `opsctl` or `opsctl-core` is always forbidden.

## 10. External desired-state exception

Do not over-apply “semantics must be Rust”. Genuine desired-state configuration for external systems may remain versioned declarative data, e.g. desired GitHub branch/environment/check configuration.

Correct model:

```text
desired external configuration DTO/data
+
live external observation
        ↓
typed Rust policy comparison
```

Historical AR overlay chains are not current desired-state architecture.

## 11. Architecture Impact requirement after PF-3

Every later materially architecture-changing PF/FC/AR/PC candidate declares:

```text
bounded contexts touched
natural authorities touched
public/persisted/integration contracts touched
effects added/changed
observations/decisions changed
execution surfaces / activation units / release profiles changed
schema/migration impact
legacy predecessor disposition
fitness RuleIds affected
budget impact
whether a new generic architecture mechanism is introduced
```

`none` is valid only when justified by the diff.

A post-PF-3 candidate that introduces a new generic architecture mechanism must be treated as an explicit governed architecture-change request, not normal feature implementation. It fails the existing gate until supersession/anti-weakening and affected authority ownership are reviewed and accepted.

## 12. Positive proofs

At minimum:

- typed registry builds deterministically;
- all REQUIRED rules resolve to one reachable primary enforcement owner;
- accepted repository passes or each pre-existing exception is explicitly bounded/owned;
- valid governed supersession is accepted;
- measured budgets load/check deterministically;
- Rust/Python observer split does not duplicate rule semantics;
- `opsctl` pure-core, doctor and Product Runtime boundaries pass;
- Python role/effect boundary passes;
- Release Set compatibility paths require a named current consumer/durable obligation or are absent;
- release profile single-enablement passes;
- post-PF-3 bounded feature implementation that stays inside established architecture is accepted;
- Linux and applicable Windows checks pass.

## 13. Negative proofs

At minimum reject:

```text
duplicate semantic owner
provider SDK in protected pure scope
Product Runtime dependency on opsctl/opsctl-core
serde_json::Value crossing into pure policy
opsctl process/network/provider/Python child process
opsctl doctor legacy sentinel/generic authority bag
Python runtime bypass/provider mutation/secret readback without approved role
second production-enable registry
unknown execution surface/activation unit
incomplete enabled release profile closure
unversioned/breaking durable contract without bump
historical compatibility retained without proved current consumer/durable obligation
silent legacy-to-current semantic coercion
duplicate JSON member accepted in attestable contract
forbidden cross-context persistence mutation
manual AR-qualified application ownership registry used as current semantic authority
REQUIRED rule without enforcement
silent rule downgrade/removal/applicability narrowing
negative fixture removal
checker replacement without governed supersession
reachable DEAD predecessor after cutover
post-PF-3 unapproved new generic architecture mechanism
AR-16/AR-17 redesign bucket
```

## 14. Touch-to-converge

```text
GOOD             -> preserve
TOUCHED          -> applicable rules required now
LEGACY_UNTOUCHED -> bounded/classified until owning work touches it
```

No whole-repository rewrite is required solely to satisfy aesthetic symmetry.

## 15. Definition of Done

PF-3 closes only when:

1. typed Rust `FitnessRuleRegistry` or equivalent is the single semantic fitness rule owner;
2. any JSON fitness document is generated/projection only;
3. every initial REQUIRED rule has one reachable primary enforcement owner;
4. positive + negative fixtures prove fail-closed behavior;
5. anti-weakening/supersession is machine enforced;
6. required exact zero/one budgets are machine checked;
7. Architecture Fitness Gate is permanent PR CI/governance surface;
8. compatibility-without-consumer and historical-current-authority regressions are machine rejected;
9. manual AR-qualified application ownership registries cannot become current semantic inputs;
10. post-PF-3 architecture-forming freeze is documented and machine-enforced sufficiently to reject unapproved new generic architecture mechanisms;
11. Architecture Impact discipline is documented/enforced for later work;
12. no parallel roadmap/capability/lifecycle/domain authority is introduced;
13. application behavior and production fail-closed state remain unchanged;
14. exact-head CI/protected contexts are green, `behind_by=0`, reviews/threads unblocked;
15. accepted-main reread succeeds;
16. #399/#421 are re-baselined only after PF-3 acceptance;
17. FC-6 resumes only from that accepted baseline;
18. PF-3 acceptance does not set `architecture_complete=true`, authorize production or start AR-12.
