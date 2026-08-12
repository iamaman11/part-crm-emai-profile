# Pre-2J Product Readiness Remediation Plan

**Status:** PROPOSED CANONICAL / ACTIVE BLOCKER FOR PHASE 2J  
**Tracking:** issue #203  
**Planning base:** accepted `main` `c5feaaa3bbf640a372e5be2cb46c3b521ab1ed0d`  
**Date:** 2026-08-12  
**Accepted product phase:** Phase 2I  
**Production readiness:** remains `false`  
**Supersedes:** no accepted history; R1-R9 and the closed pre-2J architecture remediation record remain accepted history  
**Activation rule:** this document becomes canonical current remediation authority only when Batch 0 is accepted on `main` together with the matching status/documentation-authority updates.

## 1. Purpose

This plan closes repository-owned product, authorization, deployment and Windows delivery gaps discovered after the R1-R9 pre-2J architecture remediation had already been accepted and closed.

The findings are not Phase 2J external evidence. They are repository-owned prerequisites that must be designed, implemented, tested and accepted before external production-readiness evidence is meaningful.

The goal is not to maximize abstraction, number of crates, services or framework layers. The goal is an expert-grade, boring, modular and extensible system whose domain rules are explicit, whose infrastructure is replaceable, whose permissions fail closed, whose release provenance is exact, and whose code remains understandable to a senior developer entering the repository without hidden operational knowledge.

Until final Batch F acceptance:

- Phase 2J is blocked;
- no Phase 2J acceptance evidence may be claimed;
- `production_ready=false` is immutable;
- R1-R9 remain accepted and regression-protected history;
- Phase 2I remains the last accepted repository-local product phase;
- accepted `openapi/v1/**` compatibility authority remains frozen unless a separately justified API-versioning change is explicitly approved.

## 2. Product decisions fixed for this plan

The following decisions were explicitly confirmed by the product owner on 2026-08-12 and are no longer open planning questions.

### D1 — Web UI hosting

The canonical production web topology is **Cloudflare Workers Static Assets**, not a separate Cloudflare Pages project.

One controlled Worker release serves:

- the React/Vite SPA static asset build;
- browser-facing API routes;
- auth/device ingress owned by the control-plane application;
- the existing Cloudflare Worker/Durable Object/Queue/D1/R2 composition.

The decision intentionally minimizes separately versioned production surfaces and enables one exact source revision to identify the UI + Worker release unit.

### D2 — Member self-service Client/Profile creation

Any **ACTIVE Member** with the ordinary operator capability may create a Client and a Profile without Owner intervention.

Creation does not grant tenant-wide visibility.

The creator must receive the corresponding explicit resource grant in the same governed atomic persistence boundary as resource creation. A transaction/replay failure must never leave either:

- a resource without its required creator grant; or
- a grant for a resource that was not created.

The Owner retains lifecycle, revoke, reassignment and administrative authority.

Suspended/revoked/non-member actors must fail closed.

### D3 — Mailbox-to-client cardinality

A mailbox binding may have **zero or one active Client association** at a time.

A Client may have **zero or many mailbox bindings**.

Bind, unbind and rebind are explicit business commands, not derived joins and not ACL shortcuts. History remains auditable.

Mailbox association and mailbox authorization are separate concepts:

- association answers which Client the mailbox currently belongs to;
- grants/membership answer whether an actor may operate on that Client/mailbox.

Neither may silently substitute for the other.

### D4 — Outbound mail is required product scope

Before Phase 2J, the standalone application must have repository-owned support for:

- compose/new message;
- reply;
- reply-all;
- forward;
- provider-neutral send orchestration;
- safe idempotency/retry semantics;
- authorization and mailbox/client eligibility;
- auditable metadata-only operational evidence;
- frontend UX for the supported send operations.

Real-provider execution against production credentials remains Phase 2J external evidence, but the repository-owned application contract, adapters and operator workflow must be complete before Phase 2J starts.

### D5 — Production deployment control

Git is the release source of truth, but merge to `main` does **not** mean blind immediate production mutation.

The target flow is:

`PR -> exact-head CI -> accepted merge -> immutable release build -> staging promotion -> smoke/migration checks -> protected production promotion -> production evidence`.

Production promotion is a controlled state transition with exact artifact provenance. Database migrations must never depend on unsafe source rollback.

## 3. Non-negotiable architecture quality bar

Every batch in this plan must preserve the existing Clean Architecture direction and improve, not erode, modularity.

### 3.1 Dependency direction

The allowed direction remains inward:

```text
primitives
  <- contracts
  <- domains
  <- application-ports
  <- use-cases
  <- adapters
  <- apps / composition roots

frontend generated contracts
  <- frontend shared transport primitives
  <- feature-owned capability API modules
  <- feature UI/routes
```

More concretely:

- domain crates contain provider-independent policy/state transitions only;
- use cases own workflows, ordering, authorization intent, idempotency and transaction semantics;
- application ports are defined by application needs, not by SDK shapes;
- Cloudflare, D1, Gmail, IMAP/SMTP, Windows, crypto/signing and filesystem details stay in outer adapters;
- Worker ingress is parse/authenticate/map/call/map, not a business-policy owner;
- React owns presentation, user interaction and remote-cache behavior, not authorization or persistence policy;
- a provider SDK type must never cross into domain/use-case public APIs;
- a D1 SQL shape must never become the domain model;
- a frontend component must not become the authority for a permission or lifecycle rule.

### 3.2 Capability ownership

Changes must extend the existing capability ownership model instead of rebuilding a central application facade.

Current independent application contexts remain independent where relevant:

- identity/access;
- clients;
- query/read models;
- mailboxes;
- devices;
- profiles/generations under their currently accepted owner until an explicitly justified extraction is accepted.

A new crate is justified only when it creates meaningful compile-time isolation or a durable independent application context. One-crate-per-function fragmentation is prohibited.

### 3.3 Public contracts

