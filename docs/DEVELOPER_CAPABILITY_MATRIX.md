# Developer Capability and Module Matrix

**Status:** normative accepted implementation/evidence orientation  
**Date:** 2026-08-11  
**Execution order:** [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md)

## 1. Purpose

This matrix answers **what is actually implemented on accepted `main` and at what evidence level**.
It does not define execution order; `DEVELOPMENT_PLAN.md` does.

A feature branch, queued branch, issue or PR description is not accepted implementation evidence.
A capability becomes an accepted `main` claim only after guarded merge under the exact-head policy.

## 2. Evidence Levels

| Level | Meaning |
|---|---|
| **Composed** | Wired into the accepted executable composition root and covered by permanent CI. |
| **Library** | Typed reusable implementation exists but is not fully wired into the accepted user path. |
| **Synthetic** | Invariants/protocol are proven deterministically without claiming a real provider/runtime. |
| **Target** | Normatively planned but executable implementation is absent or incomplete. |
| **External** | Requires real provider, physical host, signing, policy or independent evidence. |

No level by itself means production readiness.

## 3. Accepted Capability Matrix

| Capability | Level on accepted `main` | Accepted scope | Still Target / External |
|---|---|---|---|
| Rust workspace / primitives | Composed | Exact toolchain, typed opaque IDs, tenant/actor context, positive versions, strict lint/policy gates. | New capability IDs are added only in owning phases. |
| Identity / memberships / ACL | Composed | Access identity adapter, memberships, owner bootstrap/transfer, invitations, membership lifecycle, profile/client grants, neutral disclosure and governed D1 commands. `use-cases-identity` is independently isolated. Phase 2H adds the generated bounded member directory without moving authorization into React. | Production Access/IdP evidence is External. |
| Client Registry baseline | Composed | Phase 2A–2C accepted decomposed `client-domain`, independent `use-cases-clients`, protected contacts, checked lifecycle/merge, historical non-authorizing assignment and grant-safe projections; Phase 2H adds canonical list/detail navigation and integrates client-scoped mail into the operator detail path. | External CRM remains future-only; Phase 2I integrated failure/recovery proof is accepted; real provider/device/production proof remains Phase 2J External evidence. |
| Profile catalog | Composed | Current create/query/grant/assignment metadata paths, profile state and active generation pointer; Phase 2H adds generated bounded profile discovery plus canonical `/profiles/:profileId` navigation. | Remaining real external runtime evidence is later Target/External. |
| Profile generation registry | Composed | Governed register/query/verify/activate/deactivate/quarantine, replay/evidence, audit/outbox and pointer integrity; Phase 2F adds device-authorized exact-object verification plus fenced/CAS dirty-generation activation. | Production R2/device unwrap/cross-device evidence is External. |
| Profile coordinator | Composed | Durable Object journal, sequence/version/epoch/fencing, timeout/drain/recovery, application-thin HTTP ingress and D1 projection; Phase 2F revalidates authoritative session/device/epoch/fence before accepting browser/device generation work. | Remote production concurrency evidence is External. |
| Full Profile Bridge operator flow | Composed / Synthetic | Phase 2F repository-local executable composes claim, device identity, Coordinator lease/fence, generation ownership, browser/network preflight, writer lock, graceful retained dirty close, immutable encrypted candidate, exact verify, authoritative commit, local successor/rematerialization and ownership release. Phase 2H exposes only safe supported operator guidance for device/session/recovery surfaces and does not impersonate device agents. | Real Camoufox, production keys, remote enrollment/coordinator, physical devices and real provider/R2 execution remain External. |
| Local profile lifecycle / materialization | Composed / Synthetic | Workspace marking/inventory/lock ownership, clone-only recovery, quota/support policy, retained writer ownership through dirty-generation commit, irreversible supersede and deterministic post-commit rematerialization recovery. | Real browser/kernel-lock and physical multi-device evidence remains Phase 2J External. |
| Encrypted cloud generations | Composed / Synthetic | XChaCha20-Poly1305 immutable container plus Phase 2F canonical exact generation key/checksum/metadata policy, short-lived create-only upload capability, exact verification and fenced/CAS activation sequencing. | Production R2/device unwrap/remote recovery atomicity evidence remains External. |
| Mailbox operations baseline | Composed / Synthetic | Phase 2E accepts decomposed `mailbox-domain`, independent `use-cases-mailboxes`, Gmail API/IMAP outer adapters, durable Queue retry/DLQ/idempotency/fencing, opaque fixed-service secret resolution, auth/suspended lifecycle and metadata-only provider observations; Phase 2F accepts the durable browser/device lane; Phase 2H adds generated bounded mailbox binding discovery in the standalone operator UI. | Real Gmail/IMAP/Camoufox/physical-device provider execution remains External evidence. |
| React standalone operator UI | Composed / Synthetic | Phase 2H accepts useful feature-owned surfaces for `/`, `/clients`, `/clients/:clientId`, `/profiles`, `/profiles/:profileId`, `/users`, `/mailboxes`, `/sessions`, `/devices`, `/audit` and `/settings`; generated operator-query DTOs back profile/member/mailbox directories; Clients/Profiles use canonical detail navigation; network/offline state is explicit; unsupported device/audit operations remain truthful rather than decorative or authority-bypassing. | Phase 2I integrated E2E/security/recovery/operational behavior is accepted; Phase 2J supplies real rollout evidence. |
| Client Mail standalone UI | Composed / Synthetic | Phase 2H wires the accepted Phase 2D/2E/2F application path end-to-end through generated Rust/OpenAPI/TypeScript request/response contracts, thin authenticated Worker ingress, D1 mailbox eligibility and real frontend search/get-message calls. Search terms and provider message references are body-carried rather than URL-carried. | Phase 2I integrated failure/recovery proof is accepted; real provider/device execution remains Phase 2J External evidence. |
| Safe HTML mail rendering | Composed / Synthetic | Phase 2H accepts sandboxed iframe rendering with deny-by-default CSP, no script/connect/form/object/frame/media execution, no referrer, remote active content blocked, only bounded `cid:`/inert raster data-image handling, and permanent negative tests against navigation/action/event/resource attributes and browser Web Storage/telemetry regressions. | Repository-local security negatives are accepted through Phase 2I; real-world provider corpus validation remains Phase 2J External evidence. |
| Cross-component standalone acceptance | Composed / Synthetic | Phase 2I expands the deterministic metadata-only standalone proof across identity/client/profile/mailbox/device/realtime/UI security, failure, recovery, supply-chain/license, support-bundle and release-freeze gates. | Real deployment/provider/device/signing/key evidence is Phase 2J External. |
| Integration event envelope/outbox | Composed / Synthetic | Phase 1A versioned envelope, event registry, evolved D1 outbox, metadata-only notification events, Queue dispatch, source guards and durable consumer idempotency; Phase 1B preserves the canonical event source for replay and Phase 2G reuses durable accepted delivery as the sole live-invalidation source. | Future capability event types/consumers must extend the same registry and durable-source rules. |
| Notification delivery/catch-up operations | Composed / Synthetic | Phase 1B `notification-domain` + `use-cases-notifications`, deterministic bounded retry/DLQ, authorized immutable-audit replay, grant-aware durable catch-up/cursors, bounded compaction, sanitizer-safe owner operations and generated notification HTTP contracts; Phase 2G extends the same application ownership with provider-neutral realtime authorization/audience/sink ports, durable-first reconnect catch-up and exact-event live authorization. Phase 2H preserves invalidation-only frontend ownership. | Real remote/browser deployment behavior remains External evidence. |
| Client contact protection | Composed | Phase 2A/2B accepted versioned normalization, separate encryption/HMAC key domains, ciphertext-only authoritative D1 persistence, key-version-aware protection and tenant-first indexed exact lookup; Phase 2D reuses the HMAC index behind live authorization/grants. | Production key operations/restore remain External; fuzzy/prefix PII search remains prohibited without a separate ADR. |
| Client Registry 2.0 | Composed | Phase 2A–2C accepted client-domain split, `use-cases-clients`, protected contacts, lifecycle/merge, grant-safe projections, historical assignment and ordinary Registry UI workflows; Phase 2H accepts cross-capability operator discovery and canonical detail navigation without opaque-ID-only workarounds where list/search exists. | Future CRM cutover remains outside active Phase 2; Phase 2I integrated release-candidate behavior is accepted. |
| Read models/global search | Composed / Synthetic | Phase 2D accepted independent `use-cases-query`, capability-owned read-model ports/projections, bounded opaque-ID global search, grant-safe D1 predicates, cursor/cost bounds and query-plan evidence. Phase 2H composes bounded profile/member/mailbox read projections into generated public operator-query contracts and standalone directories while retaining authorization-before-projection. | Phase 2I repository-local performance/security bounds are accepted; production calibration and real provider/device evidence remain Phase 2J External. |
| Client-scoped mailbox message search/body | Composed / Synthetic | Phase 2D accepted provider-neutral search/get-message contracts and authorization -> mailbox eligibility -> provider sequencing; Phase 2E accepts bounded Gmail API/IMAP cloud adapters; Phase 2F accepts the browser/Bridge lane; Phase 2H exposes the same accepted application path through thin Worker transport and sanitized transient standalone UI. | Real mailbox-provider/Camoufox/physical-device execution remains External. |
| Device job/browser mailbox execution | Composed / Synthetic | Phase 2F accepts independent `device-domain`/`use-cases-devices`, durable D1 device jobs/opaque claims, lifecycle/replay/fencing/freshness preconditions, browser/network identity checks, metadata-only generation upload capability, exact object verification, authoritative commit and retained Bridge ownership through local successor/recovery. Phase 2H keeps machine claim/heartbeat protocols out of browser authority while exposing supported operator recovery guidance. | Real physical device, Camoufox and provider execution remains External; production readiness is not implied. |
| Realtime UserNotificationHub | Composed / Synthetic | Phase 2G accepts a versioned metadata-only invalidation contract, provider-neutral realtime application ports/orchestration, per-user hibernatable Durable Object/WebSocket connection coordination, current membership/grant and exact-event authorization, durable cursor catch-up before live continuation, bounded reauthorization/revocation disconnect, synchronization-race gating, multi-tab/device broadcast, duplicate-safe frontend invalidation/refetch, and permanent privacy/authority negative fixtures. Phase 2H preserves the same invalidation-only authority boundary. | Real remote Cloudflare/browser deployment evidence is External. |
| Complete standalone UI | Composed / Synthetic | Phase 2H implementation PR #164 was accepted from exact source head `9add9b94d0de255b93e5a7c24584fcf6756462a7` and guarded squash merge `a32768feddb3da69b872e701bc529aad3521e1b0`, with exactly 12/12 permanent workflows success, `behind_by=0`, reviews=0 and unresolved review threads=0. | Phase 2I integrated E2E/security/recovery/operations hardening is accepted; Phase 2J closes real rollout evidence. |
| Integrated release-candidate hardening | Composed / Synthetic | Phase 2I implementation PR #168 was accepted from exact source head `c1075337cfc582d0f4c00ec34b1aa7cda9ac1101` and guarded squash merge `800c634147d6300ea3989ff0cf87ade6e2387ee9`, with exactly 12/12 permanent workflows success, `behind_by=0`, reviews=0 and unresolved review threads=0. Repository-owned scope includes executable security/failure/recovery/DR, metadata-safe operations/capacity, dependency/license/threat-model, support-bundle and release-freeze evidence. | Real Cloudflare/provider/Camoufox/physical-device/signing/key/independent-review evidence remains Phase 2J External; `production_ready=false` remains unchanged. |
| External CRM integration | Target / Future | Future-only contract-isolated Party/adapter direction is documented separately. | No active CRM implementation; it may be considered only after standalone Phase 2J. |
| Production readiness | External | Evidence intake/readiness interlocks exist. | Mandatory external evidence is incomplete; `production_ready=false` until Phase 2J acceptance. |

