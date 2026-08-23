# Browser Profile Platform — Development Plan

**Document status:** GENERATED_PROJECTION  
**Canonical program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Live execution tracker:** #441  
**Functional Closure umbrella:** #399  
**Production Core gate:** `BLOCKED`  
**Production readiness:** `false`

This is a compact developer-facing execution projection, not a second roadmap or semantic authority. Protected `main`, the canonical plan and the live tracker win over stale prose. Exact moving SHAs belong in GitHub state, not in this file.

```text
source_present != production_enabled
```

## 1. Current state

```text
F1/F2  ACCEPTED
N1     ACCEPTED
#454   NEXT — sole actual pre-N2 implementation transaction
N2     BLOCKED on #454
N3     BLOCKED on N2
N4     BLOCKED on N3
N5     BLOCKED on N4
PF-1   BLOCKED on N5 + fresh #430 entry reread
AR-12  NOT STARTED
production_mutation = false
```

#399/#421 preserve later Functional Closure obligations. They do not create another pre-N2 phase.

## 2. One execution discipline, not one ceremony per issue

Quality comes from strong, reusable machine proofs and clear semantic ownership, not from repeating large acceptance matrices in every issue.

For every bounded authority cutover:

```text
fresh protected main
-> discover current consumers/callers + natural owner
-> preserve still-valid product/security/durable invariants
-> switch current callers
-> old_current_callers = 0
-> old_unique_current_invariants = 0
-> delete/demote DEAD predecessor
-> targeted fast/local proof
-> one unchanged exact head through applicable permanent CI/governance
-> guarded merge bound to expected head
-> accepted-main reread
```

The shared merge/governance protocol is owned by `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md`; stage documents should describe only their stage-specific semantic DoD instead of cloning the same CI/review checklist.

Rules:

- small reviewable PRs are preferred; PR count must not create new roadmap phases;
- no compatibility bridge without a named current consumer or durable/persisted/migration obligation;
- no 1:1 successor registry for a retired JSON/Python/Node/table authority;
- no new plan document when an existing canonical owner/tracker can express the requirement;
- use `python scripts/verify-fast.py` and targeted tests during iteration; permanent CI is the acceptance proof, not the edit/compile loop;
- observations may be ephemeral PR/CI artifacts; do not commit a new authority file merely to record a one-time caller audit;
- uncertainty fails closed.

## 3. #454 — Release Set v2 bounded correction

F1 already made Release Set v3 the current writer/model. #454 answers one question: **does executable historical-v2 compatibility have a real current consumer or durable obligation?**

Rediscover on the exact execution baseline:

- `architecture/release-set-v2.json` callers/references;
- historical-v2 decode/verification callers;
- promotion, rollback, deployment-snapshot and expected-current paths;
- actual current staging Release Set identity;
- actual current known-good rollback identity;
- any explicit current #399/#421/FC-6 durable v2 input;
- hosted workflows/releases/evidence that are real current inputs rather than provenance.

Then choose exactly one outcome:

```text
A. current v2 consumer exists
   -> name exact consumer + exact identity/version
   -> keep only minimum isolated historical-v2 read/verify path
   -> current writer/model remains v3-only
   -> v2 -> v3 semantic coercion = 0
   -> explicit retirement condition

B. current v2 consumer = NONE
   -> retire executable v2 compatibility/current-v2 authority
   -> remove compatibility-only code/workflows/tests/fixtures after caller/invariant proof
   -> preserve history in Git/releases/evidence only
```

#454 stage-specific DoD is intentionally small:

```text
current_writer = v3
current_v2_semantic_authority = 0
v2_consumer = EXACT_ID | NONE
v2_executable = MINIMUM_ISOLATED | RETIRED
v2_to_v3_semantic_coercion = 0
production_mutation = false
```

The shared transaction protocol supplies the zero-caller, exact-head, review and merge proofs. Only accepted #454 protected `main` may become the N2 base.

## 4. N2–N5 — canonical-authority cutover group

N2–N5 remain separate sequential transactions because they retire different semantic owners, but they are **one normalization group**, not four new architecture programs.

After #454 acceptance, perform one read-only discovery pass across the N2–N5 predecessor estate to map current callers, current invariants and likely natural owners. In that same pass, audit concrete current/durable consumers of the **exact tracked bytes** of `architecture/inventory.json` and resolve `JUSTIFIED_MINIMUM | NOT_RETAINED` before PF-1. Keep this map ephemeral (working notes / PR discussion / CI artifact), not as a new checked-in authority registry. Before each N2/N3/N4/N5 PR, refresh only the affected reachability plus deltas since the common discovery.

The inventory retention audit is intentionally narrow:

```text
real durable exact-byte consumer exists
-> keep only the minimum deterministic GENERATED_PROJECTION it requires

consumer = NONE
-> retire tracked architecture/inventory.json after its remaining callers are naturally cut over
-> retire compatibility-only --write / tracked-byte drift ceremony
-> keep useful deterministic on-demand render/check only
```

