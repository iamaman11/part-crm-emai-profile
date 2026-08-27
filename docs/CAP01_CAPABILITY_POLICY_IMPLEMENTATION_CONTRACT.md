# CAP-01 Capability Policy Implementation Contract

**Status:** BINDING_FOR_DRAFT_IMPLEMENTATION  
**Tracking issue:** #492  
**Draft PR:** #494  
**Production authorization:** NONE  
**Provider/staging/production mutation:** FORBIDDEN  

This contract preserves the accepted implementation instructions for CAP-01. The issue remains the tracker/change-envelope boundary; this document is the durable versioned implementation contract. GitHub current authority, accepted architecture constraints, executable code, and exact-head CI outrank stale chat context.

## 1. Итоговая цель

Создать один независимый pure Rust crate `capability-policy`, который является единственным semantic owner для:

- каталога `ActivationUnit`;
- dependency и incompatibility graph;
- известных Capability Profile;
- profile inheritance/composition;
- разрешённых environments;
- profile semantic identity/digest;
- правил production authorization admission;
- отображения `RuntimeSurface -> ActivationUnit`;
- вычисления effective capabilities.

Все остальные части системы становятся потребителями:

```text
                     capability-policy
                     pure Rust library
                            ▲
           ┌────────────────┼────────────────┐
           │                │                │
  control-plane        resolver         opsctl-core
     Worker             Worker               │
           │                │              opsctl
           │                │         CLI/adapters/rendering
           └────────────────┼────────────────┘
                            │
                generated manifest
                            │
                     Release Set v3
                            │
                 authenticated projection
                            │
                        frontend
```

Запрещённые зависимости:

```text
capability-policy -> Worker             запрещено
capability-policy -> Cloudflare         запрещено
capability-policy -> opsctl             запрещено
capability-policy -> opsctl-core        запрещено

Worker -> opsctl                         запрещено
Worker -> opsctl-core                    запрещено
resolver -> opsctl-core                  запрещено

opsctl-core -> capability-policy         разрешено
Worker -> capability-policy              разрешено
resolver -> capability-policy            разрешено
```

## 2. Что означает «гибкость»

CAP-01 должен поддерживать три разных временных уровня.

### 2.1 Развитие исходного кода

Через обычный PR можно:

- добавить новую capability;
- добавить dependency;
- добавить новую runtime surface;
- создать новый профиль;
- оставить новую capability выключенной во всех production profiles;
- тестировать source-present код в CI;
- выпускать Release Set с кодом, который пока нельзя активировать.

### 2.2 Неизменность выпущенной семантики

После использования profile ID в immutable Release Set:

```text
one profile ID
=
one immutable profile semantics
```

Нельзя изменить состав `production-core-v1` и оставить прежний ID.

Если меняется:

- enabled set;
- disabled set;
- dependency closure;
- allowed environments;
- inheritance;
- semantic digest scope;

создаётся новый ID, например:

```text
production-core-v2
production-mailbox-admin-v2
production-analytics-v1
```

Product scope и Capability Profile принадлежат разным natural owners, поэтому изменение product
boundary требует явного handoff, а не изменения старого ID по месту:

```text
accepted product capability change
-> selected effective-set impact disposition
-> immutable existing profile preserved
-> new versioned profile when semantics differ
-> atomic current-selector cutover
-> exact effective-set + disabled-ingress proof
```

Наличие старого profile в каталоге или immutable Release Set разрешает historical verification, но не
создаёт fallback и не делает его current selector.

### 2.3 Выбор при deployment

Deployment может выбрать только известный profile:

```text
Release Set
+ known profile ID
+ matching semantic digest
+ canonical environment
+ valid authorization state
```

Deployment не может создать новый профиль из JSON, environment variables или CLI arguments.

## 3. Что CAP-01 не должен становиться

Не создавать:

- generic feature-flag service;
- tenant/cohort targeting;
- percentage rollout;
- dashboard-managed capability registry;
- plugin system;
- DI container;
- общий app-core;
- mutable policy database;
- remote policy service;
- JSON-конфигурацию, из которой runtime строит правила;
- `ENABLE_X`, `SHOW_X`, `VITE_ENABLE_X`;
- отдельные production rules в GitHub Actions или Wrangler scripts.

