# Architecture Re-baseline v3 — AR-3 Application Architecture Contract

**Document status:** EVIDENCE / AR-3 accepted  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Tracking:** #266 / bounded slice #274  
**Exact baseline:** `3c592e98a0435388119f5b224a864b8f0d649379`  
**Exact-green implementation candidate:** `f26726a5892e660940dffab7bce5615c3f13eb87`  
**Accepted implementation merge:** `2b7e7ec828b7d29209b97adb5100b1c2559c73f0`  
**Implementation PR:** #276 — 13/13 applicable permanent PR workflows passed on the unchanged exact head  
**Production mutation:** forbidden

## 1. Purpose

AR-3 turns the accepted AR-2 runtime topology into an explicit application-ownership contract. It does not move code, change public routes, change D1 schema, change Wrangler bindings, activate a capability or mutate staging/production.

The machine authority remains the existing canonical architecture hierarchy. `architecture/runtime-topology-ar2.json` is accepted topology input. `architecture/inventory.json` is the canonical projection target. AR-3 must not introduce another runtime/capability registry.

The contract separates six questions that were previously easy to conflate:

1. which runtime process receives an event;
2. which transport module owns protocol parsing/response mapping;
3. which application/use-case crate owns workflow sequencing;
4. which `application-ports` boundary is consumed by that workflow;
5. which outer adapter/provider implementation satisfies the port;
6. where concrete adapters are constructed and wired.

A transport module may legitimately depend on runtime SDK types and transport authentication primitives. It becomes composition debt when it constructs persistence/provider implementations for an application workflow that should be supplied by a composition root.

## 2. Accepted dependency direction

AR-3 preserves the stable inward dependency rule:

```text
primitives
contracts -> primitives
domains -> contracts + primitives
application-ports -> domains + contracts + primitives
use-cases-* -> application-ports + domains + contracts + primitives
adapters -> application-ports + domains + contracts + primitives + provider/runtime SDKs
apps -> use-cases-* + adapters + contracts + primitives
```

`apps/*` are outer protocol/composition surfaces. They may know concrete adapters because a composition root must instantiate them; ordinary route handlers should not become independent composition roots by convenience.

## 3. Runtime-process ownership

### 3.1 Control Plane Worker

- runtime resource: `control_plane_worker`;
- entrypoint: `apps/control-plane-worker/src/lib.rs`;
- route classifier: `crates/control-plane-contract/src/lib.rs::classify_route`;
- current central composition module: `apps/control-plane-worker/src/composition.rs`;
- event types: fetch, Queue and scheduled;
- authoritative business/catalog state: D1 through application-owned ports/adapters;
- object/runtime state: R2 / Durable Objects only at their accepted outer boundaries;
- accepted topology input: `architecture/runtime-topology-ar2.json`.

`lib.rs` is a transport switch. It is not a business/application owner. It must continue to dispatch typed route classes and runtime events into bounded transport/application paths rather than accumulate use-case policy.

### 3.2 Mailbox Secret Resolver Worker

- runtime resource: `mailbox_secret_resolver_worker`;
- entrypoint: `apps/mailbox-secret-resolver-worker/src/lib.rs`;
- dedicated state: `resolver_d1`;
- schedule: `resolver_reconciliation_schedule`;
- service boundary: consumed by the control plane through `mailbox_secret_resolver_service`;
- security purpose: credential isolation, replay protection, encrypted secret storage, OAuth exchange/refresh and key reconciliation.

This is an intentional security/runtime boundary, not accidental duplication. AR-3 retains it. Composition-root cleanup in the public control-plane Worker must not collapse the resolver into the control-plane process or catalog D1.

### 3.3 Profile Bridge

- runtime lane: `browser_bridge_mailbox_lane` plus the accepted local browser/profile runtime;
- executable entrypoint currently starts at `apps/profile-bridge`;
- authoritative local runtime concerns remain Windows-native device trust/materialization/process supervision and browser execution;
- cloud business authority remains behind the authenticated control-plane/device protocol.

AR-3 does not redesign the Bridge. Production-grade release/update ownership belongs to AR-15 and its inherited Batch E program.

## 4. Canonical application ownership

