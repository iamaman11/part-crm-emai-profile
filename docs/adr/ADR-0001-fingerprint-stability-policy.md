# ADR-0001: Политика Стабильности Fingerprint

**Статус:** accepted
**Дата:** 2026-08-05
**Cross-device clarification:** 2026-08-31
**Решение требуется до:** включения генерации production-профилей

## Контекст

Текущий Camouhost сохраняет стабильную часть Camoufox config, но удаляет ряд
ключей как session noise. В live canary core fingerprint совпал между двумя
холодными запусками, а canvas hash изменился.

Случайное изменение canvas/audio/fonts при каждом старте противоречит модели
долгоживущего browser profile. Одновременно требование сделать абсолютно каждый
browser-сигнал неизменяемым тоже неверно: реальные браузеры имеют время,
performance metrics, history length, media state и другие естественно
динамические значения.

Нужна явная и проверяемая политика, а не обещание «один и тот же fingerprint»
без определения сравниваемых сигналов.

## Решение

Каждый fingerprint-сигнал относится ровно к одному классу. Классификация
versioned и входит в runtime bundle ID.

### 1. Profile-Stable

Значение создается один раз и не меняется между разрешенными запусками одного
profile generation:

- UA family и browser major, согласованные с runtime;
- `navigator.platform`, OS family и architecture hints;
- hardware concurrency и device memory;
- screen geometry, color depth и device pixel ratio;
- WebGL vendor, renderer и capability envelope;
- базовый fonts bundle;
- locale family;
- touch/device capability model;
- profile-level BrowserForge config signature.

Изменение profile-stable значения требует создания нового generation и
повторной сертификации. In-place mutation запрещена.

### 2. Origin-Deterministic

Значение стабильно для пары profile + origin, но может отличаться между
origin. Оно выводится из profile entropy root:

```text
signal_seed = HMAC(profile_entropy_root, policy_version || origin || signal)
```

К этому классу целевым образом относятся:

- canvas noise;
- audio noise;
- font spacing noise;
- другие anti-fingerprinting seeds, если runtime позволяет безопасно задавать
  их детерминированно.

Такой подход сохраняет идентичность профиля на повторных визитах одного сайта и
не требует глобально одинакового raw fingerprint для всех origin.

Если текущая версия Camoufox не поддерживает origin-scoped derivation, первая
реализация фиксирует seed на уровне profile generation. Случайная регенерация
при каждом запуске запрещена.

### 3. Network-Bound

Значение зависит от выбранной network identity и меняется только атомарно вместе
с proxy binding generation:

- public IP;
- timezone;
- geolocation;
- WebRTC public address;
- Accept-Language/locale overlay, если это предусмотрено geo policy.

Нельзя изменить только один элемент группы. Перед навигацией выполняется
network preflight, проверяющий coherence всей группы.

### 4. Session-Dynamic

Значения, которые естественно меняются при каждом запуске и не входят в identity
hash:

- wall-clock и monotonic time;
- performance timings;
- history length;
- focus/visibility state;
- временные media permissions и enumerated devices;
- network latency;
- process IDs и memory pressure;
- ephemeral browser cache.

Session-dynamic signals контролируются диапазонами и coherence rules, но не
сравниваются на полное равенство.

## Cross-Device Reproducibility Contract

Цель платформы — не привязать browser profile к одному физическому компьютеру и
не решать drift запретом второго устройства. Один и тот же подтвержденный
`profile generation` должен воспроизводиться на другом авторизованном supported
Windows device через тот же pinned Camoufox runtime, если runtime способен
представить сохраненную browser-visible identity независимо от физических
характеристик нового host.

**Нормальный продуктовый результат — успешный перенос и запуск на другом
совместимом устройстве.** Fail-closed отказ существует только для необъяснимого
drift или доказанной неспособности конкретного runtime/host воспроизвести
принятый контракт; он не является целью или основным способом обеспечить
стабильность.

Binding target:

```text
server-selected exact generation
-> exact encrypted generation restore
-> typed Profile-Stable identity
-> canonical Camoufox config projection
-> exact pinned runtime
-> host/runtime compatibility admission
-> Camoufox masking/virtualization
-> typed browser-visible observation
-> exact policy comparison
-> navigation allowed
```

Физический monitor, DPI, GPU, installed fonts, Windows user, speech catalog,
audio devices или другое host-состояние не получают права молча переписать
identity существующего generation. Приоритет реализации:

1. воспроизвести сохраненное значение средствами Camoufox/config/runtime masking;
2. проверить фактически наблюдаемое browser-visible значение до пользовательской
   навигации;
