# Test And Evidence Index

**Статус:** normative evidence registry  
**Дата:** 2026-08-14

A claim is accepted only within the scope of its referenced evidence. Passing one
smoke test does not promote unrelated production gates.

## 1. Existing Evidence

| Evidence | Status | Proves | Does not prove |
|---|---|---|---|
| `RESEARCH_FINDINGS.md` | verified baseline | corpus inventory, runtime observations, known prototype defects | production implementation or unchanged historical source tree |
| `CLOUD_PROFILE_SMOKE_TEST.md` | passed with limitations | synthetic one-device create/close/encrypt/R2/restore/replay and bounded fingerprint replay | multi-device, production keys, fencing, disaster recovery, full fingerprint certification |
| ADR-0002 evidence | accepted partial | locally executed immutable cloud generation model is viable | arbitrary runtime/device portability |
| Repository Step 0 / PR #4 | accepted | exact Rust `1.97.1` locked workspace; Linux fmt/Clippy/tests; Windows tests; primitives WASM compile; status validation; current-tree high-confidence secret scan | Cloudflare SDK compatibility, Windows Bridge, historical secret remediation or production security |
| Repository Step 1 / PR #6 | accepted | exact `worker 0.8.5` D1/R2/Queue/Durable Object/Static Assets compile and `worker-build 0.8.5` release artifact | real Cloudflare deployment, binding behavior, migrations, DO consistency, remote rollback or production readiness |
| Repository Step 2 / PR #9 | accepted | typed pure domain/application boundaries, native+WASM state-machine tests, active architecture and immutable v1 contract compatibility gates | D1 persistence, Access, distributed fencing, Bridge/runtime or production completeness |
| Repository Step 3 / PR #12 | accepted | strict D1 schema, migration replay, tenant constraints, typed adapter boundary, optimistic CAS and synthetic transaction envelope | remote D1, Access identity, full API ACL slice, backup/restore or production readiness |
| Repository Step 4 / PR #15 | accepted | Access JWT and fake identity convergence, active membership resolution, owner bootstrap/transfer, membership lifecycle, explicit client/profile ACL, neutral concealment, governed atomic commands and authenticated Worker packaging | remote Access policy, remote D1, Profile Coordinator, Bridge/runtime, multi-device or production readiness |
| Repository Step 5 / PR #18 | accepted | per-profile Durable Object coordinator, monotonic epoch/fencing, stale-writer and timeout uncertainty rules, authenticated explicit profile ACL, replayable storage, repairable D1 projection/outbox and release Worker packaging | remote Durable Object behavior, physical multi-device runtime, Windows Bridge, encrypted generations or production readiness |
| Repository Step 6 / PR #21 | accepted | redacted single-use custom URI enrollment, device-bound claim, local writer epochs, process close/crash/timeout rules, versioned fake Camouhost IPC, SQLite outbox and verified Windows release executable | registry installation, production device keys, real process/job behavior, real Camouhost/Camoufox, trusted signing, multi-device or production readiness |
| Repository Step 7 / PR #24 | accepted | deterministic content-addressed synthetic runtime bundle, safe manifest/path/extraction validation, Bridge approval before spawn, fake Camouhost exact-session lifecycle and active/clean evidence | real Camoufox, redistribution rights, production Python resolution, real legacy profiles, trusted signing, multi-device or production readiness |
| Repository Step 8 / PR #27 | accepted | marked opaque local materialization, atomic Bridge lock-file protocol, deterministic inventory, clone-only recovery, explicit dirty/recovery states, safe quota and metadata-only support evidence | kernel advisory locks, real browser/database recovery, real legacy profiles, encrypted cloud generations, trusted signing, multi-device or production readiness |
| Repository Step 9 / PR #30 | accepted bounded synthetic evidence | exact-pinned XChaCha20-Poly1305/SHA-256 container, authenticated chunk/final records, immutable lifecycle, strict pointer CAS/rollback/quarantine/orphan planning, DEK-bound nonce reuse, zeroizing plaintext boundaries and Linux/Windows/WASM gates | production entropy/key wrapping, remote R2/D1 atomicity, escrow/account-loss restore, independent cryptographic review, physical multi-device or production readiness |
| Repository Step 10 / PR #33 | accepted bounded synthetic evidence | required/optional/prohibited certification policy, deterministic matrix/vector, privacy-safe support output, typed device grant journal, exact preverified update/health identity, rollback/fail-closed state and Linux/Windows/WASM gates | real fingerprint certification, physical second device, production unwrap, cryptographic signature verification, trusted signing or production readiness |
| Mailbox composition / PR #60 + repair PR #62 | accepted repository-local composed/synthetic evidence | D1 mailbox binding/job persistence, secret-handle-only request DTOs, idempotency/audit/outbox, Worker API routes, metadata-only synthetic provider path, adapter/native/WASM/release compilation | real Gmail/IMAP/browser execution, mailbox contents, production scheduler/provider evidence or production readiness |
| React operator UI / PR #64 | accepted repository-local composed/synthetic evidence | exact head `f2fcfe19335508a6062b9ed8a7c984fe2f97a417`; Node 24.19.0/npm 11.17.0 locked React/Vite/TypeScript workspace; same-origin API/problem boundary; neutral disclosure; high-impact confirmation; 11/11 permanent workflows and Frontend Gate `31207429792`; squash merge `c1e7896590661ab01cb5c9b32b72b4a7cfa4a38b` | deployed Cloudflare Access UI, real Bridge/custom-URI onboarding, real providers, missing backend list APIs or production readiness |
| Cross-component standalone acceptance / PR #66 | accepted repository-local composed/synthetic evidence | exact head `31f358c92c2e09c752155af208b7d2aaf73d472a`; all 12 permanent workflows green; Cross-Component Gate `31208960718`; deterministic six-phase metadata-only manifest, D1/ACL/mailbox/generation invariants, adapter + Worker native/WASM, actual synthetic Bridge CLI to `DIRTY_LOCAL`, Node24 frontend install/typecheck/tests/build; squash merge `eb02f3e81022193fb459b7c46d14afcb19c8900f` | real Cloudflare/Camoufox/mailbox providers, production keys/signing, physical multi-device or production readiness |
| Pre-2J D3A empty-D1 bootstrap / PR #254 | accepted bounded remote evidence | exact deterministic `0001`–`0026` SQL-file bootstrap imported into one fresh remote D1; exact ledger and 0012 schema; replay rejected with unchanged schema digest and ledger; permanent digest/ledger/negative CI binding | incremental migration of non-empty D1, canonical staging/production deployment, rollback/fail-forward policy, D3 promotion or production readiness |

