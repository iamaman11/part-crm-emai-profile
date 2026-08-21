# PF-3 — Architecture Fitness Baseline

**Document status:** SUBORDINATE_PREREQUISITE_SPEC
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`
**Cross-cutting quality contract:** `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`
**Functional Closure tracker:** issue #399
**Sequence:** PF-1 -> PF-2 -> PF-3 -> re-baseline #399/#421 -> FC-6
**Production authorization:** NONE
**AR-12 implementation:** NOT AUTHORIZED by this document

PF-3 exists to make the architecture-evolution rules mechanically persistent before Functional Closure resumes. It is not a new AR slice, roadmap, lifecycle authority, feature-flag system, capability registry or generic architecture framework.

## 1. Why PF-3 is mandatory

PF-1 establishes a clean typed `opsctl` architecture-inventory reference implementation. PF-2 establishes reusable Hosted Operational Evidence. Those primitives are necessary but do not by themselves prevent future code from drifting back toward duplicated authorities, provider leakage, hidden effects, ad-hoc production flags or permanent compatibility shims.

Before FC-6 resumes, the repository therefore needs a small permanent fitness baseline that answers two questions for every future bounded slice:

1. what architecture/evolution rules are mandatory?;
2. which permanent machine check proves each mandatory rule?

The required sequence is:

```text
PF-1  Canonical Architecture Inventory cutover
  ->
PF-2  Hosted Operational Evidence primitive
  ->
PF-3  Architecture Fitness Baseline
  ->
re-baseline #399 / #421
  ->
FC-6 continuation
  ->
FC-7
  ->
AR-12...
```

## 2. Single source model for development requirements

The project must not create one giant replacement authority. Requirements remain layered by ownership:

```text
docs/ARCHITECTURE_REBASELINE_V3_PLAN.md
    program sequence, lifecycle and production-gate authority

architecture/architecture-program-sequence.json
    static AR order only

canonical AR/domain machine authorities
    actual domain/runtime/security/release facts

architecture/release-architecture-ar11.json
    activation units, release profiles, execution surfaces, production enablement

docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md
    human-readable cross-cutting development/evolution contract

architecture/architecture-fitness-policy.json   [PF-3 target]
    machine-readable rule catalog and enforcement mapping only

architecture/inventory.json
    generated projection; never a second source authority
