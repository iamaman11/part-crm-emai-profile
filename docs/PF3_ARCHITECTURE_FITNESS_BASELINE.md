# PF-3 — Architecture Fitness Baseline

**Document status:** SUBORDINATE_PREREQUISITE_SPEC  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Issue:** #431  
**Production authorization:** NONE

PF-3 makes already-selected architecture guarantees durable and machine-enforced as a **provisional fitness baseline**. It is not a new roadmap, capability registry, final architecture-form freeze or production gate.

## 1. Entry and result

PF-3 starts only from accepted PF-2 protected `main`.

PF-3 acceptance means:

```text
prospective architecture form = designed
critical architecture constraints = machine-enforced
architecture_form_frozen = false
architecture_complete = false
production authorization = NONE
```

After PF-3, ordinary FC/AR work implements and rehearses bounded functionality inside the established architecture. A failed product acceptance scenario may justify only the smallest bounded correction; it must name the scenario, preserve one natural owner and update anti-weakening proof in the same transaction. Final architecture-form freeze is evaluated after accepted AR-15 has proved the real Windows delivery/updater/recovery scenarios.

## 2. Small typed fitness owner

A manually maintained semantic `architecture/architecture-fitness-policy.json` is forbidden.

The semantic owner is typed Rust (`FitnessRuleRegistry` or equivalent), but it should remain a **small enforcement index**, not become a universal linter framework.

Minimum rule metadata:

```text
RuleId
requiredness
scope/applicability
primary enforcement owner
owned negative fixture
```

Any JSON/report view is GENERATED_PROJECTION/evidence only.

## 3. Reuse specialized enforcement

A REQUIRED rule may point to an existing specialized validator/checker as its primary enforcement owner. Do not duplicate working logic in the registry merely to centralize it.

PF-3 adds only missing bounded enforcement and proves:

```text
REQUIRED rule without active enforcement owner -> FAIL
missing/unreachable enforcement -> FAIL
owned negative fixture removed -> FAIL
REQUIRED downgraded/removed silently -> FAIL
objective rule violated -> FAIL
competing semantic implementation where one owner is required -> FAIL
```

The registry is an index over enforcement that already has a natural owner. It must not wrap every validator in a new validator, mirror every rule into JSON, or build projections whose only consumer is PF-3 itself.

## 4. Objective machine-enforced rule families

At minimum cover:

- semantic owner uniqueness / no duplicate mutable authority;
- bounded-context ownership and inward dependency direction where mechanically observable;
- Pure Core / Effect Shell boundaries;
- explicit filesystem/process/network/provider/DB/deployment effect capabilities;
- `serde_json::Value` not crossing adapter -> pure core;
- Product Runtime -> `opsctl`/`opsctl-core` = 0;
- `opsctl` provider/network/process authority = 0;
- `opsctl -> Python semantic child process` = 0;
- `opsctl doctor` remains local read-only diagnostic composition;
- global authority bag = 0;
- generated projection used as semantic source = 0;
- manual semantic architecture-policy JSON = 0;
- manual AR-qualified application ownership registry as semantic input = 0;
- unversioned required external/durable contract = 0;
- breaking durable contract without version bump = 0;
- compatibility shim without proved consumer/durable obligation = 0;
- historical contract marked current without current consumer = 0;
- obsolete writer version emitted as current = 0;
- silent legacy -> current semantic coercion = 0;
- second production-enable authority = 0;
- stale DEAD predecessor reachability after accepted cutover = 0;
- unclassified Python production/provider effect = 0.

## 5. What PF-3 must NOT automate badly

Do not build a pseudo-intelligent generic checker to decide subjective questions such as:

```text
"is this arbitrary diff a new generic architecture framework?"
"is this abstraction architecturally justified?"
```

Those judgements remain explicit review/governance questions.

Later materially architecture-changing candidates provide a short Architecture Impact declaration covering, as applicable:

```text
bounded contexts
authorities/contracts
effect classes
execution surfaces
Release/Capability Profiles
schema/migration impact
legacy predecessor disposition
applicable RuleIds/budgets
whether a new generic architecture mechanism is introduced
```

`none` is valid when justified by the diff. Protected review plus PF-3 anti-weakening handles the semantic judgement. Objective uncertainty in automated enforcement fails closed.

## 6. `opsctl doctor`

Permanent negative proof rejects any `doctor` behavior that:

- executes Python/Node/Git/GitHub/provider/runtime subprocess/API;
- performs network/provider/secret/runtime access;
- performs mutation;
- reintroduces a global authority catalog;
- duplicates lifecycle/release/evidence/domain policy;
- treats generated `architecture/inventory.json` as semantic authority;
- restores retired AR-6/operator/Python-inventory root sentinels as permanent dependencies.

## 7. Provisional baseline and final freeze

After PF-3 acceptance, normal roadmap work may not introduce:

```text
new generic architecture layer/framework
new global authority/registry
new competing lifecycle/evidence/fitness engine
new speculative compatibility framework without proved consumer
new generic service locator/plugin container/god-policy layer
FC/AR/PC phase used as a redesign bucket
```

This restriction is a design/enforcement guardrail, not final freeze and not `architecture_complete=true`. AR-17 still owns Production Core qualification/authorization.

PF-3 itself is provisional because FC-6 and AR-12…AR-15 still exercise staging, fresh bootstrap, rotation, remote recovery and Windows delivery. A concrete rehearsal failure may produce a bounded architecture correction, but cannot become an open redesign program.

Final architecture-form freeze occurs after AR-15 acceptance when:

```text
PF-3 required rules remain enforced
PAS-4 Windows real-runtime delivery = accepted
PAS-5 failure/retry/recovery = accepted for the Windows path
PAS-6 same-bits update/LKG rollback = accepted for the Windows path
unresolved scenario-driven architecture correction = 0
architecture authority/surface budgets = satisfied
architecture_form_frozen = true
```

AR-16 audits this frozen result. AR-17 consumes it for qualification; neither phase designs another architecture.

## 8. Post-PF-3 execution semantics

```text
FC-6 preflight = fresh #399/#421 live re-baseline
FC-6 / FC-7 = staging proof + functional closeout; smallest scenario-driven correction only
AR-12 = fresh-environment rehearsal
AR-13 = rotation rehearsal
AR-14 = remote-recovery rehearsal
AR-15 = Windows updater/delivery implementation + proof + final architecture-form freeze
AR-16 = final whole-project audit only
AR-17 = Production Core qualification/authorization decision only
PC-1  = first Production Core release
```

AR-12/13/14 should change source only when rehearsal reveals a concrete defect. AR-16 is not a refactor bucket. AR-17 should consume accepted evidence/state rather than create another closeout engine.

## 9. Stage-specific DoD

```text
typed fitness registry/index = single semantic owner
every REQUIRED rule -> exactly one primary enforcement owner
every REQUIRED rule -> owned negative proof
silent required-rule weakening = mechanically rejected
objective architecture regressions = mechanically rejected
generic linter/plugin/DI framework introduced by PF-3 = 0
second roadmap/domain/capability/production-enable authority = 0
generated JSON/report used as semantic input = 0
new validator whose primary purpose is checking another validator = 0
new tracked projection without durable exact-byte consumer = 0
PF-3 plan + validator + projection surface larger than deleted predecessor surface = 0
application behavior unchanged
architecture_form_frozen = false
architecture_complete = false
production_mutation = false
```

Linux proof is mandatory. Windows is mandatory where the checker/operator path is intended cross-platform.

Shared exact-head CI/review/guarded-merge rules are owned by `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` and current protected governance.

Canonical references: `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`, `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`, `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`, `docs/OPSCTL_DOCTOR_CONTRACT.md`, `docs/PYTHON_USAGE_BOUNDARY.md`, #431.
