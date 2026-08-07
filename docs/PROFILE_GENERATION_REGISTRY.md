# Profile Generation Registry

**Статус:** composed repository-local capability  
**Tracking:** issue #44, PR #51  
**Parent:** issue #43

## 1. Назначение

Profile Generation Registry является authoritative metadata catalog для immutable
зашифрованных поколений browser profile. Он связывает tenant, profile и generation,
но не хранит browser profile bytes, encryption keys, cookies, history, message
content или другие credential-equivalent данные.

Registry решает пять repository-local задач:

1. регистрирует immutable object identity и canonical digests;
2. хранит governed verification decision;
3. атомарно активирует только verified generation;
4. атомарно снимает exact active-generation pointer перед lifecycle isolation;
5. переводит подозрительную generation в quarantine, если она не активна.

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

Generation lifecycle:

```text
REGISTERED -> VERIFIED -> QUARANTINED
     |                         ^
     +-------------------------+
```

- `REGISTERED`: immutable metadata cataloged; activation запрещена.
- `VERIFIED`: governed verification decision recorded; activation разрешена.
- `QUARANTINED`: activation запрещена; переход обратно отсутствует.

Profile pointer lifecycle вокруг verified generation:

```text
SUSPENDED/DRAFT
      |
      | activate exact VERIFIED generation
      v
    READY + active_generation_id
      |
      | deactivate exact active generation
      v
   SUSPENDED + NULL pointer
```

Profile становится `READY` только через generation activation command. Эта
транзакция одновременно:

1. проверяет exact verified generation;
2. проверяет expected profile version;
3. записывает `active_generation_id`;
4. переводит profile в `READY`;
5. увеличивает profile version;
6. сохраняет idempotency, audit и outbox evidence в том же D1 batch.

Governed deactivation является отдельной CAS-командой. Она требует, чтобы request
указывал именно текущую active generation и expected profile version, затем
атомарно очищает pointer, переводит profile в `SUSPENDED`, увеличивает version и
пишет evidence. Поэтому active generation нельзя «оторвать» прямым SQL UPDATE и
нельзя quarantined до governed deactivation.

Rollback на ранее verified generation не имеет отдельного обходного endpoint:
после governed deactivation ранее verified generation может быть снова активирована
обычным activation path с теми же tenant/owner/CAS/idempotency/integrity guards.

## 4. Governed Command Journal

Каждая mutation имеет отдельную append-only command table:

- `profile_generation_register_commands`;
- `profile_generation_verify_commands`;
- `profile_generation_activate_commands`;
- `profile_generation_deactivate_commands`;
- `profile_generation_quarantine_commands`.

D1 triggers выполняют owner, tenant, profile, status, version и time preconditions
до state mutation. Failure откатывает command row и весь transactional batch.

Дополнительные integrity guards запрещают:

- прямой insert generation без register command;
- изменение object key, digests или generation identity;
- прямой lifecycle transition без verify/quarantine command;
- pointer на missing/unverified generation;
- activation pointer без activate command;
- очистку active pointer без exact deactivate command;
- deactivate неверной или stale generation;
- quarantine активной generation;
- lifecycle timestamp regression.

## 5. Idempotency И Evidence IDs

Generation endpoints используют exact replay decision. Replay допустим только
если совпадают:

- tenant и actor;
- idempotency key;
- command name;
- request digest;
- unexpired idempotency record.

Другой command, другой digest или expired record с тем же key дают `409 conflict`,
а не старый success response.

Audit/outbox IDs не строятся усечением caller-controlled key. Они детерминированно
выводятся из domain-separated SHA-256 material, включающего tenant, actor,
idempotency key и event kind. Это устраняет cross-actor и common-prefix collisions
при сохранении deterministic replay semantics.

## 6. HTTP Surface

Additive v1 routes:

```text
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations
GET  /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/verify
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/activate
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/deactivate
POST /api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/quarantine
```

Mutations требуют active tenant owner. Query доступен owner или explicit profile
grant holder. Foreign, malformed и unauthorized resources используют neutral
`404 not_found`. Wrong method/version fail closed и не попадают в Static Assets.

Public generation response включает generation ID, digests, lifecycle status,
version и optional verification reference. Provider object key намеренно скрыт.
Request DTOs отклоняют unknown fields.

## 7. Error Boundary

Public stable classes:

- `400 invalid_request`: malformed IDs, digests, object key, reference или version;
- `404 not_found`: absent, foreign или unauthorized resource;
- `409 conflict`: exact-idempotency mismatch/reuse или uniqueness conflict;
- `409 version_conflict`: known stale aggregate/profile version;
- `409 invalid_state`: known lifecycle/state/time precondition violation;
- `500 integrity_failure`: storage invariant/foreign-key/check failure that must not
  be presented as a caller business conflict;
- `500 internal_failure`: local numeric/version overflow before mutation;
- `503 dependency_unavailable`: unknown D1/provider failure.

Raw SQLite, D1, Worker SDK и trigger messages не являются public API и не
возвращаются клиенту.

## 8. Verification

Permanent `Profile Generation Gate` проверяет:

- Rust formatting;
- pure registry lifecycle tests;
- exact idempotency decision tests;
- deterministic evidence-ID vectors/collision resistance;
- Cloudflare adapter tests;
- Worker native helpers и complete Worker WASM composition, включая deactivate route;
- full migration replay;
- owner, tenant, stale-version and lifecycle negatives;
- activation/deactivation CAS и monotonic-time rollback;
- unjournaled SQL bypass attempts;
- migration contiguity;
- metadata-only storage policy;
- additive OpenAPI fragment merge/collision rules и exact six-operation surface.

Acceptance дополнительно требует всех repository-wide permanent workflows на том
же exact head, отсутствия unresolved review threads/reviews и squash merge в
`main`. CI, а не branch Markdown, является authoritative acceptance evidence.

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
Legacy proxy credential/provider полностью исключён из этого capability.
`production_ready` остаётся `false`.
