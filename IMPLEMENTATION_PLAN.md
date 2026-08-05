# Экспертный План Реализации Browser Profile Platform

**Статус:** reviewed, готов к поэтапной реализации через обязательные gates

**Дата:** 2026-08-05

**Горизонт:** standalone production-ready продукт с последующей интеграцией в CRM

**Основание:** research findings, ADR-0001..ADR-0005, proposed ADR-0006 и cloud
replay smoke test

## 1. Результат

Построить приложение, в котором owner организации:

1. допускает пользователей и управляет их доступом;
2. создает карточки клиентов;
3. создает, импортирует и назначает browser profiles клиентам;
4. выдает пользователям права просмотра или работы с конкретным профилем;
5. видит cloud status, активные сессии, поколения, mailbox status,
   certification и audit;
6. может отозвать доступ, безопасно завершить сессию и восстановить поколение.

Member видит только выданные ему profiles и минимально необходимую связанную
информацию. Из web UI он открывает профиль через установленный один раз Windows
Profile Bridge. Bridge запускает отдельное видимое окно Camoufox и после
закрытия автоматически сохраняет новое encrypted generation в Cloudflare R2.

## 2. Принятые Решения

| Область | Решение |
|---|---|
| Product UI | responsive React web application |
| Hosting | Cloudflare Workers Static Assets, SPA и API на одном origin |
| Cloud API | Rust `workers-rs`, WebAssembly, один control-plane Worker |
| Identity | Cloudflare Access с approved IdP или email OTP |
| App authorization | tenant owner + live memberships + explicit resource grants |
| Catalog | Cloudflare D1, один tenant в первой production deployment |
| Profile coordinator | один Durable Object на profile: lease, fencing, session serialization |
| Async | Cloudflare Queues + Scheduled Workers; idempotent consumers |
| Cloud payload | client-side encrypted immutable R2 generations |
| Local runtime | Windows-native Rust Profile Bridge + separate Camoufox window |
| Local state | encrypted filesystem workspace; SQLite cache/outbox only |
| Runtime delivery | signed content-addressed side-by-side bundles с rollback |
| CRM integration | stable contracts и replaceable Cloudflare adapters |

Standalone v1 не требует отдельной VM, PostgreSQL, Keycloak, Docker Compose или
регистрации приложения в Microsoft. Microsoft Store остается необязательным
distribution channel. Cloudflare Browser Run и Containers не исполняют
Camoufox-профили.

## 3. Нефункциональные Инварианты

### 3.1 Correctness

- один профиль имеет не более одного writer;
- lease epoch монотонен, каждый write несет fencing token;
- активируется только verified immutable generation;
- replay любого command/queue message безопасен;
- dirty local workspace не удаляется до подтвержденного cloud sync;
- live browser directory никогда не архивируется;
- D1, Durable Object и R2 связываются saga/reconciliation, не фиктивной
  distributed transaction.

### 3.2 Security

- Access identity не заменяет app membership/grant;
- default deny применяется к каждому profile/client endpoint;
- browser/Bridge не получают D1 binding, root key или постоянный R2 credential;
- PII и email не используются как IDs, paths или R2 keys;
- profile payload шифруется до R2;
- secrets не попадают в Git, logs, audit details, screenshots и support bundles;
- device command требует proof-of-possession, а не только bearer token;
- cross-tenant и IDOR negative tests являются release gate.

### 3.3 Operability

- вся мутация имеет correlation ID, actor, idempotency key и audit result;
- каждый async transition имеет retry budget, dead-letter handling и reconciler;
- deploy, migration, rollback, key rotation и restore имеют runbook;
- cloud/local состояние отображается пользователю без ложного `Saved`;
- обновление Bridge/runtime происходит side-by-side и откатывается атомарно.

## 4. Identity, Users И Access

### 4.1 Login

Cloudflare Access закрывает SPA и browser API. Пользователь входит через внешний
OIDC provider или Cloudflare email OTP. Device-bound `/bridge/*` routes имеют
отдельную Worker policy и не зависят от browser cookie. Приложение не принимает,
не хранит и не восстанавливает пользовательский пароль.

