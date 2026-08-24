# Architecture Re-baseline v3 — Current Program Authority

**Document status:** CURRENT_AUTHORITY  
**Program:** Architecture Re-baseline v3  
**Tracking issue:** #266  
**Post-AR-11 Functional Closure:** #399  
**FC-6 execution tracker:** #421  
**Accepted PF-2 corrections:** #480 / #477 / #471  
**Accepted PF-3 truthfulness correction:** #478 / #431  
**Accepted code checkpoint before this documentation-only convergence:** `a8af2120255f117a7cf58ab86ff79963005f58a0`  
**Next permitted stage:** FC-6 — READY TO BEGIN ONLY AFTER FINAL READ-ONLY AUDIT AND A SEPARATE EXPLICIT INSTRUCTION  
**FC-6 execution in this transaction:** NOT STARTED  
**Next AR slice:** AR-12 — Fresh Rehearsal Environment — BLOCKED / NOT STARTED  
**Production authorization:** NONE  
**Architecture form frozen:** `false`  
**Architecture complete:** `false`  
**Production Core gate:** `BLOCKED`  
**Production ready:** `false`  
**Production mutation:** `false`

This document is the single current architecture/program execution authority. Fresh protected `main`, current GitHub governance and executable code outrank stale prose, old SHAs, closed trackers and historical documents. Historical AR/PF/FC records preserve provenance but do not silently become current semantic authority.

The application remains one modular product with one protected `main`, one architecture hierarchy, one schema/compatibility lineage and one Release / Capability Profile authority for production admission.

```text
source_present != production_enabled
```

## 1. Binding prospective architecture contract

All future PF/FC/AR/PC work is governed by this document together with:

- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`;
- `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`;
- `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`;
- `docs/OPSCTL_DOCTOR_CONTRACT.md`;
- `docs/PYTHON_USAGE_BOUNDARY.md`.

Permanent shape:

```text
natural canonical owners
        ↓
typed policy / contracts
        ↓
bounded-context domain + application
        ↓
explicit ports / adapters / effect capabilities
        ↓
composition roots
        ↓
Release / Capability Profile admission
        ↓
