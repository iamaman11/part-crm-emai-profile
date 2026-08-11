# Pre-2J Architecture Remediation Plan

**Status:** ACTIVE / BLOCKING PHASE 2J  
**Audit base:** accepted `main` at `d8b5bd07b99596d066fa2af976b263d41b5e2f3c`  
**Production readiness:** remains `false`  
**Rule:** Phase 2J External evidence work must not begin while any repository-owned P1 below is open.

## 1. Purpose

This plan records repository-owned architecture, contract, maintainability and documentation defects found during the pre-2J 10/10 revision. It is intentionally separate from Phase 2J: these are foundation corrections that must be accepted on `main` before external rollout evidence can be trusted.

The target is not maximal abstraction. The target is a boring, explicit architecture that can scale to future CRM capabilities without central facades, duplicate contract authorities or hidden CI gaps.

## 2. Audit verdict

At the audit base, no P0 correctness/security blocker was found. The foundation is materially strong in domain purity, tenant scoping, authorization-before-projection/provider access, fencing/CAS, durable outbox/realtime sequencing, encrypted immutable generations, Bridge recovery ownership and negative CI evidence.

However, accepted green CI is not yet sufficient for a 10/10 foundation because several P1 guardrail/authority/documentation defects remain. Phase 2J therefore stays frozen until this plan reaches the closure rule in section 6.

## 3. Findings and remediation sequence

### R1 — P1 — Query application dependency gate hole — ACCEPTED

**Accepted main:** squash merge `88d4412084f85b3512ce28a3bec637fc6e687151` via PR #173.

**Finding**

`use-cases-query` was an accepted independent provider-neutral application context, but `scripts/check-architecture.py` did not govern its dependency allowlist. A provider/runtime dependency could therefore enter that context while the main architecture gate stayed green.

**Remediation**

- govern `use-cases-query` in `PURE_CRATE_ALLOWLISTS`;
- add a permanent negative fixture proving a `worker` dependency is rejected;
- make the architecture checker own that negative proof instead of relying only on an external workflow step.

**Acceptance**

- architecture policy passes on the repository;
- the query negative fixture fails closed;
- all permanent workflows pass at exact PR head.

### R2 — P1 — Client Mail bypasses canonical route authority — ACCEPTED

**Accepted main:** squash merge `443bd39a9589eb0fb75f305043a2acc1b93314a1` via PR #175.

**Finding**

Client Mail ingress was dispatched before `control_plane_contract::classify_route` and `apps/control-plane-worker/src/client_mail_query.rs` owned a second `matches_route` implementation. This violated the intended single fail-closed route authority and allowed route semantics to drift between contract and Worker transport.

**Remediation**

- make Client Mail paths/methods first-class `RouteClass` variants in the canonical contract classifier;
- dispatch Client Mail only from canonical `RouteClass` values;
- delete Worker-local path/method matching;
- add negative route tests for wrong method/version/shape and static fallback denial.

**Acceptance**

- one route authority for all authenticated `/api/v1/**` public surfaces;
- no Client Mail route literal classifier in Worker ingress;
- permanent route tests fail closed.

### R3 — P1 — Contract compatibility gate can miss breaking OpenAPI/protobuf changes — ACCEPTED

**Accepted main:** squash merge `d3bbd49dde9129e52b7c72bff053ce82a325bc0b` via PR #177.

**Finding**

The previous compatibility checker detected removed paths/properties but did not comprehensively protect schema directionality or protobuf field semantics. In particular, it could miss request-side required-field additions, request enum narrowing, response-side required-field removal, response enum widening, incompatible type/format/reference changes, and protobuf type/cardinality changes.

A first remediation attempt deliberately exposed why component schemas cannot be judged by a direction-free rule: the accepted `Problem` schema is response-only, so adding guaranteed response fields and narrowing values the server may emit are compatible with old consumers even though the same changes would be breaking for a request schema. The permanent policy therefore classifies accepted schemas by request/response use before applying required/enum compatibility rules.

**Remediation**

- derive request/response schema roles from accepted OpenAPI operations and local component references;
- for request schemas, reject newly required fields and enum narrowing while allowing optional-field additions and accepted-value widening;
- for response schemas, reject removal of guaranteed required fields and enum widening while allowing additional guaranteed fields and output-value narrowing;
- treat schemas used in both directions, or with unknown role, conservatively under both rules;
- preserve accepted type/format/reference identity for existing constrained fields and check supported array item constraints recursively;
- preserve protobuf field number/name/type/cardinality while allowing new field numbers;
- run deterministic positive and negative self-tests from the existing contract baseline interlock;
- keep the accepted v1 baseline immutable.

**Acceptance**

- every supported breaking class has a negative fixture;
- compatible request/response additive evolution has positive fixtures;
- the current accepted v1 contract tree passes without grandfathered path/schema exceptions;
- accepted v1 baseline remains immutable;
- compatibility gate remains deterministic and provider-independent.