Access session может быть долгоживущей в пределах security policy, но не
«вечной». Logout, owner revoke, Access policy change и device revoke должны
действовать немедленно для новых команд.

Worker валидирует Access JWT: issuer, audience, signature, expiry и subject.
Затем приложение загружает актуальный `Membership`; email claim используется
только как verified contact hint, но не как resource authorization.

### 4.2 Tenant И Owner

Первая production deployment обслуживает одну организацию. У нее ровно один
active `TENANT_OWNER`; остальные пользователи имеют `MEMBER`. Последнего owner
нельзя удалить, заблокировать или понизить без audited owner-transfer ceremony.

Добавление пользователя состоит из двух независимых разрешений:

1. identity разрешена Cloudflare Access policy;
2. owner создал app membership или single-use invitation.

Прохождение Access без membership дает нейтральный denied/bootstrap экран.

### 4.3 Grants

`PROFILE_VIEWER` видит status/history/certification. `PROFILE_OPERATOR` также
может materialize/open/close/sync и запускать mailbox check. Owner-only остаются
ACL, assignment, rollback, delete, export и policy changes.

`CLIENT_VIEWER` видит полную карточку клиента, `CLIENT_EDITOR` ее изменяет.
Profile grant дает только минимальную projection связанного клиента. Client
assignment не выдает никакого grant.

## 5. Client Registry

`ClientRecord` содержит opaque `client_id`, `tenant_id`, aggregate version,
kind, display/legal name, status, locale/timezone/country, governed contact
points, tags, notes, optional `external_party_ref` и audit metadata.

Contact point хранится как encrypted display value плюс tenant-keyed HMAC lookup
token. Client архивируется/merge-ится, но не hard-delete при active profiles,
retention hold или audit need.

`ProfileClientAssignment` является отдельной исторической сущностью:

- profile/client принадлежат одному tenant;
- у profile ноль или один active primary assignment;
- reassignment закрывает предыдущую запись и создает новую атомарно;
- actor, reason и timestamps обязательны;
- archived client нельзя назначить;
- assignment не является access grant.

## 6. Runtime Topology

```text
Cloudflare Access
  -> Rust control-plane Worker
    -> React Static Assets
    -> D1 catalog/audit/outbox
    -> Durable Object per profile
    -> Queues/Scheduled consumers
    -> R2 encrypted generations/runtime artifacts/evidence

React UI
  -> profilebridge://claim/<single-use-code>
    -> Windows Profile Bridge
      -> device-bound HTTPS protocol
      -> encrypted local materialization
      -> Camouhost typed IPC
        -> visible Camoufox process
```

Custom URI содержит только random single-use code с TTL 30-60 секунд. Profile
ID, JWT, email, R2 credential и key в URI запрещены. Intent связан с tenant,
actor, device, profile, capability, expiry и nonce.

## 7. Source Layout И Dependency Rules

