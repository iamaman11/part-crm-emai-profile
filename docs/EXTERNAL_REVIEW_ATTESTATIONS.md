# External Review Attestations

**Статус:** normative terminal-review binding boundary  
**Дата:** 2026-08-06  
**Tracking issue:** #39  
**Depends on:** `EXTERNAL_EVIDENCE_PROTOCOL.md`, `EXTERNAL_EVIDENCE_READINESS.md`

## 1. Назначение

Terminal external evidence records (`passed` или `failed`) содержат GitHub reviewer
login, exact review/comment URL и review timestamp. Structural validation alone не
доказывает, что объект существует, принадлежит указанному reviewer и действительно
утверждает exact record.

Этот boundary добавляет cryptographic binding между immutable record и exact
GitHub issue comment, pull-request review или pull-request review comment.

Он проверяет identity review attestation, но не проверяет raw provider evidence и
не доказывает качество или независимость reviewer.

## 2. Canonical Claim

Для terminal record вычисляется SHA-256 от canonical compact JSON:

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
fail closed.

## 3. Поддерживаемые GitHub Объекты

Verifier поддерживает только exact same-repository URLs:

```text
https://github.com/<owner>/<repo>/issues/<n>#issuecomment-<id>
https://github.com/<owner>/<repo>/pull/<n>#pullrequestreview-<id>
https://github.com/<owner>/<repo>/pull/<n>#discussion_r<id>
```

Соответствующие GitHub API resources:

- issue comment;
- pull-request review submission;
- pull-request inline review comment.

Foreign repository, missing fragment, unsupported path или deleted object
отклоняются.

## 4. Identity И Timestamp Binding

Verifier требует:

- API `user.login` совпадает с `review.github_login` case-insensitively;
- issue/review comments используют exact API `updated_at`;
- PR review submissions используют exact API `submitted_at`;
- API timestamp совпадает с `review.reviewed_at`;
- API body совпадает с canonical claim.

Если issue или inline review comment изменён после commit record, GitHub
`updated_at` меняется и record перестаёт проходить. Нельзя тихо исправить accepted
record: требуется новый immutable record/review flow.

## 5. Operator Workflow

1. Подготовить terminal candidate record со всеми evidence metadata и временным
   structurally valid `review` object. Claim digest не зависит от `review`.
2. Получить canonical claim template:

   ```text
   python scripts/check-external-review-attestations.py --print-claims
   ```

3. Reviewer публикует exact claim отдельным GitHub comment/review без raw evidence,
   credentials, PII или narrative.
4. Заменить temporary review URL/time на exact GitHub URL и effective API timestamp.
5. Выполнить полный набор:

   ```text
   python scripts/check-external-evidence.py
   python scripts/check-external-evidence-scope.py
   python scripts/check-external-readiness-summary.py --write
   python scripts/check-external-review-attestations.py \
     --repository iamaman11/part-crm-emai-profile
   ```

6. Commit record и regenerated summary одним reviewable PR.

Raw review report остаётся в approved external storage; GitHub comment содержит
только bounded metadata и SHA-256 binding.

## 6. Permanent CI

`.github/workflows/external-review-attestation-gate.yml` получает только:

- `contents: read`;
- `issues: read`;
- `pull-requests: read`.

Workflow:

1. повторно запускает intake, scope и readiness validators;
2. выполняет offline mock HTTP fixtures;
3. проверяет каждый repository terminal record через GitHub API.

Pending records не требуют network attestation. Пустой production record set
проходит с zero terminal records и не создаёт readiness claim.

## 7. Offline Evidence

Mock HTTP tests доказывают:

- issue comment, PR review и inline review comment positive paths;
- pending record performs no request;
- wrong body/digest, author и timestamp rejection;
- edited comment rejection через changed `updated_at`;
- deleted/404 object rejection;
- foreign repository rejection.

Issue #39 содержит bounded synthetic example claim. Он является только примером
claim format и не считается production evidence.

## 8. Ограничения

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
