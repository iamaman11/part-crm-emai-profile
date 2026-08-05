# Belarus–Poland Research Platform — Master Implementation Plan

**Статус:** proposed  
**Класс документа:** исполнимый архитектурный план и контракт качества  
**Целевая платформа:** Rust-first, offline-first, reproducible research system  
**Область:** исследование нестабильного веб-сайта, его поведения и надёжности применяемых инструментов  
**Источники истины:** типизированные записи, неизменяемые события и хэшированные доказательства  

---

## 1. Цель

Преобразовать текущий набор исследовательских скриптов, Markdown-заметок, сетевых
дампов и скриншотов в профессиональную научно строгую платформу, которая:

1. воспроизводит каждый запуск и объясняет его контекст;
2. отделяет сбои сайта от сбоев инструмента, среды и входных данных;
3. рассматривает инструменты как версионируемые и калибруемые измерительные приборы;
4. сохраняет полную цепочку происхождения данных — provenance;
5. не смешивает наблюдения, классификации, гипотезы и подтверждённые знания;
6. обеспечивает машинную проверяемость и понятность для человека;
7. масштабируется от одной WSL-машины до нескольких workers без смены доменной модели;
8. защищает секреты и персональные данные на всём жизненном цикле;
9. позволяет повторно анализировать старые запуски новыми классификаторами;
10. не требует скрытых ручных действий для сборки, запуска или проверки.

## 2. Что означает уровень 10/10

Система считается завершённой не по количеству компонентов, а по выполнению
проверяемых свойств.

| Свойство | Проверяемый критерий |
|---|---|
| Прозрачность | Любой итог объясняется ссылками на события, артефакты и версию правила |
| Архитектурная чистота | Доменное ядро не зависит от web, SQL, Playwright и конкретного storage |
| Воспроизводимость | Запуск содержит commit, lock hashes, toolchain, config, seed и environment |
| Модульность | Instrument, storage, catalog и classifier заменяются через стабильные интерфейсы |
| Повторяемость | Одинаковый fixture + seed + tool bundle даёт одинаковую классификацию |
| Научная строгость | Наблюдение, интерпретация и claim являются разными сущностями |
| Целостность | Все evidence имеют SHA-256; sealed manifest подписан и проверяем |
| Безопасность | В Git, логах, trace и отчётах отсутствуют открытые секреты и raw PII |
| Отказоустойчивость | Прерванный run восстанавливается или корректно финализируется |
| Анализируемость | События экспортируются в Parquet и доступны через SQL |
| Эволюция | Schema и protocol versioned; миграции тестируются на legacy corpus |
| Простота | Базовый локальный режим запускается одной командой без Kubernetes |

Ни один пункт не может быть заменён субъективной оценкой «работает у меня».

## 3. Архитектурные инварианты

Эти правила обязательны для любого кода и изменения архитектуры.

1. **Rust владеет истиной.** Run lifecycle, ledger, manifests, artifacts,
   classification, knowledge и reports реализуются в Rust.
2. **Browser worker является прибором.** Python/Camoufox/Playwright исполняет
   типизированные команды, но не изменяет knowledge base и не объявляет научный итог.
3. **Никакого удалённого `exec`.** Межпроцессный API состоит только из заранее
   определённых protobuf-команд.
4. **События неизменяемы.** Исправление создаёт новое событие; старое не удаляется.
5. **Raw evidence неизменяемы.** Производные представления имеют собственный hash и
   ссылку `derived_from`.
6. **Markdown не является базой данных.** Markdown и HTML генерируются из
   типизированных записей.
7. **Классификация детерминирована.** Одинаковые evidence и classifier version дают
   одинаковый результат.
8. **Неопределённость допустима.** `INCONCLUSIVE` предпочтительнее недоказанного
   причинного вывода.
9. **Наблюдение не равно claim.** Ответ API — observation; объяснение причины —
   claim с evidence и альтернативами.
10. **PII отделена от исследования.** В обычном run-store используются subject
    aliases на базе keyed HMAC.
11. **Секреты не проходят через CLI arguments.** Только environment, inherited file
    descriptor либо краткоживущий файл `0600`.
12. **Пути вычисляются от project root.** Абсолютные пользовательские пути запрещены.
13. **Все часы — UTC.** Для длительностей используется monotonic clock.
14. **Каждый run имеет ULID.** Человеческие номера экспериментов не используются как
    уникальный технический идентификатор.
