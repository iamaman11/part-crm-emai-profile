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
-> F2 closure audit/fixes
-> invariant guards
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

### 4.4 Relationship between recovery steps and F1/F2

`R0…R9` is an execution/recovery sequence, not a second architecture roadmap and not a decomposition in which each `R` is a separate F-step.

The current combined transaction is interpreted as:

```text
R0                      recovery baseline / anti-drift setup
R1…R7                   F1 implementation + shared F2 foundations/cutovers
R8                      F2 closure gate
R9                      shared F1/F2 exact-head acceptance + merge gate
```

Therefore:

- `R8` is **not** a phase after F2 and must not be treated as a post-F2 checker project;
- F2 is not complete until R8 exits successfully;
- implementation of a real F2 boundary defect discovered during the R8 audit happens **before** the corresponding guard is accepted;
- work explicitly assigned by binding contracts to N1/N2/N3/N4/N5/PF-1/PF-2/PF-3 is not pulled forward merely to make F2 look globally clean;
- the generalized typed fitness registry remains PF-3 work, not R8 work;
- one existing natural checker/owner should be strengthened where possible instead of creating parallel checker frameworks.

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

### R8 — F2 closure: gap audit, bounded fixes, invariant guards

R8 closes F2; it does not start a separate checker architecture.

First perform an explicit contract-to-code audit of the F2 scope against the exact current head. Every F2 requirement must be classified as exactly one of:

```text
ALREADY_PROVED
TRUE_F2_IMPLEMENTATION_GAP
EXPLICITLY_OWNED_BY_LATER_BINDING_STEP
```

Rules:

- `ALREADY_PROVED`: reuse existing implementation/tests/evidence; do not add duplicate machinery merely to increase checker count;
- `TRUE_F2_IMPLEMENTATION_GAP`: fix the architecture first in its natural owner, add focused positive/negative evidence, then guard the invariant;
- `EXPLICITLY_OWNED_BY_LATER_BINDING_STEP`: record the exact owning N/PF step and do not pull the retirement/cutover forward into F2.

After all true F2 implementation gaps are closed, enforce the permanent F2 boundaries that are owned by the current transaction, including as applicable:

- pure-core effect/representation boundaries;
- no Product Runtime -> `opsctl`/`opsctl-core` dependency;
- no `opsctl` process/network/provider authority;
- no new/global semantic authority bag in the F1/F2-owned current path;
- no generated projection used as semantic input by the F1/F2-owned current path;
- Release Set breaking-contract version discipline;
- canonical JSON/digest and duplicate-member guarantees already owned by F1/F2.

Guard policy:

- prefer Rust types/tests and existing natural repository checkers over new framework code;
- one invariant gets one natural enforcement owner;
- do not freeze arbitrary dependency counts, exact forever dependency lists, file layouts or byte counts;
- do not build the PF-3 `FitnessRuleRegistry` early;
- do not turn transitional N2/N4/PF-1 retirement obligations into false F2 blockers when the binding contract explicitly assigns them later.

R8 exit gate:

```text
unclassified F2 requirements = 0
true unresolved F2 implementation gaps = 0
new duplicate checker/semantic owner = 0
F2-owned permanent zero budgets = mechanically protected where applicable
later-owned debt = explicitly mapped to its binding N/PF owner
production mutation/enablement = false
```

### R9 — Exact-head acceptance and guarded merge gate

R9 adds no architecture feature. It proves the completed R8 head and decides whether #445 may merge.

On one exact head prove:

```text
R8 F2 closure exit gate = PASS
behind_by = 0
required protected contexts = green
reviews blocking = 0
unresolved threads = 0
old current Release Set writer callers = 0
old unique current Release Set invariants = 0
semantic owner count per Release Set fact = 1
production state unchanged/fail-closed
```

Merge discipline:

1. PR #445 remains Draft throughout R8 implementation and its exact-head CI.
2. Record the R8 closure checkpoint only after the R8 exact head is fully proven.
3. Run/re-read the complete R9 hosted acceptance on that same exact head: protected contexts, divergence, reviews, threads and production fail-closed state.
4. Only after R9 passes may #445 leave Draft and enter guarded merge review.
5. Do not add code/document commits between the accepted R9 head and merge. Any head change invalidates R9 and requires exact-head acceptance again.
6. Merge F1+F2 together as the current atomic foundation transaction; do not merge at R7 merely because F1 implementation is functionally complete.
7. After merge, freshly re-read protected `main`, record the accepted-main checkpoint in #441, retire/delete the merged implementation branch where tooling permits, and only then create the N1 implementation branch from the new accepted `main`.

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
R8 implemented as a new generic checker framework
pulling N2/N4/PF-1/PF-3 retirement work into F2 without a binding ownership reason
```

## 8. Recovery Definition of Done

F1/F2 is complete only when the protected-main candidate evidence proves the current F1/F2-owned semantics and boundaries below. Legacy transitional obligations that the binding contracts explicitly assign to N1…N5/PF-1/PF-2/PF-3 remain visible debt, not hidden F2 work.

```text
Release Set v3 breaking-contract discipline = true
one current Release Set semantic owner = true
current writer is typed Rust-owned = true
historical v2 is immutable and isolated or absent by proven lack of need = true
old Python/current writer semantic authority = 0
serde_json::Value crossing into F1/F2 pure core = 0
filesystem/process/network/provider effects in F1/F2 pure core = 0
opsctl provider/network/process authority introduced/retained by F1/F2 = 0
Product Runtime -> opsctl/opsctl-core dependency introduced/retained by F1/F2 = 0
hidden command/composition bypass = 0
new/current F1/F2 semantic global authority bag = 0
generated projection used as semantic input by F1/F2 current path = 0
canonical digest layer has independent vectors = true
duplicate-member ambiguity rejected where attestable = true
breaking Release Set durable-contract change without version bump = 0
unclassified F2 requirement = 0
unresolved true F2 implementation gap = 0
later-owned normalization debt has explicit N/PF owner = true
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

After guarded acceptance, re-read protected `main`, update #441, delete/retire the merged implementation branch where tooling permits, and only then start N1 from a fresh baseline.
