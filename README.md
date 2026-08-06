# Browser Profile and Mail Runtime

Самостоятельное приложение для создания, хранения, воспроизведения и
сертификации browser profiles, безопасной проверки подключенных почтовых ящиков
и привязки профилей к карточкам клиентов.

Первая рабочая версия разрабатывается в этом репозитории независимо. В будущем
она подключается к CRM через версионированные contracts/events и заменяемые
adapters, а не переносит Camoufox или локальный browser lifecycle в процесс CRM.

## Текущий Статус

**Repository Steps 0–9 приняты.**

- Step 0 создал exact Rust workspace, pure primitives и постоянный
  Linux/Windows/WASM quality gate.
- Step 1 добавил минимальный Rust Cloudflare Worker и доказал repository cold
  build D1/R2/Queue/Durable Object/Static Assets boundary.
- Step 2 добавил typed IDs и actor context, pure identity/client/profile/session/
  mailbox domains, application ports, initial use cases, OpenAPI/protobuf v1 roots
  и активные architecture/contract breaking-change gates.
- Step 3 добавил strict D1 catalog migrations, tenant-inclusive constraints,
  typed Cloudflare adapter, optimistic versions, idempotency/audit/outbox envelope
  и постоянные migration/isolation negative gates.
- Step 4 добавил Cloudflare Access RS256 identity adapter, active membership to
  `ActorContext` resolution, owner bootstrap/transfer, invitations and membership
  lifecycle, explicit client/profile ACL, neutral foreign-resource concealment,
  governed atomic D1 commands and versioned authenticated Worker API.
- Step 5 добавил one Durable Object per profile, monotonic lease epoch и
  server-generated fencing token, launch/heartbeat/TTL/drain/recovery state
  machine, stale-writer rejection, authenticated profile ACL boundary и
  idempotently repairable D1 projection/outbox reconciliation.
- Step 6 добавил pure Bridge domain и Windows-native `profile-bridge.exe`,
  fail-closed redacted custom URI enrollment, single-use device-bound claim,
  one-writer workspace epoch, clean/crash/timeout supervision, versioned fake
  Camouhost IPC и локальный idempotent SQLite command/outbox protocol.
- Step 7 добавил dependency-free runtime-bundle domain, deterministic
  content-addressed synthetic bundle, safe path/extraction validation, typed
  Bridge approval before spawn, rollback on IPC failure, fake Camouhost subprocess
  и active/clean synthetic profile evidence.
- Step 8 добавил marked opaque materialization paths, atomic Bridge-owned
  lock-file protocol, deterministic regular-file inventory, clone-only recovery,
  explicit dirty/recovery lifecycle, forgotten-window and safe quota policies,
  metadata-only support evidence и отдельный Linux/Windows Local Profile Gate.
- Step 9 добавил exact-pinned XChaCha20-Poly1305/SHA-256 container,
  authenticated canonical metadata и final record, immutable generation
  lifecycle, strict pointer CAS/rollback/quarantine/orphan planning, DEK-bound
  nonce-reuse policy, zeroizing plaintext boundaries и Linux/Windows/WASM gate.

Accepted Step 9 source head: `73685241a6d70cf6d8ec80210d94b66cf37b1b45`.
Exact-head Quality Gate run: `31072625808`. Encrypted Generation Gate run:
`31072625852`. Local Profile regression run: `31072625849`. Runtime Bundle
regression run: `31072625892`. Squash merge: `bc5286e3fea767acf955fb2622dab6221ecf1c3b`.

Следующий этап — **Repository Step 10: Certification And Multi-Device**:
bounded certification policy, drift/repeatability evidence, device-scoped
unwrap/revoke contracts и signed-update rollback boundary. Принятие ADR-0001,
второй независимый Windows host и trusted signing остаются внешними gates и не
считаются доказанными. ADR-0006 остаётся proposed; production key management,
remote R2/D1 atomicity и clean-environment escrow restore ещё не доказаны.
Машиночитаемый статус: [`docs/status.json`](docs/status.json).

Единственный разрешенный источник legacy-профилей:

```text
temp/browser_profiles/
```

В нем находится 22 профиля. Оригиналы запрещено запускать, автоматически
исправлять, очищать или мигрировать на месте. Все эксперименты выполняются на
изолированных копиях с новым generation ID.

## Порядок Разработки

