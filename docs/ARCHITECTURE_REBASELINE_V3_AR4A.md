# Architecture Re-baseline v3 — AR-4A Composition-root consolidation

**Document status:** EVIDENCE / AR-4A accepted
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`
**Tracking:** #266 / bounded slice #279
**Exact accepted baseline:** `681831e23fc901d057553815a4de9f527f3c0d08`
**Exact-green implementation candidate:** `f257a30a1df437812edb5c9e4b33c3de7e0740bc`
**Accepted implementation merge:** `74672285ef0146c2dc6da298024b378438e5a75d`
**Implementation PR:** #280 — 13/13 applicable permanent PR workflows passed on the unchanged exact head
**Production mutation:** forbidden

## 1. Purpose

AR-4A closes the general control-plane composition debt accepted by AR-3. It centralizes construction of request-scoped D1/provider adapters in `apps/control-plane-worker/src/composition.rs` without moving application policy, changing route ownership, changing provider behavior, changing public contracts, changing D1 schema or mutating any external environment.

The accepted AR-3 application architecture contract remains the base contract. AR-4A is accepted as its composition-root remediation after exact-green guarded merge and post-merge authority closeout; AR-4B is the next required slice.

## 2. Consolidated seams

| Transport | Before AR-4A | Accepted composition seam |
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

## 5. Acceptance record

- exact source behavior tests and Worker compilation remain green;
- canonical inventory regenerates deterministically;
- AR-4A-owned transport construction debt is accepted as closed while AR-4B/AR-4C remain open;
- frozen public contracts and D1 migrations remain unchanged;
- `architecture_complete=false`, Production Core remains `BLOCKED`, `production_ready=false`;
- no production/provider mutation;
- all applicable permanent PR workflows pass on one unchanged exact head;
- post-merge authority projects AR-4A accepted and AR-4B next before AR-4B begins.
