# Architecture Re-baseline v3 — Canonical Candidate Plan

**Status:** proposed forward execution authority; becomes normative only through a separately accepted AR-1 authority transaction  
**Audit base:** `5be54c2989dbfa22822d3692e22156f23d2a4602`  
**Tracking:** #266  
**Production readiness:** unchanged; `production_ready=false`

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
- which executable tool is validator, infra mutator, test-data mutator, synthetic evidence, external research or historical quarantine;
- which exact source/artifact/schema/topology is released;
- which credential metadata is repository-safe policy and which live identity/state must remain protected;
- how credentials, schemas, resources and application versions rotate, recover and roll back independently.

The target is deliberately boring: few concepts, explicit ownership, one mutable authority, strong fitness tests and no architecture framework added merely for appearance.

## 2. Non-negotiable invariants

1. **One mutable concern = one authority.** Two independently usable mutators for the same lifecycle are forbidden.
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
19. `production_ready=false` remains mandatory until later external production evidence explicitly changes it.

## 3. Target dependency model

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
  Wrangler + GitHub Actions + project-specific operational tooling
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

## 4. Preserve what is already strong

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

## 5. Canonical architecture inventory — extend, do not duplicate

`architecture/inventory.json` already aggregates workspace, routes, migrations, generated contracts and documentation authority. It remains the derived machine-readable architecture map rather than adding competing registries.

Its generator must evolve to complete these dimensions:

### 5.1 Capability ownership

For every active capability record domain owner, use-case owner, inbound route/contract, outbound ports, persistence owner, provider/runtime adapters, produced/consumed events, Queue/DO/service binding ownership, security boundary, concurrency/idempotency model, allowed composition root, positive/negative tests and evidence level.

### 5.2 Generated-contract ownership

Every generated frontend contract module has one row: capability, Rust source/exporter, OpenAPI fragment/root, TypeScript target, generator, contract authority and drift/negative checker.

Existing inventory must reach 100% coverage, including separately generated Client Mail send and mailbox-client-association modules.

### 5.3 Document status

Implementation-influencing documents are classified as:

- `CURRENT_AUTHORITY`;
- `TARGET`;
- `ACCEPTED_HISTORICAL`;
- `EVIDENCE`;
- `RUNBOOK`;
- `GENERATED_PROJECTION`.

CI fails if historical/target material is presented as current execution authority.

### 5.4 Executable-tool role

Executable operational/research surfaces (`scripts/`, `tools/`, relevant runtime utilities) are classified as:

- `VALIDATOR_GENERATOR`;
- `INFRA_MUTATOR`;
- `APP_TEST_DATA_MUTATOR`;
- `SYNTHETIC_EVIDENCE`;
- `EXTERNAL_RESEARCH_ONLY`;
- `HISTORICAL_QUARANTINE`.

Inventory records allowed environment, input authority and whether persistent/customer-state mutation is permitted.

## 6. Frontend and public contracts

Preserve the current feature model:

- app/router consumes features through public APIs;
- sibling feature internals remain forbidden;
- `shared/api` remains transport primitives, not a god facade;
- generated DTOs remain canonical server projections;
- React never owns authorization/business policy.

AR-1/AR-3 make **TARGET versus ACCEPTED** machine-explicit. `UI_ARCHITECTURE.md` may describe future topology, while capability matrix/inventory owns what is accepted/composed.

No broad frontend rewrite is planned.

## 7. Concrete application hotspots proven by AR-0

### 7.1 Composition-root singularity

`apps/control-plane-worker/src/composition.rs` is a real composition root, but concrete construction also occurs in `lib.rs::binding_probe`. AR-3 defines allowed diagnostic roots; AR-4 consolidates construction so root/transport code cannot become a second general wiring authority.

### 7.2 Client Mail route ownership

Client Mail read routes (`/clients/{id}/mail/search`, `/mail/message`) are historically classified in `routes/clients.rs`; send is in `routes/client_mail.rs`. Public URLs stay unchanged, but AR-3 chooses one capability owner and AR-4 normalizes the internal classifier.

