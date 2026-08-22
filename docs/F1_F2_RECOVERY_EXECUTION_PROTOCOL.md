# F1/F2 Clean Recovery Execution Protocol

**Document status:** SUBORDINATE_EXECUTION_PROTOCOL  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Prerequisite specification:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**Execution tracker:** #441  
**Superseded implementation reference:** Draft PR #443 / `codex/f1-f2-architecture-foundations`  
**Production authorization:** NONE

This protocol exists only to prevent implementation drift while completing F1/F2. It does not create a new roadmap, lifecycle slice, semantic registry or architecture authority. If this document conflicts with the program authority, prerequisite specification or `OPSCTL_ARCHITECTURE_BOUNDARY`, the higher-level contract wins.

## 1. Recovery decision

The prior F1/F2 implementation branch remains useful as forensic/salvage material, but it is not the continuation base. The clean recovery transaction starts from freshly re-read protected `main`.

Recovery baseline:

```text
protected main = ae092fe2791059b7f2a71a32331e8797c8457e24
clean branch   = codex/f1-f2-clean-recovery
old branch     = codex/f1-f2-architecture-foundations  # salvage/reference only
```

No commit from the old branch is cherry-picked wholesale. Salvage happens by concern after independent review against current `main` and the binding contracts.

## 2. Why the previous drift happened

The failure mode was not stale Git ancestry. It was local optimization inside a long-running WIP transaction:

- intermediate compatibility paths started becoming architecture;
- a machine-only Release Set path temporarily bypassed the normal typed command pipeline;
- mechanical guards began enforcing implementation accidents such as dependency-count policy rather than architectural invariants;
- Release Set semantics were being distributed between pure core, reader/model and finalizer rendering paths;
- the branch accumulated many commits before one stable end-to-end owner/caller cutover was proven.

The recovery therefore changes the development method, not only the code.

## 3. Permanent anti-drift rules for F1/F2

Every implementation step MUST satisfy all rules below before the next step begins.

### 3.1 One semantic owner before code movement

For every fact/rule being changed, write down its natural owner first.

```text
semantic_owner_count_per_fact = 1
```

Adapters may decode, read, hash or render. They do not re-decide core policy. Generated projections never become semantic inputs.

### 3.2 One command/composition path

Non-trivial `opsctl` behavior uses the normal pipeline:

```text
CLI/composition
-> local input adapters
-> versioned DTO decode
-> typed semantic input
-> pure core
-> typed result
-> output adapter
```

Forbidden:

```text
hidden argv command path in main.rs
second parser/registry for machine callers
special binary entrypoint bypassing typed Invocation/composition
```

Machine callers may use a machine-stable command/DTO, but it must enter through the same composition boundary.

### 3.3 Guard invariants, not implementation accidents

Fitness/checker rules must enforce architecture properties, not arbitrary current shapes.

Correct examples:

```text
filesystem/network/process/provider effects in pure core = 0
serde_json::Value crossing adapter -> pure core = 0
Product Runtime -> opsctl dependency = 0
opsctl provider/network/process authority = 0
global authority bag = 0
breaking durable contract without version bump = 0
```

Forbidden examples unless separately justified by a binding contract:

```text
core dependency count must equal zero
shell dependency list must equal one exact hard-coded set forever
main.rs must stay below an arbitrary byte count
one specific file layout is architecture by itself
```

A narrow pure dependency is acceptable when it preserves purity and has an explicit reason. Zero hidden effects/representation leakage is the goal; zero dependencies is not.

### 3.4 No semantic round-trip through JSON inside the core decision path

JSON is an external representation. Internal validation must not depend on:

```text
typed model -> serde_json::Value -> canonical JSON -> reader -> typed model
```

as a semantic correctness mechanism.

Required target:

```text
external bytes
-> strict DTO
-> typed core model
-> typed core validation/decision
-> output DTO
-> canonical external bytes
```

