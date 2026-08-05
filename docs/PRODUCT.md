# Product Definition

**Статус:** normative product boundary  
**Дата:** 2026-08-05

## 1. Назначение

Browser Profile Platform — самостоятельное приложение для управляемого жизненного
цикла browser profiles: создание и безопасный импорт, назначение клиенту, выдача
доступа пользователю, локальный запуск через Windows Profile Bridge, сохранение
immutable generations, восстановление, сертификация и audit.

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

Mailbox Operations является вторичной capability. Предпочтительны Gmail API/OAuth
и IMAP/application password adapters; browser-assisted mailbox flow допускается
только как управляемый fallback. Платформа не дублирует полные письма без отдельного
product, privacy и retention решения.

## 4. Standalone v1

Standalone v1 обслуживает одну организацию и несколько пользователей. Он включает:

- owner/member identity and grants;
- client registry и historical profile assignment;
- profile metadata, generations, sessions, devices и audit;
- Windows-native Profile Bridge;
- Cloudflare-native control plane;
- encrypted immutable R2 generations;
- mailbox status/check jobs без превращения в Communications system.

Добавление второго независимого tenant запрещено до отдельного isolation ADR.

## 5. Не Цели

- обещание абсолютной невидимости или гарантированного обхода anti-fraud;
- запуск Camoufox внутри Cloudflare Worker;
- использование R2 как live Firefox filesystem;
- удалённый generic `exec` на устройстве;
- хранение открытых mailbox/proxy credentials;
- автоматическое изменение original legacy profiles;
- скрытая выдача profile access через client assignment;
- прямой доступ к таблицам или profile payload из CRM.

## 6. Definition Of Product Success

Продукт успешен, когда без CLI owner может управлять users, clients, profiles,
grants, devices, sessions, generations, certification, mailbox jobs и audit;
member видит только granted resources; dirty state не теряется; cloud restore и
key recovery доказаны; runtime updates подписаны и откатываемы; интеграция с CRM
не требует переписывать domain rules или Bridge protocol.

Текущий фактический статус хранится в [`status.json`](status.json). Markdown не
может повышать readiness без соответствующего CI или evidence artifact.
