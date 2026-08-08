# Browser Profile Platform — Development Plan Amendment

**Status:** normative amendment to `DEVELOPMENT_PLAN.md`  
**Date:** 2026-08-08  
**Tracking:** issue #94  
**Production readiness:** unchanged; this document is planning only

## 1. Authority And Scope

This document amends the current `DEVELOPMENT_PLAN.md` after the architecture/product review recorded in issue #94. Where this document conflicts with the older plan on the topics below, this later amendment takes precedence until the next documentation-consolidation slice folds the changes back into the main plan.

The amendment covers:

- application use-case modularity;
- frontend feature-boundary enforcement;
- documentation navigation;
- outbox retry/DLQ policy;
- users-first discovery;
- client-scoped mailbox message search and full message-body access;
- CRM isolation wording.

No repository-local plan item is an implementation or production-readiness claim.

## 2. Product Search Decision — Keep It Simple

The product has two different search experiences and must not force them into one over-engineered indexing system.

### 2.1 Global catalog search

Global search is for finding business/control-plane objects quickly:

- clients;
- profiles;
- members/users, subject to owner/admin policy;
- mailbox bindings/status.

Devices are not a primary business-search object. Device lookup remains available in the infrastructure/admin surface, but `/users` / Members & Access is higher priority for ordinary administration.

The global search query remains a read-model/query-service concern. Tenant scope and live authorization are applied before result construction.

### 2.2 Client-scoped mailbox message search

Mailbox message search is intentionally client-scoped.

Primary UX:

```text
Client
  -> Mail
     -> search messages
     -> results
     -> open message
     -> read full body
```

An authorized user must be able to search messages associated with the selected client by at least:

- subject;
- sender;
- recipient;
- message body text;
- date/time filters.

Search results return a bounded projection such as opaque message reference, mailbox reference, subject, sender/recipient display fields, timestamp and snippet. Selecting a result loads the message detail and exposes the full authorized message body.

The initial product does **not** require a tenant blind-index, n-gram index or a central D1 full-text copy of all mail. Those may be introduced later only if measurements show they are needed.

## 3. Mailbox Message Application Contract

Add provider-neutral application/query contracts conceptually equivalent to:

- `SearchClientMailboxMessages`;
- `GetClientMailboxMessage`.

The public API is client-scoped. Exact URL spelling remains owned by the versioned API/control-plane contract, but the logical contract is:

```text
search messages for client_id + query/filter + cursor
get one message for client_id + opaque message reference
```

Mandatory ordering:

```text
resolve authenticated actor
  -> resolve tenant + live membership/grants
  -> authorize client/mailbox access
  -> resolve only mailbox bindings eligible for that client
  -> call Mailbox Provider / Bridge query adapter
  -> build bounded result or message detail
  -> return to UI
```

Authorization happens **before** provider search/body retrieval. The frontend never receives a larger foreign result set and filters it locally.

Provider implementation stays behind the adapter:

- Gmail/API/IMAP adapters should use provider-native search/fetch where available;
- browser-only providers may execute the same application contract through Profile Bridge/Camoufox;
- when a supported provider has no useful native body search, the adapter may perform a bounded fetch/search behind the same port;
- optimization with a local or cloud index is an implementation detail and requires separate evidence/ADR only when introduced.

The application API and UI must not change depending on which search technique an adapter uses.

## 4. Full Message Body Is A Supported Product Projection

Full mailbox message body is allowed to be displayed to an authorized user. It is not prohibited product data.

Rules:

- message body is treated as `CONFIDENTIAL` content and may contain higher-sensitivity material such as OTPs, reset links or credentials;
- initial cloud architecture does not make the full body canonical D1/R2 data; fetch it on demand through the authorized provider/Bridge path;
- the body may transit authorized Worker/Bridge memory and the HTTPS response required to display it;
- full body and attachments never enter ordinary logs, audit events, metrics, realtime/integration events, correlation fields or support bundles;
- the frontend must not persist the body in `localStorage`/`sessionStorage`;
- HTML mail must be sanitized/sandboxed before rendering; remote images/external active content are disabled by default;
- message identifiers exposed to the product are opaque/provider-scoped references, never subject/email-derived technical IDs;
- attachments are a separate capability and are not required by this amendment.