A generator checking the file because it exists, documentation references, historical evidence, and CI drift tests that exist solely for the tracked projection are not consumer proof. This is not an early PF-1 compiler implementation and does not create another phase.

### N2 — Python estate

Target:

```text
Python file existence != Python semantic authority
```

- retire AR-6/AR-10/AR-11 per-file Python-estate authority and `scripts/python-estate-ar6.py` when dead;
- govern Python by source-derived role/effects; do not create a successor file registry;
- remove retiring Python/AR sentinels from `opsctl doctor` and repository-root detection;
- keep `runtime/camouhost/real.py` as the real Camoufox outer-runtime adapter behind Profile Bridge + versioned IPC + `runtime-lock.json`;
- keep `runtime/camouhost/main.py` synthetic/test-only;
- do not rewrite legitimate Python tests/generators/adapters to Rust merely for symmetry;
- for obsolete helpers: caller=0 -> delete; caller>0 -> move only the still-valid responsibility to its natural owner.

### N3 — GitHub governance

Replace historical overlay reconstruction with:

```text
current desired governance configuration
+
live GitHub observation
-> typed governance evaluation
```

Declarative desired configuration for an external system is legitimate data. N3 does not move GitHub/network authority into `opsctl` and does not require rewriting all GitHub automation in Rust.

### N4 — operator/provenance

Typed Rust command/effect metadata becomes the semantic owner. Prefer metadata colocated with existing command definitions and aggregate it through the existing parser/composition root; do not build a second generic command framework.

`architecture/operator-contract.json` must not authorize Rust behavior. If no current external consumer requires a JSON view, delete it after caller proof; otherwise it is generated projection only. AR-8 provenance leaves normal current semantic paths.

### N5 — runtime cutover authority

For each still-current field in `architecture/runtime-cutover-ar10.json`:

```text
not current -> retire with predecessor
current -> Product Rust | runtime-lock | Bridge/IPC | governance | release/lifecycle natural owner
```

N5 is field disposition + caller cutover + deletion, not a new runtime framework. Do not create `RuntimeCutoverRegistryV2` or an equivalent successor authority.

## 5. PF-1 → PF-3 — final architecture-forming work

### PF-1 — lifecycle + bounded inventory

PF-1 replaces the legacy Node lifecycle and Python inventory/projection cluster with typed lifecycle evaluation and bounded owner projections.

Hard rule:

```text
PF-1 compiler may COMPOSE facts
PF-1 compiler may NOT DISCOVER domain semantics
PF-1 compiler may NOT DECIDE bounded-subject policy
```

Target shape:

```text
natural owner -> validated narrow projection --\
natural owner -> validated narrow projection ----> ArchitectureInventoryCompiler
natural owner -> validated narrow projection --/
```

No `GlobalRepositoryAuthorityLoader`, `GlobalAuthoritySet`, giant policy compiler or 1:1 port of historical AR-qualified tables.

PF-1 consumes the pre-PF-1 tracked-inventory retention result instead of deciding it for the first time. If the result is `NOT_RETAINED`, PF-1 must not resurrect checked-in `architecture/inventory.json` or compatibility-only `--write`/drift ceremony; useful deterministic on-demand rendering/checking may remain. If the result is `JUSTIFIED_MINIMUM`, PF-1 emits only the minimum deterministic generated projection required by the proved exact-byte consumer. In either case generated output is never semantic input.

Similarly, command surface should stay minimal: do not keep `render/check/write/inspect` as four permanent commands unless each has a distinct proved consumer/value.

### PF-2 — minimal hosted evidence pipeline

PF-2 is an evidence pipeline, not a universal provider/plugin framework:

```text
outer GitHub/provider observation
-> strict secret-free versioned DTO
-> typed pure EvidencePolicy
-> HostedEvidenceEnvelopeV1
-> canonical bytes/digest
-> immutable artifact/attestation
```

Add abstraction only after multiple concrete consumers prove the need. Network/provider reads and publication stay outside pure policy.

### PF-3 — small enforcement index + freeze

PF-3 makes already-selected architecture guarantees permanent and machine-enforced. `FitnessRuleRegistry` (or equivalent) should be a small typed index such as:

```text
RuleId
requiredness
scope
primary enforcement owner
negative fixture
```

Reuse specialized validators/checkers as enforcement owners. Do not build a generic linter/plugin/DI platform. Machine checks enforce objective properties (dependency/effect/authority/compatibility/projection rules); PR Architecture Impact + protected review handles genuinely semantic questions that cannot be reliably inferred by a universal checker.

PF-3 remains the architecture-forming freeze. After it, ordinary FC/AR/PC work does not invent new generic architecture mechanisms.

## 6. Functional Closure — proof, not another architecture program

The logical `fresh #399/#421 re-baseline` remains mandatory but executes as the **first read-only FC-6 preflight observation**, not as a separate implementation transaction or PR.

