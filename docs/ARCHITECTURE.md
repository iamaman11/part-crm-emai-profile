# Architecture Map

**Статус:** normative target architecture

**Дата:** 2026-08-05

**Для кого:** developer, reviewer, operator и будущая CRM integration team

## 1. Системная Граница

Browser Profile Platform является самостоятельным продуктом и будущим внешним
CRM bounded context. Он владеет профилями, поколениями, browser sessions,
назначениями профилей клиентам, profile ACL, certification и cloud lifecycle.

До CRM integration локальный Client Registry владеет минимальной client card.
После integration CRM Party/Customer Master становится authoritative owner, а
платформа хранит `party_ref` и read projection. Cloudflare, D1, R2, Python и
Camoufox являются adapters/runtime, но не источником domain policy.

## 2. Runtime Topology

```text
Browser routes through Cloudflare Access
  -> app.example.com (one origin, one deployment)
    -> Rust Worker /api/*
    -> React SPA through Workers Static Assets
    -> D1 authoritative standalone catalog + audit + outbox
    -> Durable Object profile coordinator
    -> Queues and Scheduled Worker consumers
    -> R2 encrypted immutable objects
    -> Workers Secrets/Secrets Store key root and service secrets

Web UI
  -> profilebridge://claim/<single-use-code>
    -> Windows Profile Bridge
      -> HTTPS Worker API + device proof
      -> local SQLite cache/outbox
      -> local encrypted staging/materialization
      -> typed local IPC
        -> embedded Python Camouhost
          -> separate visible Camoufox process
```

Standalone v1 не имеет VM/backend daemon, PostgreSQL или Keycloak. Cloudflare
Workers выполняет только control-plane code. Camoufox, browser profile data,
filesystem locks и native process supervision остаются на Windows-компьютере.
Cloudflare Browser Rendering/Browser Run не заменяет Camoufox, а Containers не
используются как stateful profile filesystem.

## 3. Deployment И URL Boundary

Один Workers deployment обслуживает:

```text
https://app.example.com/             React SPA
https://app.example.com/profiles/*   SPA routes
https://app.example.com/clients/*    SPA routes
https://app.example.com/api/v1/*     Rust Worker API
https://app.example.com/auth/*       identity/bootstrap endpoints
https://app.example.com/bridge/*     enrollment/claim endpoints
```

Static Assets использует SPA fallback, а Worker запускается первым для
`/api/*`, `/auth/*` и `/bridge/*`. UI и API same-origin, поэтому CORS не является
частью штатной browser-модели. Browser routes защищает Cloudflare Access; Worker
проверяет Access JWT и затем живой membership/grant. `/bridge/*` принимает только
rate-limited device/enrollment protocol: enrollment требует actor-bound
single-use intent, остальные команды требуют device signature и short-lived app
token. Access доказывает identity, но не право на profile/client, а Bridge не
зависит от browser cookie.

## 4. Слои И Разрешенные Зависимости

```text
apps
  -> use-cases
    -> application-ports + domains + contracts + primitives

adapters
  -> application-ports + contracts + primitives

domains
  -> primitives
```

### `primitives`

Opaque IDs, tenant scope, safe path segments, digests, time/value types и общие
validation primitives. Здесь нет HTTP, Cloudflare bindings, SQL, authorization
storage или browser-specific behavior.

### Domain Crates

- `identity-access-domain`: tenant owner, memberships, grants и authorization
  decisions;
- `client-domain`: client cards, contact points и assignment rules;
- `profile-domain`: profile, generation, snapshot и fingerprint policies;
- `session-domain`: launch intent, lease, fencing, session and recovery states;
- `mailbox-domain`: provider-neutral mailbox binding/check rules.

Domain принимает value objects и возвращает decisions/events. Он собирается как
обычный native Rust для tests/Profile Bridge/будущей CRM и как
`wasm32-unknown-unknown` dependency Cloudflare Worker. Он не знает о D1, Durable
Objects, R2, Access, Windows, Python или wall-clock singleton.

### `application-ports`

Порт принадлежит use case, который его потребляет:

- tenant-scoped catalog repositories;
- live identity, membership и authorization;
- profile coordinator, clock, idempotency, audit и outbox;
- object store, envelope encryption и key provider;
- local workspace и browser runtime;
- certification checker/artifact store;
- mailbox provider и CRM Party projection.

Read/write capabilities разделяются. Adapter реализует порт, но не определяет
domain policy.

### `use-cases`

Один public handler соответствует одной application command/query. Use case:

1. принимает verified actor, typed tenant scope и versioned request;
2. выполняет live membership/grant check;
3. загружает aggregate через tenant-scoped ports;
4. вызывает pure domain decision;
5. сохраняет state, idempotency, audit и outbox в одной D1 batch transaction,
   если данные принадлежат одной D1 boundary;