production exposure
```

Binding rules:

```text
Single semantic owner
Bounded Context ownership
Inward Dependency direction
Pure Core / Effect Shell
Observation != policy decision
Explicit effects/capabilities
Typed critical IDs/states/contracts
Command != Query
Context-owned persistence
Typed validated configuration at composition edges
Versioned integration/external contracts
Release / Capability Profile = sole production-enable authority
Frontend = projection, never security boundary
Cutover -> zero live callers -> zero unique current invariants -> delete predecessor
Fresh Git/GitHub state wins over stale prose
One bounded concern per transaction
Exact-head acceptance cannot be bypassed
```

### 1.1 Negative complexity budget

The prerequisite architecture program exists to reduce ambiguity and ship a coherent Production Core, not to maximize architecture artifacts.

```text
new parallel roadmap/current-plan document = 0
new 1:1 successor registry = 0
new hand-maintained global authority catalog = 0
new tracked projection without a durable exact-byte consumer = 0
new checker whose primary purpose is checking another checker = 0
legacy predecessor retained only for internal CI/docs/self-test = 0
```

When a cutover replaces an authority, switch all live callers and remove the predecessor in the same bounded transaction unless a named external/persisted/migration obligation requires isolated compatibility.

### 1.2 Pre-production compatibility default

This repository has not yet had a production release. Historical internal implementation shape is not a compatibility target by default.

```text
current product/security/durable obligations
-> current prospective architecture
-> natural semantic owners
-> proved live consumers/external observations
-> accepted historical evidence
-> historical internal implementation shape
```

No proved current/external consumer + no durable/persisted/migration obligation means compatibility bridge default = NO.

## 2. Artifact roles and JSON boundary

Durable machine artifacts are one of:

```text
CODE_SEMANTIC_AUTHORITY
EXTERNAL_DATA_CONTRACT_OR_MANIFEST
OBSERVATION
GENERATED_PROJECTION
HISTORICAL_EVIDENCE
TRANSITIONAL_SEMANTIC_SOURCE
```

JSON is valid for versioned external contracts/manifests, provider observations, evidence/artifacts and generated projections. JSON is invalid as a manually duplicated internal business model, global registry or second source of truth.

Strict observation/transport DTOs must be versioned, reject unknown critical fields, carry facts rather than precomputed verdicts, contain no secrets and convert to domain types at the adapter boundary.

## 3. Canonical AR sequence

`architecture/architecture-program-sequence.json` is static-order-only data; it does not own mutable current state.

```text
AR-0   Delta Architecture Inventory                              DONE / ACCEPTED
AR-1   Architecture Authority Re-baseline                        DONE / ACCEPTED
AR-2   Runtime Topology + D3 Compatibility                       DONE / ACCEPTED
AR-3   Application Architecture Contract                         DONE / ACCEPTED
AR-4A  Composition-root consolidation                            DONE / ACCEPTED
AR-4B  Client Mail route ownership                               DONE / ACCEPTED
AR-4C  Outbound Mail composition extraction                      DONE / ACCEPTED
AR-4D  Profile extraction                                        NOT REQUIRED unless evidence reopens
AR-5   Wrangler / Runtime Authority Cleanup                      DONE / ACCEPTED
AR-6   Full Python Estate + read-only Rust opsctl                DONE / ACCEPTED
AR-7   Environments + GitHub Governance + Operational Boundaries DONE / ACCEPTED
AR-8   Secrets / Keys / OAuth Refresh Concurrency                 DONE / ACCEPTED
AR-9   D1 Evolution / Schema Compatibility                       DONE / ACCEPTED
AR-10  Runtime and Historical Executable Simplification          DONE / ACCEPTED
AR-11  Release-set / Promotion Architecture                      DONE / ACCEPTED
AR-12  Fresh Rehearsal Environment                               NOT STARTED
AR-13  Rotation Rehearsal
AR-14  Remote Recovery Rehearsal
AR-15  Windows Delivery Program — inherited Batch E
AR-16  Final Whole-project 10/10 Audit
AR-17  Architecture Closeout + Production Core Gate
```

No production provisioning/promotion is implicitly authorized by AR-0…AR-17.

## 4. Accepted prerequisite chain and current boundary

The architecture-normalization prerequisites are now historical accepted work:

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 current Release Set v3 / historical-v2 isolation ACCEPTED
-> N2 ACCEPTED
-> N3 ACCEPTED
-> N4 ACCEPTED
-> N5 ACCEPTED
-> PF-1 ACCEPTED (#466)
-> PF-2 ACCEPTED + semantic-authority correction #477 + raw-provider-observation correction #480 ACCEPTED (#471 provenance)
-> PF-3 ACCEPTED provisional + truthfulness correction ACCEPTED (#478 / #431)
-> documentation convergence
-> final read-only readiness audit
-> FC-6 next permitted stage
```

A historical read-only FC-6 re-baseline and the repository-only rehearsal-verifier correction #476 occurred before the later PF corrections. #476 removed a Release Set v2/v3 verifier mismatch and performed no staging mutation. #480 subsequently completed PF-2 raw-provider-observation convergence. That history does not authorize resuming FC-6 inside the current correction/documentation work.

FC-6 may begin only after a fresh explicit user instruction following readiness acceptance.

## 5. Permanent `opsctl` boundary

`tools/opsctl` is standalone project-specific operator/policy tooling. Product Runtime never depends on it.

```text
CLI / composition
-> effect adapters: filesystem / strict external decoding / rendering
-> versioned DTOs
-> typed domain input
-> PURE CORE
-> typed result
-> output adapter
```

