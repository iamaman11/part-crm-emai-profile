# Architecture Re-baseline v3 — Canonical Candidate Plan

**Status:** proposed forward execution authority; becomes normative only through a separately accepted AR-1 authority transaction  
**Audit base:** `5be54c2989dbfa22822d3692e22156f23d2a4602`  
**Tracking:** #266; subordinate pre-production authority work #268  
**Production readiness:** `production_ready=false`  
**Architecture completion:** `architecture_complete=false`  
**Production Core v1 gate:** `BLOCKED`

> This is delta remediation of an already mature repository, not a greenfield rewrite. Accepted R1–R9, product phases through 2I, Pre-2J A/B/C/D1/D2 and repository-side D3 remain immutable history/evidence. Only unaccepted future execution authority changes after AR-1.

## 1. Ultimate target

The repository is architecture-complete when a new engineer can determine, without issue archaeology or tribal knowledge:

- which capability owns each business rule and contract;
- which use case owns authorization intent, orchestration, concurrency and idempotency;
- which application port expresses each external need;
- which adapter owns each D1/R2/Queue/DO/OAuth/provider/Windows translation;
- where concrete wiring is allowed;
- which runtime resource has which producer, consumer and failure boundary;
- which document is current, target, historical evidence or runbook;
- which generator owns every OpenAPI/TypeScript projection;
- which executable tool is validator, generator, operator reader, remote mutator, test-data mutator, synthetic evidence, research-only or historical quarantine;
- which exact source/artifact/schema/topology is released;
- which capability is source-present versus production-enabled in each release profile;
- which credential metadata is repository-safe policy and which live identity/state must remain protected;
- how credentials, schemas, resources and application versions rotate, recover and roll back independently;
- which exact gate authorizes production mutation and which later evidence makes a release actually production-ready.

The target is deliberately boring: few concepts, explicit ownership, one mutable authority per lifecycle, strong fitness tests, no architecture framework added merely for appearance, and no production rollout hidden inside architecture remediation.

## 2. Current verified governance facts from AR-0

At the audit base and current PR #267 candidate:

- `main` is exact SHA `5be54c2989dbfa22822d3692e22156f23d2a4602`;
- branch metadata reports `main` as `protected=false` and repository rulesets are empty;
- the GitHub `production` Environment exists, has a required reviewer, `can_admins_bypass=false`, and its custom deployment branch policy allows only `main`;
- this means production-environment protection is materially stronger than repository merge protection, but the two are different security boundaries;
- issue #251 (Pre-2J D3 external promotion gate) is currently open and remains historical/current predecessor evidence until the authority cutover explicitly classifies it;
- `architecture/inventory.json` is already the correct machine-authority foundation, but current repository search does not expose `production_enabled` / `release_profile` semantics;
- Rust `opsctl` does not yet exist in accepted `main`;
- full per-file Python disposition with exact LOC/callers/authority has not yet been proven and is therefore an explicit AR deliverable, not an assumed completed fact.

These facts are inputs to the plan; they are not claims that AR-0 has already changed repository/runtime governance.

## 3. Non-negotiable invariants

1. **One mutable concern = one authority.** Two independently legitimate mutators for the same lifecycle are forbidden.
2. **Dependencies point inward.** Domain/application code never depends on Cloudflare, D1, Wrangler, provider SDKs, Windows or React implementation details.
3. **Application release != persistent infrastructure lifecycle.** Deploying code must not implicitly create/recreate durable resources.
4. **Credential/key lifecycle != infrastructure lifecycle.** Rotation must not recreate D1/R2/Worker/Queue resources.
5. **Database lifecycle != code rollback.** Compatibility windows are explicit; rollback cannot assume schema rollback.
6. **Wrangler remains Worker deployment/configuration authority.** Terraform is excluded.
7. **No generic hidden IaC state.** `opsctl` stays project-specific; no DSL, generic graph, plugin framework, state backend or automatic destructive reconciliation.
8. **Public v1 contracts do not change for internal refactoring.** Contract changes require owning capability authority and compatibility/version proof.
9. **Build once per component.** A release identity never points to rebuilt bytes.
10. **Release set, not fictional monolith.** Multiple deployables may be promoted under one accepted source/release-set authority.
11. **Staging/production persistent identities are isolated.** Shared identity requires an explicit accepted exception.
12. **Recovery and rotation are executable capabilities.** Prose-only procedures are insufficient.
13. **Windows updater is first-class.** Existing Profile Bridge runtime does not prove update delivery.
14. **Historical truth is immutable.** Old accepted decisions are classified, not rewritten.
15. **Target != accepted.** A target document may never silently masquerade as currently composed behavior.
16. **Research executables cannot masquerade as supported runtime paths.** Historical/external-research tools are explicitly classified and guarded.
17. **Secret metadata is classified field-by-field.** “Metadata” is not automatically safe for Git/evidence.
18. **Refresh is single-authority and race-safe.** OAuth refresh for one credential handle cannot be an uncoordinated read-refresh-upsert race.
19. **Source-present != production-enabled.** Capability code may remain in `main` while production activation is fail-closed and machine-authorized separately.
20. **No production mutation before final architecture audit and closeout.** AR-0..AR-17 may use rehearsal/staging/disposable environments, but production provisioning/promotion begins only in PC-1.
21. **Architecture-complete != gate-authorized != production-ready.** These are independent machine states.
22. `production_ready=false` remains mandatory until PC-1 external production evidence explicitly changes it.