## 4. Architecture Obligation Status

This table prevents historical architecture requirements from being misread as completed merely
because phase numbering changed.

| ID | Requirement | Accepted-main status | Fixed execution owner |
|---|---|---|---|
| A1 | Adapter dependency boundary | **Accepted** — corrected inward dependency rule + executable allowlists. | Preserve continuously. |
| A2 | `application-ports` splitting | **Accepted** — Phase 0A/PR #79 split capability modules with thin facade. | Preserve; add modules with owning capabilities. |
| A3 | Domain aggregate splitting | **Accepted** — Phase 2A decomposed `client-domain`; Phase 2E decomposed `mailbox-domain` into binding/job/runtime-lane/observation ownership behind a thin compatibility facade; Phase 2F owns device state separately in `device-domain`. | Preserve continuously. |
| A4 | OpenAPI -> TypeScript generation | **Accepted for active Phase 2H public UI additions** — generator/CI remains canonical; Phase 2H adds Rust-owned operator-query and executable Client Mail public contracts with committed OpenAPI/TypeScript output instead of feature-local server DTO duplication. | Preserve/extend for new Phase 2I public surfaces; existing legacy surfaces are migrated only in an explicit owning change. |
| A5 | Feature-sliced SPA route composition | **Accepted through Phase 2I** — Phase 2C established feature-owned public route APIs; Phase 2H expands all required route families while root composition continues to reject sibling-feature internals. | Preserve continuously through Phase 2J. |
| A6 | Architecture consistency gate | **Accepted** — deterministic inventory/docs consistency in CI. | Expand coverage with new modules/routes/contracts. |
| A7 | Route classifier modularization | **Accepted** — capability-owned fail-closed classifiers. | Preserve for new route families. |
| A8 | CQRS/read-model boundary | **Accepted through Phase 2I** — independent `use-cases-query`, capability-owned read projections and authorization-before-projection/provider sequencing remain authoritative; Phase 2H exposes accepted projections through thin generated transport/UI without moving query policy outward. | Preserve through Phase 2J. |
| 6.1 | Integration event envelope | **Accepted foundation** in Phase 1A. | Extend registry/versioned events only; Phase 2G proves no ad-hoc unversioned WebSocket business payload. |
| 6.2 | Durable-before-notify | **Accepted through Phase 2I.** Phase 1B durable delivery remains canonical; Phase 2G live fanout occurs only after durable `Delivered`; Phase 2H UI continues to treat realtime as invalidation/refetch rather than success authority. | Preserve through Phase 2J. |
| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2I.** Notification replay, mailbox/device consumers and realtime duplicate delivery remain duplicate-neutral at canonical state/UI logical-state boundaries. | Preserve for every later consumer/surface. |
| 6.4 | Authorization-before-projection | **Accepted through Phase 2I.** Live membership/grants precede projection/provider access and realtime subscription/catch-up; Phase 2H operator lists and Client Mail transport call accepted authorization/query application paths rather than recreating ACL in UI. | Preserve continuously through Phase 2J. |
| 6.5 | PII contact protection | **Accepted through Phase 2B/2D** — protected D1 contacts, separate versioned encryption/HMAC domains and tenant-first exact lookup are accepted; query reuse remains grant-safe. | Preserve continuously; fuzzy/prefix PII indexing still requires a separate accepted ADR. |
| 6.6 | Profile materialization | **Accepted repository-local through Phase 2I** — browser/device integration, retained writer ownership, immutable dirty-generation evolution and deterministic rematerialization/recovery evidence are composed/synthetic. | Phase 2I accepted broader repository-local recovery/E2E evidence; Phase 2J supplies real physical/provider evidence. |

