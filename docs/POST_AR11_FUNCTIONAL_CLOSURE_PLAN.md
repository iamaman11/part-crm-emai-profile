# Post-AR-11 Functional Closure Plan

**Document status:** SUBORDINATE_FUNCTIONAL_CLOSURE_PLAN  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Functional Closure umbrella:** #399  
**FC-6 tracker:** #421  
**Production authorization:** NONE

This document preserves AR-11 Functional Closure obligations without creating another architecture program. Current prospective architecture and natural semantic owners win over historical implementation shape; immutable AR-11 evidence remains provenance.

```text
source_present != production_enabled
production_mutation = false
```

## 1. Entry order

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 bounded Release Set v2 correction
-> N2 -> N3 -> N4 -> N5
-> PF-1
-> PF-2
-> PF-3 architecture-forming freeze
-> FC-6 preflight (includes fresh #399/#421 live re-baseline)
-> FC-6 staging ceremony
-> FC-7 closeout
-> AR-12 implementation entry
```

The `fresh #399/#421 re-baseline` is a logical requirement, not a separate implementation transaction or PR. It executes as the first read-only FC-6 preflight step against accepted PF-3 `main`.

## 2. Functional Closure defect classes

The following outcomes remain binding through normalization. Their implementation owners may change; the guarantees may not silently weaken.

- **FCG-1 — historical accepted-source semantics:** promotion proves that its source **was accepted protected main** under the then-valid acceptance protocol; it does not require the historical source to equal current main HEAD.
- **FCG-2 — accepted-source evidence:** evidence binds repository + exact source + protected-main acceptance/lineage, not merely a self-consistent digest claim.
- **FCG-3 — provenance closure:** canonical verification closes every locally provable release-critical artifact/manifest/contract/schema/protocol/runtime/build identity.
- **FCG-4 — rollback compatibility:** target and known-good rollback use one canonical `COMPATIBLE | INCOMPATIBLE | UNKNOWN` evaluator against observed current state; `UNKNOWN` blocks.
- **FCG-5 — credential exposure:** read-only observation/preflight structurally precedes deploy-capable credential exposure and mutation execution.
- **FCG-6 — behavioural traceability:** mandatory release/promotion requirements map mechanically to permanent behavioural proof and expected fail-closed result.

Accepted foundations such as immutable content addressing, exact artifact-byte verification, stale expected-current fencing, `NO_CHANGE`, staging-only promotion and production blocking remain mandatory.

## 3. #454 relationship

#454 is the only actual implementation transaction before N2. It does not weaken rollback or same-bits semantics; it decides whether historical Release Set v2 has a real current consumer.

```text
current v2 consumer exists
-> exact consumer/identity named
-> minimum isolated historical reader/verify path only
-> v3 writer/model remains current
-> explicit retirement condition

OR

current v2 consumer = NONE
-> executable v2 compatibility/current-v2 authority retires
-> historical evidence remains immutable
```

At FC-6 preflight, current target/known-good identities are observed again from then-current accepted authorities. FC-6 does not inherit a stale assumption that v2 must or must not exist.

## 4. FC-6 — one typed staging ceremony

FC-6 begins with a read-only preflight that **is** the fresh #399/#421 re-baseline:

```text
accepted PF-3 main
-> observe current hosted/repository/provider state
   - live workflows + branch governance
   - credential readiness and scope
   - current staging identity
   - current known-good rollback identity
   - current Release Set identities
   - current required evidence/attestation surfaces
-> typed READY | BLOCKED
```

Only `READY` may cross into deploy-capable credential exposure and mutation execution.

The ceremony then proves, through current typed owners:

```text
exact accepted source / same bits
-> staging promotion/deploy
-> post-deploy verification
-> rollback compatibility and rollback when required
   OR explicit NO_CHANGE convergence where applicable
-> idempotent machine-readable terminal success/failure audit
```

Permanent workflow YAML remains actionlint-protected. Live workflow registrations are reconciled to current desired governance; stale temporary registrations are removed rather than permanently allow-listed.

FC-6 is staging proof inside the PF-3-frozen architecture. It may expose a defect; it may not invent a new generic architecture mechanism to fix one.

## 5. FC-7 — closeout, not a second implementation project

FC-7 remains a logical acceptance checkpoint for traceability. It consumes:

- FC-6 terminal evidence;
- repository-owned requirement/proof mappings;
- required Linux/Windows/hosted proof;
- the then-current Functional Closure defect map.

If the evidence establishes:

```text
P0 = 0
P1 = 0
P2 = 0
```

FC-7 is a closeout decision. It should not create another implementation branch merely to restate evidence. New source work occurs only for a concrete defect discovered by FC-6/FC-7; that defect is fixed in a bounded PR and the proof is rerun.

## 6. Shared acceptance discipline

Exact-head CI/review/merge rules are owned by `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` and current protected governance. This document intentionally does not clone that checklist.

Historical workflow counts, SHAs and provider observations are not timeless constants. Every hosted/staging decision consumes fresh observations.

## 7. Post-FC semantics

After accepted PF-3:

```text
FC-6 / FC-7 = proof and closeout; no redesign
AR-12 = fresh-environment rehearsal
AR-13 = rotation rehearsal
AR-14 = remote-recovery rehearsal
AR-15 = Windows updater/delivery implementation + proof
AR-16 = final whole-project audit only
AR-17 = Production Core qualification/authorization decision only
PC-1  = first Production Core release
```

Production remains fail-closed until the later owning gates explicitly authorize it.

Canonical references: `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`, `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`, #399, #421, #430, #431, #441, #454.