15. **Локальный режим — основной.** Распределённость является адаптером, а не
    обязательным условием работы.

## 4. Контекст и границы

### 4.1 В scope

- планирование экспериментов;
- preflight и calibration инструментов;
- управление запуском;
- сбор DOM, trace, HAR, console, network metadata и скриншотов;
- структурированные события шагов;
- content-addressed artifact storage;
- классификация причин;
- сравнение запусков и версий инструментов;
- hypotheses, claims, recipes и incidents;
- legacy import;
- CLI, локальный web UI, отчёты и аналитика;
- backup, restore, integrity verification и retention.

### 4.2 Вне scope первой версии

- Kubernetes;
- Kafka;
- Temporal;
- Elasticsearch;
- Neo4j;
- отдельная vector database;
- автоматические LLM-выводы без детерминированного evidence layer;
- перенос Camoufox/Playwright на неофициальный Rust binding;
- хранение открытых паролей или токенов ради «полноты исследования».

## 5. Логическая архитектура

```text
                      ┌─────────────────────┐
                      │ labctl / local UI   │
                      └──────────┬──────────┘
                                 │ typed API
                      ┌──────────▼──────────┐
                      │ labd — Rust core    │
                      │ planner / runner    │
                      │ ledger / classifier│
                      └───┬────────┬─────┬──┘
                          │        │     │
                       gRPC/UDS  SQLx  OpenDAL
                          │        │     │
                ┌─────────▼──┐ ┌──▼──┐ ┌▼──────────────┐
                │ Browser    │ │SQLite│ │ Artifact CAS  │
                │ instrument │ │ledger│ │ trace/HAR/DOM │
                └──────┬─────┘ └──┬──┘ └──────┬────────┘
                       │            │           │
                   target site      └────┬──────┘
                                        │
                                  Arrow / Parquet
                                        │
                                   DataFusion SQL
```

## 6. Технологический стек

### 6.1 Rust core

| Область | Компоненты |
|---|---|
| Toolchain | pinned stable Rust, edition 2024, `Cargo.lock` |
| Async | `tokio` |
| HTTP | `axum`, `tower`, `tower-http` |
| IPC | `tonic`, `prost`, Protocol Buffers, Unix Domain Socket |
| CLI | `clap` |
| Domain serialization | `serde`, `serde_json`, `toml` |
| Schema generation | `schemars`, `jsonschema` |
| Persistence | `sqlx`, SQLite WAL; PostgreSQL adapter later |
| Artifact access | Apache OpenDAL |
| Analytics | Apache Arrow, Parquet, Apache DataFusion |
| Telemetry | `tracing`, `tracing-subscriber`, OpenTelemetry OTLP |
| Errors | `thiserror` in libraries, `anyhow` at binary boundary |
| Identifiers | ULID |
| Integrity | SHA-256 plus BLAKE3 internal fast checks |
| Compression | Zstandard |
| Secrets in memory | `secrecy`, `zeroize` |
| UI | Axum + Askama/HTMX; JavaScript only for charts |

### 6.2 Browser instrument

| Область | Компоненты |
|---|---|
| Runtime | Python 3.12, pinned through `.python-version` |
| Environment | `uv`, `pyproject.toml`, committed `uv.lock` |
| Browser | Camoufox + supported Playwright Python |
| Contract | protobuf stubs generated from `proto/instrument/v1` |
| Evidence | Playwright trace, HAR, DOM snapshots, console, screenshots |
| Process model | один ephemeral worker на run |
| Isolation | UDS, temporary workspace, least privilege, deadlines |

### 6.3 Development and supply chain

- `cargo fmt`;
- `cargo clippy -D warnings`;
- `cargo nextest`;
- `cargo llvm-cov`;
- `cargo audit`;
- `cargo deny`;
- `cargo machete`;
- `proptest`;
- `insta`;
- `gitleaks`;
- `buf lint` и `buf breaking`;
- SBOM для release artifacts;
- signed release artifacts;
- pinned GitHub Actions по commit SHA;
- dependency update PR с обязательным test gate.

## 7. Целевая структура репозитория

