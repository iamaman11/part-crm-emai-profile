# Developer Capability and Module Matrix

**Статус:** normative developer orientation  
**Дата:** 2026-08-07  
**Tracking:** completed hardening #41; composition epic #43; generation slice #44 / PR #51

## 1. Зачем Нужен Этот Документ

Repository Steps 0–10 приняли много строгих domain, adapter, workflow и synthetic
boundaries. Слово `accepted` означает, что конкретный bounded claim подтверждён
кодом и permanent CI. Оно не означает, что весь целевой продукт уже собран,
развёрнут или проверен на реальных внешних ресурсах.

Эта матрица является быстрым источником истины для разработчика:

- что реально вызывается текущим composition root;
- что реализовано как reusable pure domain/library;
- что доказано только synthetic/fake harness;
- что пока существует только как target architecture или external gate.

## 2. Уровни Готовности

| Уровень | Значение |
|---|---|
| **Composed** | Подключено к текущему executable composition root и проверяется его CI lane. |
| **Library** | Реальная typed implementation существует, но не полностью подключена к пользовательскому executable path. |
| **Synthetic** | Invariants и protocol доказаны fake/in-memory/generated fixtures; real provider/runtime не заявлен. |
| **Target** | Нормативно спроектировано, но executable implementation отсутствует или неполна. |
| **External** | Требует provider, physical host, policy, signing или независимого evidence вне GitHub CI. |

Ни один уровень сам по себе не означает production readiness. Branch/PR capability
становится accepted только после exact-head green и merge. Поэтому строки,
изменяемые текущим PR, описывают composition candidate и становятся состоянием
`main` только после acceptance.

## 3. Текущая Capability Matrix

