# Documentation Index

**Status:** navigation / documentation governance  
**Date:** 2026-08-08

This page is the entry point for repository documentation and makes document authority
explicit.

## Normative Current Sources

- [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) — **single normative post-composition execution order**: what comes next, phase dependencies and acceptance.
- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — stable architecture boundaries, ownership and allowed dependency direction.
- [`DATA_CLASSIFICATION.md`](./DATA_CLASSIFICATION.md) — normative data classes, storage/logging/disclosure and mailbox-content handling.
- [`UI_ARCHITECTURE.md`](./UI_ARCHITECTURE.md) — normative standalone product/UI target.
- accepted ADRs/security/privacy documents — invariant authority for their bounded concern.

## Current Implementation / Evidence Orientation

- [`DEVELOPER_CAPABILITY_MATRIX.md`](./DEVELOPER_CAPABILITY_MATRIX.md) — authoritative accepted implementation/evidence level (`Composed`, `Library`, `Synthetic`, `Target`, `External`).
- [`../architecture/inventory.json`](../architecture/inventory.json) — deterministic machine-readable inventory of workspace members, D1 migrations, public route/classifier ownership, generated public contracts and documentation authority; CI verifies drift against repository truth.
- [`PROFILE_APPLICATION_BOUNDARY.md`](./PROFILE_APPLICATION_BOUNDARY.md) — capability-specific profile application-boundary architecture/evidence.
- [`PROFILE_GENERATION_REGISTRY.md`](./PROFILE_GENERATION_REGISTRY.md) — profile-generation lifecycle/storage contract.

## Future Development — Not Active Execution

- [`FUTURE_DEVELOPMENT.md`](./FUTURE_DEVELOPMENT.md) — post-standalone evolution only. External CRM/Party integration is explicitly outside the active Phase 1–2 critical path and can be considered only after Phase 2J production-readiness closure.

## Historical / Design Baseline

- [`DELIVERY_ROADMAP.md`](./DELIVERY_ROADMAP.md) — historical Repository Steps 0–10 and their delivery discipline. Old Step 11–12 sketches are superseded by current `DEVELOPMENT_PLAN.md` phases.
- [`../IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md) — original expert design baseline; not current execution order.
- [`../PROFILE_LIFECYCLE_PLAN.md`](../PROFILE_LIFECYCLE_PLAN.md) and similar original plans — historical/design input unless a current normative document explicitly delegates an invariant to them.

## Authority Rules

1. **What to implement next:** `DEVELOPMENT_PLAN.md` only.
2. **How the architecture is allowed to work:** `ARCHITECTURE.md` + accepted ADR/security/data-classification invariants.
3. **What is actually implemented/accepted:** `DEVELOPER_CAPABILITY_MATRIX.md` + merged code/CI/evidence.
4. **Future development:** `FUTURE_DEVELOPMENT.md` records post-standalone ideas only; it never supplies `NEXT` and never overrides the active Phase 1–2 order.
5. **Historical rationale:** old roadmaps/plans; they never override current execution order.

A plan/specification is not an implementation claim. PR descriptions are not acceptance
evidence; the actual merged diff and exact-head gates are authoritative.

External/physical/provider claims remain External until the repository's evidence process
accepts real evidence.

## Documentation Governance

- Do not create another parallel normative execution roadmap.
- Execution-order changes go into `DEVELOPMENT_PLAN.md`.
- Invariant changes go into the relevant architecture/ADR/security document before code acceptance.
- Capability PRs update the matrix only for claims they actually change.
- New normative documents must be linked here or folded into an existing source.
- Temporary amendments must be consolidated and removed rather than accumulating precedence layers.
- Machine-checkable documentation claims should become CI checks where practical.
