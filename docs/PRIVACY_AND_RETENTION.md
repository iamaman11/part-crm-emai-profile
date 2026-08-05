# Privacy And Retention Governance

**Статус:** design baseline; numeric production retention values are not yet accepted  
**Дата:** 2026-08-05

Этот документ задаёт технические обязательства. Он не заменяет применимое
юридическое заключение, договоры с пользователями или privacy notice.

## 1. Принципы

- purpose limitation: данные профиля используются только для явно разрешённых
  browser/mailbox operations;
- data minimization: каталог не копирует payload, а mailbox domain не копирует
  полные письма без отдельного требования;
- least privilege: identity не равна grant, assignment не равен access;
- storage limitation: каждое поколение и evidence имеет versioned retention class;
- transparency: UI показывает cloud/local state и не сообщает ложный `Saved`;
- deletion by workflow: сначала revoke/lease drain/retention checks, затем keys,
  local data, remote objects и reconciliation;
- recovery limitation: backup и escrow не становятся бессрочным скрытым архивом.

## 2. Обязательные Retention Classes

До production должны быть приняты числовые значения и legal basis для:

- active verified profile generations;
- superseded generations and rollback window;
- quarantined/corrupt/orphan objects;
- dirty local workspace and abandoned device state;
- certification evidence;
- security/business audit;
- invitations, launch intents and idempotency records;
- mailbox observations, message metadata and error artifacts;
- deleted-profile tombstone;
- backups, D1 Time Travel/export and offline key escrow inventories.

Отсутствие принятого значения является production blocker, а не разрешением
хранить данные бессрочно.

## 3. Technical Retention Rules

- Active generation is never deleted before a verified replacement or explicit
  profile deletion workflow.
- Dirty/unsynced local state is retained until sync, governed discard or recovery.
- Superseded objects are deleted only after catalog/reconciliation confirms they
  are not active, held or required for rollback.
- Launch intents and short-lived tokens expire by protocol and are not retained as
  bearer values in audit.
- Key deletion is coordinated with object deletion; orphan ciphertext and orphan
  wrapped keys are both detectable.
- Bucket lock may protect bounded evidence but cannot silently override personal
  data deletion obligations.
- Audit retains minimal actor/action/result/reference data and excludes payload.

## 4. Data Subject And Owner Operations

The application design must support governed workflows for:

- export of client/profile metadata without exposing unrelated tenant data;
- correction of client card and contact points;
- revoke of user/device access;
- profile deletion with active-session and retention checks;
- mailbox binding removal and token revocation;
- inventory of local, D1, R2, backup and escrow references;
- final reconciliation report and minimal non-PII tombstone.

Browser history or third-party site data may have separate legal/contractual
constraints. The UI must not promise deletion from external websites or provider
systems.

## 5. Support And Incident Access

- No standing support access to profile payload.
- Break-glass operations require explicit purpose, actor, bounded duration and
  audit reference.
- Support bundles use allowlisted metadata and sanitized diagnostics only.
- Credential-equivalent exposure triggers revoke/rotate and incident inventory.
- External processors, regions and recovery locations must be documented before
  production onboarding.

## 6. Production Acceptance

Production privacy gate requires:

1. accepted retention matrix with values, owner and rationale;
2. privacy notice/acceptable-use policy appropriate to deployment;
3. subprocessor and data-region inventory;
4. tested export/delete/reconciliation workflow;
5. backup and escrow expiry handling;
6. incident response and support-access runbooks;
7. confirmation that mailbox/browser automation is authorized for intended use.