| Capability / workflow | Transport surface | Canonical application owner | Outer adapter/composition owner | AR-3 finding |
|---|---|---|---|---|
| Identity ceremonies/governance | `apps/control-plane-worker/src/identity.rs` | `crates/use-cases-identity` | `apps/control-plane-worker/src/composition.rs` + `crates/cloudflare-adapters` | conforming seam |
| Client mutations/registry | `apps/control-plane-worker/src/clients.rs` | `crates/use-cases-clients` | `apps/control-plane-worker/src/composition.rs` + `crates/cloudflare-adapters` | conforming seam |
| Cross-capability/operator queries | `apps/control-plane-worker/src/operator_queries.rs` | `crates/use-cases-query` | currently constructed partly inside transport | composition debt -> AR-4A |
| Client Mail search/message | `apps/control-plane-worker/src/client_mail_query.rs` | `crates/use-cases-query` | currently constructed inside transport | route/composition ownership debt -> AR-4A + AR-4B as distinct concerns |
| Outbound Client Mail send | `apps/control-plane-worker/src/client_mail_send.rs` | `crates/use-cases-mailboxes::outbound_mail` | currently constructs D1 + Gmail/SMTP concrete providers inside transport | outbound composition debt -> AR-4C |
| Profile catalog/assignment/grants | `apps/control-plane-worker/src/profiles.rs` | `crates/use-cases` | `crate::composition::profile_application` | conforming seam; no extraction required |
| Profile generation registry | `apps/control-plane-worker/src/profile_generations.rs` | `crates/use-cases::generations` | `crate::composition::profile_generation_application` | conforming seam; no extraction required |
| Mailbox bindings/browser lane | `apps/control-plane-worker/src/mailbox_bindings.rs` | `crates/use-cases-mailboxes` | `crate::composition::*` | conforming seam |
| Mailbox jobs | `apps/control-plane-worker/src/mailbox_jobs.rs` | `crates/use-cases-mailboxes` | repositories are composed centrally; provider router remains directly constructed/owned in transport path | residual composition debt -> AR-4A |
| Device jobs | `apps/control-plane-worker/src/device_jobs.rs` | `crates/use-cases-devices` | `crate::composition::*` | conforming seam |
| Notification catch-up/replay/operations | `apps/control-plane-worker/src/notifications.rs` | `crates/use-cases-notifications` | D1 repositories currently constructed inside transport | composition debt -> AR-4A |
| Profile coordinator ingress | `apps/control-plane-worker/src/profile_coordinator_ingress.rs` / DO ingress | `crates/use-cases::coordinator_ingress` + `session-domain` decisions | accepted coordinator adapter/composition boundary | retain boundary |
| Resolver operations | resolver Worker ingress | resolver Worker application/security modules | resolver-specific storage/provider adapters | intentionally isolated |

The table identifies application ownership. It does not imply that every row needs a separate Cargo crate. Crate extraction is justified only by a concrete dependency/ownership benefit.

## 5. AR-4 remediation map

### AR-4A — Composition-root consolidation

AR-4A owns general control-plane composition debt that is independent of route taxonomy or outbound-mail-specific orchestration.

Current evidence includes:

- `operator_queries.rs` directly constructs `D1QueryRepository`;
- `notifications.rs` directly constructs `D1NotificationOperationsRepository` and `D1NotificationRepository`;
- `mailbox_jobs.rs` retains direct concrete Cloud mailbox provider-router knowledge in the transport path;
- `composition.rs` is already the accepted common construction seam for Clients, Identity, Profiles, Generations, Mailbox bindings and Device jobs.

AR-4A should consolidate construction without changing application behavior, public route semantics or provider policy.

### AR-4B — Client Mail route ownership

AR-4B owns route-classification ownership, not generic composition cleanup.

Current contract evidence is explicit:

- `ClientMailSearchApi` and `ClientMailMessageApi` are classified in `routes/clients.rs` as capability `clients`;
- `ClientMailSendApi` is classified in `routes/client_mail.rs` as capability `client_mail`;
- all three routes share the `/clients/{client_id}/mail/*` surface but currently have split route ownership.

AR-4B must establish one coherent Client Mail route owner while preserving exact methods/paths, authentication and generated public-contract compatibility.

### AR-4C — Outbound Mail composition extraction

AR-4C owns outbound send composition specifically.

`client_mail_send.rs` currently constructs or selects:

- client-mail eligibility persistence;
- source-message query authorization/provider access;
- mailbox binding persistence;
- outbound intent persistence;
- concrete Gmail send provider;
- concrete SMTP send provider;
- provider selection from stored mailbox binding state.

The application workflow itself is already owned by `use-cases-mailboxes::outbound_mail`; AR-4C should make the outer provider/repository composition explicit without moving provider policy inward or changing the accepted ambiguous-outcome/idempotency semantics.

### AR-4D — Profile extraction decision

**AR-3 decision: `NOT_REQUIRED`.**

Evidence:

- `profiles.rs` consumes `crate::composition::profile_application` rather than constructing D1/R2 provider implementations;
- `profile_generations.rs` consumes `crate::composition::profile_generation_application`;
- canonical Profile and Generation application ownership in `crates/use-cases` is already explicit in the stable architecture authority;
- no second competing Profile application owner was found;
- a separate crate would currently improve symmetry more than dependency isolation.

Therefore AR-4D must not run unless a later accepted change introduces new evidence that invalidates this decision. The default sequence after AR-4C proceeds directly to AR-5.

## 6. Runtime-resource projection rule

AR-3 does not re-decide AR-2 resources. The canonical inventory must project every resource row from `architecture/runtime-topology-ar2.json` together with application/process ownership, while preserving the AR-2 `KEEP` / `DEFER` / `DELETE` decision and execution slice.

In particular:

- `generation_verification` remains `DELETE`, but source/Wrangler binding removal remains AR-5;
- resolver Worker/D1/service/schedule remain `KEEP` as one credential boundary;
- `MAILBOX_JOBS` and its DLQ remain `KEEP`;
- provider lanes remain source/runtime topology facts and are not production activation claims;
- no AR-3 resource creates, updates or deletes a provider object.

The inventory projection must fail closed if AR-2 resource identity/decision/owner data drifts without an explicit later owning slice.

## 7. Closed AR-3 composition taxonomy

The canonical projection uses a closed status vocabulary:

- `CONFORMING_COMPOSITION_SEAM` — route/ingress obtains application adapters from an explicit composition seam;
- `TRANSPORT_COMPOSITION_DEBT` — ordinary transport constructs persistence/provider application dependencies;
- `ROUTE_OWNERSHIP_DEBT` — public route is classified under the wrong/split capability owner while semantics remain accepted;
- `INTENTIONAL_RUNTIME_BOUNDARY` — runtime/security boundary is outer by design and must not be collapsed;
- `CONDITIONAL_EXTRACTION_NOT_REQUIRED` — extraction was reviewed and rejected for lack of measurable benefit.

A direct runtime SDK import alone is not sufficient to label debt. Protocol parsing, request authentication, Worker event registration and Durable Object/Queue ingress are legitimate outer-runtime responsibilities.

## 8. Fitness-test requirements

AR-3 machine checks must prove at least:

1. accepted AR-2 topology is the only runtime-resource decision input;
2. every AR-2 resource is projected exactly once;
3. every application capability has one canonical application owner;
4. referenced app/use-case/composition paths exist;
5. Client Mail split route ownership remains visible until AR-4B and cannot silently grow;
6. known direct application-adapter construction debt remains visible until its owning AR-4 slice removes it;
7. Profile/Generation transport keeps the current composition seam while AR-4D is `NOT_REQUIRED`;
8. resolver isolation remains intact;
9. `architecture_complete=false`, `production_core_gate=BLOCKED`, `production_ready=false`;
10. no public API/OpenAPI or D1 migration mutation is part of AR-3.

Negative self-tests must demonstrate that changing an owner, deleting a resource projection, inventing a second application owner, prematurely authorizing AR-4D or enabling the Production Core gate fails verification.

## 9. AR-3 completion boundary

AR-3 is complete only when the canonical inventory and generator/checker reproduce this contract on one unchanged exact candidate head, all applicable permanent workflows pass, the branch is current with accepted `main`, and the accepted state atomically advances to AR-3 with AR-4A next.

Until that merge:

```text
accepted architecture checkpoint = AR-2
next slice = AR-3
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
```

After accepted AR-3 merge, only the checkpoint/next-slice projection changes:

```text
accepted architecture checkpoint = AR-3
next slice = AR-4A
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
```

No provider or production mutation is authorized by either state.