### Repository Step 0 Evidence

- baseline: `3fef715c5b74f723d8a30c16471bf62a3609a34b`;
- accepted source head: `c8927bc79ab7f68123fc122409792326043e29b3`;
- permanent Quality Gate run: `31035179366`, conclusion `success`;
- squash merge: `dc2dc2e1a7acd07d89550328309833988bb05a2e`;
- jobs: `Rust Linux and WASM`, `Rust Windows`;
- user/legacy profile data involved: no.

The tracked-file scan covers the accepted tree only. It does not prove that the
known legacy credential was rotated or absent from repository history; that
external remediation remains issue #1.

### Repository Step 1 Evidence

- baseline: `cc345301baaa1e549caf4045ce16739402edca02`;
- technical implementation head: `196804579bd6535b75dd964bc50fd184703b52cb`;
- accepted source head: `990fe8262f933b1a20a0c786a6a5ebc26f4fe7e2`;
- technical Quality Gate run: `31036328555`, conclusion `success`;
- final Quality Gate run: `31036967681`, conclusion `success`;
- squash merge: `cba724a0d7fd116859a30d9e0101e56349c1358c`;
- jobs: `Rust Linux and WASM`, `Rust Windows`, `Cloudflare Worker Release Build`;
- exact runtime/build pins: Rust `1.97.1`, `worker 0.8.5`,
  `wasm-bindgen 0.2.126`, `worker-build 0.8.5`;
- output checks: `build/worker/shim.mjs` and generated Wasm present;
- detailed report:
  [`evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md`](evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md);
- Cloudflare credentials or real resources involved: no;
- user/legacy profile data involved: no.

