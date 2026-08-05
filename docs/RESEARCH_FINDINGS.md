# Проверенные Выводы Исследования

**Статус:** verified baseline
**Дата:** 2026-08-05
**Назначение:** зафиксировать наблюдаемые факты до начала реализации
**Не является:** обещанием production-ready или гарантией обхода anti-fraud

## 1. Область Проверки

Проверялись:

- текущий репозиторий `part-crm-emai-profile`;
- архитектурные и инфраструктурные паттерны `part_crm`;
- runtime/workflow-паттерны `customer-service`;
- локальный runtime `camoufox`;
- только профили из `temp/browser_profiles`.

Профили не запускались. Содержимое cookies, логинов, email и IndexedDB не
читалось. Во время первоначального structural audit команда
`sqlite3 -readonly ... PRAGMA quick_check` создала или обновила 311 служебных
`*.sqlite-shm` файлов во всех 22 source-профилях. В окне изменения других файлов
не обнаружено, но утверждение о полностью неизменном filesystem tree было
ошибочным.

Следствие: SQLite нельзя открывать даже с `-readonly` непосредственно на source
профиле. Любая integrity-проверка выполняется только на snapshot/clone; source
сканируется filesystem API без открытия browser databases.

## 2. Rust И Toolchain

Rust edition и версия компилятора являются разными координатами:

```toml
[package]
edition = "2024"
rust-version = "1.97.1"
```

```toml
[toolchain]
channel = "1.97.1"
profile = "minimal"
components = ["cargo", "clippy", "rustfmt"]
```

Целевой проект использует Rust 2024 и exact compiler 1.97.1. Это совместимо с
родительским CRM и обеспечивает воспроизводимость. Плавающий `stable` не
считается достаточным pin.

Обнаруженный drift в источниках повторного использования:

- `part_crm` указывает `stable`, а не exact `1.97.1`;
- `customer-service` не содержит `rust-toolchain.toml`;
- локальный `camoufox` во время проверки использовал Rust `1.95.0`.

Кодовые паттерны из этих проектов можно использовать, но toolchain нового
репозитория должен задаваться независимо.

## 3. Legacy Browser Profiles

Проверено 22 профиля общим размером 2,278,707,379 байт.

| Свойство | Результат |
|---|---:|
| Browser `152.0.4-beta.27` | 16 профилей |
| Browser `152.0.4-beta.28` | 6 профилей |
| SQLite databases | 548 |
| Успешный `PRAGMA quick_check` | 548 |
| Профили с cookies/key4/logins/prefs/storage | 22 |
| Профили с `sessionstore.jsonlz4` | 20 |
| Профили с `camoufox_config.json` | 1 |
| Профили с `.parentlock` и `lock` | 22 |

Единственный сохраненный fingerprint config содержит UA Firefox 150 при
browser profile 152. Это может быть допустимой особенностью генератора или
несогласованностью, но без provenance и live evidence безопасно доказать одно
из двух нельзя.

Следствия:

1. Все 22 профиля импортируются как `legacy` и `provenance_unknown`.
2. Отсутствие `sessionstore.jsonlz4` у двух профилей не означает потерю cookies
   или storage state.
3. Наличие lock-файла не разрешает его удаление. Сначала проверяются PID,
   hostname, открытые descriptors и lease.
4. Legacy-профиль нельзя автоматически обновлять на другой browser patch.
5. Сертификация выполняется только на copy-on-write клоне.

## 4. Фактический Размер Данных

Крупнейшие классы данных:

| Класс | Размер | Политика |
|---|---:|---|
| `storage/` | 715,997,234 байт | включать в recovery snapshot |
| `cache2/` | 644,833,508 байт | local ephemeral |
| `startupCache/` | 614,457,244 байт | local ephemeral |
| `places.sqlite` | 115,343,360 байт | включать |
| `favicons.sqlite` | 115,343,360 байт | включать по policy |
| `cookies.sqlite` | 11,534,336 байт | обязательно включать |
| `logins.db` и `key4.db` | 14,417,920 байт | обязательно включать и шифровать |

