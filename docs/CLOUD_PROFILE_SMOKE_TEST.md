# Cloud Profile Smoke Test

**Статус:** passed with documented limitations
**Дата:** 2026-08-05
**Scope:** новый synthetic profile вне `temp/browser_profiles`

## Цель

Проверить реальным headful browser lifecycle:

```text
create -> user authorization -> graceful close -> encrypt -> R2 upload
  -> download -> verify -> decrypt -> safe restore -> replay -> resync
```

Тест не открывал и не изменял 22 legacy-профиля.

## Runtime И Storage

| Компонент | Фактическое значение |
|---|---|
| Camoufox | `0.5.4` official |
| Browser | `152.0.4-beta.28` |
| BrowserForge | `1.2.4` |
| Playwright | `1.59.0` |
| Режим | WSLg headful |
| Object store | Cloudflare R2, EEUR placement |
| Bucket access | bucket-scoped Object Read/Write |
| Profile ID | opaque UUID, не email |
| Encryption | client-side chunked AES-256-GCM, HKDF per generation |

Cloudflare bootstrap, provisioning и S3 credentials хранятся только в Secret
Vault. Browser subprocess не наследует R2 credential или encryption key.

## Результаты

1. R2 S3 canary выполнил `PUT`, `LIST`, `GET`, SHA-256 verification и `DELETE`.
2. После пользовательской авторизации создан первый immutable encrypted
   generation: 91 файл, около 35.7 MB ciphertext.
3. Generation скачан, AEAD и SHA-256 проверены, archive безопасно распакован в
   новый каталог, все 91 inventory entry совпали.
4. Восстановленный профиль открылся без повторной авторизации. Результат
   подтвержден пользователем.
5. После replay создан второй generation: 90 файлов, около 17.7 MB ciphertext.
6. Current pointer восстановил именно второй generation в следующий новый
   каталог.
7. Автоматический eight-second headful replay завершился без пользователя,
   создал и remote-verified третий generation.
8. Current pointer третьего generation повторно восстановлен, 90 файлов
   прошли inventory verification.

Во всех materializations fingerprint config совпал побайтно. Проверены 48
закрепленных Camoufox параметров в категориях:

- navigator и headers;
- screen и window;
- canvas и audio seeds;
- fonts и font-spacing seed;
- voices;
- WebGL и WebGL2;
- locale и timezone;
- geolocation и WebRTC binding;
- media devices;
- addons.

Rendered probe совпал в трех headful запусках и включал UA/platform/languages,
hardware, screen, timezone, WebGL и canvas. Это сильное подтверждение replay,
но не доказательство всех браузерных поверхностей.

Exact third-generation restore дополнительно проверен на disposable headful
clone, который не синхронизировался обратно:

| Checker | Результат | Сигналы |
|---|---:|---|
| BrowserLeaks | `100/100`, grade `A` | `webrtc_no_leak` |
| CreepJS | `90/100`, grade `A` | warning `headless_like_percent` |

CreepJS warning остается открытым ограничением WSLg/текущего runtime lane и
должен быть разобран до production promotion.

## Snapshot Policy

В encrypted snapshot включены cookies, credential databases, preferences,
storage, IndexedDB, extension state и session state. Не включены:

- `cache2`;
- `startupCache`;
- crash/minidump data;
- browser locks;
- `*.sqlite-shm`.

WAL не исключается. Неизвестный файл по умолчанию включается.

## Найденный Дефект

При ручном закрытии WSLg-окна Playwright дважды оставил stale `context.pages`,
хотя OS browser process уже завершился. Первый и второй sync были завершены
recovery-командой после доказанной quiescence.

Controller исправлен: завершение определяется комбинацией Playwright state,
`TargetClosedError` и исчезновения OS process, использующего конкретный
`user_data_dir`. После исправления unattended close-to-sync прошел end-to-end.

Production supervisor должен владеть process handle напрямую, а не сканировать
`/proc`; smoke-реализация остается исследовательским инструментом.

## Что Еще Нужно Доказать

- audio output, font metrics, media codecs, permissions, WebRTC candidates,
  WebGPU и другие runtime surfaces отдельными probes;
- TLS/HTTP2/client-network coherence;
- 10 cold starts и drift classification;
- 100-profile uniqueness cohort;
- BrowserScan и Pixelscan для exact generation;
- controlled crash, power loss, disk full и interrupted multipart upload;
- concurrent writer rejection, lease epoch и fencing;
- restore на другом физическом устройстве;
- key loss, rotation, device revocation и disaster recovery.

До этих gates формулировка «все fingerprint-параметры воспроизводятся» не
используется. Подтверждено: authorization replay, exact stored config replay и
ограниченный rendered probe replay.
