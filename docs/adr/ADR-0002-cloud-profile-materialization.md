# ADR-0002: Облачный Профиль С Локальной Материализацией

**Статус:** accepted, smoke-tested on one device
**Дата:** 2026-08-05
**Решение требуется до:** реализации R2 adapter

## Контекст

Firefox profile состоит из множества SQLite databases, WAL-файлов, IndexedDB,
small files, symlinks и browser locks. Во время работы браузер выполняет частые
random writes и ожидает локальные filesystem semantics.

Cloudflare R2 является strongly consistent object storage, но не POSIX
filesystem. Попытка смонтировать R2 как live `user_data_dir` добавит latency,
сломает корректность locks и создаст риск повреждения profile state.

При этом приложение должно позволять пользователю работать с профилями,
хранящимися в облаке, без ручного скачивания и загрузки.

## Решение

Система поддерживает cloud-backed profiles полностью на уровне пользовательского
жизненного цикла, но browser runtime всегда работает с локально
материализованным generation.

```text
R2 immutable generation
  -> download to staging
  -> decrypt and verify
  -> safe unpack
  -> atomic local activation
  -> acquire lease and launch browser
  -> graceful close
  -> create next immutable generation
  -> encrypt and upload
  -> verify remote object
  -> atomically advance catalog pointer
```

Это модель `cloud-backed, locally executed`. Локальным диском может быть диск
пользовательской машины или ephemeral disk удаленного browser worker.

## Пользовательские Возможности

Для cloud-only профиля приложение поддерживает:

- list и search без скачивания payload;
- просмотр metadata и certification status;
- materialize и open одной командой;
- background prefetch;
- graceful close и автоматический cloud sync;
- восстановление предыдущего generation;
- запуск на другом authorized worker;
- локальное eviction после подтвержденного sync;
- offline продолжение уже материализованного профиля;
- retry sync после восстановления сети.

Пользователь не обязан управлять архивами вручную.

## Состояния Generation

```text
CLOUD_ONLY
  -> MATERIALIZING
  -> LOCAL_READY
  -> LEASED
  -> DIRTY_LOCAL
  -> QUIESCING
  -> SNAPSHOTTING
  -> UPLOADING
  -> SYNCED
  -> EVICTABLE
```

Ошибочные состояния:

- `CORRUPT_LOCAL`;
- `CORRUPT_REMOTE`;
- `LEASE_CONFLICT`;
- `RUNTIME_INCOMPATIBLE`;
- `QUARANTINED`;
- `SYNC_RETRY_PENDING`.

Переходы задаются доменной state machine и фиксируются в audit/outbox.

## Что Хранится Локально

### Обязательно Во Время Работы

- полный активный `user_data_dir`;
- SQLite/WAL и IndexedDB;
- extension state;
- browser locks;
- текущие downloads и temporary files;
- `cache2`, `startupCache` и другие runtime caches;
- staging restore и staging snapshot;
- lease/fencing metadata, не являющаяся browser payload.

### Допустимый Hot Cache

- активный generation;
- предыдущий last-known-good generation;
- prefetched cloud generation;
- rebuildable browser caches;
- незавершенный encrypted upload с bounded TTL.

Локальный cache управляется quota и eviction policy. Dirty или unsynced
generation удалять запрещено.

## Что Хранится В R2

Каждый generation имеет immutable prefix:

```text
profiles/v1/<tenant_id>/<profile_id>/<generation_id>/
  manifest.pb
  profile.tar.zst.enc
  certification.pb
  inventory.blake3
```

Tenant ID, profile ID и generation ID являются safe opaque segments. Email,
provider login и display name не входят в object key.

R2 хранит:

- encrypted compact recovery snapshot;
- versioned manifest;
- ciphertext and logical inventory digests;
- runtime bundle reference;
- fingerprint policy reference;
- sanitized certification report;
- optional full forensic snapshot по отдельной retention policy.

## Compact Snapshot Policy

Обязательно включаются:

- `storage/**`;
- `cookies.sqlite` и согласованный WAL, если он остался после close;
- `key4.db`, `logins.db`;
- `prefs.js`;
- permissions, certificates и security state;
- extension manifests и extension storage;
- service worker state;
- session restore state;
- compatibility metadata.

