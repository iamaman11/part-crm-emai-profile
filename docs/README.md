# Индекс Документации

## Начальная Точка

- [`../README.md`](../README.md): фактический статус, границы и быстрый вход.
- [`status.json`](status.json): machine-readable readiness projection.
- [`PRODUCT.md`](PRODUCT.md): product identity, primary value and non-goals.
- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md): исполнимый порядок Repository Steps
  для автономной разработки через GitHub и CI.

## Нормативные Архитектурные Документы

- [`ARCHITECTURE.md`](ARCHITECTURE.md): runtime topology, слои, module contracts,
  dependency rules и безопасное расширение.
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md): standalone routes, screens,
  frontend layers, state ownership и UI acceptance gates.
- [`../IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md): полный целевой план и
  Definition of Done; порядок delivery уточняется `DELIVERY_ROADMAP.md`.
- [`../PROFILE_LIFECYCLE_PLAN.md`](../PROFILE_LIFECYCLE_PLAN.md): state machine
  создания, запуска, snapshot, restore и cloud sync.
- [`ADR_STATUS.md`](ADR_STATUS.md): authoritative acceptance status всех ADR.

## Security, Privacy И Operations Governance

- [`THREAT_MODEL.md`](THREAT_MODEL.md): assets, trust boundaries, threats,
  fail-closed controls and residual risk.
- [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md): data classes, allowed
  storage/logging and credential-equivalent handling.
- [`PRIVACY_AND_RETENTION.md`](PRIVACY_AND_RETENTION.md): privacy principles,
  retention gates, export/delete and support-access requirements.
- [`TEST_EVIDENCE_INDEX.md`](TEST_EVIDENCE_INDEX.md): accepted evidence scope and
  promotion rules.
- [`../SECURITY.md`](../SECURITY.md): vulnerability and credential incident policy.
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md): branch, PR, test and acceptance rules.

## Исследование И Evidence

- [`RESEARCH_FINDINGS.md`](RESEARCH_FINDINGS.md): проверенные локальные факты,
  результаты canary и ограничения доказательств.
- [`CLOUD_PROFILE_SMOKE_TEST.md`](CLOUD_PROFILE_SMOKE_TEST.md): фактический
  encrypted R2 create/sync/restore/replay test и его границы.
- [`evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md`](evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md):
  exact Rust/`workers-rs` cold build, binding compile surface, release artifact,
  найденные дефекты, ограничения и supply-chain observation.
- [`PLAN_READINESS_REVIEW.md`](PLAN_READINESS_REVIEW.md): readiness review,
  corrected status claims and external gates.

## Architecture Decision Records

- [`adr/ADR-0001-fingerprint-stability-policy.md`](adr/ADR-0001-fingerprint-stability-policy.md):
  proposed policy for stable, origin-deterministic, network-bound and dynamic
  fingerprint signals.
- [`adr/ADR-0002-cloud-profile-materialization.md`](adr/ADR-0002-cloud-profile-materialization.md):
  accepted cloud-backed/local-execution model with one-device smoke evidence.
- [`adr/ADR-0003-desktop-runtime-distribution.md`](adr/ADR-0003-desktop-runtime-distribution.md):
  accepted web application + Profile Bridge and runtime packaging model.
- [`adr/ADR-0004-tenant-access-and-client-ownership.md`](adr/ADR-0004-tenant-access-and-client-ownership.md):
  accepted owner/member, grants, client cards and assignment model.
- [`adr/ADR-0005-cloudflare-native-control-plane.md`](adr/ADR-0005-cloudflare-native-control-plane.md):
  accepted Cloudflare-native standalone topology.
- [`adr/ADR-0006-cloud-profile-key-management.md`](adr/ADR-0006-cloud-profile-key-management.md):
  proposed key hierarchy, rotation and offline recovery; blocks production cloud.

## Executable Baseline

- Rust `1.97.1`, edition `2024`, exact `rust-toolchain.toml`;
- committed workspace `Cargo.lock`;
- `worker 0.8.5`, direct `wasm-bindgen 0.2.126`, `worker-build 0.8.5`;
- permanent Linux/WASM, Windows and Cloudflare Worker release-build jobs;
- binding contract for D1, R2, Queue, Durable Object and Static Assets;
- remote Cloudflare deployment remains a separate external evidence gate.

## Правила Ведения

1. Проверенный факт и архитектурное решение находятся в разных документах.
2. Любое изменение инварианта требует ADR.
3. ADR status определяется `ADR_STATUS.md`, а implementation readiness —
   `status.json` и merged evidence.
4. Статус шага подтверждается green permanent CI и merge, не branch Markdown.
5. Документы не содержат email профилей, proxy endpoints, credentials, cookies,
   message content или другие secrets/PII.
6. Устаревший документ помечается superseded и перестает быть источником истины.
7. External action не отмечается выполненной без reviewable evidence reference.