The upstream `worker-build 0.8.5` installation emitted yanked-package warnings
for two build-tool transitive versions. This remains an upgrade and supply-chain
review item; it is not a Worker runtime dependency claim.

### Repository Step 2 Evidence

- baseline: `29956f6a71ea5f76618e97c651276f2a43698870`;
- technical evidence head: `a3d0852e11708297bb7d5e04ed23ff981e774d7c`;
- accepted source head: `70fdbe8b494f61aeda639e004e54ea088e4ddc3e`;
- technical Quality Gate run: `31039199212`, conclusion `success`;
- final Quality Gate run: `31039802642`, conclusion `success`;
- squash merge: `14e96a7841c3652767f37ee76151c3cf39be6301`;
- jobs: `Rust Linux and WASM`, `Rust Windows`, `Cloudflare Worker Release Build`;
- positive architecture and contract checks: passed;
- deliberately forbidden domain dependency fixture: rejected as required;
- deliberately breaking protobuf fixture: rejected as required;
- accepted v1 contract baseline immutability gate: passed;
- all governed pure crates: native tests and `wasm32-unknown-unknown` check passed;
- detailed report:
  [`evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md`](evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md);
- Cloudflare credentials, storage resources or user data involved: no.

### Repository Step 3 Evidence

- baseline: `313a94aa59d10fa6a2d3e9a6da80bd9315e33fc4`;
- technical evidence head: `40d84c5cf5d7832a3db964ab639e822f2e055031`;
- accepted source head: `bff2109448d8109b963e9bd2077da273e54e8da2`;
- technical Quality Gate run: `31043260598`, conclusion `success`;
- final Quality Gate run: `31043753595`, conclusion `success`;
- squash merge: `189f36cdce092a05bccf5757e368eee87a0e2c50`;
- jobs: `Rust Linux and WASM`, `D1 Catalog Migrations`, `Rust Windows`,
  `Cloudflare Worker Release Build`;
- Wrangler `4.94.0` applies `0001_catalog.sql` from empty state and replay is a
  no-op;
- deterministic SQLite schema, foreign-key/integrity, tenant constraints,
  optimistic CAS and rollback/commit envelope: passed;
- real typed D1 adapter boundary: passed;
- deliberately raw-D1 use-case fixture: rejected as required;
- Worker with the D1 adapter in its dependency graph: checked and release packaged;
- detailed report:
  [`evidence/2026-08-05-repository-step-3-d1-catalog-foundation.md`](evidence/2026-08-05-repository-step-3-d1-catalog-foundation.md);
- Cloudflare credentials, remote resources or user data involved: no.

### Repository Step 4 Evidence

- baseline: `5667779d59413d4736e58d6eb83a892dfdd2f522`;
- technical evidence head: `5b187ebd786cdca068ed209b79642ecaaebe3be6`;
- accepted source head: `1174a0720bc1c44fbb0c8e22b5c0cbac5f0810ad`;
- technical Quality Gate run: `31052479944`, conclusion `success`;
- final exact-head Quality Gate run: `31052742660`, conclusion `success`;
- squash merge: `bd3db24ffc62d50654e385e587cab3e6a01b928c`;
- jobs: `Rust Linux and WASM`, `D1 Catalog Migrations`, `Rust Windows`,
  `Cloudflare Worker Release Build`;
- Access RS256/JWK/WebCrypto and deterministic fake identity adapters converge on
  the same verified external identity contract;
- active tenant membership resolves to `ActorContext`; missing, suspended and
  revoked membership is denied for covered flows;
- empty-boundary owner bootstrap, atomic owner transfer and last-active-owner
  protection: passed;
- invitation, membership lifecycle, client/profile metadata, historical
  assignment and explicit grant/revoke flows: passed within the repository-local
  synthetic boundary;
- stale optimistic preconditions and downstream envelope failures abort full
  governed command transactions;
- foreign, missing and unauthorized client/profile reads use the same neutral
  disclosure result;
- deliberate assignment-as-authorization fixture: rejected as required;
- raw D1, superseded unguarded writes, temporary workflows and tracked Rust build
  output: rejected by permanent gates;
