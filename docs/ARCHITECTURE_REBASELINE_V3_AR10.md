# Architecture Re-baseline v3 — AR-10 Runtime Cutover Evidence

**Document status:** CURRENT_IMPLEMENTATION_CANDIDATE  
**Owning issue:** #368  
**Parent program:** #266  
**Accepted predecessor:** AR-9 / #366 / PR #367  
**Exact start base:** `main@5933a5e30a534209138485556b4a895706af765a`  
**Canonical implementation branch:** `agent/ar10-runtime-cutover`  
**Completion PR:** #371 (Draft until acceptance matrix is complete)  
**Production mutation:** `false`

## Purpose

AR-10 converts the accepted browser-profile lifecycle/security primitives into the supported repository-owned real Camoufox runtime boundary and retires historical direct executable authority only after permanent parity evidence exists.

This document is implementation evidence, not a second program roadmap. `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` remains the canonical execution authority and `docs/CAMOUFOX_RUNTIME_CUTOVER_PLAN.md` remains the subordinate design input.

## Single execution authority

- #368 is the sole owning AR-10 issue.
- #369 is duplicate history and is not an execution authority.
- `agent/ar10-runtime-cutover` is the sole implementation branch.
- #371 is the single Draft completion PR.
- Any other pre-existing empty AR-10 branch is non-authoritative and must not receive parallel implementation.
- AR-10 is delivered as one final unchanged exact-head candidate after all repository-owned evidence is green.

## Accepted starting facts

The implementation starts from these repository truths:

1. `runtime/camouhost/main.py` is a deterministic synthetic fixture and does not launch Camoufox.
2. `tools/profile_browser.py` was the research-only direct-launch predecessor; successor parity was mechanically proven on `fdaf9e5071bf48b25bb0a60e95895fe349bc789f`, after which the file was retired with the rest of the exact AR-10 cohort.
3. Profile Bridge already owns materialization freshness, writer ownership, browser-lock recovery, network preflight, runtime preflight, dirty-generation publication and fenced/CAS commit semantics.
4. `BrowserIdentityManifest` and runtime bundle primitives already exist and are extended through generation-scoped identity evidence rather than replaced by a parallel lifecycle.
5. ADR-0001 is accepted only after executable AR-10 parity evidence established the repository-owned identity/runtime contract.
6. AR-6 remains the immutable Python-estate baseline; after successor parity the six `DELETE_AFTER_SEQUENCE` browser/profile executables were retired through the bounded AR-10 overlay, with no global Python-to-Rust rewrite.
7. AR-10 also removes the final `opsctl doctor -> Python validators` child-process compatibility bridge while retaining legitimate standalone Python validators.

## Binding supported topology

```text
native Rust Profile Bridge
  -> approved exact runtime bundle
  -> successful generation/runtime/network/writer preflight
  -> one-time runtime launch binding
  -> managed typed/versioned IPC
  -> real Camouhost outer adapter
  -> exact pinned Camoufox + BrowserForge + Playwright + browser
  -> visible persistent browser context
```

Synthetic Camouhost remains test-only and must be mechanically impossible to select as the supported real-runtime entrypoint.

## Exact runtime candidate

Fresh upstream revalidation superseded the earlier planning-only `camoufox==0.5.4` candidate before acceptance. The current exact candidate is:

- Camoufox Python distribution: `0.5.5`;
- exact Camoufox Python source commit: `cd83f7fd2fdf631dfde0c7eb53bd3d30f102ec4a`;
- Camoufox browser: `152.0.4-beta.28`;
- exact browser release commit: `0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d`;
- BrowserForge: `1.2.4`;
- Playwright: `1.60.0`;
- Python: `3.12`;
- target delivery platform: Windows x86_64;
- repository integration evidence platform: Linux virtual-headful plus Windows Bridge regression;
- floating `latest` or channel resolution at normal launch: forbidden.

`runtime/camouhost/runtime-lock.json` is the machine-readable tuple. It is a candidate until the permanent compatibility matrix passes on the final exact PR head. A replacement is allowed only by an explicit reviewed candidate correction with evidence; silent downgrade/relaxation is forbidden.

## Identity contract requirements

Persistent browser state and persistent browser identity are separate contracts.

For an existing generation:

- browser state lives in a stable generation-owned `user_data_dir`;
- one complete canonical `camoufox-config.json` is materialized once for the candidate generation and retained;
- `BrowserIdentityManifest.fingerprint_config_sha256` binds the exact canonical config bytes;
- `BrowserIdentityManifest.fingerprint_source` is versioned as `profile-stability-v1-probe-<sha256>` and binds the accepted profile-stable probe without creating a second identity sidecar authority;
- the approved runtime-bundle inventory binds both the exact real Camouhost entrypoint and `runtime-lock.json`, while `BrowserIdentityManifest.runtime_version/runtime_inventory_sha256` binds the selected bundle to the materialized generation;
- every allowed relaunch reuses the exact retained config rather than invoking random/default BrowserForge identity generation;
- normal launch validates Bridge writer ownership, materialization freshness, network policy, runtime inventory identity, config digest and profile-stable probe before reporting `ready`;
- the one-time launch binding is published only after those checks and is consumed by the managed subprocess spawn exactly once;
- an incompatible runtime/policy/config transition requires a candidate generation and recertification rather than in-place mutation.

