# Browser Profile Platform — Development Plan

**Document status:** GENERATED_PROJECTION  
**Canonical program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Live execution tracker:** #471
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
#454   ACCEPTED
N2–N5  ACCEPTED
PF-1   ACCEPTED (#466)
PF-2   CURRENT (#471)
PF-3   BLOCKED on PF-2
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

## 3–4. Accepted cutover history

#454, N2–N5 and PF-1 are accepted and closed. They retired current Release Set v2 semantics and the Python-estate, governance-overlay, operator-JSON, runtime-cutover, legacy lifecycle and tracked-inventory predecessor authorities without successor registries. Exact detail and provenance remain in #441, #430, #466 and their historical contracts; this current developer projection does not repeat or reopen them.

Their permanent anti-regression rule remains: one natural semantic owner; internal validators/workflows are cutover callers rather than durable-consumer proof; no tracked JSON projection as semantic input; no Python/Node compatibility tail; no provider/network/process authority in `opsctl`; no Product Runtime dependency on `opsctl`.

## 5. PF-1 → PF-3 — bounded replacement + provisional fitness

### PF-1 — lifecycle + bounded inventory

PF-1 is accepted via #466. Its permanent result is a typed pure lifecycle evaluator plus bounded natural-owner projections and a composition-only inventory compiler. `architecture/inventory.json`, legacy Node/Python policy and compatibility-only write/drift commands remain retired; generated output is never semantic input.

### PF-2 — minimal hosted evidence pipeline

PF-2 is CURRENT under #471. It is an evidence pipeline, not a universal provider/plugin framework:

```text
outer GitHub/provider observation
-> strict secret-free versioned DTO
-> typed pure EvidencePolicy
-> HostedEvidenceEnvelopeV1
-> canonical bytes/digest
-> immutable artifact/attestation
```

Add abstraction only after at least two concrete current consumers prove shared semantics. Network/provider/clock/credential reads and publication stay in GitHub Actions or official provider tooling, outside `opsctl`. `opsctl` accepts only strict secret-free observations; its pure `EvidencePolicy` must contain no provider-specific policy. Replace and delete any predecessor evidence-validity authority in the same transaction.

### PF-3 — small provisional enforcement baseline

PF-3 makes already-selected architecture guarantees permanent and machine-enforced. `FitnessRuleRegistry` (or equivalent) should be a small typed index such as:

```text
RuleId
requiredness
scope
primary enforcement owner
negative fixture
```

Reuse specialized validators/checkers as enforcement owners. Do not build a generic linter/plugin/DI platform. Machine checks enforce objective properties (dependency/effect/authority/compatibility/projection rules); PR Architecture Impact + protected review handles genuinely semantic questions that cannot be reliably inferred by a universal checker.

PF-3 is provisional: it prevents silent weakening and generic-framework growth, but the final architecture-form freeze occurs only after accepted AR-15 proves the real Windows delivery/updater/recovery scenarios. FC-6…AR-15 may make only the smallest correction required by a named failed product acceptance scenario; no open redesign bucket is allowed.

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

After PF-3 the provisional baseline is enforced; later AR stages prove/deliver the product and permit only the smallest correction required by a named failed scenario. AR-15 acceptance establishes the final architecture-form freeze.

- **AR-12 — Fresh Rehearsal Environment:** operational proof that a clean environment can be bootstrapped from canonical inputs, deployed, smoke-tested, torn down and recreated without hidden manual state. Write source only for real gaps discovered by the rehearsal.
- **AR-13 — Rotation Rehearsal:** prove real key/secret/credential rotation using existing mechanisms. It is primarily an operational test, not a new subsystem.
- **AR-14 — Remote Recovery Rehearsal:** prove recovery on a clean/remote side from durable state/artifacts using existing backup/restore/release mechanisms. Fix only real gaps.
- **AR-15 — Windows Delivery Program:** the substantive late implementation stage. It owns the production-grade Profile Bridge/Camoufox updater/delivery chain: signed update contract, signature verification/key rotation, side-by-side staging, safe activation, health/LKG rollback, publisher integration, Windows negative matrix and production-equivalent rehearsal. It may use several bounded implementation PRs, but only the final governed AR-15 candidate accepts the slice.
- **AR-16 — Final Whole-project Audit:** audit-only. It must not become a cleanup/refactor bucket. A finding blocks -> small defect PR -> audit again.
- **AR-17 — Qualification / Production Core gate:** decision-only as far as practical. It consumes accepted evidence/state and may authorize `architecture_complete=true`, `production_core_gate=AUTHORIZED`, while `production_ready=false` and `production_mutation=false`. It must not invent another closeout engine.

### 7.1 Seven binding product acceptance scenarios

The detailed contracts live in the canonical plan. The developer-facing map is:

```text
PAS-1 governed identity/access
PAS-2 client + browser-profile UI/API/bulk workflow
PAS-3 encrypted generation/persist/open/restore lifecycle
PAS-4 real Windows Profile Bridge + pinned Camoufox + updater/LKG
PAS-5 crash/timeout/duplicate/partial-failure recovery and observability
PAS-6 fresh same-bits staging delivery + rollback/recreate
PAS-7 production-core admission + later-capability fail-closed negatives
```

Every scenario includes its user-visible result, real UI/API/runtime route, data/external contracts, authorization and failure negatives, retry/idempotency behavior, observability, platform, measurable product-owned SLO and durable evidence.

```text
FC-6/FC-7 -> PAS-1,2,3,6,7
AR-12     -> PAS-1,2,3,6
AR-13     -> PAS-3,5,7
AR-14     -> PAS-3,5,6
AR-15     -> PAS-4,5,6 + final architecture-form freeze
AR-16     -> audit PAS-1..7
AR-17     -> authorize only with PAS-1..7 accepted
PC-1      -> promote/re-prove PAS-1..7 admission and observability
```

Passing validators without the assigned end-to-end scenario evidence does not complete a phase.

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

## 9. Hosted tooling boundary

Use an authenticated supported GitHub client available in the execution environment (`gh` or the connected GitHub API surface). Before mutation, verify viewer identity, repository, branch and exact head; fresh Git/GitHub state wins over prose. Do not shell-scrape GitHub or move GitHub/network authority into `opsctl` as a workaround.

Repository-local verification remains local (`python scripts/verify-fast.py`, targeted Cargo/tests). GitHub Actions/provider workflows use their pinned hosted tooling. Runbooks specify the operation and required authority, not one mandatory interactive client.

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
- #471 — live PF-2 execution state;
- #441 / #430 / #466 — accepted historical pre-PF-1 and PF-1 provenance;
- #454 — accepted Release Set v2 correction;
- `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` / #430 / #466 — accepted PF-1;
- `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431 — PF-3;
- #399 / #421 — Functional Closure obligations;
- `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` — shared exact-head/merge acceptance discipline;
- `architecture/accepted-phases.json` + Git history — immutable accepted product-phase provenance.
