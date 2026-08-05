# ADR-0001: Политика Стабильности Fingerprint

**Статус:** proposed
**Дата:** 2026-08-05
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

## Profile Entropy Root

Каждый новый профиль получает 256-bit cryptographically random entropy root.
Он:

- генерируется CSPRNG;
- хранится только через Secret Vault/KMS port: локальный Secret Vault допустим
  для one-device smoke, production multi-device использует выбранный key manager;
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
2. virtual-headful с fingerprint-sized Xvfb для unattended jobs;
3. native headless только после отдельной сертификации target site и runtime
   bundle.

## Ответ На Вопрос О Решении Проблемы

Да, проблема решаема как engineering problem:

- сохранять полную identity policy, а не случайный subset config;
- перестать случайно менять canvas/audio/font seeds между рестартами;
- привязать изменения к generation и runtime bundle;
- проверять consistency автоматически;
- блокировать запуск профиля при unexplained drift.

Не решаемой является только абсолютная гарантия «никогда не будет обнаружен».
Такую гарантию нельзя достоверно дать ни для Camoufox, ни для физического
браузера.

## Последствия

- текущая функция, удаляющая все session-noise seeds, должна быть переработана;
- snapshot schema получает signal classes и policy version;
- certification service становится обязательным release gate;
- legacy-профили без entropy provenance не получают статус certified без
  canary-клонирования и baseline capture;
- любая смена policy создает новую certification lineage.
