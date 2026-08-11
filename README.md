# Browser Profile Platform

Browser Profile Platform is a standalone, provider-neutral control plane for governed browser-profile,
client, mailbox, device and notification workflows. The Rust control plane, Cloudflare adapters,
Windows Profile Bridge and React operator UI are developed as one product with explicit authority and
privacy boundaries.

## Current state

- **Accepted repository-local product phase: Phase 2I.** The immutable acceptance ledger is
  [`architecture/accepted-phases.json`](architecture/accepted-phases.json).
- **Pre-2J remediation: ACTIVE / BLOCKING PHASE 2J.** The active repository-owned closeout is
  [`docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md).
- **Phase 2J has not started.** Real production evidence and controlled rollout remain blocked until
  the pre-2J closure rule is satisfied on accepted `main`.
- **Production readiness:** `production_ready=false`. The machine-readable projection is
  [`docs/status.json`](docs/status.json).

Repository Steps 0–10 are historical delivery history, not the current implementation queue. Their
accepted evidence remains in [`docs/DELIVERY_ROADMAP.md`](docs/DELIVERY_ROADMAP.md) and
[`docs/evidence/`](docs/evidence/).

## Current authority

Start with [`docs/INDEX.md`](docs/INDEX.md). The main current sources are:

- [`docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) —
  temporary pre-2J blocker and closure rule while its status is ACTIVE;
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — normative product phase plan;
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) + accepted ADRs — stable architecture invariants;
- [`docs/DEVELOPER_CAPABILITY_MATRIX.md`](docs/DEVELOPER_CAPABILITY_MATRIX.md) — accepted capability
  and evidence level on `main`;
- [`docs/status.json`](docs/status.json) — machine-readable current/readiness projection;
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — canonical current repository-local threat model;
- [`architecture/accepted-phases.json`](architecture/accepted-phases.json) — immutable accepted phase
  provenance.

If prose conflicts, follow the hierarchy in `docs/INDEX.md`; do not infer acceptance from an open PR,
historical evidence file or phase-specific closeout document.

## Architecture snapshot

- pure Rust domain/application crates own business rules and provider-neutral use cases;
- capability-specific application ownership is separated (`use-cases-clients`, `use-cases-identity`,
  `use-cases-mailboxes`, `use-cases-devices`, `use-cases-notifications`, `use-cases-query`), while the
  remaining shared `use-cases` crate owns current Profile/Generation/coordinator orchestration;
- Cloudflare D1/R2/Queue/Durable Object and provider mechanics stay in outer adapters/composition;
- the React SPA uses feature-owned routes and feature-owned capability API modules; shared API code is
  transport/generated-contract infrastructure only;
- governed public DTOs are Rust-owned and generated deterministically to OpenAPI/TypeScript;
- authorization, neutral disclosure, idempotency, fencing, privacy and recovery boundaries are backed
  by permanent positive and negative CI evidence.

## Fast local verification

```bash
python scripts/verify-fast.py
python scripts/verify-fast.py --with-compile
```

Full acceptance still requires all permanent GitHub workflows to succeed on one unchanged exact PR
head, zero blocking reviews/unresolved threads, `behind_by=0` and a guarded squash merge.

## Development and security

- Contributor workflow: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Security policy: [`SECURITY.md`](SECURITY.md)
- Product intent: [`docs/PRODUCT.md`](docs/PRODUCT.md)
- Future CRM boundary: [`docs/FUTURE_DEVELOPMENT.md`](docs/FUTURE_DEVELOPMENT.md)

Phase 2J External evidence cannot be substituted with repository-local/synthetic proof and cannot set
`production_ready=true` before every mandatory external acceptance gate is reviewed and accepted.