### 7.3 Outbound Mail composition extraction

The outbound use case already correctly owns reserve/claim/replay/ambiguous-dispatch semantics. Remaining Worker leakage is:

- duplicate eligibility pre-check before application access check;
- source-message access orchestration in Worker support code;
- direct construction of query/eligibility/intent repositories;
- provider binding lookup and Gmail/SMTP selection in handler;
- concrete provider enum in Worker code.

Target bounded design:

1. preserve DTO -> `OutboundMailIntent` mapping in transport;
2. shape an application-level source-access need so reply/reply-all/forward source authorization belongs to application orchestration;
3. add an outer provider-router adapter resolving mailbox provider and delegating Gmail/SMTP;
4. expose one composition-root application bundle/factory to handler;
5. remove duplicate transport eligibility;
6. extend the existing Worker application-boundary checker/self-test to reject direct D1/provider coupling in Client Mail transport.

No public contract, provider semantics, retry taxonomy or D1 intent schema rewrite is implied.

### 7.4 Shared profile/generation use-case cluster

The remaining shared `use-cases` crate is inward-only and concentrated around Profile/Generation/coordinator/grants/assignment. Size alone does not justify another crate. AR-3 measures semantic/dependency cohesion; extraction happens only with measurable ownership/compile/cognitive benefit.

### 7.5 OAuth refresh concurrency

Current resolver behavior is stronger than the first draft assumed:

- onboarding domain already owns versioned `Pending/Active/ReauthRequired/...` state transitions;
- Gmail/Microsoft ceremonies are explicit and replay/expiry bounded;
- resolver automatically refreshes expiring Google/Microsoft credentials and has an explicit Graph refresh route;
- refresh-token replacement is persisted.

The remaining gap is concurrency: resolver credential records do not expose a credential revision/lease for the general refresh store path, and refresh currently follows load -> provider refresh -> ordinary upsert. Two concurrent resolves/refreshes for one handle therefore lack a proven single-flight/CAS rule.

AR-8 must choose and prove one design (for example compare-and-swap revision, short refresh lease/fence, or equivalent single-flight owner) such that:

- only one refresh result becomes authoritative for a credential generation;
- a stale concurrent response cannot overwrite a newer rotated refresh token;
- transient provider failure does not destroy the previously usable credential;
- explicit refresh and implicit resolve-time refresh share the same concurrency authority;
- provider `invalid_grant`/revocation is translated into the existing onboarding `ReauthRequired` lifecycle rather than inventing a second state machine;
- negative concurrency/replay tests are permanent.

## 8. Runtime topology policy

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
- mailbox-jobs Queue/DLQ: KEEP pending final AR-2 ownership table;
- `GENERATION_VERIFICATION` Queue: **DELETE candidate**, subject to proof of no external/independent consumer;
- Gmail API/read/send: KEEP;
- IMAP read + standards SMTP send: KEEP;
- Microsoft Graph OAuth/read/delta: KEEP;
- Microsoft Graph `Mail.Send`: DEFER unless separately implemented/accepted;
- browser/Bridge mailbox lane: KEEP at current evidence level.

## 9. Historical/research executable policy

Operational simplification includes executable artifacts.

Initial classifications:

- `tools/runtime_bundle.py`: `SYNTHETIC_EVIDENCE` — preserve;
- `tools/fingerprint_certify.py`: `EXTERNAL_RESEARCH_ONLY`/bounded external evidence — preserve with explicit constraints;
- `tools/r2_s3_canary.py`: `APP_TEST_DATA_MUTATOR` — ephemeral canary only, never infra authority;
- `tools/profile_browser.py`: `EXTERNAL_RESEARCH_ONLY` candidate — direct persistent Camoufox launcher must be clearly separate from supported Profile Bridge and hardened/quarantined if necessary;
- `runtime/camouhost/main.py`: `SYNTHETIC_EVIDENCE` — preserve;
- `tools/cloud_profile_smoke.py`: **`HISTORICAL_QUARANTINE` candidate**.

