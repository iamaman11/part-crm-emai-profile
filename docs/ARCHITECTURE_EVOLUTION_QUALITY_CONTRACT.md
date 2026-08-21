# Architecture Evolution Quality Contract

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Static lifecycle order:** `architecture/architecture-program-sequence.json`  
**Current functional-closure plan:** `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`  
**Scope:** prospective quality/evolution rules for PF work, AR-12…AR-17 and PC-1…PC-4 without rewriting accepted AR-0…AR-11 history  
**Production authorization:** NONE

This document is not a second roadmap, lifecycle authority, capability registry, architecture inventory, or product release authority. It records the cross-cutting quality contract that future bounded slices must apply while preserving the canonical AR sequence and the principle:

```text
source_present != production_enabled
```

The existing accepted AR-0…AR-11 checkpoints remain immutable accepted history. They are not reopened merely to restyle already-correct code. When a later bounded slice materially touches an older subsystem, that touched subsystem must converge to this contract unless a current canonical authority proves a stronger or more specific invariant.

## 1. Target architecture

The project converges toward a high-assurance modular architecture built from four independent concerns:

```text
canonical authorities
        ↓
typed policy / contracts
        ↓
bounded-context domain + application
        ↓
explicit ports / adapters / effect capabilities
        ↓
composition roots
        ↓
release profile / capability admission
        ↓
production exposure
```

The application remains one modular product with one protected `main`, one architecture hierarchy, one schema/compatibility history and progressive production capability enablement.

## 2. Permanent evolution rules

### 2.1 Single Authority Rule

One semantic fact has one canonical owner. A projection, DTO, database row, frontend state, generated document or operator view may represent that fact but must not become a competing semantic authority.

For machine-governed architecture/operations concerns:

```text
one concern -> one legitimate current authority
```

Dual mutable authorities and duplicate lifecycle implementations are forbidden.

### 2.2 Bounded Context Rule

Business semantics belong to bounded contexts such as `identity`, `clients`, `profiles`, `devices/runtime`, `mailboxes`, and `notifications`, not to generic technical buckets such as a global service/helper layer.

A context owns its domain model and use-case semantics. Cross-context access occurs through explicit versioned contracts/ports or integration events rather than importing another context's persistence implementation.

### 2.3 Inward Dependency Rule

The intended dependency direction remains inward:

```text
domain
  ↑
application/use cases + ports
  ↑
adapters
  ↑
composition roots
```

Provider SDKs, HTTP frameworks, environment lookups, filesystem/process/network primitives and concrete persistence implementations must not leak into provider-free domain/application logic.

### 2.4 Pure Core / Effect Shell Rule

Decision, compatibility, compilation and state-transition logic should be deterministic and side-effect-free wherever practical. Effects are executed only at explicit boundaries.

Examples of explicit effect classes include:

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

A new abstraction is justified when it makes an invalid dependency or side effect impossible or mechanically detectable; abstractions added only for aesthetic layering are discouraged.

### 2.5 Typed Identity / State / Contract Rule

Use typed identities for semantically distinct IDs and explicit state machines for meaningful lifecycles instead of unrestricted strings and unrelated booleans. External/persisted contracts are versioned.

Representative identities/lifecycles include `ClientId`, `MailboxId`, `ProfileId`, `GenerationId`, `DeviceId`, `ReleaseSetId`, capability-profile identity, mailbox OAuth/onboarding state, profile-generation state, device-job state, release/promotion state and updater state.

Do not create a second state machine where an accepted authority already exists.

### 2.6 Command / Query Rule

Read/query operations and mutating commands remain semantically distinct. A query must not acquire incidental mutation. A mutating command must have an explicit mutation/effect capability and fail closed before the first side effect when authorization/capability/preconditions are not satisfied.

### 2.7 Release Profile Is Production Enablement Authority

Do not introduce a parallel feature-flag framework for production capability admission.

The canonical path is:

```text
Release / Capability Profile
        ↓
Effective Capability Set
        ↓
execution-surface admission
```

Environment variables, frontend visibility and ad-hoc config flags may project or configure an already-authorized capability but must not independently authorize it.