```

`architecture/architecture-fitness-policy.json` must not duplicate domain values from existing authorities. It records rule identity, applicability and proof ownership. The domain fact remains owned by its canonical authority.

## 3. Machine-readable fitness policy

PF-3 must introduce one closed/versioned policy document, proposed path:

```text
architecture/architecture-fitness-policy.json
```

Minimum top-level contract:

```json
{
  "schema_version": 1,
  "kind": "ARCHITECTURE_FITNESS_POLICY",
  "status": "current",
  "rules": []
}
```

Each rule must have at least:

```text
rule_id
name
category
scope
source_authority
severity
required_from
machine_enforcement
negative_fixture_required
status
```

Allowed `status` is closed and fail-closed, for example:

```text
REQUIRED
DEFERRED_WITH_OWNER
RETIRED
```

A `REQUIRED` rule without an active machine enforcement mapping is a gate failure. `DEFERRED_WITH_OWNER` requires an explicit future owner/slice and may not be used for a Production-Core-critical P0/P1 boundary at AR-16/AR-17.

## 4. Initial mandatory rule catalog

PF-3 must encode and mechanically enforce an initial baseline covering at least the following rule families.

### AF-AUTH — authority uniqueness

- one semantic fact -> one canonical owner;
- one mutable concern -> one legitimate current mutable authority;
- no second lifecycle derivation;
- no second production-enable registry;
- generated projections cannot become input authority for the facts they project.

### AF-DEP — dependency direction / bounded contexts

- domain/application scopes remain provider/runtime-SDK free;
- adapters may depend inward, never the reverse;
- runtime product code must not depend on `opsctl`;
- cross-context mutation must not import another context's concrete persistence implementation;
- generic global service/helper layers may not become a second business-domain owner.

### AF-EFFECT — explicit side effects

Mechanically distinguish at least:

```text
DatabaseRead
DatabaseWrite
ProviderRead
ProviderWrite
SecretResolve
FilesystemRead
GeneratedProjectionWrite
ProcessExecution
NetworkAccess
DeploymentMutation
```

Critical mutations must have one explicit owner/capability and must fail closed before the first side effect when capability/auth/preconditions fail.

### AF-TYPE — typed identities/state/contracts

- critical semantically distinct IDs are not interchangeable raw strings at owned boundaries;
- lifecycle state uses one accepted state machine rather than unrelated boolean combinations;
- persisted/external/integration contracts are versioned where required;
- no duplicate OAuth/mailbox/profile/update lifecycle state machine.

### AF-CAP — capability / production exposure

- `source_present != production_enabled` remains true;
- release/capability profile is the only production-enable authority;
- no ad-hoc environment/frontend/config flag independently authorizes a capability;
- every execution surface maps to one valid activation unit or explicitly projected profile rule;
- enabled profile dependency closure is complete;
- disabled capability rejects backend-side before side effect.

### AF-PERSIST — persistence and migration ownership

- context-owned persistence boundaries;
- no unsafe cross-context direct table/repository mutation;
- one legitimate migration executor;
- schema compatibility and release compatibility remain machine-linked;
- fresh bootstrap is not silently treated as upgrade provenance.

### AF-CONFIG — configuration boundary

- raw environment/provider bindings resolved at bootstrap/composition edges;
- domain/application code does not scatter direct environment reads when typed configuration is required;
- secret material is not converted into general application configuration/readback.

### AF-EVENT — event/queue contracts

- domain events and cross-context integration events are not conflated;
- cross-context events are explicit/versioned where required;
- queue/DLQ remains transport/recovery boundary, not a parallel business state machine.

### AF-LEGACY — cutover discipline

A cutover is not complete until:

```text
new owner proved
-> all current callers switched
-> predecessor caller count = 0
-> predecessor unique-current-invariant count = 0
-> historical disposition updated
-> DEAD predecessor/shim removed
```

Compatibility aliases/shims require a proved current consumer or accepted compatibility contract.

### AF-OPS — `opsctl` boundary

- project-specific operational policy only;
- no runtime application dependency;
- no hidden persistent state backend;
- no provider/deployment executor role unless a later explicit authority changes the contract;
- network/process/provider effects remain explicitly bounded;
- command/operator-contract parity remains machine-checked.

### AF-READ — developer readability

For every Production-Core-relevant bounded context, repository metadata/docs must make it possible to identify:

```text
owner
canonical authority
entry/composition root
domain/application boundary
ports/adapters
effect boundary
persistence owner
execution surfaces
activation unit / release profile
```

AR-16 will re-audit this whole-project; PF-3 only establishes the persistent rule/gate mechanism.

## 5. Permanent enforcement architecture

PF-3 must not build a generic linter framework. Reuse existing repository validators and `opsctl` where they naturally own the policy. Add only missing bounded checks.

Target flow:

```text
canonical authorities
+ architecture-fitness-policy.json
+ repository/source graph
        ↓
typed/bounded validators
        ↓
positive + negative fixtures
        ↓
Architecture Fitness Gate
        ↓
required PR status
```

The gate must fail when:

- a REQUIRED rule has no declared enforcement;
- declared enforcement is missing/unreachable;
- a negative fixture expected to fail passes;
- a positive repository fixture fails;
- authority/rule IDs are duplicated or unknown;
- a rule is silently downgraded/removed without owning policy change;
- the candidate introduces a prohibited dependency/effect/production-enable path.

## 6. Enforcement registry rules

Do not encode the same semantic check in multiple unrelated scripts merely to increase coverage. Each fitness rule has one primary enforcement owner; additional tests may exercise it but do not become competing authorities.

The machine policy should reference permanent gate IDs/commands rather than copy their implementation details.

Example conceptual mapping:

```text
AF-DEP-001
  source_authority: docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md::Inward Dependency Rule
  machine_enforcement: architecture-fitness-gate::dependency_direction

