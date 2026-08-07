# Mailbox composition capability

## Current repository capability

**Status: Composed / Synthetic.**

The repository composes the mailbox domain through the versioned Worker API and governed D1 persistence. The composed path includes:

- tenant-scoped mailbox bindings with provider metadata and an opaque `SecretHandle` only;
- typed mailbox jobs with bounded cursor, attempts, due time, retry state and checked aggregate versions;
- owner-scoped versioned API routes for binding create/query/revoke and job create/query/run;
- exact actor-scoped idempotency using the shared command/digest/expiry policy;
- actor-bound command-journal identity and collision-resistant audit/outbox evidence IDs;
- atomic D1 command + idempotency + audit + outbox batches;
- direct-SQL guards for mailbox aggregate inserts, updates and deletes;
- support-safe read projections that omit secret handles and cursors;
- a deterministic metadata-only provider adapter behind `MailboxProviderPort`;
- a deterministic fake provider covering success, retryable failure, terminal failure and exhausted attempts;
- permanent SQLite privacy, lifecycle, replay and rollback regressions in the existing Quality Gate.

## Privacy boundary

The mailbox catalog, audit/outbox payloads and support-safe API responses must not contain raw mailbox passwords, OAuth access/refresh tokens, authorization headers or message bodies. Credential material is represented only by an opaque `SecretHandle` whose backing secret store is outside this slice.

The composed provider observation is bounded to provider status, item count and an opaque cursor. The cursor is persisted for scheduling but deliberately omitted from support-safe job responses.

## What this does not prove

This slice does **not** prove or claim:

- live Gmail API connectivity;
- live IMAP connectivity;
- browser-fallback mailbox automation;
- availability or correctness of external mailbox credentials;
- exactly-once side effects at a remote mailbox provider;
- production scheduling infrastructure beyond the repository-local deterministic job contract;
- production readiness.

A real provider adapter must remain behind `MailboxProviderPort`, preserve the same privacy boundary, classify retryable versus terminal failures explicitly, and supply external evidence before its capability can be promoted beyond **Composed / Synthetic**.
