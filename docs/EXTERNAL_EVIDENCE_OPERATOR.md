# External Evidence Operator Workflow

**Status:** normative external-operation guidance  
**Tracking:** issue #69, parent external-gates issue #3

## Purpose

External production gates require real provider, physical-host, signing, recovery, policy or independent-review actions outside repository CI. Repository records are sanitized observations of that work; repository tooling must never manufacture a terminal `passed`/`failed` claim.

The current semantic authority is typed Rust in `tools/opsctl/core/src/external_evidence.rs`. It owns gate identities, required checks, allowed environments, terminal-state validity, supersession lineage and mandatory production-review requirements. `tools/opsctl/tests/external_evidence_policy.rs` is the strict repository adapter/permanent enforcement caller. There is no separate Python gate/readiness semantic catalog or pending-draft generator.

For the real operation, use [`EXTERNAL_GATE_EXECUTION_RUNBOOK.md`](EXTERNAL_GATE_EXECUTION_RUNBOOK.md). Before acting, inspect the exact accepted typed gate contract on the same source revision that will carry the evidence.

## Observation flow

1. Perform the real external operation through the approved provider, physical host, signing service, policy/review process or recovery environment.
2. Keep credentials, host paths, logs, screenshots, browser/mailbox payloads, certificates, account identifiers, raw network identities and key material outside Git.
3. Create a new canonical metadata-only JSON record under `evidence/external/records/`; never mutate accepted evidence in place.
4. Use an evidence ID whose UTC date matches `observed_at`, one typed-policy-approved environment, an opaque subject ID, sanitized references and bounded limitations.
5. For `passed`, include every typed-policy-required gate check as `pass`, at least one SHA-256 identity of a sanitized review artifact, and an exact same-repository GitHub terminal review object.
6. For `failed`, include a terminal review and at least one required check with outcome `fail`.
7. For a newer observation, point `supersedes` at the prior immutable record. Forks, cycles, dangling parents, cross-gate supersession and non-newer successors fail closed.
8. Run the permanent typed Rust validation before requesting acceptance.

A `pending` record is deliberately non-evidentiary: it has no terminal review, cannot contain a final failed check and satisfies no mandatory production-review requirement.

## Repository validation

From repository root:

```bash
cargo test --locked --manifest-path tools/opsctl/Cargo.toml -p opsctl-core external_evidence
cargo test --locked --manifest-path tools/opsctl/Cargo.toml --test external_evidence_policy
```

The first command tests the pure semantic owner. The second exercises strict JSON decoding, duplicate-key/canonical-byte rejection, privacy/scope lexical boundaries, typed policy integration, lineage fixtures, deterministic readiness projection and `production_ready` fail-closed behavior.

## Terminal GitHub review observation

Provider/GitHub acquisition remains outside pure core. For terminal records, collect the exact GitHub review/comment object with the observer-only Python adapter and pass its secret-free DTO to typed Rust:

```bash
python scripts/check-external-review-attestations.py \
  --repository iamaman11/part-crm-emai-profile \
  --output-observation-json /tmp/external-review-attestation-observation.json
cargo run --quiet --manifest-path tools/opsctl/Cargo.toml --locked -- \
  --root . \
  hosted-evidence external-review-attestation verify \
  --observation-json /tmp/external-review-attestation-observation.json
```

The Python step owns GitHub GET acquisition only. It does not decide active lineage, repository binding, reviewer identity, timestamps, canonical claim validity or readiness. `opsctl` itself performs no provider/GitHub network access and reads no provider credentials.

## Safety properties

The permanent checks reject unknown fields/duplicate JSON keys, noncanonical records, unsafe/sensitive identifiers and references, unsupported gates/statuses/environments/checks, evidence-ID/UTC-date mismatch, invalid terminal state, incomplete passed evidence, unsafe supersession lineage, false readiness projection and `production_ready=true` when mandatory external evidence is incomplete.

A passing repository validator proves only the integrity of recorded observations. It does not prove that an external operation occurred, does not authorize production and does not weaken the independent Production Core gate.
