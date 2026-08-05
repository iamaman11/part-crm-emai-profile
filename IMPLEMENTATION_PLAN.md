# Экспертный План Реализации Browser Profile Platform

**Статус:** reviewed, разрешен к поэтапному выполнению через обязательные gates

**Дата:** 2026-08-05

**Горизонт:** самостоятельный production-ready продукт с последующей интеграцией в CRM

**Основание:** проверенные исследования, ADR-0001..ADR-0004 и cloud replay smoke test

## 1. Результат

Построить приложение, в котором администратор организации:

1. создает пользователей и управляет их доступом;
2. создает карточки клиентов;
3. создает, импортирует и назначает browser profiles клиентам;
4. выдает конкретным пользователям права просмотра или работы с профилем;
5. видит cloud status, активные сессии, историю поколений, mailbox status,
   certification и полный audit;
6. может безопасно отозвать доступ, завершить сессию и восстановить поколение.

Обычный пользователь видит только разрешенные ему профили и связанные с ними
данные клиента. Он открывает профиль из web application; установленный один раз
локальный Profile Bridge материализует профиль, запускает отдельное видимое окно
Camoufox и после закрытия надежно синхронизирует новое поколение в Cloudflare R2.

## 2. Зафиксированные Решения

| Область | Решение |
|---|---|
| Product UI | web application, единая точка входа для каталога, клиентов и операций |
| Local runtime | Windows-native Rust Profile Bridge и отдельное окно Camoufox |
| Server | Rust API/BFF + worker; один deployment, логически разделенные процессы |
| Identity | OIDC; standalone deployment использует Keycloak `26.3.3`, совместимый с CRM baseline |
| Browser login UX | долгоживущая server-side session в Secure HttpOnly cookie; пароль не хранится в приложении |
| Database | PostgreSQL с tenant-scoped keys и принудительным RLS с первой версии |
| Local database | SQLite только как неавторитетный device cache/outbox Profile Bridge |
| Browser storage | локальная материализация активного поколения; R2 не является live filesystem |
| Cloud storage | client-side encrypted immutable generations в Cloudflare R2 |
| Concurrency | один writer через lease epoch, fencing token, OS lock и conditional object create |
| Client relation | у профиля не более одного активного primary client; у клиента много профилей |
| Authorization | tenant owner + явные resource grants; default deny |
| Runtime delivery | signed content-addressed bundles, side-by-side install и atomic rollback |
| CRM integration | versioned capability/events; никакого прямого доступа к таблицам CRM |

Приложение не требует регистрации в Microsoft. Подписанный MSIX/App Installer или
обычный подписанный bootstrap installer распространяется напрямую. Microsoft
Store остается необязательным каналом распространения.

### 2.1 Условия Исполнимости

План можно выполнять с Phase 0. Следующие условия являются phase gates, а не
скрытыми предположениями:

- новый workspace действительно pin-ит Rust `1.97.1`; текущий parent checkout
  использует `channel = "stable"`, а локальный WSL toolchain на дату review имеет
  Rust `1.95.0`, поэтому cold-build gate включает установку и проверку exact
  toolchain до написания production-кода;
- перед реализацией envelope encryption принимается отдельный ADR с конкретным
  production key manager, rotation, backup, unseal/recovery и key-loss policy;
- development Bridge может быть подписан тестовым сертификатом, но stable release
  невозможен без trusted Windows code-signing certificate;
- multi-device gate требует второго независимого Windows-native устройства или
  физически эквивалентного acceptance host, а не второй папки на том же WSL;
- production deployment требует domain/TLS, PostgreSQL backup target,
  observability target и owner break-glass procedure;
- BFF остается выбранным browser-facing auth pattern. Совместимость с текущим
  CRM SPA OIDC обеспечивается общими issuer/subject/tenant semantics и versioned
  integration contracts, а не хранением browser access token в Web Storage.

Ни одно из этих условий не блокирует Phase 0 или первый domain vertical slice.
Они запрещают только преждевременное продвижение в соответствующую phase.

## 3. Пользователи И Доступ