## 4. Target dependency model

```text
primitives
  <- contracts
  <- domains
  <- application ports
  <- capability use cases
  <- outer adapters
  <- apps / explicit composition roots

frontend
  <- generated public contracts + feature public APIs

operations
  GitHub Actions/Environments -> opsctl/project tooling -> Wrangler/provider API
  (outside business dependency direction)
```

### Domain

Pure provider-independent state/policy. No Worker/D1/R2/Queue/DO/HTTP/OAuth SDK/Windows/React types.

### Application/use cases

Own authorization intent, orchestration, idempotency/replay policy, ambiguous-outcome policy, concurrency/fencing and transaction intent. Ports describe application needs rather than provider APIs.

### Adapters

Own D1/R2/Queue/DO/Access/OAuth/mail/provider/Windows implementation and outer-to-inner translation. Provider DTOs, D1 rows and runtime SDK types never cross inward.

### Composition roots

Concrete adapter construction is permitted only in explicitly enumerated app composition roots or bounded diagnostic composition roots. Transport modules may parse/authenticate/map but must not become a second wiring authority.

## 5. Preserve what is already strong

The current repository already has substantial 10/10 foundations and these are preserved:

- provider-independent domain crates;
- capability-specific use-case contexts;
- `application-ports` and outer `cloudflare-adapters`;
- architecture dependency allowlists and negative fixtures;
- capability-layout/ownership fitness tests;
- several thin-Worker checks;
- React feature public-API boundaries and sibling-feature negative checks;
- Rust -> OpenAPI -> generated TypeScript discipline;
- explicit composed/library/synthetic/target/external evidence levels;
- canonical control-plane/resolver Wrangler config;
- dedicated mailbox-secret-resolver boundary and resolver D1;
- immutable control-plane/SPA and resolver releases with no-rebuild promotion;
- catalog/resolver D1 migration/bootstrap foundations;
- versioned contact/resolver keyrings and resolver key reconciliation ledger;
- versioned mailbox-onboarding state machine including `ReauthRequired`;
- Google/Microsoft OAuth ceremonies and actual resolver token-refresh primitive/path;
- repository-local Phase 2I recovery drills;
- substantial Profile Bridge runtime;
- exact-head CI/guarded acceptance discipline.

AR work extends or simplifies these foundations; it does not create parallel replacements.

## 6. Canonical machine architecture authority — extend, do not duplicate

`architecture/inventory.json` already aggregates workspace, routes, migrations, generated contracts and documentation authority. It remains the machine-readable architecture foundation. New dimensions may be normalized into source files internally, but they must project into one canonical architecture hierarchy rather than create competing registries.

### 6.1 Capability ownership

For every active capability record:

- capability identity and bounded-context owner;
- domain/use-case owner;
- inbound route/contract;
- outbound ports;
- persistence owner;
- provider/runtime adapters;
- produced/consumed events;
- Queue/DO/service binding ownership;
- security boundary;
- concurrency/idempotency model;
- allowed composition root;
- positive/negative tests;
- evidence level.

### 6.2 Production Capability / Release Profile

Before Production Core v1, machine authority must distinguish at least:

```text
source_present
accepted
production_enabled
environment
release_profile
dependencies
compatibility
backend_enforcement
frontend_projection
activation_gate
```

Production enablement must be fail-closed server-side. UI visibility is a projection from the same authority, never an independent safety control.

### 6.3 Generated-contract ownership

Every generated frontend contract module has one row: capability, Rust source/exporter, OpenAPI fragment/root, TypeScript target, generator, contract authority and drift/negative checker.

Existing inventory must reach 100% coverage, including separately generated Client Mail send and mailbox-client-association modules.

### 6.4 Document status

Implementation-influencing documents are classified as:

- `CURRENT_AUTHORITY`;
- `TARGET`;
- `ACCEPTED_HISTORICAL`;
- `EVIDENCE`;
- `RUNBOOK`;
- `GENERATED_PROJECTION`;
- `SUPERSEDED` where needed for forward clarity without rewriting history.

CI fails if historical/target material is presented as current execution authority.

### 6.5 Executable-tool role and exact Python estate

Executable operational/research surfaces (`scripts/`, `tools/`, runtime utilities and every repository-owned `.py`) receive machine records with at least:

