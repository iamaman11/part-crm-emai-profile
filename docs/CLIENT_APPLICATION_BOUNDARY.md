# Client Application Boundary

**Status:** Phase 0 reference pattern for the migrated client create/by-id vertical.

**Scope:** repository architecture only. This document does not claim production readiness or completion of the remaining Worker orchestration migration.

## Purpose

The client create and visible-by-id routes are the first bounded Phase 0 vertical moved from Worker-owned orchestration to an application-owned command/query path. The accepted direction is:

```text
HTTP / Workers SDK
  -> apps/control-plane-worker/src/clients.rs
     transport parsing + authenticated actor context + protocol mapping
  -> crates/use-cases/src/clients.rs
     authorization intent + domain validation + replay/write/query sequencing
  -> crates/application-ports/src/clients.rs
     provider-neutral inward contract
  -> crates/cloudflare-adapters/src/d1_clients.rs
     D1/idempotency/visibility projection adapter
  -> existing atomic D1 catalog command/query implementations
```

Concrete D1 construction is isolated in `apps/control-plane-worker/src/composition.rs`. The migrated `clients.rs` transport must not import `cloudflare_adapters::d1_*`, construct `CreateClientMutation`, or instantiate D1 repositories directly.

## Command Evidence

`application-ports::CommandExecutionEvidence` carries the provider-neutral durable command evidence required by the existing catalog transaction:

- typed idempotency key;
- request digest;
- deterministic audit event ID;
- deterministic outbox event ID;
- command timestamp;
- idempotency expiry timestamp.

The Worker transport may derive this evidence from HTTP headers/runtime time, but D1-specific mutation envelopes remain adapter-owned.

## Client Create Ownership

The application use case owns these decisions and ordering rules:

1. only a tenant owner may execute client create; non-owner disclosure remains neutral `not_found`;
2. owner authorization is evaluated before request-body/idempotency parsing, preserving the previous disclosure boundary;
3. `ClientRecord::create` owns display-name normalization and domain validation;
4. exact idempotency replay returns the prior logical result without issuing a write;
5. a concurrent unique conflict is rechecked for exact replay and is otherwise a conflict;
6. the adapter maps the validated record and command evidence into the existing atomic `CreateClientMutation` so client state, idempotency, audit and outbox retain the same D1 transaction boundary;
7. storage/provider failures are mapped to stable application failure classes and are not relabeled as business `not_found`.

The HTTP response contract remains transport-owned: a fresh create is `201`, an exact replay is `200`, and the existing camelCase receipt shape is preserved.

## Visible Client Query Ownership

The Worker parses the client ID and calls the application query. The D1 adapter retains the existing disclosure-safe visibility SQL. It projects provider strings/version into typed `ClientKind`, `ClientStatus` and `AggregateVersion`; invalid stored projection values fail as integrity errors rather than being treated as a missing business resource.

## CI Enforcement

Permanent Repository Quality Audit checks enforce two layers:

- `check-capability-module-layout.py` requires the command/client application symbols to remain capability-owned instead of collapsing into root facade files;
- `check-worker-application-boundary.py` rejects direct D1/client-mutation dependencies in the migrated Worker transport and includes a deliberately failing direct-D1 fixture to prove the policy is active.

Pure use-case tests use a deterministic fake `ClientApplicationPort`; Cloudflare adapter tests, Worker native tests and Worker WASM checks then prove the outward composition separately.

## Migration Rule For The Remaining A0 Work

Subsequent profile, generation, mailbox and identity/governance slices should copy the dependency direction, not the client-specific interface shape:

1. isolate one bounded route family;
2. define the minimum inward port owned by that application use case;
3. move authorization/domain/idempotency sequencing into the use-case layer;
4. adapt existing provider operations behind that port without moving SQL/provider types inward;
5. switch live routing only after pure + adapter + native/WASM evidence passes;
6. delete the superseded Worker orchestration so one implementation remains;
7. add a permanent positive/negative architecture check before merge.

Other ordinary Worker handlers are still subject to Phase 0 migration. This reference vertical therefore does **not** close architecture gap A0 by itself, and `production_ready` remains `false`.