Если в будущем понадобится tenant/cohort rollout, это должна быть отдельная подчинённая политика:

```text
profile_allowed
AND rollout_allowed
AND subject_authorized
```

Rollout policy сможет дополнительно запрещать, но никогда не сможет разрешить то, что запрещено Capability Profile. Реализовывать rollout сейчас не нужно.

## 4. Физическая структура crate

Удалить оба `#[path = ".../capability-policy/src/lib.rs"]`.

Создать полноценный workspace crate:

```text
crates/capability-policy/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── activation.rs
    ├── profile.rs
    ├── admission.rs
    ├── surface.rs
    ├── identity.rs
    └── snapshot.rs
```

Это рекомендуемое разбиение, не обязательное публичное API. Не следует дробить каждый enum в отдельный crate.

### Cargo.toml

Crate должен:

- быть `publish = false`;
- использовать workspace edition/rust-version/lints;
- не зависеть от worker;
- не зависеть от `serde_json`;
- не содержать filesystem/network/process/provider dependencies;
- при необходимости зависеть только от pure libraries, например `sha2`.

В корневом workspace:

```toml
[workspace]
members = [
    # ...
    "crates/capability-policy",
]

[workspace.dependencies]
capability-policy = { path = "crates/capability-policy" }
```

В обоих Worker:

```toml
[dependencies]
capability-policy.workspace = true
```

В `tools/opsctl/core/Cargo.toml`:

```toml
[dependencies]
capability-policy = { path = "../../../crates/capability-policy" }
```

`opsctl-core` может иметь эту зависимость: это narrow pure dependency, а не превращение `opsctl-core` в application core.

## 5. Публичная модель capability-policy

Pure evaluator не должен принимать `worker::Env`, JSON или сырые строки.

Предпочтительная модель:

```rust
pub enum ActivationUnit { /* ... */ }

pub enum CanonicalEnvironment {
    Rehearsal,
    Staging,
    Production,
}

pub enum ProfileId { /* ... */ }

pub enum RuntimeSurface { /* ... */ }

pub struct ProfileDigest([u8; 32]);

pub struct AdmissionRequest {
    pub environment: CanonicalEnvironment,
    pub profile_id: ProfileId,
    pub presented_digest: ProfileDigest,
    pub authorization: AuthorizationState,
}

pub struct EffectiveProfile {
    pub profile_id: ProfileId,
    pub semantic_digest: ProfileDigest,
    pub capabilities: EffectiveCapabilities,
}

pub enum AdmissionDenial {
    DigestMismatch,
    EnvironmentNotAllowed,
    ProductionNotAuthorized,
    DependencyUnsatisfied { unit: ActivationUnit, dependency: ActivationUnit },
    IncompatibleCapabilities { left: ActivationUnit, right: ActivationUnit },
    ProfileInheritanceCycle,
}
```

Raw parsing выполняется на adapter boundary:

```text
Worker Env strings
-> parse CanonicalEnvironment/ProfileId/ProfileDigest
-> AdmissionRequest
-> capability-policy::admit()
```

И аналогично в opsctl:

```text
strict JSON/CLI DTO
-> typed conversion
-> AdmissionRequest
-> capability-policy::admit()
```

### 5.1 Типизированные gate IDs

Не оставлять критичные gate IDs свободными строками:

```rust
pub enum ActivationGate {
    Ar12OrLaterRehearsal,
    Pc1AfterAr17,
    TargetAuthorization,
    ProductionAuthorization,
    Pc2,
    Pc3,
    Pc4,
}
```

Строковое представление используется только при manifest/CLI rendering.

### 5.2 EffectiveCapabilities

Не использовать `u16`, зависящий от порядка enum.

Предпочтительно:

```rust
pub struct EffectiveCapabilities(BTreeSet<ActivationUnit>);
```

При десятках capabilities разница в производительности несущественна, а преимущества важны:

- нет скрытого потолка в 16 элементов;
- порядок enum не меняет значение;
- проще тестировать и отображать;
- проще добавлять новые capabilities;
- не требуется ручное управление битами.

Если bitset всё же сохраняется, discriminants должны быть явными и стабильными, а размер — с большим запасом. Но для текущего проекта `BTreeSet` понятнее.

## 6. Каталог capabilities

Каждая capability должна иметь одно определение:

```rust
pub struct CapabilityDefinition {
    pub unit: ActivationUnit,
    pub dependencies: &'static [ActivationUnit],
    pub incompatible_with: &'static [ActivationUnit],
    pub requires_windows_profile_bridge: bool,
}
```

Нужно исключить возможность забыть добавить enum variant в `ALL_ACTIVATION_UNITS`.

Допустим маленький локальный macro, который из одного объявления генерирует enum, `ALL_ACTIVATION_UNITS`, stable string ID, definition/dependencies и parse/display mapping. Он не должен быть generic framework или экспортируемой DSL. Если macro ухудшает читаемость, можно оставить enum + exhaustive match, но полнота каталога должна быть доказана тестом и compile-time exhaustiveness.

### 6.1 Полная валидация графа

Валидация каталога должна отдельно проверять:

- уникальность string IDs;
- dependency не указывает на самого себя;
- все dependencies существуют;
- dependency graph не содержит циклов;
- incompatibility не указывает на себя;
- incompatibility policy симметрична либо явно документирована как направленная;
- все runtime surfaces отображаются на известную capability.

Нельзя ограничиваться проверкой «если A включена, то B тоже включена»: цикл `A -> B -> A`, где обе включены, такая проверка не обнаруживает.

## 7. Capability Profiles

Каждый профиль должен быть типизированной immutable definition:

```rust
pub struct CapabilityProfileDefinition {
    pub id: ProfileId,
    pub version: u16,
    pub allowed_environments: &'static [CanonicalEnvironment],
    pub extends: Option<ProfileId>,
    pub enabled: &'static [ActivationUnit],
    pub disabled: &'static [ActivationUnit],
    pub activation_gate: ActivationGate,
    pub production_authorization_required: bool,
}
```

Профиль должен проходить:

- проверку inheritance cycle;
- проверку неизвестного parent;
- проверку enable/disable overlap;
- вычисление effective set;
- dependency closure;
- incompatibility validation;
- environment admission.

### 7.1 Почему сохраняются enabled и disabled

Для текущего digest v1 важен точный semantic document, включающий обе коллекции. Поэтому нельзя «упростить» старые profiles до одного effective set, если это изменит существующие digests.

Новые profile definitions должны придерживаться той же versioned digest discipline либо получить новую версию digest algorithm.

## 8. Semantic digest

Текущий вариант с hardcoded digest рядом с profile definition не является завершённым решением.

Нужно определить точный versioned scope:

```text
ProfileSemanticIdentityV1:
- profile_id
- profile_version
- allowed_environments
- extends, если есть
- enabled_activation_units
- disabled_activation_units
```

Не включать в profile digest:

- текущий BLOCKED/AUTHORIZED;
- временные blockers;
- observed provider state;
- Release Set ID;
- deployment resources;
- GitHub issue status.

Иначе одинаковый профиль будет менять identity при каждом изменении program state.

### 8.1 Алгоритм

Должен существовать один pure алгоритм:

```text
typed ProfileSemanticIdentityV1
-> deterministic canonical bytes
-> SHA-256
-> ProfileDigest
```

Он должен сохранить все пять существующих digest byte-for-byte.

Не использовать `serde_json::Value`.

Допустимы два подхода:

1. Ручной детерминированный encoder, точно воспроизводящий текущий stable JSON v1.
2. Сериализация строго типизированной структуры с зафиксированной canonicalization implementation.

Первый вариант лучше сохраняет pure/core boundary и не тащит generic JSON model в runtime.

### 8.2 Обязательные digest tests

Сохранить отдельный exact digest vector для пяти исторически использованных v1 profiles:

- `production-core-v1`;
- `rehearsal-core-v1`;
- `production-mailbox-admin-v1`;
- `production-mailbox-jobs-v1`;
- `production-outbound-mail-v1`.

Новые versioned profiles получают собственный frozen vector; для текущего first-release cutover это:

- `production-core-v2`;
- `rehearsal-core-v2`.

Дополнительно:

- изменение enabled set меняет digest;
- изменение disabled set меняет digest;
- изменение environment меняет digest;
- изменение parent меняет digest;
- перестановка, если порядок не является семантикой, нормализуется;
- другой profile ID меняет digest;
- current authorization state digest не меняет.

## 9. Production authorization