```text
.
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── deny.toml
├── IMPLEMENTATION_PLAN.md
├── README.md
├── SECURITY.md
├── CONTRIBUTING.md
├── docs/
│   ├── architecture/
│   ├── adr/
│   ├── operations/
│   └── scientific-method/
├── crates/
│   ├── lab-domain/
│   ├── lab-protocol/
│   ├── lab-ledger/
│   ├── lab-runner/
│   ├── lab-artifacts/
│   ├── lab-instruments/
│   ├── lab-classifier/
│   ├── lab-redaction/
│   ├── lab-analytics/
│   ├── lab-knowledge/
│   ├── lab-report/
│   └── lab-testkit/
├── apps/
│   ├── labd/
│   ├── labctl/
│   └── lab-web/
├── workers/
│   └── browser-python/
├── proto/
│   └── instrument/v1/
├── schemas/
├── migrations/
├── configs/
│   ├── sites/
│   ├── scenarios/
│   ├── instruments/
│   └── retention/
├── knowledge/
│   ├── experiments/
│   ├── hypotheses/
│   ├── claims/
│   ├── recipes/
│   ├── incidents/
│   ├── legacy/
│   └── generated/
├── fixtures/
├── tests/
│   ├── contract/
│   ├── integration/
│   ├── golden/
│   └── recovery/
├── runs/                 # small redacted metadata only
├── objects/              # ignored local CAS, backed up separately
└── warehouse/            # generated Parquet, reproducible
```

## 8. Доменные сущности

### 8.1 Experiment

Определяет исследовательский вопрос до запуска:

- `experiment_id`;
- objective;
- hypothesis references;
- controlled variables;
- independent variables;
- expected controls;
- repetition policy;
- stop conditions;
- ethics/safety constraints;
- scenario version.

### 8.2 Run

Один фактический запуск:

- `run_id` — ULID;
- `experiment_id`;
- optional `parent_run_id` и `control_run_id`;
- lifecycle state;
- start/end UTC;
- monotonic duration;
- environment snapshot;
- scenario hash;
- instrument bundle;
- random seed;
- evidence index;
- classifier version;
- result.

### 8.3 Instrument

Версионируемый измерительный прибор:

- immutable ID/version;
- source commit/tree hash;
- dependency lock hash;
- capabilities;
- config hash;
- protocol version;
- known limitations;
- calibration status;
- compatibility matrix.

### 8.4 Observation

Событие, полученное напрямую:

- HTTP status;
- redacted response;
- DOM state;
- selector state;
- browser error;
- timeout;
- screenshot;
- environment measurement.

Observation не содержит причинного вывода.

### 8.5 Classification

Детерминированная интерпретация observations:

- result class;
- classifier ID/version;
- evidence references;
- rule trace;
- contradicting evidence;
- confidence rubric;
- alternatives;
- manual review status.

### 8.6 Claim

Версионируемое знание:

- subject/predicate/value;
- evidence runs;
- contradicting runs;
- valid interval;
- status: `proposed`, `supported`, `contradicted`, `obsolete`;
- review history;
- supersedes relation.

## 9. Состояния запуска

```text
PLANNED
  → PREFLIGHT_RUNNING
  → PREFLIGHT_PASSED
  → INSTRUMENT_STARTING
  → RUNNING
  → EVIDENCE_SEALING
  → EVIDENCE_VALIDATED
  → CLASSIFIED
  → REVIEW_PENDING
  → FINALIZED
```

Терминальные альтернативы:

```text
CANCELLED
EXPERIMENT_INVALID
INTERRUPTED_RECOVERABLE
INTERRUPTED_FINAL
```

Каждый переход:

- проверяет expected current state;
- выполняется транзакционно;
- имеет idempotency key;
- создаёт immutable event;
- содержит actor, timestamp, reason и correlation ID.

## 10. Таксономия результатов

```text
SUCCESS_CONFIRMED
SITE_FAILURE
├── SITE_API_FAILURE
├── SITE_UI_DRIFT
├── SITE_TIMEOUT
└── SITE_INCONSISTENT_STATE
INSTRUMENT_FAILURE
├── SELECTOR_FAILURE
├── BROWSER_CRASH
├── TRACE_CAPTURE_FAILURE
└── WORKER_PROTOCOL_FAILURE
ENVIRONMENT_FAILURE
├── PROXY_FAILURE
├── DNS_FAILURE
├── NETWORK_IDENTITY_FAILURE
└── RESOURCE_EXHAUSTION
INPUT_FAILURE
EXPERIMENT_INVALID
INCONCLUSIVE
```

