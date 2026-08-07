# Индекс Документации

## Начальная Точка

- [`../README.md`](../README.md): фактический статус, границы и быстрый вход.
- [`status.json`](status.json): machine-readable readiness projection.
- [`PRODUCT.md`](PRODUCT.md): product identity, primary value and non-goals.
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md): текущий архитектурный аудит и
  нормативный порядок post-composition развития standalone/realtime/mailbox/CRM.
- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md): исторический исполнимый порядок
  Repository Steps 0–10 и исходная delivery discipline.
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md): что реально
  composed, что является library/synthetic evidence, target architecture или
  external gate.
- [`QUALITY_AUDIT_2026-08-06.md`](QUALITY_AUDIT_2026-08-06.md): repository-local
  аудит модульности, инвариантов, fail-closed routing, надёжности и DX.

## Нормативные Архитектурные Документы

- [`ARCHITECTURE.md`](ARCHITECTURE.md): runtime topology, слои, module contracts,
  dependency rules и безопасное расширение.
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md): standalone routes, screens,
  frontend layers, state ownership и UI acceptance gates.
- [`../IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md): полный целевой план и
  Definition of Done; post-composition очередность delivery задаёт
  `DEVELOPMENT_PLAN.md`.
- [`../PROFILE_LIFECYCLE_PLAN.md`](../PROFILE_LIFECYCLE_PLAN.md): state machine
  создания, запуска, snapshot, restore и cloud sync.
- [`PROFILE_GENERATION_REGISTRY.md`](PROFILE_GENERATION_REGISTRY.md): authoritative
  metadata registry, lifecycle, exact idempotency, governed D1 commands and
  verified-generation activation boundary.
- [`CONTRACT_POLICY.md`](CONTRACT_POLICY.md): OpenAPI/protobuf v1 roots, stable
  problem taxonomy, compatibility rules and baseline governance.
- [`D1_CATALOG.md`](D1_CATALOG.md): authoritative catalog boundary, tenant
  isolation model, migrations, transaction envelope and evidence limits.
- [`ADR_STATUS.md`](ADR_STATUS.md): authoritative acceptance status всех ADR.

## Security, Privacy И Operations Governance

- [`THREAT_MODEL.md`](THREAT_MODEL.md): assets, trust boundaries, threats,
  fail-closed controls and residual risk.
- [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md): data classes, allowed
  storage/logging and credential-equivalent handling.
- [`PRIVACY_AND_RETENTION.md`](PRIVACY_AND_RETENTION.md): privacy principles,
  retention gates, export/delete and support-access requirements.
- [`EXTERNAL_EVIDENCE_PROTOCOL.md`](EXTERNAL_EVIDENCE_PROTOCOL.md): immutable
  metadata-only intake, review and lineage rules for external production gates.
- [`EXTERNAL_EVIDENCE_OPERATOR.md`](EXTERNAL_EVIDENCE_OPERATOR.md): fail-safe
  operator workflow for deriving gate requirements and creating validator-approved
  `pending` drafts without manufacturing terminal evidence.
- [`EXTERNAL_GATE_EXECUTION_RUNBOOK.md`](EXTERNAL_GATE_EXECUTION_RUNBOOK.md):
  validator-covered provider/host/policy execution guidance for every accepted
  external gate, with metadata-safe capture and explicit stop conditions.
- [`EXTERNAL_EVIDENCE_READINESS.md`](EXTERNAL_EVIDENCE_READINESS.md): deterministic
  active-record projection, mandatory production matrix and readiness interlock.
- [`EXTERNAL_REVIEW_ATTESTATIONS.md`](EXTERNAL_REVIEW_ATTESTATIONS.md): canonical
  terminal claim digest and same-repository GitHub review identity verification.
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
- [`evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md`](evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md):
  typed domain boundaries, state machines, native/WASM evidence, architecture and
  contract negative gates, defects and explicit limitations.
- [`evidence/2026-08-05-repository-step-3-d1-catalog-foundation.md`](evidence/2026-08-05-repository-step-3-d1-catalog-foundation.md):
  strict catalog schema, Wrangler migration replay, tenant constraints, typed D1
  adapter, CAS and transaction-envelope evidence.
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
- typed primitives and pure identity/client/profile/session/mailbox domains;
- provider-independent application ports and initial use-case decisions;
- OpenAPI/protobuf v1 contract roots and immutable accepted baseline;
- strict forward-only D1 catalog migration and typed Cloudflare adapter;
- Wrangler `4.94.0` migration apply/replay plus deterministic SQLite invariants;
- permanent positive and deliberately negative architecture/contract/D1 gates;
- permanent Linux/WASM, D1 migration, Windows and Cloudflare Worker release jobs;
- remote Cloudflare deployment, Access identity and production recovery remain
  later evidence gates.

## Правила Ведения

1. Проверенный факт и архитектурное решение находятся в разных документах.
2. Любое изменение инварианта требует ADR.
3. ADR status определяется `ADR_STATUS.md`, а implementation readiness —
   `status.json` и merged evidence.
4. Статус шага подтверждается green permanent CI и merge, не branch Markdown.
5. Accepted v1 contract baseline не меняется обычным PR; incompatibility требует
   нового major root или отдельно управляемого migration/cutover.
6. Raw D1 statements не выходят за typed Cloudflare adapter boundary.
7. Документы не содержат email профилей, proxy endpoints, credentials, cookies,
   message content или другие secrets/PII.
8. Устаревший документ помечается superseded и перестает быть источником истины.
9. External action не отмечается выполненной без reviewable evidence reference.
10. Post-composition execution order меняется только через `DEVELOPMENT_PLAN.md`;
    capability matrix и status при этом отражают только фактически принятый код/evidence.
