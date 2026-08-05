# ADR-0005: Cloudflare-Native Control Plane Без Отдельной VM

**Статус:** accepted

**Дата:** 2026-08-05

## Контекст

Standalone-приложению нужны web UI, identity, multi-user ACL, client/profile
catalog, single-writer coordination, durable jobs, encrypted cloud generations и
локальный Camoufox. Отдельная VM с PostgreSQL/Keycloak/Axum увеличивает
обслуживание и patching, хотя browser runtime все равно работает на компьютере
пользователя.

## Решение

Standalone v1 использует Cloudflare-native control plane без отдельной VM:

| Capability | Cloudflare component |
|---|---|
| React SPA hosting | Workers Static Assets |
| API/composition | Rust `workers-rs` Worker/WASM |
| Workforce login perimeter | Cloudflare Access |
| Business catalog/audit/outbox | D1 |
| Per-profile writer coordination | Durable Object |
| Async delivery/verification | Queues + Scheduled Workers |
| Encrypted generations/artifacts | R2 |
| Service/root secrets | Workers Secrets/Secrets Store behind a port |

Camoufox не запускается в Workers, Browser Run или Container. Windows Profile
Bridge на пользовательском PC владеет local filesystem, process tree, runtime
bundle, OS locks и dirty workspace.

## One-Origin Deployment

```text
app.example.com/*          -> React SPA/static assets
app.example.com/api/v1/*   -> Rust Worker
app.example.com/auth/*     -> Rust Worker
app.example.com/bridge/*   -> Rust Worker
```

Workers Static Assets выбран вместо Pages, потому что API, Durable Objects,
Queues, bindings и assets управляются одним Worker deployment и rollback.
Same-origin boundary устраняет штатную CORS-конфигурацию и упрощает Access.

## Rust Boundary

Cloudflare Worker реализуется целиком на Rust через official `workers-rs`,
который предоставляет bindings для D1, R2, Queues и Durable Objects. Domain
crates остаются pure Rust; cloud types разрешены только в adapter/app crates.

Cloud runtime не использует Axum, SQLx или Tokio threads. Native Profile Bridge
может использовать Tokio. Exact Cloudflare crate versions принимаются после
cold-build spike с pinned Rust `1.97.1`.

## Identity И Authorization

Cloudflare Access подтверждает browser identity через approved IdP или email OTP,
но не является resource ACL. Worker валидирует Access JWT, active membership и
live capability. Device-bound `/bridge/*` routes не зависят от browser cookie:
enrollment требует actor-bound single-use intent, затем каждая команда требует
device signature и short-lived app token. Эти routes rate-limited и не отдают
anonymous business data. Приложение не хранит пользовательские пароли, а
прохождение Access без explicit grant не показывает профили.

## D1 И Isolation

D1 является authoritative standalone catalog. Первая production deployment
обслуживает одну организацию и несколько пользователей. D1 не имеет PostgreSQL
RLS, поэтому обязательны typed `TenantScope`, tenant-inclusive keys, отсутствие
direct client access, default-deny repositories и cross-tenant/IDOR tests.

Добавление второго независимого tenant запрещено до отдельного ADR о
D1-per-tenant, sharding или переходе catalog adapter на PostgreSQL CRM.

## Durable Object Boundary

Один deterministic Durable Object на профиль сериализует open/close/heartbeat,
выдает monotonic lease epoch/fencing token и переживает eviction через storage.
Он не хранит client cards, grants или authoritative generation history. D1
остается business catalog.

## Consistency

D1, Durable Object, Queue и R2 не имеют общей транзакции. Cross-service flows
используют idempotency key, expected version, immutable objects, fencing, D1
outbox, at-least-once idempotent consumers и scheduled reconciliation.

## Key Management

Cloudflare secret storage содержит только versioned root wrapping material.
Wrapped tenant KEK хранится в D1, wrapped generation DEK связан с manifest, а
payload зашифрован до R2.

Workers Secret не считается полной key-management policy. Production cloud gate
требует отдельный ADR, offline recovery escrow, rotation, dual-read/single-write,
restore drill и key-loss runbook. При необходимости HSM меняется только
`KeyProviderPort` adapter.

## Не Выбрано

- **VM + PostgreSQL + Keycloak:** надежно, но избыточно для выбранного standalone
  operating model.
- **Cloudflare Pages:** рабочий static-hosting вариант, но разделяет deployment
  там, где нужен Worker-centric control plane.
- **Cloudflare Browser Run:** управляемый headless Chrome, не Camoufox.
- **Cloudflare Containers:** optional utility lane; ephemeral filesystem не
  является live profile store.
- **TypeScript Worker + Rust WASM bridge:** лишняя production-language boundary,
  так как Rust SDK покрывает выбранные bindings.

## Последствия

Плюсы: нет VM patching, единый hosting/control plane, локальная установка
ограничена Bridge/runtime, Rust domain переиспользуется в CRM.

Минусы: vendor-specific adapters/limits, отсутствие RLS в D1, явные saga и
reconciliation, обязательная Rust/WASM compatibility проверка и включение
Cloudflare account recovery в disaster recovery.

## Future CRM

Cloudflare adapters заменяются на CRM OIDC/PostgreSQL/job adapters. Domain
crates, public IDs, Bridge protocol, R2 manifests и lifecycle остаются
совместимыми. Прямой доступ к таблицам CRM запрещен.

## Официальные Источники

- [Workers Rust support](https://developers.cloudflare.com/workers/languages/rust/)
- [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/)
- [Durable Objects storage](https://developers.cloudflare.com/durable-objects/best-practices/access-durable-objects-storage/)
- [D1 data security](https://developers.cloudflare.com/d1/reference/data-security/)
- [Access identity providers](https://developers.cloudflare.com/cloudflare-one/integrations/identity-providers/)
- [R2 Workers API](https://developers.cloudflare.com/r2/get-started/workers-api/)