Hard invariants:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
runtime product dependency on opsctl = 0
opsctl provider/network/process/credential authority = 0
opsctl -> Python semantic child process = 0
global authority bag = 0
```

`opsctl` does not become a GitHub/Cloudflare owner, browser launcher, generic provider client, credential manager or second control plane.

## 6. `opsctl doctor`

`opsctl doctor` is local read-only diagnostic composition only. It may inspect bounded local filesystem structure and render diagnostics; it may not execute processes, access GitHub/providers/network/secrets/browser runtime, mutate state or aggregate a semantic authority catalog.

## 7. Python / Node boundary

Python/Node may remain bounded effect-shell utilities where objectively necessary, including the owned Camouhost cross-language boundary, provider observation adapters and tests.

They may not independently compute READY/PASS/TRUSTED/compatibility/business policy already owned by Rust. After caller cutover, replaced Python/Node semantic predecessors are deleted rather than preserved as wrappers/fallbacks.

`runtime/camouhost/real.py` remains the legitimate Product Runtime adapter behind Profile Bridge + versioned IPC + `runtime-lock.json`; synthetic paths remain test-only.

## 8. Release Set and canonical contract foundation

Current Release Set writer/target model is v3-only. Historical v2 remains isolated minimum integrity verification only where a real historical obligation exists; current v2 semantic authority and v2->v3 semantic coercion remain zero.

Attestable/content-addressed JSON requires:

```text
explicit kind + schema_version
bounded bytes/depth/complexity
duplicate-member rejection before canonicalization
reviewed/pinned SHA-256
independent canonicalization/hash vectors
canonical bytes separated from pretty rendering
explicit semantic-vs-exact-byte digest scope
```

## 9. PF-2 accepted boundary

PF-2, semantic-authority correction #477 and raw-provider-observation correction #480 are accepted.

Current flow:

```text
GitHub / provider read-only effects
-> raw secret-free HTTP/provider/process observations
-> strict Hosted Evidence v3 DTO
-> typed Rust EvidencePolicy / external-evidence policy
-> Rust-derived trust/readiness/outcome
-> deterministic durable evidence/artifact projection
```

Workflow/Python/Node effects do not supply READY/PASS/TRUSTED or provider-read success booleans as semantic inputs. #480 removed the remaining workflow-side `workers_deployments_read`, `d1_catalog_read`, `r2_bucket_read`, `queue_read` and `worker_secret_names_read` verdict authority. Rust alone classifies allowed HTTP/provider/process results, freshness/credential/mutation constraints and the exact staging account binding (`a94259ab73151da7058613fe8ec17b4d` / `pvisakp`). Legacy Hosted Evidence v2/read-verdict shape, unknown critical fields and wrong account identity fail closed.

The remaining external-review Python acquisition path produces raw GitHub observations; typed Rust owns repository binding, lineage/review validity and acceptance.

## 10. PF-3 accepted provisional boundary

PF-3 and truthfulness correction #478 are accepted.

The earlier typed metadata index was removed because free-text `enforcement_owner` / `negative_proof` fields were descriptive rather than executable linkage. PF-3 does not replace it with a generic engine.

Truthful enforcement is:

```text
objective invariant
-> natural specialized production checker
-> executable negative fixture/self-test using that checker path
-> real permanent CI caller
-> protected required context where admission-critical
```

There is no semantic fitness JSON, global fitness registry, architecture DSL or checker-for-checker layer. See `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`.

PF-3 remains provisional. FC-6 through AR-15 may make only the smallest scenario-driven correction required by concrete failing evidence. Final architecture-form freeze follows accepted AR-15 Windows delivery/updater/recovery proof; `architecture_complete` remains false until AR-17 qualification.

## 11. FC-6 / Functional Closure boundary

Live trackers: #399 and #421.

The next FC-6 run begins with a fresh read-only observation of accepted protected `main`, governance, workflows, credential scope/readiness, current staging identity, known-good identity, current Release Sets and hosted evidence.

```text
fresh read-only #399/#421 re-baseline
-> typed READY | BLOCKED
-> only READY may expose deploy-capable credentials or permit staging mutation
-> exact accepted bits / same-bits staging proof
-> post-deploy verification
-> rollback compatibility + rollback or explicit NO_CHANGE
-> idempotent terminal typed evidence
-> FC-7 closeout
```

Forbidden before READY:

- guessed/substituted `expected_current`;
- promotion/deployment as diagnosis;
- staging or production mutation;
- Release Set switching;
- D1/R2/provider mutation;
- parallel provider/credential authority.

Current documentation/PF corrections perform none of those operations.

## 12. Post-PF-3 phase semantics and final freeze

```text
FC-6 / FC-7     functional closure + staging proof; bounded scenario correction only
AR-12           fresh-environment rehearsal
AR-13           rotation rehearsal
AR-14           remote-recovery rehearsal
AR-15           Windows updater/delivery/LKG proof + final architecture-form freeze
AR-16           final whole-project audit only
AR-17           qualification/authorization decision only
PC-1            first Production Core release
```

No later roadmap stage is a generic redesign bucket.

## 13. Production state model

These states remain independent:

```text
architecture_form_frozen = false until accepted AR-15
architecture_complete = false until AR-17 qualification
production_core_gate = BLOCKED until AR-17 authorization
production_ready = false until later PC-1 admission
production_mutation = false for this prerequisite/documentation work
```

Source presence does not grant production exposure.

## 14. Binding product acceptance scenarios

| ID | Scenario | Required outcome |
| --- | --- | --- |
| PAS-1 | Identity and governed access | Owner/sign-in/membership/authorization work end to end; unauthorized/revoked/stale access fails before mutation. |
| PAS-2 | Client and browser-profile workflow | Real UI/API create/update/bind/grant/bulk flow with stable validation/audit. |
| PAS-3 | Encrypted profile lifecycle | Generation encrypt/persist/open/close/restore without plaintext/identity ambiguity. |
| PAS-4 | Real Windows browser execution | Profile Bridge launches pinned real Camoufox through versioned IPC, enforces single writer and safe update/rollback. |
| PAS-5 | Failure/retry/recovery | Crash/timeout/duplicate/partial failure is fenced/idempotent and recoverable with actionable observability. |
| PAS-6 | Fresh same-bits delivery/rollback | Clean staging from canonical inputs deploys exact accepted bits, verifies, rolls back/LKG-restores and recreates without hidden state. |
| PAS-7 | Production admission fails closed | Production Core enables only declared surfaces; missing evidence/invalid credentials/unknown compatibility/later capabilities block before side effect. |

Phase mapping:

```text
FC-6/FC-7 -> PAS-1,2,3,6,7
AR-12     -> PAS-1,2,3,6
AR-13     -> PAS-3,5,7
AR-14     -> PAS-3,5,6
AR-15     -> PAS-4,5,6 + final architecture-form freeze
AR-16     -> audit PAS-1..7
AR-17     -> authorize only with PAS-1..7 accepted
PC-1      -> promote/re-prove production admission/observability
```

Validators cannot substitute for assigned end-to-end scenario evidence.

## 15. Production capability roadmap

```text
PC-1  Production Core v1
PC-2  Mailbox Administration
PC-3  Mailbox Jobs / Automation
PC-4  Outbound / later capabilities
```

PC-1 includes identity/users, clients/customer cards, browser profiles and bulk operations, client↔profile binding, grants/access, generations/sessions/devices, encrypted persistence/restore, real Camoufox, Windows Profile Bridge + AR-15 updater/delivery, and required audit/health/readiness/observability/recovery foundations.

Mailbox administration/jobs/outbound code may remain source-present and tested while `production_enabled=false`.

## 16. Governance acceptance and stop rule

Every bounded source transaction:

```text
fresh protected main + trackers + PRs
-> one bounded concern
-> natural owner/effects/contracts/callers/predecessor
-> smallest coherent diff + predecessor retirement
-> targeted positive/negative proof
-> unchanged exact candidate head
-> permanent CI green
-> protected required contexts green
-> behind_by = 0
-> reviews/threads clear
-> guarded merge bound to exact head
-> accepted-main reread
```

Historical workflow counts/names/SHAs are observations, never timeless constants.

For the current prerequisite closeout, the required checkpoint is:

```text
PF-2 semantic-authority + raw-provider-observation corrections accepted
+ PF-3 truthfulness correction accepted
+ current documentation converged
+ protected-main acceptance evidence read fresh
+ final read-only audit
= FC-6 next permitted stage, but not started
```

A separate explicit user instruction is required to begin FC-6. Until then no FC-6 branch, preflight execution, rehearsal, promotion, deployment, rollback, staging mutation or production mutation is authorized.