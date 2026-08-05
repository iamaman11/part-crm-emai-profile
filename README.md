# Browser Profile and Mail Runtime

Самостоятельное приложение для создания, хранения, воспроизведения и
сертификации browser profiles, безопасной проверки подключенных почтовых ящиков
и привязки профилей к карточкам клиентов.

Первая рабочая версия разрабатывается в этом репозитории независимо. В будущем
она подключается к CRM через версионированные contracts/events и заменяемые
adapters, а не переносит Camoufox или локальный browser lifecycle в процесс CRM.

## Текущий Статус

**Repository Step 0 — Executable Foundation принят.** Exact Rust workspace,
pure primitives crate и постоянный Linux/Windows/WASM quality gate подтверждены
PR #4 и merged evidence.

**Repository Step 1 — Cloudflare cold-build and binding spike выполняется в PR
#6.** Permanent Quality Gate run `31036328555` подтвердил exact
`worker 0.8.5`, D1/R2/Queue/Durable Object/Static Assets API surface и release
artifact через `worker-build 0.8.5`. Это repository cold-build evidence, а не
доказательство remote Cloudflare deployment.

Минимальный Worker skeleton существует только в Step 1 branch до merge. React UI,
Windows Profile Bridge, D1 migrations и product use cases ещё не реализованы.
Машиночитаемый статус: [`docs/status.json`](docs/status.json). Готовность
повышается только после merge и проверяемого CI/evidence.

Единственный разрешенный источник legacy-профилей:

```text
temp/browser_profiles/
```

В нем находится 22 профиля. Оригиналы запрещено запускать, автоматически
исправлять, очищать или мигрировать на месте. Все эксперименты выполняются на
изолированных копиях с новым generation ID.

## Порядок Разработки

Работа выполняется последовательными Repository Steps через GitHub branch, PR,
постоянный CI и squash merge. Нормативный порядок находится в
[`docs/DELIVERY_ROADMAP.md`](docs/DELIVERY_ROADMAP.md).

Текущая среда позволяет автономно выполнять repository code, tests, workflows,
issues, PR review fixes и merge. Внешние операции не симулируются: credential
rotation, Cloudflare account provisioning, физический Windows host, trusted code
signing и offline key escrow требуют отдельного подтверждаемого evidence.

## Основная Документация

- [Product boundary](docs/PRODUCT.md)
- [Delivery roadmap](docs/DELIVERY_ROADMAP.md)
- [Экспертный implementation plan](IMPLEMENTATION_PLAN.md)
- [Архитектурная карта](docs/ARCHITECTURE.md)
- [Архитектура standalone UI](docs/UI_ARCHITECTURE.md)
- [Целевой жизненный цикл профиля](PROFILE_LIFECYCLE_PLAN.md)
- [ADR status registry](docs/ADR_STATUS.md)
- [Threat model](docs/THREAT_MODEL.md)
- [Data classification](docs/DATA_CLASSIFICATION.md)
- [Privacy and retention governance](docs/PRIVACY_AND_RETENTION.md)
- [Test and evidence index](docs/TEST_EVIDENCE_INDEX.md)
- [Cloudflare cold-build evidence](docs/evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md)
- [Проверенные выводы исследования](docs/RESEARCH_FINDINGS.md)
- [Cloud profile smoke test](docs/CLOUD_PROFILE_SMOKE_TEST.md)
- [Текущая проверка готовности плана](docs/PLAN_READINESS_REVIEW.md)
- [Полный индекс документов](docs/README.md)

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

- Rust `1.97.1`, edition `2024`, exact toolchain;
- `worker 0.8.5`, direct `wasm-bindgen 0.2.126` и `worker-build 0.8.5` для
  Cloudflare Worker baseline;
- Tokio только в native Profile Bridge и локальных tools;
- Cloudflare Workers Static Assets для React SPA и same-origin API;
- Cloudflare Access identity отдельно от application memberships/grants;
- D1 для каталога/audit, Durable Object на профиль, R2 для encrypted immutable
  generations;
- SQLite WAL только для локального rebuildable cache/outbox Bridge;
- embedded Python/Camoufox runtime как отдельный signed bundle;
- OpenAPI для web API и protobuf для Bridge/CRM contracts.

Step 1 подтвердил reproducible repository cold build этих Cloudflare pins.
Каждое обновление SDK остаётся отдельным compatibility gate. Remote staging,
binding behavior и account recovery подтверждаются отдельным evidence и пока не
считаются выполненными.

## Ключевые Инварианты

1. Profile ID является opaque ID и никогда не равен email или имени каталога.
2. Один профиль имеет только одного writer через Durable Object lease, monotonic
   epoch/fencing token и локальный OS lock.
3. Firefox lock-файлы никогда не удаляются автоматически.
4. Snapshot создается только после graceful close и подтвержденного quiescence.
5. R2 не используется как live filesystem: generation сначала материализуется
   на локальный диск.
6. Cookies, localStorage, IndexedDB, fingerprint data и mailbox secrets являются
   credential-equivalent данными.
7. Пароли, proxy credentials и OAuth tokens хранятся только как secret handles.
8. Качество профиля утверждается только versioned certification report;
   абсолютная невидимость не обещается.
9. Assignment профиля клиенту не является правом доступа.
10. Member без явного grant не видит и не запускает профиль.
11. Каждый tenant-owned repository API требует typed tenant scope; cross-tenant и
    IDOR negative tests являются release gate.
12. D1, Durable Objects и R2 связываются idempotency, immutable objects, outbox и
    reconciliation, а не фиктивной общей транзакцией.
13. Статус ADR и readiness берётся из `ADR_STATUS.md` и `status.json`.

## Безопасность До Реализации

В legacy-скриптах и истории репозитория обнаружен hardcoded proxy credential.
Он считается скомпрометированным и должен быть отозван/ротирован владельцем с
provider-side подтверждением (issue #1). Удаление строки из Git не является
remediation. Значение секрета запрещено переносить в новую конфигурацию,
документацию, тесты, issues или логи.

До production cloud sync ADR-0006 должен стать accepted, а clean-environment
restore drill должен подтвердить root wrapping key, tenant KEK, rotation,
offline recovery escrow и key-loss policy. Обычный Workers Secret сам по себе не
считается завершенной key-management системой.