The last tool implements early AES-GCM + mutable R2 `current.json`, while accepted architecture uses immutable generation objects and authoritative D1 active-generation registry. AR-10 removes or quarantines it and adds regression preventing mutable active-R2 operational paths from returning.

## 10. Operational authority model

Operational architecture has five roles:

1. model/policy authority;
2. validator/generator;
3. mutation executor;
4. approval/orchestration authority;
5. evidence verifier.

Current Python code is not deprecated by language. Current Cloudflare mutation is already a composition of validators/generators, GitHub Actions/Environment approval and pinned Wrangler.

Before mutating `opsctl`, an operations ledger classifies relevant `scripts/`/`tools/` concerns as:

- `KEEP_VALIDATOR`;
- `MIGRATE_MODEL`;
- `MIGRATE_MUTATOR`;
- `DELETE_AFTER_PARITY`.

A validator may remain indefinitely. A mutator moves only after parity, rehearsal, workflow cutover and negative proof the old mutator cannot silently return.

## 11. Wrangler, environment identity and deployment authority

### Wrangler owns

Worker deployment configuration, compatibility, Worker names/environment blocks, routes/static assets, binding names, Queue/DO/service declarations, required secret names and Worker build/runtime settings.

### Typed environment identity owns

Environment lifecycle (`staging`, `production`, `rehearsal`), account identity, persistent resource identities where externally addressed, environment membership/ownership metadata and non-secret topology facts for inventory/plan/drift. It does not duplicate Wrangler binding semantics.

### Derived deploy/release manifests

Remain generated projections binding immutable releases to environment identity; never a third manually maintained authority.

Future `opsctl doctor/drift` verifies:

`environment model <-> Wrangler <-> actual Cloudflare inventory`.

## 12. Credential, secret and key architecture

### 12.1 Preserve accepted domain/crypto behavior

Do **not** create a second OAuth lifecycle. Preserve:

- mailbox onboarding version/CAS and `ReauthRequired` state;
- provider-specific OAuth ceremonies;
- resolver encrypted credential storage;
- current token refresh implementation;
- versioned contact/resolver keyrings;
- resolver key re-encryption/reconciliation and rotation ledger.

### 12.2 Two-level inventory, governed by data classification

“Metadata-only” does not mean “Git-safe”. Credential handles, key identifiers and live security configuration can themselves be classified sensitive.

Use two layers:

**Repository policy registry (tracked only for fields permitted by `DATA_CLASSIFICATION`)**

- credential/key class, not live secret instance;
- owning capability;
- environment applicability;
- allowed secret-slot names where already repository-safe;
- consumer roles;
- refresh/rotation/recovery policy;
- expected algorithm/protocol/version policy where safe;
- evidence policy and classification.

**Protected live operational inventory (not committed by default)**

- actual active credential/key identity/version when classified sensitive;
- live secret handles;
- provider account/tenant identifiers if sensitive;
- rotation generation/current previous instance relationship;
- last rotation/refresh/revocation operational state;
- any field whose disclosure would improve credential targeting or reveal protected configuration.

`opsctl doctor` may query protected live inventory through authenticated provider/Cloudflare/GitHub APIs and compare only safe digests/status to repository policy. Evidence remains redacted/metadata-minimal.

### 12.3 Key lifecycle target

- WRITE: active only;
- READ: active + explicitly supported previous versions;
- retirement only after re-encryption/compatibility proof;
- rotation never recreates infrastructure;
- recovery requires protected key material availability and policy-safe evidence.

### 12.4 OAuth refresh target

Refresh uses the single per-handle concurrency authority from §7.5. Explicit refresh and implicit resolve refresh are one lifecycle. Provider rejection that means revoked/invalid refresh credential maps to the existing `ReauthRequired` application state through a deliberate durable reconciliation path.

## 13. D1 evolution architecture

Catalog and resolver D1 are independent schema owners.

Every migration is classified `EXPAND_SAFE`, `BACKFILL`, `CONTRACT`, or `MANUAL_EXCEPTION`.

Every release component declares min/max supported revisions for every database it uses and pre/post deployment requirements.

Rules:

- preserve forward-only/replay-safe migration mechanics;
- fresh bootstrap != upgrade migration;
- migration execution is concurrency-locked/fail-closed;
- destructive changes require multi-release sequencing;
- code rollback is blocked outside its schema window;
- remote evidence contains metadata/digests/revisions, not customer rows.

Release-freeze/check scripts hard-coded to #203/old authority belong to AR-1 closure so they cannot become a hidden AR-9 blocker.

## 14. Immutable release-set architecture

A release set binds one accepted source SHA to every required deployable component:

- source SHA;
- release-set ID;
- component names and artifact digests/release IDs;
- internal protocol compatibility;
- per-database schema windows;
- deploy order;
- required topology revision;
- workflow/evidence identities;
- staging evidence;
- production deployment/version identities;
- health/attestation timestamp.

Current resolver + control-plane/SPA no-rebuild behavior is preserved. Staging and production receive identical component bytes; bindings/version IDs may differ. Independent resolver/control-plane versions require explicit service protocol compatibility.

Windows Bridge is a separate release domain but can be associated with overall product release compatibility.

## 15. Recovery model

Repository-local Phase 2I recovery is accepted input.

AR-14 advances it to remote/rehearsal evidence:

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

## 16. Windows release/update architecture

AR-15 owns missing updater while preserving existing Bridge runtime:

`publish signed manifest -> discover -> download staging -> verify digest/signature -> side-by-side install -> safe activation -> health -> accept or bounded LKG rollback`.

Controls: exact artifact/version, trusted signing/key rotation, anti-downgrade where needed, process-safe activation, runtime/profile-generation compatibility, failed download/verification/activation recovery, LKG retention and metadata-only evidence. Physical host/trusted production certificate proof remains external.

## 17. Documentation authority model

After AR-1:

1. root `README.md` — product + one current docs pointer;
2. `docs/INDEX.md` — authority hierarchy;
3. `docs/DEVELOPMENT_PLAN.md` — product phase state + active execution pointer;
4. this plan — current remediation execution authority;
5. capability matrix + architecture inventory — actually accepted/composed state;
6. stable architecture/threat/data/contract authorities;
7. active bounded issue/PR.

Historical plans remain searchable but unmistakably historical/superseded. `IMPLEMENTATION_PLAN.md`, `PROFILE_LIFECYCLE_PLAN.md`, stale root retirement references and old accepted slice docs are classified without rewriting their historical content.

## 18. Architecture fitness tests

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
- missing recovery/rotation evidence.

## 19. Concurrency inventory

AR-3/AR-8/AR-9 record owner + lock/idempotency/fencing for:

- OAuth refresh/reauthorization (per credential handle/generation);
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

# 20. Execution sequence

## AR-0 — Delta Architecture Inventory

Read-only/governance research. Compare exact accepted `main`, classify `PRESENT/PARTIAL/MISSING/CONFLICT/SUPERSEDED`, record topology/tool candidates and repeated-review evidence. No runtime/OpenAPI/migration/workflow/Cloudflare/provider/secret/deployment mutation.

## AR-1 — One-shot Architecture Authority Re-baseline

Activate one future authority in a complete mechanically checked transaction, including where applicable:

- `README.md`, `docs/README.md`, `docs/INDEX.md`, `docs/DEVELOPMENT_PLAN.md`, `docs/status.json`;
- old Pre-2J plan status/banner without rewriting history;
- capability-matrix orientation;
- `IMPLEMENTATION_PLAN.md`/`PROFILE_LIFECYCLE_PLAN.md` classification;
- target/historical document status projection;
- stale legacy-root references;
- `scripts/check-documentation-authority.py` + negative/self-tests;
- `scripts/check-phase2i-release-freeze.sh` and other hard-coded #203 consumers;
- `scripts/generate-architecture-inventory.py` + `architecture/inventory.json` + negative fixtures;
- issue relationships #203/#251/#266.

Exit: exactly one current future program passes fast preflight/permanent CI; historical/target docs cannot look current.

## AR-2 — Runtime Topology + D3 Compatibility Gate

