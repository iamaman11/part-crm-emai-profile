# Architecture Re-baseline v3 — AR-4A Composition-root consolidation

**Document status:** EVIDENCE / AR-4A candidate
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`
**Tracking:** #266 / bounded slice #279
**Exact accepted baseline:** `681831e23fc901d057553815a4de9f527f3c0d08`
**Production mutation:** forbidden

## 1. Purpose

AR-4A closes the general control-plane composition debt accepted by AR-3. It centralizes construction of request-scoped D1/provider adapters in `apps/control-plane-worker/src/composition.rs` without moving application policy, changing route ownership, changing provider behavior, changing public contracts, changing D1 schema or mutating any external environment.

The accepted AR-3 application architecture contract remains the authority while this document is a candidate. Acceptance is projected only after an exact-green guarded merge and mandatory post-merge authority closeout.

## 2. Consolidated seams

| Transport | Before AR-4A | Candidate composition seam |
|---|---|---|
| `operator_queries.rs` | direct `D1QueryRepository` construction | `composition::query_repository` |
| `notifications.rs` | direct notification D1 repository construction | `composition::notification_operations_repository` + `notification_cursor_repository` |
| `client_mail_query.rs` | direct query/eligibility/provider adapter construction | `composition::query_repository` + `client_mail_eligibility_repository` + `client_mail_query_provider` |
| `mailbox_jobs.rs` | direct `CloudMailboxProviderRouter` construction | `composition::mailbox_job_provider` |

All functions remain statically typed. No service locator, dynamic container, global mutable registry or `Box<dyn ...>` composition layer is introduced. Request-scoped `ActorContext`/`ClientId` references remain explicit inputs where required.

## 3. Reserved boundaries

AR-4A deliberately does not resolve Client Mail route ownership. `ClientMailSearchApi`/`ClientMailMessageApi` remain under the existing route classifier until AR-4B. Outbound Client Mail send composition remains owned by AR-4C. Profile extraction remains `NOT_REQUIRED` under AR-4D unless later accepted evidence reopens it.

AR-4A also does not change mailbox provider selection, Microsoft Graph authorization/refresh semantics, notification replay/cursor semantics, mailbox-job idempotency/version behavior, Queue/DLQ behavior, OpenAPI, migrations, Wrangler bindings or production capability state.

## 4. Permanent fitness rule

The canonical application-architecture verifier requires the new composition functions, forbids the AR-4A-owned concrete adapter imports/constructors in the affected transports, and includes a negative source fixture proving that reintroduced transport construction is rejected.

## 5. Candidate exit criteria

- exact source behavior tests and Worker compilation remain green;
- canonical inventory regenerates deterministically;
- AR-4A-owned transport construction debt is projected as a candidate closure while AR-4B/AR-4C remain open;
- frozen public contracts and D1 migrations remain unchanged;
- `architecture_complete=false`, Production Core remains `BLOCKED`, `production_ready=false`;
- no production/provider mutation;
- all applicable permanent PR workflows pass on one unchanged exact head;
- after guarded merge, post-merge authority closeout must mark AR-4A accepted and AR-4B next before AR-4B begins.
