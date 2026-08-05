# ADR Status Registry

**Статус:** normative status registry  
**Дата:** 2026-08-05

Этот файл устраняет неоднозначность между архитектурными документами. Текст ADR
определяет решение; этот registry определяет текущий acceptance status и gate.

| ADR | Решение | Статус | Разрешает | Блокирует |
|---|---|---|---|---|
| ADR-0001 | fingerprint stability policy | `proposed` | research и implementation spike | production profile generation/certification |
| ADR-0002 | cloud profile materialization | `accepted`, one-device smoke evidence | local/cloud lifecycle implementation | multi-device и production recovery claims |
| ADR-0003 | desktop runtime distribution | `accepted` | Bridge/runtime implementation | stable release без signing evidence |
| ADR-0004 | tenant access/client ownership | `accepted` | single-tenant ACL/client slice | второй independent tenant без isolation ADR |
| ADR-0005 | Cloudflare-native control plane | `accepted` | Cloudflare cold-build и standalone implementation | production promotion без account/recovery evidence |
| ADR-0006 | cloud key hierarchy/recovery | `proposed`, production blocker | cryptographic design and test implementation | production cloud generations and multi-device key delivery |

## Acceptance Rules

1. `proposed` не трактуется как production policy.
2. Implementation spike может начаться для получения evidence, если ADR явно это
   допускает и не обрабатывает real user secrets.
3. Перевод в `accepted` требует ссылки на review и проверяемые acceptance
   artifacts.
4. `accepted` не означает, что реализация завершена.
5. Smoke-tested status содержит точный scope доказательства.
6. Изменение инварианта или security boundary требует нового ADR либо superseding
   ADR, а не тихой правки существующего решения.

## Открытые Решения

До production необходимо дополнительно принять:

- D1 isolation strategy before second tenant;
- exact streaming AEAD/container and device key-delivery protocol;
- Windows installer/signing/update channel;
- privacy/retention values and acceptable-use policy;
- D1-to-PostgreSQL CRM cutover strategy.

Machine-readable projection статусов находится в [`status.json`](status.json).