3. только если pinned runtime доказанно не умеет безопасно представить требуемую
   identity на данном host, вернуть явный incompatible-host/recovery outcome.

`IncompatibleHost` — fail-closed граница безопасности, а не штатная стратегия
переносимости. Supported Windows acceptance должен стремиться доказать успешный
cross-device запуск на неодинаковых физических hosts, а не объявлять одинаковое
железо обязательным условием.

### Один semantic owner identity

`browser-execution-domain` остается natural semantic owner browser identity.
Нельзя создавать отдельный Windows/fingerprint registry или updater-owned copy.
Целевая typed identity должна описывать policy-relevant значения, а не только
непрозрачный общий hash.

S0 Windows shipping/recovery может реализовать недостающую composition/runtime
closure, необходимую для clean-host/cross-device запуска, только через этот
существующий semantic owner. Windows delivery state, Release Set или updater не
получают права определять fingerprint значения, выбирать новое browser identity
или создавать parallel compatibility policy.

Для generation canonical identity material должен однозначно определять как
минимум:

- browser/OS/architecture identity и UA coherence;
- hardware capability model (`hardwareConcurrency`, `deviceMemory`,
  `maxTouchPoints` и применимые browser-visible capability fields);
- display identity: width/height, available geometry, color/pixel depth,
  device-pixel-ratio и те window/display fields, которые текущая policy
  классифицирует как profile-stable;
- graphics identity: WebGL vendor/renderer плюс применимый extensions,
  parameters, shader-precision и context-attribute capability envelope;
- fonts identity и deterministic font-spacing seed;
- deterministic canvas/audio identity inputs;
- locale/language identity;
- применимые stable speech/media/input capability values;
- fingerprint policy/schema version и exact runtime compatibility identity.

Конкретный persisted representation может эволюционировать, но должен иметь
один canonical owner. `camoufox-config.json` является runtime projection этой
identity, а его SHA-256 — integrity evidence, не самостоятельный semantic owner.
Aggregate probe SHA также является evidence и не заменяет typed comparison.

### Host compatibility не является browser identity

Физический host проверяется отдельным compatibility admission. Минимально
значимы supported Windows/runtime class, architecture, display/DPI environment,
graphics backend, execution/display mode, clock sanity и необходимые
filesystem/process capabilities.

Host observation не должен попадать в generation как новая identity только
потому, что профиль открылся на другом ПК. Если физический host отличается, но
Camoufox воспроизводит ту же browser-visible identity, запуск совместим.

Запрещено:

- менять сохраненные screen/DPR/WebGL/fonts/audio/canvas значения под новый host;
- требовать одинаковую модель монитора/GPU как основной portability contract;
- автоматически создавать новый fingerprint/config при restore;
- fallback на другой runtime/config при несовпадении;
- считать общий probe hash достаточным, если policy-relevant поля не были
  классифицированы и проверены.

### Execution surface

`Headful`, `VirtualHeadful` и будущий отдельно сертифицированный execution mode
являются частью compatibility surface. Существующий generation нельзя молча
переключать на режим, для которого не доказана эквивалентность его
browser-visible identity.

Для первого интерактивного Windows release предпочтительный путь — real
headful. Virtual-headful может использоваться только при отдельной доказанной
совместимости с той же generation identity.

### Profile-state portability

Browser identity и переносимость browser state — разные контракты.
Generation snapshot должен переносить обычное browser-owned состояние, для
которого принята portability guarantee: cookies, localStorage, IndexedDB и
применимое persistent Firefox profile state.

Machine/device-bound материалы должны классифицироваться явно:

```text
PORTABLE
REBIND_REQUIRED
DEVICE_BOUND_UNSUPPORTED
```

Например, device authentication key и proxy credential могут требовать нового
разрешения/привязки на устройстве и не являются частью browser fingerprint.
Platform passkeys, Windows Hello, hardware-backed keys, client-certificate
private keys и native integrations нельзя обещать как portable без отдельного
поддерживаемого механизма. Их наличие не разрешает молча менять browser identity.

## Profile Entropy Root

Каждый новый профиль получает 256-bit cryptographically random entropy root.
Он:

- генерируется CSPRNG;
- хранится только через `KeyProviderPort` по ADR-0006: локальный Secret Vault
  допустим для one-device smoke, production требует accepted recovery policy;
- в metadata представлен opaque secret handle;
- не попадает в protobuf events, audit, logs или R2 manifest;
- никогда не используется повторно другим profile ID;
- ротируется только созданием нового generation.

Fingerprint snapshot в R2 содержит уже материализованные разрешенные значения
или зашифрованные derivation metadata, но не открытый entropy root.

## Runtime И Browser Upgrades

