# Architecture Evolution Quality Contract

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT
**Unique responsibility:** safe architecture change and simplification without weakening guarantees
**Program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)
**Mandatory architecture:** [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md)
**Production authorization:** CAP-08 exact-candidate R3 only; NOT GRANTED by this document

This contract is not a roadmap, lifecycle/status registry, capability catalog, architecture inventory or
generic framework specification. Historical AR/PF/PAS documents preserve provenance; they do not add
current rules unless a current natural owner explicitly adopts the invariant.

## 1. Precedence and natural ownership

```text
current product/security/legal/durable obligations
-> current prospective architecture
-> one natural semantic owner
-> proved current consumers and external observations
-> historical outcomes/evidence
-> historical implementation shape
```

Every current semantic fact has exactly one natural owner. DTOs, rows, manifests, generated JSON,
evidence envelopes, CLI output and frontend projections may carry a fact but do not become competing
owners.

Historical implementation is not a compatibility obligation by itself:

```text
named current/external consumer = 0
AND persisted/durable/legal obligation = 0
-> compatibility bridge default = NO
```

A retained bridge requires a named consumer/obligation, exact supported shape/version, isolated adapter
and explicit retirement condition. A real obligation is versioned, migrated or explicitly retired by
its owner; it is never silently discarded for aesthetic simplification.

## 2. Change shape

Preserve the inward direction:

```text
domain
  <- application/use cases + ports
  <- adapters
  <- composition roots
  <- Release / Capability admission
```

Provider SDKs, HTTP frameworks, concrete persistence, filesystem/process/network and runtime choices do
not enter provider-free domain/application semantics. Product Runtime never depends on `opsctl` or
`opsctl-core`.

A shared crate/service/policy layer is an exception. It is justified only when at least two real
independent consumers require the same pure invariant and one owner materially prevents duplicate
authority. Generic `common`, global service layers, plugin frameworks and god-policy registries are
forbidden.

Policy, compatibility, planning and state transitions are deterministic over typed explicit inputs
where practical. Clock, randomness, environment, filesystem, process, network, database and provider
effects stay in explicit outer adapters/composition.

## 3. Durable artifact roles

Every durable machine artifact has one declared role:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

Generated projections and observations are not semantic inputs unless a bounded contract explicitly
defines that external input role. A tracked file, validator or CI caller is not proof of durable value.

A transitional source is retired atomically:

```text
natural owner proved
-> current/durable consumers enumerated
-> unique current invariants moved to their owners
-> callers cut over
-> old_current_callers = 0
-> old_unique_current_invariants = 0
-> predecessor + predecessor-only generators/checkers/docs deleted or demoted to passive provenance
```

Do not reproduce a retired JSON/YAML/TOML/Node/Python registry in another language or add a checker whose
purpose is to check the replacement checker.

## 4. External contracts, JSON and identity

External/persisted/integration/observation contracts are closed and versioned. Critical JSON admission
uses bounded bytes/depth/complexity, strict UTF-8, duplicate-member rejection before canonicalization,
closed fields unless a deliberate extension point exists, typed DTO conversion and a breaking-version
bump when meaning changes.

```text
external bytes
-> strict adapter/DTO
-> typed semantic input
-> pure decision
-> typed result
-> output adapter
```

`serde_json::Value`, provider response types and filesystem paths do not cross into pure semantic APIs.
Semantic identity hashes canonical semantic bytes; artifact identity hashes exact artifact bytes. The
scope is explicit and the two are not interchangeable.

## 5. Context, data, security and effects

- persistence remains context-owned; no universal business repository or cross-context mutation owner;
- queries do not acquire incidental mutation;
- commands declare effects and fail closed on identity, authorization, capability and preconditions
  before the first side effect;
- raw bindings/config are validated at composition edges; secret material stays behind its owned
  boundary and never becomes general config/readback/evidence;
- retry/idempotency/concurrency guarantees belong to the operation/context that owns the effect, not a
  universal engine;
- Release / Capability Policy is the only production-enable authority; UI/env/helper presence never
  enables a capability;
- security authority stays near each effect while composing the accepted identity, membership, device,
  runtime and release trust boundaries.

Detailed permanent role/effect rules remain in the bounded owners linked by `INDEX.md`, including
`OPSCTL_ARCHITECTURE_BOUNDARY.md`, `OPSCTL_DOCTOR_CONTRACT.md` and `PYTHON_USAGE_BOUNDARY.md`.

## 6. Future-check creation standard

A new permanent check/workflow is accepted only when one proposal proves all of:

```text
named current consumer
+ concrete failure/security/data/release risk
+ one objective invariant
+ one semantic owner
+ cheapest sufficient proof tier
+ positive and negative/fail-closed proof in the same checker family
+ minimum required execution level
+ existing stable required context by default
+ lifecycle and retirement condition
+ no checker-for-checker
+ no generic semantic registry
```

Primary rule:

```text
one objective invariant
-> one primary proof
-> cheapest sufficient lifecycle tier
```

Proof tiers are selected by risk, not ceremony:

1. pure unit/type/compile-time proof;
2. owner-local contract/integration proof;
3. repository/source-boundary proof;
4. exact artifact/runtime/environment proof;
5. hosted/external scenario proof only when lower tiers cannot prove the invariant.

