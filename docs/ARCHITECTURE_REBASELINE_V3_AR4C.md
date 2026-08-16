# Architecture Re-baseline v3 — AR-4C Outbound Mail composition extraction

Status: **CANDIDATE — NOT ACCEPTED UNTIL POST-MERGE CLOSEOUT**

Parent program: #266  
Bounded implementation issue: #286  
Accepted baseline: `bf323fbb8af160471299cdf30f0fcf406fe0457d` (`main`, AR-4B accepted)

## Purpose

AR-4C closes the remaining Outbound Mail transport-composition debt identified by the accepted AR-3 application architecture contract.

The remediation is structural and behavior-neutral: `apps/control-plane-worker/src/client_mail_send.rs` remains the HTTP transport owner, while concrete D1/query/provider construction and provider selection move behind the control-plane composition boundary.

## Candidate composition ownership

The accepted existing composition seams are reused for:

- Client Mail eligibility: `composition::client_mail_eligibility_repository`;
- query authorization: `composition::query_repository`;
- source-message query provider: `composition::client_mail_query_provider`.

AR-4C adds composition-owned seams for:

- outbound intent persistence: `composition::outbound_mail_intent_repository`;
- concrete outbound provider selection: `composition::client_mail_outbound_provider`;
- the Gmail/SMTP/unsupported provider router, implemented under `apps/control-plane-worker/src/composition/outbound_mail.rs`.

The transport no longer owns `D1ClientMailboxEligibilityRepository`, `D1QueryRepository`, `CloudMailboxQueryAdapter`, `D1OutboundMailIntentRepository`, `D1MailboxRepository`, `CloudflareGmailOutboundMailProvider`, `CloudflareSmtpOutboundMailProvider`, or mailbox-provider routing.

## Preserved semantics and authority

AR-4C does **not** change:

- public HTTP paths, methods or `RouteClass` identities;
- request/response/OpenAPI semantics;
- outbound-mail intent/state-machine, replay, idempotency or ambiguity semantics;
- Gmail or SMTP provider behavior/enablement;
- the unsupported treatment of Browser Fallback / Microsoft Graph in this outbound path;
- mailbox credential/OAuth policy;
- D1 schema or migration history;
- Cloudflare resources, Wrangler bindings or runtime topology;
- capability activation or production/provider state.

Application policy remains owned by `crates/use-cases-mailboxes::outbound_mail`; provider contracts remain owned by `crates/application-ports::outbound_mail`.

## Permanent fitness enforcement

The canonical architecture checker must make the extraction fail closed:

1. `client_mail_send.rs` must call the accepted composition seams and must not contain `cloudflare_adapters::` imports;
2. direct D1/query/Gmail/SMTP adapter constructors are forbidden in the transport;
3. the composition-owned Outbound Mail module must contain the intent-repository and provider-router wiring;
4. a negative fixture must reintroduce concrete transport adapter construction and be rejected;
5. the obsolete transport-owned `client_mail_send/provider.rs` module must not return.

## Candidate machine state

Until the separate closeout step accepts the merged implementation:

- `accepted_through = AR-4B`;
- candidate slice = `AR-4C`;
- candidate status = `OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE`;
- AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it;
- next required slice after AR-4C acceptance = `AR-5`;
- `architecture_complete = false`;
- Production Core remains `BLOCKED`;
- `production_ready = false`;
- production/provider mutation remains forbidden.

## Verification contract

The candidate must pass, at minimum:

```text
cargo fmt --all -- --check
cargo test --locked -p control-plane-worker
python scripts/generate-architecture-inventory.py --check
python scripts/generate-architecture-inventory.py --self-test
python scripts/check-contract-compatibility.py
```

Acceptance additionally requires the full applicable permanent PR workflow set green on one unchanged exact human-authored head, `behind_by=0`, no blocking reviews, no unresolved review threads, guarded merge, and a separate post-merge authority closeout before AR-5 begins.

## Non-goals

AR-4C does not implement AR-5 Wrangler/runtime cleanup, AR-8 credential/OAuth refresh concurrency, new mailbox providers, capability activation, production provisioning/deployment, provider mutation, D1 migration changes or unrelated cleanup.
