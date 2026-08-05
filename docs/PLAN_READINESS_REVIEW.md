# Plan Readiness Review

**Статус:** approved for Phase 0 and phased execution

**Дата review:** 2026-08-05

**Актуальная архитектура:** ADR-0005 Cloudflare-native control plane без VM

## Проверено

- README, implementation plan, architecture map, UI, ADR-0001..ADR-0005 и
  proposed ADR-0006;
- сохранение локального Camoufox runtime при Cloudflare control plane;
- `workers-rs` support для D1, R2, Queues и Durable Objects;
- one-origin hosting через Workers Static Assets;
- Cloudflare Access identity отдельно от application memberships/grants;
- D1 catalog boundary и отсутствие PostgreSQL RLS;
- Durable Object per-profile coordination и fencing;
- partial-failure protocol между D1, DO, Queue и R2;
- encrypted R2 materialization, forgotten-window и multi-device flows;
- будущая замена adapters на CRM OIDC/PostgreSQL без переписывания domain.

## Исправленные Противоречия

Предыдущая версия документов одновременно предполагала выбранную Cloudflare-only
архитектуру и VM/PostgreSQL/Keycloak/Axum. Теперь нормативный standalone stack
един: Rust Worker/WASM, Static Assets, Access, D1, Durable Objects, Queues,
Scheduled Workers, R2 и локальный Windows Bridge.

Также явно зафиксировано:

1. D1 является бизнес-каталогом, Durable Object только profile coordinator.
2. D1/DO/R2 не имеют общей транзакции; требуются saga, idempotency, fencing,
   immutable objects, outbox и reconciliation.
3. D1 не имеет RLS; первая deployment single-tenant, а typed scope и IDOR suite
   являются обязательными компенсирующими controls.
4. Access login не является profile grant и не делает профили общими.
5. Cloudflare Workers не запускает Camoufox; browser остается Windows-native.
6. Workers Secret не считается готовой disaster-recoverable key system.
7. Production Worker пишется на Rust `workers-rs`, без TypeScript/Rust domain
   duplication; TypeScript остается только frontend/test tooling.

## Phase Gates

- отозвать legacy proxy credential до использования прототипа;
- проверить cold build `workers-rs` на exact Rust `1.97.1` и pin dependencies;
- довести ADR-0006 key hierarchy/recovery до accepted и пройти restore drill;
- создать Cloudflare dev/staging/prod resources, Access policy и cost limits;
- получить trusted Windows code-signing certificate до stable release;
- использовать второй independent Windows host для multi-device proof;
- завершить fingerprint certification до production runtime promotion;
- принять отдельный isolation ADR до добавления второго independent tenant.

## Остаточные Риски

- Cloudflare limits/pricing должны быть подтверждены load/cost test на реальных
  profile sizes и command rates;
- Rust SDK/runtime compatibility является upgrade gate;
- Cloudflare account recovery входит в disaster recovery;
- D1 adapter не дает defense-in-depth RLS до CRM/PostgreSQL migration;
- production key root и offline escrow пока являются планом, не реализованным
  доказательством;
- multi-device доказан архитектурно, но текущий smoke выполнен на одном device.

## Вердикт

Архитектура послойна, модульна и не содержит блокирующего противоречия. Phase 0
и первый vertical slice готовы к разработке. Cloud profile production,
multi-device и stable installer сознательно закрыты отдельными проверяемыми
gates; их нельзя считать выполненными только на основании документации.
