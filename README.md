# Browser Profile and Mail Runtime

Самостоятельное приложение для создания, хранения, воспроизведения и
сертификации browser profiles, безопасной проверки подключенных почтовых ящиков
и привязки профилей к карточкам клиентов.

Первая рабочая версия разрабатывается в этом репозитории независимо. В будущем
она подключается к CRM через версионированные contracts/events и заменяемые
adapters, а не переносит Camoufox или локальный browser lifecycle в процесс CRM.

## Статус

Проект находится в исследовательской и архитектурной фазе. Production-код новой
системы еще не создан. Существующие Python-скрипты являются legacy-прототипом и
не задают целевую архитектуру.

Единственный разрешенный источник legacy-профилей:

```text
temp/browser_profiles/
```

В нем находится 22 профиля. Оригиналы запрещено запускать, автоматически
исправлять, очищать или мигрировать на месте. Все эксперименты выполняются на
изолированных копиях с новым generation ID.

## Документация

- [Проверенные выводы исследования](docs/RESEARCH_FINDINGS.md)
- [План реализации](IMPLEMENTATION_PLAN.md)
- [Архитектурная карта](docs/ARCHITECTURE.md)
- [Архитектура standalone UI](docs/UI_ARCHITECTURE.md)
- [Целевой жизненный цикл профиля](PROFILE_LIFECYCLE_PLAN.md)
- [Текущая проверка готовности плана](docs/PLAN_READINESS_REVIEW.md)
- [ADR key management и recovery](docs/adr/ADR-0006-cloud-profile-key-management.md)
- [Индекс ADR и документов](docs/README.md)

## Целевая Форма Системы

```text
app.example.com -> one Cloudflare Worker deployment
  browser routes -> Cloudflare Access -> Rust API + React Static Assets
  bridge routes  -> device-proof policy -> Rust API
  control plane  -> D1 + Durable Objects + Queues/Schedules + encrypted R2

React UI
  -> profilebridge://claim/<single-use-code>
    -> Windows-native Rust Profile Bridge
      -> local encrypted workspace + SQLite cache/outbox
      -> typed IPC -> embedded Python Camouhost
        -> separate visible Camoufox window
```

Standalone v1 не требует отдельной VM, PostgreSQL или Keycloak. Cloudflare
является control plane и хостингом, а Camoufox исполняется только на компьютере
пользователя. Browser payload никогда не исполняется непосредственно из R2.

## Базовый Стек

- Rust `1.97.1`, edition `2024`, `rust-version = "1.97.1"`;
- `workers-rs`/WebAssembly для Cloudflare Worker, D1, R2, Queues и Durable
  Objects; без Tokio/Axum/SQLx в cloud runtime;
- Tokio только в native Profile Bridge и локальных tools;
- Cloudflare Workers Static Assets для React SPA и same-origin API;
- Cloudflare Access для browser workforce identity; Bridge routes используют
  отдельную device-proof policy; app membership/grants остаются authoritative;
- D1 для каталога и audit, Durable Object на профиль для single-writer
  coordination, R2 для encrypted immutable generations;
- SQLite WAL только для локального rebuildable cache/outbox Profile Bridge;
- Python `3.12.3`, Camoufox official `0.5.4`, BrowserForge `1.2.4`,
  Playwright `1.59.0`;
- React `19.2.7`, TypeScript `7.0.2`, Vite `8.1.5`, pnpm и TanStack Query
  `5.101.2`, синхронизированные с текущим CRM baseline;
- OpenAPI для web API и protobuf для Bridge/CRM contracts.

Версии новых Cloudflare crates/packages pin-ятся только после Phase 0 cold-build
spike на Rust `1.97.1`; документация не выдает непроверенный version pin за
принятое решение.

## Ключевые Инварианты

1. Profile ID является opaque ID и никогда не равен email или имени каталога.
2. Один профиль имеет только одного writer через Durable Object lease, monotonic
   epoch/fencing token и локальный OS lock.
3. Firefox lock-файлы никогда не удаляются автоматически.
4. Snapshot создается только после graceful close и подтвержденного quiescence.
5. R2 не используется как live filesystem: generation сначала материализуется
   на локальный диск.
6. Cookies, localStorage, IndexedDB, fingerprint data и mailbox secrets являются
   secret-bearing данными.
7. Пароли, proxy credentials и OAuth tokens хранятся только как secret handles.
8. Качество профиля утверждается только versioned certification report; обещание
   абсолютной невидимости или идеального fingerprint запрещено.
9. У профиля не более одного active primary client assignment; assignment не
   является правом доступа.
10. Member без явного grant не видит и не запускает профиль.
11. D1 не имеет PostgreSQL RLS: каждый repository API требует typed tenant scope,
    а cross-tenant/IDOR negative tests являются release gate.
12. D1, Durable Objects и R2 не образуют общую транзакцию: операции используют
    idempotency, immutable objects, outbox и compensating reconciliation.

## Безопасность До Реализации

В legacy-скриптах и истории репозитория обнаружен hardcoded proxy credential.
Его необходимо отозвать до любого использования прототипа. Значение секрета
нельзя переносить в новую конфигурацию, документацию, тесты или логи.

До production cloud sync ADR-0006 должен стать accepted, а restore drill должен
подтвердить root wrapping key, tenant KEK, rotation, offline recovery escrow и
key-loss policy. Обычный Workers Secret сам по себе не считается завершенной
key-management системой.