- path;
- exact LOC;
- owner/capability;
- purpose;
- direct callers;
- indirect callers where mechanically derivable;
- workflow callers;
- local/remote reads;
- local/remote mutations;
- secret/customer-data access;
- current mutable authority if any;
- expected lifetime;
- replacement/cutover target;
- exact disposition.

Allowed Python dispositions:

- `KEEP_PYTHON`;
- `MOVE_OPSCTL_READ`;
- `MOVE_OPSCTL_MUTATION_LATER`;
- `MERGE`;
- `RETIRE`;
- `DELETE`.

A new executable Python file without an explicit disposition must fail CI after AR-6 cutover.

## 7. Frontend and public contracts

Preserve the current feature model:

- app/router consumes features through public APIs;
- sibling feature internals remain forbidden;
- `shared/api` remains transport primitives, not a god facade;
- generated DTOs remain canonical server projections;
- React never owns authorization/business policy;
- production capability UI exposure derives from the machine capability profile and cannot override backend fail-closed policy.

`UI_ARCHITECTURE.md` may describe future topology, while capability matrix/inventory owns what is accepted/composed/production-enabled.

No broad frontend rewrite is planned.

## 8. Concrete application hotspots proven by AR-0

### 8.1 Composition-root singularity

`apps/control-plane-worker/src/composition.rs` is a real composition root, but concrete construction also occurs in `lib.rs::binding_probe`. AR-3 defines allowed diagnostic roots; AR-4 consolidates construction so root/transport code cannot become a second general wiring authority.

### 8.2 Client Mail route ownership

Client Mail read routes (`/clients/{id}/mail/search`, `/mail/message`) are historically classified in `routes/clients.rs`; send is in `routes/client_mail.rs`. Public URLs stay unchanged, but AR-3 chooses one capability owner and AR-4 normalizes the internal classifier.

### 8.3 Outbound Mail composition extraction

The outbound use case already correctly owns reserve/claim/replay/ambiguous-dispatch semantics. Remaining Worker leakage includes duplicate eligibility, source-message access orchestration, direct D1/query/intent repository construction, binding lookup and provider selection.

Target bounded design:

1. preserve DTO -> `OutboundMailIntent` mapping in transport;
2. shape an application-level source-access need so reply/reply-all/forward source authorization belongs to application orchestration;
3. add an outer provider-router adapter resolving mailbox provider and delegating Gmail/SMTP;
4. expose one composition-root application bundle/factory to handler;
5. remove duplicate transport eligibility;
6. extend the existing Worker application-boundary checker/self-test to reject direct D1/provider coupling in Client Mail transport.

No public contract, provider semantics, retry taxonomy or D1 intent schema rewrite is implied.

### 8.4 Shared profile/generation use-case cluster

The remaining shared Profile/Generation/coordinator/grants/assignment application cluster is inward-only enough that size alone does not justify another crate. AR-3 measures semantic/dependency cohesion; extraction happens only with measurable ownership/compile/cognitive benefit.

### 8.5 OAuth refresh concurrency

Current resolver behavior is stronger than an initial rewrite assumption:

- onboarding domain already owns versioned `Pending/Active/ReauthRequired/...` transitions;
- Gmail/Microsoft ceremonies are explicit and replay/expiry bounded;
- resolver automatically refreshes expiring Google/Microsoft credentials and has an explicit Graph refresh route;
- refresh-token replacement is persisted.

The gap is concurrency: refresh follows load -> provider refresh -> ordinary encrypted-record upsert without a proven per-handle revision/lease/fence.

AR-8 must choose and prove one per-handle design (CAS revision, short lease/fence, single-flight, or equivalent) such that:

- only one refresh result becomes authoritative for a credential generation;
- a stale concurrent response cannot overwrite a newer rotated refresh token;
- transient provider failure does not destroy the previously usable credential;
- explicit refresh and implicit resolve-time refresh share the same concurrency authority;
- provider `invalid_grant`/revocation maps durably into the existing onboarding `ReauthRequired` lifecycle;
- negative concurrency/replay tests are permanent.

## 9. Runtime topology policy

Every Worker, D1, R2, Queue, DLQ, DO, service binding and provider lane receives one decision: `KEEP`, `SIMPLIFY`, `DEFER`, `DELETE`.

For each Queue record producer, consumer, schema, retry/idempotency, DLQ, backpressure, ordering and failure boundary.

Initial direction:

- control-plane Worker + Static Assets: KEEP;
- mailbox-secret-resolver Worker/service boundary: KEEP unless threat-model evidence proves a safer simplification;
- catalog D1: KEEP;
- resolver D1: KEEP;
- immutable generation R2: KEEP;
- ProfileCoordinator DO: KEEP;
- NotificationHub DO: KEEP;
- integration-events Queue: KEEP;
- `MAILBOX_JOBS` Queue/DLQ: KEEP, with final ownership/recovery proof;
- `GENERATION_VERIFICATION` Queue: **DELETE_CANDIDATE**, subject to proof of no external/independent consumer;
- Gmail API/read/send: KEEP;
- IMAP read + standards SMTP send: KEEP;
- Microsoft Graph OAuth/read/delta: KEEP;
- Microsoft Graph `Mail.Send`: DEFER unless separately implemented/accepted;
- browser/Bridge mailbox lane: KEEP at current evidence level.

