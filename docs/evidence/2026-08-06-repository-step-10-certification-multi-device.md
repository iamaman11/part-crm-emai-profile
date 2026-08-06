# Repository Step 10 — Certification And Multi-Device Evidence

**Дата:** 2026-08-06  
**Статус:** accepted bounded synthetic implementation evidence  
**Baseline:** `71296404dd5ffb78faf9033cbbb6b6fa395f72cd`  
**Accepted source head:** `7d5ba8c2a00bac256a9365a40dee7e3c28ef5b56`  
**Pull request:** #33  
**Tracking issue:** #32  
**Exact-head Quality Gate:** `31074745842`  
**Exact-head Certification Gate:** `31074745854`  
**Encrypted Generation regression:** `31074745859`  
**Local Profile regression:** `31074745880`  
**Runtime Bundle regression:** `31074745848`  
**Squash merge:** `3ddde2f48ddf82decf66c933ae5326a4455263e5`

## 1. Accepted Boundary

Step 10 adds one pure Rust certification domain and a permanent Linux, Windows
and Workers WASM gate. The accepted scope contains three repository-local
synthetic contracts:

1. a versioned required/optional/prohibited signal policy and deterministic
   repeatability/drift matrix;
2. device-scoped generation authorization with monotonic grant versions,
   immediate revoke and immutable grant-event history;
3. preverified update evidence, side-by-side activation, exact health identity
   and deterministic rollback/fail-closed state transitions.

ADR-0001 remains `proposed`. No real browser signal, physical second device,
production key, certificate or trusted signature was used.

## 2. Certification Policy And Matrix

The policy requires a non-zero version, at least one required signal, unique
bounded signal names and explicit requirement for every signal. Duplicate rules,
unknown observations, duplicate observation sequences and prohibited signals
fail closed.

Outcome precedence is deterministic:

```text
PROHIBITED > INCOMPLETE > DRIFTED > STABLE
```

Observation order does not change the canonical SHA-256 matrix identity. The
accepted synthetic regression vector is:

```text
6667869272890abc935bdfe135c849e03bcb1ba5c93cd76f588dc390fae9f765
```

Raw observation types do not implement `Debug`. The metadata-only support summary
exposes aggregate counts and outcome only; it excludes signal names, raw values
and the value-derived matrix digest.

## 3. Device Authorization

A grant key binds typed tenant, profile, generation and device IDs. New grants
start at version 1. Grant, revoke and regrant require the exact current version
and non-regressing time. Revocation immediately denies new unwrap authorization,
while another independently keyed synthetic device remains active.

Every successful grant/revoke/regrant appends an immutable event snapshot with
key, version, status and time. Failed stale operations append no event. Support
output exposes aggregate active/revoked/history counts only and omits typed IDs.

This proves state-machine semantics, not DPAPI/CNG/TPM unwrap, remote key delivery
or two physical computers.

## 4. Update Evidence And Rollback

`PreverifiedSignatureEvidence` is structurally bound to the exact release ID,
version and content digest. A release candidate is rejected when an opaque
verification record approves another artifact.

Staging additionally checks the observed content digest and a strictly increasing
release version. Activation enters `AWAITING_HEALTH`. Health success and failure
must identify the exact active release ID, version and digest, preventing a stale
signal from confirming a newer artifact that reused an opaque release ID.

A failed update restores the previous approved release and returns an explicit
`RollbackOutcome::Restored`. A failed first installation has no invented rollback
target: the candidate is removed from the active slot and the controller enters
`FAILED` with `NoPreviousRelease`. Previously seen failed versions cannot be
replayed.

The pure domain consumes already verified evidence. It does not parse
certificates, verify signatures, execute an installer or activate a real binary.

## 5. Permanent Policy And Negative Fixture

`scripts/check-step10-certification.py` requires the certification, journal,
evidence-binding, exact-health and rollback boundaries. It rejects unsafe code,
diagnostic output macros, raw signal support output, sensitive observation
`Debug`, matrix digest support output, legacy profile paths, platform/network SDKs,
temporary Step 10 workflows and the obsolete rollback error contract.

The deliberate fixture otherwise resembles the Step 10 surface but emits a raw
signal value from `render_metadata_only`; the dedicated gate proves it is
rejected.

## 6. Exact-Head CI

All permanent workflows succeeded on accepted source head
`7d5ba8c2a00bac256a9365a40dee7e3c28ef5b56`.

### Certification Gate `31074745854`

- policy compiled and passed;
- raw-signal-output fixture was rejected;
- rustfmt and warnings-denied Clippy passed;
- all certification, device authorization and update tests passed on Linux and
  Windows;
- the pure crate compiled for `wasm32-unknown-unknown`.

### Quality Gate `31074745842`

- all architecture, contract, D1, identity/ACL, coordinator, Bridge, runtime,
  local profile and encrypted-generation policies remained green;
- native workspace tests and Cloudflare adapter tests passed;
- pure crates compiled for Workers WASM;
- D1 migration apply/replay/schema checks passed;
- Windows produced and verified a non-empty release `profile-bridge.exe`;
- status validation and tracked-tree high-confidence secret scan passed;
- pinned Cloudflare Worker release artifact was built and verified.

### Regression Gates

- Encrypted Generation Gate `31074745859`: Linux/Windows/WASM success;
- Local Profile Gate `31074745880`: Linux/Windows success and Bridge artifact;
- Runtime Bundle Gate `31074745848`: Linux/Windows success and runtime/Bridge
  artifacts.

## 7. Defects Found And Corrected

Implementation review corrected the following before acceptance:

- strict Clippy findings were fixed without lint suppression;
- policies without any required signal were rejected;
- raw observation values lost accidental `Debug` exposure;
- matrix digest was removed from support telemetry;
- device history changed from a counter to immutable auditable event snapshots;
- signature evidence was bound to exact release ID/version/digest;
- health signals were bound to exact release ID/version/digest;
- first-install failure received an explicit fail-closed `FAILED` state;
- the obsolete `RollbackUnavailable` API was removed and prohibited by policy;
- temporary bootstrap scripts/workflows were removed before accepted source head.

## 8. Explicit Limitations And Remaining Gates

This evidence does not prove:

- ADR-0001 production signal selection or tolerance values;
- real Camoufox/Camouhost fingerprint stability or browser-version drift;
- repeatability across real hardware, networks or a second Windows host;
- device attestation, DPAPI/CNG/TPM key unwrap or production generation-key
  delivery;
- cryptographic signature verification, trusted Windows signing or installer
  execution;
- physical update activation/rollback;
- remote Cloudflare production behavior, key escrow/account-loss recovery or
  independent security review;
- production readiness.

The roadmap defines no Repository Step 11. Remaining work is represented as
external production evidence gates rather than an invented numbered step.
`production_ready` remains `false`; ADR-0001 and ADR-0006 remain proposed.
