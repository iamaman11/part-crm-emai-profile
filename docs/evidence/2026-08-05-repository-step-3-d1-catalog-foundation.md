# Repository Step 3 — D1 Catalog Foundation Evidence

**Дата:** 2026-08-05  
**Статус:** technical scope passed; merge acceptance pending  
**Baseline:** `313a94aa59d10fa6a2d3e9a6da80bd9315e33fc4`  
**Technical evidence head:** `40d84c5cf5d7832a3db964ab639e822f2e055031`  
**Pull request:** #12  
**Technical Quality Gate run:** `31043260598`

## 1. Реализованная Catalog Boundary

Migration `migrations/d1/0001_catalog.sql` establishes strict tables and indexes
for:

- tenants and external identities;
- memberships and invitations;
- clients and browser profile metadata;
- historical profile/client assignments;
- explicit profile and client grants;
- idempotency records;
- sanitized audit events;
- transactional outbox events.

The migration uses tenant-inclusive composite primary and foreign keys for owned
resources. Bounded IDs, enum values, versions and timestamps are guarded by SQL
checks.

## 2. Database Invariants

The schema and permanent synthetic suite prove the covered invariants:

- no more than one active `TENANT_OWNER` per tenant;
- no more than one open primary client assignment per profile;
- a new assignment requires an active same-tenant client;
- a new grant requires an active same-tenant membership;
- foreign tenant client/profile links are rejected;
- assignment history remains separate from authorization grants;
- tenant-scoped foreign and missing client lookup have the same empty shape;
- stale optimistic version cannot overwrite the current client row.

The database enforces “at most one owner”. Preventing removal of the final owner
and performing an atomic owner transfer remain Step 4 application commands.

## 3. Mutation Transaction Envelope

The typed D1 adapter contains a create-client `batch` with:

1. aggregate state;
2. idempotency result;
3. sanitized audit event;
4. outbox event.

The SQLite suite independently forces a duplicate-audit failure after an
aggregate CAS update plus idempotency/audit/outbox inserts. It proves that all
covered rows roll back together. A succeeding transaction proves all four
components commit together and the aggregate version increments once.

## 4. Typed Cloudflare Adapter Boundary

`crates/cloudflare-adapters` owns raw `worker::d1` access. Public repository reads
require `TenantScope`; mutations require verified `ActorContext`.

The permanent static gate:

- accepts the real repository tree;
- excludes only the deliberate negative fixture from the positive scan;
- rejects raw D1 preparation inside a synthetic `use-cases` crate;
- prevents domain/application code from acquiring provider-specific D1 APIs.

The production Worker depends on the adapter and constructs
`D1CatalogRepository` from the `CATALOG_DB` binding. The adapter therefore
participates in the checked and packaged Worker WASM rather than existing as an
unreferenced crate.

## 5. Migration Evidence

Permanent job `D1 Catalog Migrations` uses Wrangler `4.94.0` and an isolated local
D1 state directory:

- applies `0001_catalog.sql` from empty state;
- records 25 successful SQL commands;
- repeats migration apply and receives no pending migrations;
- queries the migrated tenant/client/profile tables and `d1_migrations` row.

Wrangler local D1 rejects arbitrary integrity PRAGMAs through `d1 execute` with
`SQLITE_AUTH`. Therefore `PRAGMA foreign_key_check` and
`PRAGMA integrity_check` are executed in the permanent Python SQLite suite using
the same migration files; Wrangler remains responsible for migration application
and replay evidence.

## 6. Permanent CI Result

Quality Gate run `31043260598` succeeded on technical head
`40d84c5cf5d7832a3db964ab639e822f2e055031`.

### Rust Linux and WASM

- architecture positive and forbidden-dependency negative checks;
- raw D1 boundary positive and negative checks;
- current and deliberately breaking contract checks;
- accepted contract baseline immutability;
- deterministic D1 schema, tenant constraints, concealment, CAS, rollback/commit
  envelope, file-backed reopen, foreign-key and integrity tests;
- rustfmt, Clippy with warnings denied and native tests;
- governed pure crates compile for `wasm32-unknown-unknown`;
- status and current-tree tracked-secret checks.

### D1 Catalog Migrations

- pinned Wrangler `4.94.0` local migration apply;
- replay no-op;
- migrated catalog shape and migration record query.

### Rust Windows

- all native non-Worker/non-Cloudflare-adapter workspace tests.

### Cloudflare Worker Release Build

- Worker and typed D1 adapter check for WASM;
- pinned `worker-build 0.8.5` release packaging;
- generated shim and Wasm artifact verification.

## 7. Defects Found And Corrected

- the initial permanent workflow did not actually contain D1 gates; the four-job
  read-only gate now makes them merge-blocking;
- `worker::query!` required a direct crate-root `serde` dependency;
- the adapter initially was not composed into the Worker dependency graph;
- negative fixtures were initially visible to the positive raw-D1 scan;
- assignment and transaction fixtures rolled back their own setup rows;
- `sqlite3.Row` was compared directly with a tuple;
- Wrangler local D1 denied integrity PRAGMAs, so the gate was separated into
  authorized Wrangler shape queries and the existing strict SQLite integrity
  suite;
- all temporary write/debug workflows were removed before the technical run.

## 8. Что Не Доказано

This evidence does not prove:

- remote Cloudflare D1 deployment, latency, contention, limits or Time Travel;
- real Cloudflare account bindings, backup or restore;
- Cloudflare Access JWT validation or membership resolution;
- owner bootstrap/transfer and full invitation/revoke use cases;
- production API authorization or React UI;
- Durable Object/D1 reconciliation or distributed fencing;
- R2 encryption/generation lifecycle, Windows Bridge or Camouhost;
- production privacy, key management, multi-device behavior or readiness.

No Cloudflare credential, remote resource, user profile, mailbox content or
personal data was used.
