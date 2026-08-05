# Browser Profile and Mail Runtime

Самостоятельное приложение для создания, хранения, воспроизведения и
сертификации browser profiles, а также для безопасной проверки подключенных
почтовых ящиков.

Первая рабочая версия разрабатывается в этом репозитории независимо. В будущем
она должна подключаться к CRM как внешний operational service через
версионированный protobuf/gRPC-контракт, а не встраивать Python/Camoufox в
процесс CRM.

## Статус

Проект находится в исследовательской и архитектурной фазе. Production-код новой
системы еще не создан. Существующие Python-скрипты считаются legacy-прототипом и
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
- [Черновик плана реализации](IMPLEMENTATION_PLAN.md)
- [Архитектурная карта для разработчика](docs/ARCHITECTURE.md)
- [Архитектура standalone UI](docs/UI_ARCHITECTURE.md)
- [Целевой жизненный цикл профиля](PROFILE_LIFECYCLE_PLAN.md)
- [ADR-0001: политика стабильности fingerprint](docs/adr/ADR-0001-fingerprint-stability-policy.md)
- [ADR-0002: облачные профили и локальная материализация](docs/adr/ADR-0002-cloud-profile-materialization.md)
- [ADR-0003: desktop-приложение и поставка runtime](docs/adr/ADR-0003-desktop-runtime-distribution.md)
- [ADR-0004: доступ пользователей и владение клиентами](docs/adr/ADR-0004-tenant-access-and-client-ownership.md)
- [Отчет cloud profile smoke test](docs/CLOUD_PROFILE_SMOKE_TEST.md)
- [Индекс документации](docs/README.md)

## Целевая форма системы

```text
React web UI -> Rust API/BFF -> PostgreSQL, R2, KMS and workers
  -> one-time custom-protocol launch intent
    -> Windows-native Rust Profile Bridge
      -> local materialization, lease/sync supervisor and updater
        -> Camouhost IPC -> separate visible Camoufox window
```

Server-side Rust владеет tenant/users, client cards, ACL, каталогом профилей,
поколениями, lease, snapshot lifecycle, сертификацией и audit. Profile Bridge
владеет локальным process lifecycle, но не бизнес-каталогом. Camouhost является
заменяемым browser runtime provider и не является источником бизнес-истины.

## Базовый стек

- Rust `1.97.1`, edition `2024`, `rust-version = "1.97.1"`;
- Tokio, tonic/prost, Axum, SQLx;
- PostgreSQL с RLS как authoritative standalone catalog; SQLite WAL только для
  локального cache/outbox Profile Bridge;
- Keycloak/OIDC и BFF session для identity;
- Python `3.12.3`, Camoufox official `0.5.4`, BrowserForge `1.2.4`,
  Playwright `1.59.0`;
- React `19.2.7`, TypeScript `7.0.2`, Vite `8.1.5`, pnpm и TanStack Query
  `5.101.2`, синхронизированные с текущим CRM baseline;
- локальный filesystem для активных профилей;
- Cloudflare R2 для зашифрованных immutable generations;
- protobuf как межпроцессный и будущий CRM-контракт.

## Ключевые правила

1. Profile ID является opaque ID и никогда не равен email или имени каталога.
2. Один профиль может иметь только одного writer через lease, fencing token и
   OS lock.
3. Firefox lock-файлы никогда не удаляются автоматически.
4. Snapshot создается только после graceful close и подтвержденного quiescence.
5. R2 не используется как live filesystem. Cloud profile материализуется на
   локальный диск worker перед запуском.
6. Cookies, localStorage, IndexedDB и fingerprint snapshot считаются секретными
   данными.
7. Пароли, proxy credentials и OAuth tokens хранятся только как secret handles.
8. Утверждение о качестве профиля допускается только после versioned
   certification report.
9. Нельзя обещать абсолютную невидимость или идеальный fingerprint. Требуются
   измеримые consistency, coherence и uniqueness gates.
10. У профиля не более одного active primary client assignment; assignment не
    является правом доступа.
11. Member без явного grant не видит и не запускает профиль.

## Безопасность До Реализации

В legacy-скриптах и истории репозитория обнаружен hardcoded proxy credential.
Его необходимо отозвать и заменить до любого использования прототипа. Значение
секрета нельзя переносить в новую конфигурацию, документацию, тесты или логи.