`cache2` и `startupCache` вместе занимают около 55% набора. Они не нужны в
обычном recovery snapshot и пересоздаются браузером. Для forensic snapshot
может существовать отдельная policy, включающая все файлы после graceful close.

`storage/` исключать нельзя: в нем находятся localStorage, IndexedDB, extension
state и другие данные, необходимые для авторизации и поведения профиля.

## 5. Выбранный Runtime Baseline

Основной lane для новых профилей:

| Компонент | Pin |
|---|---|
| Python | `3.12.3` |
| Camoufox official | `0.5.4` |
| Browser | `official/stable/152.0.4-beta.28` |
| BrowserForge | `1.2.4` |
| Apify fingerprint datapoints | `0.13.0` |
| Playwright | `1.59.0` |

Compatibility lane:

| Компонент | Pin |
|---|---|
| Cloverlabs Camoufox | `0.5.5` |
| Browser | `152.0.4-beta.27` |
| Назначение | legacy compatibility only |

Clover package помечен как experimental и не выбран источником новых
production-профилей.

Runtime bundle ID должен включать как минимум:

- package name и version;
- browser channel, exact version и binary digest;
- BrowserForge и datapoints versions;
- Playwright version;
- OS image, fonts bundle и virtual display policy;
- GeoIP database digest;
- proxy policy version;
- fingerprint policy version;
- Camouhost contract version и commit.

## 6. Live Synthetic Canary

Проверки выполнялись на свежих синтетических профилях. Пользовательские профили,
почта и proxy credentials не использовались.

### 6.1 Специализированные Сайты

| Checker | Результат |
|---|---|
| BrowserLeaks Canvas/WebGL/WebRTC | `100/100`, grade `A` |
| WebRTC | `no_leak` |
| CreepJS | `90/100`, grade `A` |
| CreepJS warning | `headless_like_percent` |

Эти результаты подтверждают жизнеспособность upstream official lane, но не
являются полной production-сертификацией. Checker implementations и внешние
сайты меняются, поэтому каждый отчет должен содержать timestamp, checker
version, runtime bundle ID и raw sanitized evidence digest.

### 6.2 Generation И Replay

- создано 32 fingerprint snapshot;
- получено 32 уникальных signature;
- коллизий в canary cohort не обнаружено;
- snapshot успешно сохранен и загружен через текущий `ProfileStore`;
- core fingerprint vector совпал в двух холодных запусках;
- localStorage воспроизвелся;
- persistent cookie с expiry воспроизвелся;
- canvas hash между холодными запусками не совпал.

Canvas меняется не из-за потери профиля. Текущий Camouhost удаляет
`canvas:seed`, `audio:seed`, font spacing seed и часть других значений как
session noise. Это поведение требует изменения и отдельной политики, описанной
в ADR-0001.

### 6.3 Encrypted R2 Profile Replay

Отдельный 2026-08-05 smoke test создал новый профиль вне legacy scope, выполнил
ручную авторизацию, encrypted upload в R2, восстановление в новый локальный
каталог и повторный headful запуск.

Подтверждено:

- S3 `PUT/LIST/GET/DELETE` canary с проверкой SHA-256;
- client-side encrypted immutable generations;
- safe restore с проверкой AEAD, archive digest и file inventory;
- исключение `cache2`, `startupCache`, locks и `*.sqlite-shm`;
- сохранение авторизации после cloud restore, подтвержденное пользователем;
- побайтовое совпадение 48 закрепленных Camoufox config-параметров;
- одинаковый rendered fingerprint probe в трех запусках;
- automatic close-to-sync после исправления OS process supervision.
- exact restored generation: BrowserLeaks `100/A`, CreepJS `90/A` с открытым
  warning `headless_like_percent` на disposable unsynced clone.

Не подтверждено этим тестом:

- multi-device restore;
- полный набор runtime fingerprint surfaces;
- внешний checker score именно этого авторизованного профиля;
- network/TLS/IP coherence при смене устройства;
- crash-consistent checkpoint открытого браузера;
- concurrent-writer fencing и conditional catalog update.

Полный отчет: [`CLOUD_PROFILE_SMOKE_TEST.md`](CLOUD_PROFILE_SMOKE_TEST.md).

