# O0 — Operational Outcome Convergence

**Status:** proposed binding pre-V2 stage; becomes executable only after insertion into `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` is accepted on protected `main` and Issue #266 selects O0.

**Purpose:** make critical operational procedures self-explaining and mechanically composable without creating a second semantic owner, mutation owner, provider client, mutable status database, or generic orchestration framework.

## 1. Why O0 exists

TX-1..TX-7 industrialized the D1 migration control plane. In particular, TX-7 proved one standard D1 operator procedure with typed outcomes, exact authorization binding, sole-executor execution, receipt/post-verify and no ordinary log archaeology.

After V2 resumed, AR11 Release Set Promotion exposed a broader boundary defect: existing natural owners already return structured decisions, blockers and remediation, but workflow composition can redirect those results into temporary files and collapse the first non-zero condition into an opaque shell `exit 1`. The semantic core remains correct and fail-closed, yet the operational boundary loses the reason that the operator needs.

The defect is therefore not missing D1 policy and not missing promotion policy. It is **lossy owner -> orchestration -> operator transport**.

O0 closes that gap once, before V2 continues.

## 2. Architectural target

```text
one fact -> one natural owner
one operation -> one standard procedure
one owner verdict -> lossless terminal projection
one terminal operation -> one OperationalOutcome
one blocked operation -> one exact cause + one next action
```

The desired critical path is:

```text
provider/read-only observations
        +
existing typed semantic owners
        +
existing authorization/effect owners
        |
        v
thin orchestration/composition
        |
        v
one secret-free terminal OperationalOutcome
        |
        +-> human-readable Step Summary
        +-> immutable evidence refs/digests
        +-> exact next action
```

The outcome is a projection of authoritative results. It is never a policy engine and never decides domain semantics that belong to D1, Release, Promotion, Recovery, Capability, Identity or another natural owner.

## 3. Permanent invariant

For every **critical operational procedure** that participates in provider mutation authorization, provider mutation, recovery, release promotion, or candidate/environment acceptance:

> A typed or structured natural-owner verdict MUST survive unchanged in meaning through workflow/shell composition to one durable terminal operational outcome. An operator MUST NOT need raw log archaeology or manual cross-assembly of unrelated temporary artifacts to learn the first blocking owner, exact reason, provider-effect state and next action.

This invariant applies prospectively to new/touched critical procedures and to currently reachable critical procedures when O0 or a later bounded natural-owner transaction touches them. It is not permission for a repository-wide big-bang rewrite of unrelated CI.

## 4. Natural-owner boundaries

O0 preserves current owners.

```text
D1 semantics / GateResult / D1 operator outcomes     = existing opsctl D1 owner
Release compatibility / verification                 = existing release owner
Promotion plan / preflight / rollback compatibility  = existing promotion owner
Provider observations                                 = existing read-only provider adapters/workflows
Provider mutation                                     = existing sole mutation workflow for that effect
Authorization                                         = existing stage/transaction authorization owner
Recovery disposition                                  = existing recovery/receipt owner
Release/candidate identity                            = existing Release Set owner
OperationalOutcome composition                        = thin transport/projection only
```

O0 MUST NOT move provider credentials or provider mutation into `opsctl`, Python, a daemon, UI, or a new service. O0 MUST NOT duplicate existing blocker/reason-code semantics in YAML.

## 5. `OperationalOutcome` external contract

The exact implementation language/location is an implementation decision, but the terminal contract must be versioned, secret-free, deterministic for the same authoritative inputs, and sufficient for both a human and a machine to understand the terminal boundary.

Minimum semantic fields:

```text
schema_version
contract
procedure
status
phase
failed_gate                       # nullable on success
owner                             # authoritative semantic owner
owner_contract                    # exact owner output contract when available
owner_reason_code                 # copied/projected, never re-invented
summary
remediation
exact_next_action

source_sha                        # when source-bound
tree_sha                          # when source-bound
release_set_id                    # when release-bound
operation_id / transaction_id / promotion_id as applicable
target_identity                   # when target-bound
authorization_state

provider_mutation_started
provider_mutation_executed
production_mutation_executed
effect_state                      # EXACT_NO_EFFECT | EFFECT_VERIFIED | RECOVERY_REQUIRED | UNKNOWN etc. as mechanically supportable

evidence_refs
evidence_digests
```

The transport may carry owner-specific structured diagnostics under an `owner_diagnostic` field rather than flattening or translating them.

### 5.1 Status families

A common transport status may classify the operator boundary without replacing owner-specific semantic decisions. At minimum the composition must mechanically distinguish:

```text
READY / ACTION_REQUIRED
BLOCKED
FAILED_NO_EFFECT
RECOVERY_REQUIRED
COMPLETED
NOOP / REPLAY
INFRASTRUCTURE_FAILURE
```

Owner-specific statuses/reason codes remain authoritative inside their own contract.

### 5.2 Semantic failure vs infrastructure failure

If a natural owner produced a valid structured blocker, `OperationalOutcome` must carry that owner result losslessly.

If infrastructure fails before a semantic owner can produce a verdict, O0 MUST NOT invent a semantic root cause. It emits an infrastructure disposition with:

```text
failed boundary = known
owner verdict = unavailable
provider effect = exact true/false when proven, otherwise UNKNOWN
safe retry/remediation = derived only from existing fence/receipt/effect evidence
```

Unknown effect state is fail-closed.

## 6. Workflow composition rules

Critical workflows must follow these rules:

1. **Capture before assert.** A semantic command writes/captures its complete structured result before a pass/fail assertion can terminate the procedure.
2. **No silent semantic sink.** `jq -e ... >/dev/null`, shell `test`, pipeline exit status or similar may enforce a gate, but cannot be the only surviving representation of why a structured owner blocked.
3. **No YAML reason ownership.** YAML/shell may identify the orchestration phase/gate, but does not replace or rename an available owner reason code.
4. **First blocking boundary is explicit.** The terminal outcome identifies the first authoritative blocking owner/gate; downstream gates are not guessed or fabricated.
5. **Always terminalize.** A small outcome artifact and human-readable summary are emitted with `always()`/equivalent terminal handling whenever the runner can still execute cleanup/evidence steps.
6. **Success is equally typed.** READY, COMPLETED and NOOP are first-class outcomes; success must not require reconstructing state from multiple artifacts.
7. **Evidence remains append-only.** OperationalOutcome points to immutable evidence; it is not a mutable readiness/status database.
8. **Machine-owned values remain machine-owned.** No new manual copying of SHA, Release Set ID, digest, target identity, TransactionId, promotion ID or `expected_current`.

## 7. First implementation target: AR11 Release Set Promotion

AR11 is the first mandatory proof because it exposed the current defect and is on the V2 critical path.

Current composition contains distinct authoritative boundaries such as:

```text
accepted source / immutable Release Set verification
provider observation / deployment snapshot
Catalog D1 compatibility
release compatibility
promotion plan
promotion preflight / rollback compatibility
quiescence / exact current-state fence
authorization binding
provider effect
post-verify / receipt
```

O0 does **not** merge these semantics into one new policy engine.

The implementation must make the existing AR11 procedure produce exactly one terminal outcome whose `owner` and `owner_reason_code` point to the natural owner that actually admitted or blocked progression.

### 7.1 AR11 prepare/read-only path

The automatic/read-only path must:

```text
resolve exact accepted main + immutable target Release Set
-> collect existing read-only provider facts
-> run existing compatibility/plan/preflight owners
-> preserve every structured owner result
-> build one terminal outcome
-> publish evidence + Step Summary
-> if ready, expose the existing READ_ONLY_READY authorization boundary
-> if blocked, expose exact owner reason/remediation
```

No provider mutation is authorized by this path.