Исключаются как rebuildable:

- `cache2/**`;
- `startupCache/**`;
- thumbnails;
- crash reports;
- temporary downloads;
- `.parentlock`, `lock`, `parent.lock`;
- `*.sqlite-shm` как transient coordination sidecars;
- transient sockets и process metadata.

Exclusion list versioned и записывается в manifest. Неизвестный top-level path
по умолчанию включается или отправляется на manual classification, но не
молча удаляется.

## Snapshot Protocol

1. Получить profile lease и fencing token.
2. Перевести generation в `QUIESCING`.
3. Выполнить typed drain и graceful browser close.
4. Подтвердить отсутствие browser process и открытых descriptors.
5. Не удаляя locks, проверить, что browser освободил их корректно.
6. Выполнить read-only integrity checks.
7. Создать deterministic inventory.
8. Упаковать snapshot в staging.
9. Зашифровать client-side envelope encryption.
10. Загрузить по новому immutable key с conditional create.
11. Проверить remote size, checksum и decryptable canary range/full restore.
12. Записать manifest и certification evidence.
13. D1 compare-and-set активирует catalog pointer при актуальном fencing token.
14. Только после этого разрешить eviction предыдущего local generation.

## Restore Protocol

1. Скачать manifest и ciphertext в новый staging directory.
2. Проверить schema, runtime compatibility, size и digest.
3. Расшифровать потоково.
4. Распаковать с защитой от path traversal, absolute paths и symlink escape.
5. Проверить inventory и SQLite integrity.
6. Создать новый local generation ID.
7. Выполнить atomic rename staging -> generation.
8. Обновить local catalog transactionally.
9. Запускать только после lease и runtime preflight.

Restore никогда не перезаписывает активный каталог на месте.

## Конкуренция И Конфликты

Один profile generation имеет одного writer. Защита состоит из:

- profile Durable Object lease;
- monotonically increasing lease epoch;
- opaque fencing token;
- OS-level profile lock;
- conditional object create;
- immutable generation keys.

Last-write-wins для mutable object key запрещен. Два независимых dirty
generation не сливаются автоматически. Один из них помещается в conflict branch
и требует явного выбора.

## Шифрование И Секреты

R2 server-side encryption используется как дополнительный слой. До upload весь
profile archive шифруется приложением, потому что cookies и local credentials
эквивалентны секретам.

- per-generation Data Encryption Key;
- key wrapping через `KeyProviderPort` по ADR-0006;
- AEAD authenticated streaming format;
- key ID без самого ключа в manifest;
- отдельные права read/write/delete;
- короткоживущие credentials и audit доступа.

## Retention

- хранить последние N verified generations;
- minimum recovery window задается policy;
- bucket lock использовать ограниченно, не бессрочно для персональных данных;
- lifecycle удаляет superseded generations только после catalog reconciliation;
- delete profile создает auditable deletion workflow и удаляет local keys,
  wrapped DEKs и remote objects согласно legal/retention policy.

## Ответ На Вопрос О Полноте Cloud Support

Для пользователя поддержка облачных профилей будет полной: cloud-only profile
можно открыть, изменить, синхронизировать, восстановить и продолжить на другом
authorized Windows Profile Bridge.

Технически исполнение всегда частично локальное, потому что Firefox не может
безопасно работать непосредственно из R2. Это не функциональное ограничение, а
обязательный слой корректности. Аналогично Git хранит историю объектами, но
checkout материализует рабочее дерево на диске.

Smoke test подтвердил lifecycle на одном устройстве. Доступ с любого компьютера
появится только после реализации device enrollment, remote key wrapping,
short-lived object credentials, single-writer lease и exact runtime bundle
distribution. Локальный Secret Vault текущего прототипа намеренно не является
multi-device key service.

## Последствия

- требуется достаточный local disk quota хотя бы для одного generation и
  staging archive;
- первый запуск cloud-only профиля зависит от download latency;
- prefetch и last-known-good cache улучшают UX;
- network outage не теряет dirty local state, но блокирует eviction;
- cloud sync становится отдельным надежным workflow с retry/outbox;
- R2 adapter не должен быть доступен domain crates напрямую.