For browser-facing/public HTTP contracts:

- Rust remains the source of truth;
- generated OpenAPI/TypeScript artifacts remain deterministic;
- frontend transport DTOs are not handwritten duplicates;
- provider-specific request/response objects do not become public application contracts;
- existing accepted v1 compatibility rules remain fail closed.

### 3.4 Transactional authority

Whenever the business invariant spans records that can be committed in one D1 transaction, it must be committed in one governed D1 transaction.

Where a workflow spans D1, R2, Queue, Durable Object, external provider or Windows process boundaries, it must use durable-before-side-effect, idempotency, fencing/versioning and reconciliation rather than pretend distributed transactions exist.

### 3.5 Authorization ordering

The default command/query order is:

1. authenticate/resolve actor and tenant;
2. verify ACTIVE membership;
3. evaluate resource/capability authorization;
4. validate resource association/eligibility when required;
5. load or mutate protected data;
6. call provider/runtime side effects only after the durable application decision that authorizes them.

Unauthorized callers must not learn existence through differentiated resource/provider errors when neutral disclosure is required.

### 3.6 Developer readability standard

Every batch must leave a future developer able to answer, from code and docs:

- which bounded capability owns this behavior;
- which domain invariant applies;
- which use case sequences it;
- which port abstracts the external dependency;
- which adapter implements that port;
- which app/composition root wires it;
- which generated contract exposes it;
- which tests prove positive, negative, replay and failure behavior;
- which document is the current authority.

If the answer depends on tribal knowledge, hidden setup or a giant cross-capability facade, the batch is not done.

## 4. Current repository-owned gaps

The post-closeout verification against accepted `main` found the following current blockers.

### F1 — P1 — ACTIVE Member cannot currently create Client/Profile as required

Current create authorization is Owner-only. The confirmed product behavior requires ACTIVE Member self-service creation with atomic creator grants and no tenant-wide visibility.

### F2 — P1 — Mailbox-to-client association is not durable authority

Current Client Mail eligibility can match an active Client and active mailbox in the same tenant without a persisted active mailbox-to-client relationship. The current D1 eligibility path also requires Owner role, preventing the intended granted Member behavior.

### F3 — P1 — Required outbound mail capability is incomplete

The currently inspected Client Mail product surface proves read/search/get-message behavior, not the full compose/reply/reply-all/forward/send workflow now confirmed as required product scope.

### F4 — P1 — Cloudflare production release architecture is not implemented

The repository has permanent CI/acceptance workflows but no accepted production deployment/promotion pipeline that deterministically publishes Worker + Static Assets + bindings/migrations with exact-head provenance and safe database-release ordering.

### F5 — P1 — Windows Profile Bridge updater is not implemented as a production delivery path

Repository-local Windows Bridge/runtime code exists, but the trusted signed-manifest, staged side-by-side update, activation, health validation, rollback and last-known-good delivery path required before external signing/update evidence is missing.

### F6 — P2 — Mailbox credential onboarding/re-auth lifecycle is incomplete as an operator product flow

Bindings use opaque secret handles by design, but the repository must define and implement enough Gmail/IMAP/SMTP onboarding, refresh/re-auth/revoke and secret-handle provisioning UX/operations for Phase 2J to execute without undocumented manual setup.

Repository-owned severity at plan creation: **P0 = 0, P1 = 5, P2 = 1**.

Severity counts are current-blocker accounting, not immutable historical findings. Batch F must recompute them from the final repository state rather than simply decrementing counters.

## 5. Target runtime and release topology

### 5.1 Browser/cloud topology

```text
Browser
  -> Cloudflare Access
  -> one production origin
       -> Worker dynamic routes: /api/* /auth/* /bridge/*
       -> Workers Static Assets: React/Vite dist for SPA routes
       -> D1: business/catalog/audit/outbox/release metadata
       -> Durable Objects: coordination only
       -> Queue/Scheduled consumers
       -> R2: immutable encrypted artifacts/evidence as already governed
       -> secret resolver/service bindings
       -> Gmail API / IMAP+SMTP outer adapters where selected
```

Rules:

- dynamic namespaces never fall through to SPA assets;
- SPA fallback applies only to intended static/client-side routes;
- UI and Worker artifact versions are released together from one immutable source revision;
- environment-specific bindings live outside domain/application policy;
- staging and production use isolated Cloudflare resources and secrets;
- production evidence records artifact identity, environment and promotion event without leaking secrets.

### 5.2 Windows topology

```text
release publisher
  -> immutable signed release manifest
  -> immutable Bridge/runtime artifacts

Profile Bridge current version
  -> fetch metadata
  -> verify manifest signature + artifact digest
  -> stage side-by-side
  -> verify package
  -> activate only at safe lifecycle boundary
  -> bounded health probe
  -> mark known-good OR rollback to previous known-good
```

Rules:

- updater logic is not browser-profile domain policy;
- release verification belongs to a dedicated application/adapter boundary;
- updater never mutates the active installation in place before verification;
- loss of network or process crash at any step must produce a recoverable state;
- rollback never rolls back profile generations/data semantics unsafely;
- signed update metadata is authoritative; transport TLS alone is insufficient artifact trust.

## 6. Data and authority model additions

### 6.1 Creator grant transaction

Client/Profile creation requires one application transaction contract whose durable outcome includes both resource and grant.

Required invariants:

- one idempotency key identifies one creation intent;
- actor must be ACTIVE in tenant at command evaluation;
- generated resource ID and creator grant refer to the same tenant/resource;
- replay returns the accepted prior result rather than inserting a duplicate resource/grant;
- any validation/authorization/constraint failure leaves neither half committed;
- audit/outbox describes one governed business command and does not contain sensitive payloads;
- Owner creation follows the same durable model even if Owner already has administrative authority, so creator-access semantics stay explicit and predictable.

Preferred ownership:

- pure authorization/creator-grant decision in the appropriate inner domain/application policy;
- transaction envelope in the owning client/profile use case;
- D1 adapter implements the atomic persistence port;
- Worker/frontend merely expose the result.

Do not solve this by making all ACTIVE Members tenant-wide resource readers.

### 6.2 Mailbox-client association aggregate/relationship

Introduce an explicit durable mailbox-client relationship concept with a stable opaque relationship identity or equivalent versioned row semantics.

Minimum authoritative fields:

- tenant id;
- mailbox binding id;
- client id;
- state (`ACTIVE`, historical inactive/released state or equivalent);
- monotonically checked version/revision;
- created/bound metadata;
- released/rebound metadata sufficient for audit without sensitive credential data.

Schema invariants:

- no cross-tenant relationship;
- at most one active Client association per mailbox binding;
- a Client may have many active mailbox associations;
- relationship state changes are conflict/version checked;
- mailbox revoke makes the relationship ineligible even if historical association remains;
- archived/ineligible Client state is handled fail closed;
- deleting history to implement rebind is prohibited.

Application commands:

- bind mailbox to client;
- unbind mailbox from client;
- rebind mailbox from one client to another using an explicit expected current state/version;
- query/list eligible mailboxes for a client through grant-safe projection.

Authorization:

- Owner may administer associations but may not bypass association when invoking Client Mail;
- an ACTIVE granted Member may operate only through the Client access/capability granted to that actor;
- profile assignment never grants mailbox access;
- mailbox association never grants Client access.

### 6.3 Outbound mail command model

Outbound mail must be provider-neutral at domain/application boundaries.

Minimum application intents:

- compose;
- reply;
- reply-all;
- forward.

Provider-neutral command data should model only concepts the application actually owns, for example:

- client id;
- mailbox binding id;
- recipient sets;
- subject;
- text body and/or sanitized supported HTML representation according to the accepted privacy model;
- provider message/thread reference when needed for reply/forward;
- idempotency key;
- expected association/authorization context as application inputs, never trusted from the browser as proof.

Provider output remains an adapter translation, e.g. accepted provider message id/thread id and bounded metadata. Provider tokens/raw protocol objects never become canonical domain data.

Sending policy:

- authorization + active mailbox-client association precede provider access;
- durable send intent/outbox state precedes non-idempotent external side effects where the provider semantics require recovery;
- retries must avoid duplicate sends to the extent supported by provider identifiers/idempotency reconciliation;
- ambiguous provider outcomes become explicit recoverable states, not blind resend loops;
- audit records metadata required for accountability but never credentials and should avoid message body persistence unless explicitly required by later policy;
- browser fallback send, if retained, is a distinct device/runtime lane behind the same provider-neutral application intent and must not become the reference implementation for cloud mail.

## 7. Ordered implementation batches

No mega-PR is allowed. Each batch starts from the latest accepted `main`, has one primary architecture purpose, and must be independently reviewable.

The letters below are dependency order, not permission to work several write-heavy branches in parallel against stale authority.

## Batch 0 — Restore truthful current repository authority

### Goal

Make repository documentation/machine-readable status agree with the newly discovered repository-owned blockers before feature implementation resumes.

### Required changes

Create/accept this plan as the active pre-2J product-readiness follow-up and update current authority consistently:

- `docs/status.json`:
  - preserve accepted product phase Phase 2I;
  - preserve R1-R9 closed architecture-remediation history;
  - add a distinct active product-readiness remediation authority pointing to this document and issue #203;
  - record repository-owned P0=0, P1=5, P2=1 at plan activation;
  - set Phase 2J to blocked/pending repository remediation;
  - keep `production_ready=false`;
  - update compatibility `next_repository_step` to the active remediation, not Phase 2J;
- `README.md`, `docs/README.md`, `docs/INDEX.md`, `docs/DEVELOPMENT_PLAN.md`:
  - state that R1-R9 closeout remains accepted history;
  - state that the post-closeout product-readiness follow-up is active and blocks Phase 2J;
  - link this plan and issue #203;
- `scripts/check-documentation-authority.py`:
  - fail closed on the new current status;
  - preserve negative fixtures for false Phase 2J activation and premature production readiness;
  - preserve R1-R9 history checks;
- `scripts/generate-architecture-inventory.py` and generated inventory when required:
  - register this plan as current pre-2J execution authority while retaining the older plan as accepted closeout history;
  - require the correct blocked Phase 2J marker;
- no product implementation;
- no `openapi/v1/**` change;
- no weakening/removal of permanent CI workflows.

### Acceptance

- all current docs agree on Phase 2I accepted / follow-up active / Phase 2J blocked / production false;
- documentation-authority negative self-test rejects a false unblocked/accepted Phase 2J state;
- generated architecture inventory is deterministic/current;
- R1-R9 accepted history remains intact;
- exact-head permanent CI is complete and successful;
- squash merge only after ready/post-ready interlocks.

## Batch A — Member autonomy and atomic creator grants

Batch A should be split if needed so Client and Profile ownership remain independently reviewable.

### A1 — Client creator grant

#### Goal

Allow an ACTIVE Member to create a Client and atomically receive the explicit required Client grant.

#### Work

- define/adjust inner authorization decision for `client.create`;
- keep inactive/suspended/revoked/non-member rejection fail closed;
- define creator grant role/capability explicitly;
- extend the Client creation persistence port to own resource + grant + audit/outbox as one transaction outcome;
- implement D1 transaction semantics and uniqueness/idempotency handling;
- keep transport DTOs provider-neutral and generated where public contract evolution is needed;
- update UI wording from Owner-governed to allowed member creation without making UI role checks authoritative;
- ensure Owner admin controls remain unchanged.

#### Required tests

Positive:

- ACTIVE Owner create;
- ACTIVE Member create;
- creator can immediately read/operate on created Client according to grant;
- unrelated Member cannot see the new Client;
- replay returns the same resource/grant outcome.

Negative/failure:

- suspended Member;
- revoked/non-member;
- cross-tenant actor;
- transaction failure before grant insertion;
- transaction failure after resource insertion attempt;
- duplicate idempotency key with mismatched payload;
- grant constraint failure;
- no partial durable state after any failed transaction.

### A2 — Profile creator grant

Mirror the Client pattern without creating a shared cross-capability mega-transaction abstraction.

#### Goal

Allow an ACTIVE Member to create a Profile and atomically receive the intended explicit Profile operator grant.

#### Work/tests

Apply the same authorization, atomicity, replay, cross-tenant and partial-failure standards as A1 while preserving Profile Catalog/Generation ownership boundaries.

Profile-to-Client assignment remains a business relationship and must not be treated as authorization.

### Batch A acceptance

- owner-only create behavior is removed only where explicitly intended;
- creator access is always explicit and durable;
- no tenant-wide visibility is introduced;
- transaction/replay negative evidence is permanent;
- UI/backend authorization semantics agree;
- no capability ownership regression into a central facade.

## Batch B — Mailbox-to-client relationship authority and Client Mail authorization

### B1 — Domain/application relationship model

#### Goal

Define the provider-neutral relationship state and commands before writing SQL/UI around it.

#### Work

- add relationship value/state types in the correct inner owner;
- define bind/unbind/rebind decisions and expected-version behavior;
- define authorization intents separately from association rules;
- define ports for command persistence and grant-safe read eligibility;
- ensure Gmail/IMAP/Browser provider concepts are absent from the inner relationship model.

### B2 — D1 schema and transaction implementation

#### Goal

Persist one active Client per mailbox with historical auditability and strong tenant constraints.

#### Work

- additive D1 migration;
- tenant-first foreign-key/constraint strategy consistent with existing schema policy;
- unique active-association enforcement using a D1-compatible design;
- version/conflict columns or equivalent CAS semantics;
- governed bind/unbind/rebind transaction + audit/outbox;
- migration must not synthesize arbitrary Client links for existing mailboxes.

Existing mailbox bindings after migration should default to **unassigned** unless there is already a trustworthy repository-owned association source. Guessing from UI/query usage is forbidden.

### B3 — Repair Client Mail eligibility

#### Goal

Replace tenant-only mailbox/client coincidence with real resource authorization + association eligibility.

Required evaluation:

1. ACTIVE membership;
2. Client grant/capability;
3. active Client;
4. active mailbox binding;
5. active mailbox-to-that-Client relationship;
6. allowed provider lane for requested operation;
7. only then provider search/get/send execution.

Remove the current implicit TenantOwner-only dependency for ordinary granted Member usage while preserving Owner administrative capabilities.

### B4 — UI association management

#### Goal

Make mailbox assignment understandable and explicit.

UX must support:

- view unassigned mailboxes;
- view mailboxes assigned to Client;
- bind;
- unbind;
- rebind with clear conflict/current-owner feedback;
- revoked/inactive state display;
- permission-aware commands;
- no raw credential exposure.

Do not hide authorization mistakes by filtering only in React; backend projections remain grant safe.

### Batch B permanent evidence

Positive:

- Client with multiple mailboxes;
- mailbox unassigned then bound;
- rebind A -> B with preserved history;
- Owner administration;
- granted Member reads/sends through a mailbox associated with granted Client.

Negative:

- same-tenant mailbox associated with a different Client;
- cross-tenant association attempt;
- stale expected version/rebind race;
- revoked mailbox;
- archived/ineligible Client;
- revoked Client grant;
- suspended membership;
- assignment-as-ACL shortcut;
- Owner attempting Client Mail through a mailbox not associated with the requested Client;
- duplicate active association.

## Batch C — Mailbox onboarding and complete cloud mail UX

Batch C establishes an executable repository-owned operator flow. Production credentials/provider certification remain Phase 2J evidence.

### C1 — Provider-neutral account onboarding contract

Define a lifecycle that can represent:

- onboarding pending;
- active credential handle;
- refresh/re-auth required;
- revoked/disabled;
- terminal configuration error where appropriate.

Credentials remain outside browser-visible/domain-readable storage. Application code receives opaque secret/token references through ports.

### C2 — Gmail OAuth/API onboarding

Implement/document the supported Gmail ceremony at the correct outer boundary:

- authorization initiation/state/PKCE or equivalent approved OAuth security mechanism;
- callback completion;
- secret/token storage through the approved secret resolver boundary;
- refresh-token lifecycle;
- revoke/re-auth path;
- minimum required scopes;
- no token logging;
- bounded operator status projection.

Do not store OAuth tokens in D1 application tables or browser storage merely for convenience.

### C3 — IMAP/SMTP onboarding for Outlook/standards-based mailboxes

The repository-owned contract must explicitly distinguish:

- IMAP read/search capability;
- SMTP send capability;
- authentication/credential type supported by the application;
- transport security requirements;
- re-auth/rotation lifecycle.

Microsoft Graph must not be claimed unless separately implemented and accepted. Outlook support through standards-based IMAP/SMTP must be described exactly as such.

### C4 — Provider-neutral outbound mail use cases

Introduce bounded send workflows owned by the mailbox/mail application context, not the Worker transport.

Required operations:

- new message;
- reply;
- reply-all;
- forward.

Required semantics:

- authorization and active mailbox-client relationship first;
- deterministic request validation;
- provider-neutral recipient/body/subject model;
- durable send intent/outbox when needed for at-least-once infrastructure;
- duplicate/retry suppression/reconciliation;
- explicit ambiguous outcome state;
- metadata-only audit;
- stable application problem mapping.

### C5 — Gmail API send adapter

Translate the provider-neutral send intent to Gmail API semantics in an outer adapter.

Must handle:

- reply/thread references correctly;
- provider error classification into retryable/non-retryable/reauth-required;
- rate-limit/backoff without moving Gmail concepts inward;
- bounded provider response projection;
- test fakes/fixtures with no live credentials in CI.

