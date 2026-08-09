# Architecture Map

**Status:** normative target architecture  
**Date:** 2026-08-09  
**For:** developers, reviewers, operators and future CRM integration work

This document defines stable architecture boundaries and invariants. It does **not** define
implementation order; current execution order lives in [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md).

## 1. System Boundary

Browser Profile Platform is a standalone product and future external CRM bounded context.
It owns browser profiles, generations, sessions, profile-client assignments, profile ACL,
certification, mailbox runtime integration and cloud/local lifecycle.

Before CRM integration, the standalone Client Registry owns client cards. After CRM cutover,
CRM Party/Customer Master may become authoritative for client master fields while this
platform keeps an opaque `party_ref`/external reference and profile-runtime ownership.

Cloudflare, D1, R2, Durable Objects, Windows, Python and Camoufox are adapters/runtime
technologies, never sources of domain policy.

## 2. Runtime Topology

```text
Browser through Cloudflare Access
  -> app.example.com
     -> Rust Worker /api/* /auth/* /bridge/*
     -> React SPA via Workers Static Assets
     -> D1 authoritative business/catalog data + audit/outbox/read projections
     -> per-profile Durable Object session coordinator
     -> Queues / Scheduled consumers
     -> R2 encrypted immutable objects
     -> Cloudflare secret/key providers

Web UI
  -> profilebridge://claim/<single-use-code>
     -> Windows Profile Bridge
        -> authenticated Worker/device protocol
        -> local SQLite cache/outbox
        -> local encrypted materialization/workspace
        -> typed local IPC
           -> Camouhost
              -> separate Camoufox process
```

Standalone v1 has no required VM/backend daemon, PostgreSQL or Keycloak. Workers executes
control-plane code only. Camoufox/browser filesystem/process supervision stays on Windows.

## 3. Deployment And URL Boundary

One Workers deployment serves the SPA and browser-facing API on one origin.

```text
https://app.example.com/             React SPA
https://app.example.com/profiles/*   SPA routes
https://app.example.com/clients/*    SPA routes
https://app.example.com/api/v1/*     Rust Worker API
https://app.example.com/auth/*       identity/bootstrap endpoints
https://app.example.com/bridge/*     device enrollment/claim endpoints
```

`/api/*`, `/auth/*` and `/bridge/*` are fail-closed Worker routes and must never fall through
to SPA assets on unknown methods/versions. Cloudflare Access authenticates browser users;
the Worker always applies live application membership/grants. Device-bound Bridge routes use
a separate proof/token policy and do not rely on browser cookies.

## 4. Allowed Dependency Direction

Clean Architecture direction is inward:

```text
primitives
contracts -> primitives
domains -> contracts + primitives
application-ports -> domains + contracts + primitives
use-cases -> application-ports + domains + contracts + primitives
adapters -> application-ports + domains + contracts + primitives + provider SDKs
apps -> use-cases + adapters + contracts + primitives
frontend -> generated public contracts + frontend shared/entities/feature public APIs
```

The important prohibition is **inner -> outer**. An outer adapter may depend on inner domain
value/state types when implementing an application port. Domain/use-case/application-port
code may not depend on concrete Cloudflare/Windows/React/provider implementations.

### `primitives`

Opaque IDs, tenant scope, safe path segments, digests, time/value types and provider-neutral
validation primitives. No storage/runtime/business workflow. `ContactPointId` is opaque and
PII-independent; email/phone/name/URL values are never technical resource identities.

### Domain crates

- `identity-access-domain`: tenant owner, memberships, grants and authorization decisions;
- `client-domain`: decomposed client/contact/assignment/merge rules behind a thin facade;
- `profile-domain`: profile/generation lifecycle policy;
- `session-domain`: launch intent, lease epoch, fencing, session and recovery states;
- `mailbox-domain`: provider-neutral mailbox binding/job/runtime-lane rules.

Domains contain pure decisions/state machines and compile without D1/Workers/Windows/Python.

### `application-ports`

Ports are owned by application needs and grouped by capability: identity, clients, profiles,
generations, sessions, mailboxes, notifications/search/devices/CRM as those capabilities are
introduced. Read and write capabilities are separated where useful.

Client contact persistence ports accept protected representations only; transient contact plaintext
may enter only the application/protection boundary and cannot cross the persistence interface by type.

Phase 2B accepts authoritative protected client-contact D1 persistence and a separate exact-lookup
query port. Exact lookup accepts tenant scope + contact kind + normalization version + versioned HMAC
token only; D1 lookup adapters do not receive plaintext and do not decrypt/scan all contact rows.

