# F1/F2 R1 — Release Set v3 Ownership and Caller Matrix

**Document status:** EXECUTION_EVIDENCE  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Prerequisite specification:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**Recovery protocol:** `docs/F1_F2_RECOVERY_EXECUTION_PROTOCOL.md`  
**Execution tracker:** #441  
**Recovery PR:** #445  
**Production authorization:** NONE

This document records the R1 discovery/ownership decision for the F1 Release Set v2 -> v3 cutover. It is evidence for implementation sequencing only. It is not a new semantic authority, contract registry, roadmap or generated input.

## 1. Exact baseline

R1 was performed against freshly re-read protected:

```text
main = ae092fe2791059b7f2a71a32331e8797c8457e24
clean branch = codex/f1-f2-clean-recovery
superseded reference = codex/f1-f2-architecture-foundations / PR #443
```

The clean branch is based directly on that `main`; no implementation commit from #443 is trusted wholesale.

## 2. Binding F1 problem

Current `main` has a breaking semantic mismatch:

```text
current durable Release Set contract/version = v2
current Rust release reader/model constant    = v2
current Python writer                         = v2

but current schema identity field already is:
    schemas.d1_repository_identity_sha256
```

The prerequisite contract already classifies the transition from the older `d1_evolution_authority_sha256` meaning to `d1_repository_identity_sha256` as a breaking external-contract semantic change. Therefore the current writer/model cannot continue to call the changed meaning Release Set v2.

Required destination:

```text
new current writer = v3
existing immutable v2 assets = unchanged
historical/current v2 reader support = isolated, read-only, only while a verified current consumer requires it
```

## 3. Current executable owners and callers discovered on `main`

### 3.1 Current Release Set writer

`script/release-set-ar11.py` is not merely packaging glue. On current `main` it owns all of the following for Release Set v2:

- `SCHEMA_VERSION = 2` and `release-set-v2-sha256-`;
- the external field inventory and current contract path;
- canonical JSON identity byte construction;
- Release Set content-address computation;
- source/component/contracts/protocol/schema/runtime/profile/build/artifact aggregation;
- deterministic runtime bundle construction;
- deterministic Profile Bridge component packaging;
- build/validate/self-test command behavior.

Classification for F1:

```text
TRANSITIONAL_SEMANTIC_SOURCE
```

Its Release Set semantic ownership must retire after callers are switched. Legitimate component-packaging helpers may survive only if separated from Release Set semantic/version/canonical identity ownership and assigned to their natural component/build owners.

### 3.2 Rust current reader/verifier

`tools/opsctl/src/release/model.rs` currently mixes:

```text
external JSON representation parsing
+ current v2 version policy
+ typed Release Set structures
+ semantic validation
+ retained serde_json::Value identity payload
+ content-address reconstruction
```

It is therefore a convergence target, not the final layer boundary.

### 3.3 Current contract artifact

`architecture/release-set-v2.json` currently declares v2 as `CURRENT_PRE_PRODUCTION`, v2 prefix only, unknown versions rejected and legacy compatibility disabled.

For F1 this file becomes historical v2 contract evidence after current writer cutover. It must never be rewritten to pretend that v2 had v3 semantics.

### 3.4 Hosted writer caller

`.github/workflows/release-set-build.yml` is the canonical hosted Release Set writer path on `main`:

- runs `python scripts/release-set-ar11.py self-test`;
- uses `profile-bridge-package` from that script;
- runs `python scripts/release-set-ar11.py build`;
- requires v2 ID regex;
- verifies with Rust `opsctl` expecting schema version 2;
- publishes immutable GitHub Release assets and notes as Release Set v2.

This is the primary R5 writer caller family to cut over atomically.

### 3.5 Current v2 reader/rollback consumers

`.github/workflows/release-set-promotion.yml` explicitly accepts and verifies `release-set-v2-sha256-*` durable releases. It resolves existing/current/known-good Release Sets and uses them in promotion/rollback evaluation.

Therefore R1 proves:

```text
historical_v2_reader_current_need = true
historical_v2_writer_current_need = false after build caller cutover
```

The v2 reader cannot be deleted at the same moment the v3 writer is introduced. It must be isolated from the v3 writer/model and remain read-only until later caller evidence proves it unnecessary.

### 3.6 Current architecture/CI caller

`.github/workflows/release-architecture-gate.yml` runs `python scripts/release-set-ar11.py self-test` before Rust `opsctl` release tests. This is another current caller that must stop treating the Python v2 writer as semantic authority before R7 retirement.

## 4. Release Set v3 single-owner matrix

The matrix separates **natural source owners**, **Release Set aggregate semantic invariants**, and **adapter effects/representation**. The Release Set core must not copy subject-domain policy from natural owners.

