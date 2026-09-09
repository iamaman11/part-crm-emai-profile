# D1 Catalog and Migration Operations Contract

**Status:** PERMANENT NORMATIVE BOUNDED CONTRACT  
**Last architecture acceptance:** TX-7 / Issue #642  
**Production authorization:** NONE

## 1. Boundary

D1 is the authoritative standalone catalog for tenant membership, clients,
profile metadata, assignments, explicit grants, idempotency, audit and outbox.
It does not store browser profile archives, plaintext secrets, mailbox content or
Durable Object lease state.

All production statements are owned by `crates/cloudflare-adapters`. The Worker
composition root may obtain the `CATALOG_DB` binding and construct the typed
repository, but application/use-case/domain crates cannot prepare or execute raw
D1 SQL.

This document is also the permanent repository contract for ordinary D1 migration
operations. Historical Issues #597, #624 and #642 preserve rationale and accepted
evidence; they are provenance, not an operator procedure or live authority.

## 2. Tenant Isolation

D1 has no PostgreSQL RLS. The schema and repository boundary therefore require:

- composite `(tenant_id, resource_id)` primary and foreign keys;
- `TenantScope` for every read;
- verified `ActorContext` for every mutation;
- no unscoped public repository method;
- foreign and absent resource lookups use the same scoped query shape;
- permanent negative tests for cross-tenant foreign keys and raw D1 access;
- standalone deployment remains one organization until a separate isolation ADR.

`tenant_id` is not inferred from request payload or resource ID. It comes from the
verified actor/application context.

## 3. Schema Invariants

Migration `0001_catalog.sql` establishes:

- tenants and external identities;
- memberships and invitations;
- clients and browser profile metadata;
- historical profile/client assignments;
- separate profile and client grants;
- idempotency records;
- sanitized append-oriented audit events;
- transactional outbox events.

Database guards include:

- one active `TENANT_OWNER` at most per tenant;
- one active primary client assignment at most per profile;
- composite tenant foreign keys;
- active client required for a new assignment;
- active membership required for a new resource grant;
- bounded opaque IDs and enum checks;
- valid JSON outbox payload;
- positive aggregate versions and ordered timestamps.

The schema enforces “at most one active owner”. Preventing removal of the final
owner and audited owner transfer remain application commands; they cannot be
implemented as a simple trigger without blocking a valid transfer transaction.

## 4. Mutation Envelope

A governed application mutation uses a D1 transactional `batch` when its rows
belong to the same database boundary:

1. aggregate state insert/update;
2. idempotency record;
3. sanitized audit event;
4. outbox event.

If any statement fails, D1 rolls back the complete batch. The permanent schema
suite independently proves the same envelope on SQLite by forcing a later audit
constraint failure and checking that no aggregate/idempotency/audit/outbox row
escaped rollback.

Optimistic updates use:

```sql
UPDATE ...
SET version = version + 1, ...
WHERE tenant_id = ? AND resource_id = ? AND version = ?
```

Zero changed rows are interpreted by the application adapter as a neutral
not-found or version-conflict result according to authorization context. A stale
version never overwrites the accepted row.

## 5. Migration Source and Local Proof

- migration files are forward-only and ordered `0001_...sql`;
- migration/change semantics have one natural owner and are projected into typed
  D1 policy rather than copied into operator YAML/Python;
- Wrangler owns applied migration bookkeeping and remains only the physical
  Cloudflare D1 adapter;
- the permanent D1 jobs apply migrations to isolated local databases and prove
  clean bootstrap/replay, canonical prefix handling, compatibility and recovery
  fixtures;
- a second local apply must report no pending migration;
- fresh database construction and reopen checks must preserve D1 invariants;
- remote staging/Production state is never inferred from local results or stale
  evidence.

Wrangler is pinned to `4.94.0` for the accepted control plane. Changing it is a
compatibility change and must rerun the complete migration suite and relevant
provider-path evidence before the changed path is accepted.

## 6. Permanent Migration Operations Contract

The accepted TX-1 through TX-7 result is permanent architecture meaning, not a
temporary stage procedure. Ordinary D1 migration operation has exactly one
standard operator-facing procedure:

```text
fresh protected-main / current authority
-> observe + qualify exact target
-> prepare immutable transaction
-> TransactionId
-> STOP for exact transaction-scoped authorization
-> sole protected executor
-> ExecutionReceipt
-> fresh post-state verification / recovery disposition
-> durable evidence
```

Canonical implementation owners are:

- `.github/workflows/d1-operator.yml` — the zero-input standard operator-facing
  orchestration surface; it is thin transport/composition only and has no provider
  credential or migration-semantic ownership;
