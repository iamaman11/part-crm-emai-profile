# Phase 2E acceptance-candidate status

Phase 2E is implemented as an acceptance candidate. This document distinguishes implementation-complete deterministic work from environment-dependent External evidence and intentionally does not claim production readiness.

## Deterministic implementation coverage

The candidate contains:

- mailbox-domain decomposition behind the compatibility facade and extracted `use-cases-mailboxes` ownership;
- provider-neutral scheduled lifecycle, canonical retry/failure decisions, authentication-required and suspended states;
- additive D1 lifecycle migration and v2 governed mailbox-run journal;
- real Gmail API and IMAP scheduled outer adapters with bounded provider observations;
- a fixed internal `MAILBOX_SECRET_RESOLVER` binding so persisted `SecretHandle` values remain opaque dynamic handles rather than per-mailbox Cloudflare bindings;
- Phase 1B-style scheduled dispatch using Cloudflare Queue, durable dispatch metadata, execution leases/fences, bounded retry, and DLQ configuration;
- deterministic execution identity so duplicate Queue delivery converges on the same canonical command/idempotency path;
- a real `ClientMailProviderQueryPort` implementation for Gmail API and IMAP behind the accepted Phase 2D authorization -> eligibility -> provider sequence;
- Gmail provider-scoped cursors/references and bounded transient body decoding;
- IMAP UIDVALIDITY-scoped cursors/references, bounded backward UID windows, implicit TLS/STARTTLS, synchronizing UTF-8 search literals, bounded MIME/body parsing, and attachment exclusion;
- privacy boundaries preventing mailbox credentials and message subject/sender/recipient/body from entering mailbox coordination D1 tables, Queue payloads, command evidence, audit/outbox, or telemetry paths;
- permanent Phase 2E architecture/privacy policy checks plus D1 duplicate/fencing proof.

The permanent Quality Gate remains the authoritative machine verdict for formatting, Clippy, native tests, Cloudflare adapter tests, Worker WASM compilation, architecture boundaries, migrations, privacy checks, and generated consistency on the exact candidate source revision.

## External evidence still required

The following are deliberately not synthesized or inferred from repository-local tests:

- real Gmail API scheduled execution and accepted Client Mail search/get against a deployed non-production test account;
- real IMAP scheduled execution and accepted Client Mail search/get against a remote TLS/STARTTLS server;
- real invalid/expired credential behavior reaching canonical `AUTH_REQUIRED` without credential exposure;
- real provider failure/rate-limit behavior where it can be safely and reproducibly exercised;
- deployed Queue duplicate-delivery/fencing behavior on the same candidate source revision;
- deployed negative privacy inspection showing no credentials or message content in durable/telemetry evidence surfaces.

The exact evidence requirements are defined in `docs/evidence/phase2e-cloud-mailbox.md`. These records belong only in the repository's External evidence system and must identify the immutable source revision and environment while remaining redacted.

## Readiness

`production_ready` stays `false`. Phase 2E must remain draft/not accepted until the exact candidate source revision passes all permanent repository gates and the required real Gmail API and IMAP External evidence is captured and accepted. Phase 2F browser/device/Camoufox/Profile Bridge runtime remains out of scope for this phase.