| Capability | Текущий уровень | Фактически реализовано | Не реализовано / не доказано |
|---|---|---|---|
| Rust workspace и pure primitives | Composed | Exact toolchain, typed opaque IDs, tenant/actor context, positive aggregate versions, strict lint policy. | Нет внешнего runtime dependency. |
| Identity, memberships и ACL | Composed | Access identity adapter, memberships, owner lifecycle, invitations, profile/client grants, neutral disclosure, governed D1 commands. | Реальный production Access/IdP deployment остаётся External. |
| Client Registry | Composed | Create/query/assignment/grant bounded Worker paths и D1 schema. | Полный CRM Customer Master, merge UI и advanced contact workflows — Target. |
| Profile catalog | Composed | Create/query/grant/assignment metadata paths, typed profile state и active generation pointer. | Реальные encrypted object operations выполняются не catalog, а будущим R2/provider flow. |
| Profile generation registry | Composed candidate (#51) | Metadata-only register/query/verify/activate/deactivate/quarantine routes, exact command+digest+expiry replay, collision-resistant deterministic evidence IDs, command journals, audit/outbox, monotonic time, immutable digests/object identity и verified-pointer integrity. | Production R2 object verification, device unwrap и cross-device execution — External. |
| Profile Coordinator | Composed | Durable Object journal, monotonic sequence/version/epoch, fencing, timeout/drain/recovery, D1 projection. | Remote production concurrency evidence — External. |
| Windows Profile Bridge executable | Library | `profile-bridge.exe` build, strict redacted claim-URI CLI, local-profile/runtime modules and Windows lane. | Текущий executable только принимает claim URI; complete enrollment/network/device-key/runtime composition отсутствует. |
| Device identity/key ports | Synthetic | Typed ports и deterministic fake implementations. | Production CNG/DPAPI/TPM unwrap/revoke/recovery — External. |
| Camouhost IPC и process supervision | Synthetic | Versioned messages, fake Camouhost, process state machine, generated subprocess/runtime fixtures. | Real bundled Python/Camoufox lifecycle на physical host — External. |
| Runtime bundle | Synthetic | Canonical manifest, inventory, path/case safety, digest checks, approval/rollback tests. | Trusted signed distribution/update channel — External. |
| Local profile lifecycle | Library / Synthetic | Marked workspace, inventory, lock ownership, clone-only recovery, quota/support policies and Bridge library tests. | Full kernel-lock/real-browser integration on physical Windows hosts — External. |
| Encrypted cloud generations | Synthetic | XChaCha20-Poly1305 container, metadata authentication, nonce domain, immutable in-memory lifecycle, pointer/rollback/quarantine/orphan policies. | Production R2 adapter, device unwrap and remote R2/D1 atomicity — External. |
| Certification | Synthetic | Typed policy, deterministic matrix, prohibited/incomplete/drift outcomes, privacy-safe summary and update rollback state. | Real Camoufox observations, specialized-site review and independent certification — External. |
| Mailbox operations | Library | Provider-neutral binding/job domain and mailbox provider port. | Gmail/IMAP/browser adapters, API routes, scheduling, persistence and user workflow are not composed. |
| React web UI | Target | UI architecture and route contracts are documented. | В репозитории нет `frontend/package.json`; current Worker has no composed React build. |
| CRM integration | Target | Versioned boundary principles and replaceable adapter direction documented. | CRM Party projection, OIDC/PostgreSQL adapters and event integration отсутствуют. |
| Production readiness | External | Immutable evidence intake, readiness projection and GitHub attestation interlocks are composed. | Mandatory external evidence matrix currently incomplete; `production_ready` remains `false`. |

## 4. Module Ownership

```text
crates/primitives
  Stable value objects only. No provider, storage or business workflow.

crates/*-domain
  Pure decisions and state machines. No Worker, D1, Windows, Python or HTTP.

crates/application-ports
  Interfaces owned by application needs. No concrete provider behavior.

crates/use-cases
  Authorization-aware application decisions. No concrete Cloudflare SDK.

crates/cloudflare-adapters
  D1, Access, Durable Object serialization/projection and Worker-facing adapters.
  Storage validation may duplicate pure value checks as defense-in-depth, but may
  not import domain policy into provider code.

apps/control-plane-worker
  Cloudflare Worker composition root and route/DTO/problem mapping.

apps/profile-bridge
  Windows executable plus Bridge-local libraries; current CLI composition is
  intentionally much narrower than the available library surface.
```

A developer must not move policy downward into an adapter or upward into a UI.
If a rule can be expressed without a provider, it belongs in a domain/use case.

## 5. Current End-to-End Paths

### Worker API path

```text
HTTP request
  -> fail-closed route classification
  -> Access identity verification
  -> active membership/grant resolution
  -> exact idempotency decision for generation mutations
  -> typed D1 or Durable Object adapter
  -> governed transaction/projection
  -> stable response/problem shape
```

This path is repository-built and integration-tested, but not a claim of deployed
production infrastructure.

### Profile generation path

```text
register immutable metadata
  -> governed verification decision
  -> atomic activation of exact VERIFIED generation
  -> READY profile + active_generation_id
  -> coordinator eligibility
  -> governed exact-pointer deactivation when isolation is required
  -> SUSPENDED profile + NULL pointer
  -> optional generation quarantine
```

The registry proves catalog/lifecycle consistency. It does not prove that a real
R2 object exists, decrypts on a device or launches successfully in Camoufox.

### Profile Bridge path

```text
profilebridge://claim/<opaque-code>
  -> strict URI parsing and redacted CLI result
```

The richer enrollment, local workspace, runtime bundle and process modules are
currently exercised through library/synthetic tests rather than one complete
operator executable flow. New developers must not infer otherwise from the
presence of those modules.

## 6. Definition of a Complete New Capability

A capability is not considered fully composed until all applicable items exist:

1. versioned contract or typed command;
2. pure domain decision and negative tests;
3. minimal owned application ports;
4. authorization, exact idempotency and stable error mapping;
5. concrete adapter plus forward-only migration where required;
6. executable composition-root wiring;
7. replay, failure, forbidden-access and boundary tests;
8. developer documentation updated in this matrix;
9. exact-head permanent CI and squash merge;
10. external evidence only when the claim depends on real infrastructure/runtime.

## 7. Local Verification Entry Point

Use the exact commands in [`../CONTRIBUTING.md`](../CONTRIBUTING.md). CI remains
the acceptance authority because it also executes Windows, Wrangler/D1, WASM,
Worker release, runtime, local-profile, encrypted-generation, certification and
external-evidence lanes.

PR #51 adds a dedicated Profile Generation Gate; it is acceptance evidence only
after the same final head also passes every permanent repository workflow.

## 8. Audit Exclusion

Repository quality and composition work under issues #41, #43 and #44 does not
inspect, modify or operate the legacy proxy credential/provider. That external
item remains separate and has no effect on repository-local architecture findings.