- authenticated Worker with the Step 4 adapters: checked and release packaged;
- detailed report:
  [`evidence/2026-08-06-repository-step-4-identity-clients-acl.md`](evidence/2026-08-06-repository-step-4-identity-clients-acl.md);
- Cloudflare credentials, remote resources or user data involved: no.

### Repository Step 5 Evidence

- baseline: `bd292093778c954f2126c2165fd65c78cbe37f65`;
- accepted source head: `e338186e53f02784d1d685ae3cd761f3cef34ef7`;
- exact-head Quality Gate run: `31056722531`, conclusion `success`;
- squash merge: `78931f529152c209ebececbcbef1aca770b7e3e0`;
- jobs: `Rust Linux and WASM`, `D1 Catalog Migrations`, `Rust Windows`,
  `Cloudflare Worker Release Build`;
- deterministic opaque-profile Durable Object naming and replayable typed journal:
  passed;
- monotonic epoch, server-generated fencing token and delayed stale-writer
  rejection after turnover: passed;
- duplicate, reordered, stale-version and conflicting-idempotency commands:
  rejected as required;
- idle, hard and drain TTL uncertainty, including late nominally clean release:
  passed;
- active actor and explicit profile ACL are resolved before Durable Object access;
- deliberate assignment-derived coordinator authorization fixture: rejected as
  required;
- migration `0004` applies and replays; append-only projection commands,
  materialized latest projection and outbox evidence are verified;
- stale projection, same-sequence conflict and repair-from-authoritative-snapshot
  tests: passed;
- real Worker dependency graph checks for WASM and packages a verified release
  shim and Wasm artifact;
- detailed report:
  [`evidence/2026-08-06-repository-step-5-profile-coordinator.md`](evidence/2026-08-06-repository-step-5-profile-coordinator.md);
- Cloudflare credentials, remote resources, physical devices or user data
  involved: no.

### Repository Step 6 Evidence

- baseline: `aac9f994cd79b5d6534f6ae9ec1669cdfeb8b73c`;
- accepted source head: `cceb7e97da980c905739eb02366019015f247d6e`;
- exact-head Quality Gate run: `31058767330`, conclusion `success`;
- squash merge: `d0e2e0b1d11eb689b57f8ebaaefd591a6a7b6bab`;
- jobs: `Rust Linux and WASM`, `D1 Catalog Migrations`,
  `Rust Windows And Profile Bridge Artifact`, `Cloudflare Worker Release Build`;
- exact custom URI parsing, token redaction, strict expiry, single use and device
  binding: passed;
- one-writer workspace epoch and stale release rejection: passed;
- graceful close, unexpected crash and forced timeout remain distinct: passed;
- versioned fake Camouhost negotiation/launch/ready/close and malformed-message
  rejection: passed;
- local SQLite exact replay, conflicting idempotency, stale version, reordered
  sequence, append-only commands and immutable outbox payload tests: passed;
- deliberate known browser-lock deletion fixture: rejected as required;
- Windows-only safe adapter test: passed;
- pinned Windows runner produced and verified a non-empty release
  `profile-bridge.exe`;
- all Step 0–5 D1, Worker, ACL, coordinator and contract regression gates remained
  green;
- detailed report:
  [`evidence/2026-08-06-repository-step-6-windows-bridge-feasibility.md`](evidence/2026-08-06-repository-step-6-windows-bridge-feasibility.md);
- production credentials, remote resources, real browser runtime, legacy profile
  data or physical multi-device evidence involved: no.

### Repository Step 7 Evidence

- baseline: `6f76109d48272109ea305c6f8690cc4c6540542f`;
- accepted source head: `936d3c9529b897daac2ea5d13ba01f7babf07b8a`;
- exact-head Quality Gate run: `31060683502`, conclusion `success`;
- exact-head Runtime Bundle Gate run: `31060683898`, conclusion `success`;
- squash merge: `9d01ccb34598a8aeb9406570b623582d710c88e7`;
- deterministic canonical manifest, inventory and byte-identical bundle rebuild:
  passed;
- SHA-256 source/payload tamper detection: passed;
- absolute, drive, traversal, reserved, symlink, duplicate and case-colliding
  paths: rejected as required;