### 2.8 Frontend Projection Rule

Frontend visibility is never the security boundary. Backend execution surfaces must reject production-disabled capability paths before provider/database/customer-state mutation.

### 2.9 Domain vs Integration Event Rule

Internal domain events may use rich context-owned semantics. Cross-context integration events are explicit, minimal and versioned. Internal Rust/application structures do not automatically become public/integration contracts.

### 2.10 Persistence Ownership Rule

Persistence is context-owned. Avoid a universal business `Repository`, global CRUD service or cross-context direct table mutation. Use context-specific ports/readers/writers where they make ownership and mutation rights clear.

Schema migration, runtime compatibility and release compatibility must remain connected machine-checked concerns rather than independent directories/processes.

### 2.11 Typed Configuration Rule

Raw environment/provider bindings are resolved at composition/bootstrap edges into validated typed configuration. Domain/application modules do not scatter direct environment reads.

### 2.12 Error Taxonomy Rule

Keep domain/authorization/contract/infrastructure/operator failures distinguishable so HTTP, CLI and workflow presentation can map them without collapsing semantics or leaking provider details.

### 2.13 Cutover / Legacy Retirement Rule

A bounded authority or implementation cutover is complete only when:

```text
new owner active and proved
-> all current callers switched
-> predecessor caller count = 0
-> predecessor unique-current-invariant count = 0
-> historical classification updated where applicable
-> predecessor/shim removed when DEAD
```

Compatibility is not retained by default before production. It is retained only for a proved current consumer or an explicit accepted compatibility contract.

### 2.14 Touch-to-Converge Rule

Do not stop feature delivery for a repository-wide rewrite. Classify code as:

```text
GOOD             -> preserve
TOUCHED          -> bring the bounded touched scope to target architecture
LEGACY_UNTOUCHED -> classify and leave until an owning slice needs it
```

When `LEGACY_UNTOUCHED` becomes `TOUCHED`, the owning slice must converge it and remove the superseded path instead of permanently stacking old + new + compatibility shims.

## 3. Architecture fitness functions

Future slices must increase mechanical enforcement rather than rely only on prose. The permanent gate set should converge toward checks for at least:

- forbidden dependency edges;
- provider/runtime SDK imports in provider-free domain/application scopes;
- direct environment/process/network/filesystem mutation outside owned boundaries where the architecture requires an explicit port/capability;
- duplicate semantic/mutable authorities;
- duplicate or unknown activation-unit ownership;
- unknown execution surfaces;
- enabled release profile with incomplete dependency closure;
- mutation surface without explicit mutation capability;
- unversioned persisted/external/integration contract where versioning is required;
- second production-enable flag/authority competing with release profiles;
- stale or unreachable legacy executables after a cutover;
- hidden `opsctl` state or provider mutation authority;
- source-present production-disabled paths that do not fail closed backend-side.

The fitness suite should be implemented incrementally by the slice that first needs each rule. Do not create a broad generic architecture framework ahead of demonstrated use.

## 4. PF and Functional Closure impact

### PF-1 — Canonical Architecture Inventory cutover to `opsctl`

PF-1 is the first reference implementation of this contract:

```text
typed canonical authorities
-> validated inputs
-> pure inventory compiler
-> canonical render/check/inspect
-> exactly one bounded GENERATED_PROJECTION_WRITE
```

PF-1 must demonstrate Single Authority, Pure Core/Effect Shell, explicit effect capability, typed contracts, minimal composition-root wiring and predecessor retirement. It must not create a second lifecycle derivation or retain the historical Python generator as current compatibility authority.

### PF-2 — Hosted Operational Evidence

PF-2 must reuse the same canonical serialization/digest and typed-policy principles. Hosted evidence is an immutable evidence envelope/attestation pipeline, not a second evidence database or hidden `opsctl` state system. Provider/GitHub observation stays outside the pure policy core.

### PF-3 — Architecture Fitness Baseline