A final external-byte round-trip test may exist as adapter-contract evidence, but it is not the semantic owner.

### 3.5 Bounded commits and stop-the-line review

Each commit owns one concern and must be independently explainable.

Before proceeding to the next concern, verify:

```text
scope is still F1/F2
no second semantic owner introduced
no temporary compatibility path promoted to permanent architecture
no new provider/process/runtime authority
no production enablement
```

If a step requires a special architectural exception to make CI pass, STOP. Do not encode the exception. Re-evaluate ownership and composition.

### 3.6 No CI-driven architecture changes

CI failures are evidence, not design requirements. A fix is accepted only when it follows the binding architecture without weakening or inventing boundaries.

Never:

```text
make private internals public only to preserve a wrong composition path
bypass parser/registry to satisfy one workflow
weaken semantic ownership to preserve a transitional caller
add duplicate logic because migrating the caller is inconvenient
```

### 3.7 Salvage by concern, never by branch

Material from PR #443 is classified per concern:

```text
SALVAGE_CANDIDATE
REIMPLEMENT_FROM_CONTRACT
REJECT
```

No whole commit/series is trusted merely because it previously compiled or passed some checks.

### 3.8 Small review horizon

The F1/F2 recovery must not again become a 30+ commit architecture experiment before exact-head integration proof.

Preferred execution cadence:

```text
foundation contract/proof
-> pure model
-> external adapters
-> one caller cutover
-> predecessor retirement
-> full exact-head proof
```

If the transaction cannot remain reviewable, split F1 and F2 at a real rollback boundary rather than accumulating compatibility scaffolding.

## 4. F1/F2 target ownership

### 4.1 Release Set

Target:

```text
ReleaseSetV3 external DTO
        ↓
strict adapter validation
        ↓
opsctl-core::release typed model
        ↓
ONE semantic owner
        ↓
typed output DTO
        ↓
RFC 8785 JCS + SHA-256 adapter
```

The pure core owns Release Set semantic invariants. The shell owns filesystem, DTO conversion, canonical rendering and exact-byte hashing. Historical v2 support, if still required by current callers, is isolated and cannot influence v3 writer semantics.

### 4.2 Canonical JSON/digest

Allowed salvage candidates from #443 include reviewed/pinned SHA-256, RFC 8785-compatible canonicalization, duplicate-member rejection, bounded input size/depth and independent vectors. These remain adapter concerns and must not leak `serde_json::Value` into the pure model.

### 4.3 `opsctl-core`

The internal crate exists only where compile-time separation materially protects pure semantics.

Required:

```text
opsctl shell/adapters -> opsctl-core
opsctl-core -X-> filesystem/network/process/provider/runtime
Product Runtime -X-> opsctl-core
```

Dependency purity is reviewed by capability/effect, not by dependency count alone.

## 5. Step-by-step recovery sequence

### R0 — Baseline and freeze old path

- re-read protected `main` and required contexts;
- keep PR #443 Draft/superseded and non-mergeable as current work;
- use the old branch only for diff/forensic salvage;
- prove clean branch `behind_by=0` from protected `main`.

### R1 — Contract-first Release Set v3 boundary

Before moving implementation:

- define current v3 external fields and identity scope;
- define historical-v2 isolation decision;
- define exact semantic owner matrix for source/components/contracts/protocols/schemas/runtime compatibility/capability compatibility/build provenance/artifact inventory;
- add failing/positive focused tests for version discipline without introducing writer duplication.

Exit gate: one unambiguous v3 semantic owner design.

### R2 — Minimal pure Release Set model

- introduce/extract the typed pure model;
- no filesystem, process, environment, paths-as-semantic-identity, network/provider APIs or generic JSON values;
- only narrowly justified pure dependencies;
- in-memory positive and negative tests.

Exit gate: pure model can be tested without repository fixtures.

### R3 — Strict external DTO + canonicalization adapters