### 3.1 Tenant И Администратор

Даже если первый deployment обслуживает одну организацию, каждая бизнес-таблица
сразу содержит `tenant_id`. Это исключает дорогостоящую переделку при включении
multi-tenancy в CRM.

В MVP у tenant есть ровно один активный `TENANT_OWNER`. Он является главным
администратором и имеет все capabilities внутри tenant. Роль не привязывается
навсегда к первому аккаунту: предусмотрена подтверждаемая передача владения и
отдельный audited break-glass recovery. Нельзя удалить или заблокировать
последнего owner без успешной передачи роли.

Остальные пользователи имеют membership `MEMBER` и не получают доступ к
профилям автоматически. Создание, изменение и отзыв grants выполняет только
owner. Модель допускает добавление delegated admins позднее без изменения ACL.

### 3.2 Resource Grants

`ProfileAccessGrant`:

| Роль | Возможности |
|---|---|
| `PROFILE_VIEWER` | видеть карточку, status, связанного клиента в минимальном виде, history и certification |
| `PROFILE_OPERATOR` | все viewer-права, materialize/open/close/sync, mailbox check и безопасный retry |

`ClientAccessGrant`:

| Роль | Возможности |
|---|---|
| `CLIENT_VIEWER` | видеть полную разрешенную карточку клиента |
| `CLIENT_EDITOR` | редактировать карточку, теги, contact points и заметки |

Право на browser profile не означает право управлять ACL, удалять профиль,
делать rollback, переназначать клиента или раскрывать secret material. Эти
операции остаются административными. Доступ к профилю дает только минимальную
проекцию связанного клиента; полная карточка требует отдельного client grant.

Будущие teams реализуются как новый тип grant subject. В MVP субъектом grant
является только user, чтобы не усложнять authorization engine раньше времени.

### 3.3 Проверка Авторизации

- identity и `tenant_id` берутся только из проверенной OIDC session;
- JWT/session сообщает identity, но не является единственным источником ACL;
- актуальный grant проверяется сервером при каждой чувствительной команде;
- PostgreSQL RLS является вторым независимым барьером tenant isolation;
- revoke запрещает новые launch intents немедленно;
- активная сессия отозванного пользователя получает typed drain по policy;
- list/get возвращают одинаковый `not found` для чужого и отсутствующего ресурса;
- каждая ACL-мутация, open, close, export, restore и assignment попадает в audit.

## 4. Карточки Клиентов

### 4.1 Standalone Owner

До интеграции собственником минимальной клиентской записи является bounded
context `Client Registry`. После интеграции authoritative owner становится CRM
Party/Customer Master, а этот сервис хранит только `party_ref`, локальную
проекцию и sync metadata. Browser profile payload при этом не мигрирует.

`ClientRecord`:

- opaque `client_id`, `tenant_id`, aggregate version;
- kind: `PERSON` или `ORGANIZATION`;
- display name и optional legal name;
- status: `ACTIVE`, `ARCHIVED`, `MERGED`;
- country, timezone, locale;
- tags и структурированные notes;
- governed contact points: email, phone, URL;
- optional `external_party_ref` и projection version;
- created/updated actor and timestamps.

Contact point value не используется как ID. Для поиска хранятся нормализованный
tenant-keyed HMAC lookup token и отдельно зашифрованное отображаемое значение.
PII не попадает в URL, filesystem path, R2 key, telemetry или audit details.

### 4.2 Назначение Профиля Клиенту

Связь является отдельной сущностью `ProfileClientAssignment`, а не изменяемым
полем в `BrowserProfile`:

- profile и client обязаны принадлежать одному tenant;
- у профиля может быть ноль или один active primary assignment;
- у клиента может быть любое число профилей;
- новое назначение атомарно закрывает предыдущее;
- история assignment immutable и содержит actor, reason и timestamps;
- archived client нельзя назначать новым профилям;
- client нельзя hard-delete при активных профилях, audit или retention hold;
- assignment сам по себе не выдает пользователю права на профиль или клиента.

