# Architecture Re-baseline v3 — CAP Execution Program

**Document status:** CURRENT TEMPORARY EXECUTION AUTHORITY

**Program tracker:** [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266)

**Research provenance:** [CAP-INDEX #505](https://github.com/iamaman11/part-crm-emai-profile/issues/505)

**Production authorization:** NOT GRANTED

This is the single repository document that owns the ordered implementation program produced by the
completed CAP research. Issue #266 owns the live transaction pointer and accepted-main evidence. This
document deliberately contains no moving SHA, workflow count, provider observation, readiness result or
environment state.

Chat history, handoff text, a stale branch and historical plans never authorize work. An agent must read
fresh protected `main`, Issue #266 and the owning bounded Issue before starting a transaction.

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
8. Production is authorized only for one exact candidate and enabled capability set by the CAP-08 model.

## 3. Binding execution order

Each row is a separate bounded Issue, branch, PR and accepted-main reread. A later row cannot start merely
because its code is easy or an earlier branch is green.

| Order | Transaction | Natural owner and required outcome |
|---|---|---|
| E0 | Documentation authority cutover | CAP-06: current entrypoints converge on this sequence; stale PAS/FC current-state claims become history. |
| E1 | CAP-01 runtime-surface correction | `capability-policy` + Worker ingress: Queue/Scheduled consume canonical `RuntimeSurface -> ActivationUnit`; health membership/bypass is explicit and tested. |
| E2 | Release/promotion strict JSON | CAP04-T1 adapters: duplicate-member/bounded admission precedes typed decode on current release/promotion inputs. |
| E3 | D1 strict JSON | CAP04-T2 adapters: current D1 inputs use the same strict outer admission without changing provider/migration ownership. |
| E4 | First CAP-05 retirement | Retire the first independently proven completed transition/duplicate authority bundle; preserve its objective current invariant and passive provenance. |
| P1 | CAP12-I1 Client/Profile relationship | Client card shows authorized attached Profiles; attach, detach and atomic reassign use the existing assignment owner; relation never grants access. |
| P2 | CAP12-I2 authorized launch/Bridge | One server-authorized shipping path composes device trust, claim, lease/fence, workspace lock and pinned real Camoufox; replaced launch path is deleted. |
| P3 | CAP12-I3 confirmed save/reopen | Controlled close commits an exactly verified encrypted successor before `Saved`; failure preserves the last confirmed generation; reopen uses authoritative active generation. |
| V1 | Release-facing verification convergence | Apply accepted CAP-05 risk tiers to the exact reachable release surface; retire/narrow only proven duplicate orchestration without losing an invariant. |
| V2 | CAP12-I4 exact scenario acceptance | On one immutable non-Production candidate prove the accepted positive flow and required negative, replay, concurrency and recovery cases. |
| R1 | Exact candidate and evidence | Instantiate the CAP-08 envelope: source/artifacts, migrations/contracts, environment/config, capability digest/effective set, evidence, risks and named authorities. |
| R2 | Controlled pilot decision | If all mandatory guarantees pass, authorize only a bounded cohort with stop, recovery and expansion conditions. Pilot does not waive security/data guarantees. |
| R3 | Production Authorization | A named authority issues GO/PILOT or NO-GO for the unchanged exact candidate. No earlier transaction implies Production permission. |

The owning Issue must refine one row without widening it. If fresh evidence invalidates a prerequisite,
stop that transaction and record the exact blocker in Issue #266; do not skip ahead or invent a phase.

## 4. Transaction protocol

```text
fresh protected-main + GitHub re-baseline
-> select exactly one row and one bounded Issue
-> identify natural owner, effects, contracts, consumers and predecessor
-> implement the smallest coherent change
-> cut over callers and remove the replaced path
-> inspect full diff and simplification ledger
-> run targeted positive + negative proof
-> atomic commit and exact-head applicable CI
-> required contexts green; behind_by = 0; reviews/threads clear
-> guarded merge bound to the proven head
-> reread accepted main and update Issue #266
-> only then select the next row
```

Every implementation Issue must state:

- accepted finding or product obligation;
- natural owner and exact affected surface;
- consumers and bounded blast radius;
- predecessor deletion or evidenced retirement condition;
- minimum tests/evidence and execution tier;
- exit criteria, failure/recovery behavior and explicit non-goals;
- whether external, governance or Production mutation is authorized.

## 5. Gates and stop rules

- E0–E4 authorize no provider, staging or Production mutation.
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

## 6. Non-blocking owner-local convergence

The following work may be performed only as separately authorized natural-owner transactions and must
not become a second pre-Production program:

- remaining CAP-06 documentation/ADR/history cleanup;
- legacy AR program-sequence/acceptance projection retirement or reclassification after exact consumer
  and invariant proof;
- unsupported `opsctl` projection/report retirement after final consumer proof;
- D1/promotion pure-core extraction only when those families are touched;
- contract-floor and recovery-language reconciliation under CAP-07;
- conditional compatibility/publication deletion after consumer and persisted-obligation proof;
- future provider/capability work after an explicit product decision.

## 7. Program completion

The temporary program closes when:

1. E0–V2 have accepted evidence on protected `main`;
2. one exact candidate envelope is complete and all universal/reachable guarantees are decided;
3. R2/R3 record the named authorization outcome;
4. no temporary execution owner is still needed for ordinary feature development;
5. permanent decisions live in their natural docs/code/checks and #266 is closed as provenance.

Until then, Issue #266 is the only live transaction pointer. Never copy its mutable state into README,
AGENTS, projections or handoff documents.
