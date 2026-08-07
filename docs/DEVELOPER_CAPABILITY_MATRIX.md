# Developer Capability and Module Matrix

**Статус:** normative developer orientation  
**Дата:** 2026-08-07  
**Tracking:** completed hardening #41; completed composition epic #43; generation slice #44 / PR #51; Profile Bridge slice #54 / PR #55; mailbox slice #56 / PR #60 + repair #61 / PR #62; React UI #63 / PR #64; cross-component acceptance #65 / PR #66

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

Ни один уровень сам по себе не означает production readiness. `Composed` описывает
исполняемую wiring/composition форму capability; repository claim становится
accepted только после exact-head green и merge. Поэтому во время review capability
может быть `Composed` в feature branch, но состоянием `main` становится только
после acceptance.

## 3. Текущая Capability Matrix

| Capability | Текущий уровень | Фактически реализовано | Не реализовано / не доказано |
|---|---|---|---|
| Rust workspace и pure primitives | Composed | Exact toolchain, typed opaque IDs, tenant/actor context, positive aggregate versions, strict lint policy. | Нет внешнего runtime dependency. |
| Identity, memberships и ACL | Composed | Access identity adapter, memberships, owner lifecycle, invitations, profile/client grants, neutral disclosure, governed D1 commands. | Реальный production Access/IdP deployment остаётся External. |
| Client Registry | Composed | Create/query/assignment/grant bounded Worker paths и D1 schema. | Полный CRM Customer Master, merge UI и advanced contact workflows — Target. |
| Profile catalog | Composed | Create/query/grant/assignment metadata paths, typed profile state и active generation pointer. | Реальные encrypted object operations выполняются не catalog, а будущим R2/provider flow. |
| Profile generation registry | Composed | Metadata-only register/query/verify/activate/deactivate/quarantine routes, exact command+digest+expiry replay, collision-resistant deterministic evidence IDs, command journals, audit/outbox, monotonic time, immutable digests/object identity и verified-pointer integrity. | Production R2 object verification, device unwrap и cross-device execution — External. |
| Profile Coordinator | Composed | Durable Object journal, monotonic sequence/version/epoch, fencing, timeout/drain/recovery, D1 projection. | Remote production concurrency evidence — External. |
| Full Profile Bridge operator flow | Composed / Synthetic | Explicit `profile-bridge-synthetic` binary composes strict claim parsing, device/key/auth/enrollment, coordinator lease validation, approved runtime selection, existing generation ownership, Bridge writer lock, local lifecycle, Camouhost v1 negotiation, supervised launch/close, process stop confirmation, recovery state and fail-closed cleanup blocking. The default `profile-bridge` binary remains the accepted narrow claim-only CLI and is preserved as `default-run`. | Real Camoufox execution, production device-key protection, remote enrollment/coordinator providers, production R2 generation lifecycle and physical Windows evidence remain External. |
| Device identity/key ports | Synthetic | Typed ports и deterministic fake implementations used only by explicit synthetic composition/tests. | Production CNG/DPAPI/TPM unwrap/revoke/recovery — External. |
| Camouhost IPC и process supervision | Synthetic | Versioned messages, fake Camouhost, process state machine, generated subprocess/runtime fixtures and exact clean-stop confirmation. | Real bundled Python/Camoufox lifecycle на physical host — External. |
| Runtime bundle | Synthetic | Canonical manifest, inventory, path/case safety, digest checks, approval/rollback tests and composed synthetic selection before lease/local runtime use. | Trusted signed distribution/update channel — External. |
| Local profile lifecycle | Library / Synthetic | Marked workspace, inventory, lock ownership, clone-only recovery, quota/support policies and composed synthetic operator tests. | Full kernel-lock/real-browser integration on physical Windows hosts — External. |
| Encrypted cloud generations | Synthetic | XChaCha20-Poly1305 container, metadata authentication, nonce domain, immutable in-memory lifecycle, pointer/rollback/quarantine/orphan policies. | Production R2 adapter, device unwrap and remote R2/D1 atomicity — External. |
| Certification | Synthetic | Typed policy, deterministic matrix, prohibited/incomplete/drift outcomes, privacy-safe summary and update rollback state. | Real Camoufox observations, specialized-site review and independent certification — External. |
| Mailbox operations | Composed / Synthetic | Provider-neutral binding/job domain, D1 persistence, strict secret-handle-only request DTOs, idempotency/audit/outbox, versioned Worker create/query/revoke/job/run routes and metadata-only synthetic provider decision path. Adapter modules are exported and exercised by Worker native/WASM/release and Cloudflare adapter tests. | Real Gmail API/IMAP/browser provider execution, mailbox message payload processing, production scheduling and external provider evidence remain unproven. |
| React web UI | Composed / Synthetic | Accepted PR #64 provides exact Node 24.19.0/npm 11.17.0 workspace, React 19/Vite 8/TypeScript 7, same-origin typed API/problem layer, tenant-scoped operator shell, session/client/profile/ACL/assignment/generation/coordinator/mailbox/user surfaces, neutral disclosure, high-impact confirmation, strict tests and permanent Frontend Gate. Worker Static Assets targets `frontend/dist`. | No deployed Cloudflare Access UI, real Bridge onboarding/custom-URI acceptance, real provider execution, or missing backend list APIs are claimed. |
| Cross-component standalone acceptance | Composed / Synthetic | Accepted PR #66 provides a deterministic metadata-only manifest/validator and permanent read-only lane that executes governed D1 invariants, generation integrity, Worker/adapters native+WASM, actual synthetic Bridge CLI, and Node24 frontend tests/build in one repository-local flow. Exact accepted head and 12/12 CI are recorded in the evidence index. | No external deployment/provider/device evidence is implied; all production gates remain External. |
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
  Windows executable plus Bridge-local libraries. `profile-bridge` remains the
  narrow default claim-only CLI. `profile-bridge-synthetic` is the explicit
  repository-local composed operator path and must never be described as a real
  Camoufox or production-provider implementation.