6. инициирует внешние side effects только после durable state transition;
7. повтор безопасен и возвращает тот же logical result.

Use case не формирует browser UI и не вызывает concrete Cloudflare SDK напрямую.

### `adapters`

- `cloudflare`: Access claims, D1, Durable Objects, R2, Queues, Secrets;
- `windows`: CNG/DPAPI, filesystem, process tree, custom protocol, updater;
- `camouhost`: typed IPC к Python/Camoufox;
- `crm`: будущие OIDC, PostgreSQL и Party projection adapters.

Adapter переводит protocol/storage errors в стабильную application taxonomy и
не проталкивает SDK types в domain.

### `apps`

- `control-plane-worker`: `workers-rs` routing, assets binding, limits, DTO
  mapping, queue/scheduled handlers и composition root;
- `profile-bridge`: device trust, local materialization, supervisor и updater;
- `camouhost`: узкий typed browser runtime provider;
- `web`: React SPA, не владеющая business decisions.

Workers runtime не поддерживает native threads, поэтому cloud app не зависит от
Tokio multi-thread runtime, Axum или SQLx. Tokio разрешен в native Bridge.

## 5. Cloud Data Ownership

| Aggregate/data | Authoritative owner | Storage/coordination boundary |
|---|---|---|
| Tenant/Membership/Grant | Identity & Access | D1 mutation + audit/outbox |
| ClientRecord | Client Registry | D1 mutation + audit/outbox |
| Profile/Assignment | Profile Catalog | D1 mutation + audit/outbox |
| Active generation pointer | Profile Catalog | D1 compare-and-set after verification |
| Lease/session/fencing | Runtime Sessions | one Durable Object per profile |
| Snapshot payload | Profile Storage | immutable encrypted R2 object |
| CertificationRun | Certification | D1 decision + sanitized immutable R2 evidence |
| MailboxBinding | Mailbox Operations | D1 metadata; secret handle only |
| Device local state | Profile Bridge | encrypted workspace + SQLite cache/outbox |

D1 является authoritative catalog для standalone v1. Durable Object не является
вторым каталогом: он сериализует profile commands, выдает monotonic lease epoch и
держит минимальное recoverable coordination state. D1 хранит бизнес-проекцию и
последний принятый session/generation result.

## 6. Transaction Model

D1, Durable Object и R2 не образуют distributed transaction. Protocol обязан
быть crash-safe:

1. command получает idempotency key и ожидаемую aggregate version;
2. profile Durable Object сериализует writer transitions и выдает fencing token;
3. Bridge загружает новый immutable R2 object по новому generation key;
4. verifier проверяет object, manifest, digest и restore-readability;
5. D1 compare-and-set активирует pointer только для актуального fencing token;
6. outbox/queue публикует projection/event;
7. reconciler удаляет orphan objects и повторяет incomplete transitions.

Pointer на непроверенный object, mutable active R2 key и last-write-wins
запрещены. Queue delivery считается at-least-once; consumer всегда idempotent.

## 7. D1 Isolation Rules

D1 не предоставляет PostgreSQL RLS, поэтому standalone isolation строится явно:

- первая production deployment обслуживает один tenant/организацию, но много
  users с default-deny grants;
- каждый tenant-owned PK/FK/unique key включает `tenant_id`;
- repository method невозможно вызвать без `TenantScope` и `ActorContext`;
- raw unscoped D1 access разрешен только migration/reconciliation adapter;
- UI никогда не получает D1 binding или прямой storage URL;
- list/get скрывают различие между чужим и отсутствующим resource;
- cross-tenant и IDOR negative suite обязательны на каждый public endpoint;
- переход к нескольким независимым tenants требует отдельного ADR: D1-per-tenant,
  controlled sharding либо ранний перенос catalog adapter в PostgreSQL CRM.

В будущей CRM PostgreSQL adapter добавляет `FORCE ROW LEVEL SECURITY` как defense
in depth, не меняя domain/use-case contracts.

## 8. Identity И Device Trust

- Cloudflare Access выполняет workforce login через approved IdP или email OTP;
- приложение не хранит пароль пользователя и не реализует password reset;
- Worker проверяет issuer, audience, signature, expiry и subject Access JWT;
- app membership связывает Access identity с tenant и может быть немедленно
  revoked независимо от Access session;
- owner управляет invitations/memberships и resource grants;
- Bridge enrollment требует одновременно logged-in actor, single-use code и
  новую device-bound key pair;
- private device key защищается Windows CNG/DPAPI, TPM используется при наличии;
- Bridge получает short-lived app token только после proof-of-possession;
- постоянный bearer R2 credential на device запрещен.

## 9. Key Hierarchy

```text
Cloudflare secret root wrapping key (versioned)
  -> wrapped tenant KEK in D1
    -> wrapped generation DEK in generation metadata
      -> AEAD encrypted profile archive in R2
```

