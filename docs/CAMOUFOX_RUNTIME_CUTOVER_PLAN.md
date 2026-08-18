# Camoufox Runtime Cutover Plan

**Document status:** TARGET / SUBORDINATE_DESIGN_INPUT  
**Tracking issue:** #359  
**Parent program:** #266  
**Canonical execution authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Current implementation:** AR-8D under #308  
**Owning future slice:** AR-10 — Runtime and Historical Executable Simplification  
**Windows delivery owner:** AR-15  
**Production ready:** `false`

## 1. Authority and purpose

This document clarifies an existing AR-10 obligation. It does not create a new roadmap, does not make AR-10 current, and does not supersede the Architecture Re-baseline v3 sequence.

The clarification is required because the repository already contains strong browser-profile lifecycle primitives and a real Camoufox research launcher, while the supported Camouhost executable remains synthetic. Without an explicit cutover contract, the phrase `Real Camoufox = External` can incorrectly hide a repository-owned implementation gap and allow AR-15 Windows delivery work to begin before there is an accepted real runtime to deliver.

The correct ownership split is:

```text
AR-8   secrets / keys / OAuth credential lifecycle
AR-9   D1 evolution / schema compatibility
AR-10  supported real Camouhost/Camoufox runtime cutover
AR-11  release-set / promotion identity for the accepted components
AR-12  fresh rehearsal environment
AR-13  rotation rehearsal
AR-14  remote recovery rehearsal
AR-15  Windows signing / updater / activation / LKG / rollback
AR-16  final whole-project audit
AR-17  architecture closeout / Production Core authorization gate
PC-1   first production rollout after authorization
```

Physical-host, specialized-site and production Camoufox evidence remains External and cannot be manufactured by repository-local tests.

## 2. What the project already planned

The historic profile plan and accepted ADRs already establish the intended architecture rather than a direct Python browser launcher.

### 2.1 Desktop runtime topology

`docs/adr/ADR-0003-desktop-runtime-distribution.md` accepts:

```text
React web UI
-> Cloudflare control plane
-> one-time launch intent
-> native Rust Profile Bridge / supervisor
-> managed IPC to embedded Python Camouhost
-> separate visible Camoufox window
```

The runtime bundle is intended to contain exact locked Python, Camoufox, BrowserForge, Playwright, browser binary, fonts/addons/data, contract identity, hashes, SBOM/licensing and compatibility metadata. Browser UI is not embedded into a WebView.

### 2.2 Cloud-backed profile lifecycle

`docs/adr/ADR-0002-cloud-profile-materialization.md` accepts immutable encrypted cloud generations with local materialization. The browser never treats R2 as a live filesystem. The lifecycle is:

```text
immutable encrypted generation
-> local staging
-> decrypt + exact verify
-> materialize
-> acquire governed writer ownership
-> run browser locally
-> graceful close
-> create next immutable generation
-> encrypt + upload + exact verify
-> fenced/CAS activation
```

### 2.3 Fingerprint identity was intentionally separate from profile files

`docs/adr/ADR-0001-fingerprint-stability-policy.md` is still `proposed`, but its target model is important and remains the design input for AR-10:

- `Profile-Stable` signals are generation-stable;
- `Origin-Deterministic` signals derive from profile identity/entropy rather than being regenerated arbitrarily;
- `Network-Bound` signals change with a coherent network/proxy generation;
- `Session-Dynamic` signals are not required to be byte-identical across launches;
- runtime/fingerprint-policy upgrades create a candidate generation and require certification rather than mutating the active identity silently.

AR-10 must either accept this policy with executable evidence or replace it through an explicit ADR decision. It must not silently treat the proposed ADR as already accepted.

## 3. Current implementation reality

### 3.1 Strong accepted foundation

The current Rust implementation already provides substantial repository-local safety:

- `BrowserIdentityManifest` separates runtime identity and fingerprint-config identity from browser profile files;
- materialization binds tenant/profile/generation, source container identity, local inventory and browser identity;
- preflight checks selected runtime identity before launch;
- coordinator/device/epoch/fencing and one-writer ownership are enforced;
- browser lock files are not blindly deleted;
- dirty local state becomes a new immutable encrypted generation;
- upload is exact-verified before authoritative metadata commit;
- crash/ambiguous close paths remain recovery-required rather than being relabeled clean.

These controls are reused by AR-10. The real runtime must not create a parallel profile lifecycle.

### 3.2 Supported Camouhost is still synthetic

`runtime/camouhost/main.py` explicitly identifies itself as a deterministic fake process for repository contract evidence. It implements typed IPC and synthetic profile state but does not import or launch Camoufox.

`apps/profile-bridge/src/bin/profile-bridge-synthetic.rs` similarly provides the repository-local synthetic composition path.

Therefore repository-local lifecycle composition is strong, but a supported real Camoufox outer-runtime adapter is still Target work.

### 3.3 A useful real-runtime prototype already exists

