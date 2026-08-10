# Phase 2H Standalone UI Gap Inventory

**Status:** Phase 2H implementation evidence / non-normative inventory  
**Baseline:** accepted pre-2H `main` `944861634b24e0b0221b19ec4cc24a99f8cd0705`  
**Normative owner:** [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md), Phase 2H

This inventory records the accepted UI and public-contract starting point before Phase 2H grows the
standalone operator surface. It is not a parallel roadmap and does not override the normative phase
order.

## 1. Accepted Frontend Route Baseline

The root router currently composes only these feature-owned routes:

| Route | Current owner | Starting capability |
|---|---|---|
| `/` | app shell | dashboard + authenticated session panel |
| `/clients` | `features/clients` | client create/resolve/manage workflows |
| `/profiles` | `features/profiles` | profile resolve/manage workflows |
| `/mailboxes` | `features/mailboxes` | mailbox binding/job workflows |
| `/users` | `features/access` | owner/invitation/membership administration |

The accepted dashboard still tells operators to resolve resources through opaque IDs because the UI
must not invent list endpoints. Phase 2H removes that workaround only where a real authorized
list/search projection exists or is added inward-first.

Normative Phase 2H route families not yet composed at baseline:

- `/clients/:clientId`;
- `/profiles/:profileId`;
- `/sessions`;
- `/devices`;
- `/audit`;
- `/settings`.

## 2. Accepted Public Server Surface At The Baseline

The fail-closed route inventory exposes the following operator-relevant capability groups.

### Clients — UI-ready for list/detail expansion

Accepted public routes include:

- `GET/POST /api/v1/tenants/{tenant_id}/clients`;
- `GET/PATCH /api/v1/tenants/{tenant_id}/clients/{client_id}`;
- archive/contact/merge/history/grant routes.

The Client Registry public contract is already Rust-derived and TypeScript-generated. Phase 2H can
therefore create feature-owned client list/detail route composition without inventing a new client
transport shape.

### Profiles — detail-ready, list projection missing

Accepted public routes include profile create/detail, assignment, grants, coordinator and generation
subresources. The collection route is mutation-only at the accepted baseline: there is no public
`GET` profile collection classifier.

Phase 2H may compose `/profiles/:profileId` on the accepted detail contract, but a discoverable
`/profiles` list must first add a bounded, grant-safe read projection and generated public contract.
The UI must not emulate a list by probing opaque IDs.

### Users & Access — mutation workflows exist, discovery projection missing

Accepted public identity routes cover owner bootstrap/transfer, invitation create/accept and membership
status mutation. There is no accepted public member-directory/list projection in the route inventory.

Phase 2H therefore treats Users & Access discovery as contract-first work: authenticate -> live tenant
membership/authorization -> bounded member projection -> generated public contract -> UI. Client/profile
assignments remain non-authorizing and cannot be used to infer membership visibility.

### Client Mail — application contract exists

The accepted Phase 2D/2E/2F query-mail contract already defines authorized client-scoped message search
and full-message retrieval across eligible cloud/Bridge lanes. Phase 2H must connect it to Client detail
and add a sanitized full-body presentation surface. Message bodies remain transient confidential product
data and must never enter Web Storage, telemetry, audit, integration events or realtime payloads.

### Mailboxes — commands/detail exist, operator collection discovery is incomplete

Accepted routes expose mailbox binding create/detail/revoke/browser-execution plus mailbox job
create/detail/run. There is no accepted public mailbox collection `GET` in the fail-closed route
inventory. A useful administration index therefore requires a bounded authorized mailbox read projection
before the UI may claim discoverability.

### Sessions — current actor session exists, operator session catalog missing

`GET /api/v1/session` exposes the authenticated application session. Profile Coordinator state is nested
under a specific profile. There is no accepted standalone operator `/sessions` collection/read-model
contract. Phase 2H must define the exact safe operational projection before composing a session catalog;
it must not expose credentials, raw Access assertions or unbounded identifiers.