Build complete Worker/D1/R2/Queue/DLQ/DO/service/provider ownership table and resolve `KEEP/SIMPLIFY/DEFER/DELETE`.

Mandatory: final `GENERATION_VERIFICATION` proof; preserve resolver isolation unless threat-model evidence says otherwise; classify executable tools; decide whether current unaccepted D3 external target topology can proceed before irreversible production provisioning.

## AR-3 — Application Architecture Contract + Inventory Completion

Produce canonical capability/dependency/composition/concurrency contract and extend existing inventory/fitness tests.

Mandatory outputs:

- allowed composition/diagnostic roots;
- 100% generated-contract inventory;
- document-status model;
- executable-tool role model;
- Client Mail route ownership;
- Outbound Mail source-access/provider-routing boundary;
- OAuth refresh concurrency ownership specification;
- Profile/Generation shared-use-case cohesion decision;
- runtime-resource ownership projection.

No speculative crate split.

## AR-4 — Bounded Application/Composition Cleanup

Initial candidate order:

- **AR-4A Composition-root consolidation**;
- **AR-4B Client Mail internal route ownership normalization**;
- **AR-4C Outbound Mail composition extraction** + existing boundary checker extension;
- **AR-4D Profile application extraction only if AR-3 proves benefit**.

No mega-PR.

## AR-5 — Wrangler Authority Cleanup

Apply AR-2 topology decisions to canonical Wrangler/binding checks. Remove dead bindings/resources only with proof. Preserve required-secret names, environment isolation and no automatic production provisioning.

## AR-6 — Rust `opsctl` Read-only Foundation

Create operations ledger over `scripts/` and `tools/`. Add typed `inventory`, `plan`, `doctor`, read-only `drift`; prove parity. No mutation.

## AR-7 — Typed Environment Model + Controlled Mutation Cutover

Define staging/production/rehearsal persistent identity model without duplicating Wrangler. Migrate mutation concerns one at a time after parity/rehearsal. GitHub Environment remains approval authority; Wrangler may remain Worker mutation executor.

## AR-8 — Credential / Secret / Key Lifecycle and Refresh Concurrency

Preserve existing onboarding state machine, OAuth ceremonies, encrypted resolver store and versioned keyrings. Implement:

1. repository-safe credential/key **policy** registry governed by `DATA_CLASSIFICATION`;
2. protected live operational inventory for sensitive identities/handles/state;
3. per-credential refresh single-flight/CAS/fence shared by implicit and explicit refresh;
4. durable reconciliation from provider revocation/invalid refresh credential to existing `ReauthRequired` state;
5. uniform key active/previous/retirement/recovery policy;
6. mechanical redaction/classification checks and concurrency/rotation negatives.

## AR-9 — D1 Evolution / Schema Compatibility

Add migration classes, catalog/resolver revisions, per-component compatibility windows, migration concurrency and rollback blocker; integrate release-set metadata.

## AR-10 — Runtime and Historical Executable Simplification

Execute accepted deletions/simplifications. Explicitly resolve `GENERATION_VERIFICATION`, historical `cloud_profile_smoke.py`, `profile_browser.py` classification/hardening and any orphan binding/adapter/tool discovered by inventory.

## AR-11 — Release-set Simplification

Generalize D2/D3 provenance into explicit multi-component release set with no-rebuild promotion, protocol compatibility and schema windows.

## AR-12 — Fresh Rehearsal Environment

Prove `inventory -> plan -> create -> verify -> application bootstrap -> plan again == NO CHANGE` with no production data/secrets.

## AR-13 — Rotation Rehearsal

Execute key/credential rotation against rehearsal resources: active-write/previous-read, no infra recreation, safe retirement and fail-closed rollback/reauth.

## AR-14 — Remote Recovery Rehearsal

Extend repository-local DR to disposable real Cloudflare resources/key ceremonies. Measure RTO/RPO and prove catalog/resolver D1, R2, key/credential, Queue and application invariant recovery.

## AR-15 — Windows Release & Update Architecture

Implement updater/publisher only; preserve Bridge runtime. Signed manifest, staged side-by-side install, safe activation, health/LKG rollback, negative recovery paths.