frontend
  React operator composition only. Routes/forms display accepted projections and
  invoke typed same-origin API calls; authorization, lifecycle transitions and
  provider decisions remain server-side. No secret/token persistence in Web Storage.
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

### React operator path (PR #64 candidate)

```text
Cloudflare Static Assets / frontend/dist
  -> React operator route
  -> explicit tenant + opaque resource ID
  -> same-origin /api/v1 request
  -> bounded JSON/problem normalization
  -> Worker re-authorizes every operation
  -> authoritative server projection/result
```

The UI intentionally does not invent list/read APIs that the Worker does not own.
Client/profile/generation/mailbox resources are resolved by explicit opaque ID.
High-impact mutations are confirmed and never optimistically treated as success.
This is repository-local composition evidence only until PR #64 is accepted and
external deployment evidence exists.

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

### Profile Bridge paths

The default accepted CLI stays deliberately narrow:

```text
profile-bridge profilebridge://claim/<opaque-code>
  -> strict URI parsing
  -> redacted claim result
```

The repository-local composed path is explicit and synthetic:

```text
profile-bridge-synthetic profilebridge://claim/<opaque-code> <absolute-materialization-root>
  -> strict claim redemption
  -> deterministic fake device identity + key handle
  -> explicit synthetic authentication/enrollment
  -> approved runtime selection
  -> coordinator lease acquisition + exact tenant/profile/device validation
  -> existing generation workspace + Bridge writer lock using lease epoch
  -> LocalGenerationRecord InUse
  -> supervised process spawn + Camouhost v1 Hello/Launch
  -> exact clean Close + process stop confirmation
  -> DirtyLocal + writer lock release + coordinator lease close
```

Any runtime/protocol failure after local use transitions to `RecoveryRequired`.
Cleanup failures remain observable; unresolved cleanup blocks another operator
session in the same composed instance. This proves composition and failure
ordering only. It does not prove a real Camoufox binary, real remote enrollment,
production coordinator deployment, production key protection or R2 object use.

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
Worker release, runtime, local-profile, encrypted-generation, certification,
frontend and external-evidence lanes.

For the Profile Bridge composition slice, `cargo test --locked -p profile-bridge --all-targets`
covers the library state machine, explicit synthetic executable and integration
failure-ordering regressions. Repository acceptance still requires every permanent
workflow on the same final head.

For the accepted React UI, `frontend/.nvmrc`, `package.json` engines/packageManager
and `.github/workflows/frontend-gate.yml` pin Node `24.19.0` and npm `11.17.0`.
The permanent lane performs `npm ci`, strict TypeScript, Vitest, Vite production
build, application-source credential-persistence scanning and Static Assets output
verification.

## 8. Audit Exclusion

Repository quality and composition work under issues #41, #43, #44, #54, #56,
#61 and #63 does not inspect, modify or operate the legacy proxy credential/provider.
That external item remains separate and has no effect on repository-local
architecture findings.