Phase 2C accepts deterministic one-way client merge and historical primary assignment semantics,
application-owned merge/reassignment sequencing, governed atomic D1 merge/history, and grant-safe
Client Registry read projections. Assignment remains explicitly non-authorizing; source grants are
removed rather than transferred on merge; revoked access disappears before projection construction.

The current plan may keep `application-ports` as one Cargo crate with capability modules while
that remains clear. It is not required to split every port into a separate crate.

### Use cases

One application command/query owns one workflow. A use case:

1. receives verified actor/tenant/request context;
2. applies live authorization intent;
3. loads state/projections through typed ports;
4. invokes pure domain decisions;
5. sequences idempotency/versioning/repository work;
6. persists canonical state + audit/outbox atomically within one D1 boundary where possible;
7. initiates external side effects only after the durable transition that authorizes them;
8. returns stable provider-neutral results/problems.

Use cases never call concrete Cloudflare SDKs. High-growth independent application contexts
should become separate Cargo crates when that improves compile-time dependency isolation;
crate splitting is not an excuse for one-crate-per-function fragmentation.

Accepted independent application ownership includes `use-cases-identity`,
`use-cases-notifications` and `use-cases-clients`; shared `use-cases` compatibility re-exports do
not regain canonical ownership of those capabilities.

Phase 2D adds `use-cases-query` as the independent cross-capability read/search application context;
mutation aggregates remain owned by their existing capability use cases.

### Adapters

Adapters implement ports and map provider/storage/runtime behavior:

- Cloudflare: Access, D1, R2, Queues, Durable Objects, secrets;
- Windows: CNG/DPAPI, filesystem, process tree, custom protocol, updater;
- Camouhost: typed IPC to Python/Camoufox;
- mailbox providers: Gmail/API/IMAP/browser-backed providers;
- CRM: future Party/OIDC/PostgreSQL integration adapters.

Adapters may depend inward on domain/application types. They may not redefine domain policy
or expose provider SDK types across inner boundaries.

### Apps / ingress

`apps/control-plane-worker` and Durable Object ingress are protocol/composition surfaces:
parse/authenticate, construct adapters/context, call one application command/query, map the
typed result. Ordinary transports do not own D1 mutation construction, authorization policy,
idempotency workflow or provider-specific business sequencing.

`apps/profile-bridge` owns Windows-native device trust, materialization and process/runtime
composition. React owns presentation/navigation/remote-cache behavior only.

## 5. Core Data Ownership

| Aggregate/data | Authoritative owner | Storage/coordination boundary |
|---|---|---|
| Tenant/Membership/Grant | Identity & Access | D1 + audit/outbox |
| ClientRecord | Client Registry | D1 + audit/outbox |
| ClientContactPoint | Client Registry | D1 ciphertext + nonce/key-version metadata + tenant-first HMAC lookup index; audit/outbox remain metadata-only |
| Profile/Assignment | Profile Catalog | D1 + audit/outbox |
| Active generation pointer | Profile Catalog | D1 fenced/CAS activation after verification |
| Lease/session/fencing | Runtime Sessions | one Durable Object per profile + `session-domain` |
| Encrypted generation payload | Profile Storage | immutable R2 object |
| Certification result/evidence | Certification | D1 sanitized decision + governed evidence object |
| MailboxBinding/jobs/results | Mailbox Operations | D1 metadata; credential secret handle only |
| Mailbox message body | Provider/authorized transient projection by default | not canonical D1/R2 storage in initial design |
| Device local state | Profile Bridge | encrypted workspace + local SQLite cache/outbox |

D1 is the standalone business/catalog authority. Durable Object is **not** a parallel catalog;
it serializes session work, issues monotonic fencing and keeps minimal recoverable coordinator
state. Session transitions belong to `session-domain`, not `profile-domain`.

## 6. D1 / Durable Object / R2 Transaction Model

There is no distributed transaction across D1, DO and R2.

Generation lifecycle follows crash-safe saga/reconciliation semantics:

1. command carries idempotency + expected aggregate version;
2. per-profile coordinator serializes writer state and supplies fencing;
3. Bridge creates a new immutable encrypted generation object;
4. verifier checks manifest/digest/restore readability;
5. D1 compare-and-set activates only the expected generation/version/fencing outcome;
6. audit/outbox/event follows durable acceptance;
7. orphan/incomplete transitions are reconciled under bounded retention/recovery policy.

Forbidden:

- mutable active R2 key;
- “latest timestamp/object listing wins”;
- last-write-wins generation activation;
- deleting dirty local state before verified sync;
- stale fencing token activating/overwriting newer generation.

## 7. Durable Object / Session Boundary

A profile Durable Object is a distributed coordination adapter/runtime boundary for the pure
session state machine. It may keep only state needed for lease/session recovery and
serialization.

It must not become authoritative for:

- client cards;
- profile catalog metadata;
- grants;
- mailbox business catalog;
- CRM Party data.

D1 stores business projections and accepted session/generation outcomes. The Worker/DO ingress
should become thin transport/application composition; cleaning ingress does **not** move
lease/fencing state into `profile-domain`.

## 8. Authorization And Query Boundary

Cloudflare Access proves identity, not resource authorization. Every public application query
or command rechecks active tenant membership and required grants.

Authorization is applied before:

- list/search result construction;
- detail projection construction;
- realtime history/catch-up projection;
- CRM-facing projection;
- provider mailbox search;
- full message-body retrieval.

Frontend filtering is never an authorization mechanism. Foreign/missing resources use the
accepted neutral-disclosure behavior.

## 9. Mailbox Message Data Boundary

Mailbox message content is authorized product data, classified according to
`DATA_CLASSIFICATION.md`.

The default design is simple:

- client-scoped message search is an application query;
- provider/Bridge adapters perform search/fetch behind a provider-neutral port;
- full body is fetched on demand for an authorized view;
- no mandatory central D1 blind/full-text index;
- no canonical D1/R2 body copy by default;
- message body/subject/addresses do not enter ordinary logs/audit/realtime/events/support;
- HTML mail is sanitized/sandboxed; tracking images/active content disabled by default.

Any later central encrypted/full-text/blind index requires a separate threat/storage/retention
decision.

## 10. D1 Isolation Rules

Standalone isolation is explicit because D1 has no PostgreSQL RLS:

- first production deployment may be one organization with multiple users/default-deny grants;
- tenant-owned keys/uniqueness include `tenant_id`;
- normal repository/application APIs require typed tenant scope;
- raw unscoped D1 access is restricted to approved migration/reconciliation adapters;
- client exact-contact lookup is tenant-first and equality/index-backed on kind + normalization
  version + lookup-key version + HMAC token; plaintext/decrypt-all search paths are prohibited;
- UI/Bridge never receives D1 binding/direct storage URL;
- cross-tenant/IDOR negative tests cover every public capability;
- multi-tenant expansion beyond the accepted deployment model requires an explicit ADR or
  catalog-adapter strategy.

A future PostgreSQL CRM adapter may add FORCE RLS as defense in depth without changing domain
contracts.

## 11. Identity And Device Trust

- Cloudflare Access handles workforce login through approved IdP/email OTP;
- application membership/grants can revoke access independently of Access session;
- owner manages invitations/memberships/resource grants;
- Bridge enrollment binds a logged-in actor, single-use intent and device key pair;
- private device keys use approved Windows protection adapters;
- Bridge gets short-lived app authorization after proof-of-possession;
- long-lived bearer R2/device bucket credentials are forbidden.

## 12. Key Hierarchy

```text
Cloudflare secret root wrapping key (versioned)
  -> wrapped tenant KEK in governed metadata
     -> wrapped generation DEK
        -> AEAD encrypted immutable generation in R2
```

Client contact protection is a separate application/adaptor key domain from generation storage:
contact display encryption keys and exact-lookup HMAC keys are distinct, versioned domains. The
Phase 2A inner contract fixes separation, domain-separated lookup input and normalization metadata.
Phase 2B accepts authoritative protected client-contact D1 persistence, current+legacy versioned
keyring behavior, current-key writes, version-selected decrypt and lookup candidates across active
lookup-key versions so planned rotation/backfill does not require plaintext database scans.

Plain root/KEK/DEK/contact-encryption/HMAC key material never belongs in Git, D1, R2, logs, audit,
events or client bundles. Production promotion requires explicit rotation, recovery/escrow, restore
and operator-separation policy.

## 13. Events, Queues And Realtime

Queues are at-least-once; consumers are idempotent. Canonical mutation is durable before
notification.

Outbox consumers require bounded retries, maximum attempts, DLQ/terminal handling,
sanitized alerting and idempotent replay.

Realtime uses safe versioned event envelopes as invalidation/change signals. WebSocket state
is never canonical business state and never carries prohibited secrets/PII/mail bodies.

## 14. Frontend Boundary

