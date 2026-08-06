# External Production Evidence Protocol

**Статус:** normative metadata intake boundary  
**Дата:** 2026-08-06  
**Tracking issue:** #35  
**Related external gates:** #1, #3

## 1. Назначение

Repository Steps 0–10 завершили ограниченную repository-local реализацию и
синтетические доказательства. Оставшиеся production claims требуют действий,
которые GitHub CI не может честно выполнить: provider-side credential rotation,
Cloudflare provisioning, физические Windows hosts, real Camoufox runs, OS-backed
key unwrap, trusted signing, clean-environment escrow restore, policy/license
approval и независимый security review.

Этот протокол задаёт строгий immutable metadata envelope для таких доказательств.
Он позволяет ревьюировать источник, scope, точные checks и artifact identity, не
помещая в Git фактические credentials, PII, browser payload, raw logs или ключи.

Протокол **не выполняет** внешнюю операцию и **не переводит** проект в production
readiness автоматически.

## 2. Нормативные Артефакты

- `scripts/check-external-evidence.py` — executable schema, gate catalog и lineage
  validator;
- `evidence/external/records/` — immutable production metadata records;
- `tests/external-evidence/fixtures/` — canonical positive и deliberately negative
  fixtures;
- `.github/workflows/external-evidence-gate.yml` — постоянный CI gate.

Executable validator является источником истины для точных enum/check values.
Изменение gate contract требует review того же уровня, что изменение security
boundary.

## 3. Record Model

Каждый record — один canonical JSON-файл:

```text
evidence/external/records/<evidence_id>.json
```

Обязательные поля:

| Поле | Правило |
|---|---|
| `schema_version` | только `1` |
| `evidence_id` | `ev-YYYYMMDD-<opaque-token>`, совпадает с именем файла |
| `gate` | один из versioned external gate identifiers |
| `status` | `pending`, `passed` или `failed` |
| `observed_at` | whole-second RFC3339 UTC с `Z` |
| `scope` | только `environment` и opaque `subject_id` |
| `checks` | bounded gate-specific code/outcome pairs |
| `references` | reviewable GitHub URL, opaque provider case или report digest |
| `artifact_digests_sha256` | identity sanitized artifacts, не их содержимое |
| `limitations` | bounded machine tokens, не free-form narrative |

Terminal records (`passed`/`failed`) дополнительно содержат `review` с GitHub
login, GitHub review reference и UTC timestamp. Новый observation может содержать
`supersedes`, указывающий на предыдущий immutable record того же gate.

## 4. Статусы И Promotion

### `pending`

Используется, когда собрана только часть required checks. Он не содержит terminal
review и не может содержать `fail`. Отсутствующие checks остаются отсутствующими,
а не маскируются `not_applicable`.

### `passed`

Допустим только когда:

1. присутствуют все required checks выбранного gate;
2. каждый check имеет outcome `pass`;
3. указан минимум один SHA-256 artifact identity;
4. присутствует terminal GitHub review;
5. record проходит secret/PII, canonical-format и lineage validation.

### `failed`

Требует минимум один explicit `fail` и terminal GitHub review. Failure не
перезаписывается. Повторная попытка создаёт новый record с `supersedes`.

## 5. Immutable Lineage

- accepted JSON не редактируется in place;
- новый record может supersede только существующий более старый record того же
  gate;
- один record может иметь не более одного successor;
- на gate допускается только одна active leaf lineage;
- dangling references, cycles, forks и параллельные active roots fail closed.

Эта модель сохраняет историю failed/pending/passed observations и не позволяет
тихо заменить неудобное evidence.

## 6. External Gate Catalog

Validator содержит точные required checks для:

- legacy credential rotation;
- isolated Cloudflare environment;
- primary и independent secondary Windows hosts;
- trusted Windows signing/update verification;
- offline key escrow restore;
- privacy/retention/acceptable-use approval;
- product license и redistribution review;
- real fingerprint certification;
- production device-key unwrap/revoke/recovery;
- remote R2/D1 atomicity, nonce claim, rollback и reconciliation;
- independent security and cryptographic review.

Gate считается доказанным только в scope конкретного terminal record. Например,
успешный staging Cloudflare record не доказывает production environment, а
Windows primary host не заменяет independent secondary-host evidence.

## 7. Data Minimization

Records разрешают только metadata allowlist. Запрещены:

- passwords, tokens, authorization headers, URI credentials и PEM material;
- root/KEK/DEK/device private key bytes, certificates и escrow payload;
- email addresses, raw IP addresses и account names;
- Windows/Unix user paths и device serial numbers;
- screenshots, raw provider exports, full logs и free-form incident narrative;
- cookies, profile archives, browser databases, mailbox content и client PII.

Raw artifacts остаются в одобренном provider/review storage. Git содержит только
opaque case reference и SHA-256 identity безопасного review artifact.

## 8. Review Procedure

1. Выполнить внешнюю операцию вне Git и сохранить raw evidence в approved storage.
2. Создать sanitized review artifact без credentials/PII и вычислить SHA-256.
3. Добавить новый immutable record; при повторной попытке указать `supersedes`.
4. Запустить `python scripts/check-external-evidence.py`.
5. Получить отдельный GitHub review terminal claim.
6. После merge отдельно обновить `docs/status.json` только для точного доказанного
   scope. Record сам по себе не изменяет readiness projection.
7. `production_ready` может стать `true` только после отдельного review всех
   обязательных gates и residual risk.

## 9. CI Evidence

Permanent workflow проверяет:

- repository production record set, включая допустимый пустой set;
- canonical valid pending record;
- canonical valid terminal `passed` record;
- rejection synthetic PII/unsafe metadata;
- rejection incomplete `passed` claim;
- rejection forked и dangling lineage.

Negative fixtures доказывают fail-closed поведение validator и сами не являются
production evidence.

## 10. Явные Ограничения

Этот protocol не доказывает ни один внешний gate на дату создания. Он не заменяет
provider confirmation, physical-host execution, trusted certificate validation,
real browser certification, independent review или clean-environment recovery.
`docs/status.json.implementation.production_code` и production readiness остаются
без повышения до появления и отдельного принятия реального evidence.