- strict versioned DTO;
- unknown-field and duplicate-member fail-closed behavior where required;
- byte/depth budgets;
- RFC 8785/JCS-compatible canonical rendering where contract-compatible;
- reviewed SHA-256 implementation;
- independent canonicalization/hash vectors.

Exit gate: representation code remains outside pure semantics.

### R4 — Normal typed `opsctl release finalize` composition

- one typed invocation/composition path;
- no hidden/machine-only parser bypass;
- local filesystem inputs resolved by adapters;
- DTO converted to typed inputs;
- pure core returns typed result;
- renderer produces canonical bytes.

Exit gate: CLI and machine usage share one architecture.

### R5 — Current writer caller cutover

- inventory every current Release Set writer caller;
- migrate one caller family at a time to the typed v3 path;
- preserve accepted behavior and release identity rules;
- no predecessor removal until current caller count is zero.

Exit gate:

```text
old_current_writer_callers = 0
```

### R6 — Reader/verifier compatibility cutover

- current v3 reader/verifier uses the typed owner;
- historical v2 compatibility exists only if a current verified consumer requires it;
- historical decoder is isolated and cannot author v3.

Exit gate: no dual current Release Set semantic authority.

### R7 — Retire predecessor semantic ownership

- remove Python/current v2 writer semantic authority;
- remove duplicated version/field/canonical identity tables;
- keep historical fixtures/evidence without executing retired authority;
- prove `old_unique_current_invariants = 0`.

### R8 — F2 guards from architectural invariants

- enforce pure-core effect/representation boundaries;
- enforce no runtime->opsctl dependency;
- enforce no process/network/provider authority;
- enforce version bump for breaking durable contracts;
- reject global authority bags and generated-projection semantic inputs;
- do not freeze arbitrary dependency/file/byte-count implementation details.

### R9 — Exact-head acceptance

On one exact head prove:

```text
behind_by = 0
required protected contexts = green
reviews blocking = 0
unresolved threads = 0
old current Release Set writer callers = 0
old unique current Release Set invariants = 0
semantic owner count per Release Set fact = 1
production state unchanged/fail-closed
```

Only then may the recovery PR leave Draft and enter guarded merge review.

## 6. Mandatory per-commit checklist

Every commit message/PR checkpoint should be reviewable against:

- What single concern changed?
- What is the natural semantic owner?
- Did any second path/owner appear?
- Are effects confined to adapters?
- Is JSON only representation at the relevant boundary?
- Is a new guard checking an invariant rather than a file-layout accident?
- Which current caller moved?
- Which predecessor caller count decreased?
- What positive and negative evidence was added?
- Does production remain fail-closed?

If any answer is unclear, the next implementation step does not begin.

## 7. Explicit rejected patterns from the superseded attempt

These patterns are not to be reintroduced:

```text
hidden --machine-* command bypass in main.rs
second argv parser for build workflows
pure-core dependency-count dogma
exact shell dependency list as permanent architecture
large finalizer that becomes a second Release Set semantic owner
JSON round-trip used as internal semantic validation
CI-specific architecture exceptions
```

## 8. Recovery Definition of Done

F1/F2 is complete only when protected-main candidate evidence proves:

```text
Release Set v3 breaking-contract discipline = true
one current Release Set semantic owner = true
current writer is typed Rust-owned = true
historical v2 is immutable and isolated or absent by proven lack of need = true
old Python/current writer semantic authority = 0
serde_json::Value crossing into pure core = 0
filesystem/process/network/provider effects in pure core = 0
opsctl provider/network/process authority = 0
Product Runtime -> opsctl/opsctl-core dependency = 0
hidden command/composition bypass = 0
global authority bag = 0
generated projection used as semantic input = 0
canonical digest layer has independent vectors = true
duplicate-member ambiguity rejected where attestable = true
breaking durable-contract change without version bump = 0
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

After guarded acceptance, re-read protected `main`, update #441, delete/retire the merged implementation branch where tooling permits, and only then start N1 from a fresh baseline.