## 5. Current Module Ownership

```text
crates/primitives
  stable provider-neutral value objects

crates/*-domain
  pure provider-independent invariants/state machines, including accepted device-domain ownership

crates/application-ports
  one accepted Cargo crate with capability-owned interface modules, including Phase 2G realtime notification authorization/audience/sink ports

crates/control-plane-contract
  canonical migrated public control-plane contracts, generated OpenAPI source and fail-closed route classifiers; Phase 2H adds Rust-owned operator-query and executable Client Mail contract sources

crates/use-cases-identity
  independent identity governance + verified-identity application context

crates/notification-domain
  provider-neutral delivery attempt/terminal/cursor invariants

crates/use-cases-notifications
  independent notification dispatch, retry, replay, catch-up, retention, operations and Phase 2G durable-first realtime invalidation orchestration

crates/use-cases-clients
  independent Client Registry command/application context accepted in Phase 2A–2C

crates/use-cases-query
  independent cross-capability read/search application context accepted in Phase 2D and composed into Phase 2H operator query/Client Mail transports

crates/use-cases-mailboxes
  independent mailbox binding/job/scheduled application context accepted in Phase 2E

crates/use-cases-devices
  independent durable device-job/browser execution application context accepted in Phase 2F

crates/use-cases
  canonical application owner for the remaining Profile Catalog and Profile Generation Registry workflows; notification/client/query/mailbox/device ownership does not return to this compatibility surface

crates/cloudflare-adapters
  D1/Access/DO/R2/Queue/provider implementations depending inward, including realtime authorization and Phase 2H client-mail eligibility/query projection adapters

apps/control-plane-worker
  thin Worker/DO/Queue/Scheduled composition and transport; Phase 2H adds thin operator-query and Client Mail ingress over accepted application ownership

apps/profile-bridge
  Windows-native local/device/runtime composition including accepted Phase 2F retained dirty-close orchestration

frontend
  React presentation/navigation/query cache; Phase 2H composes canonical operator routes/directories and safe transient mail rendering while Phase 2G realtime remains invalidation-only
```

