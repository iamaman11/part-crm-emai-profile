# External Evidence Readiness Projection

**Статус:** normative repository-local readiness interlock  
**Дата:** 2026-08-06  
**Tracking issue:** #37  
**Depends on:** `EXTERNAL_EVIDENCE_PROTOCOL.md`, PR #36

## 1. Назначение

Immutable external evidence records описывают provider-side, physical-host,
signing, recovery, policy и independent-review observations. Этот документ задаёт
детерминированную metadata-only проекцию active records и запрещает ложное
повышение `docs/status.json.production_ready`.

Проекция не выполняет external actions, не проверяет raw artifacts и не принимает
residual risk. Она вычисляет только:

- какие immutable records являются active lineage leaves;
- какие обязательные gate/environment requirements удовлетворены terminal
  `passed` records;
- допускает ли набор evidence отдельный production-readiness review.

## 2. Нормативные Артефакты

- `scripts/check-external-readiness-summary.py` — policy, projection и readiness
  interlock;
- `docs/external-evidence-summary.json` — committed canonical projection;
- `tests/external-readiness/test-summary.py` — executable synthetic fixtures;
- `tests/external-readiness/fixtures/empty/` — canonical empty-record и false
  production-readiness fixtures;
- `.github/workflows/external-readiness-gate.yml` — permanent CI gate.

## 3. Active Record Rule

Record считается active, если ни один другой record не указывает его в
`supersedes`. В проекции участвует только active leaf каждой gate lineage.

Fail-closed правила:

- duplicate evidence ID запрещён;
- dangling `supersedes` запрещён;
- более одного active record для одной gate запрещено;
- pending и failed leaves не удовлетворяют requirement;
- passed record в неправильном environment не удовлетворяет production
  requirement.

Полная schema, privacy и lineage validation до projection выполняется обоими
intake validators:

```text
python scripts/check-external-evidence.py
python scripts/check-external-evidence-scope.py
```

## 4. Mandatory Production Matrix

Policy version 1 требует terminal `passed` evidence в следующем точном scope:

| Gate | Required environment |
|---|---|
| `legacy_credential_rotation` | `none` |
| `cloudflare_environment` | `production` |
| `windows_primary_host` | `production` |
| `windows_secondary_host` | `production` |
| `trusted_windows_signing` | `production` |
| `offline_key_escrow_restore` | `production` |
| `privacy_retention_approval` | `none` |
| `product_license` | `none` |
| `real_fingerprint_certification` | `production` |
| `production_device_key_unwrap` | `production` |
| `remote_r2_d1_atomicity` | `production` |
| `independent_security_review` | `none` |

Staging evidence остаётся полезным и reviewable, но не удовлетворяет production
requirement. Изменение матрицы является security/readiness policy change и требует
отдельного review.

## 5. Summary Data Minimization

`docs/external-evidence-summary.json` содержит только:

- schema/policy version;
- полный mandatory gate/environment matrix;
- active gate, environment, evidence ID, terminal status и observation date;
- missing requirements;
- aggregate record/status/satisfaction counts;
- `eligible_for_production_review`.

Проекция намеренно исключает:

- references и provider case IDs;
- artifact digests;
- reviewer login и review URL;
- subject IDs;
- check-level details;
- limitations и free-form data.

## 6. Readiness Interlock

`eligible_for_production_review=true` означает только, что каждый mandatory
requirement имеет active terminal `passed` record в точном environment.

Он **не означает** `production_ready=true` и не изменяет `docs/status.json`
автоматически. После полной eligibility всё ещё требуется отдельный human review:

- residual risk;
- unresolved findings;
- production ownership and rollback;
- legal/privacy/license approvals;
- exact deployment scope.

Permanent gate отклоняет:

```text
status.production_ready == true
AND
summary.eligible_for_production_review == false
```

Обратная комбинация допускается и является ожидаемой:

```text
summary.eligible_for_production_review == true
status.production_ready == false
```

Она означает, что evidence complete, но final production review ещё не принят.

## 7. Regeneration

После добавления или supersession external evidence record:

```text
python scripts/check-external-evidence.py
python scripts/check-external-evidence-scope.py
python scripts/check-external-readiness-summary.py --write
python scripts/check-external-readiness-summary.py
```

Committed summary обязан byte-for-byte совпадать с deterministic generator output.
Ручное редактирование summary без соответствующего record set отклоняется CI.

## 8. Synthetic Evidence

Executable fixtures доказывают:

- empty set остаётся ineligible;
- pending и failed records не удовлетворяют requirement;
- staging Cloudflare pass не удовлетворяет production scope;
- superseded root не влияет на active projection;
- полный synthetic 12/12 set даёт eligibility, но не меняет production readiness;
- input ordering не меняет output;
- duplicate active gate и false `production_ready=true` fail closed;
- support projection не содержит sensitive source metadata.

Synthetic fixtures не являются external production evidence.

## 9. Явные Ограничения

Этот interlock не подтверждает credential rotation, Cloudflare resources,
physical Windows execution, trusted signing, escrow restore, real fingerprint
stability, OS-backed device-key behavior, remote R2/D1 atomicity, policy approval
или independent review. Issues #1 и #3 остаются открытыми до появления реальных
reviewed records.
