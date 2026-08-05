# ADR-0006: Cloud Profile Key Hierarchy И Recovery

**Статус:** proposed, blocks production cloud promotion

**Дата:** 2026-08-05

## Контекст

R2 generations содержат cookies, local credentials и site storage. Server-side
R2 encryption недостаточно: archive шифруется приложением до upload. При этом
профиль должен открываться на другом authorized device, keys должны вращаться, а
потеря одного Cloudflare secret не должна необратимо уничтожить все профили.

Текущий smoke test использовал локальный Secret Vault key и доказал только
технический encrypt/upload/restore на одном device.

## Предлагаемое Решение

```text
Root Wrapping Key version N (Cloudflare secret storage)
  -> wraps Tenant KEK version M (ciphertext in D1)
    -> wraps Generation DEK (ciphertext in D1/manifest)
      -> encrypts profile.tar.zst.enc in R2
```

- отдельный random DEK для каждого generation;
- versioned tenant KEK, не используемый для прямого payload encryption;
- root key доступен только `KeyProviderPort` adapter;
- manifest хранит algorithm suite, key IDs, nonces и wrapped keys, но не
  plaintext key material;
- Bridge получает только generation-scoped key material после live grant,
  device proof и lease;
- plaintext key живет в bounded memory и не пишется в logs/SQLite/filesystem;
- device revoke запрещает новые unwrap, но не изменяет immutable archive.

## Rotation

- root/KEK rotation использует dual-read/single-write;
- новые generations сразу пишутся новой active version;
- wrapped KEK/DEK rewrap выполняется idempotent background job без decrypting
  profile payload;
- старая version удаляется только после inventory reconciliation и restore drill;
- emergency rotation имеет отдельный audited command/runbook.

## Recovery

Production требует offline encrypted recovery escrow для root key material:

- минимум две контролируемые копии в разных failure domains;
- split operator access или эквивалентное dual control;
- документированные owner/account recovery prerequisites;
- periodic restore на чистом isolated environment;
- checksum/version inventory без profile PII;
- явная процедура permanent key loss и affected-generation quarantine.

Recovery artifact никогда не хранится в том же Cloudflare account как
единственная копия.

## Cryptographic Gate

До реализации нужно выбрать и review-ить exact streaming AEAD/container format,
nonce strategy, authenticated manifest binding, memory zeroization limitations и
maximum object/chunk sizes. Самодельный crypto format без test vectors, fuzzing и
independent review запрещен.

## Альтернативы

- **Один Workers Secret шифрует все archives:** просто, но увеличивает blast
  radius и делает rotation дорогой.
- **Ключ только на одном PC:** не поддерживает multi-device и disaster recovery.
- **External managed KMS/HSM:** strongest separation, остается допустимым
  `KeyProviderPort` adapter, если Cloudflare-only controls не проходят threat/
  compliance review.
- **Нешифрованный payload в R2:** запрещено.

## Acceptance До Accepted

1. threat model и algorithm review завершены;
2. clean-environment recovery из offline escrow доказан;
3. root/KEK rotation и rollback доказаны;
4. revoked device не получает новый unwrap;
5. loss/corruption одного wrapped key не затрагивает другие generations;
6. keys отсутствуют в logs, D1 plaintext, R2 metadata и support bundle;
7. Cloudflare account-loss scenario имеет tested recovery/cutover runbook.

До выполнения этих пунктов ADR остается `proposed`, а cloud smoke evidence не
повышается до production key-management proof.