- extraction requires a marked empty synthetic destination and remains contained;
- typed Bridge bundle approval is required before spawn;
- failed IPC negotiation invokes forced-termination rollback;
- fake Camouhost IPC v1 exact-session hello/launch/ready/close: passed;
- synthetic active evidence appears on launch and clean evidence only after exact
  graceful close; mismatch and premature EOF remain active;
- deliberate legacy-corpus reference fixture: rejected as required;
- Linux and Windows dedicated runtime jobs passed; Profile Bridge executable and
  Cloudflare Worker release artifacts remained verified;
- detailed report:
  [`evidence/2026-08-06-repository-step-7-camouhost-runtime-bundle.md`](evidence/2026-08-06-repository-step-7-camouhost-runtime-bundle.md);
- production credentials, network dependency resolution, real Camoufox, real
  legacy profiles or physical multi-device evidence involved: no.

### Repository Step 8 Evidence

- baseline: `ef8777b69ff6c89c176b79b04adecce17bc6c68e`;
- accepted source head: `dbf3770f58c45b9f247579191b2b2d5f342c1bc8`;
- exact-head Quality Gate run: `31068856595`, conclusion `success`;
- exact-head Local Profile Gate run: `31068856619`, conclusion `success`;
- exact-head Runtime Bundle regression run: `31068856601`, conclusion `success`;
- squash merge: `eb55f67d742661019438891764c388dc19f62d96`;
- marked absolute root and typed opaque tenant/profile/generation paths: passed;
- symlinked roots, components, control markers and inventory entries: rejected as
  required;
- atomic Bridge lock acquisition, second-writer rejection and ownership-tamper
  failure without deletion: passed;
- browser-owned `.parentlock`, `parent.lock` and `lock` files are preserved;
- deliberate browser-lock deletion fixture: rejected as required;
- deterministic regular-file inventory and clone-only source/clone integrity:
  passed;
- graceful close preserves `DIRTY_LOCAL`; crash preserves `RECOVERY_REQUIRED`;
- forgotten-window no-action/warn/drain/force-close progression: passed;
- dirty, active, recovery-required and locked synced generations are excluded from
  quota candidates;
- support summary contains aggregate metadata only;
- Linux and Windows dedicated lifecycle jobs passed; Profile Bridge executable,
  Runtime Bundle and Cloudflare Worker release regressions remained green;
- detailed report:
  [`evidence/2026-08-06-repository-step-8-local-profile-lifecycle.md`](evidence/2026-08-06-repository-step-8-local-profile-lifecycle.md);
- production credentials, remote resources, real browser runtime, legacy profile
  data or physical multi-device evidence involved: no.

### Repository Step 9 Evidence

- baseline: `e596fbe5692aa5b020700e7462c608dd23bacc15`;
- accepted source head: `73685241a6d70cf6d8ec80210d94b66cf37b1b45`;
- exact-head Quality Gate run: `31072625808`, conclusion `success`;
- exact-head Encrypted Generation Gate run: `31072625852`, conclusion `success`;
- exact-head Local Profile regression run: `31072625849`, conclusion `success`;
- exact-head Runtime Bundle regression run: `31072625892`, conclusion `success`;
- squash merge: `bc5286e3fea767acf955fb2622dab6221ecf1c3b`;
- exact RustCrypto XChaCha20-Poly1305, SHA-256 and zeroization pins: passed;
- canonical authenticated metadata, ordered chunks and mandatory final record: passed;
- deterministic SHA-256 container regression vector: passed;
- metadata/chunk/final tamper, truncation, reorder and identity mismatch: rejected;
- invalid magic/version, oversized metadata and trailing bytes: rejected;
- same DEK and nonce prefix across different key IDs: rejected as nonce reuse;
- DEK and nonce-domain memory is non-printable and zeroized on drop;
- plaintext-bearing results are non-`Debug` and use `Zeroizing` buffers;
- restore grows plaintext only after authenticated records;
- immutable conflict, stale pointer, invalid rollback and corrupt promotion: rejected;
- wrong-key restore cannot quarantine an unchanged digest-matching object;
- orphan planning protects current and rollback generations;
- deliberate sensitive-output fixture: rejected as required;
- Linux, Windows and Workers WASM dedicated jobs passed; Profile Bridge, Runtime
  Bundle and Cloudflare Worker release regressions remained green;
