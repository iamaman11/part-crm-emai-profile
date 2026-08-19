# Architecture Re-baseline v3 — AR-10 Runtime Cutover Evidence

**Document status:** CURRENT_IMPLEMENTATION_CANDIDATE  
**Owning issue:** #368  
**Parent program:** #266  
**Accepted predecessor:** AR-9 / #366 / PR #367  
**Exact start base:** `main@5933a5e30a534209138485556b4a895706af765a`  
**Canonical implementation branch:** `agent/ar10-runtime-cutover`  
**Production mutation:** `false`

## Purpose

AR-10 converts the accepted browser-profile lifecycle/security primitives into the supported repository-owned real Camoufox runtime boundary and retires historical direct executable authority only after permanent parity evidence exists.

This document is implementation evidence, not a second program roadmap. `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` remains the canonical execution authority and `docs/CAMOUFOX_RUNTIME_CUTOVER_PLAN.md` remains the subordinate design input.

## Single execution authority

- #368 is the sole owning AR-10 issue.
- #369 is duplicate history and is not an execution authority.
- `agent/ar10-runtime-cutover` is the sole implementation branch.
- Any other pre-existing empty AR-10 branch is non-authoritative and must not receive parallel implementation.
- AR-10 is delivered as one Draft completion PR and one final unchanged exact-head candidate.

## Accepted starting facts

The implementation starts from these repository truths:

1. `runtime/camouhost/main.py` is a deterministic synthetic fixture and does not launch Camoufox.
2. `tools/profile_browser.py` is research-only but already demonstrates real Camoufox startup, a persistent `user_data_dir`, one-time fingerprint-config materialization, config SHA-256 identity and repeat-launch probing.
3. Profile Bridge already owns materialization freshness, writer ownership, browser-lock recovery, network preflight, runtime preflight, dirty-generation publication and fenced/CAS commit semantics.
4. `BrowserIdentityManifest` and runtime bundle primitives already exist but need AR-10 compatibility identity expansion rather than replacement by a parallel lifecycle.
5. ADR-0001 remains proposed until executable AR-10 evidence supports its acceptance or an explicit replacement decision is made.
6. AR-6 owns the accepted Python estate classification: the six `DELETE_AFTER_SEQUENCE` browser/profile executables are retired only after successor parity; the remaining Python estate is not subject to a global Python-to-Rust rewrite.
7. AR-10 also removes the final `opsctl doctor -> Python validators` child-process compatibility bridge while retaining legitimate standalone Python validators.

## Binding supported topology

```text
native Rust Profile Bridge
  -> approved exact runtime bundle
  -> typed/versioned IPC
  -> real managed Camouhost outer adapter
  -> exact pinned Camoufox + BrowserForge + Playwright + browser
  -> visible persistent browser context
```

Synthetic Camouhost remains test-only and must be mechanically impossible to select as the supported runtime.

## Runtime candidate

Primary AR-10 candidate, subject to the permanent compatibility matrix:

- Camoufox Python distribution: `0.5.4`
- browser line: `152.0.4-beta.28`
- target platform: Windows x86_64
- BrowserForge / Playwright / Python: exact compatible versions and artifact/inventory identities must be pinned before project acceptance
- floating `latest` or channel resolution at launch: forbidden

A replacement candidate is allowed only if permanent AR-10 evidence demonstrates a blocker and records the replacement explicitly.

## Identity contract requirements

Persistent browser state and persistent browser identity are separate contracts.

For an existing generation:

- browser state lives in a stable generation-owned `user_data_dir`;
- one complete canonical fingerprint config is materialized and retained;
- every allowed relaunch reuses that exact config rather than invoking random/default BrowserForge identity generation;
- canonical config bytes have one SHA-256 identity;
- fingerprint policy version and config schema version are explicit compatibility inputs;
- runtime compatibility binds exact component identity, not a floating release label;
- an incompatible runtime/policy/config transition requires a candidate generation and recertification rather than in-place mutation.

Raw fingerprint config, profile entropy, profile payload and credentials are not normal log/audit/support evidence.

## Safety invariants

Real Camoufox execution must remain behind the existing Profile Bridge lifecycle:

- exact active generation and freshness;
- approved runtime bundle identity;
- coordinator lease / epoch / fencing;
- one-writer ownership;
- browser-lock recovery without blind lock deletion;
- network identity preflight;
- immutable encrypted generation publication;
- exact upload verification;
- fenced/CAS authoritative commit after a proven clean close;
- crash, forced termination, child loss or ambiguous close becomes recovery-required and never reports a clean generation.

Camouhost does not become cloud/profile lifecycle authority.

## Implementation order

1. Correct stale post-AR-9 projections so they truthfully state `AR-9 accepted / AR-10 current` without claiming AR-10 acceptance.
2. Expand the generation/runtime identity contract and permanent positive/negative compatibility tests.
3. Add an explicitly real Camouhost persistent-context adapter while preserving an explicitly synthetic fixture.
4. Add the native Profile Bridge process/IPC adapter behind existing preflight and lifecycle ports.
5. Pin and verify the exact runtime component tuple and inventory identities before browser launch.
6. Prove parity with the useful behavior of `tools/profile_browser.py`.
7. Retire the six AR-10-owned `DELETE_AFTER_SEQUENCE` executables only after parity/reference gates pass.
8. Remove the final `opsctl` Python/Node child-process spawn authority.
9. Run the complete permanent positive/negative/replay/failure matrix on one unchanged exact head.
10. Only after guarded merge and accepted-main reread may projections advance to `AR-10 accepted / AR-11 next`.

## Repository-owned acceptance floor

AR-10 cannot close without permanent evidence for at least:

- repeated cold launch of one generation preserves the canonical fingerprint config digest and accepted profile-stable identity;
- cookie and localStorage state survive a clean relaunch through the generation-owned `user_data_dir`;
- different generations do not share state or identity directories;
- missing, partial or digest-mismatched identity fails before Camoufox launch;
- runtime component mismatch fails before Camoufox launch;
- incompatible runtime/policy identity requires candidate-generation migration;
- malformed, oversized, unsupported or replayed IPC fails closed;
- child death before ready and close/protocol ambiguity never report `clean=true`;
- writer/Firefox lock contention remains fail closed and no browser lock is blindly deleted;
- clean close preserves the existing dirty-generation publish/verify/fenced-CAS ordering;
- crash/forced/ambiguous close stays recovery-required;
- synthetic and real runtime roles are mechanically distinct;
- six historical direct executables have zero accepted authority/reference after retirement;
- final `opsctl` production code has zero Python/Node process-spawn sites;
- normal evidence contains metadata/digests only, not raw fingerprint/profile/secret material.

## Explicit non-goals

AR-10 performs no production provisioning or mutation, no Terraform/generic IaC work, no AR-11 release/promotion cutover, no AR-15 signing/updater/side-by-side/LKG/rollback implementation, and no claim of physical Windows, real proxy-provider or specialized fingerprint-site certification.

Throughout AR-10:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```
