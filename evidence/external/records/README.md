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
5. terminal `passed`/`failed` records require a GitHub review reference;
6. `scripts/check-external-evidence.py` is the normative executable validator.