Строковый поиск по console output не может быть источником классификации.

## 11. Evidence и provenance

Минимальный evidence bundle:

```text
manifest.json
events.jsonl
outcome.json
artifacts.json
environment.json
instrument.json
trace.zip
network.har.zip
console.jsonl
dom/
screenshots/
```

### 11.1 Manifest

Обязательные поля:

- schema version;
- run/experiment/control IDs;
- timestamps и duration;
- Git commit и dirty flag;
- sanitized patch SHA-256;
- Cargo.lock и uv.lock hashes;
- Rust/Python/OS/browser versions;
- scenario/config hashes;
- random seed;
- instrument bundle;
- artifact hashes;
- classifier version;
- sealing signature.

### 11.2 Artifact rules

1. Artifact адресуется по SHA-256 содержимого.
2. Логическое имя не является identity.
3. Запрещено изменять объект после ingest.
4. Derived artifact хранит `derived_from`.
5. Перед ingest выполняется redaction policy.
6. Не прошедший validation объект помещается в quarantine.
7. Integrity check запускается регулярно и перед restore.

## 12. Хранение

### 12.1 Operational ledger

Первая production-версия использует SQLite:

- WAL mode;
- foreign keys;
- explicit migrations;
- append-only `run_events`;
- projections в отдельных таблицах;
- online backup API;
- проверка `integrity_check`;
- single-writer discipline через `labd`.

PostgreSQL реализуется позднее через storage port, только если появляются
несколько одновременно пишущих nodes.

### 12.2 Artifact store

OpenDAL adapter:

1. local filesystem;
2. encrypted backup;
3. optional S3-compatible backend.

Локальная работа не зависит от доступности облака.

### 12.3 Analytics

- normalized events экспортируются в Parquet;
- partitioning: `site/year/month/day`;
- DataFusion предоставляет SQL и prepared reports;
- DuckDB может использоваться внешним исследователем;
- warehouse является производным и полностью пересобирается.

## 13. Безопасность и приватность

### Gate 0 — до публикации текущего рабочего дерева

- [ ] Ротировать все найденные proxy/API/account secrets.
- [ ] Проверить Git history на секреты.
- [ ] Согласовать безопасную очистку истории, если секреты уже опубликованы.
- [ ] Ввести `.env.example` только с названиями переменных.
- [ ] Подключить `gitleaks`.
- [ ] Удалить открытые пароли из knowledge и launcher scripts.
- [ ] Псевдонимизировать email/телефоны.
- [ ] Проверить trace/HAR/HTML на PII и cookies.
- [ ] Запретить push, если secret scan не пройден.

### Постоянные меры

- SOPS + age для repository-managed secrets;
- OS secret store для runtime credentials;
- `secrecy` и `zeroize` в Rust;
- role-based access в UI;
- raw evidence закрыта по умолчанию;
- audit trail просмотра restricted artifacts;
- configurable retention;
- encrypted backup;
- documented incident response.

## 14. Browser instrument contract

Запрещается переносить текущий произвольный `/execute` API.

Обязательные методы:

```text
GetCapabilities
HealthCheck
StartRun
ExecuteStep
CaptureSnapshot
CancelRun
FinalizeRun
```

Каждый request содержит:

- protocol version;
- run ID;
- command ID;
- idempotency key;
- deadline;
- expected state.

Worker:

- стартует на один run;
- не принимает сетевые подключения извне;
- работает через UDS;
- имеет временную staging directory;
- пишет stdout/stderr только как operational telemetry;
- не может изменять ledger напрямую;
- завершается и очищается supervisor-ом.

## 15. Calibration

Run не может быть научно валидным без успешной calibration совместимого
instrument bundle.

Calibration проверяет:

- старт/останов браузера;
- proxy reachability и network identity;
- trace/HAR capture;
- DOM и screenshot capture;
- fixture selectors;
- protocol cancellation и deadlines;
- worker crash recovery;
- redaction;
- artifact hashing;
- declared capabilities;
- resource ceilings;
- clock sanity.

Результат calibration — такой же signed run, а не строка в README.

## 16. CLI

