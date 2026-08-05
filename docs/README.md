# Индекс Документации

## Нормативные Документы

- [`../README.md`](../README.md): границы проекта и быстрый вход.
- [`ARCHITECTURE.md`](ARCHITECTURE.md): runtime topology, слои, module contracts,
  dependency rules и рецепты безопасного расширения.
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md): standalone routes, screens,
  frontend layers, state ownership и UI acceptance gates.
- [`../IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md): этапы реализации и
  проверяемый Definition of Done.
- [`../PROFILE_LIFECYCLE_PLAN.md`](../PROFILE_LIFECYCLE_PLAN.md): state machine
  создания, запуска, snapshot, restore и cloud sync.
- [`RESEARCH_FINDINGS.md`](RESEARCH_FINDINGS.md): проверенные локальные факты,
  результаты canary и ограничения доказательств.
- [`CLOUD_PROFILE_SMOKE_TEST.md`](CLOUD_PROFILE_SMOKE_TEST.md): фактический
  encrypted R2 create/sync/restore/replay test и его границы доказательств.
- [`PLAN_READINESS_REVIEW.md`](PLAN_READINESS_REVIEW.md): повторная проверка
  исполнимости плана, исправленные замечания и внешние phase gates.

## Architecture Decision Records

- [`adr/ADR-0001-fingerprint-stability-policy.md`](adr/ADR-0001-fingerprint-stability-policy.md):
  какие fingerprint-сигналы стабильны, детерминированы по origin или динамичны.
- [`adr/ADR-0002-cloud-profile-materialization.md`](adr/ADR-0002-cloud-profile-materialization.md):
  почему облачный профиль хранится в R2, но исполняется из локальной
  материализации.
- [`adr/ADR-0003-desktop-runtime-distribution.md`](adr/ADR-0003-desktop-runtime-distribution.md):
  web application + Profile Bridge, упаковка Rust/Camoufox/Python и multi-device
  режим.
- [`adr/ADR-0004-tenant-access-and-client-ownership.md`](adr/ADR-0004-tenant-access-and-client-ownership.md):
  модель администратора, resource grants, client cards и историческое назначение
  профиля клиенту.
- [`adr/ADR-0005-cloudflare-native-control-plane.md`](adr/ADR-0005-cloudflare-native-control-plane.md):
  почему standalone использует Workers/Access/D1/DO/Queues/R2 без отдельной VM.
- [`adr/ADR-0006-cloud-profile-key-management.md`](adr/ADR-0006-cloud-profile-key-management.md):
  proposed key hierarchy, rotation и offline recovery gate для cloud profiles.

## Правила Ведения

1. Проверенный факт и архитектурное решение должны находиться в разных
   документах.
2. Любое изменение инварианта требует ADR.
3. Статус реализации подтверждается тестом или артефактом, а не формулировкой в
   Markdown.
4. Документы не должны содержать email профилей, proxy endpoints, credentials,
   cookies, message content или другие секреты и PII.
5. Устаревший документ явно помечается superseded и перестает быть источником
   истины.