### R4 — P1 — Public contract authority is incomplete; handwritten frontend DTOs remain — ACTIVE

**Active batch:** R4a — Profile + Generation via issue #178 / draft PR #179.

**Finding**

Rust -> OpenAPI -> generated TypeScript is canonical only for migrated surfaces. Profile, generation, mailbox and coordinator DTOs coexist as handwritten definitions in `frontend/src/shared/api/types.ts` and handwritten request shapes in the central endpoint facade. That creates multiple wire-contract authorities and will compound as Billing, Orders, Projects and other CRM capabilities are added.

**Remediation**

Migrate legacy public surfaces capability-by-capability, not into one giant contract file:

- R4a migrates Profile + Generation transport DTOs to `crates/control-plane-contract/src/profile_generation_api.rs`, deterministic OpenAPI schema fragment and generated TypeScript;
- R4b will migrate Mailbox transport DTOs in a separate PR from the next accepted `main`;
- R4c will migrate Coordinator transport DTOs in a separate PR from the next accepted `main`;
- replace Worker-local and SPA handwritten wire DTOs with generated/canonical types while keeping domain-to-wire mapping at the transport boundary;
- retain existing operation/security metadata ownership and avoid duplicate OpenAPI path authorities;
- keep frontend-local view models only when they are presentation models rather than transport DTOs;
- keep compatibility baselines additive and versioned.

**R4a progress**

- Rust-owned Profile/Generation request, projection and enum DTOs exist in `control-plane-contract`;
- Worker Profile/Generation handlers use canonical DTOs and shared `MutationReceipt` rather than local transport structs;
- Profile unknown-field tolerance and Generation fail-closed unknown-field behavior are preserved from runtime semantics;
- deterministic schema-only OpenAPI and TypeScript artifacts are generated from the Rust source without duplicating legacy operation paths;
- SPA Profile/Generation projections and request-body types are generated rather than handwritten;
- architecture inventory registers the complete Rust -> OpenAPI -> TypeScript chain;
- exact-head permanent CI and merge interlocks remain required before R4a can be accepted.

**Acceptance**

- every public HTTP wire DTO used by the SPA has one Rust-owned contract source once R4a/R4b/R4c complete;
- handwritten frontend transport interfaces for each migrated surface are gone;
- generated-contract freshness gate covers the full accepted public SPA surface;
- each R4 batch passes all permanent workflows at exact head before merge.

### R5 — P1/P2 — Historical `use-cases` compatibility coupling is cemented by CI

**Finding**

The shared `use-cases` crate genuinely owns remaining Profile/Generation/cross-resource ACL orchestration, but it also depends on and re-exports extracted `use-cases-clients`, `use-cases-identity` and `use-cases-mailboxes` solely as compatibility import paths. Worker code still consumes those re-exports, and `check-capability-module-layout.py` explicitly requires them. This turns a historical migration bridge into permanent inward coupling.

**Remediation**

- update Worker composition/transport imports to depend directly on owning capability application crates;
- remove compatibility re-exports and now-unneeded extracted-crate dependencies from shared `use-cases`;
- change capability-layout policy to forbid ownership returning to `use-cases` without requiring historical re-exports;
- retain genuinely shared Profile/Generation/ACL orchestration in `use-cases`; do not create new crates merely for symmetry.

**Acceptance**

- `use-cases` no longer depends on extracted capability application crates solely for re-export;
- Worker imports ownership directly;
- policy protects ownership, not migration history.

### R6 — P1/P2 — Frontend API facade is becoming a central bottleneck

**Finding**

`frontend/src/shared/api/endpoints.ts` and `types.ts` centralize many unrelated capabilities. Feature-sliced imports are otherwise well enforced, but continued growth of a shared endpoint/type facade would reintroduce a frontend application monolith.

**Remediation**

- keep only transport primitives, common problem handling and generated contract exports in `shared/api`;
- move capability endpoint functions/adapters behind owning feature public APIs or bounded capability API modules;
- prohibit sibling-feature internals as today;
- avoid duplicating generated DTOs in feature code.

**Acceptance**

- adding a new CRM capability does not require editing a giant shared endpoint/type registry beyond intentionally shared transport registration;
- root/feature boundary gate remains green with a negative fixture for cross-feature internals.

### R7 — P1 — Current repository documentation is not a single truthful authority

**Finding**

The root `README.md` still presents older Repository Steps as current orientation, `docs/status.json` is stale relative to accepted Phases 1A-2I, and canonical `docs/THREAT_MODEL.md` is still labelled a Phase 0 baseline while the effective current security controls live in `docs/PHASE2I_THREAT_MODEL.md`. A senior engineer entering through the documented entrypoint receives conflicting status/security truth.

**Remediation**

- make root README a concise current architecture/status entrypoint and link historical step material explicitly as history;
- update `docs/status.json` to the accepted pre-2J state without claiming production readiness;
- fold current repository-local threat controls/residual risks into canonical `docs/THREAT_MODEL.md` and treat phase-specific threat documents as evidence/history;
- add/extend documentation consistency checks for current phase/status/security authority.

