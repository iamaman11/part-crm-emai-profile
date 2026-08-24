# External Gate Execution Runbook

**Status:** normative external-operation guidance  
**Tracking:** issue #73, parent external-gates issue #3

## Purpose and authority

This runbook explains how to execute real external gates without turning repository-local or synthetic behavior into a production claim. Exact gate identities, allowed environments, required checks, terminal-state semantics, lineage and mandatory readiness requirements are owned by `tools/opsctl/core/src/external_evidence.rs`; this document is guidance, not a parallel semantic catalog.

For every gate:

1. inspect the typed Rust contract on the exact accepted source revision;
2. perform the real operation outside Git using approved provider/host/review storage;
3. keep raw logs, screenshots, certificates, credentials, key material, host/account/network identities and customer/profile data outside Git;
4. retain only sanitized metadata, opaque references and SHA-256 artifact identities in the repository record;
5. stop on any failed required observation instead of omitting/renaming/coercing it;
6. create terminal evidence only from actual observations plus independent GitHub review;
7. preserve accepted records and use a strictly newer `supersedes` record for later observations.

A staging observation is diagnostic unless the typed mandatory matrix accepts that environment. It never satisfies a production requirement merely because the gate name is the same.

## Gate procedures

### `legacy_credential_rotation`
Perform rotation at the credential provider. Prove the old credential is revoked and rejected, review provider access evidence, place the replacement only in approved secret storage and run the repository regression scan without exposing either value. Stop if the old credential still authenticates or the replacement appears in unapproved storage.

### `cloudflare_environment`
Use truly isolated resources for the selected allowed environment, enforce the intended Access boundary and cost controls, deploy only accepted artifacts and execute the real remote smoke path. Do not use local/miniflare substitutes as evidence of hosted isolation.

### `windows_primary_host`
Use an approved physical Windows host and the exact accepted Profile Bridge release. Exercise the real Camouhost/Camoufox lifecycle and review support material for metadata-only behavior. Stop on synthetic runtime fallback, release-identity ambiguity or sensitive support output.

### `windows_secondary_host`
Use an operationally independent second physical host. Apply the real device grant, restore and launch the accepted generation, revoke authorization and prove the revoked device can no longer proceed.

### `trusted_windows_signing`
Sign the exact release through the approved trusted signing service. Verify trusted chain, signed-binary digest and Windows/update verification. Private keys and signing credentials never enter repository evidence.

### `offline_key_escrow_restore`
Exercise dual-control recovery in a clean environment with no pre-existing production key material. Prove recovery, rotation/recovery and the approved key-loss policy. Stop if one operator can bypass dual control or undocumented secret material is required.

### `privacy_retention_approval`
Obtain explicit owner/legal/privacy approval for concrete retention, acceptable-use, export/delete and support-access policies. Record approved policy identities and review decision only, never customer data.

### `product_license`
Select the product/repository license, review third-party notices and confirm redistribution rights for shipped components/runtimes. Build success is not licensing approval.

### `real_fingerprint_certification`
Run the accepted real-browser certification matrix on the required physical/runtime surface, including cold launches, stable and origin-deterministic signals, network coherence, specialized-site review and cross-profile isolation. Raw fingerprint/network/account/profile values stay external.

### `production_device_key_unwrap`
Exercise the production OS-backed device-key path on an approved device. Verify device identity, unwrap authorization, revocation and documented recovery. Plaintext key material or key-store exports must never escape the approved boundary.

### `remote_r2_d1_atomicity`
Use real remote Cloudflare R2/D1 resources in the selected allowed environment. Prove immutable upload, pointer CAS, nonce claim, rollback and orphan reconciliation under real failure/race conditions. Object contents, database rows, credentials and raw requests remain external.

### `independent_security_review`
Use a reviewer independent of the implementation acceptance path. Review the current threat model and cryptographic/security boundaries, resolve findings or record explicit risk acceptance, and obtain residual-risk approval from the owning authority.

## Repository and terminal-review validation

Repository evidence/scope/lineage/readiness uses the typed owner and strict Rust adapter:

```bash
cargo test --locked --manifest-path tools/opsctl/Cargo.toml -p opsctl-core external_evidence
cargo test --locked --manifest-path tools/opsctl/Cargo.toml --test external_evidence_policy
```

For terminal GitHub review objects, the Python adapter performs GET-only acquisition and typed Rust makes the semantic decision:

```bash
python scripts/check-external-review-attestations.py \
  --repository iamaman11/part-crm-emai-profile \
  --output-observation-json /tmp/external-review-attestation-observation.json
cargo run --quiet --manifest-path tools/opsctl/Cargo.toml --locked -- \
  --root . \
  hosted-evidence external-review-attestation verify \
  --observation-json /tmp/external-review-attestation-observation.json
```

Passing these checks proves only the integrity of the recorded metadata and review binding. It proves none of the external operations by itself and never authorizes production mutation.