```text
labctl doctor
labctl init
labctl migrate
labctl experiment validate <file>
labctl instrument list
labctl instrument inspect <id>
labctl calibrate <instrument>
labctl run <experiment>
labctl run resume <run-id>
labctl run cancel <run-id>
labctl inspect <run-id>
labctl compare <run-id> <run-id>
labctl classify <run-id> --classifier <version>
labctl verify <run-id>
labctl claim propose <file>
labctl claim review <claim-id>
labctl report <run-id>
labctl warehouse rebuild
labctl backup
labctl restore --verify
```

Для каждой destructive-команды обязателен dry-run либо явное подтверждение.

## 17. Тестовая стратегия

| Уровень | Что проверяет |
|---|---|
| Unit | доменные правила и state transitions |
| Property | сериализацию, state machine и classifier invariants |
| Snapshot | schemas, reports, manifests и diagnostics |
| Contract | Rust↔Python protobuf compatibility |
| Migration | каждую DB/schema migration на legacy corpus |
| Golden | фиксированные evidence → фиксированный classification |
| Integration | ledger + CAS + worker + report |
| Recovery | kill/restart на каждом lifecycle state |
| Fault injection | disk full, timeout, corrupt artifact, worker crash |
| Security | secret/PII leakage, traversal, malformed protobuf |
| Performance | event ingest, artifact hashing, report generation |
| Reproducibility | повтор fixture-run в чистом окружении |

Критичные доменные ветки требуют mutation testing либо эквивалентной проверки
силы тестов.

## 18. CI quality gate