**Acceptance**

- README, `docs/README.md`, `docs/status.json`, `DEVELOPMENT_PLAN.md` and canonical threat model agree on current state;
- no Phase 2J or production-ready claim is introduced.

### R8 — P2 — Green CI still tolerates owned warning/dead-code debt and duplicates expensive checks

**Finding**

Native workspace clippy already uses `-D warnings`, but Cloudflare adapter tests emit owned unused-import and deprecated-API warnings, and `apps/control-plane-worker/src/mutation_failure.rs` is dead legacy code. Repository Quality Audit also repeats a material subset of Quality Gate work.

**Remediation**

- remove dead legacy code and unused imports;
- replace owned deprecated crypto API usage safely;
- make adapter lint/check lanes warning-free where the Workers toolchain permits it;
- inventory duplicated workflow steps and remove only exact duplicates while preserving unique negative/evidence guarantees.

**Acceptance**

- owned Rust code is warning-free in permanent CI lanes;
- no dead mutation string-classifier remains;
- CI reduction is evidence-preserving, not gate weakening.

### R9 — P2 — Certification domain contains a second device-authorization model

**Finding**

`certification-domain` contains an in-memory `DeviceAuthorizationRegistry` with grant/revoke/version/unwrap semantics while production device authorization is separately owned by `device-domain` plus D1/application layers. Repository search found this registry only in certification-domain tests. Two domain meanings called device authorization are unnecessary ambiguity.

**Remediation**

- verify no production/runtime consumer exists;
- either remove the obsolete synthetic registry or isolate/rename it as certification-only evidence with no production-authority semantics;
- keep actual device authorization ownership exclusively in `device-domain`/application persistence.

**Acceptance**

- one production device-authorization domain owner;
- certification evidence cannot be mistaken for runtime authorization state.

## 4. PR strategy

Use multiple bounded PRs. The expected sequence is:

1. **Guardrail integrity** — R1. Accepted via PR #173 / `88d4412084f85b3512ce28a3bec637fc6e687151`.
2. **Canonical routing** — R2. Accepted via PR #175 / `443bd39a9589eb0fb75f305043a2acc1b93314a1`.
3. **Compatibility semantics** — R3. Accepted via PR #177 / `d3bbd49dde9129e52b7c72bff053ce82a325bc0b`.
4. **Contract authority migration** — R4a Profile + Generation active via issue #178 / draft PR #179; then R4b Mailbox and R4c Coordinator as separate PRs.
5. **Application ownership cleanup** — R5.
6. **Frontend API modularization** — R6, preferably aligned with R4 capability migrations rather than a mechanical rewrite.
7. **Current documentation/security authority** — R7.
8. **Warning/dead-code/CI hygiene** — R8.
9. **Certification ownership cleanup** — R9.
10. **Final pre-2J audit closeout** — re-run full inventory, verify no repository-owned P0/P1, record accepted exact-head evidence, then and only then resume Phase 2J.

R4/R6 may require more than one PR because public contract migration should be capability-bounded. A single mega-PR is explicitly rejected for this remediation because it would weaken reviewability and exact-head evidence.

## 5. Rules for every remediation PR

- branch from the latest accepted `main` after the previous remediation merges;
- one coherent architectural risk per PR; no unrelated cleanup;
- preserve `production_ready=false` and Phase 2J External evidence state;
- update tests/negative fixtures with the implementation, not later;
- require all permanent workflows green at the exact source head;
- require `behind_by=0`, no unresolved review threads and no unaddressed review findings before merge;
- prefer deleting compatibility/dead code over adding another facade;
- add a new abstraction only when it has a concrete current owner/use case and reduces coupling;
- do not split crates/modules mechanically for line count alone.

## 6. Closure rule before Phase 2J

Phase 2J may resume only when all of the following are true on accepted `main`:

- repository-owned P0 = 0;
- repository-owned P1 = 0;
- all R1-R7 findings are closed with permanent regression evidence;
- remaining P2 items are either closed or explicitly documented as non-blocking with no hidden correctness/security/contract risk;
- current documentation and machine-readable status agree;
- full permanent workflow set is green at the exact accepted head;
- `production_ready` is still `false` until Phase 2J external acceptance itself is completed.

## 7. Non-findings worth preserving

The audit explicitly found no reason to rewrite these working foundations:

- provider-neutral domain crates and dependency direction;
- tenant-inclusive persistence keys and transactional invariants;
- authorization-before-projection/provider sequencing and neutral denial;
- coordinator epoch/fencing/CAS and stale-writer rejection;
- immutable encrypted generation model and retained dirty-state recovery ownership;
- durable outbox/catch-up before realtime invalidation;
- metadata-only realtime refetch authority model;
- feature-sliced frontend sibling boundary enforcement;
- bounded metadata-only operational/support evidence.

Remediation must preserve these properties rather than replacing them with a broader framework.
