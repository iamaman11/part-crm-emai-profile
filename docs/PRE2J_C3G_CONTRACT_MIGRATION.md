# Pre-2J C3G Microsoft Graph provider contract migration

**Status:** proposed authority for issue #226; implementation remains issue #225.  
**Decision base:** accepted C3 `7392581fb4ea0eb40bf2317c34b8fb7f151ca669`.  
**Umbrella:** #203 / Batch C #214.  
**Production readiness:** remains `false`; Phase 2J remains blocked.

## Decision

C3G will represent Microsoft Graph as a first-class durable mailbox execution provider named `MICROSOFT_GRAPH`.

It must **not** be hidden behind `IMAP`. The existing `MailboxProvider` value is an execution discriminator across domain state, D1 persistence, Worker routing and the public mailbox contract. Reusing `IMAP` for Graph would make durable/public state semantically false and would couple resolver metadata to hidden routing policy.

A separate `/api/v2` mailbox island is also rejected for this bounded pre-production change. The repository has not entered Phase 2J and `production_ready=false`; therefore this is the controlled window to perform one explicitly governed provider-enum migration before any production compatibility certification.

## Governed v1 compatibility exception

The historical baseline remains byte-immutable. Normal compatibility policy classifies widening a response enum as breaking, and that default remains unchanged.

Issue #226 authorizes exactly one pre-production exception after the authority is first accepted on `main`:

- existing artifact: `openapi/v1/fragments/mailboxes.json`;
- schema: `MailboxProviderDto`;
- accepted values remain `GMAIL_API`, `IMAP`, `BROWSER_FALLBACK`;
- one and only one appended value may be added: `MICROSOFT_GRAPH`;
- no existing provider value or meaning may change;
- no other existing v1 schema/path/enum may change;
- `contracts/baseline/**` and `proto/**` remain byte-immutable.

The implementation may additionally add exactly one new previously absent fragment:

`openapi/v1/fragments/mailbox-microsoft-graph-onboarding.json`

The generic compatibility checker may be taught to waive only the exact `MailboxProviderDto +MICROSOFT_GRAPH` response-enum widening when the accepted C3G authority is present. That waiver must have negative self-tests proving that a second token, another schema, another response enum or any other contract drift still fails.

The one-shot C3G checker owns the complete `openapi/v1` diff while the migration is pending. After consumption, both the migrated mailbox fragment and the new Graph fragment become immutable and C3G moves to invariant-only mode for any later separately governed authority.

## Runtime and persistence consequences

C3G implementation must add the same provider value consistently across the inner provider model, D1 persistence constraints, provider routing and generated/public projection. Existing D1 migration history is immutable; any schema change is an append-only new migration with repository schema tests.

The implementation must not create a parallel mailbox aggregate or a hidden Graph-only authorization path. Existing mailbox binding, mailbox-to-Client association and grant-safe Client Mail sequencing remain authoritative.

## OAuth and secret boundary

`MAILBOX_SECRET_RESOLVER` remains the sole credential authority/storage boundary.

C3G initial delegated permissions are limited to:

- `https://graph.microsoft.com/Mail.Read`;
- `offline_access`;
- only the OIDC scopes required by the accepted authorization ceremony.

`Mail.Send` is forbidden in C3G until C4 accepts the provider-neutral outbound send/retry/reconciliation model.

Authorization codes, PKCE material, client secrets, access tokens, refresh tokens and Graph bearer material remain transient/resolver-owned. They must not enter D1 business rows, audit/outbox, browser storage, mailbox association state, ordinary logs or domain-readable state. The application sees only the existing opaque `SecretHandle` and provider-neutral execution contracts.

## Adapter boundary

`MicrosoftGraphMailboxAdapter` is an outer adapter. Microsoft Graph REST/OData/provider DTOs and provider error payloads do not cross into domain/use-case public APIs.

Read/search/get operations translate to the existing provider-neutral mailbox observation/query model. Graph paging/delta links remain opaque provider cursor material. `429` obeys `Retry-After`; retryable network/5xx failures use bounded existing mailbox-job retry/fencing semantics; terminal authorization failure maps to the existing `REAUTH_REQUIRED` lifecycle.

## Acceptance sequence

1. Accept issue #226 authority PR with **no** `openapi/v1/**`, D1, provider enum or runtime Graph implementation change.
2. Start #225 from that exact accepted `main`.
3. Consume the one-shot migration exactly once: `MailboxProviderDto +MICROSOFT_GRAPH` plus the one Graph onboarding fragment.
4. Add append-only D1/provider/runtime implementation and permanent positive/negative evidence.
5. Require all 12 permanent workflows `completed/success` on one exact implementation head.
6. Require `behind_by=0`, clean reviews/threads/Conversation, Ready/post-Ready rechecks and guarded squash merge.
7. Keep Phase 2J blocked and `production_ready=false` throughout.