## AR-16 — Production Provisioning and Promotion

Only after architecture/rehearsal acceptance, provision/verify production persistent resources and promote exact release set through protected gates. No schema/credential lifecycle is smuggled into application deploy.

## AR-17 — Whole-project 10/10 Re-audit

Re-audit backend, frontend, contracts, docs, executable tools, Cloudflare topology, DB evolution, credential refresh/rotation, release/protocol compatibility, recovery, Windows updater and CI/security.

Required repository-owned result: `P0=0`, `P1=0`.

New-engineer acceptance: an unfamiliar engineer can identify current/target/historical truth, explain every deployable/resource owner, run verification, find contract generators, run doctor/rehearsal dry-run and locate recovery/rotation procedures without issue archaeology.

Only accepted AR-17 may return Phase 2J to `unblocked_not_started`; this alone does not set `production_ready=true`.

# 21. Historical roadmap mapping

- R1–R9: preserve accepted history/regression protections;
- Pre-2J A/B/C: preserve;
- D1 Wrangler: preserve, refine AR-5;
- D2 immutable release: preserve, generalize AR-11;
- D3 repository-side promotion: preserve subject to AR-2 compatibility;
- D3 external: gated by AR-2 if still unaccepted at v3 activation;
- D4: absorbed AR-9;
- D5: useful evidence preserved, remaining AR-3/AR-10/AR-17;
- D6: AR-1/AR-6/AR-7/AR-12–14/AR-17;
- Batch E Windows updater: AR-15;
- Batch F: superseded forward by AR-17;
- Phase 2J: blocked until AR-17 acceptance.

# 22. Forbidden shortcuts

- greenfield rewrite;
- Terraform/generic IaC replacement;
- `opsctl` as second mutator;
- new registry where architecture inventory can evolve;
- one-crate-per-interface fragmentation;
- one-checker-per-finding when an existing fitness framework can extend;
- Queue/Worker/tool deletion without ownership proof;
- target docs treated as accepted;
- historical executables left operational-looking;
- treating sensitive credential/key identity metadata as Git-safe by default;
- new OAuth lifecycle parallel to accepted onboarding domain;
- refresh by uncoordinated load-provider-upsert after AR-8;
- deploy creating durable resources;
- key rotation by resource recreation;
- app rollback assuming D1 rollback;
- rebuild between staging/production;
- repository-local recovery relabelled as remote evidence;
- Bridge runtime relabelled as updater;
- rewriting historical ledgers;
- weakening exact-head CI.

# 23. Final Definition of Done

V3 completes only when:

- one current execution/documentation authority is mechanically enforced;
- every implementation-influencing document has status class;
- every generated TS contract has source/generator/authority inventory row;
- every executable operational/research tool has role/environment class;
- historical planning ambiguity is eliminated;
- all capabilities have domain/application/adapter/composition ownership;
- dependency/wiring leakage fails CI;
- Client Mail transport is thin and provider/D1 wiring composition-owned;
- runtime has no ownerless binding/resource;
- mutable-active-R2 historical operational paths are absent;
- Wrangler/environment/derived-manifest responsibilities do not overlap;
- no Terraform/generic hidden state;
- `opsctl` has no competing mutation authority;
- sensitive live credential/key metadata stays in protected inventory according to data classification;
- OAuth refresh is single-authority/race-safe and provider revocation reconciles to `ReauthRequired`;
- catalog/resolver schema windows are enforced;
- release-set components are immutable and protocol/schema compatible;
- staging/production use identical component bytes;
- credentials/keyrings have executable rotation/retirement/recovery;
- fresh rehearsal converges to no-change;
- remote recovery passes with measured RTO/RPO decision;
- Windows updater passes signed/staged/health/LKG rollback;
- public contracts remain compatible or have accepted versioning proof;
- AR-17 ends at repository-owned `P0=0`, `P1=0`;
- a new engineer can operate/understand the repository without tribal knowledge;
- `production_ready=false` remains truthful until later external evidence permits otherwise.
