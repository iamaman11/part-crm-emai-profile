# Delivery Roadmap

**Статус:** normative execution order  
**Дата:** 2026-08-05  
**Исполнитель:** автономная разработка через GitHub repository, pull requests и
GitHub Actions в текущей среде

Этот документ уточняет порядок реализации из `IMPLEMENTATION_PLAN.md`. При
конфликте по очередности repository steps этот документ является источником
истины; архитектурные инварианты исходного плана сохраняются.

## 1. Модель Выполнения

Каждый Repository Step выполняется отдельной веткой и pull request:

1. baseline `main` фиксируется commit SHA;
2. изменение включает код, tests и обновление evidence/status;
3. постоянные workflows должны быть зелёными;
4. PR проверяется по diff, comments и unresolved threads;
5. принимается squash merge;
6. `docs/status.json` обновляется только по доказанным результатам.

Работы, доступные в текущей среде, выполняются без ожидания ручных действий:
repository files, architecture, Rust/TypeScript/Python code, GitHub Actions,
issues, PR, review fixes и merge после зелёного CI.

Внешние действия не симулируются и не объявляются выполненными без evidence:
отзыв чужого credential, Cloudflare account provisioning, физический Windows
host, trusted code-signing certificate, offline key escrow и юридическое
одобрение certification targets.

## 2. Repository Steps

### Step 0 — Executable Foundation

- exact Rust toolchain и locked workspace;
- pure primitives crate и tenant scope;
- Linux, Windows и WASM quality gate;
- machine-readable status;
- product, security, delivery и evidence governance;
- tracking issues для внешних gates.

**Gate:** workspace форматируется, компилируется и тестируется на Linux/Windows;
domain primitive компилируется в `wasm32-unknown-unknown`; tracked-file secret
scan и status validation зелёные.

### Step 1 — Cloudflare Cold-Build Spike

- minimal `workers-rs` Worker crate;
- exact dependency pins после cold build;
- Static Assets routing contract;
- D1, R2, Queue и Durable Object bindings behind adapters;
- local fake bindings и production Worker build;
- no-cloud-credential integration tests.

**Gate:** production WASM build и local binding tests зелёные. Remote staging
остаётся внешним evidence gate, если Cloudflare credentials недоступны.

### Step 2 — Domain And Contract Skeleton

- opaque IDs, actor/tenant context and safe paths;
- identity, client, profile, session и mailbox domain crates;
- application ports and use-case boundaries;
- OpenAPI/protobuf version roots;
- forbidden dependency architecture test.

**Gate:** domain state and architecture tests зелёные на native и WASM.

### Step 3 — D1 Catalog Foundation

- migrations for tenant, membership, client, profile, assignment and grants;
- typed tenant-scoped repositories;
- optimistic aggregate versions;
- idempotency, audit и outbox;
- migration replay and negative isolation tests.

**Gate:** unscoped repository API невозможно использовать из application code;
IDOR/cross-tenant tests fail closed.

### Step 4 — Identity, Clients And ACL Slice

- Access JWT adapter plus fake test identity;
- owner bootstrap/transfer invariant;
- invitations, memberships and revoke;
- client/profile metadata, assignments and grants;
- owner/member web API and initial React UI.

**Gate:** owner/member use cases работают end-to-end через generated contracts;
direct endpoint abuse не раскрывает resources.

### Step 5 — Profile Coordinator

- one Durable Object per profile;
- monotonic lease epoch and fencing token;
- launch intent, heartbeat, idle/hard TTL and drain;
- duplicate, reorder, eviction and stale-writer tests;
- D1 projection and reconciliation protocol.

**Gate:** delayed writer после lease turnover не может commit logical result.

### Step 6 — Windows Bridge Feasibility

- Windows-native Rust executable in CI;
- custom URI parsing with single-use opaque code;
- device key abstraction and DPAPI/CNG adapter boundary;
- process handle/job supervision test fixture;
- encrypted local workspace and SQLite outbox skeleton;
- fake Camouhost typed IPC.

**Gate:** Windows runner proves enrollment protocol, process ownership, bounded
shutdown and dirty-state preservation without requiring physical Camoufox.

### Step 7 — Camouhost Runtime Bundle

- embedded Python/Camoufox packaging boundary;
- protobuf IPC and exact runtime manifest;
- signed/content-addressed development bundle format;
- create/open/graceful-close on disposable synthetic profile;
- original legacy corpus remains untouched.

**Gate:** disposable profile lifecycle works on an approved Windows evidence host;
GitHub CI covers contract and packaging independently.

### Step 8 — Local Profile Lifecycle

- safe materialization paths and OS locks;
- deterministic inventory and clone-only integrity checks;
- crash recovery, forgotten-window and quota policies;
- no secret/PII support bundle;
- generation state machine.

**Gate:** dirty local generation cannot be evicted; lock files are never deleted
blindly; recovery runs only on clone.

### Step 9 — Encrypted Cloud Generations

- accepted ADR-0006 implementation;
- reviewed streaming AEAD container and test vectors;
- immutable R2 adapter, verification and pointer CAS;
- restore, rollback, orphan reconciliation and retention;
- clean-environment key/data restore evidence.

**Gate:** create -> close -> encrypt -> upload -> remove local clone -> restore ->
replay succeeds; corruption and stale fencing fail closed.

### Step 10 — Certification And Multi-Device

- accepted ADR-0001 signal policy;
- drift and repeatability matrix;
- second independent Windows device;
- device-scoped unwrap and revoke;
- signed Bridge/runtime update with rollback.

**Gate:** authorized second device restores exact generation; revoked device
cannot obtain new key material; failed update rolls back.

### Step 11 — Mailbox Operations

- Gmail OAuth/API and IMAP adapters through secret handles;
- bounded jobs, cursors, retries and provider rate limits;
- browser-assisted fallback as separate capability;
- safe metadata UI and audit;
- Communications ownership contract for CRM.

**Gate:** tokens/message content never enter logs or ordinary audit; provider
contract tests and revoke behavior are deterministic.

### Step 12 — Production Operations And CRM Adapter

- environment promotion, rollback, backup and disaster game day;
- SLO, alerts, DLQ and cost controls;
- stable Windows signing/update channel;
- v1 contracts/events;
- D1-to-PostgreSQL schema map, shadow parity, cutover and rollback protocol;
- CRM Party projection.

**Gate:** operations recover catalog, keys and generation on clean environment;
CRM cutover preserves IDs and domain decisions without direct filesystem/R2
access.

## 3. External Evidence Gates

The following issues may run in parallel but cannot be marked complete by code
alone:

- rotate/revoke the legacy proxy credential and review provider access logs;
- provision isolated Cloudflare dev/staging/prod resources and budgets;
- obtain trusted Windows code-signing certificate;
- provide second independent Windows-native evidence host;
- approve offline root-key escrow and dual-control process;
- approve legal/acceptable-use policy for certification and mailbox automation;
- choose repository license.

## 4. Status Discipline

A step is complete only after merge and green permanent workflows. A smoke test
may prove a bounded property but cannot advance unrelated gates. Proposed ADRs
remain proposed until their acceptance criteria have evidence references.