A future encrypted cache or centralized search index is permitted only as a separate storage/security decision. It is not a prerequisite for message search or body viewing.

## 5. Phase 0 Amendment — Modularity And DX Before Expansion

Phase 0 keeps the existing Worker-thinness work and adds three mandatory convergence outcomes.

### 5.1 Use-case Cargo boundaries

Capability source files inside one `crates/use-cases` crate are not the final growth boundary. Before search/realtime/CRM expansion makes the application layer significantly larger, split independent application capabilities into workspace crates with explicit dependency sets.

Target direction:

```text
crates/
  use-cases-identity/
  use-cases-clients/
  use-cases-profiles/
  use-cases-mailboxes/
  use-cases-notifications/   # introduced with Phase 1
  use-cases-search/          # introduced with Phase 3
  use-cases-devices/         # introduced with Phase 5
  use-cases-crm-projection/  # introduced with Phase 9
```

A temporary compatibility facade may re-export during migration, but it must not become the permanent owner of cross-domain orchestration.

Acceptance:

- domain-oriented application crates compile/test independently;
- no circular capability dependencies;
- provider SDKs remain outside all use-case crates;
- parallel feature work does not require editing one central application `lib.rs` for ordinary capability changes.

### 5.2 Frontend feature-boundary CI gate

The existing Feature-Sliced rule becomes executable CI policy:

- a feature may not import sibling feature internals;
- cross-feature composition occurs through `entities`, `shared`, route/app composition or an explicitly public feature API;
- generated API/event contracts remain under shared/generated ownership;
- CI contains a positive repository check and a deliberately forbidden cross-feature fixture.

Implementation may use ESLint boundary tooling or a repository-local checker; the invariant matters more than the plugin choice.

### 5.3 Documentation navigation

Add and maintain `docs/INDEX.md` with explicit status classes:

- **Normative target/current plan**;
- **Capability specification**;
- **Accepted/historical**;
- **Evidence/runbook/external**.

`DEVELOPMENT_PLAN.md` remains the main execution plan; amendments are temporary and must be folded back during documentation consolidation.

## 6. Phase 1 Amendment — Retry, DLQ And Recovery

Outbox/Queue consumers require explicit bounded failure handling in addition to idempotency.

Every asynchronous consumer must define:

- deterministic attempt accounting;
- exponential backoff with bounded jitter;
- maximum automatic attempt policy;
- transition to a Dead Letter Queue or equivalent terminal failure lane after exhaustion;
- sanitized alerting/operational visibility;
- owner/operator-safe replay procedure;
- replay idempotency so manual recovery cannot duplicate the business effect.

Acceptance adds:

- repeated transient failures back off instead of hot-looping;
- poison messages reach DLQ/terminal state after the configured bound;
- DLQ payloads contain no prohibited secrets/PII;
- replay after remediation is idempotent and auditable.

## 7. Phase 3 Amendment — Read Models And Search

Phase 3 is split into two simple query surfaces.

### 7.1 Global search

Conceptual endpoint:

```text
GET /api/v1/search?q=<query>&types=client,profile,member,mailbox
```

Global search covers:

- client display/searchable fields allowed by disclosure policy;
- exact contact lookup where already supported;
- profile label/status/assignment projection;
- member/user identity projection allowed to the current actor (owner/admin scope);
- mailbox binding/provider/status metadata.

Devices remain an infrastructure/admin list/filter capability rather than a primary global-search result type.

### 7.2 Client mailbox message search

Phase 3 adds the provider-neutral query contracts, API shapes, authorization tests and synthetic/fake acceptance for `SearchClientMailboxMessages` and `GetClientMailboxMessage`.

Supported search semantics include subject, sender, recipient and body text. The product must support full body retrieval for a selected result.

No mandatory D1 full-message index is introduced in Phase 3.

Acceptance:

- cross-tenant and ungranted clients produce neutral/no-leak results;
- provider search is not called before authorization succeeds;
- a user cannot use a message reference from another client/mailbox to retrieve a body;
- body-search result counts do not reveal inaccessible client mail;
- full body retrieval works in the synthetic adapter without logging/auditing the body;
- global member search is owner/admin-policy constrained.