The notification, client, query, mailbox and device extraction points were accepted in Phase 1B,
Phase 2A, Phase 2D, Phase 2E and Phase 2F. Phase 2G extends the already-independent notification
application context rather than creating a provider-specific inner boundary. Phase 2H composes those
accepted capabilities into the standalone UI without moving their authorization/business ownership into
React or Worker ingress. Profile Catalog and Profile Generation Registry application workflows remain
canonically owned by shared `crates/use-cases` until a future explicitly scoped extraction is accepted;
no accepted independent application capability may move back into that shared surface merely for convenience.

## 6. Current End-To-End Boundaries

### Browser/API path

```text
React / same-origin request
  -> fail-closed route classification / authenticated query ingress
  -> verified identity
  -> live membership/grant resolution
  -> capability application command/query
  -> typed adapter
  -> governed durable result/projection
```

UI never invents authorization or storage access. Concrete D1 mutation types remain adapter-only.
Phase 2H list/detail UI consumes generated server contracts or explicit private presentation view models.

### Profile generation/runtime path

D1 is authoritative for generation metadata/active pointer; DO/session fencing owns writer
coordination; local workspace is materialization/cache/recoverable dirty state, not cloud authority.
Phase 2F accepts repository-local Bridge -> immutable encrypted generation -> exact object verify ->
fenced/CAS activation -> local successor/recovery ordering. Phase 2H exposes only supported operator
surfaces around those contracts; it does not create browser authority over machine device protocols.
Real R2/device/Camoufox behavior remains External until proven.

