# Browser Profile Platform

Browser Profile Platform is a standalone, provider-neutral control plane for governed browser-profile,
client, mailbox, device and notification workflows. The Rust control plane, Cloudflare adapters,
Windows Profile Bridge and React operator UI are developed as one product with explicit authority and
privacy boundaries.

## Current state

- **Accepted repository-local product phase:** Phase 2I. Immutable provenance is
  [`architecture/accepted-phases.json`](architecture/accepted-phases.json).
- **Architecture Re-baseline v3:** active program, tracked by issue #266.
- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C, AR-5 and AR-6.
- **Current accepted checkpoint:** AR-6 — Full Python Estate + read-only Rust opsctl.
- **Next slice:** AR-7 — Environments + GitHub Governance + Operational Boundaries.
- **Architecture complete:** `false`.
- **Production Core gate:** `BLOCKED`.
- **Production readiness:** `production_ready=false`.
- **Phase 2J / old pre-2J execution sequence:** historical predecessor context; it is not the forward
  implementation queue after AR-1.

The single current architecture/program execution authority is
[`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md), tracked by issue
#266. AR-6 accepts the full Python estate and read-only Rust `opsctl` foundation while AR-5 remains the accepted runtime-authority cleanup and the AR-4C-remediated application/runtime ownership contract remains in
[`architecture/inventory.json`](architecture/inventory.json), with AR-4C acceptance evidence in
[`docs/ARCHITECTURE_REBASELINE_V3_AR4C.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4C.md), AR-4B evidence preserved in
[`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md), AR-4A evidence preserved in
[`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md), and the AR-3 base contract preserved in
[`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md); the accepted AR-2 topology input remains
[`architecture/runtime-topology-ar2.json`](architecture/runtime-topology-ar2.json). Machine transition
state is [`architecture/architecture-rebaseline-v3-transition.json`](architecture/architecture-rebaseline-v3-transition.json).

The sequence is fail-closed:

```text
AR remediation
  -> AR-16 final whole-project audit with P0=0/P1=0
  -> AR-17 architecture closeout / Production Core gate authorization
  -> PC-1 Production Core v1
  -> real production mutation
```

No production provisioning or promotion is authorized in AR-0…AR-17. Successful AR-17 may set
`architecture_complete=true` and `production_core_gate=AUTHORIZED`, but it still leaves
`production_ready=false`. Only successful PC-1 may set `production_ready=true` for the accepted
`production-core-v1` scope.

## Current authority

Start with [`docs/INDEX.md`](docs/INDEX.md). The main current sources are:

- [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md) — single current
  architecture/program execution authority, issue #266;
- [`docs/ARCHITECTURE_REBASELINE_V3_AR6.md`](docs/ARCHITECTURE_REBASELINE_V3_AR6.md) — accepted AR-6 Python-estate/read-only-opsctl evidence;
- [`architecture/python-estate-ar6.json`](architecture/python-estate-ar6.json) — accepted full tracked Python disposition;
- [`docs/ARCHITECTURE_REBASELINE_V3_AR5.md`](docs/ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;
- [`docs/ARCHITECTURE_REBASELINE_V3_AR4C.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4C.md) — accepted AR-4C Outbound Mail composition-extraction evidence;
- [`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;
- [`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;
- [`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 base application architecture evidence;
- [`architecture/runtime-topology-ar2.json`](architecture/runtime-topology-ar2.json) — accepted AR-2 topology/D3 input retained by the AR-3 projection;
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — current product/program projection and
  immutable accepted-phase provenance;
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) + accepted ADRs — stable architecture invariants;
- [`docs/DATA_CLASSIFICATION.md`](docs/DATA_CLASSIFICATION.md) — data/privacy authority;
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — current repository-local threat model;
- [`docs/DEVELOPER_CAPABILITY_MATRIX.md`](docs/DEVELOPER_CAPABILITY_MATRIX.md) — capability/evidence
  accepted on `main`;
- [`docs/status.json`](docs/status.json) — machine-readable current/readiness projection;
- [`architecture/inventory.json`](architecture/inventory.json) — canonical architecture inventory;
- [`architecture/accepted-phases.json`](architecture/accepted-phases.json) — immutable accepted product
  phase provenance.

The former issue #203 pre-2J product-readiness plan and current-looking root plans are preserved as
accepted predecessor history and explicitly marked historical/superseded for forward execution. AR-2
classified issue #251's old real-production D3 sequence as superseded forward execution while retaining
its repository-side bootstrap/release/promotion evidence; the legacy D3 production lane is fail-closed.

## Architecture snapshot

- pure Rust domain/application crates own business rules and provider-neutral use cases;
- capability-specific application ownership stays explicit;
- Cloudflare D1/R2/Queue/Durable Object and provider mechanics stay in outer adapters/composition;
- the React SPA uses feature-owned routes and feature-owned capability API modules;
- governed public DTOs are Rust-owned and generated deterministically to OpenAPI/TypeScript;
- authorization, neutral disclosure, idempotency, fencing, privacy and recovery boundaries remain
  regression-protected by permanent positive and negative CI evidence;
- `source_present != production_enabled`; UI visibility is never the production security boundary.

## Fast local verification

```bash
python scripts/verify-fast.py
python scripts/verify-fast.py --with-compile
```

Full acceptance still requires all applicable permanent GitHub workflows to succeed on one unchanged
exact PR head, zero blocking reviews/unresolved threads, `behind_by=0` and a guarded squash merge.

## Development and security

- Contributor workflow: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Security policy: [`SECURITY.md`](SECURITY.md)
- Product intent: [`docs/PRODUCT.md`](docs/PRODUCT.md)
- Future CRM boundary: [`docs/FUTURE_DEVELOPMENT.md`](docs/FUTURE_DEVELOPMENT.md)

Do not infer production authorization from source presence, UI visibility, an open PR, historical plan,
or synthetic/external evidence. Current machine state remains fail-closed until its owning gates are
accepted on `main`.