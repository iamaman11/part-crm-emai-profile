# Post-AR-11 Functional Closure Plan

**Document status:** HISTORICAL_PROVENANCE / SUPERSEDED EXECUTION PLAN

**Current execution authority:** none; use `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` and Issue #266

**Historical trackers:** #399 and #421

**Operational/Production authorization:** none

This document preserves Post-AR-11 Functional Closure rationale and historical obligations. Its stage
order, readiness statements and FC-6 entry rules are not current execution authority. A current
obligation applies only when its natural owner or the CAP execution program explicitly carries it
forward.

This document preserves AR-11 Functional Closure obligations without creating another architecture program. Fresh protected `main`, the canonical plan and current live observations outrank historical implementation shape.

```text
source_present != production_enabled
production_mutation = false
```

## 1. Entry state

```text
F1/F2 ACCEPTED
-> N1 / #454 / N2 / N3 / N4 / N5 ACCEPTED
-> PF-1 ACCEPTED (#466)
-> PF-2 ACCEPTED + semantic-authority correction #477 + raw-provider-observation correction #480 ACCEPTED (#471 provenance)
-> PF-3 ACCEPTED provisional + truthfulness correction ACCEPTED (#478 / #431)
-> staging-baseline adoption ACCEPTED under temporary #486 authority
-> temporary adoption mechanism removed (#487)
-> fresh audit recorded FC-6 READY TO BEGIN / NOT STARTED
-> PAS-2/TC-1 executable frontend transport contract closure
-> final read-only readiness audit
-> FC-6 BLOCKED UNTIL FRESH READINESS + SEPARATE INSTRUCTION / NOT STARTED
-> FC-7 closeout
-> AR-12 implementation entry
```

A historical read-only FC-6 re-baseline found a Release Set v2/v3 rehearsal-verifier mismatch; #476 corrected that repository-only verifier and performed no staging mutation. PF-2/PF-3 were subsequently corrected, with #480 completing PF-2 raw-provider-observation convergence. Staging-baseline adoption then completed under temporary #486 authority, #487 removed that mechanism, and the fresh tracker audit recorded `FC-6 READY TO BEGIN / NOT STARTED`. The subsequently demonstrated PAS-2/TC-1 defect must now be removed before readiness is decided again. FC-6 may not resume from historical observations or prerequisite proof. A new FC-6 execution begins only after accepted PAS-2/TC-1, a separate explicit instruction and a fresh read-only re-baseline.

## 2. Functional Closure guarantees

The following outcomes remain binding:

- **FCG-1 — accepted-source semantics:** promotion proves its exact source was accepted protected `main`; historical source need not equal current main HEAD.
- **FCG-2 — accepted-source evidence:** binds repository + exact source + protected-main acceptance/lineage, not a self-consistent digest assertion.
- **FCG-3 — provenance closure:** canonical verification closes locally provable release-critical artifact/manifest/contract/schema/protocol/runtime/build identity.
- **FCG-4 — rollback compatibility:** target and known-good use one canonical `COMPATIBLE | INCOMPATIBLE | UNKNOWN` evaluator; `UNKNOWN` blocks.
- **FCG-5 — credential exposure:** read-only observation/preflight structurally precedes deploy-capable credential exposure and mutation.
- **FCG-6 — behavioural traceability:** mandatory release/promotion requirements map to permanent behavioural proof and fail-closed negatives.

Same-bits content addressing, stale `expected_current` fencing, `NO_CHANGE`, staging-only promotion and production blocking remain mandatory.

## 3. Release Set boundary

Current writer/target model is v3-only. Historical v2 is isolated integrity verification only; obsolete v2 writer semantics cannot become a current target or a second semantic authority.

```text
current writer/target = v3-only
current v2 semantic authority = 0
historical v2 = isolated integrity only
v2 -> v3 semantic coercion = 0
```

Current target and known-good identities are always observed fresh at FC-6 execution time.

`NONE` is valid only when the provider observation proves that no deployment exists. A live deployment without exactly one supported Release Set/profile annotation is `UNKNOWN`/`BLOCKED`; secret-triggered or legacy deployments must never be relabelled, guessed or treated as a clean environment.

## 4. FC-6 — one typed staging ceremony