```text
apps/
  control-plane-worker/      # workers-rs routes, assets, queues, schedules
  profile-bridge/            # native Windows supervisor/updater
  camouhost/                 # embedded Python runtime provider
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

```text
apps -> use-cases -> application-ports + domains + contracts + primitives
adapters -> application-ports + contracts + primitives
domains -> primitives
```

Domain crates не зависят от `worker`, D1/R2 SDK, Windows, Python, Playwright или
Camoufox. Cloud Worker не зависит от Axum, SQLx или Tokio threads. Profile Bridge
может использовать Tokio. Architecture test проверяет forbidden Cargo edges.

## 8. Технологический Стек

### 8.1 Cloud Control Plane

| Область | Решение |
|---|---|
| Language | Rust `1.97.1`, edition `2024`, exact `rust-toolchain.toml` |
| Runtime | `workers-rs`, `wasm32-unknown-unknown`, Workers Modules |
| Hosting | Workers Static Assets + Worker-first API routes |
| Identity | Cloudflare Access JWT + app membership/grants |
| Catalog | D1 migrations, typed repositories, optimistic versions |
| Coordination | Durable Object per profile |
| Jobs | Queues, scheduled triggers, DO alarms where profile-local |
| Object storage | R2 native binding; temporary/presigned access only when required |
| Secrets | Workers Secrets/Secrets Store behind `KeyProviderPort` |
| Contracts | OpenAPI web DTO; protobuf Bridge/CRM contracts |
| Observability | Workers Logs/Analytics plus safe structured audit in D1/R2 |

Exact `workers-rs`, Wrangler и Cloudflare test package versions выбираются и
pin-ятся после cold-build compatibility spike с Rust `1.97.1`. Rust official
`workers-rs` support covers D1, R2, Queues и Durable Objects; production code не
дублируется на TypeScript.

### 8.2 Web UI

- React `19.2.7`;
- TypeScript `7.0.2`;
- Vite `8.1.5`;
- pnpm;
- TanStack Query `5.101.2`;
- generated OpenAPI client;
- schema-driven forms, accessible primitives и Playwright E2E.

Frontend versions синхронизированы с текущим parent CRM baseline и снова
проверяются перед scaffold, а не обновляются автоматически.

### 8.3 Local Runtime

- Rust Profile Bridge, Windows native;
- Windows CNG/DPAPI, TPM-backed device key where available;
- SQLite WAL только для cache/outbox;
- embedded Python `3.12.3`;
- Camoufox official `0.5.4`;
- Camoufox browser `152.0.4-beta.28`;
- BrowserForge `1.2.4`;
- Playwright `1.59.0`;
- zstd, BLAKE3 и versioned streaming AEAD container;
- signed SBOM-bearing runtime bundles.

Эти browser versions являются исследованным baseline, а не автоматическим
upgrade channel. Каждое обновление проходит replay/certification matrix.

## 9. D1 Data Model

Минимальные таблицы:

- `tenants`, `identities`, `memberships`, `invitations`;
- `clients`, `client_contact_points`, `client_grants`;
- `browser_profiles`, `profile_client_assignments`, `profile_grants`;
- `profile_generations`, `profile_sessions`, `launch_intents`;
- `devices`, `device_credentials`, `runtime_channels`;
- `certification_runs`, `mailbox_bindings`, `mailbox_jobs`;
- `idempotency_records`, `audit_events`, `outbox_events`, `reconciliation_jobs`.

Правила схемы:

- UUID/ULID-like opaque IDs создаются приложением;
- tenant-owned unique/FK keys включают `tenant_id`;
- foreign keys и check constraints включены;
- active assignment и active owner защищены partial/guarded uniqueness и
  transactional command checks;
- aggregate version меняется compare-and-set;
- migration forward-only, repeatable local test и remote backup marker;
- ciphertext/large evidence/profile payload не хранятся в D1.

D1 не имеет RLS. Typed `TenantScope` обязателен на repository boundary;
unscoped queries запрещает static/architecture test. Для multi-tenant SaaS до
добавления второго tenant принимается отдельный isolation ADR.

## 10. Durable Object Protocol

Deterministic DO name: tenant/profile opaque IDs after safe canonical encoding.
DO хранит только profile-local coordination state:

- current lease epoch и fencing token hash;
- actor/device/session binding;
- heartbeat/idle/hard deadlines;
- last accepted command/idempotency result;
- pending close/snapshot/generation transition;
- recovery marker after eviction/restart.

DO не хранит client card, grants или authoritative generation history. Перед
выдачей lease Worker проверяет D1 membership/grant/status; перед commit нового
generation D1 повторно проверяет актуальность fencing token и profile version.

Acceptance обязательно моделирует concurrent open, delayed old writer, DO
eviction, duplicate command, Queue redelivery и network partition.

## 11. Encryption И Cloud Storage

Каждый generation хранится по immutable key:

```text
profiles/v1/<tenant_id>/<profile_id>/<generation_id>/
  manifest.pb
  profile.tar.zst.enc
  inventory.blake3
  certification.pb
```

Key hierarchy:

```text
versioned root wrapping key in Cloudflare secret storage
  -> wrapped tenant KEK in D1
    -> wrapped generation DEK in D1/manifest metadata
      -> encrypted R2 archive