This deliberately uses the already accepted manifest + runtime-inventory authorities instead of introducing `camoufox-identity.json` or another mutable state file. Fewer authorities reduce drift and replay surface without weakening cryptographic binding.

Raw fingerprint config, profile entropy, profile payload and credentials are not normal log/audit/support evidence.

## Safety invariants

Real Camoufox execution remains behind the existing Profile Bridge lifecycle:

- exact active generation and freshness;
- approved runtime bundle identity;
- coordinator lease / epoch / fencing;
- one-writer ownership;
- browser-lock recovery without blind lock deletion;
- network identity preflight;
- cryptographically bound generation/runtime identity;
- immutable encrypted generation publication;
- exact upload verification;
- fenced/CAS authoritative commit after a proven clean close;
- crash, forced termination, child loss or ambiguous close becomes recovery-required and never reports a clean generation.

The real subprocess adapter consumes a one-time runtime binding published only after successful Bridge preflight. It cannot obtain launch authority from a direct filesystem path alone. Camouhost does not become cloud/profile lifecycle authority.

## Implementation order

1. Correct stale post-AR-9 projections so they truthfully state `AR-9 accepted / AR-10 current` without claiming AR-10 acceptance.
2. Complete the generation/runtime identity contract and permanent positive/negative compatibility tests.
3. Keep the explicitly synthetic fixture and add the explicitly real Camouhost persistent-context adapter.
4. Compose the native Profile Bridge process/IPC adapter behind existing preflight and lifecycle ports.
5. Pin and verify the exact runtime component tuple and inventory identities before browser launch.
6. Prove parity with the useful behavior of `tools/profile_browser.py`, including Bridge-mediated real-runtime execution.
7. Retire the six AR-10-owned `DELETE_AFTER_SEQUENCE` executables only after parity/reference gates pass.
8. Remove the final `opsctl` Python/Node child-process spawn authority.
9. Accept ADR-0001 only when the executable contract and evidence are present on the same candidate.
10. Run the complete permanent positive/negative/replay/failure matrix on one unchanged exact head.
11. Only after guarded merge and accepted-main reread may projections advance to `AR-10 accepted / AR-11 next`.

## Current implementation checkpoint

- accepted predecessor: AR-9 final green `6110a32ade85d08c6ad93d9064190fff768e7cc2`, accepted main `5933a5e30a534209138485556b4a895706af765a`;
- successor parity source head: `fdaf9e5071bf48b25bb0a60e95895fe349bc789f`;
- exact six AR-10 historical direct executables: retired after that parity proof;
- AR-10 Python estate overlay: 116 tracked, `DELETE_AFTER_SEQUENCE=0`;
- ADR-0001: accepted after executable evidence;
- real runtime production certification: still `false`; physical Windows/proxy/specialized-site evidence remains later;
- final exact-green PR head / guarded merge / accepted-main reread: still pending and must be recorded only after the final unchanged candidate passes.

## Repository-owned acceptance floor

AR-10 cannot close without permanent evidence for at least:

- repeated cold launch of one generation preserves the canonical fingerprint config digest and accepted profile-stable identity;
- cookie and localStorage state survive a clean relaunch through the generation-owned `user_data_dir`;
- different generations do not share state or identity directories;
- missing, partial or digest-mismatched identity fails before Camoufox launch;
- runtime component/runtime-lock/inventory mismatch fails before browser launch;
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

Repository-owned real-runtime integration is not external production certification. Physical supported Windows-host behavior, real proxy/provider coherence and specialized fingerprint-site observations remain later external evidence.

## Explicit non-goals

AR-10 performs no production provisioning or mutation, no Terraform/generic IaC work, no AR-11 release/promotion cutover, no AR-15 signing/updater/side-by-side/LKG/rollback implementation, and no claim of physical Windows, real proxy-provider or specialized fingerprint-site certification.

Throughout AR-10:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```


## Final acceptance — 2026-08-19

AR-10 is accepted. Implementation PR #371 was exact-head green at `c7f8ac9704433d3e52d3b79f985c9ac60aa068db` across 16/16 permanent workflows, merged by guarded squash to accepted `main` `7ab5edf583f541d08ff732624af25881d430d427`, and reread from that accepted main. Live `main` protection preserves the accepted AR-7 required-check baseline and additionally requires `Real Camoufox cold-launch proof` plus `Profile Bridge Windows regression` (23 required contexts total).

The accepted runtime path is the native Profile Bridge -> bounded typed/versioned IPC -> real pinned Camouhost/Camoufox persistent-context chain. Persistent browser state, generation-stable fingerprint identity, OS-level Firefox writer ownership, rollback/ambiguous-close behavior, malformed/oversized/replayed IPC rejection, zero remaining AR-10 historical direct executables, and zero `opsctl` production child-process spawn authority are permanent regression evidence.

Acceptance does **not** authorize production: `architecture_complete=false`, `production_core_gate=BLOCKED`, `production_ready=false`, `production_mutation=false`. AR-11 — Release-set / Promotion Architecture is current under issue #372; AR-12 follows only after AR-11 acceptance. Canonical machine evidence is `docs/evidence/2026-08-19-ar10-final-acceptance.json`.