### Mailbox path

Current accepted mailbox capability combines Phase 2E composed metadata/job scheduling with the Phase 2D
client-scoped message query contract and bounded Gmail API/IMAP cloud adapters, plus the Phase 2F
browser/Bridge lane at repository-local deterministic evidence. Phase 2H completes the standalone path:
authenticated thin Worker ingress calls the accepted query application contract, D1 eligibility is checked
before provider/body invocation, generated request/response DTOs cross the public boundary, and sanitized
message content remains transient in the authorized UI. Queue/D1 coordination remains metadata-only.
Real provider, Camoufox and physical-device execution evidence remains External.

### Realtime notification path

```text
durable accepted notification delivery / outbox event
  -> bounded authorized audience
  -> per-user NotificationHub Durable Object
  -> current membership/grant + exact-event authorization
  -> metadata-only invalidation to every attached tab/device socket
  -> frontend dedupe + TanStack Query invalidation
  -> authoritative HTTPS refetch
```

Reconnect performs durable cursor catch-up before live continuation. Process memory/WebSocket/DO
connection loss cannot delete or advance canonical event/cursor state, and live transport failure is
repaired by durable catch-up. Bounded reauthorization closes revoked/suspended actors without page reload.
The realtime overlay never carries client-contact plaintext, mailbox subject/body, credentials or secret
handles and never becomes business/query authority. Phase 2H permanently preserves this rule.