Browser version является частью fingerprint identity. Upgrade выполняется как
controlled migration:

1. создать clone предыдущего generation;
2. применить новый immutable runtime bundle;
3. пересчитать только policy-разрешенные browser-bound значения;
4. выполнить consistency и specialized-site certification;
5. сравнить evidence с предыдущим generation;
6. активировать новый generation атомарно или отклонить migration.

Автоматический silent upgrade запрещен.

## Certification Gates

### Intra-Profile Consistency

- не менее 10 холодных запусков;
- profile-stable vector совпадает во всех запусках;
- origin-deterministic vector совпадает для каждого origin;
- network-bound vector согласован с proxy policy;
- storage marker, persistent cookie и localStorage воспроизводятся;
- отсутствует cross-profile storage contamination.

### Cross-Device Reproducibility

Для supported Windows portability evidence требуется минимум:

1. создать/подтвердить generation на Host A;
2. сохранить browser state marker и exact generation identity;
3. восстановить тот же encrypted generation на независимо авторизованном Host B;
4. использовать тот же exact accepted runtime/config identity;
5. доказать совпадение typed Profile-Stable browser-visible vector до
   пользовательской навигации;
6. доказать восстановление persistent cookie/localStorage и применимого
   generation state;
7. отдельно доказать network-bound coherence для route, выбранного на Host B.

Host B должен по возможности отличаться от Host A физическими display/DPI/GPU
или Windows-user характеристиками, чтобы acceptance доказывал virtualization и
portability, а не случайное равенство среды. Repository CI проверяет контракт,
negative cases и deterministic projections; claim о реальной физической
cross-device совместимости требует соответствующего Windows-host evidence.

Repository-owned S0 closure должна сделать этот путь code-complete и fail-closed
на одном shipping runtime owner. V2/B10 и соответствующий внешний Windows-host
evidence затем доказывают реальный Host A -> Host B сценарий на exact candidate;
они не должны впервые обнаруживать отсутствующий portability contract.

### Inter-Profile Uniqueness

- cohort не менее 100 синтетических профилей перед первым release;
- collision profile config signature равен нулю;
- одинаковые редкие комбинации анализируются как возможная generator anomaly;
- распределения screen, WebGL, hardware и fonts проверяются на правдоподобие, а
  не только на уникальность.

### Specialized Sites

- CreepJS;
- BrowserLeaks Canvas, WebGL и WebRTC;
- BrowserScan;
- Pixelscan;
- Sannysoft;
- EFF Cover Your Tracks, если условия и стабильность сайта допускают automation.

Результат внешнего сайта является time-bound evidence, а не вечной истиной.
Release gate требует отсутствия fail-сигналов и явного рассмотрения warnings.

## Headless Policy

Обычный headless режим не является production default. Live canary получил
`headless_like_percent` warning в CreepJS.

Production-порядок:

1. real headful для интерактивной работы;
2. virtual-headful только после доказанной fingerprint-equivalence для
   применимого runtime/generation contract;
3. native headless только после отдельной сертификации target site и runtime
   bundle.

## Ответ На Вопрос О Решении Проблемы

Да, проблема решаема как engineering problem:

- сохранять полную typed identity policy, а не случайный subset config;
- перестать случайно менять canvas/audio/font seeds между рестартами;
- использовать Camoufox masking/virtualization для воспроизведения сохраненной
  identity на другом compatible device;
- отделять host compatibility и network identity от browser identity;
- привязать изменения к generation и runtime bundle;
- проверять typed browser-visible consistency автоматически до навигации;
- блокировать только unexplained drift или доказанно несовместимый host, не
  запрещая cross-device запуск как продуктовую модель.

Не решаемой является только абсолютная гарантия «никогда не будет обнаружен».
Такую гарантию нельзя достоверно дать ни для Camoufox, ни для физического
браузера.

## Последствия

- текущая функция, удаляющая все session-noise seeds, должна быть переработана;
- snapshot/identity schema получает signal classes, policy version и typed
  profile-stable identity evidence;
- существующий `BrowserIdentityManifest` должен эволюционировать без создания
  второго owner и связывать canonical config, typed stable identity и exact
  runtime compatibility;
- runtime probe должен покрывать policy-relevant stable surface, а не только
  несколько агрегированных сигналов;
- host environment наследуется только через минимальный bounded runtime contract;
  browser-visible locale/timezone/display/network semantics приходят от своих
  natural owners, а не случайно от Windows host;
- certification service становится обязательным release gate;
- legacy-профили без entropy provenance не получают статус certified без
  canary-клонирования и baseline capture;
- любая смена policy создает новую certification lineage.