Это позволяет отвечать на вопросы «кто, когда и почему переназначил профиль» и
строить Customer 360 projection без потери истории.

## 5. Bounded Contexts И Владение

| Контекст | Владеет | Не владеет |
|---|---|---|
| Identity & Access | tenant, membership, grants, device enrollment | OIDC password hash |
| Client Registry | standalone client card, contact points, CRM reference | browser state |
| Profile Catalog | profile, assignment, generation pointer, placement | browser process |
| Runtime Sessions | launch intent, lease, session, drain/close orchestration | Camoufox internals |
| Profile Storage | encrypted snapshot, manifest, restore, retention | catalog authorization |
| Certification | immutable runs, evidence, promotion decision | external checker truth |
| Mailbox Operations | binding, check jobs, provider observations | canonical CRM communications |
| Runtime Distribution | device, bridge/runtime versions, signed update policy | profile data |
| Audit & Outbox | append-only security/business trail and integration events | domain decisions |

Один mutable aggregate имеет одного authoritative owner. Межконтекстные связи
представляются opaque IDs и versioned contracts, а не foreign-table writes.

## 6. Компоненты И Потоки

```text
Browser
  -> Web UI
    -> Rust API/BFF
      -> PostgreSQL + outbox + audit
      -> background worker
      -> KMS/secret adapter
      -> Cloudflare R2

Web UI
  -> profilebridge://claim/<single-use-code>
    -> Windows Profile Bridge
      -> HTTPS ticket redemption + lease/device authorization
      -> local encrypted staging/cache
      -> managed Camouhost IPC
        -> separate visible Camoufox process
```

Web page не открывает localhost HTTP/WebSocket server. Custom URI содержит
только случайный одноразовый code с TTL 30-60 секунд, без profile ID, JWT,
email, R2 credential или encryption key. Bridge погашает code через HTTPS с
device-bound ключом и получает минимальный session descriptor. Server-side
intent жестко связан с tenant, actor, device, profile, requested capability,
expiry и nonce; другой actor или device не может его погасить.

### 6.1 Создание Пользователя

1. Owner создает signed, random, single-use invitation с identity hint и expiry.
2. Keycloak выполняет регистрацию, password policy, reset и optional MFA.
3. Callback связывает проверенный OIDC subject с pending invitation; совпадение
   одного email само по себе не является доказательством владения invitation.
4. В транзакции создаются membership, audit и outbox event.
5. Новый member видит пустой каталог до выдачи grants.

### 6.2 Создание И Назначение Профиля

1. Owner создает client card или выбирает существующую.
2. API создает opaque profile ID и initial assignment.
3. Runtime policy выбирает approved bundle и fingerprint policy. До Phase 6 это
   только test lane; production promotion требует certification gate.
4. Bridge создает профиль локально и пользователь проходит browser login.
5. После close создается encrypted generation и immutable R2 objects.
6. Catalog активирует generation только после remote verification.
7. Owner выдает `PROFILE_OPERATOR` нужному member.

### 6.3 Открытие Профиля

1. API проверяет membership, grant, device, profile status и certification.
2. Создает single-use launch intent, но еще не выдает R2 access.
3. Bridge redeems intent с device proof.
4. Server получает lease epoch/fencing token и выдает short-lived prefix-scoped
   object access плюс wrapped generation DEK.
5. Bridge materializes в staging, проверяет manifest/digest/inventory и атомарно
   активирует local workspace.
6. Supervisor запускает Camoufox и подтверждает effective runtime/fingerprint.
7. Heartbeat поддерживает lease, а web UI получает status только через server.

### 6.4 Закрытие И Cloud Sync

1. Пользователь закрывает Camoufox или нажимает `Save & Close`.
2. Bridge выполняет typed drain и подтверждает завершение process tree.
3. Compact snapshot создается только из quiescent workspace.
4. Новый generation шифруется per-generation DEK и загружается immutable.
5. Worker проверяет объект, manifest и restore-readability.
6. Одна catalog transaction меняет active pointer, пишет outbox и audit.
7. Только после `SYNCED` снимается lease и допускается eviction local data.

