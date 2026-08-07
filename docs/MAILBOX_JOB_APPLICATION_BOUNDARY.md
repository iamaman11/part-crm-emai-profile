# Mailbox Job Application Boundary

**Status:** Phase 0 reference pattern for mailbox job create / visible-by-ID / run.

**Scope:** repository architecture only. This document preserves the existing synthetic metadata-provider behavior; it does not claim real mailbox provider readiness, scheduler expansion, or production readiness.

## Purpose

Mailbox job orchestration moves from Worker-owned D1/idempotency/provider sequencing to an application-owned path:

```text
HTTP / Workers SDK
  -> apps/control-plane-worker/src/mailbox_jobs.rs
     authenticated actor + strict protocol parsing + response mapping
  -> crates/use-cases/src/mailbox_jobs.rs
     authorization + validation + replay/query/run sequencing
  -> crates/application-ports/src/mailbox_jobs.rs
     provider-neutral job contract with opaque prepared-run token
  -> crates/cloudflare-adapters/src/d1_mailbox_jobs.rs
     D1/idempotency + synthetic provider adapter
  -> existing atomic D1 mailbox job mutations/queries
```

Concrete D1 construction belongs to `apps/control-plane-worker/src/composition.rs`. The Worker job transport must not instantiate D1 repositories, idempotency repositories, D1 mutation envelopes, `MetadataMailboxProviderAdapter`, or call `decide_mailbox_run` directly.

## Protocol And Disclosure Invariants

The migrated transport preserves the accepted observable contract:

1. active actor resolution occurs before route/body handling;
2. only a tenant owner may use mailbox job create/get/run; non-owner disclosure remains neutral `not_found`;
3. malformed binding/job path identifiers remain neutral after actor/owner resolution;
4. request DTOs deny unknown fields;
5. `requestDigest` remains exactly 64 lowercase hexadecimal characters;
6. create keeps `jobId`, `cursor`, `delayMs`, `maxAttempts`, `requestDigest`;
7. run keeps `expectedJobVersion`, `requestDigest`;
8. create fresh and exact replay remain HTTP `201`; run fresh and exact replay remain HTTP `200`;
9. visible job response remains `jobId`, `status`, `attempt`, `maxAttempts`, `nextRunAtMs`, `providerStatus`, `boundedItemCount`, `version`;
10. message-body, raw credential and secret-handle data are absent from the job read/response model.

## Create Ownership

The application layer owns:

- tenant-owner authorization intent;
- delay, max-attempt and cursor validation;
- checked scheduling timestamp calculation from command evidence;
- exact idempotency replay before write;
- concurrent unique-conflict replay recheck;
- fresh aggregate version `1`;
- stable application failure taxonomy.

The use case intentionally does not add a preliminary binding read. This preserves the established replay/error ordering and leaves the existing atomic D1 create mutation responsible for binding existence/state enforcement.

## Query Ownership

The application query owns tenant-owner authorization and typed job projection. It exposes only job metadata and provider-status summary, never raw mailbox contents or credentials.

A missing or non-visible job remains disclosure-neutral at the Worker protocol boundary.

## Run Ownership

The run use case preserves the established sequence:

```text
owner authorization
  -> checked expected-version +2 response version
  -> exact idempotency replay
  -> binding lookup
  -> job lookup
  -> expected-version check
  -> provider run preparation
  -> prepared-version integrity check
  -> atomic D1 run write
  -> concurrent unique-conflict replay recheck
```

Version arithmetic uses checked aggregate-version operations; saturation or wrapping is forbidden.

## Opaque Prepared Run

`MailboxJobApplicationPort` owns an associated `RunDecision` type. `prepare_run` returns `MailboxJobPreparedRun<RunDecision>` containing a provider-neutral summary plus an opaque decision token.

The Cloudflare adapter uses the existing `MailboxRunDecision` as that token. The same decision produced by `decide_mailbox_run` is passed into the existing `RunMailboxJobMutation`; the application layer never needs to understand or reconstruct provider-specific decision internals, and provider execution is not repeated.

The current adapter intentionally preserves the existing synthetic metadata provider behavior:

- provider status `SYNTHETIC_OK`;
- bounded item count `0`;
- deterministic metadata cursor `meta_<job_id>_<attempt>`;
- existing retry/terminal decision rules from `decide_mailbox_run`.

This is repository-local synthetic evidence, not real provider evidence.

## Stable Failure Mapping

The Cloudflare application adapter maps existing storage/provider failures into stable inward classes:

- missing binding/job -> `NotFound`;
- version mismatch -> `VersionConflict`;
- revoked/not-due/attempts-exhausted/retry-time-invalid -> `InvalidState`;
- unique conflict -> `Conflict` with exact replay recheck in the use case;
- D1 integrity/governance failures -> `IntegrityFailure`;
- aggregate/SQLite/idempotency overflow -> `InternalFailure`;
- provider/dependency failures -> `DependencyUnavailable`;
- provider `InvalidJobState` -> `InvalidState`.

The Worker maps only these application classes to the established HTTP problem taxonomy.

## CI Enforcement

Permanent evidence is layered rather than relying on a single test:

- `check-capability-module-layout.py` requires dedicated mailbox-job port/use-case ownership;
- `check-mailbox-job-worker-application-boundary.py` forbids D1/idempotency/provider orchestration in `mailbox_jobs.rs`, requires application calls and composition wiring, and verifies provider/D1 ownership in the Cloudflare adapter;
- the negative job-boundary fixture proves direct D1/provider orchestration is rejected;
- `check-mailbox-binding-worker-application-boundary.py` continues to protect the independent binding vertical;
- pure fake-port tests prove replay/read/provider/write ordering and checked version semantics;
- Cloudflare adapter tests prove failure mapping;
- Worker native/WASM and release builds prove composition;
- mailbox D1 invariant tests prove metadata-only lifecycle/replay/atomicity;
- Cross-Component Acceptance proves the composed synthetic mailbox flow and UI boundary.

## Remaining A0 Work

This slice does not migrate profile-generation handlers or remaining governance orchestration. Those remain separate bounded Phase 0 work.

No real mailbox-provider integration, queue/scheduler expansion, public API feature change, or production-readiness promotion is included.

`production_ready=false` remains unchanged.
