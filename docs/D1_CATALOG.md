# D1 Catalog Foundation

**Статус:** Repository Step 3 implementation baseline  
**Дата:** 2026-08-05

## 1. Boundary

D1 is the authoritative standalone catalog for tenant membership, clients,
profile metadata, assignments, explicit grants, idempotency, audit and outbox.
It does not store browser profile archives, plaintext secrets, mailbox content or
Durable Object lease state.

All production statements are owned by `crates/cloudflare-adapters`. The Worker
composition root may obtain the `CATALOG_DB` binding and construct the typed
repository, but application/use-case/domain crates cannot prepare or execute raw
D1 SQL.

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

A governed mutation uses a D1 transactional `batch` when its rows belong to the
same database boundary:

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

## 5. Migrations

- migration files are forward-only and ordered `0001_...sql`;
- Wrangler owns applied migration bookkeeping;
- the permanent D1 job applies migrations to an isolated local D1 database;
- a second apply must report no pending migration;
- Python SQLite tests apply the same files to two fresh databases and compare the
  resulting schema;
- file-backed reopen, `foreign_key_check` and `integrity_check` are mandatory;
- remote production/staging apply requires a separate backup marker and evidence.

Wrangler is pinned to `4.94.0` for this repository gate. Changing it is a
compatibility change and must rerun the complete migration suite.

## 6. Evidence Limits

The Step 3 local gates can prove SQL compatibility, constraints, typed adapter
compilation, local Wrangler migration replay and synthetic transaction behavior.
They do not prove:

- remote D1 latency, contention or production limits;
- a real backup/Time Travel restore;
- Cloudflare account or binding configuration;
- Access JWT/membership integration;
- Durable Object/D1 cross-service coordination;
- production privacy, key management or multi-device behavior.
