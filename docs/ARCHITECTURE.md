# Architecture Map

**Статус:** normative target architecture

**Для кого:** разработчик, reviewer, operator и будущая CRM integration team

## 1. Системная Граница

Browser Profile Platform является самостоятельным продуктом и будущим внешним
CRM bounded context. Он владеет профилями, поколениями, browser sessions,
назначениями профилей клиентам, profile ACL, certification и cloud lifecycle.

До CRM integration локальный Client Registry владеет минимальной client card.
После integration CRM Party/Customer Master становится authoritative owner, а
платформа хранит только `party_ref` и read projection. Ни Python, ни Camoufox, ни
R2 не являются источником бизнес-истины.

## 2. Runtime Topology

```text
Web browser
  -> HTTPS Rust API/BFF
    -> PostgreSQL authoritative catalog
    -> outbox/job worker
    -> Keycloak OIDC
    -> KMS/secret provider
    -> Cloudflare R2

Web browser
  -> profilebridge://claim/<single-use-code>
    -> Windows Profile Bridge
      -> HTTPS API/device trust
      -> local SQLite cache/outbox
      -> local encrypted staging/materialization
      -> typed local IPC
        -> Camouhost Python process
          -> separate visible Camoufox process
```

API и worker могут сначала жить на одном VM и в одном repository, но являются
разными process entry points. Profile Bridge исполняется на компьютере
пользователя. Web UI не обращается к localhost Bridge API.

## 3. Слои И Разрешенные Зависимости

```text
apps
  -> use-cases
    -> application-ports + domains + contracts + primitives

infrastructure
  -> application-ports + contracts + primitives

domains
  -> primitives
```

### `primitives`

Содержит opaque IDs, safe path segments, digests, time/value types и общие
validation primitives. Не содержит SQL, HTTP, authorization policy или
browser-specific behavior.

### Domain Crates

- `identity-access-domain`: tenant owner, membership, grants и authorization
  decisions;
- `client-domain`: client card, contact points и assignment rules;
- `profile-domain`: profile, generation, snapshot и fingerprint policies;
- `session-domain`: launch intent, lease, fencing, session and recovery states;
- `mailbox-domain`: provider-neutral mailbox binding/check rules.

Domain принимает value objects и возвращает decision/events. Domain не знает об
Axum, SQLx, R2, Keycloak, Windows, Python или wall-clock singleton.

### `application-ports`

Интерфейсы принадлежат use case, который их потребляет. Минимальные группы:

- catalog repositories and unit of work;
- live authorization and identity context;
- lease/clock/idempotency/audit/outbox;
- object store, envelope encryption and secret manager;
- local workspace and browser runtime;
- certification checker and artifact store;
- mailbox provider and CRM Party projection.

Read и write capabilities разделяются, если adapter не должен получать лишние
права. Infrastructure crate реализует ports, но никогда не определяет domain
policy.

### `use-cases`

Один public handler соответствует одной application command/query. Use case:

1. принимает actor/tenant context и versioned request;
2. выполняет live authorization;
3. начинает unit of work и устанавливает RLS context;
4. загружает aggregates;
5. вызывает domain decisions;
6. сохраняет state, idempotency, audit и outbox атомарно;
7. после commit разрешает внешние asynchronous side effects.

Use case не формирует HTTP response и не вызывает concrete SDK напрямую.

### `infrastructure`

Содержит PostgreSQL/SQLx, R2/S3, KMS, Keycloak, Windows IPC, filesystem и
Camouhost adapters. Adapter переводит protocol/storage errors в стабильную
application taxonomy и не проталкивает SDK types внутрь domain.

### `apps`

- `api`: Axum routing, BFF session, request limits, DTO mapping and composition;
- `worker`: leased jobs, outbox delivery, retention and reconciliation;
- `profile-bridge`: device trust, local materialization, supervisor and updater;
- `camouhost`: узкий typed browser runtime provider.

Apps содержат composition root, но не бизнес-правила.

## 4. Compile-Time Enforcement

- каждый domain crate имеет минимальный dependency allowlist;
- SQLx/Axum/AWS/Python dependencies запрещены в domain manifests;
- architecture test анализирует Cargo metadata и fail-ит forbidden edges;
- protobuf/OpenAPI compatibility проверяется в CI;
- frontend импортирует только generated public API types, не Rust persistence
  shapes;
- `cargo deny`, Clippy, formatting, test, secret scan и migration checks входят в
  единый quality gate.

## 5. Data Ownership И Transactions

