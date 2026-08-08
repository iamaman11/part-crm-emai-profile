# Future Development — External CRM Integration

**Status:** future product evolution; not active execution
**Date:** 2026-08-08
**Prerequisite:** standalone Phase 2J accepted with production-readiness evidence

## 1. Purpose

This document records product evolution that is deliberately **outside** the active Phase 1–2
execution plan. It is not a roadmap competing with `DEVELOPMENT_PLAN.md`, cannot contain `NEXT`, and
must not block completion or release of the standalone application.

The standalone Browser Profile Platform must reach its full product definition of done before any
external CRM becomes authoritative.

## 2. External CRM / Party Integration

External CRM integration is a future capability, not an active Phase 3.

Only after standalone Phase 2J acceptance may a new planning decision consider:

1. preserve the standalone opaque `client_id` as the platform resource identity;
2. introduce/verify an opaque `external_party_ref` rather than deriving IDs from CRM names/contacts;
3. consume versioned CRM Party/Customer projections/events through an isolated adapter;
4. reconcile/link standalone Client and CRM Party without changing profile/generation/session IDs;
5. prove parity before transferring authority for canonical name/contact/status fields;
6. keep profile assignments, browser profiles, generations, sessions, certification, mailbox runtime
   and Profile Bridge lifecycle owned by this platform;
7. after authority transfer, block conflicting local edits or translate explicit commands through the
   CRM adapter;
8. evaluate any PostgreSQL/SQLx + RLS replacement as a separate architecture migration;
9. evaluate any CRM OIDC identity replacement as a separate security/identity migration;
10. preserve R2 encrypted-generation and Profile Bridge lifecycle boundaries without CRM coupling.

## 3. Future Acceptance Principles

Any later CRM initiative must still satisfy:

- versioned contract/event isolation; no CRM SDK/table/entity imports in core domain/application code;
- async durable projections for synchronization, with synchronous HTTP only where a user command
  legitimately needs an immediate acknowledgement/result;
- tenant/authorization checks before projection/fetch;
- no raw PII in technical identifiers, logs, events or support evidence;
- no standalone feature regression and no requirement to migrate R2 generation objects merely to
  integrate CRM;
- its own issue/ADR/branch/PR/evidence plan created **after** Phase 2J, based on the then-current
  product requirements.

## 4. Explicit Non-Goals Of The Active Roadmap

The active Phase 1–2 roadmap does not implement:

- CRM Party authority/cutover;
- CRM OIDC migration;
- CRM-backed PostgreSQL migration;
- CRM-specific UI/workflows;
- any dependency that prevents the standalone application from operating independently.

Until a future post-Phase-2 decision is accepted, external CRM integration remains `Target`/future
only and has no effect on `production_ready` for the standalone product.