AF-CAP-001
  source_authority: architecture/release-architecture-ar11.json
  machine_enforcement: architecture-fitness-gate::release_profile_single_enablement

AF-OPS-001
  source_authority: architecture/operator-contract.json
  machine_enforcement: opsctl-guard::effect_boundary
```

## 7. Slice-level Definition of Done after PF-3

Every future PF/FC/AR/PC implementation candidate that materially changes application architecture must include an Architecture Impact declaration covering:

```text
bounded contexts touched
authorities touched
public/persisted contracts touched
effect classes added/changed
execution surfaces added/changed
activation units/release profiles affected
migration/schema impact
legacy predecessor disposition
fitness rules affected
```

`none` is valid only when mechanically/procedurally justified by the diff.

Acceptance then requires:

```text
all applicable fitness rules PASS
+ all affected rule negative fixtures PASS
+ no new unenforced REQUIRED rule
+ no unexplained authority duplication
+ exact-head CI green
```

## 8. Touch-to-converge enforcement

PF-3 must preserve delivery velocity. It does not require a whole-repository rewrite.

```text
GOOD             -> preserve
TOUCHED          -> enforce applicable target rules now
LEGACY_UNTOUCHED -> may remain classified until owning work touches it
```

However a touched Production-Core-critical scope cannot use `legacy` as a permanent exemption. Any exemption must be explicit, bounded, owned by a future slice and fail AR-16/AR-17 if still material to Production Core.

## 9. PF-3 positive proofs

At minimum:

- policy schema parses and validates;
- all REQUIRED initial rules map to reachable permanent enforcement;
- current accepted repository passes the baseline or every pre-existing exception is explicitly bounded/owned;
- Linux CI passes;
- Windows CI passes where the checker/tooling is cross-platform;
- release-profile single-enablement and `opsctl` runtime-boundary checks pass;
- representative dependency/effect/cutover checks pass.

## 10. PF-3 negative proofs

At minimum prove rejection of fixtures containing:

- duplicate canonical owner;
- provider SDK import in a protected pure scope;
- product/runtime dependency on `opsctl`;
- unauthorized filesystem/network/process/provider mutation;
- second production-enable flag/registry;
- execution surface with missing/unknown activation unit;
- enabled release profile with incomplete dependency closure;
- unversioned required external/integration contract;
- direct cross-context persistence mutation where forbidden;
- REQUIRED rule with missing enforcement;
- stale DEAD predecessor still reachable after declared cutover;
- hidden operator state/provider executor authority.

## 11. PF-3 Definition of Done

PF-3 is complete only when:

1. one versioned machine-readable fitness policy exists;
2. every initial REQUIRED rule has one primary permanent enforcement owner;
3. positive + negative fixtures prove fail-closed behavior;
4. the Architecture Fitness Gate is part of permanent PR CI and is intended to become/participate in protected required contexts as governance permits;
5. architecture-impact metadata/process for later slices is documented and mechanically checked where practical;
6. no parallel roadmap/capability/lifecycle/domain authority is introduced;
7. current application behavior and production fail-closed state remain unchanged;
8. exact-head CI is green and accepted on `main`;
9. #399/#421 are freshly re-baselined only after PF-3 acceptance;
10. FC-6 resumes only from that accepted baseline.

## 12. What PF-3 deliberately does not do

PF-3 does not:

- refactor every bounded context;
- start AR-12;
- enable production;
- rewrite accepted AR-0…AR-11 history;
- replace existing specialized validators with one generic framework;
- create a second architecture inventory;
- create a second release/capability authority;
- create a generic DI/plugin/service-locator architecture.

Its job is narrower and more important: make the agreed development discipline persistent, machine-visible and fail-closed before feature/functional work continues.