| Fact / section | Natural source owner | Release Set pure-core responsibility | Shell / adapter responsibility |
| --- | --- | --- | --- |
| current `schema_version` | F1 Release Set contract/version discipline | typed current version = v3; reject unsupported current-writer version | encode/decode numeric external field |
| ID prefix / `release_set_id` | derived Release Set identity | bind typed ID shape/version to semantic result; no JSON hashing | canonicalize external identity DTO, SHA-256 exact canonical bytes, render `release-set-v3-sha256-*` |
| `source.repository` | canonical repository identity | validate expected repository identity | acquire/decode explicit source observation |
| `source.commit_sha` | Git source identity | validate typed SHA shape and cross-component equality | acquire exact Git/GitHub observation outside pure core |
| accepted-main fact | accepted-source evidence owner / outer GitHub observation | consume typed accepted-source fact; fail closed when not accepted | collect/verify external observation and convert to typed input; no network in core |
| component release IDs/manifests | each component builder/natural component owner | allowed/required component set; component key consistency; same source SHA; aggregate artifact correspondence | read component manifest/artifact observations and convert to typed facts |
| `contracts.files` source bytes | natural public/cross-language contract owners from release-input topology | unique/non-empty typed provenance entries and aggregate identity consistency only | read files, exact byte hashes/sizes, DTO conversion |
| `contracts.sha256` | derived aggregate identity | consume/validate typed digest identity | canonicalize/hash the defined external aggregate scope |
| public API protocol digest | accepted public API contract owner | include typed digest in release compatibility identity | acquire from contract projection/bytes |
| Camouhost IPC version | runtime/IPC natural owner | typed positive/compatible protocol fact | extract from canonical runtime/IPC contract |
| Profile Bridge protocol | Profile Bridge/bridge-domain natural owner | typed positive/compatible protocol fact | extract from component/runtime observation |
| resolver protocol | resolver natural owner | typed non-empty/compatible protocol identity | extract from resolver contract/manifest |
| `schemas.d1_repository_identity_sha256` | typed D1 repository identity owner | include typed digest; never reconstruct D1 policy | call/read typed D1 projection adapter and convert result |
| catalog/resolver schema windows | typed D1 release-schema owner | validate typed compatibility-window coherence only; no SQL/filesystem policy duplication | obtain typed D1 release contracts |
| runtime lock digest/facts | `runtime/camouhost/runtime-lock.json` + runtime natural owners | typed runtime compatibility identity/invariants | strict decode/extract/hash runtime lock externally |
| capability profile compatibility IDs | release architecture capability-profile owner | non-empty/unique/canonical typed set; no reimplementation of profile graph | project exact profile IDs from natural release architecture owner |
| build provenance lock/toolchain digests | corresponding lock/toolchain/release-architecture files | typed digest presence/shape only | read and hash exact source bytes |
| artifact inventory | component artifact builders | unique paths, positive sizes, digest shape, component↔artifact correspondence | inspect exact artifact files/bytes |
| `display_version` | presentation only | no semantic identity ownership | optional output DTO/presentation; excluded from content address |

Invariant:

```text
semantic_owner_count_per_fact = 1
```

A natural owner supplies facts; Release Set core owns only cross-section Release Set invariants and compatibility identity composition. Adapters own I/O and representation.

## 5. Target layer split

R1 fixes the required shape before implementation movement:

```text
raw local/hosted observations
        ↓
filesystem / explicit observation adapters
        ↓
strict versioned ReleaseSetV3 DTOs
        ↓
typed conversion
        ↓
opsctl-core::release
ONE Release Set aggregate semantic owner
        ↓
typed ReleaseSetV3 semantic result
        ↓
ReleaseSetV3 output DTO
        ↓
RFC 8785-compatible canonical JSON adapter
        ↓
SHA-256 adapter
        ↓
release-set-v3-sha256-* external identity
```

Forbidden dependency direction:

```text
opsctl-core -> serde_json::Value
opsctl-core -> filesystem/path/process/network/provider/runtime
Product Runtime -> opsctl / opsctl-core
```

## 6. Historical v2 isolation decision

R1 proves a current read need because promotion/rollback still resolves durable v2 Release Sets.

Target isolation:

```text
current writer/model authority
    = v3 only

historical v2 decoder/verifier
    = read-only compatibility module
    = cannot construct/write v3
    = cannot define current writer constants
    = cannot influence v3 field inventory/identity
    = retained only while exact current callers require it
```

Rules:

1. Existing immutable v2 GitHub Release assets/tags are never rewritten.
2. No new v2 Release Set is authored after R5 writer cutover.
3. New accepted-main build output is v3 only.
4. Promotion/rollback may read verified historical/current-known-good v2 while such durable releases are still legitimate current inputs.
5. v2 compatibility removal is a later zero-caller decision, not an F1 shortcut.
6. Python v2 writer semantics retire once writer/self-test callers are cut over and `old_unique_current_invariants = 0`.

## 7. Component packaging boundary

The existing Python writer also builds the runtime bundle and Profile Bridge package. Those operations are not automatically Release Set semantic policy.

During R5/R7, each helper must be classified independently:

```text
component deterministic packaging concern
    -> natural component/build owner

Release Set field/version/canonical identity policy
    -> current v3 Release Set path only
```

Do not keep `scripts/release-set-ar11.py` alive merely because one component packaging helper is still useful. Extract/rehome legitimate packaging behavior before retiring its Release Set semantic authority.

## 8. Selective salvage classification for PR #443

No item below is trusted merely because it existed or compiled in #443.

### SALVAGE_CANDIDATE — review/rebuild narrowly

- typed v3 Release Set structures and validation concepts from old `tools/opsctl/core/src/release.rs`;
- reviewed/pinned SHA-256 replacement concept;
- RFC 8785/JCS canonicalization concept and independent vectors;
- strict duplicate-member + byte/depth JSON admission concept;
- the final correction that restored `opsctl release finalize` to the normal typed CLI/composition path.

### REIMPLEMENT_FROM_CONTRACT

- filesystem/release-input acquisition for finalization;
- ReleaseSetV3 DTO conversion and rendering;
- current-v3 vs historical-v2 reader split;
- caller cutover in build/promotion workflows;
- F2 architecture guards.

Reason: the superseded implementation mixed correct ideas with distributed ownership and transitional compatibility assumptions.

### REJECT

- hidden `--machine-*` command path or second argv parser;
- making a wrong composition path work by widening unrelated internal visibility;
- dependency-free core as a permanent architectural dogma;
- exact forever shell dependency allowlist as semantic architecture;
- arbitrary `main.rs` byte-size threshold as architecture;
- typed model -> JSON -> reader round-trip as internal semantic correctness mechanism;
- a large finalizer that duplicates Release Set semantic rules already owned by pure core;
- any CI-specific architecture exception.

## 9. R1 caller cutover inventory

Known current caller families that must be explicitly re-checked during R5-R7:

```text
.github/workflows/release-set-build.yml
    - Python v2 self-test
    - Python v2 build
    - Python Profile Bridge packaging helper
    - v2 regex/version assertions
    - v2 publication wording

.github/workflows/release-architecture-gate.yml
    - Python v2 self-test
    - AR-11 behavioural certification paths that may encode v2 assumptions

.github/workflows/release-set-promotion.yml
    - v2 dispatch validation
    - v2 target/current/known-good download and verify
    - promotion/rollback compatibility with immutable historical v2

tools/opsctl release/promotion tests and commands
    - current v2 reader/model constants
    - output schema-version assertions
    - fixtures/tags/prefix assumptions

architecture/release-set-v2.json
    - historical contract after current-v3 cutover; never rewritten
```

Before R7 retirement, repository-wide current caller discovery is repeated on the exact candidate; this list is an R1 baseline, not permission to assume no additional callers exist.

## 10. R2 implementation boundary fixed by R1

R2 may now introduce only a **minimal typed pure Release Set v3 model**.

R2 MAY own:

- `ReleaseSetSchemaVersion::V3`;
- typed Release Set aggregate structures;
- cross-section semantic invariants listed in the matrix;
- pure typed errors/decisions;
- in-memory positive/negative tests.

R2 MUST NOT yet own:

- filesystem/path acquisition;
- JSON DTO parsing/rendering;
- canonical JSON;
- SHA-256 implementation;
- GitHub/provider observations;
- runtime-lock parsing;
- D1 source parsing;
- CLI `release finalize`;
- workflow cutover;
- historical-v2 parsing inside the current v3 model.

This bounded R2 scope prevents the pure model from becoming a second finalizer/adapter authority.

## 11. R1 exit gate

R1 is complete for implementation purposes when the clean recovery branch records all of the following:

```text
breaking v2 -> v3 requirement = explicit
current writer owner/caller = identified
current v2 read compatibility need = identified
historical v2 isolation strategy = explicit
one-owner matrix = explicit
adapter vs pure-core responsibilities = explicit
salvage/reimplement/reject matrix = explicit
R2 allowed scope = bounded
production authorization = NONE
```

With these conditions satisfied, the next code change is R2 only: minimal in-memory pure Release Set v3 semantics, independently reviewed before R3 begins.