- detailed report:
  [`evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md`](evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md);
- production credentials, remote resources, real profiles, production keys or
  physical multi-device evidence involved: no;
- ADR-0006 and production readiness remain unaccepted.

### Repository Step 10 Evidence

- baseline: `71296404dd5ffb78faf9033cbbb6b6fa395f72cd`;
- accepted source head: `7d5ba8c2a00bac256a9365a40dee7e3c28ef5b56`;
- exact-head Quality Gate run: `31074745842`, conclusion `success`;
- exact-head Certification Gate run: `31074745854`, conclusion `success`;
- Encrypted Generation regression run: `31074745859`, conclusion `success`;
- Local Profile regression run: `31074745880`, conclusion `success`;
- Runtime Bundle regression run: `31074745848`, conclusion `success`;
- squash merge: `3ddde2f48ddf82decf66c933ae5326a4455263e5`;
- policy requires at least one required signal and rejects duplicate/unknown/
  prohibited misuse;
- stable, drifted, incomplete and prohibited outcomes are distinct;
- canonical matrix is input-order independent and matches the committed SHA-256
  regression vector;
- raw observations are non-`Debug`; support output excludes raw values and matrix
  digest;
- two typed synthetic devices have independent grant versions and revoke behavior;
- successful grant/revoke/regrant operations retain immutable event history;
- preverified evidence and health results bind exact release ID/version/digest;
- failed update restores the previous release; first-install failure enters explicit
  `FAILED` state with no active release;
- deliberate raw-signal-output fixture: rejected as required;
- Linux, Windows and Workers WASM jobs passed; Step 7–9 and Windows/Cloudflare
  release regressions remained green;
- detailed report:
  [`evidence/2026-08-06-repository-step-10-certification-multi-device.md`](evidence/2026-08-06-repository-step-10-certification-multi-device.md);
- real browser signals, production keys, physical devices, trusted signatures or
  user data involved: no;
- ADR-0001/ADR-0006 and production readiness remain unaccepted.

### Post-roadmap Mailbox Composition Evidence

- accepted composition: issue #56 / PR #60, followed by forward-only repair issue #61 / PR #62;
- repair accepted source head: `80d5da1239595c2562752307d2e48a7b14a0ba3e`;
- squash merge establishing repaired composed state: `497953bf67af1c40fd35da465106f93b0a68685d`;
- exact-head Quality Gate run: `31204097542`, conclusion `success`;
- Repository Quality Audit Gate run: `31204098505`, conclusion `success`;
- Profile Generation regression run: `31204096927`, conclusion `success`;
- adapter mailbox suite was compiled/executed and Worker native/WASM/release composition passed;
- real mailbox providers, mailbox payload processing and production scheduling were not involved.

### React Operator UI Accepted Evidence

- tracking: issue #63 / PR #64, closed completed;
- accepted source head: `f2fcfe19335508a6062b9ed8a7c984fe2f97a417`;
- squash merge: `c1e7896590661ab01cb5c9b32b72b4a7cfa4a38b`;
- exact-head Frontend Gate run `31207429792`, conclusion `success`;
- all 11 then-permanent workflows were green on the same accepted head;
- runtime pins: Node `24.19.0`, npm `11.17.0`; lockfile is authoritative and clean install uses `npm ci`;
- frontend lane passed source credential-persistence scan, strict TypeScript, 7 unit tests, Vite production build and Static Assets output verification;
- no Cloudflare production deployment, real device onboarding, real provider execution or user data was involved.

### Cross-Component Standalone Candidate Evidence

