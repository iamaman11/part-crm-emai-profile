# Documentation Index

**Status:** navigation / documentation governance  
**Date:** 2026-08-08

This page is the entry point for repository documentation. It does not replace the
source-of-truth rules inside the documents themselves; it makes their role explicit.

## Normative Target / Current Plan

- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — stable architecture boundaries, ownership and dependency direction.
- [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) — main post-composition execution plan and phase order.
- [`DEVELOPMENT_PLAN_AMENDMENT_2026-08-08.md`](./DEVELOPMENT_PLAN_AMENDMENT_2026-08-08.md) — normative amendment for application modularity, frontend boundaries, retry/DLQ, users-first discovery and client-scoped mailbox message search/body access. Where it conflicts on those topics, the amendment takes precedence until folded into the main plan.
- [`DATA_CLASSIFICATION.md`](./DATA_CLASSIFICATION.md) — normative data classes, storage/logging boundaries and mailbox-content handling.
- [`UI_ARCHITECTURE.md`](./UI_ARCHITECTURE.md) — normative standalone product/UI target.

## Capability Specifications / Accepted Architecture

- [`PROFILE_GENERATION_REGISTRY.md`](./PROFILE_GENERATION_REGISTRY.md) — profile-generation lifecycle/storage contract.
- [`PROFILE_APPLICATION_BOUNDARY.md`](./PROFILE_APPLICATION_BOUNDARY.md) — accepted profile application-boundary architecture.
- [`DEVELOPER_CAPABILITY_MATRIX.md`](./DEVELOPER_CAPABILITY_MATRIX.md) — authoritative implementation/evidence status (`Composed`, `Library`, `Synthetic`, `Target`, `External`).

## Historical / Accepted Delivery Record

- [`DELIVERY_ROADMAP.md`](./DELIVERY_ROADMAP.md) — historical Repository Steps 0–10 and their accepted delivery/acceptance discipline. It does not override the current post-composition execution order.

## Governance Rules

- A plan/specification is not an implementation claim.
- `DEVELOPER_CAPABILITY_MATRIX.md` is used to decide what is actually implemented and at what evidence level.
- External/physical/provider claims remain external until accepted by the repository's external-evidence process.
- New normative documents must be linked here or folded into an existing normative source rather than creating an untracked parallel roadmap.
- Temporary development-plan amendments must be consolidated into `DEVELOPMENT_PLAN.md` during the next documentation-convergence slice.