### C6 — SMTP send adapter for IMAP/SMTP mailbox lane

Implement SMTP transport as an outer adapter where this is the selected standards-based lane.

Must handle:

- TLS/security policy;
- authentication through opaque secret resolver material;
- retry/ambiguous-send semantics conservatively;
- message-id/provider response reconciliation where available;
- no protocol object leakage into application/domain layers.

### C7 — Compose/reply/reply-all/forward UI

Frontend feature-owned UX must include:

- compose editor;
- reply/reply-all/forward affordances from message view;
- mailbox selection constrained to eligible Client mailboxes;
- recipient validation;
- send progress/outcome/retry-safe status;
- re-auth-required guidance;
- prevention of accidental cross-client mailbox switching;
- body handling consistent with data classification/privacy rules;
- no token/credential/secret handle display except intentionally opaque admin identifiers where already allowed.

### Batch C evidence

- deterministic fake Gmail send success/retry/permanent failure/reauth cases;
- deterministic fake SMTP success/connection failure/ambiguous outcome cases;
- duplicate queue/delivery/replay does not create uncontrolled duplicate send;
- unauthorized and wrong-client mailbox calls never reach provider adapter;
- generated/public contracts remain single-authority;
- browser storage/telemetry does not persist sensitive mail body or credentials outside approved policy.

## Batch D — Cloudflare release, staging and production promotion architecture

This batch implements deployment machinery; it does not claim real production acceptance.

### D1 — Canonical Wrangler/application configuration

Implement one repository-owned deployment configuration for:

- Rust Worker artifact;
- React/Vite `dist` via Workers Static Assets;
- D1 bindings;
- R2 bindings;
- Durable Object bindings/migrations;
- Queue producers/consumers;
- service bindings/secret resolver references;
- environment-specific names/ids supplied through controlled configuration.

Staging and production must be isolated resources. Local development must not silently target production resources.

### D2 — Immutable release build

Produce a release manifest from an exact Git source revision containing at minimum:

- source commit SHA;
- frontend asset bundle digest or deterministic asset manifest digest;
- Worker artifact digest;
- generated contract/version identity;
- migration set/version expected by the release;
- build/toolchain identity sufficient for provenance;
- release identifier.

Build once for promotion when feasible; avoid rebuilding different bits for staging and production under the same release identity.

### D3 — Deployment/promotion workflow

Permanent GitHub Actions workflow should separate:

1. accepted source/build;
2. staging deploy;
3. staging smoke/health checks;
4. protected production promotion;
5. post-promotion verification/attestation.

Requirements:

- no deploy from unreviewed arbitrary fork code with production secrets;
- environment protections/permissions are explicit;
- production promotion names exact immutable release/source;
- a failed staging deploy cannot partially become production;
- deployment workflow does not weaken existing 12 permanent CI/acceptance gates;
- workflow run/job evidence must be real `completed/success`, not zero-job/skipped pseudo-green evidence.

### D4 — D1 migration compatibility policy

D1 migration design follows expand/migrate/contract principles where schema evolution can overlap versions.

For every migration used by a release, record:

- whether old code remains compatible after migration;
- whether new code is compatible before migration;
- ordering requirement;
- backfill/reconciliation if any;
- failure recovery;
- whether rollback is code-only or requires fail-forward schema repair.

Destructive/incompatible contraction must not be coupled to the same promotion that first introduces the new representation unless explicitly proven safe.

Production rollback means selecting a compatible known-good release, not blindly reversing D1 SQL.

### D5 — Static asset routing safety

Permanent tests must prove:

- known SPA routes return SPA assets;
- `/api/*`, `/auth/*`, `/bridge/*` never fall through to `index.html`;
- unsupported API methods/versions fail closed;
- stale/static asset caching cannot pair an incompatible UI with API under one release beyond the supported compatibility window;
- content/security headers remain appropriate.

### D6 — Deployment operator documentation

Document exact commands/workflow controls for:

- create/configure staging;
- provision bindings/secrets without committing them;
- build release;
- deploy staging;
- run smoke checks;
- promote production;
- inspect release identity;
- rollback to compatible known-good;
- recover from migration/deploy failure.

Phase 2J should execute these instructions, not invent them.

## Batch E — Windows Profile Bridge trusted update delivery implementation

### E1 — Release/update contract

Define a versioned provider-neutral update manifest contract containing at minimum:

- release version/id;
- artifact URI/reference;
- exact artifact digest;
- artifact size;
- minimum/maximum compatible updater/runtime protocol versions when needed;
- channel/environment;
- signature/key identifier;
- signed canonical payload rules;
- optional rollback/known-good metadata only if it cannot create downgrade ambiguity.

Manifest parsing is strict and bounded. Unknown critical semantics fail closed.

### E2 — Signature verification/key rotation

Implement verification behind an inner port/application policy + Windows/crypto outer adapter boundary.

Requirements:

- trusted public-key material/key-id policy is explicit;
- artifact digest verified independently of transport;
- key rotation supports an overlap path without accepting arbitrary new trust roots;
- expired/revoked/disallowed key ids fail closed according to policy;
- no private signing key ships in the client.

Real trusted Windows certificate/signing infrastructure remains Phase 2J external evidence, but verification code and release contract are repository-owned.

### E3 — Download and side-by-side staging

Updater must:

- download to a non-active staging location;
- enforce safe paths and bounded sizes;
- verify manifest/artifact before activation;
- never overwrite running binaries in place as the first step;
- preserve current known-good installation until candidate validation succeeds;
- clean abandoned staging state under a bounded recovery policy.

### E4 — Safe activation lifecycle

Define the exact activation boundary with Profile Bridge/runtime ownership.

Preferred rule:

- no forced switch while the Bridge owns an active profile writer/session transition;
- candidate becomes activatable only at a safe quiescent lifecycle boundary or a specifically governed restart handoff;
- updater and browser/profile materialization locks cannot deadlock or bypass existing writer/fencing invariants.

