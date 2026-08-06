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
8. `scripts/check-external-readiness-summary.py` regenerates the canonical active
   record/readiness projection;
9. `scripts/check-external-review-attestations.py` verifies every terminal record
   against the exact GitHub API object;
10. every local terminal review must run the full sequence from repository root:

   ```text
   python scripts/check-external-evidence.py
   python scripts/check-external-evidence-scope.py
   python scripts/check-external-readiness-summary.py --write
   python scripts/check-external-review-attestations.py \
     --repository iamaman11/part-crm-emai-profile
   ```

Use `python scripts/check-external-review-attestations.py --print-claims` to obtain
the exact bounded claim body before posting the final GitHub review/comment.

Permanent External Evidence, External Readiness and External Review Attestation
workflows enforce these boundaries independently.