Frontend rules:

- feature routes are composed through feature-owned public route APIs; root app routing does not import feature-internal workspace components;
- generated public API/event DTOs/enums are authoritative;
- TanStack Query owns remote state;
- business authorization/decisions remain server-side;
- sibling feature internals are not imported directly;
- cross-feature composition uses shared/entities/app/routes or explicit feature public APIs;
- high-impact server mutations are not optimistically shown as committed success;
- mailbox body is not persisted in Web Storage or telemetry.

Generated-contract drift and sibling-feature violations are permanent CI targets in Phase 0.

## 15. Compile-Time And CI Enforcement

Permanent policy should cover:

- dependency allowlists / forbidden outer dependencies;
- Worker/DO transport thinness;
- D1 governed write boundaries;
- contract compatibility;
- generated frontend contract drift;
- frontend feature-boundary imports;
- cross-tenant/IDOR negative fixtures;
- D1 migration/replay invariants;
- protected client-contact persistence and tenant-scoped exact-HMAC lookup positive/negative fixtures;
- Phase 2C client merge/assignment-as-non-ACL/grant-safe projection and feature-route positive/negative fixtures;
- generation freshness/fencing;
- secret/PII/content scans;
- native + WASM + Windows/release composition as applicable;
- exact-head acceptance before merge.

A policy rule should have a positive repository check and, where practical, a deliberately
forbidden fixture proving the checker fails closed.

## 16. Target Structure

Target structure is capability-oriented; exact crate names may evolve through the normative
Phase 0 split, but dependency direction may not.

```text
apps/
  control-plane-worker/
    ingress/http/
    ingress/queue/
    ingress/scheduled/
    durable_objects/
    composition/
  profile-bridge/

crates/
  primitives/
  contracts/
  identity-access-domain/
  client-domain/
  profile-domain/
  session-domain/
  mailbox-domain/
  application-ports/
  use-cases-identity/
  use-cases-clients/
  use-cases-profiles/
  use-cases-mailboxes/
  # later: notifications/search/devices/crm projection as justified
  cloudflare-adapters/
  windows-adapters/

frontend/src/
  app/
  routes/
  features/
  entities/
  shared/api/generated/
  shared/ui/
```

A temporary compatibility facade is allowed during crate migration but must not become the
permanent cross-domain orchestration owner.

## 17. Prohibited Shortcuts

- inner domain/application import from adapter/app/provider SDK;
- domain policy implemented in React or provider adapter;
- D1/R2 direct access from frontend/Bridge/domain;
- authorization only by Access policy or hidden UI button;
- Durable Object as parallel business catalog;
- session lease/fencing moved to `profile-domain` merely for convenience;
- mutable active R2 object or newest-object heuristic;
- deleting browser lock files blindly;
- snapshotting live browser directory;
- email/client/message subject in technical IDs/paths/object keys;
- long-lived device bearer/bucket credential;
- generic remote `exec` instead of typed device command;
- message body in ordinary logs/audit/events/telemetry/support;
- direct CRM table/entity dependency from Profile Platform core.

## 18. CRM Boundary

CRM integration preserves platform IDs/contracts/state machines and replaces/adapts outer
ownership only where explicitly cut over.

Default synchronization is versioned event/projection based and async-first. This does not
require every user-triggered HTTP acknowledgement to be asynchronous. Core domain/application
code never imports CRM implementation details.

Possible adapter replacements after accepted cutover:

```text
Cloudflare Access -> CRM OIDC identity adapter
D1 catalog        -> PostgreSQL/SQLx + FORCE RLS adapter
local ClientRegistry -> CRM Party projection/command adapter
```

R2 generations, session state machine and Profile Bridge browser lifecycle remain independent.

## 19. Documentation Authority / Reading Order

1. [`INDEX.md`](./INDEX.md)
2. [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) for current execution order
3. this `ARCHITECTURE.md` for stable boundaries
4. relevant accepted ADR/security/data-classification document
5. [`DEVELOPER_CAPABILITY_MATRIX.md`](./DEVELOPER_CAPABILITY_MATRIX.md) for actual accepted implementation/evidence level
6. capability-specific contracts/tests/runbooks
7. historical `DELIVERY_ROADMAP.md` / root `IMPLEMENTATION_PLAN.md` only when historical context is needed

If code and a normative invariant diverge, the gate should fail. Invariant changes are first
recorded in architecture/ADR/contracts and only then implemented. Execution-order changes are
made only in `DEVELOPMENT_PLAN.md`.