# Browser Profile Platform

Browser Profile Platform is a standalone, provider-neutral control plane for governed browser-profile, client, mailbox, device and notification workflows. The Rust control plane, Cloudflare adapters, Windows Profile Bridge, Camouhost runtime boundary and React operator UI are developed as one product with explicit authority, privacy and production-capability boundaries.

## Current state

- **Accepted repository-local product phase:** Phase 2I. Immutable provenance: [`architecture/accepted-phases.json`](architecture/accepted-phases.json).
- **Architecture Re-baseline v3:** active, tracked by issue #266.
- **Accepted top-level architecture slices:** AR-0 through AR-11; AR-8A…AR-8F accepted subslices.
- **Current accepted checkpoint:** AR-11 — Release-set / Promotion Architecture.
- **Current architecture slice:** AR-12 — Fresh Rehearsal Environment, **DERIVED CURRENT / NOT STARTED**.
- **Current implementation authority:** Post-AR-11 Functional Closure #399; current prerequisite **PF-1 #430**.
- **Mandatory pre-AR12 continuation:** PF-1 #430 -> PF-2 / Draft PR #428 -> PF-3 #431 -> fresh #399/#421 re-baseline -> FC-6 -> FC-7 -> AR-12 implementation entry.
- Issue #375 is closed historical hardening; it is not the current blocker.
- **Architecture complete:** `false`.
- **Production Core gate:** `BLOCKED`.
- **Production readiness:** `production_ready=false`.

### CURRENT_DELIVERY_MAP

Canonical machine projection: `architecture/inventory.json::current_delivery_map`. This section is human-readable projection, not a second roadmap or release authority.

| Delivery dimension | Current status | Scope / gate |
|---|---|---|
| Source implemented | **ACCEPTED THROUGH AR-11** | AR-11 source is accepted; AR-12 is derived current and NOT STARTED. |
| Accepted on main | **COMPLETE THROUGH AR-11** | AR-11 remains the latest accepted top-level architecture checkpoint. |
| Staging live | **PARTIAL** | AR-8C staging provider/credential foundation is live and smoke-verified only. |
| Production authorized | **NO** | `production_core_gate=BLOCKED`; only AR-17 may authorize the gate. |
| Production enabled | **NO** | `production_ready=false`; only PC-1 may enable accepted `production-core-v1` after AR-17. |
| Current blocker | **POST-AR-11 FUNCTIONAL CLOSURE #399** | PF-1/PF-2/PF-3/FC-6/FC-7 remain before AR-12 implementation. |
| Next gate | **PF-1 acceptance on protected main** | PF-2 is blocked on PF-1; FC-6 is blocked on PF-3 plus fresh #399/#421 re-baseline. |

`source_present != production_enabled` is binding. Staging success never implies production authorization or enablement.

## Current authority and target architecture

Start with [`docs/INDEX.md`](docs/INDEX.md).

The single current program authority is [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md). Prospective development must also satisfy the subordinate [`docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md). Current Functional Closure execution is owned by [`docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md); PF-3 enforcement is specified by [`docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`](docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md).

Target development flow:

```text
canonical authorities
-> typed policy/contracts
-> bounded-context domain + application
-> explicit ports/adapters/effect capabilities
-> composition roots
-> Release / Capability Profile admission
-> production exposure
```

Permanent themes: one semantic authority, bounded-context ownership, inward dependencies, provider-free domain/application core, typed critical IDs/state/contracts, command/query separation, explicit effects, context-owned persistence, typed configuration, versioned integration events, frontend projection only, touch-to-converge and cutover-to-deletion.

`one main` means one product architecture/source/data lineage and one capability-authority model; it does not require the Worker, Profile Bridge, Camouhost and `opsctl` to be one OS executable.

PF-3 #431 will make the cross-cutting rules machine-persistent through `architecture/architecture-fitness-policy.json`, Rule IDs, one primary enforcement owner per REQUIRED rule, positive/negative fixtures and an Architecture Fitness Gate. After PF-3, materially architecture-changing PF/FC/AR/PC work must declare Architecture Impact and pass all applicable REQUIRED rules on the exact candidate head.

## Production capability model

Production enablement is owned only by the accepted Release / Capability Profile path. Environment flags or UI visibility cannot independently authorize a capability.

Current intended Production Core includes foundation, identity/users, clients/customer cards, browser profiles, profile runtime, Camoufox, Windows Profile Bridge delivery/runtime and required notification/audit/health/readiness/observability foundations. Exact activation-unit facts remain owned by [`architecture/release-architecture-ar11.json`](architecture/release-architecture-ar11.json).

Mailbox administration/read/bindings/jobs/outbound code may remain present and tested in the same `main` while production-disabled; later PC-2/PC-3/PC-4 profiles enable them progressively.

The fail-closed program sequence is:

```text
PF-1 -> PF-2 -> PF-3 -> FC-6/FC-7
-> AR-12 -> AR-13 -> AR-14 -> AR-15
-> AR-16 final whole-project convergence audit
-> AR-17 architecture closeout / Production Core gate authorization
-> PC-1 Production Core v1
-> PC-2 Mailbox Administration
-> PC-3 Mailbox Jobs / Automation
-> PC-4 Outbound / later capabilities
```

No production provisioning or promotion is authorized in AR-0…AR-17. AR-17 may set `architecture_complete=true` and `production_core_gate=AUTHORIZED`, but keeps `production_ready=false`; PC-1 owns first Production Core enablement.

## Key current sources

- [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md) — program authority;
- [`docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) — target architecture/development discipline;
- [`docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md`](docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md) — PF/FC execution plan;
- [`docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md`](docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md) — PF-3 specification;
- [`architecture/release-architecture-ar11.json`](architecture/release-architecture-ar11.json) — accepted release/capability authority;
- [`architecture/runtime-cutover-ar10.json`](architecture/runtime-cutover-ar10.json) — runtime cutover authority;
- [`architecture/d1-evolution-ar9.json`](architecture/d1-evolution-ar9.json) — D1 evolution authority;
- [`architecture/credential-authority.json`](architecture/credential-authority.json), [`architecture/credential-lifecycle.json`](architecture/credential-lifecycle.json), [`architecture/profile-security.json`](architecture/profile-security.json), [`architecture/operator-contract.json`](architecture/operator-contract.json) — current subject authorities;
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — current development projection;
- [`docs/status.json`](docs/status.json) — machine-readable state projection;
- [`architecture/inventory.json`](architecture/inventory.json) — tracked generated architecture projection;
- [`architecture/accepted-phases.json`](architecture/accepted-phases.json) — immutable product-phase provenance.

## Architecture snapshot

- pure Rust domain/application crates own business rules and provider-neutral use cases;
- capability-specific ownership stays explicit;
- provider/D1/R2/Queue/Durable Object mechanics stay in outer adapters/composition;
- frontend consumes governed public contracts and never becomes activation/security authority;
- authorization, neutral disclosure, idempotency, fencing, privacy and recovery boundaries remain regression-protected;
- `source_present != production_enabled`.

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

Do not infer production authorization from source presence, UI visibility, an open PR, historical plan or synthetic evidence.