FC-6 entry additionally requires accepted PAS-2/TC-1. The current browser path can otherwise accept a
successful JSON representation as a caller-selected TypeScript type without runtime proof and manually
duplicates operation metadata in feature adapters. That contradicts the PAS-2 stable-validation proof
assigned to Functional Closure. The bounded correction is owned by
`docs/PAS2_FRONTEND_TRANSPORT_CONTRACT_CLOSURE.md`; it authorizes no FC-6 or provider mutation.

FC-6 starts with a read-only preflight that is the fresh #399/#421 re-baseline:

```text
accepted protected main
-> observe live governance/workflows
-> observe credential readiness/scope
-> observe current staging identity
-> observe current known-good identity
-> observe current Release Set identities
-> observe required hosted evidence/attestations
-> typed READY | BLOCKED
```

The PF-2 hosted-evidence prerequisite now requires raw secret-free provider/process observations in strict Hosted Evidence v3. Typed Rust alone derives trust/readiness/outcome and enforces the exact staging account binding; workflow/shell code must not pre-classify provider reads into semantic booleans.

The FC-6 credential entry gate is concrete and fail-closed:

- the observation token is active, read-only, bound to the exact staging account, carries only Workers/D1/R2/Queues read permissions and has a provider expiry no later than six calendar months after issuance;
- the deploy token is active, staging-account/zone scoped, contains only the permissions already required by the guarded mutation executor, has the same six-calendar-month maximum lifetime and is not exposed before typed `READY`;
- the protected staging bindings contain the observation token, deploy token, deploy manifest and Access service-auth pair; secret values are never read back;
- every credential consumed by FC-6 is remotely valid and has enough remaining lifetime for the whole ceremony; an invalid, expired, non-expiring exportable static credential or wrong account/resource identity yields `BLOCKED`;
- a replacement is issued and validated before binding, and the predecessor is revoked only after the replacement binding and accepted-main/read-only proof succeed;
- FC-6 does not rotate Worker runtime keys, R2 S3 pairs, OAuth client secrets or Access service-auth. Those effectful overlap/rollback rehearsals remain owned by AR-13.

Only `READY` may cross into deploy-capable credential exposure or mutation.

Forbidden before READY:

- guessed or substituted `expected_current`;
- promotion/deployment used to discover state;
- staging/production mutation;
- Release Set switching;
- D1/R2/provider mutation;
- parallel diagnostic/provider authority.

The ceremony then proves:

```text
exact accepted source / same bits
-> staging promotion/deploy
-> post-deploy verification
-> rollback compatibility + rollback when required
   OR explicit NO_CHANGE where applicable
-> idempotent machine-readable terminal result
```

FC-6 is proof under the PF-3 provisional baseline, not a redesign stage. A failure permits only the smallest bounded correction required by the named scenario.

## 5. FC-7 — closeout

FC-7 consumes FC-6 terminal evidence, permanent requirement/proof mappings and required Linux/Windows/hosted proof. If evidence establishes repository-owned `P0=0`, `P1=0`, `P2=0`, FC-7 is a closeout decision rather than another implementation project.

## 6. Acceptance discipline

Exact-head CI/review/guarded-merge rules are owned by `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` and current protected governance. Historical workflow counts, SHAs and provider observations are not timeless constants; every hosted/staging decision uses fresh observations.

## 7. Post-FC semantics

```text
PAS-2/TC-1 = executable frontend transport closure; bounded pre-FC-6 correction only
FC-6 / FC-7 = proof and closeout; no generic redesign
AR-12 = fresh-environment rehearsal
AR-13 = rotation rehearsal: account/service-owned short-lived automation identity where supported; GitHub App short-lived tokens instead of durable PAT authority; observation/deploy, Access, R2, OAuth and runtime-key overlap/cutover/retirement; remove obsolete bundle/bootstrap bindings only after verified successors
AR-14 = remote-recovery rehearsal: encrypted escrow, break-glass issuance, Vault loss/restore and credential-compromise recovery without secret readback
AR-15 = Windows updater/delivery + LKG proof + hardware-backed code/update-signing key lifecycle + final architecture-form freeze
AR-16 = final audit only
AR-17 = Production Core qualification/authorization only: independently issued production credentials, protected reviewers, no staging reuse and fail-closed expiry/resource checks
PC-1  = first Production Core release
```

Production remains fail-closed until its owning gates explicitly authorize it.

Canonical references: `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`,
`docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`,
`docs/PAS2_FRONTEND_TRANSPORT_CONTRACT_CLOSURE.md`, #399, #421, #431/#478,
#471/#477/#480 and #486/#487.
