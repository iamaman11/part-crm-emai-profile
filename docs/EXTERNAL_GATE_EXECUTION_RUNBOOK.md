# External Gate Execution Runbook

**Status:** normative external-operation guidance  
**Tracking:** issue #73, parent external-gates issue #3  

## Purpose

This runbook describes how an operator should execute each real external gate without
turning repository-local or synthetic behavior into a production claim. The exact gate
names, allowed environments and required terminal check codes remain authoritative in
`scripts/check-external-evidence.py` and `scripts/check-external-evidence-scope.py`.
Always inspect that live contract immediately before an external operation.

For every gate:

1. run the gate-specific `describe` command in the section below;
2. create only a `pending` metadata draft with `prepare-external-evidence.py draft` if
   a repository envelope is useful before the operation;
3. perform the real operation outside Git using approved provider/host/review storage;
4. retain raw logs, screenshots, certificates, key material, host identifiers and other
   sensitive artifacts only in approved external storage;
5. produce a sanitized review artifact and record only its SHA-256 identity plus an
   allowed opaque reference in Git;
6. stop on any failed required observation; do not coerce, omit or rename a required
   check to obtain a pass;
7. create terminal evidence only as a newer immutable record after the real observation
   and independent GitHub review. The preparation CLI intentionally cannot generate it.

A staging observation is diagnostic unless the mandatory readiness matrix explicitly
accepts that environment. It never proves a production requirement merely because the
same gate name is used.

## Gate procedures

<!-- external-gate: legacy_credential_rotation -->
### Legacy credential rotation

```bash
python scripts/prepare-external-evidence.py describe --gate legacy_credential_rotation
```

Perform the rotation at the credential provider, not in repository code. Revoke or
rotate the compromised value, independently prove that the old value no longer
authenticates, review provider access/usage logs for suspicious use, place the
replacement only in the approved secret store, and run the repository regression scan
without printing either credential. Preserve only an opaque provider case/reference and
a sanitized incident-review digest. Stop if the old credential still authenticates, log
review cannot be completed, or the replacement exists in source, local config or other
unapproved storage. This gate is also tracked by issue #1.

<!-- external-gate: cloudflare_environment -->
### Isolated Cloudflare environment

```bash
python scripts/prepare-external-evidence.py describe --gate cloudflare_environment
```

Provision truly isolated resources for the target environment, apply the intended
Cloudflare Access policy, configure a bounded cost/budget control, deploy the accepted
artifact and execute the real remote smoke path against those resources. Record only
opaque resource/environment identifiers and sanitized report digests. Do not copy API
tokens, account identifiers, request headers, raw IPs or customer/profile payloads into
Git. Stop if resources are shared across an isolation boundary, Access can be bypassed,
cost controls are absent, or the remote smoke uses local/miniflare substitutes.

<!-- external-gate: windows_primary_host -->
### Primary physical Windows host

```bash
python scripts/prepare-external-evidence.py describe --gate windows_primary_host
```

Use an approved physical Windows machine, execute the exact accepted Profile Bridge
release, exercise the real Camouhost/Camoufox lifecycle rather than the fake runtime, and
review the generated support material for metadata-only behavior. Keep machine names,
user paths, serial numbers, raw process logs and profile contents outside Git. Stop if
execution falls back to a synthetic runtime, the binary identity is not the intended
release, lifecycle state is ambiguous, or the support bundle exposes sensitive data.

<!-- external-gate: windows_secondary_host -->
### Independent secondary Windows host

```bash
python scripts/prepare-external-evidence.py describe --gate windows_secondary_host
```

Use a second physical host that is operationally independent of the primary host. Apply
a real device grant, restore and launch the accepted profile generation on that host,
then revoke authorization and verify that the revoked device can no longer proceed.
Preserve only opaque device/evidence identifiers and sanitized report digests. Stop if
the host is merely another VM/session on the primary device, authorization is inferred
from assignment rather than an explicit grant, restore integrity is uncertain, or
revocation is not enforced.

<!-- external-gate: trusted_windows_signing -->
### Trusted Windows signing

```bash
python scripts/prepare-external-evidence.py describe --gate trusted_windows_signing
```

Use the approved trusted code-signing service/certificate to sign the exact release
binary. Verify the trusted chain and exact signed-binary digest with Windows-native
verification, then exercise the update verification path against the signed identity.
Private keys, certificate material and signing-service credentials remain outside Git;
Git receives only sanitized artifact identities/references. Stop on any untrusted chain,
digest mismatch, unverifiable signature, or update path that accepts an unsigned or
wrong release.

