# Mailbox Binding Application Boundary

**Status:** Phase 0 reference pattern for mailbox binding create / visible-by-ID / revoke.

**Scope:** repository architecture only. Mailbox job create/get/run is now an independent application-boundary vertical documented in `MAILBOX_JOB_APPLICATION_BOUNDARY.md`. This document does not claim production readiness or real provider readiness.

## Purpose

Mailbox binding lifecycle is moved from Worker-owned D1/idempotency orchestration to an application-owned command/query path:

```text
HTTP / Workers SDK
  -> apps/control-plane-worker/src/mailbox_bindings.rs
     authenticated actor + strict protocol parsing + response mapping
  -> crates/use-cases/src/mailboxes.rs
     owner authorization + replay/write/query/version sequencing
  -> crates/application-ports/src/mailboxes.rs
     provider-neutral mailbox binding application contract
  -> crates/cloudflare-adapters/src/d1_mailbox_bindings.rs
     D1/idempotency adapter
  -> existing atomic D1 mailbox binding mutations/queries
```

Concrete D1 construction belongs to `apps/control-plane-worker/src/composition.rs`. The migrated `mailbox_bindings.rs` transport must not instantiate `D1MailboxRepository`, `D1IdempotencyRepository`, `CreateMailboxBindingMutation`, `RevokeMailboxBindingMutation`, or a D1 mutation envelope.

Mailbox job routes remain physically separated in `apps/control-plane-worker/src/mailbox_jobs.rs` and now use their own application boundary. Binding and job policies are intentionally independent so either capability can fail closed without reopening the other.

## Security And Disclosure Invariants

The binding vertical preserves the accepted fail-closed behavior:

1. active actor resolution occurs before binding route handling;
2. only a tenant owner may use binding create/get/revoke; non-owner disclosure remains neutral `not_found`;
3. malformed binding IDs remain disclosure-neutral after actor/owner resolution;
4. create/revoke request DTOs deny unknown fields;
5. `requestDigest` remains exactly 64 lowercase hexadecimal characters;
6. the command may carry a typed opaque `SecretHandle`, but application read models and HTTP responses contain no secret handle;
7. password/message-body style fields are rejected rather than accepted as loose JSON;
8. provider/storage errors are translated into stable application classes and are not relabeled as business `not_found` or ordinary conflict.

## Create Ownership

The application use case owns:

- tenant-owner authorization intent;
- construction of the typed `MailboxBinding` aggregate;
- exact idempotency replay before write;
- concurrent unique-conflict exact replay recheck;
- fresh result version `1`;
- stable failure taxonomy.

The D1 adapter maps the typed binding and `CommandExecutionEvidence` into the existing atomic `CreateMailboxBindingMutation`. No SQL or schema is moved inward.

The HTTP transport preserves the existing contract: both fresh create and exact replay respond with `201` and the existing camelCase mutation receipt.

## Revoke Ownership

The application use case owns:

- tenant-owner authorization intent;
- typed expected aggregate version;
- checked `AggregateVersion::next()` before replay/write, so overflow cannot saturate or wrap;
- exact idempotency replay;
- concurrent conflict recheck;
- mailbox-specific missing/version/invalid-state taxonomy.

The D1 adapter retains the existing atomic revoke transaction. Fresh revoke and exact replay preserve HTTP `200`.

## Visible Binding Query

The application read model contains only:

- binding ID;
- provider;
- binding status;
- aggregate version.

`SecretHandle` is deliberately absent from the read model and the transport response. The response keeps the existing `bindingId`, `provider`, `status`, `version` field names.

## Durable Command Evidence

The vertical reuses `application-ports::CommandExecutionEvidence` for:

- idempotency key;
- strict request digest after transport validation;
- deterministic audit event ID;
- deterministic outbox event ID;
- command timestamp;
- idempotency expiry timestamp.

The adapter maps this inward evidence to the existing D1 `MutationEnvelope`; the use case never imports Worker/D1 SDK types.

## CI Enforcement

`check-mailbox-binding-worker-application-boundary.py` is the permanent fail-closed architecture policy for this slice. It proves that:

- mailbox binding routes enter `mailbox_bindings::dispatch`;
- mailbox job routes remain present and enter `mailbox_jobs::dispatch`;
- the binding transport contains no direct D1/provider mutation orchestration;
- the application port/use cases/D1 adapter exist in the accepted dependency direction;
- the old mixed `apps/control-plane-worker/src/mailboxes.rs` cannot return;
- a negative fixture that restores both direct D1 transport and the legacy mixed file is rejected.

Mailbox-job D1/provider ownership is enforced separately by `check-mailbox-job-worker-application-boundary.py`. Pure fake-port tests, D1 adapter tests, Worker native/WASM checks, mailbox D1 atomicity tests and cross-component acceptance remain separate evidence layers.

## Remaining A0 Work

Mailbox job create/get/run has its own application boundary and no longer belongs to the remaining work for this binding slice.

Generation and remaining governance orchestration remain separate Phase 0 work.

`production_ready=false` remains unchanged.