PF-3 makes this contract mechanically persistent before FC-6 resumes. It introduces one versioned machine-readable fitness-policy catalog mapping mandatory rule IDs to primary permanent enforcement owners, positive/negative fixtures and the Architecture Fitness Gate. A REQUIRED rule without active enforcement is a gate failure.

### FC-6 / FC-7

Functional Closure remains focused on proving the accepted AR-11 behavior. It must consume PF-1/PF-2/PF-3 primitives and must not create temporary parallel operational authorities. FC-7 also confirms this contract and the fitness baseline are reflected in current program/development projections before AR-12 implementation begins.

## 5. AR-by-AR prospective impact

### AR-0…AR-11 — accepted history

No retroactive rewrite or acceptance reset. Existing accepted authorities remain owners. If a future slice touches an AR-0…AR-11 subsystem, the touched scope must satisfy this contract and preserve accepted invariants.

### AR-12 — Fresh Rehearsal Environment

AR-12 keeps its existing purpose but its DoD is strengthened:

- provision and exercise rehearsal from accepted release/capability profiles rather than ad-hoc feature flags;
- build typed validated environment configuration at composition edges;
- keep provider mutation in explicit orchestration/executor adapters, outside policy/domain logic;
- prove effective capability-set admission for HTTP/queue/scheduled/service/Bridge surfaces;
- prove production-disabled capabilities remain fail-closed in rehearsal-compatible tests;
- use PF-2 Hosted Evidence for durable observations instead of feature-specific evidence mechanisms;
- satisfy the PF-3 architecture-fitness rules applicable to changed contexts/effects/contracts;
- no hidden rehearsal state authority and no new lifecycle derivation.

### AR-13 — Rotation Rehearsal

AR-13 remains credential/key rotation rehearsal but applies the stronger modular model:

- consume AR-8 credential/OAuth lifecycle authorities; no second OAuth/credential state machine;
- typed rotation plan/state and explicit secret/provider mutation capabilities;
- read/preflight separated from mutation executor;
- concurrency/fencing/revocation semantics remain in owned domain/application policy;
- version any external rotation evidence/contract;
- use Hosted Evidence for exact-head/provider observation proof;
- mechanically reject direct secret/provider mutation from non-owned modules.

### AR-14 — Remote Recovery Rehearsal

AR-14 remains recovery but becomes a reference for pure planning + explicit recovery effects:

- typed recovery identity, state and preconditions;
- pure `inspect/plan/verify` policy over D1/release/current-observation inputs;
- explicit remote restore/requeue/provider mutation capability only in the owning executor boundary;
- cross-context recovery through contracts/ports, not direct mutation of another context's storage internals;
- no hidden recovery database/state backend and no parallel DLQ state machine;
- versioned recovery evidence and fail-closed UNKNOWN compatibility.

### AR-15 — Windows Delivery Program

Existing E1–E7 + H remain binding and are strengthened rather than replaced:

- Profile Bridge runtime and updater remain separate bounded contexts/failure domains;
- typed release/update/profile-format/runtime identities;
- explicit updater state machine rather than scattered flags;
- pure compatibility/trust/activation decisions separated from filesystem/process/network effects;
- side-by-side activation and rollback own narrowly bounded filesystem/process capabilities;
- configuration/trust roots are typed and validated at startup/composition boundaries;
- cloud↔Windows release compatibility uses versioned contracts and the same release-profile authority;
- permanent Windows fitness/integration tests prove forbidden in-place mutation, unsafe activation and incompatible-profile launch paths.

### AR-16 — Final Whole-project 10/10 Audit

AR-16 is expanded into the convergence audit for this contract. In addition to existing P0/P1 requirements it must prove, for all production-core-relevant and materially touched scopes:

- authority uniqueness and no parallel production-enable registry;
- bounded-context ownership is understandable and mechanically mapped;
- inward dependency direction and provider-free core boundaries;
- no unjustified god composition/service/helper layer;
- explicit mutation/effect boundaries;
- typed critical IDs/states/versioned contracts where ambiguity creates real risk;
- context-owned persistence and no unsafe cross-context table/repository mutation;
- frontend projection remains non-authoritative;
- release profile dependency closure and backend fail-closed admission;
- no current DEAD legacy callers/shims from accepted cutovers;
- no hidden `opsctl` persistent state/provider executor role;
- Linux/Windows permanent gates for applicable operator/runtime scopes;
- developer-readability audit: a new maintainer can identify owner, entry point, authority, dependency direction, effect boundary and production gate for each Production Core context.

