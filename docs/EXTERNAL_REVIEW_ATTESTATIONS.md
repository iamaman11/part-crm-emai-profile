# External Review Attestations

**Статус:** normative active terminal-review binding boundary  
**Дата:** 2026-08-24  
**Tracking issue:** #39  
**PF-2 convergence:** #471  
**Depends on:** `EXTERNAL_EVIDENCE_PROTOCOL.md`, `EXTERNAL_EVIDENCE_READINESS.md`

## 1. Назначение

Active terminal external evidence records (`passed` или `failed`) содержат GitHub
reviewer login, exact review/comment URL и review timestamp. Structural validation
alone не доказывает, что объект существует, принадлежит указанному reviewer и
действительно утверждает exact record.

Этот boundary добавляет cryptographic binding между active immutable record и
exact GitHub issue comment, pull-request review или pull-request review comment.

Он проверяет identity review attestation, но не проверяет raw provider evidence и
не доказывает качество или независимость reviewer.

PF-2 фиксирует единственного владельца решения:

```text
GitHub API
    ↓
Python/workflow outer observation only
    ↓
strict versioned EXTERNAL_REVIEW_ATTESTATION_OBSERVATION DTO
    ↓
existing opsctl adapter
    ↓
typed Rust ReviewAttestationPolicyV1
    ↓
accept / fail closed
```

Python не является evidence-validity authority. `opsctl` не выполняет network GET,
не получает GitHub credentials и не читает provider state самостоятельно.

## 2. Canonical Claim

Для terminal record typed Rust adapter вычисляет SHA-256 от RFC8785 canonical JSON:

```text
{
  "domain": "external-evidence-review-v1",
  "record": <полный record без self-referential review object>
}
```

Исключается только top-level `review`. Все остальные поля, включая gate, status,
checks, artifact digests, references, limitations, scope, observation time и
`supersedes`, входят в digest.

Exact GitHub review body:

```text
external-evidence-review-v1
evidence_id=<evidence_id>
gate=<gate>
status=<passed-or-failed>
claim_sha256=<64-lowercase-hex>
```

Дополнительный текст, изменённый field, другой порядок строк или другой digest
fail closed. `--print-claims` в Python разрешён только как non-authoritative
operator renderer: acceptance всегда пересчитывает canonical digest в Rust и не
доверяет renderer output.

## 3. Поддерживаемые GitHub Объекты

Outer observer адресует только exact GitHub URLs:

```text
https://github.com/<owner>/<repo>/issues/<n>#issuecomment-<id>
https://github.com/<owner>/<repo>/pull/<n>#pullrequestreview-<id>
https://github.com/<owner>/<repo>/pull/<n>#discussion_r<id>
```

Соответствующие GitHub API resources:

- issue comment;
- pull-request review submission;
- pull-request inline review comment.

Observer извлекает только provider facts, необходимые policy: object availability,
repository из URL, exact reference, `user.login`, body и effective timestamp.
Foreign repository, deleted object, wrong reviewer/timestamp/body не превращаются
Python-адаптером в semantic verdict; эти observations передаются дальше и
отклоняются typed Rust policy, если record является active terminal leaf.

## 4. Identity, Timestamp И Recovery

Typed Rust verifier требует для каждого **active terminal leaf**:

- observed review repository совпадает с expected repository case-insensitively;
- observed exact reference совпадает с `review.review_reference`;
- provider object существует;
- API `user.login` совпадает с `review.github_login` case-insensitively;
- issue/review comments используют observed API `updated_at`;
- PR review submissions используют observed API `submitted_at`;
- observed API timestamp совпадает с `review.reviewed_at`;
- observed API body совпадает с canonical claim;
- `claim_sha256` является ровно 64 lowercase hexadecimal characters.

Rust adapter также единолично определяет active terminal leaves по `evidence_id`,
`status` и `supersedes`. Python observer намеренно не решает, какой terminal record
должен считаться active.

Если active issue или inline review comment изменён после commit record, GitHub
`updated_at` меняется и active record перестаёт проходить. Recovery не редактирует
accepted JSON in place:

1. создать новый immutable record, который `supersedes` invalidated active record;
2. получить новый exact GitHub review claim;
3. regenerated readiness projection делает новый record active leaf;
4. старый record остаётся в audit history; его provider observation может быть
   недоступным или изменённым, но Rust не использует superseded record как current
   acceptance dependency.

