# PF-3 — Architecture Fitness Baseline

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Issue:** #431  
**Accepted truthfulness correction:** #478  
**Production authorization:** NONE

PF-3 is an accepted **provisional fitness baseline**. It is not a new roadmap, universal linter, capability registry, final architecture-form freeze or production gate.

## 1. Accepted result

```text
PF-3 baseline #475             ACCEPTED
PF-3 truthfulness correction #478  ACCEPTED
architecture_form_frozen = false
architecture_complete = false
production_mutation = false
production authorization = NONE
```

The correction removed the decorative typed metadata registry whose `enforcement_owner` and `negative_proof` fields were free-text descriptions rather than executable linkage. PF-3 does **not** replace those strings with a generic rule engine.

## 2. Truthful enforcement model

The machine-enforced baseline is the set of actual specialized production checkers and executable negative proofs that current protected CI invokes.

```text
objective invariant
-> natural specialized production checker
-> executable negative fixture / self-test using the production checker path
-> real permanent CI caller
-> protected required context where that invariant is admission-critical
```

There is no current semantic fitness JSON and no global `FitnessRuleRegistry` source of truth.

A Markdown statement, free-text owner name, free-text test name or file path by itself is **not** enforcement. A negative proof is valid only when the tested violation deterministically fails the same semantic checker path used by production CI.

## 3. Objective machine-enforced families

Current specialized checks must preserve, where mechanically applicable:

- semantic-owner uniqueness and no duplicate mutable authority;
- bounded-context ownership and inward dependency direction;
- Pure Core / Effect Shell boundaries;
- `serde_json::Value` not crossing adapter -> pure core;
- Product Runtime -> `opsctl`/`opsctl-core` = 0;
- `opsctl` provider/network/process/credential authority = 0;
- `opsctl -> Python semantic child process` = 0;
- `opsctl doctor` remains local read-only diagnostic composition;
- global authority bag = 0;
- generated projection used as semantic source = 0;
- manual architecture-policy JSON authority = 0;
- breaking durable/external contract without version bump = 0;
- duplicate-key ambiguity in attestable JSON = 0;
- compatibility shim without a proved current consumer/durable obligation = 0;
- obsolete writer version emitted as current = 0;
- silent legacy -> current semantic coercion = 0;
- second production-enable authority = 0;
- stale DEAD predecessor reachability after accepted cutover = 0;
- unclassified Python product/provider effect = 0;
- Python duplicate product/release/lifecycle/evidence/fitness semantic authority = 0.

Examples of current natural enforcement surfaces include repository architecture boundary checks, `opsctl` read-only boundary checks, public contract compatibility tests, GitHub Actions governance checks, release-admission checks, historical executable retirement checks, workflow-secret authority checks, typed lifecycle/hosted-evidence tests, and focused repository-surface tests.

## 4. Anti-weakening discipline

Do not create a checker-for-checker or metadata registry merely to record that a checker exists.

A change that removes or weakens an admission-critical invariant must modify the natural enforcement owner and its executable negative proof deliberately in the same bounded transaction. The candidate must still pass the protected exact-head acceptance protocol.

Required outcome:

```text
objective rule violated -> production checker FAIL
negative proof removed/bypassed -> owning CI check FAIL or review blocks
competing semantic implementation introduced -> owning authority check FAIL
required CI path weakened -> governance check FAIL
```

The repository history, accepted PRs and protected CI provide provenance. They are not runtime registries.

## 5. What PF-3 must not become

Forbidden as normal PF/FC/AR work:

```text
generic architecture executor
fitness DSL
plugin/DI/service-container framework
global metadata registry
repository-wide JSON inventory
second lifecycle/evidence/fitness engine
new projection layer without durable consumer
wrapper check whose purpose is checking another checker
new generic architecture layer introduced because a rehearsal is inconvenient
```

Subjective architecture judgement remains review/governance work. Automated checks own objective properties they can deterministically prove.

## 6. `opsctl doctor`

`opsctl doctor` remains local, read-only structural diagnostics. Negative proof rejects any restoration of:

- Python/Node/Git/GitHub/provider/runtime subprocess/API execution;
- network/provider/secret/runtime access;
- mutation;
- global authority catalog;
- duplicated lifecycle/release/evidence/domain policy;
- generated inventory as semantic input;
- retired AR-qualified sentinels as permanent dependencies.

## 7. Provisional baseline and final freeze

PF-3 prevents silent weakening but is not the final architecture-form freeze. FC-6 through AR-15 may make only the smallest architecture correction required by a named failed product/rehearsal scenario, preserving one natural owner and updating the owning executable proof.

Final architecture-form freeze follows accepted AR-15 when the real Windows delivery/updater/recovery path has proved PAS-4, PAS-5 and PAS-6. AR-16 audits that result; AR-17 qualifies/authorizes it.

```text
PF-3 accepted provisional baseline
-> FC-6 / FC-7 functional closure + staging proof
-> AR-12 fresh-environment rehearsal
-> AR-13 rotation rehearsal
-> AR-14 remote-recovery rehearsal
-> AR-15 Windows delivery/updater/LKG proof + final architecture-form freeze
-> AR-16 audit only
-> AR-17 qualification/authorization only
```

Neither PF-3 nor AR-15 alone sets `architecture_complete=true` or authorizes production.

## 8. Stage-specific DoD

```text
decorative fitness registry = 0
free-text pseudo-link treated as enforcement = 0
objective invariants enforced by natural production checker paths
negative proofs executable against those paths
required CI callers remain real and protected where admission-critical
second architecture source of truth = 0
generic linter/plugin/DI framework introduced by PF-3 = 0
generated fitness JSON used as semantic input = 0
new checker-for-checker = 0
production behavior unchanged
architecture_form_frozen = false
architecture_complete = false
production_mutation = false
```

Shared exact-head CI/review/guarded-merge rules are owned by `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` and current protected governance.

Canonical references: `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`, `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`, `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`, `docs/OPSCTL_DOCTOR_CONTRACT.md`, `docs/PYTHON_USAGE_BOUNDARY.md`, #431, #475, #478.
