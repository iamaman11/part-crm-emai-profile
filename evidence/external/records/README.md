# External Evidence Records

This directory contains immutable, metadata-only JSON observations for external production gates that require provider-side, physical-host, policy, signing, recovery or independent review evidence.

No current record is required for the repository to build. An empty directory means that no external gate has been promoted. It never means production readiness.

## Authority and invariants

External evidence semantics have one current owner:

```text
strict repository JSON/filesystem adapter
        -> typed ExternalEvidenceRecordV1
        -> opsctl-core external_evidence policy
        -> deterministic readiness projection
```

`tools/opsctl/core/src/external_evidence.rs` owns gate identities, required checks, allowed environments, terminal-state semantics, immutable supersession lineage and the mandatory production-review matrix. The permanent `External Evidence Metadata` and `External Readiness Projection` jobs execute that policy through `tools/opsctl/tests/external_evidence_policy.rs`.

`scripts/check-external-review-attestations.py` remains only the outer GitHub GET observer for terminal review objects. Typed Rust `opsctl hosted-evidence external-review-attestation verify` owns repository/reference/reviewer/timestamp/canonical-claim validity. Python does not own external-evidence trust/readiness semantics.

Rules:

1. one canonical JSON file per observation, named exactly `<evidence_id>.json`;
2. never edit an accepted record in place; add a newer record with `supersedes`;
3. use only a gate/environment/check combination accepted by the typed Rust policy;
4. store only opaque IDs, bounded check codes, sanitized references and SHA-256 artifact identities;
5. never store screenshots, logs, credentials, raw IP addresses, account names, browser data, host paths, certificates, keys or free-form provider output;
6. `pending` records carry no terminal review and cannot claim a failed final check;
7. terminal `passed`/`failed` records require an exact same-repository GitHub review/comment whose observed identity is verified by the outer observer plus typed Rust policy;
8. `passed` requires every required check to pass and at least one sanitized artifact digest;
9. supersession stays within one gate, is strictly newer, cannot fork/cycle/dangle and leaves at most one active lineage per gate;
10. production readiness remains independently fail-closed in `docs/status.json`; external evidence can only make a record eligible for later production review.

## Operator flow

Before external work, inspect `tools/opsctl/core/src/external_evidence.rs` and `docs/EXTERNAL_GATE_EXECUTION_RUNBOOK.md` for the accepted gate/check/environment contract. Perform the actual provider/host/legal/security action outside Git, retain sensitive source material in approved external storage, and create a sanitized canonical record only from observed facts.

For a terminal record, acquire the GitHub review object as a secret-free observation and verify it through the typed owner:

```text
python scripts/check-external-review-attestations.py \
  --repository iamaman11/part-crm-emai-profile \
  --output-observation-json /tmp/external-review-attestation-observation.json
cargo run --quiet --manifest-path tools/opsctl/Cargo.toml --locked -- \
  --root . \
  hosted-evidence external-review-attestation verify \
  --observation-json /tmp/external-review-attestation-observation.json
```

Repository evidence/scope/lineage/readiness is exercised by:

```text
cargo test --locked --manifest-path tools/opsctl/Cargo.toml -p opsctl-core external_evidence
cargo test --locked --manifest-path tools/opsctl/Cargo.toml --test external_evidence_policy
```

These tests are permanent enforcement callers, not a second semantic catalog: all gate/check/environment/readiness decisions come from the typed core.