## 8. Phase 4 Amendment — Cloud Mailbox Search And Body Fetch

Phase 4 Cloud mailbox adapters implement the same message query contract used by Phase 3.

For Gmail/API/IMAP-capable providers:

- search uses provider capabilities when practical;
- result mapping is bounded and provider-neutral;
- selected message detail returns the authorized full body;
- provider payload/body is not written into ordinary audit/outbox/realtime events;
- message body is not required to be stored canonically in D1/R2.

Mailbox job/check logic and interactive message search are separate use cases even when they share a provider adapter.

Acceptance adds:

- provider fake demonstrates subject/sender/recipient/body search;
- full message body can be fetched by authorized client context;
- malformed/provider HTML cannot execute active content in the product viewer;
- unauthorized access is rejected before provider fetch.

## 9. Phase 5 Amendment — Browser-Lane Message Search

For providers that require an authorized browser profile, Profile Bridge/Camoufox implements the same `SearchClientMailboxMessages` / `GetClientMailboxMessage` contract behind the Bridge/provider adapter.

The cloud API does not expose whether the result came from provider-native search, bounded adapter search or browser-lane search.

Offline/busy device behavior is explicit (`PENDING_DEVICE`, `PROFILE_BUSY` or the accepted equivalent) rather than returning false empty search results.

## 10. Phase 7 UI Amendment — Users First, Mail On Client

Information-architecture priority:

1. Clients / Profiles for business work;
2. Users & Access for people, permissions and ownership;
3. Mail on the client card for message work;
4. Mailboxes for binding/provider/job administration;
5. Devices as infrastructure/technical administration.

Client Detail adds a first-class `Mail` tab:

- search input;
- filters (mailbox/date/direction where available);
- result list with subject/sender/time/snippet;
- message detail with full body;
- safe loading/offline/auth-required states.

Users & Access adds search/filter by member/actor identity and role/grant projection.

Devices remain searchable/filterable inside the Devices administration screen but are not promoted above Users in primary business navigation.

Frontend body-viewer rules:

- no body persistence in browser storage;
- sanitized/sandboxed HTML or safe text rendering;
- remote images/external active content off by default;
- no message body in telemetry/error reporting.

## 11. Phase 8 / Product Acceptance Amendment

Standalone acceptance additionally proves:

- owner/admin can find a member/user and inspect the allowed grant projection;
- an authorized user opens a client and searches that client’s mail by subject/sender/body text;
- a selected result displays the full message body;
- an ungranted client/mailbox yields no result/body disclosure;
- a foreign opaque message reference cannot bypass client/mailbox authorization;
- message body is absent from audit/outbox/realtime/log/telemetry evidence;
- Cloud provider and browser-lane fakes satisfy the same application query contract.

## 12. CRM Isolation Clarification

CRM integration remains event/contract isolated:

- Profile Platform domain/application code does not import CRM entities or tables;
- durable integration events/projections are the default cross-system synchronization mechanism;
- CRM adapters may translate explicit user commands when CRM owns a field after cutover;
- this does not require every HTTP acknowledgement to be asynchronous; it requires that the Profile Platform core never depend on CRM implementation details.

`party_ref` / external references remain opaque integration references, not profile-domain concepts.

## 13. Revised Near-Term Execution Order

The execution order is now:

1. finish bounded Phase 0 Worker application-boundary convergence;
2. complete Phase 0 application Cargo-boundary split, frontend feature-boundary gate, generated TS contracts and docs index/consolidation;
3. Phase 1 event/outbox foundation including retry + DLQ + replay policy;
4. Phase 2 Client Registry 2.0;
5. Phase 3 global read-model search + client-scoped mailbox message query contract;
6. Phase 4 Cloud mailbox provider search/body retrieval;
7. Phase 5 browser-lane/device implementation for providers requiring Camoufox;
8. Phase 6 realtime;
9. Phase 7 complete UI including Client → Mail and Users-first administration;
10. Phase 8 cross-component product acceptance;
11. Phase 9 CRM boundary/cutover;
12. Phase 10 external production evidence.

The first implementation goal remains architectural convergence. This amendment changes the planned product/query contract; it does not justify bypassing the Phase 0 gates.