`tools/profile_browser.py` launches real Camoufox and already demonstrates the most important persistence experiment:

1. generate/materialize Camoufox configuration once;
2. store the full configuration with a canonical SHA-256 digest;
3. reuse that exact configuration for later launches;
4. use a stable persistent `user_data_dir` for cookies/storage/browser profile state;
5. probe representative browser identity values and compare a digest between launches.

This direct tool is research evidence, not accepted lifecycle authority.

The accepted AR-6 machine authority `architecture/python-estate-ar6.json` already makes the future disposition explicit:

- `tools/profile_browser.py` = `DELETE_AFTER_SEQUENCE`;
- role = `legacy_direct_browser_runtime_tool`;
- target = `apps/profile-bridge + accepted Camoufox runtime boundary`;
- cutover slice = `AR-10`;
- retirement requires browser/profile execution parity through Profile Bridge.

AR-10 is therefore the existing owner of the real-runtime cutover; this document makes that accepted machine intent developer-visible.

## 4. 2026 Camoufox technology findings

Research performed on 2026-08-18 against Camoufox's official documentation and upstream repository changes gives several constraints that AR-10 must encode.

### 4.1 Persistent browser storage is not persistent generated identity

Official Camoufox usage supports a persistent context with a stable `user_data_dir`. This is the correct mechanism for cookies, localStorage, cache and Firefox profile state.

Camoufox's BrowserForge integration also states that, by default, it generates a random fingerprint under the selected constraints. Upstream issue #442 specifically reports the resulting mismatch for long-lived profiles: browser storage can persist while generated hardware/fingerprint values rotate after restart.

Project rule:

```text
persistent browser state != persistent browser identity
```

Both must be governed explicitly.

### 4.2 Exact project identity must not depend on random defaults

For an existing profile generation, a normal launch must never allow unspecified Camoufox/BrowserForge defaults to create a new identity silently.

AR-10 must define one canonical generation identity input and a deterministic materialization rule. The result is bound to the existing `BrowserIdentityManifest`/materialization evidence by cryptographic digest and runtime/policy version.

### 4.3 Upstream `fingerprint_seed` is not project authority

Upstream PR #606 proposes a `fingerprint_seed` input to reproduce generated values. As of the research date it remains open. Its own scope explicitly separates generated fingerprint values from persistent browser state and from TLS/network/renderer behavior.

If a later pinned Camoufox release accepts and stabilizes such a feature, AR-10 may use it as an implementation detail. The project contract remains deterministic generation identity; it must not depend on one unmerged upstream option or silently change semantics when upstream implementation changes.

### 4.4 Durable profiles require an in-process persistent-context path

Current upstream work around `launch_server` explicitly rejects `persistent_context` / `user_data_dir`: Playwright's browser-server model launches a browser rather than `launchPersistentContext`, so accepting those options would silently discard the intended durable profile.

Therefore the supported Camouhost for durable profiles must use the in-process persistent-context API unless a future pinned upstream release provides a separately proven equivalent. A remote/server launch path may not be used merely because it is operationally convenient.

### 4.5 Pin exact Camoufox/runtime bits

Camoufox's official site currently warns that the new 2026 releases are experimental and may contain breaking changes. Upstream Windows support has also moved through roadmap uncertainty, missing/failing builds and later renewed Windows artifacts/fixes.

The project must therefore treat `latest Camoufox` as invalid release policy. AR-10/AR-11/AR-15 consume one exact accepted runtime bundle with explicit Camoufox/browser/BrowserForge/Playwright versions and content identities.

## 5. AR-10 implementation contract

AR-10 must deliver the following as bounded implementation work after it becomes current.

### 5.1 Supported real Camouhost

Implement one supported Camouhost outer adapter that actually launches the pinned Camoufox runtime while preserving the existing versioned Bridge IPC boundary.

The fake Camouhost may remain for deterministic contract testing but must remain explicitly synthetic. It is never production runtime authority.

### 5.2 Generation-scoped identity persistence

For every active browser profile generation:

- durable browser files use the exact materialized generation and stable `user_data_dir`;
- canonical fingerprint identity input/configuration is created once and retained for that generation;
- all later allowed launches reproduce the same generation-stable identity;
- fingerprint identity metadata carries policy/runtime/config identity and a cryptographic digest;
- raw entropy/key material is never placed in logs, audit, normal support output or public generation metadata;
- an unexplained identity drift blocks launch or marks the candidate/recovery state fail-closed.

### 5.3 Upgrade and migration semantics

Runtime version, browser version, fingerprint policy/schema, BrowserForge semantics or other compatibility-relevant changes may not rewrite an existing generation in place.

The allowed path is:

```text
active generation
-> clone/candidate generation
-> apply pinned candidate runtime/policy
-> materialize deterministic identity
-> consistency/certification checks
-> exact encrypted publication
-> governed activation or rejection
```

### 5.4 Existing Bridge safety controls remain authoritative