- tracking: issue #65 / draft PR #66, parent epic #43;
- baseline: accepted React UI merge `c1e7896590661ab01cb5c9b32b72b4a7cfa4a38b`;
- candidate Cross-Component Acceptance Gate run `31208530252`, conclusion `success`;
- deterministic validator reported six repository-local synthetic phases and `productionReady=false`;
- governed D1 identity/client/profile/mailbox negative invariants and generation integrity suites passed;
- Cloudflare adapter suite: 19 passed; Worker helper suite: 14 passed; Worker WASM check passed;
- Profile Bridge library/bin/integration suites: 35 passed in aggregate, followed by successful execution of `profile-bridge-synthetic` ending exactly `DIRTY_LOCAL`;
- Node `24.19.0` / npm `11.17.0` clean frontend install, strict TypeScript, Vitest and Vite build passed in the same lane;
- metadata-only evidence scan passed; no user data or real external provider/runtime was involved;
- final accepted source head: `31f358c92c2e09c752155af208b7d2aaf73d472a`; all 12 permanent workflows passed on that exact unchanged head; Cross-Component Acceptance Gate `31208960718` succeeded; squash merge `eb02f3e81022193fb459b7c46d14afcb19c8900f`.

### Pre-2J D3A Empty-D1 Bootstrap Evidence

- tracking: blocker issue #253 / PR #254 under D3 #251 and umbrella #203;
- remote proof source: `493d399b9531776aa8208242a5d1c05681764231`;
- Wrangler `4.94.0` remote SQL-file import completed against one dedicated fresh
  proof D1 whose only initial schema object was Cloudflare's reserved `_cf_KV`;
- bootstrap identity: `261937` bytes, SHA-256
  `de1acf24f30084ba95c43bdb6f2463b068b54e27e9ec0834753dc6383efef069`;
- exact ordered `d1_migrations` ledger `0001`–`0026`, required 0012 objects and
  `outbox_events` version columns: passed;
- identical replay: rejected with `SQLITE_ERROR`; schema object count, sanitized
  schema SHA-256 and exact ledger remained unchanged; no guard residue remained;
- canonical staging/production, credentials, secret material and user data
  involved: no;
- permanent Quality Gate regenerates the same bootstrap, checks the sanitized
  external record against current migrations, proves negative fixtures and
  verifies generated output remains untracked;
- detailed report:
  [`evidence/2026-08-14-pre2j-d3a-empty-d1-bootstrap.md`](evidence/2026-08-14-pre2j-d3a-empty-d1-bootstrap.md);
- incremental non-empty D1 migration remains D4-owned; D3 promotion, #203,
  Phase 2J and `production_ready=false` remain unchanged.

## 2. Required Permanent CI Evidence

Every applicable PR must provide:

- formatting, lint and unit/integration tests;
- exact toolchain and locked dependency build;
- architecture/forbidden dependency checks;
- contract/schema compatibility;
- migration replay where storage changes;
- authorization and negative isolation tests where public API changes;
- no-secret/no-PII tracked artifact checks;
- deterministic or bounded replay tests for async commands;
- changed status/evidence only after the corresponding tests exist.

## 3. Required External Evidence

| Gate | Required artifact |
|---|---|
| Legacy credential rotation | provider-side revocation/rotation confirmation and incident reference without secret value |
| Cloudflare staging | deployment ID, resource inventory, binding smoke, rollback result and cost boundary |
| Windows runtime | signed/test-signed artifact digest, host/runtime manifest and lifecycle report |
| Multi-device | two independent host manifests and transfer/revoke results |
| Key management | algorithm review, test vectors, rotation and clean-environment escrow restore report |
| Stable release | trusted signature verification, SBOM and update rollback report |
| Production recovery | D1/R2/key clean-environment game day report |
| Privacy | accepted retention matrix and export/delete/reconciliation report |

## 4. Evidence Naming

New evidence should live under `docs/evidence/` and use:

```text
YYYY-MM-DD-repository-step-N-short-name.md
```

Each report records:

- source commit and artifact digests;
- environment/runtime versions;
- exact scope and test inputs;
- results and failures;
- limitations and unproven properties;
- links to CI runs or external evidence references;
- whether user data was involved.

Secret values, raw cookies, mailbox content and uncontrolled screenshots are
prohibited in evidence documents.

## 5. Promotion Rule

`docs/status.json` may move a property from `not_proven` or `blocked` only when a
merged permanent test or reviewed external evidence entry is present. Removing a
test, invalidating an environment or superseding an ADR may downgrade status.