Root key не хранится в Git, D1, R2, logs или client bundle. Production promotion
запрещен до ADR, который фиксирует rotation, dual-read/single-write, offline
recovery escrow, restore drill, operator separation и key-loss procedure.
Workers Secrets/Secrets Store является storage primitive, но не заменяет эту
политику. Если требования потребуют HSM/external KMS, меняется только key-provider
adapter.

## 10. Compile-Time И CI Enforcement

- domain crate manifests имеют dependency allowlist;
- `worker`, D1/R2 bindings, Windows APIs, Python и browser dependencies запрещены
  в domain crates;
- Cargo metadata architecture test fail-ит forbidden edges;
- OpenAPI/protobuf compatibility проверяется в CI;
- frontend импортирует только generated public API types;
- Rust native unit/property tests проверяют domain state machines;
- Cloudflare integration tests выполняют production Worker build с D1/R2/DO/
  Queue bindings; Vitest используется только как test harness;
- Playwright проверяет SPA/API/Access projections, Windows lane проверяет Bridge;
- `cargo deny`, Clippy, formatting, secret scan, migration tests и signed artifact
  verification входят в единый quality gate.

## 11. Целевая Структура

```text
apps/
  control-plane-worker/      # Rust workers-rs/WASM composition root
  profile-bridge/            # Windows-native supervisor and updater
  camouhost/                 # Python runtime provider
crates/
  primitives/
  contracts/
  identity-access-domain/
  client-domain/
  profile-domain/
  session-domain/
  mailbox-domain/
  application-ports/
  use-cases/
  cloudflare-adapters/
  windows-adapters/
frontend/
proto/
migrations/d1/
runtime/
deploy/cloudflare/
docs/
```

## 12. Как Добавлять Функциональность

### Новый Use Case

1. Добавить versioned command/query contract.
2. Добавить domain decision или использовать существующий aggregate method.
3. Определить минимальные owned ports.
4. Реализовать orchestration, authorization, idempotency и audit.
5. Реализовать adapter и migration при необходимости.
6. Подключить handler в composition root.
7. Добавить contract, failure, replay и forbidden-access tests.

### Новый Cloud Provider Adapter

Реализовать port и пройти contract suite. Domain и public profile manifest не
меняются. Provider-specific retry/limits остаются в adapter.

### Новый Browser Runtime Или OS Lane

Добавить отдельный signed runtime bundle, capability manifest, process
supervisor и certification lane. Один artifact не объявляется cross-platform.

## 13. Запрещенные Сокращения

- domain import из adapter/app;
- D1/R2 вызов из React, Camouhost или domain;
- authorization только Cloudflare Access policy или скрытием UI-кнопки;
- direct D1 access из frontend/Bridge;
- Durable Object как параллельный бизнес-каталог;
- mutable R2 object как active profile;
- удаление browser lock files;
- snapshot live browser directory;
- email/client name в path, URL или object key;
- long-lived bearer token или bucket credential на device;
- generic remote `exec` вместо typed command;
- Cloudflare Container/Browser Run как Camoufox runtime без отдельного ADR.

## 14. Future CRM Migration

При интеграции сохраняются IDs, contracts, state machines и audit semantics.
Заменяются composition/adapters:

```text
Cloudflare Access -> CRM OIDC/identity adapter
D1 catalog        -> PostgreSQL/SQLx + FORCE RLS adapter
Queues            -> CRM job/outbox adapter при необходимости
local ClientRegistry -> CRM Party projection
```

R2 и Profile Bridge могут остаться без изменений. Прямой доступ к таблицам CRM
и дублирование domain rules в CRM запрещены.

## 15. Официальные Основания

- [Cloudflare Workers Rust support](https://developers.cloudflare.com/workers/languages/rust/)
- [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/)
- [Durable Objects storage and transactions](https://developers.cloudflare.com/durable-objects/best-practices/access-durable-objects-storage/)
- [D1 data security](https://developers.cloudflare.com/d1/reference/data-security/)
- [R2 Workers API](https://developers.cloudflare.com/r2/get-started/workers-api/)
- [Cloudflare Access identity providers](https://developers.cloudflare.com/cloudflare-one/integrations/identity-providers/)
- [Workers testing](https://developers.cloudflare.com/workers/testing/)

## 16. Порядок Чтения

1. [`../README.md`](../README.md)
2. [`../IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md)
3. этот architecture map;
4. [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md)
5. [`../PROFILE_LIFECYCLE_PLAN.md`](../PROFILE_LIFECYCLE_PLAN.md)
6. ADR нужного bounded context;
7. tests/contracts соответствующего vertical slice.

Если code и документ расходятся, gate должен падать. Изменение инварианта сначала
оформляется ADR, затем contract/migration tests и только после этого code.
