# Product Definition

**Статус:** normative product boundary  
**Дата:** 2026-08-23

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

## 4. Первый production release — PC-1 Production Core v1

Первый production release не обязан ждать завершения всех capability, код которых
уже присутствует в `main`.

`PC-1 production-core-v1` включает только следующий production-enabled scope:

- authentication / authorization / organization membership foundation;
- users;
- client/customer cards;
- browser profiles;
- single and bulk browser-profile operations;
- client↔browser-profile bindings;
- grants / access;
- profile metadata, generations, sessions, devices и audit;
- encrypted immutable cloud generations и restore, необходимые profile lifecycle;
- real Camoufox runtime;
- Windows-native Profile Bridge;
- production-grade Windows Profile Bridge updater/publisher/delivery chain;
- runtime/profile certification, необходимую accepted Core release profile;
- health / readiness / observability;
- notifications/recovery foundations, необходимые Core lifecycle.

Cloudflare-native control plane и Windows/runtime artifacts должны быть взаимно
совместимы в одном accepted Release / Capability Profile. Если PC-1 требует Windows
Profile Bridge, отсутствие/устаревание/несовместимость обязательного AR-15 evidence
блокирует production admission fail-closed.

Добавление второго независимого tenant запрещено до отдельного isolation ADR.

## 5. Source-present, но production-disabled в PC-1

Следующие capability могут существовать, компилироваться и тестироваться в том же
`main`, но в PC-1 остаются `production_enabled=false`:

- mailbox administration;
- bulk mailbox operations;
- client↔mailbox bindings;
- mailbox jobs / automation;
- outbound mail/email side effects;
- later CRM/communications capabilities.

Плановая активация:

```text
PC-1  Production Core v1
PC-2  Mailbox Administration
PC-3  Mailbox Jobs / Automation
PC-4  Outbound / later capabilities
```

Никакой `production-lite` ветки, mailbox fork, второй schema lineage или отдельной
архитектуры для будущих capabilities не создаётся. Capability включается только
через свой accepted Release / Capability Profile и backend admission.

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

### 7.1 PC-1 Production Core success

PC-1 успешен, когда без CLI owner может управлять users, clients, profiles, grants,
devices, sessions, generations, certification и audit; member видит только granted
resources; dirty state не теряется; cloud restore и key recovery, необходимые Core
profile lifecycle, доказаны; runtime updates подписаны и откатываемы; real Camoufox
повторно открывает тот же профиль с сохранённым browser state и fingerprint identity;
Windows/cloud release identities совместимы; production admission не может быть
обойдён UI/env/Python/operator helper.

### 7.2 Full platform success

Полный продуктовый roadmap успешен, когда последующие accepted capability profiles
добавляют mailbox administration, mailbox jobs/automation, outbound и CRM integration
без переписывания уже принятой domain/runtime architecture или создания второй
истории данных/production-enable authority.

Текущий фактический статус хранится в [`status.json`](status.json). Markdown не
может повышать readiness без соответствующего CI/evidence и accepted Release /
Capability Profile.
