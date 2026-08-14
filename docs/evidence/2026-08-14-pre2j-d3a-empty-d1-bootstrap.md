# Pre-2J D3A Empty-D1 Bootstrap Evidence

**Status:** bounded remote proof passed

**Date:** 2026-08-14

**Tracking:** issue #253 / PR #254

**Proof source:** `493d399b9531776aa8208242a5d1c05681764231`

## Scope

This evidence proves only that the deterministic repository-owned bootstrap built
from the exact `migrations/d1/*.sql` inventory can initialize a brand-new remote
Cloudflare D1 database through Wrangler's SQL-file import path. It also proves
that the same bootstrap rejects replay without changing the schema or migration
ledger.

The proof does not authorize incremental migration of a non-empty deployed
database, production deployment, production promotion, rollback semantics or
Phase 2J readiness. Those boundaries remain owned by D4, D3 and umbrella blocker
#203 as applicable.

## Exact Identity

- Wrangler: `4.94.0`;
- generated bootstrap: `261937` bytes;
- bootstrap SHA-256:
  `de1acf24f30084ba95c43bdb6f2463b068b54e27e9ec0834753dc6383efef069`;
- ordered migration ledger: exact repository migrations `0001` through `0026`;
- latest ledger row: `0026_outbound_mail_intents.sql`;
- sanitized machine-readable record:
  [`2026-08-14-pre2j-d3a-empty-d1-bootstrap.json`](2026-08-14-pre2j-d3a-empty-d1-bootstrap.json).

The machine-readable record intentionally excludes Cloudflare account IDs,
database names, database IDs, credentials and secret material. Permanent CI
binds its bootstrap digest and ordered ledger to the current repository inputs;
changing the generator or migrations invalidates this external proof.

## Positive Evidence

- the fresh remote target exposed exactly Cloudflare's reserved `_cf_KV` table
  and no application schema;
- repository `validate-empty` accepted that exact singleton platform row;
- Wrangler remote `d1 execute --file` completed successfully;
- the import executed 421 SQL statements and reported 2327 written rows;
- the canonical `d1_migrations` ledger matched all 26 repository migration names
  in exact order;
- the `0012_integration_event_foundation.sql` tables, indexes, triggers and both
  `outbox_events` version columns existed after import.

## Negative Evidence

- executing the identical bootstrap a second time failed at the side-effect-free
  empty-target guard with `SQLITE_ERROR`;
- the ordered ledger remained exact 26/26 and retained the same latest row;
- schema inventory contained 246 objects before and after replay rejection;
- both sanitized inventories had SHA-256
  `4effc97617730907d4b911881a5b4346326dc88ae92471063dc7bab5717f2f13`;
- no bootstrap guard residue remained;
- local permanent self-tests reject missing, non-contiguous and tampered
  migration inputs, unexpected remote schema, non-empty targets and replay.

## External Boundaries

- canonical staging touched: no;
- canonical production touched: no;
- production credentials or secret values recorded: no;
- user data involved: no;
- `production_ready=false` remains mandatory;
- Phase 2J and issues #251/#203 remain blocked/open.