```text
PF-3 accepted
-> FC-6 preflight
   - fresh #399/#421 live re-baseline
   - current accepted main
   - live workflows / protected contexts
   - credential readiness/scope
   - current staging identity
   - current known-good identity
   - Release Set identities
   - required hosted evidence/attestations
   - live provider/GitHub observations
-> READY | typed BLOCKED
-> only READY may expose deploy-capable credentials / permit staging mutation
-> FC-6 same-bits staging / verify / rollback-or-NO_CHANGE ceremony
-> machine-readable terminal evidence
-> FC-7 closeout evaluation
```

FC-7 remains a logical acceptance checkpoint for traceability, but it should not create a second implementation project. If FC-6 evidence plus repository/Linux/Windows/hosted proofs establish `P0=0`, `P1=0`, `P2=0`, FC-7 is a closeout decision. New source work occurs only for an actual defect discovered by the proof.

## 7. AR-12…AR-17 — qualification semantics

After PF-3 the architecture is frozen; later AR stages prove/deliver the product inside it.

- **AR-12 — Fresh Rehearsal Environment:** operational proof that a clean environment can be bootstrapped from canonical inputs, deployed, smoke-tested, torn down and recreated without hidden manual state. Write source only for real gaps discovered by the rehearsal.
- **AR-13 — Rotation Rehearsal:** prove real key/secret/credential rotation using existing mechanisms. It is primarily an operational test, not a new subsystem.
- **AR-14 — Remote Recovery Rehearsal:** prove recovery on a clean/remote side from durable state/artifacts using existing backup/restore/release mechanisms. Fix only real gaps.
- **AR-15 — Windows Delivery Program:** the substantive late implementation stage. It owns the production-grade Profile Bridge/Camoufox updater/delivery chain: signed update contract, signature verification/key rotation, side-by-side staging, safe activation, health/LKG rollback, publisher integration, Windows negative matrix and production-equivalent rehearsal. It may use several bounded implementation PRs, but only the final governed AR-15 candidate accepts the slice.
- **AR-16 — Final Whole-project Audit:** audit-only. It must not become a cleanup/refactor bucket. A finding blocks -> small defect PR -> audit again.
- **AR-17 — Qualification / Production Core gate:** decision-only as far as practical. It consumes accepted evidence/state and may authorize `architecture_complete=true`, `production_core_gate=AUTHORIZED`, while `production_ready=false` and `production_mutation=false`. It must not invent another closeout engine.

## 8. CI efficiency without lowering assurance

Do not weaken protected branch requirements. Future CI optimization may make permanent required contexts applicability-aware only if it is fail-closed:

```text
definitely affected -> full proof
definitely irrelevant -> required context returns explicit fast-success reason
uncertain -> full proof
architecture/security/release acceptance candidate -> force full proof
```

An impact classifier must consider more than filename globs when dependency/contracts/effects/workflow/governance surfaces can change. This is an optimization mechanism, not permission to skip proof by default.

Repeated proof concepts such as predecessor reachability, caller=0, unique-current-invariant=0, generated-projection-not-authority and DEAD-predecessor absence should become reusable CI checks rather than new Markdown ownership/acceptance matrices per transaction.

## 9. Current interactive development environment

The current agent environment has the connected **GitHub plugin** and does not assume local `gh`.

Use the plugin for hosted GitHub reads/writes: branches, PRs, issues, reviews, required contexts, workflow/status observations, repository files and other hosted GitHub state. Do not block work on missing `gh`, shell-scrape GitHub, or move GitHub/network authority into `opsctl` as a workaround.

Repository-local verification remains local (`python scripts/verify-fast.py`, targeted Cargo/tests). GitHub Actions/provider workflows may use their own pinned hosted tooling. If an old runbook is touched and prescribes `gh`, update the runbook to describe the operation rather than making `gh` an architectural requirement.

## 10. Production roadmap

PC-1 is the first production release, not another architecture design stage. It promotes exact accepted bits under the Release / Capability Profile and enables only the Production Core: identity/users, clients/customer cards, browser profiles and bulk operations, client↔profile bindings, grants/access, generations/sessions/devices, required encrypted persistence/restore, real Camoufox, Windows Profile Bridge + AR-15 delivery/updater, and Core-required audit/health/readiness/observability/recovery foundations.

Mailbox administration, mailbox jobs/automation and outbound mail may remain source-present and tested on the same protected `main` while production-disabled.

```text
PC-1 Production Core v1
PC-2 Mailbox Administration
PC-3 Mailbox Jobs / Automation
PC-4 Outbound / later capabilities
```

## 11. Canonical references

- `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` — current program authority and immutable execution order;
- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md` — permanent architecture contract;
- `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md` — detailed #454/N2–N5 ownership retirement contract;
- #441 — live mutable execution state;
- #454 — sole current pre-N2 implementation transaction;
- `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` / #430 — PF-1;
- `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431 — PF-3;
- #399 / #421 — Functional Closure obligations;
- `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` — shared exact-head/merge acceptance discipline;
- `architecture/accepted-phases.json` + Git history — immutable accepted product-phase provenance.
