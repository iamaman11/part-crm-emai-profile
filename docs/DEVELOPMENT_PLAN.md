# Browser Profile Platform — Development Plan

**Document status:** GENERATED_PROJECTION  
**Canonical program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Live Functional Closure trackers:** #399, #421  
**Production Core gate:** `BLOCKED`  
**Production readiness:** `false`

This is a compact developer-facing projection, not a second roadmap. Protected `main`, current GitHub state and the canonical plan win over stale prose.

```text
source_present != production_enabled
```

## 1. Current state

```text
PF-1   ACCEPTED (#466)
PF-2   ACCEPTED; authority correction ACCEPTED (#477 / #471)
PF-3   ACCEPTED provisional; truthfulness correction ACCEPTED (#478 / #431)
FC-6   NEXT PERMITTED / NOT STARTED BY THIS TRANSACTION
AR-12  NOT STARTED
architecture_form_frozen = false
architecture_complete = false
production_mutation = false
```

Accepted code checkpoint before this documentation-only convergence: `81fba31e7c78966ec57e098d400d895d26e64dbf`. Re-read protected `main` before every transaction; do not copy this SHA forward as an execution assumption.

## 2. One bounded transaction discipline

```text
fresh protected main + GitHub state
-> choose exactly one bounded concern
-> identify natural owner, effects, contracts, live callers and predecessor
-> implement the smallest coherent change
-> remove replaced predecessor and callers in the same transaction
-> inspect complete diff + simplification ledger
-> targeted tests
-> one unchanged exact candidate head
-> all applicable permanent CI and protected required contexts green
-> behind_by = 0
-> reviews/threads clear
-> guarded merge bound to exact head SHA
-> accepted-main reread
```

Rules:

- fresh Git/GitHub state wins over old issues, old SHAs and saved CI;
- one semantic fact has one owner;
- a validator that independently recomputes business/policy semantics is a second authority;
- no compatibility bridge without a named live consumer or durable obligation;
- no successor registry merely to preserve a retired JSON/Python/Node/table authority;
- no new current-plan/roadmap document when existing authorities suffice;
- no checker whose primary purpose is checking another checker;
- no tracked projection without a durable exact-byte consumer;
- no CI weakening or branch/environment-protection bypass;
- uncertainty fails closed.

## 3. Permanent architecture boundaries

```text
bounded contexts + inward dependencies
Pure Core / Effect Shell
observation != policy verdict
strict versioned DTO at adapter boundary
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
Product Runtime -> opsctl/opsctl-core = 0
opsctl provider/network/process/credential authority = 0
Python duplicate product/release/lifecycle/evidence/fitness authority = 0
generated projection used as semantic input = 0
Release / Capability Profile = sole production-enable authority
```

JSON is allowed as an external/versioned contract, observation/evidence, artifact or generated projection. It is not an internal global business model or a second source of truth.

## 4. Accepted prerequisite outcomes

### PF-1

Accepted through #466. Typed Rust owns lifecycle semantics; historical Node/Python lifecycle/inventory predecessors and tracked inventory authority remain retired.

### PF-2

Accepted through #472/#474 and corrected through #477. The live path is:

```text
GitHub/provider read-only effects
-> raw secret-free observation
-> strict versioned DTO
-> typed Rust policy
-> Rust-derived trust/readiness/outcome
-> deterministic evidence/artifact projection
```

Node/Python/workflow layers do not supply READY/PASS/TRUSTED as semantic inputs. The remaining external-review Python utility is observation acquisition only.

### PF-3

Accepted through #475 and truthfulness-corrected through #478. The decorative `FitnessRuleRegistry` metadata index was removed because free-text `enforcement_owner` and `negative_proof` strings were not executable linkage.

The truthful baseline is now:

```text
objective invariant
-> natural specialized production checker
-> executable negative fixture/self-test using that checker path
-> permanent required CI caller where applicable
```

Do not rebuild a generic rule engine, metadata registry, DSL, plugin/DI/linter framework or fitness JSON. PF-3 remains provisional; final architecture-form freeze follows accepted AR-15.

## 5. Functional Closure

FC-6 is the next permitted stage after final readiness audit, but this transaction does not start it.

A historical read-only FC-6 re-baseline already found and fixed the Release Set v2/v3 rehearsal-verifier defect in #476. It did not perform staging mutation. Current FC-6 execution still requires a fresh read-only re-baseline of accepted `main`, governance, credentials, provider state, staging identity, known-good identity and Release Sets, producing typed `READY | BLOCKED` before any mutation.

```text
fresh #399/#421 re-baseline
-> typed READY | BLOCKED
-> only READY may expose deploy-capable credentials or permit staging mutation
-> exact accepted bits / same-bits staging proof
-> verify
-> rollback or explicit NO_CHANGE
-> terminal machine-readable evidence
-> FC-7 closeout
```

No guessed `expected_current`, no promotion/deployment as a diagnostic, and no parallel provider authority are allowed.

## 6. AR-12…AR-17

- **AR-12 — Fresh Rehearsal Environment:** prove bootstrap/deploy/smoke/teardown/recreate from canonical inputs.
- **AR-13 — Rotation Rehearsal:** prove real key/secret/credential rotation.
- **AR-14 — Remote Recovery Rehearsal:** prove recovery from durable state/artifacts.
- **AR-15 — Windows Delivery Program:** production-grade Profile Bridge/Camoufox updater, signed update contract, verification/key rotation, side-by-side staging, safe activation, health/LKG rollback, publisher integration and production-equivalent rehearsal. Accepted AR-15 establishes final architecture-form freeze.
- **AR-16 — Final Whole-project Audit:** audit-only; findings block and are fixed in bounded defect PRs.
- **AR-17 — Qualification / Production Core gate:** consumes accepted evidence and decides authorization; not a new closeout engine.

## 7. Binding product scenarios

```text
PAS-1 governed identity/access
PAS-2 client + browser-profile UI/API/bulk workflow
PAS-3 encrypted generation/persist/open/restore lifecycle
PAS-4 real Windows Profile Bridge + pinned Camoufox + updater/LKG
PAS-5 crash/timeout/duplicate/partial-failure recovery and observability
PAS-6 fresh same-bits staging delivery + rollback/recreate
PAS-7 production-core admission + later-capability fail-closed negatives
```

Validators are necessary but do not substitute for assigned end-to-end scenario evidence.

## 8. Production roadmap

```text
PC-1 Production Core v1
PC-2 Mailbox Administration
PC-3 Mailbox Jobs / Automation
PC-4 Outbound / later capabilities
```

PC-1 enables only accepted Production Core capabilities through Release / Capability Profile authority. Mailbox administration/jobs/outbound code may remain source-present and tested while production-disabled.

## 9. Canonical references

- `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` — current program authority;
- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md` — permanent architecture contract;
- `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md` — quality/anti-weakening contract;
- `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431 / #478 — PF-3;
- #471 / #477 — accepted PF-2 provenance;
- `docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md` / #399 / #421 — Functional Closure;
- `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` — exact-head/merge discipline;
- #441 / #430 / #466 — accepted historical pre-PF-1/PF-1 provenance.
