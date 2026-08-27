# Product Definition

**Статус:** normative product boundary  
**Дата:** 2026-08-27

## 1. Назначение

Browser Profile Platform — самостоятельное приложение для управляемого жизненного
цикла browser profiles: создание и безопасный импорт, назначение клиенту, выдача
доступа пользователю, локальный запуск через Windows Profile Bridge, сохранение
immutable generations, восстановление, сертификация и audit.

Платформа развивается как одно модульное приложение с одним защищённым `main`,
одной архитектурой, одной schema/compatibility lineage и независимым поэтапным
production enablement capabilities:

```text
source_present != production_enabled
```

Наличие mailbox/outbound/другого capability-кода в репозитории не делает его частью
первого production release и не разрешает его backend execution.

Платформа не является CRM и не становится владельцем канонических коммуникаций.
Она проектируется как будущий внешний bounded context Universal CRM со стабильными
contracts/events и заменяемыми adapters.

## 2. Основная Ценность

Owner организации получает контролируемый каталог профилей и может доказуемо
ответить:

- кому разрешено видеть и запускать профиль;
- на каком устройстве и runtime он использовался;
- какое generation является активным;
- был ли profile закрыт и синхронизирован корректно;
- почему профиль quarantined или требует recovery;
- какое evidence подтверждает certification;
- кто изменил assignment, grant, generation или policy.

Member получает только явно выданные profiles и минимально необходимую связанную
client projection.

## 3. Приоритет Возможностей

1. Governed browser profile lifecycle.
2. Device trust, single-writer coordination и recoverable local state.
3. Encrypted cloud generations и multi-device materialization.
4. Fingerprint/runtime certification.
5. Provider-neutral mailbox operations.
6. CRM integration.

Mailbox Operations является вторичной capability. Предпочтительны Gmail API/OAuth,
Microsoft Graph/OAuth и IMAP/application-password adapters там, где они соответствуют
accepted security/product contracts; browser-assisted mailbox flow допускается только
как управляемый fallback. Платформа не дублирует полные письма без отдельного product,
privacy и retention решения.

## 4. Первый production release — принятый CAP-12 vertical slice

Первый release не обязан ждать завершения всех capability, код которых присутствует
в `main`. Его пользовательская capability конечна:

```text
managed login
-> create/find/view/edit Client
-> create Browser Profile
-> attach Profile to Client
-> see independently authorized attached Profiles in Client card
-> launch through local Windows Profile Bridge and pinned real Camoufox
-> controlled close and authoritative confirmed save
-> reopen the last confirmed state from Client card
-> detach or atomically reassign without deleting either entity
-> logout
```

Принятые product semantics:

- multi-user архитектура: один tenant owner и invited members;
- для acceptance первого release достаточно одной product organization, без special-case
  single-user архитектуры;
- Cloudflare Access / managed external identity владеет credentials и account recovery;
- приложение владеет membership lifecycle, resource authorization и revoke;
- `Client 1 -> 0..N Profiles`, `Profile -> 0..1 Client`;
- assignment не выдаёт ACL; видимость и launch требуют отдельной server authorization;
- один active writer на Profile;
- локальный workspace не является canonical backup;
- `Saved` существует только после exact verification successor generation и authoritative commit;
- hard profile purge не входит в первый release без отдельного product/legal решения.

### 4.1 Обязательные supporting guarantees

Supporting guarantee является release blocker только если он универсален либо достижим из
выбранного effective capability set. Для принятого slice обязательны:

- tenant membership, grants и fail-closed backend admission до side effects;
- immutable encrypted generations, fencing/CAS и сохранение последней подтверждённой generation
  при partial failure;
- device trust, claim, lease/fence, workspace lock и один shipping Bridge/runtime path;
- immutable Windows/Bridge/runtime identity, trusted distribution, rollback и recovery;
- точные D1/R2/Durable Object/Bridge migration, integrity, backup и recovery evidence для
  реально достижимых данных;
- hosted login/logout/recovery и membership-revocation evidence;
- health/error/incident observation, достаточная для принятого сценария;
- один exact Release Candidate и target-specific Production Authorization envelope.

Certification, audit, updater, notification или recovery infrastructure может быть supporting
implementation detail только в объёме, необходимом перечисленным гарантиям. Это не превращает
соответствующий полный пользовательский feature в scope первого release.

Добавление второй независимой organization/tenant в product UX требует отдельного isolation/product
решения; существующие tenant-safe boundaries при этом не ослабляются.

## 5. Source-present, но production-disabled в первом release

Следующие capability могут существовать, компилироваться и тестироваться в том же `main`, но не входят
в выбранный effective set и не блокируют первый release своей независимой незавершённостью:

- Mailboxes, OAuth mailbox administration и client↔mailbox bindings;
- mailbox jobs, Notifications, Automation и outbound side effects;
- bulk profile operations как отдельный product feature;
- tenant-wide Audit UI, global Sessions UI и Certification UI;
- complex roles, mobile parity, generic export и hard Profile purge;
- new providers и будущая CRM/communications integration.

Никакой `production-lite` ветки, mailbox fork, второй schema lineage или отдельной архитектуры для
будущих capabilities не создаётся. Capability включается только через canonical Capability Policy,
accepted Release Candidate и target-specific backend admission.

## 6. Не Цели

- обещание абсолютной невидимости или гарантированного обхода anti-fraud;
- запуск Camoufox внутри Cloudflare Worker;
- использование R2 как live Firefox filesystem;
- удалённый generic `exec` на устройстве;
- хранение открытых mailbox/proxy credentials;
- автоматическое изменение original legacy profiles;
- скрытая выдача profile access через client assignment;
- прямой доступ к таблицам или profile payload из CRM;
- ожидание завершения mailbox/outbound capability перед первым Production Core release.

## 7. Definition Of Product Success

### 7.1 Первый release

Первый release успешен, когда все принятые CAP-12 B1–B10 доказаны на одном неизменном Release
Candidate, Production target envelope содержит свежие universal/reachable evidence, named authority
выдала GO/PILOT, а deployed candidate совпадает с авторизованным. Пользователь проходит весь сценарий
без CLI; второй writer и unauthorized/replayed launch отвергаются; любая save failure сохраняет
последнюю подтверждённую generation; UI не сообщает ложный `Saved`.

### 7.2 Full platform success

Полный продукт развивается отдельными product decisions: последующие accepted capability profiles
могут добавлять mailbox administration, mailbox jobs/automation, outbound и CRM integration без
переписывания принятой domain/runtime architecture или создания второй истории данных/production-enable
authority.

## 8. Status And Execution Authority

Этот документ владеет стабильным product scope, но не текущим статусом. Binding program живёт в
[`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md), единственный live transaction
pointer — fresh [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266), а
Production state — в exact candidate decision/evidence его natural owners. `status.json`, Markdown,
старый roadmap или green CI сами по себе не могут повысить readiness или выдать Production Authorization.

## 9. Product/Legal Input Before Distribution

До публичной/коммерческой дистрибуции product owner должен выбрать и добавить совместимую repository
и product license либо явно документировать иной legal режим. Автоматизированный агент не выбирает
license по догадке. Отсутствие решения не расширяет source/runtime scope, но блокирует утверждение о
разрешённой дистрибуции.