### MAILBOX_JOBS recovery target

Preserve at-least-once Queue semantics and existing application/domain state. Do not invent a new mailbox-domain DLQ state machine.

Target operator flow:

```text
inspect DLQ envelope
  -> resolve tenant/binding/job/version
  -> load current D1 authority
  -> validate ownership/version/fence
  -> controlled requeue | rerun | retire
  -> metadata-only audit evidence
```

Read-only inspect/doctor arrives before any mutable DLQ action.

## 10. Historical/research executable policy

Initial classifications remain evidence, not a complete Python census:

- `tools/runtime_bundle.py`: `SYNTHETIC_EVIDENCE` — preserve;
- `tools/fingerprint_certify.py`: `EXTERNAL_RESEARCH_ONLY`/bounded external evidence — preserve with constraints;
- `tools/r2_s3_canary.py`: `APP_TEST_DATA_MUTATOR` — ephemeral canary only, never infra authority;
- `tools/profile_browser.py`: `EXTERNAL_RESEARCH_ONLY` candidate — guard/harden or retire;
- `runtime/camouhost/main.py`: `SYNTHETIC_EVIDENCE` — preserve;
- `tools/cloud_profile_smoke.py`: `HISTORICAL_QUARANTINE` candidate because it retains obsolete mutable-R2 active-pointer behavior.

AR-6 must complete the entire Python estate before any language-migration conclusion is accepted. AR-10 executes proven retire/delete decisions and regression-protects them.

## 11. Operational authority model

Operational architecture has five roles:

1. model/policy authority;
2. validator/generator;
3. mutation executor;
4. approval/orchestration authority;
5. evidence verifier.

### GitHub Actions / Environments own

- scheduling/orchestration;
- CI;
- approvals;
- protected environment boundaries;
- workflow concurrency;
- credential exposure boundaries;
- exact-head evidence;
- artifact/evidence retention.

### Rust `opsctl` owns, after bounded implementation

First read-only:

```text
opsctl inventory
opsctl plan
opsctl doctor
opsctl drift
```

Later, only after parity/rehearsal/cutover for one lifecycle at a time:

- selected migration/recovery/rotation/provision semantics;
- DLQ controlled actions;
- remote status/verification.

`opsctl` does not replace CI and does not become generic IaC.

### Wrangler / provider APIs own

Actual Cloudflare/provider execution under protected orchestration. Wrangler remains Worker deployment/configuration authority.

### Python remains valid for

- deterministic static checks;
- architecture validators;
- generators;
- fixtures;
- CI helper logic;
- bounded local developer tooling;
- evidence/research roles where explicitly classified.

A mutator moves only after parity, rehearsal, workflow cutover and negative proof the old mutable authority cannot silently return.

## 12. GitHub governance target

Current production Environment protection is a good foundation but does not mechanically protect `main`.

Before AR closeout, repository governance must prove the intended policy mechanically, using branch protection and/or repository rulesets supported by the repository plan:

- no direct push to `main` under normal development flow;
- no force-push/delete on `main`;
- PR required;
- explicit required CI/check set;
- stale approvals handled deliberately;
- required review policy;
- predictable merge method;
- production Environment required reviewer(s);
- `can_admins_bypass=false` for production;
- production deployment branch policy restricted to `main`;
- explicit, documented admin/bypass exception policy if any.

Green Actions are not equivalent to required checks until GitHub mechanically enforces them.

## 13. Wrangler, environment identity and deployment authority

### Wrangler owns

Worker deployment configuration, compatibility, Worker names/environment blocks, routes/static assets, binding names, Queue/DO/service declarations, required secret names and Worker build/runtime settings.

### Typed environment identity owns

Canonical lifecycle names:

- `rehearsal` — disposable/provisioning/recovery proof;
- `staging` — persistent preproduction compatibility surface;
- `production` — protected customer/runtime environment.

Any provider-specific `prod` alias must be a derived mapping, not a second canonical environment name.

Typed environment identity owns account/resource identity, environment membership/ownership metadata and non-secret topology facts for inventory/plan/drift. It does not duplicate Wrangler binding semantics.

### Derived deploy/release manifests

Remain generated projections binding immutable releases to environment identity; never a third manually maintained authority.

Future `opsctl doctor/drift` verifies:

`environment model <-> Wrangler <-> actual Cloudflare inventory`.

## 14. Credential, secret and key architecture

### 14.1 Preserve accepted domain/crypto behavior

Do **not** create a second OAuth lifecycle. Preserve:

- mailbox onboarding version/CAS and `ReauthRequired` state;
- provider-specific OAuth ceremonies;
- resolver encrypted credential storage;
- current token refresh implementation;
- versioned contact/resolver keyrings;
- resolver key re-encryption/reconciliation and rotation ledger.

### 14.2 Two-level inventory governed by data classification

**Repository policy registry** tracks only fields explicitly safe under `DATA_CLASSIFICATION`, such as class, owner, environment applicability, safe slot names, consumers, rotation/recovery policy and evidence policy.

**Protected live operational inventory** holds sensitive active identity/version/handle/provider state and other live security configuration.

`opsctl doctor` may query protected live state and compare it to policy, but repository/evidence receives only explicitly allowed redacted/digest/status projections.

### 14.3 Logical credential lifecycle

The managed unit is a logical credential, not a collection of manually synchronized storage slots:

```text
issue/import
  -> validate
  -> bind
  -> switch
  -> verify
  -> revoke previous
```

Human action remains only where the external issuer physically requires it. Everything after obtaining credential material should be automated where provider APIs allow.

### 14.4 Key lifecycle target

- WRITE: active only;
- READ: active + explicitly supported previous versions;
- retirement only after re-encryption/compatibility proof;
- rotation never recreates infrastructure;
- recovery requires protected material availability and policy-safe evidence.

## 15. D1 evolution architecture

Catalog and resolver D1 are independent schema owners.

Every migration is classified `EXPAND_SAFE`, `BACKFILL`, `CONTRACT`, or `MANUAL_EXCEPTION`.

Every release component declares min/max supported revisions for every database it uses and pre/post deployment requirements.

Rules:

- preserve forward-only/replay-safe migration mechanics and historical provenance;
- fresh bootstrap != upgrade migration;
- a fresh current baseline/epoch may be introduced only after ownership stabilizes and must prove semantic convergence with the historical upgrade path;
- do not call a consolidated baseline `V2` unless a real compatibility break is intentionally accepted;
- destructive changes require multi-release sequencing;
- code rollback is blocked outside its schema window;
- remote evidence contains metadata/digests/revisions, not customer rows;
- migration execution has one legitimate authority and is fail-closed under workflow/operational concurrency;
- add a database-level distributed migration lock only if AR proves an independent concurrency surface that GitHub/ops orchestration cannot eliminate.

Do not add coordination state merely for architectural aesthetics.

## 16. Immutable release-set architecture

A release set binds one accepted source SHA to every required deployable component:

- source SHA;
- release-set ID;
- component names and artifact digests/release IDs;
- internal protocol compatibility;
- per-database schema windows;
- deploy order;
- required topology revision;
- workflow/evidence identities;
- staging/rehearsal evidence;
- production deployment/version identities only after PC-1;
- health/attestation timestamp.

Current resolver + control-plane/SPA no-rebuild behavior is preserved. Staging and production receive identical component bytes; bindings/version IDs may differ. Independent resolver/control-plane versions require explicit service protocol compatibility.

Windows Bridge is a separate release domain but can be associated with overall product release compatibility.

## 17. Recovery model

Repository-local Phase 2I recovery is accepted input.

AR-14 advances it to remote/disposable evidence:

- disposable Cloudflare catalog/resolver D1 restore + invariants;
- immutable R2 availability/exact verification;
- protected key/keyring recovery without material in Git/evidence;
- credential/OAuth re-establishment;
- Queue/DLQ reconciliation where applicable;
- DO/coordinator recovery assumptions;
- Profile Bridge continuity;
- measured RTO/RPO decision;
- failed-restore negatives;
- full application health/invariant suite after restore.

## 18. Windows release/update architecture

AR-15 owns the missing updater/publisher while preserving existing Bridge runtime:

`publish signed manifest -> discover -> download staging -> verify digest/signature -> side-by-side install -> safe activation -> health -> accept or bounded LKG rollback`.

Controls: exact artifact/version, trusted signing/key rotation, anti-downgrade where needed, process-safe activation, runtime/profile-generation compatibility, failed download/verification/activation recovery, LKG retention and metadata-only evidence. Physical host/trusted production certificate proof remains external.

## 19. Documentation authority model

After AR-1:

1. root `README.md` — product + one current docs pointer;
2. `docs/INDEX.md` — authority hierarchy;
3. `docs/DEVELOPMENT_PLAN.md` — product phase state + active execution pointer;
4. this plan — current Architecture Re-baseline execution authority;
5. capability/release-profile + architecture inventory — accepted/composed/production-enabled state;
6. stable architecture/threat/data/contract authorities;
7. active bounded issue/PR.

Historical plans remain searchable but unmistakably historical/superseded. `IMPLEMENTATION_PLAN.md`, `PROFILE_LIFECYCLE_PLAN.md`, #203/#251 predecessor material and accepted slice docs are classified without rewriting their historical content.

## 20. Architecture fitness tests

