# Standalone UI Architecture

**Статус:** normative product/UI target

## 1. Форма Приложения

Основной интерфейс является responsive web application. Он полностью покрывает
standalone administration и работу пользователя. Установка нужна только на
Windows-компьютере, где будет запускаться Camoufox: небольшой Profile Bridge
регистрирует custom protocol и открывает browser как отдельное окно.

React SPA и browser-facing Rust API публикуются одним Cloudflare Workers Static
Assets deployment на одном origin и защищаются Cloudflare Access. Device-bound
Bridge routes используют отдельную Worker policy. UI не хранит пароль, Access
token или R2 credentials в Web Storage.

Мобильный web UI поддерживает каталог, clients, access, history, mailbox и audit.
Кнопка запуска Camoufox доступна только при совместимом enrolled desktop device.

## 2. Information Architecture

| Route | Назначение | Доступ |
|---|---|---|
| `/` | dashboard, failures, dirty sessions, recent activity | member projection |
| `/profiles` | поиск/фильтры/статусы профилей | granted profiles |
| `/profiles/:profileId` | overview, session, generations, client, access, certification, mailbox, audit | profile grant/owner |
| `/clients` | клиентский каталог | client grants/owner |
| `/clients/:clientId` | card, contact points, profiles, activity, access | client grant/owner |
| `/mailboxes` | bindings, jobs, provider state | permitted bindings/owner |
| `/sessions` | active/stuck/dirty sessions | own sessions/owner all |
| `/certification` | runtime lanes, drift and evidence summaries | viewer/owner |
| `/users` | invitations, memberships and grants | owner only |
| `/devices` | enrolled devices, versions and revocation | own devices/owner all |
| `/audit` | security/business audit explorer | owner only |
| `/settings` | tenant policy, retention and release channel | owner only |

Маршрут не является authorization boundary. Worker всегда повторно проверяет
membership/grant; недоступный и чужой resource возвращаются одинаково.

## 3. Основные Экраны

### Profile Catalog

- client, assignee, lifecycle, cloud, certification, mailbox and runtime filters;
- явные состояния `READY`, `IN_USE`, `DIRTY_LOCAL`, `SYNCING`, `QUARANTINED`;
- primary action зависит от capability/state, но reason недоступности объясняется;
- никакой email или secret не используется в route/key.

### Profile Detail

- `Overview`: client assignment, runtime, last sync and owner-safe actions;
- `Session`: active actor/device, heartbeat, idle deadline, `Save & Close`;
- `Generations`: immutable history, verification and owner rollback workflow;
- `Access`: grants and high-impact warning для operator permission;
- `Certification`: exact runtime/policy, expiry, drift and sanitized evidence;
- `Mailbox`: binding/check status без показа secret;
- `Audit`: resource-scoped event projection.

### Client Detail

- structured card and governed contact points;
- assigned profiles without leaking ungranted profile details;
- client grants separately from profile grants;
- assignment history and future CRM `party_ref` sync state;
- archive/merge flow вместо unsafe hard delete.

### Users & Access

- invitation lifecycle and expiry;
- единственный active owner и explicit owner-transfer ceremony;
- grants grouped by user и resource;
- revoke preview показывает affected active sessions;
- confirmation требует reason, а результат отображает audit reference.

## 4. Критические UX-Потоки

### First Run

1. Owner входит через Cloudflare Access approved IdP или email OTP.
2. UI проверяет наличие Profile Bridge.
3. Guided flow скачивает signed installer и enroll-ит device.
4. Doctor показывает Bridge/runtime/network readiness без раскрытия fingerprint
   secrets.

### Create Profile

1. Выбрать или создать client.
2. Выбрать approved runtime/network policy.
3. Создать profile metadata и initial assignment.
4. Запустить one-time launch intent.
5. Пользователь проходит browser authorization.
6. После close UI показывает `DRAINING -> SYNCING -> READY` или recoverable error.

### Open Existing Profile

1. UI показывает размер download/runtime requirements до запуска.
2. Custom URI активирует Bridge.
3. UI polling/subscription получает cloud control-plane progress.
4. Отмена до browser start освобождает materialization lease безопасно.
5. После открытия UI показывает actor/device и close policy.

### Forgotten Window И Offline

- Bridge показывает native idle warning независимо от web page;
- web UI показывает countdown и owner-safe drain action;
- offline close сохраняет `DIRTY_LOCAL`, запрещает eviction и объясняет retry;
- пользователь никогда не получает ложный `Saved`, пока remote generation не
  подтвержден.

## 5. Frontend Layers

```text
frontend/src/
  app/                 # bootstrap, router, providers, app shell
  routes/              # route composition, no business rules
  features/
    profiles/
    clients/
    access/
    devices/
    sessions/
    certification/
    mailboxes/
    audit/
  entities/            # generated DTO projections and pure display helpers
  shared/
    api/                # generated client, problem-code mapping
    ui/                 # design primitives
    forms/              # validation adapters
    observability/      # correlation and safe diagnostics
```

Feature не импортирует sibling feature internals. Общая бизнес-модель не
дублируется вручную: DTO/enums генерируются из OpenAPI. Domain decisions всегда
выполняет Rust Worker/domain core.

## 6. State Management

- TanStack Query владеет remote state, invalidation and bounded retry;
- TanStack Router владеет URL, filters and navigation state;
- component/form state остается локальным;
- активная session progress приходит через Worker polling, позже SSE при
  доказанной необходимости;
- глобальный mutable business store запрещен;
- optimistic UI допускается только для обратимых display mutations, но не для
  grants, generation activation, session close или deletion.

## 7. Design И Accessibility

- визуальный язык наследует будущий CRM shell, но standalone build остается
  полноценным;
- статус не кодируется только цветом;
- keyboard navigation, focus restoration, labels and live regions обязательны;
- destructive/high-impact actions имеют consequence preview и typed reason;
- responsive tables переходят в cards без потери critical status/action;
- PII скрывается в notifications, screenshots, telemetry and support bundles;
- loading skeleton не маскирует authorization or sync failure.

## 8. UI Test Gates

- component tests для state/error/permission projections;
- MSW/contract fixtures генерируются из API schema;
- Playwright E2E для owner/member and revoked access;
- accessibility scan плюс keyboard-only critical flows;
- responsive desktop/mobile smoke;
- custom URI flow с fake Bridge adapter в CI и Windows-native acceptance lane;
- forbidden-action test доказывает Worker rejection даже при вручную вызванном
  endpoint;
- no-secret/no-PII snapshot and telemetry scan.

## 9. Standalone UI Definition Of Done

UI готов, когда без CLI можно выполнить login, invitation, client CRUD,
profile creation/assignment/grant, Bridge onboarding, open/close/sync, generation
history, certification review, mailbox check, device revoke, audit lookup and
recoverable failure handling. Пустые, loading, offline, forbidden, conflict,
quarantine и partial-success states проектируются одновременно с happy path.
