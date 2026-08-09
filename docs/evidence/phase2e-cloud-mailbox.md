# Phase 2E cloud mailbox evidence contract

Phase 2E keeps deterministic repository evidence separate from evidence that can only be produced by a real external mailbox provider and deployed Cloudflare bindings.

## Repository-local evidence

Repository-local checks may prove all of the following without claiming that a real provider was exercised:

- mailbox domain and application crates remain free of Cloudflare/provider runtime dependencies;
- the accepted Phase 2D authorization -> eligibility -> provider ordering remains unchanged;
- Queue envelopes and D1 coordination tables contain only opaque identifiers, versions, fences, and timestamps;
- duplicate delivery, active execution leases, expired-lease fencing, stale completion, bounded retry, and DLQ configuration are deterministic;
- the fixed `MAILBOX_SECRET_RESOLVER` binding is the only runtime secret-resolution surface and dynamic `SecretHandle` values are not Cloudflare binding names;
- Gmail/IMAP request construction, cursor/reference scoping, response bounds, MIME/body bounds, parsing, and negative privacy assertions are deterministic;
- no mailbox credentials or message subject/sender/recipient/body enter D1 mailbox coordination, audit/outbox evidence, Queue payloads, or telemetry paths.

The authoritative repository checks are `scripts/check-phase2e-mailbox-boundaries.py`, `scripts/test-mailbox-vertical-slice.py`, Rust unit tests, D1 migration replay, and the permanent Quality Gate. Passing them is necessary but is not real-provider evidence.

## External Gmail API evidence

A passing Gmail evidence record must come from a deployed non-production test environment using the real Gmail API and the deployed `MAILBOX_SECRET_RESOLVER` service binding. The record must demonstrate, on one immutable source revision:

1. a scheduled mailbox execution resolves an opaque secret handle, reaches the real Gmail API, and persists only the canonical bounded provider observation;
2. the accepted Client Mail search path returns real message summaries only after Phase 2D authorization and mailbox eligibility succeed;
3. the accepted Client Mail get path returns a real message body transiently without adding subject/sender/recipient/body or credentials to D1 coordination, audit/outbox, Queue payloads, or telemetry;
4. an invalid/expired credential reaches canonical `AUTH_REQUIRED` behavior without exposing the credential;
5. a provider rate-limit or an equivalent controlled provider failure reaches the canonical bounded retry/failure taxonomy;
6. a duplicate Queue delivery for the same job generation does not create a second canonical provider-result mutation.

Evidence must identify the provider as Gmail API, the source commit, environment, observed timestamps, and redacted provider/request identifiers. It must not include access/refresh tokens, full mailbox addresses unless explicitly safe test data, message bodies, or other message content.

## External IMAP evidence

A passing IMAP evidence record must come from a deployed non-production test environment using a real remote IMAP server and the deployed `MAILBOX_SECRET_RESOLVER` service binding. The record must demonstrate, on one immutable source revision:

1. implicit TLS or STARTTLS (whichever the test account is configured for), authenticated through the hardened shared IMAP session;
2. scheduled `STATUS INBOX (MESSAGES UIDNEXT)` execution and canonical bounded observation persistence;
3. accepted Client Mail search with UIDVALIDITY-scoped cursor/reference semantics and bounded UID windows;
4. accepted Client Mail get with bounded transient body parsing and no message content/credentials in durable or telemetry sinks;
5. authentication failure reaches canonical `AUTH_REQUIRED` behavior;
6. duplicate Queue delivery/fencing does not create a second canonical provider-result mutation.

Evidence must identify the provider as IMAP, source commit, environment, remote hostname in an appropriately redacted form, TLS mode, observed timestamps, and redacted mailbox/job identifiers. Passwords and message content are prohibited from evidence artifacts.

## Readiness rule

Phase 2E must not be declared externally verified merely because repository-local Quality, D1, architecture, or synthetic evidence passes. Gmail API and IMAP real-provider evidence remain environment-dependent External evidence. Until both required provider records are captured and accepted on the exact candidate source revision, `production_ready` remains `false` and Phase 2E remains an acceptance candidate rather than a production-readiness claim.