### Devices — durable job protocol exists, operator device catalog missing

Accepted device routes are job-execution protocol surfaces (`claimable`, claim, heartbeat,
generation-upload capability, generation commit and outcome). They are not an operator device inventory.
Phase 2H needs a separate bounded authorized device/job operational projection before `/devices` can be
more than a decorative shell.

### Audit — durable audit exists internally, public operator query surface missing

Governed mutations already produce immutable audit evidence, but the accepted public route inventory has
no audit query route. Phase 2H must add a sanitized, tenant-scoped, authorization-first bounded audit
projection before implementing `/audit`.

### Settings — foundation diagnostics exist, settings resource does not

Health, binding probe and authenticated session routes exist. No generic settings resource is accepted.
`/settings` must expose only concrete supported configuration/diagnostic operations; it may not become a
catch-all transport for secrets or provider credentials.

### Notifications/realtime — accepted support surface, not business authority

Durable notification event/cursor/operations routes and the Phase 2G realtime invalidation overlay are
accepted. Phase 2H may surface notification/operational state, but WebSocket state remains metadata-only
invalidation and must trigger canonical HTTPS/TanStack Query refetch.

## 3. Generated Contract Baseline

The architecture inventory currently governs three generated public contract families:

1. `control-plane-public-api`;
2. `client-registry-api`;
3. `query-mail-api`.

Phase 2H must extend canonical Rust/OpenAPI/generated TypeScript coverage before UI consumption whenever
it adds a public profile/member/mailbox/session/device/audit/settings DTO or enum. Feature code may use
private view models for presentation-only state, but may not duplicate a server-owned public shape.

## 4. Inward-First Phase 2H Work Buckets

### Bucket A — can proceed on accepted public contracts

- feature-owned `/clients/:clientId` composition and client list/detail navigation;
- feature-owned `/profiles/:profileId` detail composition while keeping unsupported list discovery
  explicit until a real projection exists;
- Client detail -> accepted Client Mail search/result/body flow;
- safe HTML/plain-text message presentation;
- accessibility/loading/empty/error/offline states for already-supported operations;
- navigation to accepted Users & Access mutation workflows without inventing member discovery.

### Bucket B — contract-first before UI discoverability claims

- grant-safe profile collection read model;
- member/access directory projection;
- mailbox administration collection projection;
- operator session projection;
- operator device/job projection;
- sanitized audit projection;
- concrete settings/diagnostics projection where product behavior requires one.

Every Bucket B surface follows:

```text
typed projection contract
  -> application authorization/query sequencing
  -> bounded adapter/read model
  -> fail-closed public classifier
  -> generated OpenAPI/TypeScript
  -> feature-owned route/UI
  -> positive + negative architecture/privacy evidence
```

## 5. Phase 2H UI Safety Gates

The Phase 2H implementation must keep these properties machine-checkable:

- exactly feature-owned route composition; root router imports public feature route APIs only;
- no sibling-feature internal imports;
- no optimistic governed-mutation success before server confirmation;
- no public server DTO/enum duplication in handwritten feature code;
- authorization/grants are never reconstructed in React;
- realtime only invalidates/refetches canonical HTTPS query state;
- mailbox content/contact plaintext/credentials never enter localStorage, sessionStorage, IndexedDB,
  telemetry, audit or realtime;
- HTML mail is sanitized/sandboxed and remote active content is disabled by default;
- loading, empty, forbidden/neutral-not-found, offline/retry and terminal states are keyboard-accessible;
- `production_ready=false` remains unchanged and no Phase 2I/2J evidence is claimed.

## 6. First Implementation Sequence

The first executable Phase 2H sequence is therefore:

```text
client list/detail route composition on generated Client Registry contracts
  -> profile detail route composition
  -> Client detail -> Mail search/body presentation on generated query-mail contracts
  -> identify and add the first missing operator read projection (Users & Access before lower-priority infrastructure)
```

This ordering uses accepted contracts first, then introduces new public query surfaces only where the
standalone UX demonstrably needs them.