| Aggregate | Authoritative owner | Transaction boundary |
|---|---|---|
| Tenant/Membership/Grant | Identity & Access | membership/grant + audit + outbox |
| ClientRecord | Client Registry | client/contact point + audit + outbox |
| Profile/Assignment | Profile Catalog | profile/assignment + audit + outbox |
| Generation pointer | Profile Catalog | verified generation activation + audit/outbox |
| Lease/Session | Runtime Sessions | lease epoch/session transition + audit |
| Snapshot object | Profile Storage | immutable object first, catalog pointer second |
| CertificationRun | Certification | immutable evidence reference + decision |
| MailboxBinding | Mailbox Operations | binding/job cursor + audit/outbox |

R2 upload не участвует в PostgreSQL transaction. Сначала создается immutable
object, затем он проверяется, после чего catalog pointer меняется транзакционно.
Orphan object безопасен и удаляется reconciler; pointer на непроверенный object
запрещен.

## 6. PostgreSQL Rules

- каждый tenant-owned PK/FK/unique key включает `tenant_id`;
- runtime role не является DB owner, superuser или `BYPASSRLS`;
- `SET LOCAL app.tenant_id` выполняется внутри transaction перед tenant query;
- connection запрещено возвращать в pool с session-level tenant setting;
- `FORCE ROW LEVEL SECURITY` и cross-tenant acceptance tests обязательны;
- active assignment обеспечивается partial unique index;
- lease epoch и aggregate version меняются compare-and-swap запросами;
- migrations forward-only, backup/restore и upgrade rollback проверяются до
  release.

## 7. Contracts И Errors

- opaque IDs не несут email, tenant display name или sequential business number;
- публичные commands/events versioned и additive по умолчанию;
- HTTP использует generated OpenAPI DTO, service/IPC boundaries используют
  protobuf;
- mutation требует `Idempotency-Key` и expected aggregate version;
- error содержит stable code, safe message, correlation ID и retry class;
- raw SQL/SDK/browser errors, cookies, PII и credentials наружу не выдаются;
- outbox event содержит references и минимальные metadata, но не profile payload.

## 8. Profile Bridge Boundary

Bridge не является локальным backend CRM. Он может:

- enroll/revoke device key;
- redeem actor/device-bound launch intent;
- получить lease, temporary object access и wrapped DEK;
- materialize/verify profile и управлять browser process tree;
- отправлять heartbeat, snapshot result и local retry state;
- обновлять signed runtime side-by-side.

Bridge не может сам выдавать grant, менять client assignment, активировать
непроверенный generation или объявлять certification. Local SQLite можно удалить
и перестроить из server/cloud state, кроме явно удерживаемого dirty workspace.

## 9. Как Добавлять Функциональность

### Новый Use Case

1. Добавить command/query contract.
2. Добавить domain decision или использовать существующий aggregate method.
3. Определить минимальные owned ports.
4. Реализовать orchestration и transaction test.
5. Добавить infrastructure adapters.
6. Подключить handler в composition root.
7. Добавить authorization, idempotency, audit и E2E acceptance.

### Новый Mail Provider

Реализовать `MailboxProviderPort`, provider capability manifest и contract test
suite. Provider-specific OAuth/IMAP errors не входят в mailbox domain. Browser
fallback является отдельным adapter и не выбирается самим provider code.

### Новый Object Store Или KMS

Реализовать соответствующий port и пройти corruption, retry, key rotation,
recovery and least-privilege suite. Domain/profile manifests не меняются.

### Новый Fingerprint Checker

Реализовать checker adapter, sanitizer и evidence schema. Внешний score не
становится domain truth напрямую: promotion принимает versioned policy.

### Новая OS Runtime Lane

Добавить отдельный signed runtime bundle, capability manifest, packaging,
process supervisor и certification lane. Нельзя объявить один artifact
cross-platform runtime.

## 10. Запрещенные Сокращения

- domain import из infrastructure/app;
- SQL или R2 вызов из React/Camouhost/domain;
- authorization только через скрытие UI-кнопки;
- mutable R2 object как active profile;
- last-write-wins для generation;
- удаление browser lock files приложением;
- snapshot live browser directory;
- email/client name в path, URL или object key;
- generic remote `exec` вместо typed command;
- второй параллельный catalog или profile lifecycle.

## 11. Порядок Чтения Для Разработчика

1. [`../README.md`](../README.md)
2. [`../IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md)
3. этот architecture map;
4. [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md)
5. [`../PROFILE_LIFECYCLE_PLAN.md`](../PROFILE_LIFECYCLE_PLAN.md)
6. ADR нужного bounded context;
7. tests/contracts соответствующего vertical slice.

Если code и документ расходятся, gate должен падать. Изменение инварианта сначала
оформляется ADR, затем contract/migration tests и только после этого code.
