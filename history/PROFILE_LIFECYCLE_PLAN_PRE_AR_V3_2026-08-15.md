# Целевой Жизненный Цикл Browser Profile

**Статус:** normative target
**Связанные решения:** ADR-0001, ADR-0002, ADR-0003, ADR-0004, ADR-0005;
ADR-0006 остается production key-management gate

Перед любой profile-командой application устанавливает проверенный tenant/actor
context и применяет актуальный `ProfileAccessGrant`. Client assignment является
бизнес-связью и никогда не заменяет authorization grant.

## 1. Агрегат И Поколения

`BrowserProfile` является долгоживущим агрегатом. Его изменяемое browser state
хранится как последовательность immutable `ProfileGeneration`.

```text
Profile P
  generation G1 imported
  generation G2 certified clone
  generation G3 updated after session
  generation G4 migrated to a new runtime bundle
```

Активный указатель меняется атомарно. Старое поколение не перезаписывается и
может использоваться для rollback в пределах retention policy.

## 2. Состояния Профиля

```text
DISCOVERED
  -> IMPORTED_UNVERIFIED
  -> QUARANTINED
  -> CERTIFYING
  -> READY
  -> MATERIALIZING
  -> IN_USE
  -> DIRTY_LOCAL
  -> SYNCING
  -> READY
  -> SUSPENDED
  -> DELETING
  -> DELETED
```

`QUARANTINED` не означает потерю данных. Профиль существует, но production open
запрещен до устранения причины.

## 3. Создание Нового Профиля

1. Application проверяет owner permission и optional active client assignment.
2. Application создает opaque profile ID и generation ID.
3. `KeyProviderPort` создает versioned profile entropy root/seed handle.
4. Runtime registry выбирает certified runtime bundle.
5. BrowserForge создает согласованный fingerprint candidate.
6. ADR-0001 materializer разделяет stable, origin-deterministic,
   network-bound и dynamic signals.
7. Fingerprint snapshot сохраняется до первого посещения target site.
8. Создается новый local `user_data_dir` в staging.
9. Synthetic consistency preflight проверяет effective browser values.
10. Generation получает статус `CERTIFYING`.
11. После certification он становится `READY` или `QUARANTINED`.

Generation не считается созданным успешно только потому, что каталог появился
на диске.

## 4. Импорт Legacy Профиля

1. Scanner читает source filesystem metadata без открытия SQLite/browser files.
2. Проверяются filesystem metadata и browser compatibility version.
3. Создается filesystem snapshot/clone с зафиксированным inventory.
4. SQLite integrity checks выполняются только на clone.
5. Source path сохраняется как external provenance reference.
6. Создается новый profile ID, не основанный на email directory name.
7. Профиль получает `IMPORTED_UNVERIFIED` и `provenance_unknown`.
8. Для теста создается изолированный clone с новым generation ID.
9. Original source никогда не запускается.
10. Clone проходит exact-runtime replay и certification.

Автоматическая очистка `.parentlock` или `lock` запрещена. Stale lock может быть
обработан только на clone после доказательства отсутствия процесса и отдельной
audit-команды.

## 5. Открытие Локального Профиля

1. Проверить tenant membership, live grant, device и session policy.
2. Проверить profile status и certification validity.
3. Проверить runtime bundle availability и compatibility.
4. Получить у profile Durable Object lease с новым epoch и fencing token.
5. Получить OS lock на generation directory.
6. Выполнить local integrity/preflight checks.
7. Проверить network-bound coherence до target navigation.
8. Открыть Camouhost session через typed IPC.
9. Проверить effective fingerprint against policy.
10. Перевести generation в `IN_USE`.
11. Heartbeat продлевает lease до graceful close или failure.

Любая ошибка после lease приводит к typed cleanup. Lease не снимается раньше,
чем runtime завершен или признан погибшим.

## 6. Открытие Cloud-Only Профиля

1. Получить у profile Durable Object materialization lease.
2. Скачать manifest и encrypted snapshot в staging.
3. Проверить schema, runtime bundle, digest и retention state.
4. Расшифровать и безопасно распаковать.
5. Проверить inventory и SQLite integrity.
6. Создать новый local materialization record.
7. Выполнить atomic activation staging directory.
8. Продолжить обычный local open flow.

Для пользователя это одна операция `Open profile`. Download и restore являются
внутренними наблюдаемыми этапами.

## 7. Работа Сессии

Runtime может:

- открывать и переключать страницы;
- выполнять typed navigation/interaction commands;
- сохранять cookies и site storage через persistent context;
- собирать sanitized observations и artifacts;
- выполнять mailbox browser fallback;
- предоставлять fingerprint/network diagnostics;
- принимать drain command.

Runtime не может:

- самостоятельно выбрать другой runtime bundle;
- удалить профиль;
- изменить active generation pointer;
- загрузить snapshot в R2 напрямую, минуя application workflow;
- материализовать raw secret в response/event;
- объявить профиль certified.

## 8. Graceful Close

1. Application переводит session в `DRAINING`.
2. Новые команды перестают приниматься.
3. Profile Bridge/Camouhost завершает активные typed operations.
4. Browser context закрывается штатно.
5. Runtime подтверждает закрытие process handles.
6. Проверяется отсутствие открытых descriptors.
7. Generation получает `DIRTY_LOCAL`.
8. Lease сохраняется до завершения snapshot или safe handoff.

Lock-файлы не удаляются application-кодом. Их освобождение является
ответственностью корректного browser close.

## 9. Snapshot И Cloud Sync

1. Зафиксировать quiescence evidence.
2. Построить versioned inventory.
3. Применить compact exclusion policy.
4. Проверить databases на отдельном snapshot clone, не на source workspace.
5. Упаковать в staging tar.zst.
6. Зашифровать per-generation key.
7. Загрузить immutable object с conditional create.
8. Проверить remote digest и restore-readability.
9. Записать manifest/certification references.
10. D1 compare-and-set активирует новый generation, audit и outbox только при
    актуальных profile version и fencing token.
11. Profile Durable Object закрывает lease после подтвержденного D1 result.
12. Разрешить eviction только после статуса `SYNCED`.

При недоступном R2 профиль остается `DIRTY_LOCAL` или `SYNC_RETRY_PENDING`. Его
локальное состояние не удаляется.

Если пользователь забыл закрыть окно, active generation не архивируется на
ходу. Supervisor продолжает heartbeat, показывает idle warning, затем по policy
выполняет typed drain и graceful close. Hard TTL ограничивает бесконечную
сессию. При недоступности пользователя или сети dirty workspace сохраняется
локально и получает `SYNC_RETRY_PENDING`; принудительное копирование live
SQLite/IndexedDB вместо close запрещено.

## 10. Crash Recovery

После restart supervisor:

1. сверяет локальные sessions с Durable Object/D1 projection;
2. проверяет существование runtime process;
3. не удаляет locks вслепую;
4. помечает generation `RECOVERY_REQUIRED`;
5. создает forensic inventory;
6. выполняет read-only integrity checks;
7. при необходимости создает clone для recovery open;
8. возвращает original generation в READY только при доказанной целостности;
9. иначе переводит в quarantine и предлагает last-known-good restore.

## 11. Runtime Migration

Migration всегда выполняется на clone:

1. исходный generation остается immutable;
2. clone открывается новым runtime bundle;
3. compatibility metadata обновляется самим browser runtime;
4. выполняются storage replay и fingerprint drift tests;
5. specialized-site certification сравнивается с baseline;
6. новый generation активируется только после approval gate.

Нельзя открывать legacy beta.27 браузером beta.28 только потому, что это более
новая patch-сборка.

## 12. Удаление

Удаление является workflow, а не `rm -rf`:

1. запретить новые leases;
2. дождаться или принудительно завершить active session по policy;
3. создать audit deletion intent;
4. применить retention/legal constraints;
5. удалить wrapped encryption keys;
6. удалить local generations;
7. удалить remote objects после разрешения retention;
8. сохранить минимальный tombstone без PII;
9. подтвердить отсутствие orphan objects.

## 13. Проверяемые Инварианты Lifecycle

- две конкурентные операции open не получают writer access;
- stale fencing token не может выполнить snapshot/activate;
- source legacy profile не меняет hash/mtime;
- interrupted upload не меняет active generation;
- corrupted restore не становится LOCAL_READY;
- unknown file не исключается из snapshot молча;
- dirty local profile не evictится;
- browser crash не приводит к автоматическому удалению locks;
- runtime upgrade не изменяет previous generation;
- cloud restore и local replay дают одинаковый logical inventory;
- certification evidence всегда связано с exact runtime/profile generation.
- Durable Object eviction не теряет lease epoch или pending transition;
- duplicate Queue message и повтор command не создают второе поколение;
- orphan R2 object не становится active и удаляется reconciler.