```

До cloud production gate отдельный ADR фиксирует root-key backup, offline
recovery escrow, key rotation, dual-read/single-write, operator separation,
revocation и key-loss response. Restore drill выполняется на новом environment.

R2 server-side encryption остается дополнительным слоем. `cache2`,
`startupCache`, temporary downloads и browser lock files исключаются из compact
snapshot по versioned policy, но остаются локально во время работы.

## 12. Основные Потоки

### 12.1 Create Profile

1. Owner выбирает/создает client и approved runtime policy.
2. Worker создает opaque profile/assignment в D1 и audit/outbox.
3. UI выпускает actor/device-bound single-use launch intent.
4. Bridge получает DO lease и создает isolated local generation.
5. Пользователь проходит авторизацию в видимом Camoufox.
6. После close Bridge создает encrypted snapshot и загружает новый R2 object.
7. Queue verifier проверяет object/manifest/restore canary.
8. D1 compare-and-set активирует generation, UI показывает `READY`.

### 12.2 Open Existing Profile

1. Worker проверяет Access identity, membership, grant, device и profile status.
2. Bridge redeems intent с device proof.
3. DO выдает lease epoch/fencing token.
4. Bridge получает только нужный wrapped DEK и scoped object operations.
5. Generation скачивается в staging, decrypt/verify/unpack проходит безопасно.
6. Atomic rename активирует workspace, supervisor запускает Camoufox.
7. Heartbeat поддерживает lease, UI видит status через Worker API.

### 12.3 Close И Sync

1. Close начинается по user action, idle policy или owner drain.
2. Bridge выполняет typed graceful drain и подтверждает process-tree exit.
3. Из quiescent workspace создается deterministic compact snapshot.
4. Archive шифруется новым DEK и immutable загружается в R2.
5. Verification Queue проверяет archive и записывает result.
6. D1 активирует pointer только при current fencing/version.
7. DO закрывает session; local workspace становится evictable после `SYNCED`.

Если окно забыто, Bridge независимо от web page показывает native warning,
применяет configurable idle timeout и hard session TTL. При offline/error dirty
workspace удерживается, а SQLite outbox повторяет sync после восстановления сети.

## 13. Phases И Definition Of Done

### Phase 0. Security И Architecture Foundation

- отозвать обнаруженный legacy proxy credential;
- проверить Git history policy и включить secret scanning;
- pin Rust `1.97.1`, wasm target, Node/pnpm и locked dependencies;
- выполнить cold build `workers-rs` с D1/R2/DO/Queue bindings;
- принять ADR-0005 и threat model STRIDE/data classification;
- довести ADR-0006 key hierarchy/recovery до accepted;
- создать architecture/dependency tests и quality-gate skeleton;
- определить dev/staging/prod Cloudflare accounts/resources и naming.

**Gate:** чистый repository scan, воспроизводимый cold build, Cloudflare staging
hello-world без секретов, approved threat/key decisions.

### Phase 1. Cloudflare Platform Skeleton

- scaffold Rust workspace, React frontend и one-origin Workers deployment;
- настроить Static Assets SPA fallback и Worker-first `/api/*` routes;
- добавить Access JWT verification adapter и local fake identity test adapter;
- создать D1 migrations, R2/DO/Queue bindings и scheduled reconciler;
- добавить correlation/problem details, idempotency и safe structured logs;
- настроить Rust tests, Cloudflare integration harness и Playwright smoke.

**Gate:** browser surface открывается только через Access; Bridge route без
valid intent/device proof ничего не раскрывает; SPA/API same-origin;
D1/R2/DO/Queue binding tests проходят на production Worker build.

### Phase 2. Identity, Clients И ACL Vertical Slice

- owner bootstrap и owner-transfer invariant;
- invitations/memberships/revoke;
- client cards/contact encryption/search token;
- profile metadata без browser payload;
- assignment и profile/client grants;
- audit/outbox и owner/member UI;
- IDOR/cross-tenant/forbidden-action acceptance.

**Gate:** owner создает client/profile/member/grant только через UI; member видит
только разрешенные resources; direct endpoint abuse не раскрывает данные.

### Phase 3. Profile Coordinator И Device Trust

- DO profile state machine: lease, fencing, heartbeat, idle/hard TTL;
- launch intent redemption и replay protection;
- Bridge device enrollment, CNG/DPAPI key и signed challenge;
- short-lived app tokens, revoke и lost-device flow;
- duplicate/delayed writer, DO eviction и partition tests;
- UI devices/sessions/owner drain.

**Gate:** старый writer после lease turnover не может commit generation; revoke
немедленно запрещает новые intents; DO restart восстанавливает protocol state.

### Phase 4. Windows Profile Bridge И Local Lifecycle

- signed/bootstrap development installer и custom protocol;
- embedded Camouhost/Python/Camoufox runtime bundle;
- safe path/materialization, OS lock и process-tree supervisor;
- local SQLite cache/outbox, quota и dirty retention;
- graceful close, forgotten-window policy и crash recovery on clone;
- doctor/support bundle без secrets/PII.

**Gate:** на Windows profile create/open/close работает без CLI; browser locks не
удаляются; offline close сохраняет recoverable dirty state.

### Phase 5. Cloud Generations

- deterministic inventory, compact snapshot и safe archive extraction;
- envelope encryption/key versioning;
- immutable R2 upload, verification Queue и D1 pointer CAS;
- restore, rollback, orphan reconciliation и retention;
- presigned/temporary access с минимальными scope/TTL;
- disaster/key restore drill на чистом environment.

**Gate:** create -> sync -> remove local clone -> restore -> replay проходит;
corrupt/truncated archive не активируется; delayed fenced writer отклоняется.

### Phase 6. Fingerprint Certification

- реализовать ADR-0001 signal taxonomy/policy manifest;
- фиксировать effective browser/OS/GPU/fonts/network/runtime tuple;
- запускать first-party и специализированные third-party checks;
- хранить sanitized evidence и versioned score decision;
- drift/expiry/quarantine/re-certification;
- матрица headful/virtual-headful и approved proxy/network policies.

**Gate:** ни один runtime lane не получает production label без repeatability,
coherence, uniqueness и replay evidence. Абсолютная «невидимость» не обещается.

### Phase 7. Multi-Device И Release System

- второй независимый Windows device;
- wrapped DEK delivery и device revocation;
- signed runtime/Bridge bundles, SBOM, staged rollout и rollback;
- compatibility matrix generation/runtime/device;
- cloud-only prefetch/eviction и conflict recovery UX;
- auto-update health/rollback telemetry.

**Gate:** generation переносится на второй authorized PC без постоянных cloud
credentials; revoked device не unwrap-ит новые keys; failed update откатывается.

### Phase 8. Mailbox Operations

- provider-neutral mailbox domain;
- OAuth/IMAP adapters через secret handles;
- browser-assisted fallback как отдельный adapter;
- bounded cursor, retry/rate limit и safe message metadata;
- owner/operator UI, audit и retention.

**Gate:** mailbox job не раскрывает token/content в logs/UI; revoke и retry
предсказуемы; provider contract suite проходит.

### Phase 9. Production Operations

- Cloudflare environments, gradual deploy и rollback;
- D1 backup/Time Travel/export policy и restore drill;
- R2 inventory/retention/deletion/reconciliation;
- Access break-glass и account recovery;
- SLO dashboards, alerts, queue DLQ и cost limits;
- incident, credential rotation, lost device и corrupt generation runbooks;
- Windows code signing и stable installer/update channel.

**Gate:** staging game day восстанавливает D1 catalog, keys и R2 generation;
operator может локализовать failure по correlation ID без чтения secrets.

### Phase 10. CRM Integration

- зафиксировать v1 contracts/events/capabilities;
- заменить Access identity adapter на CRM OIDC semantics;
- заменить D1 catalog adapter на PostgreSQL/SQLx + `FORCE RLS` при необходимости;
- переключить Client Registry на CRM Party projection;
- выполнить dual-read shadow comparison и controlled cutover;
- сохранить R2/Profile Bridge/runtime contracts.

**Gate:** CRM не получает direct R2/profile filesystem access; parity tests
доказывают одинаковые domain decisions на D1 и PostgreSQL adapters.

## 14. Первый Vertical Slice

Первый slice намеренно не запускает Camoufox:

1. Workers Static Assets + Rust Worker staging deployment;
2. Cloudflare Access login и local fake identity для tests;
3. D1 migrations и owner bootstrap;
4. client card;
5. profile metadata + client assignment;
6. member invitation/membership + profile grant;
7. owner/member React screens;
8. audit, idempotency, forbidden-access tests;
9. CI deploy preview/staging и rollback.

Так проверяются hosting, identity, domain boundaries, D1 isolation и UI прежде,
чем к системе добавится дорогой browser lifecycle.

## 15. Quality Gates

| Слой | Обязательные проверки |
|---|---|
| Domain | unit, property, state-machine, forbidden dependencies |
| D1 | migrations, FK/unique, optimistic race, tenant/IDOR negative suite |
| DO | concurrency, eviction, stale fencing, duplicate/reordered commands |
| R2 | corruption, immutable create, retry, orphan reconcile, restore |
| Security | Access JWT, membership revoke, device proof, secret/PII scan |
| Bridge | Windows process tree, locks, offline dirty, crash recovery |
| Browser | clone-only replay, runtime compatibility, certification matrix |
| UI | owner/member E2E, accessibility, responsive, failure states |
| Release | cold build, SBOM/signature, staged update and rollback |
| Recovery | D1/R2/key restore on clean environment |

## 16. SLO И Alerts

Первичные targets уточняются после load baseline, но измеряются с первого slice:

- API availability и p95 latency по route class;
- launch-intent redeem success/latency;
- session heartbeat age и stuck session count;
- sync completion latency по profile size;
- Queue age, retry и DLQ count;
- D1 errors/contention и DO alarm failures;
- R2 verification/reconciliation failures;
- Bridge/runtime version lag и update rollback rate;
- certification drift/quarantine count.

Alerts создаются для stale lease, dirty-local без heartbeat, queue backlog,
generation verification failure, repeated authorization denial, key-provider
failure и updater signature failure.

## 17. Обязательные Внешние Gates

- Cloudflare account/project с Access, Workers, D1, DO, Queues и R2;
- production domain и separate staging environment;
- explicit approval перед отзывом/ротацией существующих credentials;
- trusted Windows code-signing certificate до stable release;
- второй Windows-native host до multi-device promotion;
- approved key recovery policy и offline escrow;
- выбранные certification sites и юридически допустимая test policy;
- Cloudflare plan/limits/cost budget, подтвержденные load test.

## 18. Риски И Ответы

| Риск | Контроль |
|---|---|
| Vendor lock-in Cloudflare | ports, pure domain, versioned contracts, CRM adapters |
| D1 без RLS | single-tenant v1, typed scope, default deny, IDOR suite |
| Cross-service partial failure | idempotency, immutable objects, outbox, reconciliation |
| Key loss | versioned hierarchy, offline escrow, restore drill |
| Stolen device | proof key, revoke, short-lived token, no bucket credential |
| Forgotten browser | native warning, idle timeout, hard TTL, dirty retention |
| Fingerprint drift | versioned runtime/policy, certification and quarantine |
| Bad auto-update | signed side-by-side canary, compatibility gate, rollback |
| Cloud outage | local dirty retention, bounded offline mode, retry outbox |
| Future CRM mismatch | native Rust domain reuse and adapter parity tests |

## 19. Definition Of Done Продукта

Без CLI owner может войти, добавить member, создать client/profile, назначить
client, выдать grant, enroll/revoke device, открыть/закрыть/sync profile,
просмотреть generations/certification/mailbox/audit и восстановить generation.

Member видит только grants. Профиль cloud-backed и доступен с другого
authorized Windows PC после materialization. Забытое окно закрывается policy без
потери dirty data. Обновления подписаны и откатываемы. D1/R2/keys восстанавливаются
по проверенному runbook. CRM integration не требует переписывать domain rules,
profile payload или Bridge protocol.

## 20. Официальные Технические Основания

- [Cloudflare Workers Rust support](https://developers.cloudflare.com/workers/languages/rust/)
- [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/)
- [Cloudflare Access identity providers](https://developers.cloudflare.com/cloudflare-one/integrations/identity-providers/)
- [D1 limits](https://developers.cloudflare.com/d1/platform/limits/)
- [Durable Objects rules](https://developers.cloudflare.com/durable-objects/best-practices/rules-of-durable-objects/)
- [R2 temporary credentials](https://developers.cloudflare.com/r2/api/s3/temporary-credentials/)
- [Workers testing](https://developers.cloudflare.com/workers/testing/)
