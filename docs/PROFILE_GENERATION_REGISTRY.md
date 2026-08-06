# Profile Generation Registry

**Статус:** composed repository-local capability  
**Tracking:** issue #44, draft PR #45  
**Parent:** issue #43

## 1. Назначение

Profile Generation Registry является authoritative metadata catalog для immutable
зашифрованных поколений browser profile. Он связывает tenant, profile и generation,
но не хранит browser profile bytes, encryption keys, cookies, history, message
content или другие credential-equivalent данные.

Registry решает четыре repository-local задачи:

1. регистрирует immutable object identity и canonical digests;
2. хранит governed verification decision;
3. атомарно активирует только verified generation;
4. переводит подозрительную generation в quarantine, если она не активна.

Реальный R2 upload/download, cryptographic device unwrap и physical Windows
verification остаются external/provider capabilities и не выводятся из статуса
registry автоматически.

## 2. Владение Данными

Primary key:

```text
(tenant_id, profile_id, generation_id)
```

Metadata record содержит только:

- opaque generation ID;
- immutable provider object key;
- lowercase SHA-256 metadata digest;
- lowercase SHA-256 encrypted-container digest;
- lifecycle status и aggregate version;
- opaque verification reference;
- actor IDs и millisecond timestamps.

`object_key` необходим adapter boundary, но не возвращается публичным support-safe
HTTP response. Он не должен содержать tenant/client names, email, filesystem path,
credentials или signed provider URL.

## 3. Lifecycle

```text
REGISTERED -> VERIFIED -> QUARANTINED
     |                         ^
     +-------------------------+
```

- `REGISTERED`: immutable metadata cataloged; activation запрещена.
- `VERIFIED`: governed verification decision recorded; activation разрешена.
- `QUARANTINED`: activation запрещена; переход обратно отсутствует.

Profile становится `READY` только через generation activation command. Эта
транзакция одновременно:

1. проверяет exact verified generation;
2. проверяет expected profile version;
3. записывает `active_generation_id`;
4. переводит profile в `READY`;
5. увеличивает profile version;
6. сохраняет idempotency, audit и outbox evidence в том же D1 batch.

## 4. Governed Command Journal

Каждая mutation имеет отдельную append-only command table:

- `profile_generation_register_commands`;
- `profile_generation_verify_commands`;
- `profile_generation_activate_commands`;
- `profile_generation_quarantine_commands`.

D1 triggers выполняют owner, tenant, profile, status, version и time preconditions
до state mutation. Failure откатывает command row и весь transactional batch.

Дополнительные integrity guards запрещают:

- прямой insert generation без register command;
- изменение object key, digests или generation identity;
- прямой lifecycle transition без verify/quarantine command;
- pointer на missing/unverified generation;
- activation pointer без activate command;
- quarantine активной generation;
- lifecycle timestamp regression.

## 5. Idempotency

Новые generation endpoints используют exact replay decision. Replay допустим только
если совпадают:

- tenant и actor;
- idempotency key;
- command name;
- request digest;
- unexpired idempotency record.

Другой command, другой digest или expired record с тем же key дают `409 conflict`,
а не старый success response.

## 6. HTTP Surface

Additive v1 routes:

```text
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations
GET  /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/verify
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/activate
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/quarantine
```

Mutations требуют active tenant owner. Query доступен owner или explicit profile
grant holder. Foreign, malformed и unauthorized resources используют neutral
`404 not_found`. Wrong method/version fail closed и не попадают в Static Assets.

Public generation response включает generation ID, digests, lifecycle status,
version и optional verification reference. Provider object key намеренно скрыт.

## 7. Error Boundary

Public stable classes:

- `400 invalid_request`: malformed IDs, digests, object key, reference или version;
- `404 not_found`: absent, foreign или unauthorized resource;
- `409 conflict`: stale version, invalid state, idempotency mismatch/reuse;
- `500 internal_failure`: local numeric overflow или invariant failure.

Raw SQLite, D1, Worker SDK и trigger messages не являются public API.

## 8. Verification

Permanent `Profile Generation Gate` проверяет:

- Rust formatting;
- pure registry lifecycle tests;
- Cloudflare adapter tests;
- complete Worker WASM composition;
- full migration replay;
- owner, tenant, stale-version and lifecycle negatives;
- monotonic-time rollback;
- unjournaled SQL bypass attempts;
- migration contiguity;
- metadata-only storage policy.

Final acceptance требует также всех repository-wide permanent workflows на одном
exact head.

## 9. Ограничения

`VERIFIED` означает, что repository получил governed verification decision и
opaque reference. Это не доказывает само по себе:

- наличие или неизменность production R2 object;
- правильность external verifier;
- device-key unwrap;
- real Camoufox launch/restore;
- cross-device portability;
- production atomicity или readiness.

Такие свойства требуют external evidence protocol и остаются вне scope issue #44.
Legacy proxy credential/provider также полностью исключён из этого capability.