### 7.2 AR11 mutation path

O0 must not broaden AR11 mutation authority. Existing explicit one-shot authorization, exact-current fence, effect scope, mutation owner and post-verify remain binding.

The only O0 requirement on the mutation path is that terminal success/failure/recovery/no-effect outcomes remain equally self-explaining and evidence-bound.

## 8. Verification matrix

O0 acceptance requires positive and negative proof at the orchestration boundary, not just unit tests of the owners.

Required credential-free/fixture negatives for AR11:

```text
D1 compatibility BLOCKED
release compatibility BLOCKED
promotion plan BLOCKED/NO_CHANGE as applicable
promotion preflight BLOCKED
rollback compatibility BLOCKED/UNKNOWN
malformed/missing owner output
infrastructure failure before owner verdict
```

For every case prove:

```text
exact terminal phase/gate
correct natural owner
owner reason preserved when one exists
no fabricated semantic cause when one does not exist
provider mutation = false for read-only/negative fixtures
one deterministic remediation/next-action boundary
one terminal outcome artifact
```

Required positive fixture/proof:

```text
all owners READY
-> one READ_ONLY_READY terminal outcome
-> existing authorization boundary unchanged
```

Required hosted proof on accepted main:

- automatic AR11 read-only execution produces one terminal OperationalOutcome even when the real target is semantically BLOCKED;
- if real target is READY, it reaches the existing explicit authorization boundary and performs no provider mutation;
- if real target is BLOCKED, the outcome alone is sufficient to identify the exact next V2 concern without reading raw logs or reconstructing temporary JSON.

O0 does **not** require manufacturing a provider write merely to prove a success state.

## 9. Repository-wide adoption rule

O0 is a bounded convergence stage, not a universal workflow rewrite.

During O0:

1. define/land the permanent owner-to-orchestrator losslessness rule in its natural repository architecture/execution contract;
2. converge AR11 fully because it is the current reachable failure;
3. inspect other **currently reachable critical** authorization/mutation/recovery/acceptance workflows only to classify them as:
   - already lossless;
   - requires a bounded O0 cut because the same defect is reachable before V2/R1-R3;
   - not currently reachable / future owner-local adoption;
4. change only those whose missing outcome is a real current operational risk;
5. do not retrofit ordinary build/test CI that has no authorization/effect/recovery/acceptance responsibility.

Future development rule: when a new or modified critical operational procedure composes a structured natural owner, its PR must preserve that verdict through the terminal operator boundary. Adding a new product capability should therefore add/extend its natural semantic owner, while the operational composition pattern remains stable.

## 10. Negative complexity budget

O0 is accepted only if the architecture gets simpler to operate and no parallel authority is introduced.

```text
new semantic policy owner                         = 0
new provider client                               = 0
new provider mutation owner                       = 0
new mutable readiness/status database             = 0
new global registry/authority bag                 = 0
new generic workflow/orchestration framework      = 0
new checker-for-checker                           = 0
duplicated owner reason-code taxonomy in YAML     = 0
new manual machine-known operator inputs          = 0
new Production authorization                      = 0
```

A small shared serialization/terminalization helper is allowed only if it is clearly a mechanical projection utility with no semantic admission decisions and demonstrably removes duplicated shell/YAML plumbing. Prefer extension of an existing natural typed boundary over a new helper when practical.

## 11. Simplification ledger

Every O0 implementation PR must report:

```text
opaque critical failure boundaries before / after
manual artifact/log joins before / after
semantic reason translations/copies removed
inline shell/YAML policy removed
new files/modules/helpers added and their single consumer/owner rationale
predecessor paths deleted or retirement condition
```

The target is not fewer files at any cost. The target is lower semantic/execution ambiguity and a stable layered dependency direction.

## 12. O0 Definition of Done

O0 is complete only when all of the following are accepted on protected `main`:

1. one permanent repository rule requires lossless natural-owner verdict propagation for critical operational procedures;
2. AR11 has one standard terminal `OperationalOutcome` for READY, BLOCKED and terminal effect/recovery/no-effect boundaries;
3. the current class of failure demonstrated by AR11 can be diagnosed from one outcome without raw log archaeology or manual joining of temporary artifacts;
4. the outcome preserves exact existing owner reason/remediation when available and never fabricates a semantic reason for infrastructure failure;
5. failure-path evidence is published even when a semantic gate blocks before READY, whenever the runner remains able to terminalize;
6. existing D1, Release, Promotion, authorization, provider observation, mutation and recovery natural owners remain authoritative; no semantic logic is copied into the outcome layer;
7. existing provider-effect authorization/fencing/replay/post-verify rules are unchanged or strengthened, never weakened;
8. required AR11 negative fixtures cover every composed semantic boundary and prove `provider_mutation=false` on read-only failures;
9. one positive credential-free/hosted-safe proof establishes the READ_ONLY_READY shape and existing authorization stop boundary without manufacturing an unauthorized provider effect;
10. currently reachable pre-V2/R1 critical workflows are classified, and any same-class reachable opaque failure that would force immediate archaeology is either corrected in O0 or recorded as not reachable with a concrete trigger;
11. simplification ledger proves zero new semantic/mutation owners and no new manual machine-owned inputs;
12. exact-head permanent CI is green, protected required contexts are green, behind-by is zero, reviews/threads are clear, guarded merge is bound to the proven head, and post-merge accepted-main evidence is recorded;
13. O0 performs no Production mutation or authorization;
14. O0 itself performs no staging/provider mutation unless a separate exact operation is explicitly authorized by the owning stage; ordinary O0 acceptance needs only read-only/credential-free evidence;
15. after closure, Issue #266 returns to V2 and V2 resumes from one exact terminal AR11 outcome instead of a reconstructed diagnostic state.

## 13. Capability/profile impact

```text
CAPABILITY_LIFECYCLE_IMPACT = NONE
CORE_PROFILE_CHANGE = NONE
EFFECTIVE_SET_CHANGE = NONE
PRODUCT_RUNTIME_SEMANTICS_CHANGE = NONE
```

O0 changes engineering/operational composition only. Any implementation that requires enabling/disabling a product capability, changing the first-release effective set, or adding product runtime dependency on operations tooling is out of scope and must fail the stage scope check.

## 14. Authorization and effects

```text
REPOSITORY_SOURCE_GOVERNANCE_CHANGES = ALLOWED through ordinary PR/CI/guarded merge
READ_ONLY_PROVIDER_OBSERVATION = ALLOWED only through existing read-only owner where already permitted
PROVIDER_MUTATION_AUTHORIZED_BY_O0 = NO
D1_MUTATION_AUTHORIZED_BY_O0 = NO
WORKER_PROMOTION_AUTHORIZED_BY_O0 = NO
ACCESS_MTLS_MUTATION_AUTHORIZED_BY_O0 = NO
PRODUCTION_AUTHORIZED = NO
```

A generic instruction to continue O0 is not provider mutation authorization.

## 15. Return to V2

O0 is a prerequisite insertion, not a replacement roadmap and not a new long-running program.

Binding transition after O0 acceptance:

```text
TX-7 accepted provenance
-> O0 Operational Outcome Convergence
-> O0 accepted
-> #266 selects V2 again
-> fresh V2 baseline / current immutable Release Set
-> automatic AR11 yields one exact terminal outcome
-> follow that outcome's natural-owner next action
```

Repository/source changes made by O0 naturally create a new immutable Release Set and invalidate stale candidate-bound V2 evidence as required by the existing exact-identity model. O0 must not attempt to preserve a stale ReleaseCandidateId through compatibility hacks.

O0 closes once the operational boundary is industrialized. Any genuine product/provider blocker revealed by the new outcome returns to its existing V2 natural owner rather than expanding O0.