Такой порядок предотвращает необратимый CI denial-of-service от исторического
comment и одновременно запрещает тихо продолжать использовать изменённый active
review.

## 5. Operator Workflow

1. Подготовить terminal candidate record со всеми evidence metadata и временным
   structurally valid `review` object. Claim digest не зависит от `review`.
2. Получить bounded claim template для active terminal candidates:

   ```text
   python scripts/check-external-review-attestations.py --print-claims
   ```

   Это только renderer convenience, не acceptance authority.
3. Reviewer публикует exact claim отдельным GitHub comment/review без raw evidence,
   credentials, PII или narrative.
4. Заменить temporary review URL/time на exact GitHub URL и effective API timestamp.
5. Выполнить полный набор, сохранив provider observation вне repository tree:

   ```text
   python scripts/check-external-evidence.py
   python scripts/check-external-evidence-scope.py
   python scripts/check-external-readiness-summary.py --write
   python scripts/check-external-review-attestations.py \
     --repository iamaman11/part-crm-emai-profile \
     --output-observation-json /tmp/external-review-attestation-observation.json
   cargo run --quiet --manifest-path tools/opsctl/Cargo.toml --locked -- \
     --root . \
     hosted-evidence external-review-attestation verify \
     --observation-json /tmp/external-review-attestation-observation.json
   ```

6. Commit record и regenerated summary одним reviewable PR.

Raw review report остаётся в approved external storage; GitHub comment содержит
только bounded metadata и SHA-256 binding. Observation DTO является ephemeral
secret-free workflow input и не создаёт новую tracked evidence authority.

## 6. Permanent CI

`.github/workflows/external-review-attestation-gate.yml` получает только:

- `contents: read`;
- `issues: read`;
- `pull-requests: read`.

Публичное required-check имя остаётся `External Review Attestations`.

Workflow:

1. повторно запускает intake, scope и readiness validators;
2. выполняет offline observer fixtures;
3. запускает typed Rust hosted-evidence policy tests;
4. Python shell делает GitHub GETs и пишет strict
   `EXTERNAL_REVIEW_ATTESTATION_OBSERVATION` DTO;
5. `opsctl hosted-evidence external-review-attestation verify` выполняет финальный
   semantic verdict через typed Rust policy.

Pending records без review не требуют network observation. Observer может получить
provider facts и для superseded historical records, но active/superseded semantics
решает только Rust. Пустой production record set проходит с zero active terminal
records и не создаёт readiness claim.

## 7. Offline Evidence

Observer fixtures доказывают:

- issue comment, PR review и inline review comment observations;
- pending record performs no request;
- wrong body, author и timestamp успешно наблюдаются как facts, а не Python verdict;
- deleted/404 object наблюдается как `available=false`;
- foreign repository наблюдается без Python semantic rejection;
- superseded и replacement records оба могут быть наблюдены без выбора active leaf
  в Python.

Typed Rust tests отдельно доказывают fail-closed rejection для:

- foreign repository/reference drift;
- missing/deleted provider object;
- wrong reviewer;
- edited timestamp;
- wrong canonical claim body;
- malformed claim digest;
- unknown/legacy observation fields;
- active-terminal/supersedes selection.

Issue #39 содержит bounded synthetic example claim. Он является только примером
claim format и не считается production evidence.

## 8. Authority Budgets

```text
opsctl network authority = 0
opsctl GitHub credential authority = 0
opsctl provider mutation authority = 0
opsctl production mutation = false
provider DTO / serde_json::Value crossing into pure core = 0
Python evidence-validity authority on acceptance path = 0
generic provider/plugin/evidence framework = 0
second executable/main = 0
```

GitHub/workflow/Python outer shell владеет только acquisition effects. Existing
`opsctl` adapter владеет strict DTO parsing/canonicalization. Pure Rust policy
владеет semantic decision.

## 9. Ограничения

Attestation verifier не доказывает:

- provider-side operation;
- reviewer competence или organizational independence;
- correctness raw evidence artifact;
- physical Windows execution;
- fingerprint stability;
- device-key protection;
- remote cloud atomicity;
- signing/escrow/legal readiness.

Эти свойства подтверждаются соответствующими external gate checks и independent
review artifacts. `production_ready` остаётся `false` до полного evidence matrix и
отдельного residual-risk review.
