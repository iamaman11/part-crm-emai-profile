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
5. terminal `passed`/`failed` records require an exact GitHub review/comment URL;
6. `scripts/check-external-evidence.py` is the normative base schema, check catalog,
   privacy and immutable-lineage validator;
7. `scripts/check-external-evidence-scope.py` is the normative strict timestamp,
   evidence-date, gate/environment and IPv6 scope validator;
8. every local review must run both commands from the repository root:

   ```text
   python scripts/check-external-evidence.py
   python scripts/check-external-evidence-scope.py
   ```

The permanent External Evidence Gate runs both validators and their positive and
negative fixtures.