AR-16 does not require aesthetic restructuring of untouched correct code. It requires P0/P1=0 and no material production-core architecture debt hidden behind legacy compatibility.

### AR-17 — Architecture Closeout + Production Core Gate

AR-17 remains authorization, not implementation/rewrite. Before `production_core_gate=AUTHORIZED` it must verify:

- AR-16 convergence evidence is current;
- one canonical authority graph and one release/capability-profile enablement path;
- no competing feature-enable authority for Production Core;
- all production mutation surfaces are owned, explicit and gated;
- every Production Core activation unit has complete dependency/deployment/evidence closure;
- source-present disabled mailbox capabilities remain backend fail-closed;
- `architecture_complete=true` is justified without claiming `production_ready=true`.

AR-17 must not perform the first production deployment.

## 6. PC-by-PC impact

### PC-1 — Production Core v1

PC-1 consumes the architecture; it does not invent it. Production enablement occurs only through the accepted `production-core-v1` release/capability profile. PC-1 must deploy/promote same bits, materialize the effective capability set, prove backend admission and prove cloud + Windows compatibility. Mailbox capabilities remain source-present but disabled.

### PC-2 — Mailbox Administration

PC-2 is the bounded convergence/enablement point for mailbox administration/read/bindings:

- mailbox domain/application owns mailbox lifecycle semantics;
- Microsoft Graph/OAuth/provider code remains an adapter behind explicit ports;
- accepted onboarding/`ReauthRequired` authority is preserved rather than duplicated;
- typed mailbox/client/profile identities and explicit state transitions are used where required;
- client↔mailbox and browser↔mailbox links do not become ACL shortcuts;
- integration events are versioned where they cross context boundaries;
- enable only the PC-2 activation units/profile; jobs/outbound remain disabled.

### PC-3 — Mailbox Jobs / Automation

PC-3 activates automation without creating another mailbox state system:

- command/query semantics are explicit;
- queue/job contracts are versioned;
- idempotency, replay, fencing and ambiguous outcomes remain fail-closed;
- existing Queue + DLQ remain transport/recovery boundaries; no parallel domain DLQ state machine;
- background/scheduled/provider effects require explicit capability admission before enqueue/provider mutation.

### PC-4 — Outbound / subsequent capabilities

PC-4 activates outbound/provider side effects only after their bounded context is converged:

- explicit send command and provider-mutation capability;
- provider adapter isolated from domain/application policy;
- idempotency/outbox/reconciliation semantics proved against ambiguous provider outcomes;
- backend capability admission occurs before intent/provider side effects;
- versioned external/integration contracts and privacy/redaction requirements remain enforced.

Any later capability follows the same rule: source may land before activation, but the owning production profile is the only enablement authority.

## 7. Delivery strategy

Do not create a repository-wide architecture rewrite project. The execution model is:

```text
PF-1 reference implementation
-> PF-2 hosted evidence primitive
-> PF-3 architecture fitness baseline
-> re-baseline #399/#421
-> FC-6 / FC-7 closure
-> AR-12…AR-15 with touch-to-converge
-> AR-16 whole-project convergence audit
-> AR-17 authorization
-> PC-1 Production Core
-> PC-2 / PC-3 / PC-4 progressive capability enablement
```

Every bounded candidate keeps `main` releasable/testable, adds positive + negative permanent proofs, and deletes proven-dead predecessor paths after cutover.

## 8. Non-goals

This contract does not authorize:

- a global greenfield rewrite;
- renumbering accepted/future AR slices;
- a second roadmap/lifecycle authority;
- a generic plugin framework/service locator/DI framework;
- a universal repository/CRUD abstraction;
- a new production feature-flag authority;
- generic IaC/Terraform;
- hidden operator state;
- production mutation before the owning gate.