Если окно забыто, supervisor показывает idle warning, затем выполняет graceful
close по configurable timeout. Hard TTL ограничивает бесконечную сессию. При
ошибке сети состояние остается `DIRTY_LOCAL`/`SYNC_RETRY_PENDING` и не удаляется.

## 7. Целевая Структура Исходников

```text
apps/
  api/                       # Axum BFF/API composition
  worker/                    # outbox, snapshot verification, retention, jobs
  profile-bridge/            # Windows service/tray/protocol activation
  camouhost/                 # Python runtime process, временно отдельный package
crates/
  primitives/                # IDs, time, digests, safe segments
  contracts/                 # protobuf/OpenAPI source and compatibility tests
  identity-access-domain/    # membership, grants, authorization decisions
  client-domain/             # cards, contact points, assignment policy
  profile-domain/            # profile/generation/fingerprint state machines
  session-domain/            # lease, launch, drain and recovery
  mailbox-domain/            # provider-neutral mailbox rules
  application-ports/         # owned interfaces
  use-cases/                 # transactions and orchestration
  infrastructure/            # Postgres, R2, KMS, OIDC and IPC adapters
frontend/                    # React web application
proto/                       # public versioned contracts
runtime/                     # manifests/locks/build definitions, no binaries in Git
deploy/                      # Compose, migrations, reverse proxy, observability
docs/                        # ADR, threat model, runbooks and evidence policy
```

Canonical dependency direction:

```text
apps -> use-cases -> application-ports + domains + contracts + primitives
infrastructure -> application-ports + contracts + primitives
domains -> primitives
```

Domain crates не зависят от Axum, SQLx, AWS SDK, Keycloak, Python, Playwright или
Camoufox. Новая crate создается только при реальной compile-time boundary.

## 8. Технологический Стек

### 8.1 Server И Web

| Область | Решение |
|---|---|
| Rust | `1.97.1`, edition `2024`, exact toolchain and Cargo.lock |
| Async/API | Tokio, Axum, Tower, rustls |
| Contracts | protobuf/prost/tonic для service boundaries; OpenAPI projection для web |
| Persistence | PostgreSQL, SQLx compile-time checked queries and migrations |
| Jobs | transactional outbox + leased PostgreSQL jobs; без Kafka в MVP |
| Identity | Keycloak `26.3.3`, OIDC Authorization Code flow, BFF server session |
| Web | CRM baseline: React `19.2.7`, TypeScript `7.0.2`, Vite `8.1.5`, TanStack Query `5.101.2`; Router только при подтвержденной необходимости |
| Telemetry | tracing, OpenTelemetry, Prometheus-compatible metrics, structured audit |
| Errors | stable problem codes; no raw infrastructure/PII in client errors |

### 8.2 Browser И Cloud

| Область | Baseline |
|---|---|
| Python | embedded `3.12.3`, exact locked environment |
| Camoufox | official `0.5.4` |
| Browser | `152.0.4-beta.28` exact runtime lane |
| BrowserForge | `1.2.4` |
| Playwright | `1.59.0` |
| Object storage | Cloudflare R2 via S3-compatible adapter |
| Archive | deterministic tar + Zstandard |
| Integrity | BLAKE3 inventory + SHA-256 interoperability |
| Encryption | streaming AEAD, per-generation DEK, tenant KEK wrapping |
| Device key | Windows CNG/DPAPI-backed non-exportable key where available |

Pinned versions являются текущим verified baseline, а не разрешением обновлять
профили in-place. Любое обновление создает новый runtime lane и проходит canary.
Frontend versions сверены с текущим `part_crm`; browser runtime versions сверены
с проведенным cloud/fingerprint smoke test.

## 9. Основная Модель Данных

Центральные таблицы:

- `tenants`, `users`, `tenant_memberships`, `invitations`;
- `devices`, `device_keys`, `device_enrollments`, `device_revocations`;
- `clients`, `client_contact_points`, `client_access_grants`;
- `browser_profiles`, `profile_client_assignments`, `profile_access_grants`;
- `profile_generations`, `profile_snapshots`, `fingerprint_policies`;
- `profile_leases`, `browser_sessions`, `launch_intents`;
- `certification_runs`, `runtime_bundles`, `runtime_promotions`;
- `mailbox_bindings`, `mail_check_jobs`, `mail_observations`;
- `idempotency_records`, `outbox_events`, `audit_events`, `retention_holds`.

Каждый tenant-owned primary/unique/foreign key включает `tenant_id`, чтобы SQL
не мог случайно связать объекты разных tenants. RLS включен и `FORCE` для
application roles. Database owner не используется runtime-процессом.

Mutable aggregate имеет optimistic `version`. API mutation требует
`Idempotency-Key`; stale version возвращает typed conflict. Audit append-only и
содержит actor, tenant, action, subject refs, outcome, correlation ID и
sanitized diff без cookies, email body или credentials.

## 10. Неподлежащие Нарушению Инварианты

1. Один tenant имеет хотя бы одного и в MVP ровно одного active owner.
2. Member без grant не видит и не запускает профиль.
3. Grant и resource всегда принадлежат одному tenant.
4. У профиля не более одного active primary client assignment.
5. Assignment не является authorization grant.
6. Один generation имеет одного writer; stale fencing token не пишет snapshot.
7. Profile path и R2 key строятся только из validated opaque IDs.
8. Email, имя клиента и login не входят в path, URL, object key или launch URI.
9. Оригинальный legacy profile всегда read-only; проверки выполняются на clone.
10. Snapshot запрещен при live/opening/draining browser process.
11. Restore всегда создает новую materialization/generation и не перезаписывает active directory.
12. Remote objects immutable; active pointer меняется транзакционно в catalog.
13. Dirty local generation не удаляется до подтвержденного cloud commit.
14. Runtime update никогда не меняет существующий generation in-place.
15. Secret material существует только на outer adapter boundary и не логируется.
16. Все чувствительные mutations проходят live authorization и audit.
17. Unknown schema/runtime mismatch приводит к quarantine, не к silent downgrade.
18. Certification claim всегда связан с exact runtime, generation и evidence.

## 11. Security И Privacy Baseline

- пароль хранит только Identity Provider как Argon2id/bcrypt-compatible hash;
- приложение использует Secure, HttpOnly, SameSite cookie, rotation и revocation;
- `remember device` дает долгую сессию, но имеет idle/max lifetime, не вечный secret;
- passkeys/WebAuthn и MFA включаются без изменения domain model;
- CSRF token, strict Origin checks, CSP, HSTS и dependency integrity обязательны;
- launch intent single-use, high-entropy, короткоживущий и audience-bound;
- bridge device key регистрируется с явным approval owner/user;
- R2 client получает только временный доступ к нужному generation prefix;
- cookies, auth databases и local storage шифруются до R2 upload;
- tenant KEK хранится в KMS/secret provider, не в PostgreSQL/R2;
- production key provider выбирается до cloud phase отдельным ADR; локальный
  Secret Vault smoke key не допускается как multi-device production KEK;
- key rotation rewraps DEKs без расшифрования profile archives;
- export/download raw profile отсутствует в обычном UI;
- grant profile operator фактически дает доступ к активной web-session клиента и
  поэтому показывается администратору как high-impact permission;
- threat model покрывает malicious archive, confused deputy, cross-tenant IDOR,
  replay, stolen device, rollback/freeze update, disk theft и supply chain.

До production реализации требуется отозвать legacy hardcoded proxy credential и
временные Cloudflare bootstrap/provisioning credentials после явного approval.

## 12. Runtime Packaging И Самообновление

Profile Bridge и Camoufox runtime обновляются независимо:

1. Web UI обновляется server-side без установки.
2. Небольшой signed Profile Bridge обновляется через MSIX/App Installer или
   signed updater.
3. Большой runtime bundle скачивается content-addressed, проверяется signature,
   hash, SBOM и compatibility manifest.

