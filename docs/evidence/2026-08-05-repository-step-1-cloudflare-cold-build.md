# Repository Step 1 — Cloudflare Cold-Build Evidence

**Дата:** 2026-08-05  
**Статус:** passed for repository cold-build scope  
**Baseline:** `cc345301baaa1e549caf4045ce16739402edca02`  
**Implementation head:** `196804579bd6535b75dd964bc50fd184703b52cb`  
**Pull request:** #6  
**Permanent Quality Gate run:** `31036328555`

## 1. Проверенный Стек

- Rust `1.97.1`, edition `2024`;
- target `wasm32-unknown-unknown`;
- `worker = 0.8.5` with exact Cargo pin and `d1`, `queue` features;
- direct `wasm-bindgen = 0.2.126` for generated Durable Object exports;
- `worker-build = 0.8.5` invoked with an exact version;
- committed project `Cargo.lock`.

## 2. Что Было Собрано

`browser-profile-control-plane-worker` contains compile-time access to:

- D1 binding `CATALOG_DB`;
- R2 binding `PROFILE_OBJECTS`;
- Queue producer binding `GENERATION_VERIFICATION`;
- Durable Object namespace `PROFILE_COORDINATOR` and generated
  `ProfileCoordinator` export;
- Workers Static Assets binding `ASSETS`;
- worker-first `/api/v1/health` and `/api/v1/bindings` routes;
- fail-closed `/bridge/*` route until device protocol exists;
- SPA/static-asset fallback for other paths.

The binding names and route classification are also represented in a pure native
`control-plane-contract` crate with unit tests.

## 3. Permanent CI Result

All jobs in Quality Gate run `31036328555` succeeded:

1. `Rust Linux and WASM`
   - rustfmt;
   - Clippy for native crates with warnings denied;
   - native tests;
   - primitives WASM check;
   - machine-readable status validation;
   - current-tree high-confidence secret scan.
2. `Rust Windows`
   - native workspace tests on the Windows runner.
3. `Cloudflare Worker Release Build`
   - locked Worker check for `wasm32-unknown-unknown`;
   - exact `worker-build 0.8.5` installation;
   - `worker-build --release`;
   - Wasm optimization;
   - verified `build/worker/shim.mjs`;
   - verified at least one generated `.wasm` artifact.

The Worker compile step completed with the chosen D1/R2/Queue/Durable Object and
Static Assets API surface. The release builder selected Wasm Bindgen `0.2.126`
and produced the expected publishable artifact layout.

## 4. Defects Found And Corrected

- The first permanent run rejected non-rustfmt-compliant imports.
- The first Worker compile exposed that generated Durable Object exports require
  `wasm-bindgen` as a direct crate dependency. Exact `0.2.126` was added and the
  lockfile refreshed.
- Temporary write-enabled lock-generation workflows were removed. The accepted
  permanent Quality Gate uses `contents: read` only.

## 5. Supply-Chain Observation

Installing upstream `worker-build 0.8.5` with its published lock emitted warnings
that `time 0.3.48` and `time-macros 0.2.28` are yanked in crates.io. The tool still
built successfully. These packages belong to the build-tool installation graph,
not the Worker runtime dependency graph, but the warning remains an explicit
upgrade/supply-chain review item. It is not interpreted as proof of a known
vulnerability or silently ignored.

## 6. Что Это Доказывает

- exact Rust baseline can compile the selected current `workers-rs` API;
- D1, R2, Queue, Durable Object and Static Assets binding types coexist in one
  Rust Worker crate;
- Durable Object exports are generated successfully;
- release packaging creates the expected JavaScript shim and optimized Wasm;
- the repository can enforce this build permanently without cloud credentials;
- native domain/contract tests remain independent of the Worker runtime.

## 7. Что Не Доказано

- deployment to a real Cloudflare account;
- validity of real binding IDs, Access policy or domain routing;
- D1 migration and transaction semantics;
- Durable Object storage, eviction, alarm or fencing behavior;
- Queue delivery/redelivery and R2 object operations;
- Static Assets SPA behavior in an actual runtime;
- remote rollback, cost limits or account recovery;
- any production security, key management, profile lifecycle or multi-device
  property.

No Cloudflare credential, real account resource, user profile or personal data
was used in this evidence.