A higher-tier check may compose lower-tier results when the higher-level invariant genuinely requires
composition. It must not duplicate their semantic logic. Prefer adding a job to an existing stable
required workflow/context; creating or adding a required context needs independent evidence that a new
lifecycle/permission/scheduling boundary is necessary.

Every check states its owner, inputs, output, positive fixture, negative fixture, caller, required/optional
status, cost tier and retirement trigger. Missing consumer/risk/retirement evidence rejects the new
check. An old check is retired only after its objective invariant has a surviving primary proof.

### 6.1 Control ownership and enforcement

This is a responsibility map, not a per-file/check registry and not a copy of mutable required-context
state. Exact files and workflow composition may change; the natural owner and protected invariant may
not disappear during that change.

| Invariant class | Natural semantic owner | Primary control | Acceptance rule |
|---|---|---|---|
| Rust dependency direction and bounded module ownership | owning domain/application contract plus Cargo/module boundary | compiler/type system, governed workspace-crate classification, architecture/module-layout checks and negative forbidden-dependency/import fixtures | an unclassified crate, provider dependency in a pure layer or duplicate application owner fails the existing architecture proof |
| Capability add/enable/disable/remove and runtime admission | `crates/capability-policy` plus the owning runtime ingress/composition adapter | typed profile/surface catalog, dependency/digest/admission tests, ingress mapping tests and exact release/effective-set evidence | source alone never enables effects; unknown surface/profile/environment/authorization fails closed |
| `opsctl` shell/core/effect boundary | `OPSCTL_ARCHITECTURE_BOUNDARY.md`, `opsctl-core` pure policy and `opsctl` adapters | `check-opsctl-readonly.py` positive/self-test family plus Rust dependency and behavior tests | provider/network/process/secret/mutation capability, Product Runtime dependency or representation/effect leakage into pure core fails |
| Python and Node/MJS repository/runtime entrypoints | the bounded product, release, migration, governance or runtime concern they serve; language is not an owner | mandatory change envelope, derived current caller graph, explicit effect contract and owner-specific positive/negative proof | reachable effect with no owner/consumer/authority, duplicate semantics or bypass path fails; no hand-maintained language estate registry is created |
| Worker/application/provider and D1/R2 boundaries | owning use case/port and concrete outer adapter | owner-local Worker/application, persistence, generation and failure fixtures, including direct-provider/direct-D1 negative cases | composition may select adapters; it may not absorb business policy or bypass an application owner |
| Frontend feature and transport boundaries | capability contract/application owner plus generated transport projection | generated contract comparison, runtime response validation, feature-import/root-route negative fixtures | UI never becomes authorization or schema owner; sibling internals and handwritten duplicate transport fail |
| Documentation/current-state authority | `INDEX.md` knowledge map plus each listed natural document/live owner | documentation authority checker and deliberately stale/missing/duplicate negative fixtures | a projection/history document cannot become current execution, readiness or semantic authority |
| Verification lifecycle and complexity | this quality contract plus the invariant's natural owner | CAP-05 disposition, future-check standard, mandatory change envelope, complete diff and simplification ledger | checker-for-checker, ownerless required proof, predecessor-only caller or indefinite cleanup fails acceptance |

Objective boundaries use executable positive and negative proof at the lowest sufficient tier. The
implementation author records the change envelope and simplification ledger; the natural owner defines
the invariant; protected CI repeats objective proof; the repository maintainer accepts only the exact
proved candidate. Qualitative simplicity is not replaced by a gameable LOC/check-count score: it is
decided from ownership, dependency direction, change radius and predecessor deletion, while every
objective claim remains executable.

## 7. Bounded change protocol

Every architecture-affecting transaction follows:

```text
fresh protected-main + GitHub/live-owner observation
-> one authorized bounded Issue
-> owners/contracts/effects/consumers/predecessor enumerated
-> smallest coherent implementation
-> caller cutover and predecessor retirement in the same transaction
-> complete diff + simplification ledger
-> targeted positive and negative proof
-> one unchanged exact PR head
-> applicable permanent CI + protected contexts green
-> behind_by = 0; blocking reviews = 0; unresolved threads = 0
-> guarded merge bound to exact head
-> candidate tree == accepted merge tree
-> accepted-main reread and live tracker update
```

Green CI cannot authorize an architecture weakening or retain an ownerless predecessor. Conversely,
CI is not changed merely to make an intended code change pass; any gate correction proves the gate's
objective invariant separately and preserves fail-closed behavior.

## 8. Touch-to-converge and stop rules

```text
GOOD             -> preserve
TOUCHED          -> converge within the bounded owner transaction
LEGACY_UNTOUCHED -> classify with owner/consumer/retirement condition
```

No repository-wide rewrite is authorized for aesthetic consistency. Do not create a new global audit,
standing architecture phase, plugin/DI framework, global registry, service layer or compatibility path
to route around a concrete bounded defect.

`source_present != production_enabled` and
`CODE_COMPLETE != SCENARIO_COMPLETE != PRODUCTION_AUTHORIZED` remain binding. Production permission
comes only from the exact Release Candidate + target-specific Deployment Authorization Envelope and
named R3 decision in the current program.