Extend existing gates rather than creating a parallel checker framework. By closeout CI fails on at least:

- inner -> outer/provider leakage;
- collapsed capability ownership;
- concrete wiring outside allowed composition/diagnostic roots;
- direct D1/provider coupling in migrated Worker transports, including Client Mail send;
- sibling frontend feature internals;
- manual server DTO/generated-contract drift;
- generated TS module without inventory owner/generator;
- target/historical document presented as current authority;
- executable tool used outside role/environment;
- new executable `.py` without disposition after AR-6;
- undeclared/orphan runtime resource;
- cross-environment persistent identity reuse;
- secret/live-sensitive metadata committed where classification forbids it;
- duplicated environment/deploy authority;
- incompatible release/schema/protocol window;
- migration replay/inventory drift;
- release artifact substitution;
- old operational mutator returning after cutover;
- mutable active-R2 path returning after historical smoke retirement;
- uncoordinated credential refresh/store path returning after AR-8;
- capability source presence being mistaken for production enablement;
- production-disabled capability reachable server-side;
- production mutation attempted from AR workflow/authority before PC-1;
- missing recovery/rotation evidence;
- repository governance weaker than declared closeout policy.

## 21. Concurrency inventory

AR-3/AR-8/AR-9/AR-11 record owner + lock/idempotency/fencing for:

- OAuth refresh/reauthorization;
- outbound mail dispatch/ambiguous outcome;
- mailbox Queue consumption;
- integration-event delivery;
- migration execution;
- profile-generation publish/activate;
- notification fanout/cursors;
- key/credential rotation;
- release promotion;
- Windows update activation.

Implicit last-writer-wins is never accepted.

## 22. Production capability rollout model

### Production Core v1

Production-enabled only after AR-17 closeout authorizes the gate:

- authentication/authorization/membership foundation;
- users;
- client/customer cards;
- browser profiles;
- Camoufox/profile runtime;
- single browser-profile operations;
- bulk browser-profile operations;
- client <-> browser-profile binding;
- required audit/health/readiness/observability/release/recovery foundations.

### Source-present but production-disabled at Core v1

- mailbox administration;
- bulk mailbox operations;
- client <-> mailbox binding;
- mailbox jobs/automation;
- outbound mail/email side effects unless separately accepted.

No `production-lite` branch, mailbox fork or second schema lineage. One `main`, one architecture and one compatibility history.

# 23. Execution sequence

## AR-0 — Delta Architecture Inventory

Read-only/governance research. Compare exact accepted `main`, classify `PRESENT/PARTIAL/MISSING/CONFLICT/SUPERSEDED`, record topology/tool candidates and repeated-review evidence. No runtime/OpenAPI/migration/workflow/Cloudflare/provider/secret/deployment mutation.

Exit: candidate plan, evidence and machine transition agree on sequencing and current truth; all permanent CI passes on one unchanged head before Ready.

## AR-1 — One-shot Architecture Authority Re-baseline

Activate one future authority in a complete mechanically checked transaction, including documentation/status/checker/generator/release-freeze consumers and issue relationships.

Exit: exactly one current future program passes permanent CI; historical/target docs cannot look current.

## AR-2 — Runtime Topology + D3 Compatibility Gate

Build complete Worker/D1/R2/Queue/DLQ/DO/service/provider ownership table and resolve `KEEP/SIMPLIFY/DEFER/DELETE`.

Mandatory: final `GENERATION_VERIFICATION` proof; preserve resolver isolation unless threat-model evidence says otherwise; classify D3 predecessor state without forcing obsolete production provisioning.

## AR-3 — Application Architecture Contract + Inventory Completion

Produce canonical capability/dependency/composition/concurrency contract and extend existing inventory/fitness tests.

Mandatory outputs:

- allowed composition/diagnostic roots;
- 100% generated-contract inventory;
- document-status model;
- capability/release-profile schema target;
- executable-tool role model;
- Client Mail route ownership;
- Outbound Mail source-access/provider-routing boundary;
- OAuth refresh concurrency ownership specification;
- Profile/Generation cohesion decision;
- runtime-resource ownership projection.

No speculative crate split.

## AR-4 — Bounded Application/Composition Cleanup

Initial candidate order:

- **AR-4A** composition-root consolidation;
- **AR-4B** Client Mail internal route ownership normalization;
- **AR-4C** Outbound Mail composition extraction + boundary checker extension;
- **AR-4D** Profile application extraction only if AR-3 proves benefit.

No mega-PR.

## AR-5 — Wrangler / Runtime Authority Cleanup

Apply AR-2 topology decisions to canonical Wrangler/binding checks. Remove dead bindings/resources only with ownership proof. Preserve required-secret names, environment isolation and the rule that AR does not provision production.

## AR-6 — Full Python Estate + Read-only Rust `opsctl`

Inventory **every repository-owned `.py` executable/script** with exact LOC/callers/reads/mutations/authority/disposition. Add mechanical completeness/negative checks.