- `.github/workflows/d1-isolated-target-observation.yml` — read-only provider fact
  collection/qualification; observations are evidence inputs, never policy;
- `.github/workflows/d1-migration-prepare.yml` plus `opsctl d1` — credential-free
  typed Prepare/plan/verification owners producing an immutable transaction;
- `.github/workflows/d1-migration-executor.yml` — the sole ordinary D1 migration
  mutation owner and the sole sanctioned migration/Time-Travel-recovery effect
  boundary;
- `ExecutionReceipt` plus fresh post-observation/typed verification — append-only
  evidence of the exact attempt, never a mutable readiness/status database.

The operator must derive identities and internal contracts mechanically from the
natural owners. The following are permanent invariants:

```text
operator-visible ordinary D1 entry surfaces = 1
manual internal JSON assembly = 0
second D1 migration semantic owner = 0
second ordinary D1 migration mutation owner = 0
mutable migration status database = 0
checker-for-checker = 0
automatic destructive restore = 0
post-authorization replanning = 0
Production authorization implied by migration tooling = 0
```

Therefore:

- manual assembly of manifest/repository/observation/transaction JSON for ordinary
  operation is forbidden;
- reading #597/#624/#642 history to discover the normal procedure is forbidden as
  an operational dependency; those Issues are consulted only for investigation,
  rationale or historical evidence;
- `opsctl d1` remains credential-free typed policy/planning/verification tooling
  and must never become a Cloudflare client or mutation backend;
- the observer gathers facts only and must not become a second semantic owner;
- authorization binds one already-prepared immutable `TransactionId`, exact target,
  exact allowed effect and freshness/fence constraints; source/tree/target/prestate/
  observation/policy drift invalidates it;
- the executor consumes the sealed plan and must not silently re-prepare or replan
  after authorization;
- provider credentials are exposed only inside the protected effect boundary after
  exact admission/fence checks pass;
- ordinary success requires an exact terminal receipt and fresh post-state
  verification; missing/ambiguous/mismatched evidence fails closed;
- replay/no-op, stale observation/authorization, source/tree/transaction drift,
  target/prestate drift, pre-write abort, failed-no-effect and recovery-required
  remain mechanically distinguishable typed outcomes;
- destructive Time Travel restore is never automatic and always requires its own
  separately authorized exact recovery transaction;
- Production mutation is never authorized by this document, source presence,
  green CI, a Release Set, an operator run or a migration receipt.

A future migration may introduce new schema/data/runtime compatibility semantics,
but those semantics must be added to their existing natural owner. Difficulty or
inconvenience in a workflow is not evidence that a second migration framework,
controller, mutation path or status service is needed.

## 7. Permanent Mechanical Protection

The repository protects this contract through existing natural-owner tests rather
than a new checker layer:

- `scripts/check-d1-boundary.py`, executed by the required Quality Gate, protects
  the D1 runtime boundary and the permanent operator/mutation-owner topology;
- `scripts/check-d1-migration-executor.mjs` directly validates the protected
  executor, exact authorization, sealed-plan consumption, fencing, receipt,
  provider-credential scoping, no post-authorization re-prepare, pinned Wrangler
  apply boundary and fail-closed recovery behavior;
- `.github/workflows/d1-operator.yml` contains its own transport-only/zero-input
  contract proof and exercises the typed transaction/admission/execution/outcome
  adapters;
- the D1 fault/recovery harness and `opsctl d1` tests exercise positive and
  negative typed policy cases without unnecessary provider mutation.

Changes that weaken these invariants must fail the existing protected checks. Do
not add a checker whose only purpose is to check another checker.

## 8. Evidence and Provenance Limits

Local/source gates can prove SQL compatibility, constraints, typed policy,
operator/executor topology, exact-plan behavior and synthetic fault/recovery
cases. They do not by themselves prove current remote provider state, latency,
contention, account/binding configuration, a specific Time Travel restore, hosted
identity, Production readiness or physical multi-device behavior.

Accepted D1-OPS provenance is retained in GitHub:

- Issue #597 — TX-1..TX-5 and pre-split TX-6 architecture/failure history;
- Issue #624 — accepted TX-6 isolated non-Production real rehearsal evidence;
- Issue #642 — accepted TX-7 standard-operator convergence, typed negative matrix,
  real ordinary non-Production apply and replay/no-op acceptance evidence.

These Issues must remain provenance. Ordinary operation starts from fresh protected
`main`, Issue #266, the CURRENT stage Issue and this contract; it does not reopen or
reconstruct TX-1..TX-7.