### E5 — Health check and automatic rollback

After candidate launch/activation:

- run bounded local health/protocol checks;
- if successful, mark candidate known-good;
- if startup/protocol health fails, restore the prior known-good version;
- repeated failure enters an explicit recovery state rather than an infinite update loop;
- rollback must not roll back or delete newer browser profile generations/data.

### E6 — Release publisher integration

Repository release tooling must publish:

- immutable Windows artifacts;
- exact digests;
- signed manifest input/output contract;
- source SHA/build provenance;
- compatibility metadata.

Signing secrets/keys stay in protected external infrastructure.

### E7 — Windows updater permanent tests

At minimum:

- valid update;
- digest mismatch;
- manifest signature mismatch;
- unknown key;
- truncated artifact;
- interrupted download;
- process active/not safe to switch;
- candidate fails health probe;
- rollback succeeds;
- rollback itself interrupted/recoverable;
- retained last-known-good;
- stale/downgrade manifest policy;
- runtime protocol incompatibility;
- no profile data deletion/corruption from updater transitions.

Repository-local synthetic tests prove implementation correctness only. Physical Windows host, trusted real signing and operational rollout remain Phase 2J external evidence.

## Batch F — Full re-audit and pre-2J re-closeout

Batch F is not a paperwork decrement. It is a fresh repository audit against the product decisions and architecture rules in this plan.

### F1 — Correctness/security audit

Re-check at least:

- Member creation authorization and atomic creator grant;
- grant revoke and neutral disclosure;
- mailbox-client cardinality and history;
- Client Mail eligibility;
- outbound send authorization, replay and ambiguous outcomes;
- secret/token boundaries;
- Cloudflare dynamic/static route separation;
- deployment permissions/provenance/migration safety;
- Windows updater signature/digest/staging/switch/rollback;
- tenant isolation;
- privacy/data classification;
- no provider SDK leakage inward;
- no new central facades or capability ownership regressions.

### F2 — Architecture/readability audit

A senior developer must be able to trace every new capability through:

`domain decision -> application use case -> port -> adapter -> composition root -> public/generated contract -> feature UI -> permanent tests`.

Reject:

- circular dependencies;
- giant shared endpoint/DTO registries;
- infrastructure-owned policy;
- duplicated authorization implementations;
- hidden provider-specific branches in generic domain objects;
- undocumented environment/manual setup;
- unexplained compatibility shims with no removal/ownership rule.

### F3 — Documentation authority

Update canonical docs to the final accepted truth:

- remediation status closed only if repository-owned P0=0 and P1=0;
- Phase 2J unblocked but not started only after this fresh audit;
- `production_ready=false` remains mandatory;
- issue #171 is refreshed to the new exact accepted `main` and previous stale base/evidence is not reused;
- issue #203 records closeout provenance;
- accepted Phase 2I ledger remains historical authority until Phase 2J itself is accepted.

P2 items may remain only if they are explicitly non-blocking for Phase 2J execution and do not represent required product behavior, correctness, security, deployment or update gaps. Any required behavior still missing is a blocker regardless of label.

### F4 — Exact-head acceptance evidence

For the final candidate head:

- branch is based on latest accepted `main` with `behind_by=0`;
- no unresolved reviews/threads/Conversation blockers;
- every permanent workflow expected by repository policy has real jobs;
- every required job is `completed/success` on the exact final candidate head;
- no candidate commit is added after evidence collection without rerunning evidence;
- no temporary workflow remains;
- no frozen contract drift is hidden;
- pre-ready interlock is fresh;
- PR is marked ready only after candidate evidence;
- post-ready interlock is fresh;
- exact-head squash merge is used;
- accepted merge SHA and source-head evidence are recorded.

Only after Batch F is accepted may Phase 2J become the next executable phase.

## 8. Public API and contract-change rules during remediation

### 8.1 Frozen accepted v1

`openapi/v1/**` is treated as frozen accepted compatibility authority. A required new public operation should first attempt a compatible additive bounded contract in the currently accepted Rust -> generated artifact architecture.

If a change would be breaking, do not hide it in remediation. Open a separately justified API versioning/compatibility decision and prove migration strategy.

### 8.2 New mailbox relationship/send surfaces

New public wire DTOs must have one Rust-owned authority and deterministic generated TypeScript.

Do not introduce handwritten React interfaces for:

- mailbox association commands/projections;
- outbound send requests/results;
- onboarding status/state;
- release/update public status if exposed.

### 8.3 Error semantics

Provider-specific errors translate at adapter/application boundaries into stable typed problems such as:

- unauthorized/not found under neutral disclosure;
- conflict/stale version;
- validation;
- mailbox not associated/eligible;
- provider re-auth required;
- retryable provider unavailable;
- ambiguous send outcome requiring reconciliation;
- updater integrity/signature failure.

Do not expose raw Gmail/SMTP/Cloudflare/Windows errors directly to browser clients.

## 9. D1 migration and persistence standards

Every new migration must have:

- deterministic numbered file;
- tenant-safe constraints;
- explicit existing-row behavior;
- no secret/plaintext migration into D1;
- forward compatibility analysis;
- failure/retry behavior;
- repository-local schema tests;
- architecture inventory/freshness updates where governed.

For mailbox-client association, existing mailbox rows must not be guessed into a Client. Unassigned is the safe default.

For creator grants, prefer changes to transaction behavior over denormalized inferred access fields.

For deployment metadata, production secrets/tokens/credentials are never release-table values.

## 10. Mail privacy and security rules

The current data-classification/privacy model remains authoritative unless separately changed.

During this remediation:

- credentials, OAuth tokens, passwords and signing private keys never enter frontend storage, logs or ordinary D1 business rows;
- opaque secret handles remain non-secret references but should not be needlessly exposed;
- message bodies remain transient/provider-owned by default unless an explicit product/storage decision changes that;
- outbound body content must not be copied to telemetry/audit for convenience;
- audit may retain metadata such as actor, client, mailbox binding, operation type, provider outcome class, timestamps and stable provider references where policy permits;
- sanitization rules for rendered HTML remain enforced;
- cross-client mailbox confusion is treated as an authorization/security defect, not a UX defect.

## 11. CI and testing strategy

Each batch must add the narrowest permanent evidence that proves its new invariant while retaining existing regression lanes.

### 11.1 Unit/pure policy tests

Use for:

- membership/create authorization decisions;
- mailbox association state transitions;
- send intent validation/retry decisions;
- updater state machine and release compatibility decisions.

### 11.2 Application tests with fake ports

Use for:

- authorization-before-provider sequencing;
- atomic/replay orchestration intent;
- wrong-client mailbox rejection before provider call;
- provider error classification;
- send ambiguous-outcome reconciliation;
- updater stage/activate/rollback orchestration.

### 11.3 Adapter/integration tests

Use for:

- D1 transactions/constraints/races;
- Gmail adapter translation with deterministic fake HTTP/provider fixtures;
- IMAP/SMTP adapter translation with deterministic fake protocol fixtures;
- Wrangler/static routing configuration checks;
- Windows filesystem/process/update implementation behavior where CI can safely prove it.

### 11.4 End-to-end repository-local tests

Cover representative flows:

- Member creates Client -> auto grant -> creates/gets Profile -> explicit assignment remains separate;
- Owner/onboarding creates mailbox -> binds mailbox to Client -> granted Member reads mail;
- granted Member composes/replies/sends through associated mailbox;
- wrong Client/mailbox combination fails before provider access;
- immutable release contains UI + Worker source identity;
- Windows candidate update fails health and returns to known-good without profile-data mutation.

### 11.5 Negative evidence is first-class

A positive happy path is insufficient for any authorization, transaction, migration, release or update invariant.

Required negative categories include:

- unauthorized;
- cross-tenant;
- cross-client;
- stale version/fencing;
- duplicate/replay;
- partial failure;
- provider unavailable/ambiguous;
- malformed/untrusted artifact;
- downgrade/incompatible release;
- route fallthrough;
- missing/incorrect documentation authority.

## 12. Release acceptance discipline for every remediation PR

Each PR must satisfy the repository's exact-head discipline.

1. branch from latest accepted `main`;
2. define one bounded issue/plan scope;
3. keep PR draft during active implementation;
4. update/rebase/merge latest `main` before candidate evidence so `behind_by=0`;
5. ensure no blocking review state or unresolved thread;
6. inspect the actual permanent workflow inventory, not only a green summary badge;
7. require real jobs and `completed/success` for every expected workflow on the exact candidate SHA;
8. any candidate commit invalidates earlier workflow evidence;
9. remove any temporary diagnostic workflow before final candidate;
10. verify frozen `openapi/v1/**` net diff is zero unless separately approved;
11. verify `production_ready=false`;
12. run fresh pre-ready interlock;
13. mark ready for review;
14. run/verify fresh post-ready interlock;
15. squash merge with expected exact head SHA;
16. record accepted source head, merge SHA and relevant workflow evidence in the issue/plan/checkpoint.

No batch may be accepted based on screenshots, historical runs, workflow names with zero jobs, or evidence from a superseded candidate SHA.

## 13. Documentation/ADR strategy

Create an ADR only for a durable architecture decision whose alternatives/tradeoffs matter beyond the implementation batch.

At minimum consider ADR coverage for:

- ACTIVE Member creator-grant authority if existing identity/access ADRs do not already encode it;
- mailbox-client relationship cardinality and assignment-vs-authorization separation;
- outbound mail delivery/idempotency/ambiguous outcome model;
- controlled Worker Static Assets release/promotion and D1 migration compatibility;
- Windows update trust/side-by-side/rollback contract.

Do not create ceremonial ADRs that merely repeat code. The normative plan should link to accepted ADRs once they exist.

Every accepted batch updates only the current docs that materially changed. Historical accepted evidence should not be rewritten to look as if the new semantics existed earlier.

## 14. Failure-mode matrix that must be closed before Batch F

| Area | Failure | Required behavior |
|---|---|---|
| Client/Profile create | persistence fails mid-command | no resource/grant half-state |
| Client/Profile create | duplicate idempotency replay | same accepted outcome, no duplicate resource |
| Membership | actor suspended between sessions | live command/query denied |
| Mailbox association | concurrent rebind | one version wins; stale writer gets conflict |
| Client Mail | mailbox belongs to another Client | reject before provider access |
| Client Mail | grant revoked | reject before provider access |
| Gmail/SMTP send | retryable provider failure | bounded retry policy without uncontrolled duplicates |
| Gmail/SMTP send | ambiguous provider outcome | explicit reconcile/recovery state, no blind resend |
| OAuth/credential | refresh/re-auth required | stable re-auth-required state, no token leak |
| Cloudflare release | staging deploy fails | production remains unchanged |
| D1 migration | migration applied but release fails | use compatible rollback or fail-forward plan; no blind down migration |
| Static assets | unknown API route | fail closed, never SPA fallback |
| Windows update | download interrupted | current known-good remains runnable |
| Windows update | digest/signature invalid | candidate never activates |
| Windows update | process/profile busy | defer safe switch; never break writer/session invariant |
| Windows update | candidate health fails | bounded automatic rollback to known-good |
| Windows update | rollback interrupted | recoverable updater state; preserve data |
| Documentation | status says 2J unblocked while blockers exist | permanent gate fails |

## 15. Risk register

### R-A — Authorization broadening accidentally becomes tenant-wide access

Mitigation: creator grant is explicit; existing grant-safe reads remain; negative unrelated-member tests mandatory.

### R-B — Mailbox association is implemented as another authorization source

