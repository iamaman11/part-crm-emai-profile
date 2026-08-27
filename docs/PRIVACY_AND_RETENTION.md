# Privacy And Retention Governance

**Статус:** normative lifecycle baseline; numeric values remain candidate-scoped product/legal inputs
**Дата:** 2026-08-27

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

Applicability определяется exact Production candidate:

```text
selected enabled capability set
-> reachable data and every material copy
-> owner + purpose + legal/contractual obligation
-> retention/deletion/recovery rule + exact evidence
```

До Production должны быть приняты числовые значения и legal basis для каждого класса, реально
достижимого из выбранного effective set. Для первого CAP-12 slice это как минимум применимые:

- active verified profile generations;
- superseded generations and rollback window;
- quarantined/corrupt/orphan objects;
- dirty local workspace and abandoned device state;
- security/business audit;
- invitations, launch intents and idempotency records;
- backups, D1 Time Travel/export and offline key escrow inventories.

Certification evidence, mailbox observations/message metadata, notification/queue copies and
deleted-profile tombstones входят в gate только если соответствующая capability/guarantee enabled,
reachable или отдельно обещана/обязательна. Наличие кода, migration или исторического документа не
делает disabled capability Production blocker.

Для применимого класса отсутствие принятого значения является Production blocker, а не разрешением
хранить данные бессрочно. Закон, договор или принятая privacy promise может сделать требование
обязательным независимо от UI scope; такое обязательство фиксируется явно, а не угадывается из source
presence.

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

Для enabled/promised surfaces система должна поддерживать применимые governed workflows:

- correction of client card and contact points;
- revoke of user/device access;
- inventory of local, D1, R2, backup and escrow references;
- recovery/reconciliation of valuable accepted state.

Export, hard Profile deletion/purge, mailbox binding removal/provider token revocation and final
tombstone workflows обязательны, когда они входят в enabled capability, accepted promise или
legal/contractual obligation. CAP-12 исключает hard Profile purge и Mailboxes как пользовательские
capabilities первого release; этот scope не отменяет применимый закон и не разрешает скрытое
бессрочное хранение.

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

Production privacy gate for the exact candidate requires:

1. reachable-data/copy inventory and accepted retention matrix with values, owner and rationale;
2. privacy notice/acceptable-use policy appropriate to deployment;
3. subprocessor and data-region inventory;
4. tested revoke, correction, recovery/reconciliation and every applicable promised/legal
   export/delete workflow;
5. backup and escrow expiry handling;
6. incident response and support-access runbooks;
7. confirmation that each enabled browser/provider/automation use is authorized for its intended use;
8. proof that disabled independent capabilities cannot create data or side effects.

Future lifecycle work for a genuinely disabled and unreachable capability does not block the current
candidate. Shared stores are evaluated by semantic ownership and reachable records/copies, not by the
fact that several contexts use one physical database or bucket.