Real Camoufox launch happens only after:

- authorization/device requirements;
- exact active generation/freshness;
- coordinator lease/epoch/fencing;
- approved runtime bundle verification;
- local writer ownership;
- materialization/browser-identity validation;
- network identity preflight.

Clean close flows through the existing dirty-generation publication/commit path. Crash, force termination, runtime mismatch, identity mismatch and uncertain browser locks remain explicit recovery cases.

### 5.5 Research launcher retirement

`tools/profile_browser.py` may be deleted only after the supported Camouhost path proves parity for the useful behavior it currently demonstrates.

Required parity includes at least:

- real pinned Camoufox startup;
- persistent context/profile state;
- deterministic generation identity reuse;
- identity probe/certification hooks;
- graceful close and crash distinction;
- integration with Bridge ownership/preflight/generation commit rather than direct filesystem lifecycle authority.

## 6. AR-10 acceptance evidence

AR-10 real-runtime cutover is not complete from compilation alone. Repository-owned acceptance must cover at least:

- same generation, repeated cold launches -> accepted profile-stable identity remains stable;
- persistent cookie/localStorage marker survives clean relaunch;
- second profile generation does not reuse another profile's identity/state;
- omitted/random/default identity input for an existing generation fails closed;
- runtime/config digest mismatch fails before browser launch;
- incompatible runtime/policy requires candidate-generation migration;
- browser lock/writer contention still fails closed without blind lock deletion;
- clean close follows immutable dirty-generation commit ordering;
- crash/forced/ambiguous close becomes recovery-required;
- no direct `tools/profile_browser.py` authority remains after cutover;
- fake and real Camouhost roles are mechanically distinguishable;
- exact pinned runtime identity is recorded in release-compatible metadata;
- no secret/entropy/profile payload leaks into normal logs/audit/support evidence.

Real specialized-site, proxy/provider and physical-host certification remains External and is deliberately not claimed here.

## 7. AR-15 boundary

AR-15 consumes the accepted AR-10 runtime. It owns delivery, not first runtime semantics.

AR-15 must bind the accepted runtime identity into its Windows release/update contract and prove:

- immutable signed Windows artifact + manifest identity;
- Bridge/updater/runtime/profile-format compatibility;
- side-by-side verified staging;
- activation only at a proven quiescent boundary with no active Camoufox/profile operation;
- candidate health before LKG promotion;
- rollback to LKG and loop prevention;
- production-equivalent Windows rehearsal against the **real accepted** Profile Bridge/Camoufox boundary.

If AR-10 remains incomplete, AR-15 cannot truthfully claim a production-grade Camoufox delivery chain.

## 8. External evidence boundary

After repository-owned real-runtime integration exists, later real-world evidence still has to prove what CI cannot:

- supported physical Windows host behavior;
- real Camoufox/browser process lifecycle;
- fingerprint stability on real machines;
- specialized-site observations;
- real network/proxy coherence;
- cross-device behavior;
- trusted Windows signing/update verification;
- remote R2/key/recovery behavior where required;
- rollout/rollback evidence.

`real runtime integration` and `real production certification` are separate statements and must remain separately represented in the capability/evidence matrix.

## 9. Source references for the research decision

Project authorities/evidence:

- `docs/adr/ADR-0001-fingerprint-stability-policy.md`;
- `docs/adr/ADR-0002-cloud-profile-materialization.md`;
- `docs/adr/ADR-0003-desktop-runtime-distribution.md`;
- `architecture/python-estate-ar6.json`;
- `runtime/camouhost/main.py`;
- `tools/profile_browser.py`;
- `apps/profile-bridge/src/browser_execution.rs`;
- `apps/profile-bridge/src/browser_preflight.rs`;
- `apps/profile-bridge/src/dirty_generation*.rs`.

Upstream research snapshot, 2026-08-18:

- Camoufox Usage — persistent context / `user_data_dir`;
- Camoufox BrowserForge Integration — random-by-default generated fingerprint and explicit fingerprint injection;
- Camoufox Fingerprint Injection — explicit config with auto-population of unspecified values;
- daijro/camoufox issue #442 — persistent storage vs rotating generated fingerprint report;
- daijro/camoufox PR #606 — open persistent fingerprint seed proposal;
- daijro/camoufox PR #707 — persistent server-path rejection and current runtime/build fixes;
- Camoufox current site/release notes — experimental 2026 line and exact-version/Windows risk.

Upstream findings are research inputs, not mutable application authority. Exact implementation must be revalidated when AR-10 begins because upstream behavior can change.

## 10. Non-goals and sequencing guard

This clarification does not authorize AR-10 implementation now. AR-8D remains current, then AR-8 must fully close before AR-9. AR-10 begins only after accepted AR-9 under the canonical sequence.

It does not authorize production mutation, does not accept ADR-0001 prematurely, does not introduce a new runtime lifecycle beside Profile Bridge, and does not reduce AR-15 or External evidence requirements.