Runtime bundle включает embedded Python, exact wheels, browser binary,
Playwright driver, fonts/addons/GeoIP, protobuf descriptor, licenses, SBOM,
hashes и signature. Bundles устанавливаются side-by-side. Updater выполняет
download, verify, doctor, local canary и atomic active-pointer switch; хранит не
меньше двух последних версий. Активная сессия не обновляется и не мигрируется.

Update metadata использует TUF-подобную модель: offline root, подписанные роли,
expiry, monotonic version, threshold signatures для production и защита от
rollback/freeze. Каналы `stable` и `canary` сертифицируются отдельно.

## 13. Этапы Реализации

### Phase 0: Security Containment И Architecture Baseline

- отозвать/заменить hardcoded proxy credential;
- завершить rotation временных Cloudflare provisioning tokens;
- включить gitleaks, dependency audit и PII/log scan;
- принять ADR-0001..ADR-0004;
- принять ADR выбора production key manager и recovery ceremony;
- создать threat model, data classification и incident severity model;
- зафиксировать inventory 22 legacy profiles clone-only способом.

**Gate:** secret scan чист; источник legacy не меняется при повторном scan;
architecture documents не противоречат друг другу.

### Phase 1: Reproducible Monorepo Foundation

- создать Rust workspace и целевую структуру;
- установить и pin Rust 1.97.1, не наследуя parent `stable`; pin Node/pnpm,
  Python runtime lock и container images;
- поднять PostgreSQL, Keycloak `26.3.3`, API, worker и web в development Compose;
- создать protobuf/OpenAPI compatibility gates;
- добавить fmt, clippy, tests, deny/audit, migration and cold-build gates;
- реализовать primitives: opaque IDs, safe segments, clock, digest, errors.

**Gate:** clean checkout одной командой поднимает system health; schema мигрирует
вперед и восстанавливается из backup; architecture dependency test проходит.

### Phase 2: Identity, Tenant И Client Registry

- OIDC/BFF login, logout, refresh, CSRF и session revocation;
- зафиксировать identity compatibility contract: issuer, subject, tenant,
  audience, auth time and session revocation semantics;
- bootstrap первого tenant owner;
- invitation/member lifecycle и owner transfer;
- clients, encrypted contact points, archive/merge-ready state;
- client grants и field-level projections;
- RLS, authorization decision service, audit and idempotency;
- UI: login, users, invitations, client list/card/history.

**Gate:** cross-tenant/read-IDOR tests fail closed; последний owner не удаляется;
revoked member теряет доступ; client card CRUD и audit проходят E2E.

### Phase 3: Profile Catalog, Assignments И ACL

- BrowserProfile aggregate и generation metadata;
- historical ProfileClientAssignment;
- profile viewer/operator grants;
- list/search/filter by client, assignee, status and cloud state;
- optimistic concurrency, transactional outbox and audit;
- owner UI для create/assign/reassign/grant/revoke/archive.

**Gate:** member видит только granted resources; assignment не повышает права;
конкурентное переназначение дает один active client; revoke немедленно блокирует
новый launch intent.

### Phase 4: Profile Bridge И Device Trust

- Windows-native Rust Bridge skeleton;
- signed installer, protocol registration и device enrollment;
- non-exportable device key, revocation and capability handshake;
- single-use launch intent redemption;
- local SQLite cache/outbox и protected application data;
- supervisor через Windows Job Object;
- updater bootstrap с signature verification and rollback.

**Gate:** поддельный/expired/replayed URI отвергается; revoked device не получает
lease/key; intent нельзя погасить другим actor/device; browser не может вызвать
privileged localhost API; process tree гарантированно обнаруживается и
завершается.

### Phase 5: Local Browser Lifecycle

- managed Camouhost IPC и exact runtime bundle;
- create/open/heartbeat/drain/close state machine;
- lease epoch, fencing token, OS lock and stale-session recovery;
- fixed fingerprint manifest до первого navigation;
- idle warning, timeout, hard TTL and `Save & Close`;
- crash, reboot, disk-full and orphan-process recovery;
- clone-only import path для 22 legacy profiles.