### Safe standalone Client Mail path

```text
Client detail
  -> generated bounded mailbox/query request
  -> authenticated thin Worker ingress
  -> accepted use-cases-query authorization
  -> client/mailbox eligibility
  -> provider/Bridge query
  -> transient generated response
  -> text rendering or sandboxed CSP-restricted HTML iframe
```

Search terms and provider message references are body-carried, not URL query/path data. Mail content is
not persisted in browser Web Storage, realtime, audit, metrics or ordinary telemetry. HTML active content,
remote browsing contexts, navigation/action handlers and remote tracker loading are denied by default.

## 7. Definition Of A Complete New Capability

A capability is not accepted as Composed until all applicable items exist:

1. versioned public/internal contract;
2. pure domain decision where provider-independent state exists;
3. minimal capability-owned application ports;
4. application authorization/idempotency/version/failure sequencing;
5. concrete adapter/migration where required;
6. thin executable composition wiring;
7. replay/failure/forbidden-access/boundary tests;
8. permanent positive + negative CI policy for expensive architecture regressions;
9. generated public frontend contracts where applicable;
10. capability matrix/docs updated only for proven claims;
11. exact-head green + bounded review + guarded merge;
12. real external evidence only for provider/physical/runtime claims.

## 8. Documentation Authority

- execution order and fixed modular extraction points: `DEVELOPMENT_PLAN.md`;
- stable architecture: `ARCHITECTURE.md` + accepted ADRs;
- data handling: `DATA_CLASSIFICATION.md`;
- product/UI target: `UI_ARCHITECTURE.md`;
- accepted implementation level: this matrix;
- accepted Phase 2G realtime design/evidence: `REALTIME_NOTIFICATIONS.md`;
- accepted Phase 2H implementation provenance: issue #163 / PR #164, exact source head `9add9b94d0de255b93e5a7c24584fcf6756462a7`, squash merge `a32768feddb3da69b872e701bc529aad3521e1b0`;
- accepted Phase 2I implementation provenance: issue #167 / PR #168, exact source head `c1075337cfc582d0f4c00ec34b1aa7cda9ac1101`, squash merge `800c634147d6300ea3989ff0cf87ade6e2387ee9`;
- accepted phase issue/PR/SHA provenance: `architecture/accepted-phases.json`;
- historical delivery: `DELIVERY_ROADMAP.md`;
- post-standalone CRM evolution: `FUTURE_DEVELOPMENT.md`.

See [`INDEX.md`](./INDEX.md) for the documentation map.