<!-- external-gate: offline_key_escrow_restore -->
### Offline key escrow restore

```bash
python scripts/prepare-external-evidence.py describe --gate offline_key_escrow_restore
```

Run a dual-control recovery drill in a clean environment with no pre-existing production
key material. Restore through the approved escrow procedure, prove the recovered system
can complete the intended recovery path, exercise rotation/recovery, and confirm the
approved key-loss policy. Never place escrow payloads, key bytes, recovery phrases or
operator identities in Git. Stop if a single operator can bypass dual control, the clean
environment contains residual key material, or recovery requires undocumented secrets.

<!-- external-gate: privacy_retention_approval -->
### Privacy, retention and acceptable-use approval

```bash
python scripts/prepare-external-evidence.py describe --gate privacy_retention_approval
```

Obtain explicit product-owner/legal/privacy approval for concrete retention values,
acceptable-use constraints, export/delete behavior and support-access policy. The review
artifact should identify the approved policy versions and decision scope without
including customer data or free-form incident material. Stop if any required policy is
still draft, contradictory, unowned, or lacks a reviewable approval reference.

<!-- external-gate: product_license -->
### Product license and redistribution review

```bash
python scripts/prepare-external-evidence.py describe --gate product_license
```

Select the repository/product license, review third-party notices and confirm
redistribution rights for every shipped component/runtime that needs them. Preserve a
sanitized legal/review decision artifact and its digest; do not treat repository-local
build success as redistribution authorization. Stop if any bundled dependency/runtime
has unresolved licensing terms or the selected license is not approved for the intended
distribution model.

<!-- external-gate: real_fingerprint_certification -->
### Real fingerprint certification

```bash
python scripts/prepare-external-evidence.py describe --gate real_fingerprint_certification
```

Run the required real-browser certification matrix on the accepted physical/runtime
surface, including the required cold launches, stable-signal checks,
origin-deterministic behavior, network coherence, specialized-site review and
cross-profile isolation. Record aggregate/sanitized outcomes only; raw fingerprint
values, account data, IPs, cookies and profile contents stay outside Git. Stop on any
unresolved required failure, synthetic/fake browser substitution, or cross-profile
signal leakage.

<!-- external-gate: production_device_key_unwrap -->
### Production device-key unwrap

```bash
python scripts/prepare-external-evidence.py describe --gate production_device_key_unwrap
```

Exercise the production OS-backed key-protection path on an approved device. Verify the
device identity and unwrap authorization, then prove revocation and the documented
recovery path. Key bytes, OS key-store exports, recovery material and device serials are
never repository artifacts. Stop if plaintext key material escapes the approved
boundary, authorization can be bypassed, revocation is ineffective, or recovery depends
on an unreviewed path.

<!-- external-gate: remote_r2_d1_atomicity -->
### Remote R2/D1 atomicity and reconciliation

```bash
python scripts/prepare-external-evidence.py describe --gate remote_r2_d1_atomicity
```

Use real remote Cloudflare R2/D1 resources in the selected allowed environment. Prove
immutable generation upload behavior, pointer CAS conflict handling, the nonce-claim
boundary, rollback semantics and orphan reconciliation under real remote failure/race
conditions. Evidence in Git must remain metadata-only: object contents, database rows,
credentials, account IDs and raw request logs remain external. Stop on lost updates,
mutable generation objects, nonce reuse/ambiguous claims, rollback identity mismatch or
unreconciled orphan behavior.

<!-- external-gate: independent_security_review -->
### Independent security and cryptographic review

```bash
python scripts/prepare-external-evidence.py describe --gate independent_security_review
```

Engage a reviewer who is independent of the implementation acceptance path. Review the
current threat-model scope and cryptographic/security boundaries, track findings to
resolution or explicit risk acceptance, and obtain a residual-risk decision from the
appropriate owner. Store the full report in approved review storage; Git receives only
an allowed opaque reference/digest and the terminal GitHub attestation. Stop if reviewer
independence is not credible, review scope excludes a required security boundary, or an
unresolved finding lacks explicit risk acceptance.

## Terminal review boundary

After a real gate execution, use the existing external evidence protocol rather than
editing a pending draft in place. A terminal record must supersede prior evidence when
appropriate, carry only exact validator-defined checks, contain sanitized artifact
identities, and receive the exact same-repository GitHub terminal review attestation.
Passing this runbook coverage check proves only that operator guidance exists for every
accepted gate; it proves none of the external operations themselves.
