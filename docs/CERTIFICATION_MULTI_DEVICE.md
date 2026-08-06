# Certification And Multi-Device Boundary

**Статус:** synthetic implementation/review candidate  
**Repository Step:** 10  
**ADR-0001:** remains `proposed`  
**Production readiness:** false

## 1. Scope

This document defines the repository-local portion of certification and
multi-device behavior that can be proven without real browser signals, a second
physical Windows host, production device keys or trusted code signing.

The implementation provides three pure state-machine boundaries:

1. versioned signal policy and deterministic repeatability/drift matrix;
2. device-scoped synthetic generation authorization with monotonic grant version
   and immediate revoke for new unwrap authorization;
3. side-by-side update activation and rollback that consumes already verified
   signature evidence rather than performing production signature verification.

No result from this step is a production fingerprint certification.

## 2. Certification Policy

A policy has a non-zero version, at least one required signal and a unique
sorted set of signal rules. Every rule is exactly one of:

- `required`: present in every observation and within tolerance;
- `optional`: absence is allowed, but present values are evaluated for drift;
- `prohibited`: presence produces a distinct fail-closed outcome and tolerance
  must be zero.

Signal names are bounded lowercase opaque identifiers. Unknown signals,
duplicate rules, duplicate observation sequences and invalid tolerances are
rejected rather than silently ignored.

The outcome precedence is:

```text
PROHIBITED > INCOMPLETE > DRIFTED > STABLE
```

This prevents a prohibited observation from being hidden by another missing or
drifted signal.

## 3. Deterministic Matrix

Observation input order does not affect the matrix identity. Observation
sequences and signal names are canonicalized, and the policy plus bounded numeric
observations are hashed with SHA-256 under an explicit schema domain.

The internal report exposes policy version, observation count, aggregate result
counts, outcome and matrix digest for controlled evidence comparison. The
metadata-only support renderer omits the matrix digest as well as raw signal names
and values, preventing a value-derived identifier from becoming support telemetry.

The numeric observations in tests are synthetic buckets. They do not model or
certify actual canvas, WebGL, audio, timezone, font or network behavior.

## 4. Synthetic Device Authorization

A grant key binds four typed opaque IDs:

```text
tenant + profile + generation + device
```

New grants start at version 1. Grant and revoke operations require the exact
current version and strictly non-regressing time. Revocation advances the
version and immediately denies new authorization using either the current
revoked version or any stale version.

A revoked device can be explicitly regranted only with the exact revoked version;
the new active grant receives another monotonic version. A second synthetic
device has an independent key and remains authorized when the first is revoked.

This proves contract semantics only. It does not unwrap a DEK, use DPAPI/CNG/TPM,
contact a remote key service or represent two physical machines.

## 5. Update And Rollback

A release candidate contains:

- bounded opaque release ID;
- non-zero monotonic release version;
- exact non-zero content digest;
- opaque evidence that an external verifier already approved the signature.

The pure domain does not parse certificates or verify signatures. The
`PreverifiedSignatureEvidence` name is intentional: production adapters must
supply the proof only after trusted signature and policy verification.

Staging requires exact content identity and a version greater than every release
previously seen by the controller. Activation is side-by-side and enters
`AWAITING_HEALTH`. Successful health confirmation marks the release healthy.
Failed health confirmation restores the previous approved release. A first
installation has no rollback target and fails closed rather than inventing one.
A failed higher version cannot be replayed after rollback.

## 6. Privacy And Support Output

Certification, device and update support summaries expose aggregate counts,
versions and state only. The certification matrix digest remains available to
controlled internal evidence but is omitted from support output. They exclude:

- raw signal names and values;
- tenant, profile, generation and device IDs;
- release IDs and content digests;
- verifier/evidence IDs;
- profile payload, cookies, mailbox data and key material.

## 7. Permanent Evidence Gate

The dedicated Step 10 workflow must prove on Linux and Windows:

- positive policy enforcement and deliberate raw-signal-output fixture rejection;
- rustfmt and warnings-denied Clippy;
- all certification, device and update state-machine tests;
- pure crate compilation for `wasm32-unknown-unknown`;
- Profile Bridge and prior Step 7–9 regression gates remain green through the
  repository Quality Gate.

## 8. Explicit Limitations

This step does not prove or accept:

- ADR-0001 production signal policy;
- real Camoufox/Camouhost observations or fingerprint stability;
- drift/repeatability across real hardware, networks or browser versions;
- a second independent Windows evidence host;
- production key delivery, device unwrap or device attestation;
- cryptographic signature verification, trusted Windows signing or installer
  execution;
- physical update activation or rollback;
- production cloud behavior or production readiness.

All tests use synthetic IDs, observations, digests and preverified-evidence
identifiers. External device/signing/certification gates remain open.