## 7. Состояние Локального Camouhost

Полезные части:

- Rust daemon как публичный control plane;
- Python worker как единственный слой, знающий Camoufox/Playwright;
- protobuf contracts;
- persistent context и profile generation/replay;
- checker registry и doctor harness;
- session lifecycle и typed commands.

Блокирующие проблемы перед production reuse:

1. Rust daemon не компилируется из-за ссылки на отсутствующую переменную
   `install_verification` в health path.
2. `pyproject.toml` и official runtime config ожидают Camoufox `0.4.11`, тогда
   как установлен и проверен `0.5.4`.
3. Текущий `ProfileStore` не имеет path validation, atomic writes, lease,
   fencing, quarantine и безопасного delete.
4. Public protobuf переносит raw proxy username/password вместо secret handle.
5. Headless doctor может зависнуть без внешнего bounded deadline.
6. Документация RPC surface расходится с фактическим `ManageProfile` RPC.

Camouhost следует стабилизировать и выпускать как отдельный versioned runtime
artifact. Он не должен становиться authoritative profile catalog.

## 8. Повторное Использование

### `customer-service`

Использовать как эталон:

- canonical layering;
- application ports;
- BrowserRuntimePort и gRPC adapter;
- lease epoch и fencing token;
- provider selection, quarantine и certification;
- content-addressed artifact store;
- scorecards и fail-closed release gates.

Не переносить весь workspace и site-specific workflow engine.

### `part_crm`

Использовать как эталон:

- strict schema-bound manifests;
- safe object keys;
- R2/Queue recovery patterns;
- checksum verification;
- object restore gate;
- retention, DLQ и replay canary.

Не дублировать его Communications domain. Browser Profile Service не становится
владельцем CRM-писем и Contact Points.

### `camoufox`

Использовать через protobuf/gRPC process boundary. Не импортировать Python или
Rust runtime implementation напрямую в доменное ядро приложения.

## 9. Почтовые Адаптеры

Предпочтительный порядок:

1. Gmail API с OAuth для Gmail.
2. IMAP с application password для Mail.ru.
3. Browser adapter только как управляемый fallback или для сценария, который
   невозможно выполнить через provider API.

Browser automation не должна быть единственным способом проверки почты. Полное
содержимое сообщений в первой версии не дублируется без отдельного требования;
хранятся mailbox binding, check job, минимальные observations и ссылки на
владельца Communications.

## 10. Ограничение Доказательств

Нельзя гарантировать, что профиль:

- абсолютно неотличим от любого физического устройства;
- никогда не будет обнаружен anti-fraud системой;
- сохранит одинаковый score после изменения внешнего checker;
- безопасно воспроизведется на произвольной новой версии browser/runtime.

Можно доказуемо обеспечить:

- согласованность fingerprint внутри зафиксированной policy;
- отсутствие случайного drift между разрешенными запусками;
- уникальность в измеренной cohort;
- storage replay;
- network/geo coherence;
- versioned evidence и воспроизводимую сертификацию.

## 11. Внешние Источники

- [Camoufox persistent data и Python usage](https://camoufox.com/python/usage/)
- [Camoufox и BrowserForge](https://camoufox.com/python/browserforge/)
- [Camoufox GeoIP](https://camoufox.com/python/geoip/)
- [Playwright persistent browser context](https://playwright.dev/python/docs/api/class-browsertype)
- [Cloudflare R2 consistency](https://developers.cloudflare.com/r2/reference/consistency/)
- [Cloudflare R2 limits](https://developers.cloudflare.com/r2/platform/limits/)
- [Cloudflare R2 data security](https://developers.cloudflare.com/r2/reference/data-security/)
- [Cloudflare R2 bucket locks](https://developers.cloudflare.com/r2/buckets/bucket-locks/)
- [Cloudflare R2 object lifecycles](https://developers.cloudflare.com/r2/buckets/object-lifecycles/)
- [Gmail API guides](https://developers.google.com/workspace/gmail/api/guides)
- [Mail.ru IMAP settings](https://help.mail.ru/mail/login/mailer/)