Работа выполняется последовательными Repository Steps через GitHub branch, PR,
постоянный CI и squash merge. Нормативный порядок находится в
[`docs/DELIVERY_ROADMAP.md`](docs/DELIVERY_ROADMAP.md).

Текущая среда позволяет автономно выполнять repository code, tests, workflows,
issues, PR review fixes и merge. Внешние операции не симулируются: credential
rotation, Cloudflare account provisioning, физический Windows host, trusted code
signing и offline key escrow требуют отдельного подтверждаемого evidence.

## Основная Документация

- [Product boundary](docs/PRODUCT.md)
- [Delivery roadmap](docs/DELIVERY_ROADMAP.md)
- [Экспертный implementation plan](IMPLEMENTATION_PLAN.md)
- [Архитектурная карта](docs/ARCHITECTURE.md)
- [Архитектура standalone UI](docs/UI_ARCHITECTURE.md)
- [Целевой жизненный цикл профиля](PROFILE_LIFECYCLE_PLAN.md)
- [Contract compatibility policy](docs/CONTRACT_POLICY.md)
- [D1 catalog boundary](docs/D1_CATALOG.md)
- [Profile Coordinator boundary](docs/PROFILE_COORDINATOR.md)
- [Windows Bridge feasibility boundary](docs/WINDOWS_BRIDGE_FEASIBILITY.md)
- [Camouhost runtime bundle boundary](docs/CAMOUHOST_RUNTIME_BUNDLE.md)
- [Local profile lifecycle boundary](docs/LOCAL_PROFILE_LIFECYCLE.md)
- [Encrypted cloud generations boundary](docs/ENCRYPTED_CLOUD_GENERATIONS.md)
- [ADR status registry](docs/ADR_STATUS.md)
- [Threat model](docs/THREAT_MODEL.md)
- [Data classification](docs/DATA_CLASSIFICATION.md)
- [Privacy and retention governance](docs/PRIVACY_AND_RETENTION.md)
- [Test and evidence index](docs/TEST_EVIDENCE_INDEX.md)
- [Cloudflare cold-build evidence](docs/evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md)
- [Domain and contract evidence](docs/evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md)
- [D1 catalog evidence](docs/evidence/2026-08-05-repository-step-3-d1-catalog-foundation.md)
- [Identity, clients and ACL evidence](docs/evidence/2026-08-06-repository-step-4-identity-clients-acl.md)
- [Profile Coordinator evidence](docs/evidence/2026-08-06-repository-step-5-profile-coordinator.md)
- [Windows Bridge feasibility evidence](docs/evidence/2026-08-06-repository-step-6-windows-bridge-feasibility.md)
- [Camouhost runtime bundle evidence](docs/evidence/2026-08-06-repository-step-7-camouhost-runtime-bundle.md)
- [Local profile lifecycle evidence](docs/evidence/2026-08-06-repository-step-8-local-profile-lifecycle.md)
- [Encrypted cloud generations evidence](docs/evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md)
- [Проверенные выводы исследования](docs/RESEARCH_FINDINGS.md)
- [Cloud profile smoke test](docs/CLOUD_PROFILE_SMOKE_TEST.md)
- [Текущая проверка готовности плана](docs/PLAN_READINESS_REVIEW.md)
- [Полный индекс документов](docs/README.md)

## Целевая Форма Системы

```text
app.example.com -> one Cloudflare Worker deployment
  browser routes -> Cloudflare Access -> Rust API + React Static Assets
  bridge routes  -> device-proof policy -> Rust API
  control plane  -> D1 + Durable Objects + Queues/Schedules + encrypted R2

React UI
  -> profilebridge://claim/<single-use-code>
    -> Windows-native Rust Profile Bridge
      -> local encrypted workspace + SQLite cache/outbox
      -> typed IPC -> embedded Python Camouhost
        -> separate visible Camoufox window
```

Standalone v1 не требует отдельной VM, PostgreSQL или Keycloak. Cloudflare
является control plane и хостингом, а Camoufox исполняется только на компьютере
пользователя. Browser payload никогда не исполняется непосредственно из R2.

## Базовый Стек

- Rust `1.97.1`, edition `2024`, exact toolchain;
- `worker 0.8.5`, direct `wasm-bindgen 0.2.126` и `worker-build 0.8.5` для
  Cloudflare Worker baseline;
