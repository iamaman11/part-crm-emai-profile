# Browser Profile and Mail Runtime

Самостоятельное приложение для создания, хранения, воспроизведения и
сертификации browser profiles, безопасной проверки подключенных почтовых ящиков
и привязки профилей к карточкам клиентов.

Первая рабочая версия разрабатывается в этом репозитории независимо. В будущем
она подключается к CRM через версионированные contracts/events и заменяемые
adapters, а не переносит Camoufox или локальный browser lifecycle в процесс CRM.

## Текущий Статус

**Repository Steps 0–2 приняты. Repository Step 3 выполняется в PR #12.**

- Step 0 создал exact Rust workspace, pure primitives и постоянный
  Linux/Windows/WASM quality gate.
- Step 1 добавил минимальный Rust Cloudflare Worker и доказал repository cold
  build D1/R2/Queue/Durable Object/Static Assets boundary.
- Step 2 добавил typed IDs и actor context, pure identity/client/profile/session/
  mailbox domains, application ports, initial use cases, OpenAPI/protobuf v1 roots
  и активные architecture/contract breaking-change gates.
- Step 3 добавляет strict D1 catalog migrations, tenant-inclusive constraints,
  typed Cloudflare adapter, optimistic versions, idempotency/audit/outbox envelope
  и постоянные migration/isolation negative gates.

Technical Step 3 Quality Gate run `31043260598` полностью зелёный на head
`40d84c5cf5d7832a3db964ab639e822f2e055031`: Linux/WASM, Windows, local Wrangler
D1 migration/replay и release Worker с подключённым D1 adapter. Merge acceptance
ещё не выполнен. Следующий этап после приёмки — **Repository Step 4: Identity,
clients and ACL slice** (issue #13).

Cloudflare Access identity, React UI, Windows Profile Bridge и remote D1 staging
ещё не реализованы. Машиночитаемый статус: [`docs/status.json`](docs/status.json).

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
- [Contract compatibility policy](docs/CONTRACT_POLICY.md)
- [D1 catalog boundary](docs/D1_CATALOG.md)
- [ADR status registry](docs/ADR_STATUS.md)
- [Threat model](docs/THREAT_MODEL.md)
- [Data classification](docs/DATA_CLASSIFICATION.md)
- [Privacy and retention governance](docs/PRIVACY_AND_RETENTION.md)
- [Test and evidence index](docs/TEST_EVIDENCE_INDEX.md)
- [Cloudflare cold-build evidence](docs/evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md)
- [Domain and contract evidence](docs/evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md)
- [D1 catalog evidence](docs/evidence/2026-08-05-repository-step-3-d1-catalog-foundation.md)
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
- pure Rust domains and application ports, compiled natively and for Workers WASM;
- typed Cloudflare D1 adapter with direct `serde 1.0.229` macro dependency;
- forward-only strict D1 migrations tested with Wrangler `4.94.0` and SQLite;
- Tokio только в native Profile Bridge и локальных tools;
- Cloudflare Workers Static Assets для React SPA и same-origin API;
- Cloudflare Access identity отдельно от application memberships/grants;
- D1 для каталога/audit, Durable Object на профиль, R2 для encrypted immutable
  generations;
- SQLite WAL только для локального rebuildable cache/outbox Bridge;
- embedded Python/Camoufox runtime как отдельный signed bundle;
- OpenAPI v1 для web API и protobuf v1 для Bridge/CRM contracts.

Step 1 подтвердил reproducible repository cold build Cloudflare pins. Step 2
добавил immutable v1 compatibility floor. Step 3 technical gate подтвердил local
D1 migration replay, typed adapter compilation and Worker packaging. Remote
staging, binding behavior under load, backup/restore and account recovery пока не
считаются выполненными.

## Ключевые Инварианты

1. Profile ID является opaque typed ID и никогда не равен email или имени каталога.
2. Каждый application command/query получает verified `ActorContext` и tenant scope.
3. D1 reads require typed `TenantScope`; mutations require `ActorContext`.
4. Raw D1 statements принадлежат только Cloudflare adapter boundary.
5. Tenant-owned D1 relations используют tenant-inclusive keys и foreign keys.
6. Один профиль имеет только одного writer через Durable Object lease, monotonic
   epoch/fencing token и локальный OS lock.
7. Firefox lock-файлы никогда не удаляются автоматически.
8. Snapshot создается только после graceful close и подтвержденного quiescence.
9. R2 не используется как live filesystem: generation сначала материализуется
   на локальный диск.
10. Cookies, localStorage, IndexedDB, fingerprint data и mailbox secrets являются
    credential-equivalent данными.
11. Пароли, proxy credentials и OAuth tokens хранятся только как secret handles.
12. Assignment профиля клиенту не является правом доступа.
13. Member без явного grant не видит и не запускает профиль.
14. Pure domains не зависят от Cloudflare, Windows, Python, browser или storage SDK.
15. D1, Durable Objects и R2 связываются idempotency, immutable objects, outbox и
    reconciliation, а не фиктивной общей транзакцией.
16. Статус ADR и readiness берётся из `ADR_STATUS.md` и `status.json`.

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
