# Architecture Re-baseline v3 — CAP Execution Program

**Document status:** CURRENT TEMPORARY EXECUTION AUTHORITY

**Program tracker:** [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266)

**Research provenance:** [CAP-INDEX #505](https://github.com/iamaman11/part-crm-emai-profile/issues/505)

**Production authorization:** NOT GRANTED

This is the single repository document that owns the ordered implementation program produced by the
completed CAP research. Issue #266 owns the live stage pointer and accepted-main evidence. This
document deliberately contains no moving SHA, workflow count, provider observation, readiness result or
environment state.

Chat history, handoff text, a stale branch and historical plans never authorize work. An agent must read
fresh protected `main`, Issue #266 and the one CURRENT stage Issue before starting a transaction.

## 1. Permanent authorities

This temporary plan orders work; it does not redefine stable product or architecture meaning.

- [`PRODUCT.md`](PRODUCT.md) — product boundary and accepted first-release scenario;
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — current system topology and ownership;
- [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) — mandatory architecture invariants;
- [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — anti-weakening and simplification rules;
- [`ARCHITECTURE_ACCEPTANCE_PROTOCOL.md`](ARCHITECTURE_ACCEPTANCE_PROTOCOL.md) — exact-candidate acceptance;
- [`CONTRACT_POLICY.md`](CONTRACT_POLICY.md) — public contract evolution;
- [`OPSCTL_ARCHITECTURE_BOUNDARY.md`](OPSCTL_ARCHITECTURE_BOUNDARY.md) and [`PYTHON_USAGE_BOUNDARY.md`](PYTHON_USAGE_BOUNDARY.md) — tooling/effect boundaries.

CAP Issues are decision provenance. CAP-INDEX is research navigation, not runtime, release or execution
authority.

The legacy `architecture/architecture-program-sequence.json` and its acceptance evaluator describe the
accepted AR-0…AR-17 architecture-program state machine. They do not contain this CAP execution order,
do not select a CAP transaction and cannot override #266. Their executable consumer/lifecycle cleanup
requires a separate CAP-05/CAP-06 transaction and is not hidden inside E0.

## 2. Accepted operating model

```text
preserve essential natural owners
+ correct bounded ownership/admission defects
+ implement the accepted product vertical slice
+ retire proven obsolete/duplicate authority
+ verify at the cheapest sufficient risk tier
= controlled complexity and bounded change cost
```

Binding rules:

1. Preserve the accepted bounded-context and domain -> application/port -> adapter -> composition layering.
2. One semantic responsibility has one natural owner; generated projections are not second owners.
3. Unknown future capabilities are not first-release blockers.
4. `source_present != production_enabled`; backend admission remains fail-closed before side effects.
5. Every migration cuts over callers and deletes its predecessor in the same transaction, or records a
   concrete owner, consumer and retirement condition.
6. Verification protects objective current invariants at the narrowest sufficient lifecycle. Existing
   checks are not retained merely because another checker or registry references them.
7. Do not create a global registry, checker-for-checker, plugin framework, universal policy service or
   another standing architecture program.
8. Production is authorized only for one exact release candidate, one target-specific deployment
   authorization envelope and one enabled capability set by the CAP-08 model.

## 3. Binding execution order

The table below is the accepted top-level sequence. Issue #266 may also select a newly justified named
bounded stage (for example `TX-6` or `M2`) when fresh evidence proves that the stage has its own
objective, entry/exit criteria and DoD and is required before the next table row can proceed. Such a
stage is a peer executable stage, not a child of a simultaneously active "program" Issue.

Every executable stage selected by #266 has exactly one CURRENT owning Issue while it is active. A
later stage cannot start merely because its code is easy or an earlier branch is green.

| Order | Transaction | Natural owner and required outcome |
|---|---|---|
| E0 | Documentation authority cutover | CAP-06: current entrypoints converge on this sequence; stale PAS/FC current-state claims become history. |
| E1 | CAP-01 runtime-surface correction | `capability-policy` + Worker ingress: Queue/Scheduled consume canonical `RuntimeSurface -> ActivationUnit`; health membership/bypass is explicit and tested. |
| D0 | Documentation/setup authority convergence | CAP-06 natural owners: remove copied mutable order and stale authority/scope/setup claims; historical/projection material fails closed to current use. No runtime or hosted-setting mutation. |
| E2 | Release/promotion strict JSON | CAP04-T1 adapters: duplicate-member/bounded admission precedes typed decode on current release/promotion inputs. |
| E3 | D1 strict JSON | CAP04-T2 adapters: current D1 inputs use the same strict outer admission without changing provider/migration ownership. |
| E4 | First CAP-05 retirement and scripting-governance cut | Retire the first independently proven completed transition/duplicate authority bundle; preserve its objective current invariant and passive provenance. Establish the prospective owner/effect/consumer/retirement rule for touched or new repository Node/MJS entrypoints without a per-file registry. |
| C0 | First-release capability convergence | CAP-01/release owners: the selected Core profile and every reachable ingress include only the CAP-12 slice and required dependencies; Mailboxes, Notifications and Automation remain disabled; profile/effective-set identity is release-bound. |
| P1 | CAP12-I1 Client/Profile relationship | Client card shows authorized attached Profiles; attach, detach and atomic reassign use the existing assignment owner; relation never grants access. |
| P2 | CAP12-I2 authorized launch/Bridge | One server-authorized shipping path composes device trust, claim, lease/fence, workspace lock and pinned real Camoufox; replaced launch path is deleted. |
| P3 | CAP12-I3 confirmed save/reopen | Controlled close commits an exactly verified encrypted successor before `Saved`; failure preserves the last confirmed generation; reopen uses authoritative active generation. |
| A0 | Environment authorization wiring | Existing capability/release admission consumes a verified target authorization observation instead of a hard-coded `NotAuthorized`; rehearsal/staging may be admitted only by their exact envelope and Production stays fail-closed until R3. No second authorization owner. |
| S0 | Windows shipping/recovery closure | Windows release owner: immutable Bridge/runtime artifact identity, trusted signing/distribution, SBOM/provenance, updater compatibility, rollback and clean-host recovery are candidate-bound and proven. No alternate launcher/updater. |
| V1 | Release-facing verification and quality convergence | Apply accepted CAP-05 risk tiers to the exact reachable release surface; give every CAP-05 finding an evidenced disposition, resolve every `UNKNOWN`, and retire/narrow only proven duplicate orchestration without losing an invariant. |
| V2 | CAP12-I4 exact scenario acceptance | On one immutable non-Production release identity prove B1–B10, including the positive path and negative, replay, concurrency, recovery, hosted identity and exact-environment evidence. |
| R1 | Exact candidate and evidence | Freeze the release candidate identity and instantiate the CAP-08 target envelope: source/artifacts, migrations/contracts, target/config, capability digest/effective set, evidence, risks and named authorities. |
| R2 | Pilot readiness package | Without Production mutation, bind cohort, blast radius, stop, rollback/recovery and expansion conditions to the unchanged release candidate and Production target envelope. Mandatory security/data guarantees cannot be waived. |
| R3 | Production Authorization and controlled activation | A named authority first issues GO/PILOT or NO-GO for the exact release candidate + target envelope. Only GO/PILOT permits the protected bounded activation and observation described by that decision; NO-GO performs no Production mutation. |

The CURRENT stage Issue must refine only its selected stage without widening it. If fresh evidence
invalidates a prerequisite, stop that stage and record the exact blocker in Issue #266; do not skip
ahead or invent an implicit parallel phase.

## 4. Transaction protocol

```text
fresh protected-main + GitHub re-baseline
-> #266 selects exactly one CURRENT stage and one owning Issue
-> identify one bounded concern, natural owner, effects, contracts, consumers and predecessor
-> declare the coherent write-set/invariants and open one bounded mutation window
-> implement the smallest coherent change
-> cut over callers and remove the replaced path
-> inspect full diff and simplification ledger
-> run targeted positive + negative proof
-> atomic commit and exact-head applicable CI
-> required contexts green; behind_by = 0; reviews/threads clear
-> FRESH PRE-MERGE RE-BASELINE
-> guarded merge bound to the proven head
-> FRESH POST-MERGE protected-main reread
-> record accepted evidence in the CURRENT stage Issue and update #266
-> if stage DoD remains open, continue with its next bounded concern
-> if stage DoD is satisfied, close it and only then select the next stage
```

A fresh re-baseline opens one bounded mutation window. Expected self-authored writes inside the declared
coherent write-set do not recursively restart discovery. Re-baseline only when the window is invalidated
by real unexpected drift/scope/authority expansion or at the explicit pre-merge, post-merge and provider-
effect boundaries defined by root `AGENTS.md`.

Every implementation stage Issue must state:

- accepted finding or product obligation;
- natural owner and exact affected surface;
- capability lifecycle/profile impact (`ADD`, `ENABLE`, `DISABLE`, `REMOVE` or evidenced `NONE`),
  including the selected profile/effective-set and current-selector disposition;
- consumers and bounded blast radius;
- predecessor deletion or evidenced retirement condition;
- minimum tests/evidence and execution tier;
- exit criteria/DoD, failure/recovery behavior and explicit non-goals;
- whether external, governance or Production mutation is authorized.

### 4.1 CURRENT stage Issue lifecycle

Issue #266 is the sole mutable execution pointer and sits above all executable stages. Exactly one stage
may be CURRENT at a time, and that stage has exactly one owning GitHub Issue. Named stages such as
`TX-6`, `V2` and `M2` are peers when selected by #266; they do not require an additional simultaneously
active program Issue above or below them.

Create a stage Issue only after a fresh protected-main/GitHub re-baseline confirms that #266 selected
that stage as CURRENT. Do not pre-create Issues for later stages: an unstarted future Issue looks like
competing current work and becomes stale before its prerequisites are known.

```text
accepted-main reread + fresh GitHub re-baseline
-> #266 selects one named stage as CURRENT (discovery only; no source/provider mutation yet)
-> create or identify exactly one owning Issue for that stage
-> link #266 <-> CURRENT stage Issue and record the exact accepted base SHA
-> complete the mandatory change envelope, DoD, consumers, blockers and acceptance plan
-> execute one or more bounded implementation transactions/PRs while the same stage DoD remains open
-> after each merge, reread protected main and record exact accepted evidence in the same stage Issue
-> when stage DoD is satisfied, update #266: <STAGE> COMPLETE + accepted evidence + next permitted stage
-> close the completed stage Issue as durable provenance
-> only then create/select the next stage Issue
```

The CURRENT stage Issue is bounded working memory and evidence for one stage. It never becomes a second
roadmap, stable product/architecture contract, release selector or live pointer. Permanent decisions
must land in Git at their natural documentation/code/test owner. Historical parent/precursor Issues may
remain linked for provenance or aggregate history, but they do not duplicate the CURRENT snapshot and
do not remain a second current-stage owner. If discovery blocks or supersedes the stage, the same Issue
records the exact blocker/disposition and #266 is updated before any different stage is considered.

A second CURRENT Issue for the same stage is forbidden. A future-stage Issue is forbidden before #266
selects it. Multiple PRs are allowed only when they are bounded implementation units of the same stage
objective/DoD; a new independent objective/DoD or authorization/acceptance boundary is a new stage and
requires a #266 transition.

### 4.2 Provider-effect authorization boundary

Source/PR/CI acceptance never authorizes an external provider effect. Any rehearsal, deploy, D1
mutation, recovery mutation or Production mutation is a separate provider-effect mutation window.
Before the first provider write, the CURRENT stage Issue must contain or point to an explicit exact
transaction-scoped authorization that binds the immutable operation being permitted.

At minimum the authorization binds:

```text
exact target identity
exact accepted source / prepared transaction identity
exact effect scope
required provider pre-state / observation identity
expiry or freshness rule when applicable
fence / recovery boundary when applicable
```

The authorization is one-shot for that exact transaction/attempt unless the stage contract explicitly
states otherwise. Any material change to target, source, transaction ID/plan, observed pre-state, effect
scope, authority or freshness invalidates it. Re-observe/re-prepare and obtain a new exact authorization
before any provider write. Green CI means the implementation is accepted; it does not mean the real
provider mutation is authorized.

## 5. Gates and stop rules

- D0, E0–E4 and C0 authorize no provider, staging or Production mutation. A0 may wire and prove
  rehearsal/staging authorization only through its owning Issue; it never authorizes Production.
- P1–P3 implement only the accepted CAP-12 first-release scenario; Mailboxes, Notifications,
  Automation, Yahoo/new providers, tenant-wide Audit, global Sessions UI, complex roles, mobile parity
  and generic export are non-goals.
- `CODE_COMPLETE != SCENARIO_COMPLETE != PRODUCTION_AUTHORIZED`.
- A new feature blocks the first release only when it is required for an accepted scenario step,
  prevents a proven security/data-loss failure, or satisfies an accepted legal/contractual obligation
  for the exact candidate.
- A failed proof permits only the smallest correction required by the named scenario/invariant.
- Mutable provider/environment/readiness evidence is observed fresh in its owning R-stage; prose cannot
  promote readiness.

### 5.1 Exact identity across V2–R3

CAP-08 candidate admission has two explicit scopes:

```text
ReleaseCandidateId
  = exact source/tree + immutable artifacts + migrations/contracts
    + selected capability profile/effective-set digest

DeploymentAuthorizationEnvelope
  = ReleaseCandidateId + exact target environment + non-secret configuration/binding identity
    + target observations/evidence + risks + decision authority
```

V2 proves the accepted scenario on a non-Production envelope. R1–R3 must retain the same
`ReleaseCandidateId`; staging and Production envelopes are target-specific and are never falsely
declared byte-identical. A source, artifact, migration/contract or effective-set change invalidates V2
and repeats all affected evidence. A target/config change invalidates the affected target envelope and
repeats its applicable evidence. This prevents `tested A -> authorized B` without pretending staging
configuration is Production configuration.

### 5.2 CAP-12 B1–B10 acceptance matrix

V2 cannot close until one evidence package maps every accepted blocker to its natural proof:

| ID | Required result |
|---|---|
| B1 | Client card has an inverse read of independently authorized attached Profiles. |
| B2 | Standalone detach and atomic reassign preserve the existing relationship owner. |
| B3 | Client card calls a public server-authorized Launch operation. |
| B4 | One shipping Bridge path composes device trust, claim, lease/fence, lock and pinned real runtime. |
| B5 | Controlled close finalizes a verified successor and reports `Saved` only after authoritative commit. |
| B6 | Reopen uses the authoritative active verified generation. |
| B7 | Positive user E2E passes on the exact release identity. |
| B8 | Negative security/concurrency/replay/recovery E2E uses the same effect path and release identity. |
| B9 | Hosted managed login, logout, recovery and application membership-revocation behavior are proven. |
| B10 | Exact-environment backup, recovery and edge evidence required by the promised guarantees is proven. |

B1–B6 are product/runtime closure, B7–B10 are candidate evidence. A passing subset does not imply
`SCENARIO_COMPLETE`.

### 5.3 CAP-05, repository scripting and complexity closure

E4 is the first implementation cut, not a declaration that all verification debt is complete. Its
owning Issue must start with the smallest high-confidence CAP-05 bundle: completed AR-11
Actions-registration retirement machinery and its predecessor-only callers/manifests/self-tests. E4
must preserve the generic current GitHub structural invariant in its natural governance proof and
retain historical evidence only as passive provenance.

Every touched or new repository Node/MJS entrypoint follows the same prospective ownership discipline
as other effectful repository scripting without creating a global language owner or a hand-maintained
per-file estate registry. Its owning Issue/PR must prove:

```text
one natural semantic/execution owner
+ exact allowed and forbidden effects
+ named current consumer
+ one objective invariant
+ versioned input/output contract when durable or cross-boundary
+ positive and negative/fail-closed proof
+ explicit provider/secret/mutation authority
+ predecessor cutover or retirement trigger
```

Language is not the owner. GitHub-governance MJS remains owned by GitHub governance, release MJS by the
release boundary, D1 MJS by D1 evolution and runtime scripting by its runtime/composition owner. A new
entrypoint with no named owner/consumer or an effect outside its owning stage fails closed. Do not
replace retired Node/MJS/Python/JSON authority with an equivalent successor registry in another format.

V1 cannot close with a qualitative statement that CI is simpler. It must publish an exact CAP-05
disposition matrix covering every accepted finding and candidate from CAP-05 #496:

```text
RETIRED
  -> callers = 0
  -> unique current invariants = 0 or re-homed in one surviving primary proof

KEEP
  -> named current/durable consumer
  -> concrete risk
  -> natural semantic owner
  -> cheapest sufficient primary proof and retirement trigger

CONDITIONAL
  -> named owner
  -> missing proof/trigger
  -> non-blocking or blocking effect stated explicitly
  -> exact retirement condition
```

V1 exit criteria:

1. every accepted CAP-05 finding is `RETIRED`, evidenced `KEEP` or evidenced `CONDITIONAL`;
2. unresolved `UNKNOWN` findings are `0`, including `apps/foundation-check` caller ownership and the
   independent standalone resolver-artifact consumer question;
3. checker-for-checker and circular lifecycle retention are `0`;
4. required contexts without a named current risk, objective invariant and primary proof are `0`;
5. completed AR-8/AR-11 transition checks, live Release Set v2 compatibility, fixed workflow-count
   authority, Resolver/FC-6 transition paths and aggregate orchestration each receive an exact
   disposition rather than age-based deletion or permanent-by-existence retention;
6. duplicate setup/build/test orchestration is removed or narrowed only after unique proofs are
   preserved at the cheapest sufficient tier;
7. before/after workflow, required-context, whole-file/byte and observed-duration evidence is recorded
   without inventing unavailable p95/flakiness claims;
8. any required-context or branch-protection change is independently justified, applied through its
   authorized governance owner and reread from hosted state; green CI is never obtained by weakening an
   invariant;
9. current `opsctl`, Python and Node/MJS entrypoints on the exact reachable release surface conform to
   their natural owner/effect boundaries; unsupported projections/reports retire only after zero
   consumer and zero unique-invariant proof;
10. the permanent future-check standard remains one objective invariant -> one primary proof -> the
    cheapest sufficient lifecycle tier, with positive and negative proof and a retirement condition.
11. the permanent add/enable/disable/remove capability lifecycle is reflected in the bounded change
    protocol and its natural existing admission/owner checks; no generic feature registry or new
    required context is created merely to restate it.
12. the existing architecture-check family classifies every workspace `crates/*` package as a governed
    pure/domain/application package or an explicit outer adapter and rejects an unclassified addition;
    canonical `capability-policy` receives the same generic pure-layer coverage. This closes the current
    skip-on-unknown-crate gap without introducing a semantic registry or forcing one crate per feature.

V1 also gives the remaining transitional authority surfaces an exact executable-lifecycle verdict; it
does not silently equate `CONDITIONAL` with permanent. In particular, `docs/status.json`, the frozen
AR-program sequence/evaluator and their current callers are each included in the matrix. If a bounded
surface reaches `current callers = 0` and `unique current invariants = 0`, it is deleted/demoted in the
same transaction. Otherwise `KEEP`/`CONDITIONAL` names the durable consumer or missing proof, natural
owner and one observable retirement trigger. A generated projection, self-test, historical document or
checker that exists only to consume the predecessor is part of the deletion bundle, not retention
evidence.

Quality accountability is explicit rather than implied by tool count. The implementation author owns
the complete diff and simplification ledger; the natural semantic owner owns the invariant; protected
CI owns repeatable executable proof; the repository maintainer owns guarded acceptance. If a second
qualified maintainer exists before release qualification, CODEOWNERS/approval policy may be enabled for
critical release, security, migration and runtime surfaces through an authorized governance change.
Solo operation must not manufacture independence through self-approval. R1 records the actual named
release/security/risk authorities and any required independent evidence for the exact candidate.

### 5.4 External security, product/legal and privacy prerequisites

These are inputs to the existing V2/R1/R3 gates, not a new architecture phase or a license/privacy
policy invented by repository automation.

1. **Legacy credential incident — hard gate.** Issue
   [#1](https://github.com/iamaman11/part-crm-emai-profile/issues/1) remains a Production and
   prototype-reuse blocker. R1 cannot finalize and R3 cannot issue GO/PILOT until provider-side
   evidence proves the old credential was revoked/rotated and rejected, available usage/access logs and
   incident scope were reviewed, the replacement is only in an approved secret store, and repository
   scanning proves no reuse. The accepted record uses the existing `legacy_credential_rotation`
   external-evidence owner and secret-free references/digests; raw credentials or logs never enter Git.
2. **Privacy/retention applicability — exact target input.** Before V2 evidence is finalized, the
   product/legal/privacy authority records the target jurisdictions/regions, user and customer model,
   contractual promises and applicable retention, correction, revoke, delete, export, support-access
   and subprocessor obligations. V2/R1 map those inputs to the exact enabled capability set and every
   reachable copy. An unresolved applicable obligation blocks R1/R3; a newly discovered obligation
   invalidates and repeats every affected candidate proof rather than being waived by a narrow UI
   scope.
3. **Distribution authorization — separate from runtime readiness.** R1 records whether the target is
   private/internal use, bounded pilot or public/commercial distribution. Before any public/commercial
   distribution, the product/legal owner must accept a compatible repository/product license or other
   documented legal regime, third-party notices and redistribution restrictions. Missing authorization
   forbids distribution even when runtime evidence is otherwise complete. The current typed external
   readiness policy also requires `product_license`; changing that applicability requires a separate
   reviewed policy transaction, never a synthetic passing record.

Repository-local validation proves only the integrity of sanitized evidence. It cannot perform the
provider incident response, determine applicable law, accept a contract or grant distribution rights.

## 6. Non-blocking owner-local convergence

The following work may be performed only as separately authorized natural-owner transactions and must
not become a second pre-Production program. They may run before or after R3 only when dependency-
independent from the active stage. Reachable release/security/data obligations still block through V2/R1;
the word `non-blocking` applies only to debt outside the exact release surface.

### 6.1 Verification and executable-authority retirement

- E4 retires the first proven CAP-05 bundle; V1 classifies the complete accepted CAP-05 estate. After
  V1, every newly proven obsolete bundle is removed by a bounded natural-owner transaction until
  `proven obsolete executable verification bundles = 0`. A conditional item survives only while its
  named consumer/missing proof is real and is re-evaluated when its recorded trigger occurs.
- `docs/status.json` is not deleted by date or aspiration. Its remaining callers are cut over and the
  projection plus predecessor-only checks/docs are deleted in the first transaction that proves zero
  durable callers and zero unique current invariants.
- The frozen AR-0…AR-17 sequence/evaluator follows the same rule. Passive Git/Issue/evidence provenance
  may remain; executable current-looking machinery does not remain merely to reproduce history.
- CAP-07 contract-floor wording and recovery language are reconciled owner-locally: preserve immutable
  baseline obligations, avoid freezing the whole living API, and name `rollback | roll-forward |
  restore | compensate | abort-before-side-effect` per affected boundary. Compatibility/publication
  paths retire only after current-consumer and persisted/external-obligation proof.

### 6.2 `opsctl` command-family convergence

`opsctl` remains one bounded read-only tool; completeness does not mean moving every function into
`opsctl-core` or splitting the binary. Each touched command family applies the permanent boundary and
CAP-04 disposition:

- keep consumed D1/release/promotion/evidence families while their named risk and consumer remain;
- perform the final consumer/durable-obligation proof for credentials `status`/`rotation-plan`, public
  `d1 repository` and `release inspect`, then delete unsupported commands/projections and their
  predecessor-only tests/docs;
- decide hosted credential `seal/verify` retention only from the real evidence consumer;
- keep recovery/readiness namespaces absent until a separate product/consumer decision;
- extract pure D1/promotion semantics into `opsctl-core` only when the family is touched and physical
  separation materially reduces effect/dependency ambiguity.

No command-by-command cleanup is a release blocker unless the command is reachable from the exact
candidate or violates the provider/network/process/secret/mutation boundary.

### 6.3 Documentation and GitHub issue hygiene

- Remaining CAP-06 cleanup converges toward `current-looking duplicate authority = 0`, not toward
  deleting useful history or deduplicating every repeated term. A historical document survives only as
  indexed bounded knowledge/provenance; otherwise it is demoted/removed when touched.
- Before Issue #266 closes, Issues #3, #171, #203, #246, #399 and #421 receive a fresh disposition.
  Close them as superseded only after each unresolved current obligation is transferred to its natural
  current owner; keep one open only when it still owns a named blocker and link that fact from #266.
  An `OPEN` historical tracker may not remain an accidental alternate roadmap.
- Completed CAP research Issues are closed as research provenance after accepted permanent decisions
  and every unresolved implementation/external obligation has a current owner/link. Closing research
  never closes an unimplemented finding and never replaces Issue #1 or another real gate with prose.

### 6.4 Future capability admission

The first release is not required to complete disabled Mailboxes, Notifications, Automation, outbound,
export or future CRM behavior. Before any later capability profile enables one of them, its bounded
owner transaction must prove, for that newly reachable surface:

```text
data owner + purpose + copies
+ retention/delete/revoke/export/recovery obligations
+ retry/idempotency/concurrency/failure semantics for each effectful operation
+ positive and negative exact-path evidence
+ capability admission and retirement/recovery conditions
```

This closes CAP-09/CAP-10 incrementally without a central data platform, universal lifecycle registry,
generic saga/idempotency engine or speculative implementation for disabled modules.

## 7. Program completion

R3 is the first-release authorization boundary, not a demand to finish unrelated disabled-capability
work or every non-blocking repository cleanup first. A valid GO/PILOT may therefore occur before this
temporary CAP program is administratively closed. After R3, only the finite section 6 convergence
transactions remain under #266; they do not retroactively block the released exact candidate unless a
finding is reachable from it or invalidates its accepted evidence.

The temporary program closes only when:

1. E0–V2 have accepted evidence on protected `main`;
2. one exact candidate envelope is complete and all universal/reachable guarantees are decided;
3. R2/R3 record the named authorization outcome;
4. no temporary execution owner is still needed for ordinary feature development;
5. permanent decisions live in their natural docs/code/checks, all historical/CAP trackers have the
   dispositions required by section 6.3;
6. every accepted real finding is either `RETIRED/CLOSED` with evidence, `KEEP_NOT_A_DEFECT` with a
   durable consumer/risk and permanent natural owner, or `FUTURE_CAPABILITY_GATED` by section 6.4;
7. unresolved `UNKNOWN`, untriggered indefinite cleanup and `CONDITIONAL` debt are `0`; a conditional
   item is proved current and reclassified `KEEP_NOT_A_DEFECT`, triggered and closed, or remains an
   explicit blocker rather than disappearing into a backlog;
8. there is no second roadmap or generic debt registry;
9. #266 is closed as provenance only after criteria 1–8 hold.

Until then, Issue #266 is the only live stage pointer. Never copy its mutable state into README,
AGENTS, projections or handoff documents.
