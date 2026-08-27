# Future Development — External CRM Integration

**Status:** FUTURE_PRODUCT_EVOLUTION / NOT_ACTIVE_EXECUTION  
**Current authority:** [`INDEX.md`](INDEX.md) -> [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)  
**Planning prerequisite:** a later explicit product/architecture decision from then-current accepted `main`

## 1. Purpose

This document records a future product-evolution option. It is not a roadmap competing with the current Architecture Re-baseline v3 / Production Capability program, cannot contain `NEXT`, and must not block the current path to Production Core.

External CRM integration must be planned only from the then-current accepted architecture and capability state. This file does not authorize work to begin after any particular historical Phase 2J checkpoint.

## 2. External CRM / Party Integration

If a future explicit decision activates this initiative, preserve these principles:

1. preserve the standalone opaque `client_id` as the platform resource identity unless a separately accepted migration proves otherwise;
2. introduce/verify an opaque `external_party_ref` rather than deriving IDs from CRM names/contacts;
3. consume versioned CRM Party/Customer projections/events through an isolated adapter;
4. reconcile/link standalone Client and CRM Party without changing profile/generation/session IDs;
5. prove parity before transferring authority for canonical name/contact/status fields;
6. keep profile assignments, browser profiles, generations, sessions, certification, mailbox runtime and Profile Bridge lifecycle owned by this platform unless a future bounded authority transfer explicitly changes that ownership;
7. after an accepted authority transfer, block conflicting local edits or translate explicit commands through the CRM adapter;
8. evaluate any PostgreSQL/SQLx + RLS replacement as a separate architecture migration;
9. evaluate any CRM OIDC identity replacement as a separate security/identity migration;
10. preserve R2 encrypted-generation and Profile Bridge lifecycle boundaries without CRM coupling.

## 3. Future Acceptance Principles

Any later CRM initiative must still satisfy:

- versioned contract/event isolation; no CRM SDK/table/entity imports in core domain/application code;
- async durable projections for synchronization, with synchronous HTTP only where a user command legitimately needs an immediate acknowledgement/result;
- tenant/authorization checks before projection/fetch;
- no raw PII in technical identifiers, logs, events or support evidence;
- no standalone feature regression and no requirement to migrate R2 generation objects merely to integrate CRM;
- its own issue/ADR/branch/PR/evidence plan created from the then-current accepted `main` and capability model;
- `source_present != production_enabled` remains binding throughout any CRM introduction.

## 4. Explicit Non-Goals Of The Current Program

The current CAP program does not require or authorize:

- CRM Party authority/cutover;
- CRM OIDC migration;
- CRM-backed PostgreSQL migration;
- CRM-specific UI/workflows;
- any dependency that prevents the standalone application from operating independently.

Until a future decision is accepted, external CRM integration remains future scope only and has no
effect on the active transaction, scenario completion or exact-candidate Production Authorization.
