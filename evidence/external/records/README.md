# External Evidence Records

This directory contains immutable, metadata-only JSON records for production gates
that require provider-side, physical-host, policy, signing, recovery or independent
review evidence.

No current record is required for the repository to build. An empty directory means
that no external gate has been promoted. It does not mean failure and must never be
interpreted as production readiness.

Rules:

1. one JSON file per observation, named exactly `<evidence_id>.json`;
2. never edit an accepted record in place; add a newer record with `supersedes`;
3. store only opaque IDs, bounded check codes, sanitized references and SHA-256
   artifact identities;
4. do not store screenshots, logs, credentials, IP addresses, account names,
   browser data, host paths, certificates, keys or free-form provider output;
5. terminal `passed`/`failed` records require an exact same-repository GitHub
   review/comment whose author, effective timestamp and canonical claim body match;
6. `scripts/check-external-evidence.py` is the normative base schema, check catalog,
   privacy and immutable-lineage validator;
7. `scripts/check-external-evidence-scope.py` is the normative strict timestamp,
   evidence-date, gate/environment and IPv6 scope validator;
8. `scripts/prepare-external-evidence.py` derives both accepted contracts and can
   create only canonical `pending` drafts; it cannot create terminal evidence;
9. `scripts/check-external-readiness-summary.py` regenerates the canonical active
   record/readiness projection;
10. `scripts/check-external-review-attestations.py` is the outer GitHub observation
    adapter only on the acceptance path; typed Rust `opsctl hosted-evidence
    external-review-attestation verify` owns active-terminal selection and the
    repository/reference/reviewer/timestamp/canonical-claim validity decision.

Inspect the exact accepted gate/check/environment contract before external work:

```text
python scripts/prepare-external-evidence.py describe
python scripts/prepare-external-evidence.py describe --gate cloudflare_environment
```

Create only a pending shell with explicit sanitized metadata. For example:

```text
python scripts/prepare-external-evidence.py draft \
  --gate cloudflare_environment \
  --evidence-id ev-20260807-cloudflare-staging-draft \
  --observed-at 2026-08-07T19:15:00Z \
  --environment staging \
  --subject-id cloudflare-staging-control-plane \
  --reference https://github.com/iamaman11/part-crm-emai-profile/issues/3 \
  --limitation external-operation-pending \
  --output evidence/external/records/ev-20260807-cloudflare-staging-draft.json
```

The generated draft has `checks=[]`, `artifact_digests_sha256=[]`, no terminal
review and therefore proves no external operation. After the real external work,
keep that record immutable and create a newer terminal record with `supersedes`;
the terminal facts must come from actual evidence and review, never from the draft
tool.

Every local terminal review must run the full sequence from repository root. The
Python shell acquires provider facts first; Rust then makes the semantic decision:

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

Use `python scripts/check-external-review-attestations.py --print-claims` only as a
non-authoritative bounded operator renderer before posting the final GitHub
review/comment. Acceptance never trusts that renderer: the typed Rust verifier
recomputes the RFC8785 canonical claim digest and fails closed on any mismatch.

Permanent External Evidence, External Readiness and External Review Attestation
workflows enforce these boundaries independently. See
`docs/EXTERNAL_EVIDENCE_OPERATOR.md` for the complete fail-safe operator flow.
