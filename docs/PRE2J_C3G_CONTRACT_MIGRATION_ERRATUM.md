# Pre-2J C3G contract-authority schema-key erratum

**Status:** proposed narrow erratum for issue #228.  
**Accepted authority:** #226 / PR #227.  
**Implementation:** #225 remains blocked on acceptance of this erratum.  
**Decision base:** `cf07db195db9a85e5e3ae3a0346528e3eacd39bb`.  
**Production readiness:** remains `false`; Phase 2J remains blocked.

## Defect

The accepted C3G authority correctly governs the existing artifact
`openapi/v1/fragments/mailboxes.json`, but it names the schema inside that artifact as
`MailboxProviderDto`.

The accepted/frozen artifact actually exposes the provider enum at
`components.schemas.MailboxProvider`. `MailboxProviderDto` is the Rust contract DTO/exporter
name and is not a schema key in this frozen fragment.

Because the accepted C3G checker copied the same mistaken schema key, the authorized
`+MICROSOFT_GRAPH` one-shot migration cannot be consumed against the accepted base while the
checker remains fail-closed.

## Narrow correction

This erratum changes **only the machine interpretation of the schema key inside the already
authorized existing artifact**:

- authority-recorded schema name: `MailboxProviderDto`;
- actual frozen artifact schema key: `MailboxProvider`;
- artifact remains `openapi/v1/fragments/mailboxes.json`;
- accepted values remain `GMAIL_API`, `IMAP`, `BROWSER_FALLBACK`;
- the only authorized appended value remains `MICROSOFT_GRAPH`;
- the only authorized new v1 artifact remains
  `openapi/v1/fragments/mailbox-microsoft-graph-onboarding.json`.

The accepted `architecture/pre2j-c3g-contract-authority.json` record is deliberately left
byte-unchanged. This erratum does not reopen or broaden the substantive contract decision.

## Preserved boundaries

The correction does not authorize any OpenAPI, D1, domain, Worker, adapter, OAuth, secret,
frontend, baseline, or proto implementation change in the erratum PR itself.

All C3G security/product boundaries remain unchanged:

- C3 IMAP/SMTP remains a distinct supported lane;
- `MICROSOFT_GRAPH` remains the first-class Graph execution discriminator;
- `MAILBOX_SECRET_RESOLVER` remains the sole credential authority;
- C3G remains read-only with delegated `Mail.Read` + `offline_access` and only required OIDC
  scopes;
- `Mail.Send` remains deferred until C4;
- Phase 2J remains blocked and `production_ready=false`.

## Machine enforcement

`scripts/check-pre2j-c3g-contract-authority.py` must:

1. continue validating the original accepted #226 authority exactly as recorded;
2. validate this erratum exactly;
3. keep the accepted authority bytes immutable;
4. while the erratum is not yet in the base branch, allow only this erratum JSON, this document,
   and the checker itself to change;
5. require the erratum to be accepted in the base before any C3G OpenAPI migration can be
   consumed;
6. validate the one-shot provider widening against the actual
   `components.schemas.MailboxProvider` key;
7. self-test the real frozen fragment shape and reject the old mistaken
   `MailboxProviderDto` fragment-key assumption;
8. preserve the existing one-shot and post-consumption immutability rules.

## Resume condition

After this erratum passes permanent CI and is accepted on `main`, #225 may resume from that exact
accepted head. The C3G implementation branch must not consume the public provider migration before
that point.