**Gate:** concurrent writer fail-closed; kill/restart не повреждает профиль;
authorization state переживает graceful replay; lock files не удаляются кодом.

### Phase 6: Fingerprint Certification

- реализовать ADR-0001 signal classes и policy versioning;
- exact effective-value probe и sanitized evidence;
- 10 cold-start consistency lane;
- 100-profile uniqueness cohort;
- CreepJS, BrowserLeaks, BrowserScan и Pixelscan adapters;
- fonts/audio/canvas/WebGL/WebGPU/codecs/permissions/network coherence checks;
- runtime promotion, quarantine, expiry and drift dashboard.

**Gate:** unexplained stable-signal drift равен нулю; cohort collisions равны
нулю; specialized-site warnings classified; Windows-native lane имеет signed
certification report. Это измеримое качество, а не обещание абсолютной
невидимости.

### Phase 7: Cloud Generations И Multi-Device

- подтвердить production key-manager ADR и recovery ceremony;
- deterministic compact inventory и exclusion policy;
- safe tar/zstd, streaming AEAD and envelope keys;
- immutable conditional R2 upload and verified restore;
- short-lived scoped object credentials;
- dirty-local outbox/retry and conflict branch workflow;
- retention, deletion, orphan reconciliation and restore drills;
- второе физическое Windows-устройство в acceptance lane.

**Gate:** certified profile проходит create -> login -> close -> R2 -> clean
second device -> open с сохранением authorization и inventory; corrupt
ciphertext/path traversal rejected; cache2 не загружается; stale writer не
активирует generation; key rotation и loss/recovery drill имеют retained
evidence.

### Phase 8: Mailbox Operations

- provider-neutral MailboxBinding;
- Gmail OAuth/API и Mail.ru IMAP application-password adapters;
- browser fallback только там, где provider API недостаточен;
- idempotent jobs, cursor, rate limit, retry and audit;
- связь mailbox/profile/client и минимальные observations;
- никаких паролей, message body или raw PII в logs/events.

**Gate:** fake-provider contract suite и authorized live canaries проходят;
revoked grant останавливает mailbox command; provider failure не повреждает
profile generation.

### Phase 9: Product UX, Operations И Release

- dashboard: profiles, clients, users, active sessions and failures;
- profile card: client, grants, generations, cloud/cert/mail status;
- guided onboarding Bridge/runtime и compatibility doctor;
- safe admin operations, confirmations and recovery UX;
- metrics, tracing, alerts, audit explorer and support bundle redaction;
- PostgreSQL PITR, R2 restore, KMS recovery and key-loss runbooks;
- accessibility, responsive UI, localization and browser compatibility;
- signed stable/canary release pipeline and rollback drills.

**Gate:** end-to-end operator acceptance, WCAG baseline, backup/restore and
incident drills; update rollback сохраняет working previous runtime.

### Phase 10: CRM Integration

- CRM Party/Customer mapping and projection sync;
- заменить standalone Client Registry owner на CRM capabilities постепенно;
- единый OIDC tenant/actor context;
- versioned profile/client/mailbox capabilities and events;
- Customer 360 projection без profile payload/secrets;
- contract compatibility, dual-read comparison and rollback plan;
- сервис остается отдельным runtime boundary, пока объединение не доказано.

**Gate:** CRM не зависит от Python/Camoufox; profile service не пишет в CRM
таблицы; client links совпадают в dual-read; rollback не теряет assignments.

## 14. Тестовая Пирамида

| Уровень | Обязательные проверки |
|---|---|
| Domain | transitions, invariants, property tests, authorization matrix |
| Contract | protobuf/OpenAPI compatibility, unknown fields, version skew |
| Persistence | migrations, RLS, transactions, idempotency, lease/assignment races |
| Security | cross-tenant IDOR, CSRF, replay, malicious archive, stolen/revoked device |
| Filesystem | path safety, atomic rename, disk-full, interrupted writes, locks |
| Snapshot | inventory, encryption, corruption, restore, unknown files |
| Runtime | create/open/drain/close, crash, version mismatch, forgotten window |
| Fingerprint | consistency, uniqueness, coherence and specialized sites |
| Cloud | MinIO deterministic suite, R2 canary, conditional writes, retention |
| Mail | fake provider, OAuth expiry, IMAP failures, rate limits |
| UI | role-based navigation, accessibility, destructive confirmations |
| E2E | invite -> grant -> assign client -> cloud open -> sync -> second-device restore |