Capability profile определяет, требуется ли production authorization и какой activation gate должен быть удовлетворён.

Runtime не должен получать разрешение через `PRODUCTION_AUTHORIZED=true` или `ENABLE_PRODUCTION=true`.

До принятого target-authorization wiring Worker adapter должен всегда передавать:

```text
AuthorizationState::NotAuthorized
```

Позднее изменение authorization должно быть отдельной governed транзакцией, основанной на принятом evidence/Release Set/promotion boundary.

Важно:

```text
profile semantics immutable
authorization state mutable и внешний по отношению к digest
```

Изменение `NotAuthorized -> Authorized` не требует нового profile ID/version. Изменение состава
профиля требует нового versioned ID.

## 10. RuntimeSurface

`RuntimeSurface` должен отражать не bounded context, а конкретную исполняемую поверхность:

- HTTP route family;
- Queue consumer;
- Scheduled dispatcher;
- Resolver ingress;
- Resolver reconciliation;
- Bridge command;
- Camoufox launch.

Health/liveness, which must remain observable when Capability Profile configuration is absent or
invalid and which performs no product business side effect, is explicitly outside the governed
`RuntimeSurface` catalog. Its availability and response contract remain owned by the health ingress.

Policy crate владеет:

```text
RuntimeSurface -> ActivationUnit
```

Worker adapter владеет:

```text
RouteClass/path -> RuntimeSurface
```

Policy не должен зависеть от `control-plane-contract::RouteClass`.

### 10.1 Control-plane Worker

При входе в каждый event один раз построить capability context:

```rust
pub struct RuntimeCapabilityContext {
    effective_profile: EffectiveProfile,
}
```

Не следует многократно читать одни и те же env variables для каждой проверки.

Поток:

```text
Worker env
-> typed RuntimeCapabilityContext
-> resolve RuntimeSurface
-> require(surface)
-> only then use case / D1 / R2 / Queue / provider
```

Для HTTP:

```text
route classification
-> surface mapping
-> capability admission
-> authorization/use-case/effects
```

Для Queue:

```text
message classification
-> queue surface
-> capability admission
-> consume/mutate/provider
```

Для Scheduled:

```text
scheduled dispatcher surface
-> capability admission
-> D1 query/dispatch/enqueue
```

Cloudflare рассматривает fetch, queue и scheduled как отдельные handlers, поэтому наличие HTTP gate не защищает остальные entrypoints.

### 10.2 Queue ack/retry не относится к policy

`capability-policy` решает только `ALLOWED / DENIED`.

Что делать с запрещённым queue message — ack, retry, delay или DLQ — решает owning queue adapter/lifecycle policy.

Но во всех случаях должно быть доказано:

```text
DENIED
=> D1/provider/secret/outbound effect count = 0
```

### 10.3 Resolver Worker

Resolver обязан выполнить admission до:

- body authentication, если она требует bindings/crypto state;
- nonce claim;
- D1 lookup;
- secret resolution;
- OAuth refresh;
- provider call;
- reconciliation mutation.

Для fetch denial сохранить стабильный fail-closed response, например существующий `503 resolver_capability_unavailable`.

Для scheduled denial:

- no-op;
- structured diagnostic;
- никаких D1/provider effects.

Не следует просто проглатывать все ошибки через `unwrap_or(false)` без возможности диагностировать причину. Наружу можно возвращать общий безопасный код, но внутренний log должен различать:

- missing config;
- unknown environment;
- unknown profile;
- wrong digest;
- unauthorized production;
- disabled surface.

### 10.4 Profile Bridge/Camoufox

Необходимо отдельно доказать отсутствие обхода через replayed/previously-created device command.

Допустимы два результата исследования:

1. Bridge может выполнять только job/command, который Worker повторно авторизует при claim/lease, и profile transition делает старые команды неисполняемыми.
2. Bridge имеет самостоятельный executable ingress и должен также зависеть от `capability-policy` либо проверять существующий подписанный capability execution grant.

Нельзя просто добавить `BridgeCamoufoxLaunch` в enum и оставить без реального consumer/enforcement point.

## 11. opsctl integration

### 11.1 opsctl-core

`opsctl-core` использует `capability-policy` для:

- проверки существования profile ID;
- вычисления effective capabilities;
- environment admission;
- digest validation;
- dependency/incompatibility validation;
- production authorization admission;
- release/profile compatibility.

`opsctl-core` не должен заново определять profiles, capability graph, environment rules или digest algorithm.

### 11.2 opsctl shell/adapters

`opsctl` shell отвечает за:

- filesystem;
- строгий JSON decode;
- CLI arguments;
- canonical JSON rendering;
- exact file hashing;
- manifest writing to stdout/file boundary;
- Release Set artifact observation.

Разрешённый поток:

```text
external bytes
-> strict DTO
-> typed values
-> opsctl-core / capability-policy
-> typed result
-> JSON/human output
```

Запрещён:

```text
serde_json::Value
-> capability-policy evaluator
```

### 11.3 Удаление старого opsctl owner

Из `tools/opsctl/src/release/authority.rs` удалить:

- локальные ActivationUnit;
- локальные ReleaseProfile;
- local effective-profile evaluator;
- local dependency graph validation;
- profile inheritance implementation.

В файле могут остаться release/deployment/promotion DTO и adapters, если они имеют отдельную естественную ответственность.

## 12. Generated capability manifest

Manifest является исторической проекцией, а не входом policy.

Рекомендуемый контракт:

```json
{
  "kind": "CAPABILITY_POLICY_MANIFEST",
  "schema_version": 1,
  "activation_units": [],
  "profiles": [],
  "runtime_surfaces": []
}
```

Каждая profile row содержит:

- profile_id;
- profile_version;
- semantic_digest;
- allowed_environments;
- extends;
- enabled_activation_units;
- disabled_activation_units;
- activation_gate;
- production_authorization_required.

Не включать текущий provider state, secrets, временный GitHub status, deployment selection или mutable production authorization verdict.

### 12.1 Generation boundary

Предпочтительно:

```text
capability-policy::snapshot_v1()
-> opsctl output adapter
-> canonical capability-policy-v1.json
```

Не создавать tracked generated manifest в репозитории, если durable consumer — только Release Set. Генерировать его в release build staging directory.

### 12.2 Release Set integration

Новый release build:

1. генерирует canonical manifest;
2. считает exact byte SHA-256 и size;
3. добавляет manifest в artifact_inventory;
4. добавляет asset в GitHub Release;
5. при повторной публикации того же Release Set проверяет byte equality;
6. promotion/release verification проверяет manifest как обычный content-addressed artifact.

Например:

```text
path = capability-policy-v1.json
kind = capability_policy_manifest
sha256 = ...
size_bytes = ...
```

### 12.3 Manifest никогда не является законом

Запрещено:

```text
read capability-policy-v1.json
-> построить effective profile
-> разрешить deployment/runtime
```

Разрешено:

```text
capability-policy crate
-> принимает решение

manifest
-> подтверждает, какие semantics были упакованы
```

Для старых Release Set v3, у которых manifest ещё отсутствует:

- сохранить существующую историческую читаемость;
- не переписывать immutable bytes;
- не требовать manifest задним числом;
- новый writer после CAP-01 всегда должен его включать.

Не нужно автоматически вводить Release Set v4 только из-за добавления обычного artifact inventory entry.

## 13. Architecture JSON и Node cleanup

Из `architecture/release-architecture-ar11.json` удалить ручные:

- activation_units;
- release_profiles;
- runtime surface -> activation unit mapping, если его owner уже Rust.

Оставить только обязанности release architecture:

- deployment closures;
- provider resources/bindings;
- artifact authority;
- promotion policy;
- component release owners;
- release inputs;
- compatibility dimensions;
- metadata о том, что capability policy принадлежит Rust crate/manifest.

Из `.github/scripts/release-architecture-ar11.mjs` удалить:

- `activationDigest`;
- `validateActivationUnits`;
- `validateProfiles`;
- Rust source string inspection;
- сравнение digest через поиск строк в `.rs`;
- dependency/profile self-tests, дублирующие Rust evaluator.

Node gate может продолжить проверять workflow wiring, deployment closure, отсутствие запрещённых bindings в Core, publication structure и `production mutation remains blocked`.

Capability semantics проверяются actual Rust crate tests, а не Node-копией.

## 14. Frontend

Frontend продолжает получать только authenticated effective projection:

```json
{
  "profile_id": "rehearsal-core-v2",
  "profile_digest": "...",
  "capabilities": ["foundation", "identity", "clients"]
}
```

Правила:

- projection создаётся backend из `EffectiveProfile`;
- клиент не присылает capabilities обратно как authorization;
- local storage не является authority;
- Vite variables не включают capability;
- frontend скрывает navigation/actions;
- backend всё равно блокирует прямой HTTP вызов;
- неизвестная capability в projection обрабатывается fail-safe.

## 15. Тестовая матрица

### 15.1 capability-policy unit tests

Обязательно:

- все unit IDs уникальны;
- все profile IDs уникальны;
- string parse/display round-trip;
- неизвестный environment отклоняется;
- неизвестный profile отклоняется;
- dependency на отсутствующую capability отклоняется;
- self-dependency отклоняется;
- dependency cycle отклоняется;
- profile inheritance cycle отклоняется;
- enable/disable overlap отклоняется;
- отсутствующая dependency в effective set отклоняется;
- incompatibility отклоняется;
- wrong digest отклоняется;
- profile в неправильном environment отклоняется;
- production без authorization отклоняется;
- rehearsal profile в staging разрешается;
- пять исторических v1 digest остаются byte-exact, а каждый новый versioned profile имеет отдельный
  frozen digest vector;
- semantic change меняет digest;
- authorization state не меняет profile digest;
- все runtime surfaces имеют canonical activation unit.

### 15.2 Worker tests

Для каждой группы surface:

```text
DENIED
-> effect probe count = 0
```

Проверить:

- HTTP create/update;
- send mail;
- mailbox jobs API;
- queue mailbox job;
- replayed outbound intent;
- scheduled mailbox dispatch;
- scheduled notifications;
- resolver fetch;
- resolver reconciliation;
- bridge/runtime command, если он имеет самостоятельный execution path.

Тест должен проверять вызовы fake ports/adapters, а не наличие строки в source.

### 15.3 opsctl tests

Проверить:

- Worker и opsctl получают одинаковый effective set;
- release finalizer берёт profile IDs из crate;
- wrong digest блокирует preflight;
- unknown profile блокирует;
- profile/environment mismatch блокирует;
- production authorization отсутствует — блокирует;
- Release Set содержит manifest artifact;
- manifest hash mismatch блокирует;
- missing manifest блокирует для нового writer candidate;
- старый immutable v3 остаётся читаемым;
- manifest deterministic на Linux и Windows.

### 15.4 Manifest golden tests

Нужны:

- canonical byte golden vector;
- exact SHA-256 golden vector;
- повторная генерация даёт те же bytes;
- pretty rendering, если существует, не используется для digest;
- изменение profile semantics меняет canonical bytes;
- порядок map/set не влияет, если порядок не является семантикой.

## 16. Observability

Каждый runtime denial должен давать structured diagnostic без secrets, например:

```json
{
  "event": "capability_admission",
  "environment": "staging",
  "profile_id": "rehearsal-core-v2",
  "surface": "queue.mailbox_jobs.consumer",
  "decision": "DENIED",
  "reason": "CAPABILITY_DISABLED",
  "release_set_id": "..."
}
```

Допустимые reason codes:

```text
UNKNOWN_ENVIRONMENT
UNKNOWN_PROFILE
DIGEST_MISMATCH
ENVIRONMENT_NOT_ALLOWED
PRODUCTION_NOT_AUTHORIZED
CAPABILITY_DISABLED
DEPENDENCY_UNSATISFIED
INCOMPATIBLE_CAPABILITIES
```

Не логировать secret values, OAuth tokens, raw signed resolver envelopes или полные provider responses.

## 17. Протокол добавления новой capability

Например, появляется Analytics:

1. Добавить bounded-context/domain/use-case код.
2. Добавить `ActivationUnit::Analytics`.
3. Указать dependencies/incompatibilities.
4. Добавить все side-effecting `RuntimeSurface`.
5. Поставить admission до первого effect.
6. Не включать capability в существующий production profile.
7. При необходимости создать новый rehearsal profile.
8. Добавить positive/negative tests.
9. Сгенерировать новый manifest через обычный build.
10. Выпустить новый Release Set.
11. Только после отдельного решения создать production profile нового ID.

Правильная последовательность:

```text
code lands
-> capability exists
-> CI tests it
-> capability absent from active production profile
-> runtime execution impossible
-> new profile reviewed
-> new Release Set/profile compatibility
-> promotion admission
```

## 18. Протокол изменения профиля

Если profile ещё нигде не опубликован и не принят как durable compatibility identity, изменение возможно в текущем PR.

После durable использования:

```text
не менять существующий profile
-> создать новый ProfileId
-> получить новый digest
-> включить только в новые Release Sets
```

Никаких aliases, silent replacement, «v1, но с новым смыслом» или compatibility fallback.

Старый profile сохраняется, пока существует реальный rollback/promotion consumer. Он удаляется только после доказанного отсутствия текущих consumers.

## 19. Протокол удаления capability

Удаление проходит в обратном порядке:

1. создать новые profiles без capability;
2. прекратить новые activations;
3. отключить producers/jobs/routes;
4. обработать pending queue/job/data lifecycle;
5. убедиться, что current и known-good Release Sets её не требуют;
6. удалить execution surfaces;
7. удалить implementation;
8. удалить catalog entry;
9. удалить historical support только после отсутствия rollback consumer.

Нельзя сначала удалить enum и оставить replayable queue messages или provider side effects.

## 20. Важное ограничение для one main

Capability Policy не заменяет совместимость данных и shared code.

Даже если capability выключена, изменения в main должны соблюдать:

- expand/migrate/contract discipline для D1;
- backward compatibility общих API/contracts;
- отсутствие module-initialization effects;
- совместимость Worker bundle;
- совместимость shared domain/use-case code;
- безопасный rollout queue message schemas;
- forward/backward compatibility для rollback.

То есть:

```text
capability OFF
=> её execution surfaces запрещены
```

но не:

```text
=> любые её migrations/shared changes безопасны
```

## 21. Рекомендуемый порядок commits в Draft

Можно работать несколькими проверяемыми commits, но merge должен быть один атомарный cutover.

1. `capability-policy: establish real crate boundary`
   - Cargo.toml;
   - workspace membership;
   - normal dependencies;
   - удалить `#[path]`.
2. `capability-policy: complete typed graph and digest semantics`
   - graph validation;
   - typed IDs/gates/digest;
   - all parity and negative tests.
3. `runtime: cut over all capability admission surfaces`
   - HTTP;
   - Queue;
   - Scheduled;
   - resolver;
   - bridge proof where applicable.
4. `opsctl: consume canonical capability policy`
   - core dependency;
   - release/promotion cutover;
   - удалить local evaluator.
5. `release: generate and bind capability manifest`
   - renderer;
   - artifact inventory;
   - publication;
   - verification.
6. `architecture: retire duplicated JSON/Node authority`
   - удалить profiles/units;
   - удалить JS semantic evaluator/drift checks;
   - обновить permanent gates/docs.

Перед переводом PR в Ready commits можно squash/reorganize, чтобы итоговая история не представляла `#[path]` как принятую архитектуру.

## 22. Definition of Done

CAP-01 готов только когда на одном exact head одновременно истинно:

```text
real capability-policy Cargo crate                    true
Product Runtime -> opsctl/opsctl-core                 false
Worker/resolver -> capability-policy                  true
opsctl-core -> capability-policy                      true

manual profile definitions in Worker                  0
manual profile definitions in opsctl                  0
manual profile definitions in architecture JSON       0
Node capability/profile evaluator                     0
#[path] shared Rust source                            0

all current profile digests preserved                 true
digest derived from typed semantic identity           true
dependency and inheritance cycle rejection            true
HTTP/Queue/Scheduled/resolver fail-before-effect       true

generated capability-policy-v1.json                   true
manifest bound in Release Set artifact inventory      true
manifest used as semantic input                       false
frontend independent authorization                    false

old immutable Release Sets rewritten                  false
production capability enabled by CAP-01               false
provider/staging/production mutation                  false

applicable Linux/Windows/WASM CI                       green
blocking reviews                                      0
unresolved threads                                    0
behind_by                                              0
```

Главное: законченная система должна быть расширяемой через добавление типизированных сущностей, но закрытой для произвольного runtime-конструирования правил. Именно это даст проекту свободу развития без возвращения к нескольким владельцам и набору несвязанных feature flags.