Mitigation: separate types/use cases/ports for association and grants; permanent assignment-as-ACL negative tests.

### R-C — Provider-specific mail details contaminate core model

Mitigation: provider-neutral application commands and error classes; Gmail/SMTP only in adapters.

### R-D — Duplicate outbound mail under retries

Mitigation: durable send intent/reconciliation, provider reference tracking where safe, explicit ambiguous outcomes, replay tests.

### R-E — Cloudflare deployment pipeline couples irreversible migration to code rollback

Mitigation: expand/migrate/contract, compatibility metadata, staging first, production promotion, fail-forward strategy.

### R-F — Worker Static Assets accidentally serve SPA for API errors

Mitigation: dynamic namespace fail-closed routing tests and config review.

### R-G — Windows updater can corrupt active runtime/profile state

Mitigation: side-by-side staging, safe activation boundary, writer/session invariants, known-good rollback, no profile-data rollback.

### R-H — Remediation creates a new central mega-module

Mitigation: preserve capability application ownership, dependency gates and feature-owned frontend APIs; split PRs where ownership differs.

### R-I — Documentation returns to contradictory state

Mitigation: Batch 0 fail-closed current-authority checks and Batch F fresh closeout.

## 16. Recommended issue/PR decomposition

Issue #203 remains the umbrella blocker. Create bounded child issues or explicit issue sections for implementation slices rather than one giant PR.

Recommended sequence:

1. Batch 0 — canonical blocker/status/plan authority;
2. A1 — Client Member create + atomic creator grant;
3. A2 — Profile Member create + atomic creator grant;
4. B1/B2 — mailbox-client inner model + D1 relationship persistence;
5. B3/B4 — Client Mail eligibility/member authorization + UI association UX;
6. C1-C3 — onboarding/credential lifecycle;
7. C4-C6 — outbound application contract + Gmail/SMTP adapters;
8. C7 — outbound mail frontend UX and E2E evidence;
9. D1/D2 — Worker Static Assets/Wrangler + immutable release manifest;
10. D3-D6 — staging/production controlled promotion, D1 migration safety, route and operator docs;
11. E1-E3 — Windows signed update contract/verification/staging;
12. E4-E7 — activation/health/rollback/release integration/tests;
13. F — full re-audit and pre-2J closeout.

If a slice becomes too large for expert review, split it along application ownership or transaction boundaries, not arbitrary file-count boundaries.

Do not parallelize dependent D1/contract slices against stale assumptions. Independent documentation/research may proceed in parallel only when it cannot create conflicting current authority.

## 17. Definition of Done before Phase 2J may start

Phase 2J may be marked `unblocked_not_started` again only when all of the following are true on accepted `main`:

### Product behavior

- ACTIVE Member Client creation works with atomic creator grant;
- ACTIVE Member Profile creation works with atomic creator grant;
- unrelated Members remain unable to access those resources;
- mailbox has zero/one active Client association; Client has zero/many mailboxes;
- bind/unbind/rebind history and conflict rules are implemented;
- granted Member Client Mail works only through associated mailbox;
- compose/reply/reply-all/forward/send are repository-owned product functionality;
- Gmail API and standards-based IMAP/SMTP supported paths are accurately represented;
- onboarding/re-auth workflow is executable without hidden undocumented credential setup.

### Architecture

- clean inward dependency direction preserved;
- domain/application code remains provider-neutral;
- capability owners remain explicit;
- no new central backend/frontend facade;
- public DTO authority remains Rust -> generated contracts;
- assignment/association never becomes implicit authorization.

### Cloudflare delivery

- React UI uses Workers Static Assets under the canonical Worker deployment;
- staging and production resource configuration are isolated;
- immutable release/source provenance exists;
- controlled staging -> production promotion exists;
- D1 migration compatibility/rollback/fail-forward rules are executable;
- dynamic namespaces cannot fall through to SPA.

### Windows delivery

- updater contract, signature/digest verification, staged side-by-side install, safe activation, health validation and known-good rollback exist;
- failure recovery is permanently tested;
- repository implementation no longer relies on synthetic certification state-machine evidence as a substitute for an updater.

### Quality/evidence

- repository-owned P0=0;
- repository-owned P1=0;
- any remaining P2 is explicitly non-blocking and not missing required product/security/release behavior;
- all permanent workflows have real completed/success jobs at one exact final head;
- no temporary workflow remains;
- no unresolved review/thread blocker remains;
- current documentation is internally consistent;
- `production_ready=false` remains false.

Only then:

1. close issue #203 with exact accepted provenance;
2. refresh issue #171 to the new accepted `main` SHA;
3. mark Phase 2J `unblocked_not_started`;
4. begin Phase 2J external production evidence from that exact accepted base.

## 18. Phase 2J boundary after this plan

This plan intentionally does **not** prove:

- real isolated production Cloudflare resources;
- real production D1/R2/DO/Queue behavior;
- real Gmail/IMAP/SMTP accounts and provider quotas/behavior;
- physical Windows host/update rollout;
- trusted production signing infrastructure/certificate operation;
- production key protection/escrow restore;
- independent security/crypto review;
- production monitoring/on-call/runbook execution;
- real backup/recovery drills;
- staged real-user rollout.

Those remain Phase 2J external evidence.

The purpose of this remediation is to ensure Phase 2J validates an already designed and implemented product/release system rather than inventing missing product semantics or deployment/update architecture during production rollout.

## 19. Immediate next action after this plan is proposed

Do **not** start Batch A yet.

The immediate next action is **Batch 0**:

- make this document canonical current remediation authority;
- update `docs/status.json`, README/docs index/development plan, documentation-authority gate and generated architecture inventory consistently;
- restore a truthful machine-readable state in which Phase 2J is blocked by issue #203;
- preserve R1-R9 accepted history and `production_ready=false`;
- accept Batch 0 only through exact-head CI/interlocks and squash merge.

After Batch 0 is accepted, begin A1 from the new accepted `main`.
