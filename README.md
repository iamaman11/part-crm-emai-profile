# Browser Profile Platform

Browser Profile Platform is a standalone, provider-neutral control plane for governed browser-profile, client, mailbox, device and notification workflows. The Rust control plane, Cloudflare adapters, Windows Profile Bridge, Camouhost runtime boundary and React operator UI are developed as one product with explicit authority, privacy and production-capability boundaries.

## Current state

- **Accepted repository-local product phase:** Phase 2I. Immutable provenance: [`architecture/accepted-phases.json`](architecture/accepted-phases.json).
- **Architecture Re-baseline v3:** active, tracked by issue #266.
- **Accepted top-level architecture slices:** AR-0 through AR-11; AR-8A…AR-8F accepted subslices.
- **Current accepted checkpoint:** AR-11 — Release-set / Promotion Architecture.
- **Current architecture slice:** AR-12 — Fresh Rehearsal Environment, **DERIVED CURRENT / NOT STARTED**.
- **Current execution umbrella:** Post-AR-11 Functional Closure #399.
- **Accepted pre-PF-1 normalization:** **F1/F2 + N1**.
- **Current next work:** **N2 — AR-6 Python-estate authority retirement + role/effect normalization**, followed by N3 → N4 → N5. PF-1 #430 remains blocked until N5 acceptance and a fresh PF-1 entry reread.
- Issue #375 is closed historical hardening; it is not a current blocker or execution authority.
- Closed PR #428 is a superseded pre-normalization PF-2 checkpoint only; PF-2 must later start from a fresh branch based on accepted PF-1 `main`.
- **Architecture complete:** `false`.
- **Production Core gate:** `BLOCKED`.
- **Production readiness:** `production_ready=false`.

The binding pre-AR-12 execution path is:

```text
F1 Release Set breaking-contract version discipline                 ACCEPTED
+
F2 permanent application/opsctl/doctor/canonical-JSON/Python       ACCEPTED
 ->
N1 AR-2 runtime/resource-topology authority retirement             ACCEPTED
 ->
N2 AR-6 Python-estate authority retirement + role/effect norm.     NEXT
 ->
N3 AR-7 current GitHub-governance normalization
 ->
N4 bounded AR-8 operator/provenance cleanup
 ->
N5 AR-10 runtime semantic-authority retirement
 ->
PF-1 #430 typed lifecycle + deterministic bounded-projection inventory cutover
 ->
PF-2 Universal Hosted Operational Evidence from a fresh post-PF-1 branch
 ->
PF-3 #431 typed Rust Architecture Fitness Baseline
 ->
fresh #399/#421 re-baseline
 ->
FC-6
 ->
FC-7
 ->
AR-12 implementation entry
```

F1/F2/N1…N5 are foundation/normalization transactions, not new AR/PF lifecycle slices. `source_present != production_enabled` remains binding.

## Documentation and authority

Start with [`docs/INDEX.md`](docs/INDEX.md).

The single current program authority is [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md). Prospective development must satisfy [`docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md), [`docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md), [`docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`](docs/OPSCTL_ARCHITECTURE_BOUNDARY.md), [`docs/OPSCTL_DOCTOR_CONTRACT.md`](docs/OPSCTL_DOCTOR_CONTRACT.md) and [`docs/PYTHON_USAGE_BOUNDARY.md`](docs/PYTHON_USAGE_BOUNDARY.md).

Current prerequisite/closure execution is specified by:

- [`docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`](docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md) — F1/F2 + N1…N5;
- issue #441 — live accepted-main execution tracker and N2→N5 handoff checklist;
- [`docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md) — PF/FC execution umbrella;
- [`docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`](docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md) and #430 — PF-1 contract and live entry gate;
- [`docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`](docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md) — PF-3 typed fitness contract.

`docs/status.json`, `architecture/inventory.json`, `docs/DEVELOPMENT_PLAN.md` and README/index surfaces are projections/navigation, never independent semantic or lifecycle authority. Transitional projection lag must not be interpreted as permission to skip F/N/PF/FC prerequisites.

## Target architecture

```text
canonical natural owner
-> typed policy/contracts
-> bounded-context domain + application
-> explicit ports/adapters/effect capabilities
-> composition roots
-> Release / Capability Profile admission
-> production exposure
```

Permanent themes: one semantic authority, bounded-context ownership, inward dependencies, provider-free domain/application core, typed critical IDs/state/contracts, explicit effects, context-owned persistence, typed configuration, versioned durable/integration contracts, frontend projection only, touch-to-converge and cutover-to-deletion.

`one main` means one product architecture/source/data lineage and one capability-authority model; it does not require the Worker, Profile Bridge, Camouhost and `opsctl` to be one OS executable.

## `opsctl` and `doctor`

`opsctl` is standalone offline operator/policy/planning/verification/projection tooling with a strict Pure Core / Effect Shell boundary:

```text
filesystem / JSON / explicit observations
-> adapters + versioned DTOs
-> typed semantic inputs
-> PURE CORE
-> typed results
-> output adapters
```

Required zero budgets include:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
Product Runtime -> opsctl/opsctl-core = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
global authority bag = 0
```

`opsctl doctor` remains read-only local diagnostic composition only. It must not execute Python/Node/Git/GitHub/provider subprocess/API calls, use network/provider/secrets/runtime, mutate state, duplicate lifecycle/release/evidence/domain policy, or become a global authority catalog.

## PF-1 / PF-3 direction

PF-1 uses a typed `LifecycleEvaluator` plus bounded minimal inventory projections. It must not introduce `GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet`, and `architecture/inventory.json` remains generated output rather than compiler input.

Before PF-1 starts, issue #430 requires a fresh accepted-main reread proving F1/F2/N1…N5 accepted, no predecessor-normalization implementation PR remains open, no N1–N5 retired current authority was reintroduced, and all permanent `opsctl`/Python/generated-projection zero budgets still hold. N2–N5 prepare unambiguous natural owners; they must not pre-build a generic PF-1 projection framework.

PF-3 semantic authority belongs to typed Rust, for example:

```text
FitnessRuleRegistry
-> evaluator / enforcement mapping
-> positive + negative fixtures
-> Architecture Fitness Gate
-> optional generated report/index projection
```

A manually maintained semantic `architecture/architecture-fitness-policy.json` is explicitly **not** the target owner.

## Production capability model

Production enablement is owned only by the accepted Release / Capability Profile path. Environment flags or UI visibility cannot independently authorize a capability.

Current intended Production Core includes foundation, identity/users, clients/customer cards, browser profiles, profile runtime, Camoufox, Windows Profile Bridge delivery/runtime and required notification/audit/health/readiness/observability foundations. Mailbox administration/read/bindings/jobs/outbound code may remain present and tested in the same `main` while production-disabled; later PC profiles enable them progressively.

After Functional Closure the fail-closed program continues:

```text
AR-12 -> AR-13 -> AR-14 -> AR-15
-> AR-16 final whole-project convergence audit
-> AR-17 architecture closeout / Production Core gate authorization
-> PC-1 Production Core v1
-> PC-2 Mailbox Administration
-> PC-3 Mailbox Jobs / Automation
-> PC-4 Outbound / later capabilities
```

No production provisioning or promotion is authorized in AR-0…AR-17. AR-17 may set `architecture_complete=true` and `production_core_gate=AUTHORIZED`, while `production_ready=false`; PC-1 owns first Production Core enablement.

## Fast local verification

```bash
python scripts/verify-fast.py
python scripts/verify-fast.py --with-compile
```

Full acceptance requires all applicable permanent GitHub workflows and protected required contexts to pass on one unchanged exact PR head, zero blocking reviews/unresolved threads, `behind_by=0`, guarded merge bound to the expected head and accepted-main reread.

## Development and security

- Contributor workflow: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Security policy: [`SECURITY.md`](SECURITY.md)
- Product intent: [`docs/PRODUCT.md`](docs/PRODUCT.md)
- Future CRM boundary: [`docs/FUTURE_DEVELOPMENT.md`](docs/FUTURE_DEVELOPMENT.md)

Do not infer production authorization or current execution order from source presence, UI visibility, generated projections, an open PR, a historical plan or synthetic evidence.