Then add typed read-only:

```text
opsctl inventory
opsctl plan
opsctl doctor
opsctl drift
```

No remote mutation from `opsctl` in AR-6.

## AR-7 — Typed Environments, GitHub Governance, Secrets and Operational Cutover Foundations

Define `rehearsal/staging/production`; reconcile any `prod` aliases; mechanically enforce repository/production governance; define logical credential policy and environment identity without duplicating Wrangler. Prepare concern-by-concern mutator cutover model; do not create a second mutator.

## AR-8 — Credential / Secret / Key Lifecycle + OAuth Refresh Concurrency

Preserve existing onboarding state machine/OAuth ceremonies/encrypted resolver store/keyrings. Implement repository-safe policy vs protected-live inventory, refresh single-flight/CAS/fence, durable provider-revocation -> `ReauthRequired`, and uniform rotation/recovery policy with negative tests.

## AR-9 — D1 Evolution / Schema Compatibility

Add migration classes, catalog/resolver revisions, per-component compatibility windows, rollback blocker and one concurrency authority. Preserve historical migrations. Add DB lock only if an unavoidable independent concurrent executor is proven.

## AR-10 — Runtime and Historical Executable Simplification

Execute accepted deletions/simplifications. Resolve `GENERATION_VERIFICATION`, historical `cloud_profile_smoke.py`, `profile_browser.py` classification and any orphan binding/adapter/tool discovered by inventory. Retire old mutators only after parity/cutover proof.

## AR-11 — Release-set / Promotion Architecture

Generalize D2/D3 provenance into explicit multi-component immutable release set with no-rebuild promotion, protocol compatibility, schema windows and same-bits staging/production rule. This defines production promotion semantics but does **not** perform production promotion.

## AR-12 — Fresh Rehearsal Environment

Prove:

```text
inventory -> plan -> create -> verify -> application bootstrap -> plan again == NO CHANGE
```

using disposable/rehearsal resources and no production customer data/secrets.

## AR-13 — Rotation Rehearsal

Execute key/credential rotation against rehearsal resources: active-write/previous-read, no infra recreation, safe retirement and fail-closed rollback/reauth.

## AR-14 — Remote Recovery Rehearsal

Extend repository-local DR to disposable real Cloudflare resources/key ceremonies. Measure RTO/RPO and prove catalog/resolver D1, R2, key/credential, Queue and application invariant recovery.

## AR-15 — Windows Release & Update Architecture

Implement updater/publisher only; preserve Bridge runtime. Signed manifest, staged side-by-side install, safe activation, health/LKG rollback and negative recovery paths.

## AR-16 — Final Whole-project 10/10 Audit

Re-audit the **latest accepted main**, not the original AR-0 baseline, across:

- backend/domain/application/ports/adapters/composition;
- frontend/features/contracts;
- capability/release-profile enforcement;
- docs/developer comprehensibility;
- full executable/Python estate;
- Cloudflare topology;
- D1 evolution and migration concurrency;
- secrets/keys/OAuth refresh concurrency;
- release/protocol/schema compatibility;
- recovery/rotation;
- Windows updater;
- observability/security;
- GitHub/CI governance.

Required repository-owned result:

```text
P0 = 0
P1 = 0
```

**AR-16 performs no production provisioning or promotion.** Any unresolved P0/P1 reopens bounded remediation before closeout.

## AR-17 — Architecture Closeout + Production Core v1 Gate Authorization

Freeze and mechanically identify:

- canonical architecture/documentation authorities;
- capability/release profiles;
- ownership/dependency/composition rules;
- environment model;
- Python/opsctl/Wrangler/GitHub operational boundaries;
- secret/key/credential lifecycle;
- D1 compatibility policy;
- release-set/promotion policy;
- recovery/rotation runbooks/evidence;
- Windows update authority;
- governance requirements.

If and only if AR-16 is `P0=0/P1=0` and closeout fitness tests are green:

```text
architecture_complete = true
production_core_gate = AUTHORIZED
production_ready = false
```

AR-17 **does not deploy or provision production**.

### AR program ends here.

# 24. Post-architecture production sequence

## PC-1 — Production Core v1 Provisioning and Promotion

Only after `production_core_gate=AUTHORIZED`:

1. protected production authorization;
2. verify/create only approved persistent production resources using accepted operational authority;
3. apply allowed migrations within compatibility windows;
4. promote the exact immutable release set previously validated in staging/rehearsal;
5. run smoke/readiness/invariant checks;
6. verify recovery/rollback boundaries and production evidence;
7. record immutable deployment/evidence identities.

Only successful PC-1 may transition:

```text
production_ready = true
```

for the Production Core v1 scope.

## PC-2 — Mailbox Administration

Separately gate production activation of mailbox administration, bulk mailbox operations and client <-> mailbox binding. Existing source code may remain in `main` before activation.