- pure Rust domains and application ports, compiled natively and for Workers WASM;
- typed Cloudflare D1 adapters with direct `serde 1.0.229` macro dependency;
- forward-only strict D1 migrations tested with Wrangler `4.94.0` and SQLite;
- Cloudflare Access RS256/JWK identity verification through Workers WebCrypto;
- Tokio только в native Profile Bridge и локальных tools;
- Cloudflare Workers Static Assets для React SPA и same-origin API;
- Cloudflare Access identity отдельно от application memberships/grants;
- D1 для каталога/audit, Durable Object на профиль, R2 для encrypted immutable
  generations;
- SQLite WAL только для локального rebuildable cache/outbox Bridge;
- embedded Python/Camoufox runtime как отдельный signed bundle;
- OpenAPI v1 для web API и protobuf v1 для Bridge/CRM contracts.

Step 1 подтвердил reproducible repository cold build Cloudflare pins. Step 2
добавил immutable v1 compatibility floor. Step 3 подтвердил local D1 migration
replay, tenant constraints, typed adapter compilation and Worker packaging. Step
4 подтвердил authenticated owner/member Worker slice, explicit ACL и
transaction-fatal governed D1 mutations. Step 5 подтвердил repository-local
Durable Object coordinator, monotonic epoch/fencing, timeout uncertainty,
assignment-independent authorization, repairable D1 projection и release Worker
packaging. Step 6 подтвердил provider-free Bridge boundaries, redacted
single-use enrollment, local writer/process/outbox semantics и non-empty Windows
release executable. Step 7 подтвердил deterministic synthetic runtime bundle,
manifest/path/content verification, Bridge approval before spawn, rollback and
active-versus-clean synthetic lifecycle. Step 8 подтвердил safe marked local
materialization, atomic Bridge lock-file ownership, deterministic inventory,
clone-only integrity evidence, dirty/recovery preservation, quota exclusion and
metadata-only support output. Step 9 подтвердил synthetic authenticated encrypted-generation container,
immutable lifecycle, DEK-bound nonce protection, zeroizing plaintext memory,
strict parsing, pointer/rollback/quarantine/orphan behavior и native/WASM
portability. Remote staging, real Camoufox, kernel advisory
locking, third-party redistribution, trusted signing, backup/restore, physical
multi-device runtime и account recovery пока не считаются выполненными.

## Ключевые Инварианты

1. Profile ID является opaque typed ID и никогда не равен email или имени каталога.
2. Каждый application command/query получает verified `ActorContext` и tenant scope.
3. D1 reads require typed `TenantScope`; mutations require `ActorContext`.
4. Raw D1 statements принадлежат только Cloudflare adapter boundary.
5. Tenant-owned D1 relations используют tenant-inclusive keys и foreign keys.
6. Один профиль имеет только одного writer через Durable Object lease, monotonic
   epoch/fencing token и локальный Bridge-owned lock protocol.
7. Firefox lock-файлы никогда не удаляются автоматически.
8. Snapshot создается только после graceful close и подтвержденного quiescence.
9. R2 не используется как live filesystem: generation сначала материализуется
   на локальный диск.
10. Cookies, localStorage, IndexedDB, fingerprint data и mailbox secrets являются
    credential-equivalent данными.
11. Пароли, proxy credentials и OAuth tokens хранятся только как secret handles.
12. Assignment профиля клиенту не является правом доступа.
13. Member без явного grant не видит и не запускает профиль.
14. Missing, foreign и unauthorized resources имеют одинаковую neutral disclosure
    форму там, где раскрытие запрещено.
15. Pure domains не зависят от Cloudflare, Windows, Python, browser или storage SDK.
16. D1, Durable Objects и R2 связываются idempotency, immutable objects, outbox и
    reconciliation, а не фиктивной общей транзакцией.
17. Статус ADR и readiness берётся из `ADR_STATUS.md` и `status.json`.

## Безопасность До Реализации

В legacy-скриптах и истории репозитория обнаружен hardcoded proxy credential.
Он считается скомпрометированным и должен быть отозван/ротирован владельцем с
provider-side подтверждением (issue #1). Удаление строки из Git не является
remediation. Значение секрета запрещено переносить в новую конфигурацию,
документацию, тесты, issues или логи.

До production cloud sync ADR-0006 должен стать accepted, а clean-environment
restore drill должен подтвердить root wrapping key, tenant KEK, rotation,
offline recovery escrow и key-loss policy. Обычный Workers Secret сам по себе не
считается завершенной key-management системой.
