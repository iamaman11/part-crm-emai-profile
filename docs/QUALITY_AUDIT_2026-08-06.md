# Repository Quality Audit — 2026-08-06

**Статус:** repository-local engineering audit  
**Tracking:** issue #41, active PR #47 (supersedes #42)  
**Baseline:** `48efed23c2adc37ad0736c13d612bc22b15d7aab`

## 1. Область Аудита

Проверены доступные repository-local части приложения:

- workspace/toolchain/dependency boundaries;
- typed primitives и aggregate invariants;
- identity, client, profile, session, mailbox и coordinator domains;
- application ports и use-case authorization decisions;
- D1 schema, migrations, projections, transaction/idempotency/audit/outbox paths;
- Cloudflare Worker route classification и composition root;
- Profile Bridge executable boundary и доступные Bridge libraries;
- runtime bundle, local-profile, encrypted-generation и certification modules;
- permanent GitHub Actions;
- архитектурная, contributor и readiness документация.

Legacy proxy credential, proxy provider и issue #1 намеренно не проверялись и не
изменялись по указанию владельца проекта.

## 2. Критерии «10/10» Для Принятого Scope

Числовая оценка не должна маскировать неполный продукт. Для каждого фактически
принятого capability применяются проверяемые критерии:

1. невозможное состояние нельзя создать публичным API;
2. ошибка не оставляет частично изменённый aggregate;
3. unknown/malformed dynamic request fail closed;
4. tenant, actor, profile, generation и version представлены typed boundaries;
5. replay/idempotency не меняют результат и не обходят authorization;
6. storage schema повторяет критические domain invariants;
7. negative tests доказывают отклонение опасных сценариев;
8. executable composition отличается в документации от reusable/synthetic code;
9. каждый принятый claim проходит permanent exact-head CI;
10. production/external свойства не заявляются без внешнего evidence.

## 3. Найденные И Исправленные Дефекты

### QA-01 — обход инварианта AggregateVersion

`AggregateVersion::from_value` позволял создать нулевую версию, обходя
`AggregateVersion::new`.

**Исправление:** invariant-bypassing constructor удалён; public construction
строго положительная, overflow остаётся ошибкой.

### QA-02 — partial mutation при version overflow

Client, BrowserProfile и ProfileCoordinator могли изменить часть состояния до
того, как `version.next()` возвращал overflow.

**Исправление:** следующая версия вычисляется до mutation. Regression tests
проверяют неизменность status, active generation, intent, lease, sequence,
timestamp и receipt history при ошибке.

### QA-03 — READY без verified generation

`BrowserProfile::transition` позволял перейти в `READY` без активации проверенной
generation; open-profile test полагался на это состояние.

**Исправление:** единственный domain path в `READY` —
`activate_generation(Verified)`. Open-profile use case и Worker coordinator
требуют active generation.

### QA-04 — storage допускал live profile без generation

D1 разрешал `READY`, `IN_USE`, `DIRTY_LOCAL` и `SYNCING` с NULL
`active_generation_id`.

**Исправление:** forward-only migration добавляет INSERT/UPDATE guards. Отдельный
SQLite test доказывает reject, сохранение предыдущей строки после failure и
допустимый возврат в `DRAFT` без generation.

Ограничение: текущий catalog ещё не содержит generation registry, поэтому D1
проверяет наличие opaque generation ID, но не подтверждает remote object или
cryptographic verification. Это остаётся отдельным capability.

### QA-05 — API route мог уйти в SPA fallback

Unknown API version, wrong HTTP method и unknown `/api/*` или `/auth/*` path
классифицировались как Static Assets.

**Исправление:** введён `DynamicRouteNotFound`; Worker возвращает 404 и никогда не
передаёт такие requests в asset binding.

### QA-06 — разная семантика exact deadline

Один launch-intent path использовал `now > expires_at`, coordinator использовал
`now >= expires_at`.

**Исправление:** expiry является exclusive deadline во всех проверенных paths;
exact deadline уже expired.

### QA-07 — saturating version в HTTP response

Worker использовал `saturating_add(1)` при вычислении aggregate version для
mutation/replay response. При MAX version это маскировало невозможный increment.

**Исправление:** один checked helper вычисляет response version до D1 mutation;
overflow возвращает stable `500 internal_failure`.

### QA-08 — документация смешивала target и executable scope

Хорошо описанная target architecture могла восприниматься как полностью
скомпонованный продукт.

**Исправление:** `DEVELOPER_CAPABILITY_MATRIX.md` явно маркирует каждую область как
Composed, Library, Synthetic, Target или External и показывает реальные end-to-end
composition paths.

## 4. Надёжность После Hardening

После принятия PR #47 repository-local boundaries должны гарантировать:

- нулевая aggregate version не создаётся публичным primitive API;
- version overflow не меняет aggregate;
- профиль нельзя открыть/координировать без active generation;
- D1 не хранит live profile state без active generation ID;
- wrong-method и unknown-version API request не возвращает SPA;
- exact-deadline launch intent не принимается;
- HTTP response version не saturates;
- capability documentation не выдаёт synthetic/library code за composed product;
- regression защищён отдельным `Repository Quality Audit Gate` плюс всеми
  существующими permanent workflows.

Pinned `cargo fmt` уже применён к active hardening branch, временные formatter
workflows/jobs удалены, permanent workflow permissions возвращены к `contents: read`.
Оставшийся acceptance gate — permanent exact-head CI на финальном commit перед merge.

## 5. Что Не Является Полностью Готовым Приложением

Эти пункты не скрыты и не считаются дефектами уже принятого bounded scope, но не
позволяют честно назвать весь продукт завершённым на 10/10:

- React UI build отсутствует в repository composition;
- mailbox Gmail/IMAP/browser adapters, persistence, API и scheduling не собраны;
- Profile Bridge executable пока имеет узкий claim-URI entry path; доступные
  enrollment/runtime/local-profile modules не соединены в полный operator flow;
- profile generation registry и пользовательский create/verify/activate API не
  скомпонованы с catalog;
- real Camoufox lifecycle, physical Windows evidence и second-device acceptance
  остаются external;
- production R2/D1 atomicity, OS-backed key unwrap, trusted signing, escrow restore
  и independent review остаются external;
- policy/license/product-owner решения из issue #3 остаются открытыми;
- `production_ready` остаётся `false`.

## 6. Итог

Архитектура репозитория модульная и в основном последовательно соблюдает
hexagonal/clean boundaries. Аудит выявил реальные invariant, atomicity, routing и
clarity defects, которые существующий green CI не ловил. Active PR #47 исправляет
их и добавляет отдельный permanent regression gate; PR #42 сохранён как
superseded review history.

После exact-head green и merge можно утверждать высокий уровень надёжности
**реализованного repository-local scope**. Нельзя утверждать 10/10 для всего
продукта до композиции отсутствующего UI/mailbox/generation/Bridge functionality и
получения внешнего runtime/production evidence.