## PC-3 — Mailbox Jobs / Automation

Separately gate `MAILBOX_JOBS`, schedulers/automation and mutable DLQ actions after operational authority/recovery evidence is accepted.

## PC-4 — Outbound / Subsequent Capabilities

Separately gate outbound email side effects and later capabilities with their own compatibility/security/operational evidence.

# 25. Historical roadmap mapping

- R1–R9: preserve accepted history/regression protections;
- Pre-2J A/B/C: preserve;
- D1 Wrangler: preserve/refine AR-5;
- D2 immutable release: preserve/generalize AR-11;
- D3 repository-side promotion machinery: preserve as predecessor evidence/generalize AR-11;
- D3 external #251: currently open; classify during AR-1/AR-2 and do not force obsolete production mutation merely to close historical sequencing;
- D4: absorbed AR-9;
- D5: evidence preserved; remaining concerns AR-3/AR-10/AR-16;
- D6: AR-1/AR-6/AR-7/AR-12–14/AR-16/AR-17;
- Batch E Windows updater: AR-15;
- Batch F: superseded forward by AR-16/AR-17;
- historical Phase 2J production-readiness state remains blocked through AR-17 and then depends on PC-1/external production evidence.

# 26. Forbidden shortcuts

- greenfield rewrite;
- Terraform/generic IaC replacement;
- `opsctl` as second mutator;
- new competing capability/architecture registry;
- one-crate-per-interface fragmentation;
- one-checker-per-finding when an existing fitness framework can extend;
- Queue/Worker/tool deletion without ownership proof;
- target docs treated as accepted;
- historical executables left operational-looking;
- treating sensitive credential/key identity metadata as Git-safe by default;
- new OAuth lifecycle parallel to accepted onboarding domain;
- refresh by uncoordinated load-provider-upsert after AR-8;
- deploy creating durable resources implicitly;
- key rotation by resource recreation;
- app rollback assuming D1 rollback;
- database lock added without a proven concurrent executor problem;
- rebuild between staging/production;
- repository-local recovery relabelled as remote evidence;
- Bridge runtime relabelled as updater;
- rewriting historical ledgers;
- weakening exact-head CI;
- production provisioning/promotion inside AR-0..AR-17;
- UI-only capability disablement;
- source presence interpreted as production activation;
- long-lived reduced production branch/fork.

# 27. Architecture Definition of Done

AR-17 may close only when:

- one current execution/documentation authority is mechanically enforced;
- every implementation-influencing document has a status class;
- every generated TS contract has source/generator/authority inventory row;
- every repository-owned Python executable has exact reviewed disposition;
- new unclassified executable Python fails CI;
- capability/release-profile authority distinguishes source-present/accepted/production-enabled;
- production-disabled capability fails closed server-side;
- historical planning ambiguity is eliminated;
- all capabilities have domain/application/adapter/composition ownership;
- dependency/wiring leakage fails CI;
- Client Mail transport is thin and provider/D1 wiring composition-owned;
- runtime has no ownerless binding/resource;
- mutable-active-R2 historical operational paths are absent;
- Wrangler/environment/derived-manifest responsibilities do not overlap;
- GitHub/opsctl/Wrangler/Python mutation boundaries are singular and testable;
- no Terraform/generic hidden state;
- sensitive live credential/key metadata stays protected according to classification;
- OAuth refresh is single-authority/race-safe and provider revocation reconciles to `ReauthRequired`;
- catalog/resolver schema windows are enforced;
- migration concurrency has exactly one justified authority;
- release-set components are immutable and protocol/schema compatible;
- staging/production promotion policy guarantees identical component bytes;
- credentials/keyrings have executable rotation/retirement/recovery;
- fresh rehearsal converges to no-change;
- remote recovery passes with measured RTO/RPO decision;
- Windows updater passes signed/staged/health/LKG rollback;
- repository governance mechanically matches declared merge/production policy;
- public contracts remain compatible or have accepted versioning proof;
- AR-16 has `P0=0`, `P1=0` on latest accepted main;
- a new engineer can operate/understand the repository without tribal knowledge;
- `architecture_complete=true`, `production_core_gate=AUTHORIZED`, `production_ready=false` is truthful at closeout.

# 28. Production Core v1 Definition of Done

PC-1 is separate from Architecture DoD and completes only when:

- Production Core v1 release profile is the exact authorized profile;
- production resources match the accepted typed environment/topology plan;
- production mutations occurred only through protected accepted authority;
- the exact immutable release set validated pre-production is promoted without rebuild;
- schema/protocol/runtime compatibility checks pass;
- Core-enabled backend routes/features are reachable as intended;
- mailbox/outbound/automation capabilities remain fail-closed and production-disabled;
- production smoke/readiness/invariants pass;
- deployment/recovery evidence is captured without sensitive/customer data;
- rollback/recovery boundaries are verified;
- only then may Production Core v1 set `production_ready=true` for its scope.