Pull request не может быть принят без:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo llvm-cov
cargo audit
cargo deny check
cargo machete
sqlx prepare --check
buf lint
buf breaking
uv lock --check
gitleaks detect
schema snapshot check
legacy import test
reproducibility smoke test
```

Release дополнительно требует:

- SBOM;
- provenance attestation;
- signed binaries/images;
- restore test;
- versioned migration notes;
- compatibility matrix;
- changelog.

## 19. Git и repository policy

Текущий remote:

```text
origin = https://github.com/iamaman11/site-analize-by-pl.git
visibility = private
```

Политика:

- `main` protected;
- изменения через pull request;
- required status checks;
- signed commits/tags желательно, signed release обязательно;
- squash merge;
- CODEOWNERS для schema, security и migrations;
- Dependabot/Renovate только через PR;
- secrets и raw artifacts никогда не push;
- generated warehouse не хранится в Git;
- redacted small manifests допускаются;
- большие artifacts находятся в CAS/object storage.

До выполнения Gate 0 запрещено публиковать текущее незакоммиченное рабочее дерево.

## 20. Миграция legacy без потери истории

1. Создать immutable snapshot текущего состояния.
2. Рассчитать SHA-256 всех legacy-файлов.
3. Не изменять оригинальные experiments и scripts при импорте.
4. Создать `legacy_import.json` с source path, hash и import warnings.
5. Назначить legacy runs отдельные ULID.
6. Отметить неизвестные параметры как `unknown`, не реконструировать их догадками.
7. Отделить observed response от исторического narrative conclusion.
8. Запустить PII/secret scanner.
9. Restricted материалы зашифровать или исключить из общего store.
10. Сгенерировать новые reports рядом, но не поверх исходных документов.

## 21. Этапы реализации

### Phase 0 — Containment и baseline

**Результат:** безопасная точка старта.

- [ ] Backup рабочего дерева и `.git`.
- [ ] Inventory и checksums.
- [ ] Secret rotation.
- [ ] PII classification.
- [ ] Baseline architecture decision records.
- [ ] Запрет небезопасного push.

**Acceptance:**

- backup восстановлен на временном пути;
- secret scan проходит;
- ни один существующий пользовательский файл не потерян;
- remote visibility подтверждена как private.

### Phase 1 — Rust foundation

**Результат:** компилируемый workspace и чистая domain model.

- [ ] Workspace и pinned toolchain.
- [ ] `lab-domain`, `lab-protocol`, `lab-testkit`.
- [ ] ULID, UTC time, typed states и errors.
- [ ] Rust → JSON Schema generation.
- [ ] ADR-001: modular monolith.
- [ ] ADR-002: event ledger.
- [ ] ADR-003: Python instrument boundary.

**Acceptance:**

- clean checkout собирается одной командой;
- domain crate не зависит от infrastructure crates;
- schema snapshots стабильны;
- invalid state transitions не компилируются либо отклоняются тестами.

### Phase 2 — Ledger и manifests

**Результат:** durable lifecycle без browser worker.

- [ ] SQLite migrations.
- [ ] Append-only run events.
- [ ] Projections.
- [ ] Idempotent commands.
- [ ] Crash recovery.
- [ ] Manifest builder.
- [ ] Signed sealing.

**Acceptance:**

- kill/restart не теряет принятые события;
- projection полностью пересобирается;
- tampered manifest не проходит verify;
- concurrent command conflict определяется явно.

### Phase 3 — Artifact CAS и redaction

**Результат:** безопасное неизменяемое evidence storage.

- [ ] OpenDAL local adapter.
- [ ] SHA-256 addressing.
- [ ] Zstd.
- [ ] Staging/quarantine/ingest.
- [ ] PII and secret redaction.
- [ ] Backup/restore.

**Acceptance:**

- повторный ingest дедуплицируется;
- повреждение обнаруживается;
- restore подтверждает все hashes;
- restricted content не попадает в public report.

### Phase 4 — Typed browser worker

**Результат:** замена произвольного control server.

- [ ] Protobuf API.
- [ ] Python worker через `uv`.
- [ ] UDS transport.
- [ ] Supervisor.
- [ ] Deadlines/cancel.
- [ ] trace/HAR/DOM/screenshot capture.
- [ ] instrument descriptor.

**Acceptance:**

- arbitrary code execution отсутствует;
- worker crash корректно классифицируется;
- все artifacts принадлежат run;
- version mismatch отклоняется до запуска.

### Phase 5 — Instrument calibration

**Результат:** измеримая надёжность инструментов.

- [ ] Fixture site/pages.
- [ ] Capability tests.
- [ ] Network and proxy diagnostics.
- [ ] Golden trace.
- [ ] Resource limits.
- [ ] Compatibility matrix.

**Acceptance:**

- run без calibration получает `EXPERIMENT_INVALID`;
- calibration воспроизводима;
- degradation инструмента видна отдельно от target site.

### Phase 6 — Scientific classifier

**Результат:** доказательная причинная классификация.

- [ ] Typed observation extractors.
- [ ] Rule engine.
- [ ] Rule trace.
- [ ] Contradiction handling.
- [ ] `INCONCLUSIVE`.
- [ ] Versioned classifier.
- [ ] Reclassification старых runs.

**Acceptance:**

- classifier не читает произвольный текст console log как истину;
- golden corpus детерминирован;
- изменение classifier не изменяет raw run;
- любой outcome объясняется evidence graph.

### Phase 7 — Knowledge system

**Результат:** hypotheses, claims, recipes и incidents.

- [ ] Typed knowledge records.
- [ ] Evidence links.
- [ ] Validity intervals.
- [ ] Review workflow.
- [ ] Generated Markdown/HTML.
- [ ] Claim supersession.

**Acceptance:**

- claim без evidence не получает `supported`;
- противоречащие runs видны;
- Markdown полностью перегенерируется;
- ручная правка generated output обнаруживается.

### Phase 8 — Legacy import

**Результат:** сохранена и нормализована текущая история.

- [ ] Import scripts.
- [ ] Hash inventory.
- [ ] Experiments 001–015.
- [ ] Diagnostics JSON.
- [ ] Screenshots/HTML.
- [ ] Launcher provenance.
- [ ] Warnings report.

**Acceptance:**

- originals не изменены;
- все импортированные записи имеют source hash;
- неизвестные параметры остались `unknown`;
- PII не утекла в общий каталог.

### Phase 9 — Analytics и comparison

**Результат:** SQL-анализ запусков и инструментов.

- [ ] Arrow event model.
- [ ] Parquet export.
- [ ] DataFusion queries.
- [ ] Site drift report.
- [ ] Tool drift report.
- [ ] Control-run comparison.

**Acceptance:**

- warehouse пересобирается с нуля;
- отчёты показывают confidence и sample size;
- site/tool/environment failures разделены;
- query results имеют lineage до runs.

### Phase 10 — UX и operations

**Результат:** понятная ежедневная работа.

- [ ] `labctl`.
- [ ] Local read-only-first web UI.
- [ ] Run timeline.
- [ ] Evidence viewer.
- [ ] Diff viewer.
- [ ] Backup/restore UI status.
- [ ] Operations handbook.

**Acceptance:**

- новый исследователь выполняет fixture experiment по документации;
- любой экран с выводом показывает evidence и version;
- destructive actions требуют отдельного подтверждения;
- система работает без внешнего cloud service.

## 22. Definition of Done всей системы

- [ ] Чистый clone разворачивается по README без скрытых ручных шагов.
- [ ] Toolchain и обе dependency graph зафиксированы.
- [ ] Fixture run повторяется в чистой среде.
- [ ] Все события и артефакты проходят schema validation.
- [ ] Run восстанавливается после принудительного завершения `labd`.
- [ ] Каждый outcome имеет machine-readable explanation.
- [ ] Можно переклассифицировать run без изменения raw evidence.
- [ ] Можно сравнить две версии сайта и две версии инструмента независимо.
- [ ] Legacy corpus импортирован с hashes и warnings.
- [ ] Backup восстановлен и полностью проверен.
- [ ] Secret/PII scans проходят.
- [ ] Security review закрыт.
- [ ] CI quality gate обязателен.
- [ ] Документация проверена новым пользователем.
- [ ] Нет открытых P0/P1 defects.

## 23. Метрики качества

### Надёжность

- доля корректно финализированных runs;
- recovery success rate;
- artifact verification rate;
- worker crash rate;
- classification inconclusive rate.

### Воспроизводимость

- доля runs с полным environment manifest;
- fixture repeat agreement;
- classifier deterministic agreement;
- число missing legacy fields.

### Инструменты

- calibration pass rate;
- selector volatility;
- step retry distribution;
- trace capture completeness;
- version-specific failure rate.

### Научное качество

- claims с control evidence;
- claims с contradicting evidence;
- повторяемость observations;
- среднее число независимых runs на supported claim;
- доля conclusions, прошедших human review.

Метрики не должны стимулировать сокрытие `INCONCLUSIVE`.

## 24. Риски и противодействие

| Риск | Мера |
|---|---|
| Утечка секретов из legacy | Gate 0, rotation, scanning, encryption |
| Смешивание site/tool failures | calibration и отдельные health dimensions |
| Слишком сложная платформа | modular monolith, local-first, no Kubernetes |
| Drift схем | versioning, generated schemas, migration corpus |
| Потеря больших evidence | CAS, backup, checksum verification |
| Ложные причинные выводы | observations/classifications/claims separation |
| Невоспроизводимый browser stack | `uv.lock`, instrument descriptor, trace |
| Коррупция ledger | SQLite transactions, backups, integrity checks |
| Race conditions | single writer, idempotency, expected-state transitions |
| Тихая ручная правка отчётов | generated directory и reproducibility checks |

## 25. Архитектурные решения, которые нужно зафиксировать ADR

1. Modular monolith вместо микросервисов.
2. Rust domain core и Python browser instrument.
3. SQLite WAL как первый operational ledger.
4. Append-only events + rebuildable projections.
5. OpenDAL + content-addressed artifact storage.
6. Arrow/Parquet/DataFusion для аналитики.
7. Protobuf over UDS для instrument protocol.
8. TOML/JSON records и generated Markdown.
9. Pseudonymous subject identity.
10. Signed sealed manifests.
11. Offline-first execution.
12. Deterministic rule-based classification before probabilistic assistance.

## 26. Первые практические задачи

Порядок нельзя менять без отдельного ADR:

1. Создать безопасный backup текущего dirty worktree.
2. Выполнить Gate 0.
3. Зафиксировать ADR-001…ADR-003.
4. Создать Rust workspace и CI skeleton.
5. Реализовать domain types и schema generation.
6. Реализовать ledger и lifecycle recovery.
7. Реализовать CAS и manifests.
8. Только затем подключать browser worker.

---

## Заключение

Ультимативность этой системы определяется не количеством инфраструктуры, а
доказуемыми свойствами: неизменяемостью evidence, прослеживаемостью выводов,
версионированием инструментов, честным учётом неопределённости, воспроизводимостью
и простым локальным запуском.

Этот документ является master plan. Любое существенное отклонение оформляется ADR
с причиной, альтернативами, последствиями и влиянием на перечисленные критерии
10/10.