Каждый production defect получает regression test на минимально возможном
уровне. Live external lanes отделены от deterministic CI, имеют timeout,
sanitized artifacts и не блокируют unrelated unit feedback.

## 15. Production SLO И Operations

Начальные измеримые цели, уточняемые после load test:

- API availability: 99.9% monthly;
- catalog read p95: до 300 ms внутри deployment region;
- launch-intent issue p95: до 500 ms без учета profile download;
- cloud sync success: не менее 99.5% после автоматических retries;
- RPO для catalog: до 5 минут, RTO до 60 минут;
- zero tolerated cross-tenant exposure, secret-in-log или unverified generation activation;
- audit retention и profile retention задаются отдельно tenant policy.

Alerts строятся на пользовательских последствиях: stuck session, expiring lease,
dirty local without heartbeat, failed sync, orphan R2 object, certification drift,
KMS failure, updater signature failure и repeated authorization denials.

## 16. Definition Of Done Первого Production MVP

MVP готов, когда один owner может без CLI:

1. войти, пригласить пользователя и отозвать его доступ;
2. создать и изменить карточку клиента;
3. создать профиль, назначить клиента и выдать operator grant;
4. установить/enroll Bridge через понятный guided flow;
5. открыть Camoufox, пройти авторизацию, закрыть и дождаться `SYNCED`;
6. открыть тот же профиль на втором enrolled компьютере без повторного login;
7. увидеть active session, generation history, certification и audit;
8. восстановить last-known-good generation после controlled corruption;
9. автоматически завершить забытую сессию без потери dirty state;
10. доказать tenant isolation, single-writer и encrypted R2 restore тестами.

Дополнительно release pipeline воспроизводимо собирает подписанный Bridge и
runtime bundle, обновляет их side-by-side и успешно выполняет rollback. Backup,
restore, owner recovery, device loss, credential rotation и incident runbooks
проверены практическими drills, а не только описаны.

## 17. Первый Исполнимый Vertical Slice

После утверждения плана первый slice должен пройти через все слои, но не запускать
Camoufox:

1. Rust workspace, PostgreSQL и migrations;
2. Keycloak OIDC/BFF login и bootstrap owner;
3. client card create/list/get/update;
4. BrowserProfile metadata create;
5. transactional profile-to-client assignment;
6. invitation/member и profile grant;
7. RLS, live authorization, idempotency, audit and outbox;
8. React screens `Clients`, `Profiles`, `Users & Access`;
9. E2E: owner creates client/profile, grants member, member sees only granted
   profile and minimal linked-client projection;
10. quality gate и retained acceptance report.

Второй slice добавляет Profile Bridge enrollment и безопасный launch intent.
Только третий slice подключает Camouhost и local lifecycle. Такой порядок сначала
доказывает наиболее опасные boundaries: identity, tenant isolation, grants и
ownership, а затем допускает browser secrets и cloud materialization.

## 18. Readiness Verdict

План прошел повторный review против текущих ADR, lifecycle, cloud smoke evidence
и фактического `part_crm` checkout.

- Phase 0 можно начинать сейчас.
- Phases 1-5 имеют определенные boundaries, deliverables и testable gates.
- Phase 6 требует Windows-native certification environment и внешние checker
  lanes, уже явно включенные в gate.
- Phase 7 требует принятого key-manager ADR, temporary R2 credentials и второго
  Windows acceptance host.
- Phase 9 stable release требует trusted code signing, TLS и проверенных backup,
  restore и incident drills.

После этих уточнений блокирующих архитектурных противоречий не осталось. План
можно выполнять последовательно; запрещено только пропускать gates или считать
исследовательский smoke evidence production certification.
