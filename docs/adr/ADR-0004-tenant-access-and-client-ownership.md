# ADR-0004: Tenant Access И Client Ownership

**Статус:** accepted

**Дата:** 2026-08-05

## Контекст

Standalone-приложению уже нужны администратор, несколько пользователей,
управляемый доступ к browser profiles и карточки клиентов. Позднее приложение
станет частью CRM, где Party/Customer Master должен владеть каноническими
клиентскими данными.

Простые поля `profile.user_id` и `profile.client_id` недостаточны: они не дают
истории, не разделяют назначение и authorization, затрудняют отзыв доступа и
создают риск cross-tenant disclosure.

## Решение

1. Все бизнес-объекты с первой версии tenant-scoped.
2. В MVP у tenant ровно один active `TENANT_OWNER`; owner transfer является
   отдельной подтверждаемой и аудируемой операцией.
3. Остальные memberships не дают profile access автоматически.
4. Доступ задается явными `ProfileAccessGrant` и `ClientAccessGrant` с default
   deny. Только owner изменяет grants.
5. У профиля не более одного active primary client assignment; у клиента может
   быть много профилей.
6. Assignment хранится отдельной historical сущностью и не является grant.
7. Profile grant показывает только минимальную linked-client projection. Полная
   карточка требует client grant или owner permission.
8. Standalone `Client Registry` временно владеет client card. После CRM
   integration authoritative owner становится Party/Customer Master, а profile
   service хранит stable `party_ref` и read projection.
9. Tenant isolation в standalone обеспечивается typed D1 repository scope,
   application authorization и mandatory IDOR/cross-tenant tests.

## Роли MVP

- `TENANT_OWNER`: все действия tenant, users, clients, grants, profiles,
  recovery и audit;
- `MEMBER + PROFILE_VIEWER`: metadata/history/certification конкретного профиля;
- `MEMBER + PROFILE_OPERATOR`: viewer + open/close/sync/mailbox operations;
- `MEMBER + CLIENT_VIEWER`: полная разрешенная карточка клиента;
- `MEMBER + CLIENT_EDITOR`: viewer + изменение карточки.

Grant может быть отозван независимо от assignment. Отзыв запрещает новые launch
intents немедленно; active session переводится в drain согласно policy.

## Инварианты

- последний owner не блокируется и не удаляется до transfer;
- grant subject, resource и assignment имеют одинаковый `tenant_id`;
- чужой resource не раскрывает факт существования;
- archived client не получает новые assignments;
- active primary assignment уникален на профиль;
- переназначение закрывает старую связь и создает новую в одной транзакции;
- hard delete клиента запрещен при active links, retention hold или audit need;
- user/profile/client email и display names не являются identifiers или paths;
- каждая grant/assignment mutation содержит actor, reason и audit event.

## Последствия

- D1 используется как central authoritative catalog в standalone;
- локальный SQLite Profile Bridge является только cache/outbox;
- UI явно разделяет `назначить клиента` и `выдать доступ пользователю`;
- первая версия немного сложнее CRUD, но CRM integration не требует менять IDs,
  ACL semantics или profile payload;
- delegated admins и team grants можно добавить совместимо как новые subject и
  capability types.

Cloudflare Access подтверждает identity, но не выдает resource rights. Первый
standalone deployment обслуживает одну организацию. Добавление второго tenant
требует отдельного D1 isolation/sharding ADR; при CRM integration PostgreSQL
adapter добавляет `FORCE ROW LEVEL SECURITY` без изменения domain semantics.

## Initial organization setup

The first organization is initialized through the existing identity ceremony owner.
Cloudflare Access verifies the caller; it does not itself grant organization ownership.
`INITIAL_OWNER_EMAIL` pre-authorizes exactly one verified contact for initial setup.
Missing, malformed or nonmatching configuration/verified contact disables bootstrap.
The value belongs to the target's reviewed configuration, never to browser input or Git.

The ordinary setup request contains only the organization display name. The server
derives the initial tenant from the accepted Access issuer/audience, and derives actor
and identity identifiers from that target and the verified subject. Stable identifiers
preserve retry and concurrent-first-request safety without a second bootstrap registry.
The existing empty-boundary/idempotency checks and atomic tenant, identity, owner,
audit and outbox transaction remain authoritative. A suspended/revoked owner cannot
regain access by repeating setup. Initial setup cannot create another organization.

The authenticated tenant-context projection reports setup eligibility. An eligible
identity with no organization sees a name field and Create organization. Other
identities see an access-required explanation; the UI never guesses a tenant or grants
access locally. Successful setup refreshes that same authorized context/session path.
